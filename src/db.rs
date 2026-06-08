use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::csv_import::parse_csv_transactions;
use crate::finance::{
    calculate_annual_forecast, calculate_income_plan, calculate_monthly_required_amount,
    calculate_monthly_summary, calculate_planned_expenses, calculate_tax_reserve, progress_ratio,
    round_money,
};
use crate::models::*;

pub fn database_path_from_url(database_url: &str) -> String {
    database_url
        .strip_prefix("file:")
        .unwrap_or(database_url)
        .to_string()
}

pub fn connect_database_url(database_url: &str) -> Result<Connection> {
    let path = database_path_from_url(database_url);
    if let Some(parent) = Path::new(&path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    enable_foreign_keys(&conn)?;
    ensure_database(&conn)?;
    ensure_local_data(&conn)?;
    Ok(conn)
}

pub fn connect_path(path: &Path) -> Result<Connection> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    enable_foreign_keys(&conn)?;
    ensure_database(&conn)?;
    ensure_local_data(&conn)?;
    Ok(conn)
}

pub fn foreign_keys_enabled(conn: &Connection) -> Result<bool> {
    let enabled: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    Ok(enabled == 1)
}

fn enable_foreign_keys(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

pub fn ensure_database(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    ensure_column(conn, "balances", "origin", "TEXT NOT NULL DEFAULT 'manual'")?;
    ensure_column(conn, "balances", "meta_json", "TEXT NOT NULL DEFAULT '{}'")?;
    ensure_column(
        conn,
        "transactions",
        "source_meta_json",
        "TEXT NOT NULL DEFAULT '{}'",
    )?;
    Ok(())
}

fn ensure_column(conn: &Connection, table: &str, column: &str, definition: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let exists = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .iter()
        .any(|name| name == column);
    if !exists {
        conn.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition}"
        ))?;
    }
    Ok(())
}

pub fn ensure_local_data(conn: &Connection) -> Result<()> {
    remove_legacy_sample_data(conn)?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))?;
    if count > 0 {
        ensure_static_integration_rows(conn)?;
        return Ok(());
    }
    let timestamp = now_iso();
    conn.execute(
        "INSERT INTO settings (id, tax_mode, monthly_income, monthly_taxable_base, tax_rate, base_currency, life_currency, updated_at)
         VALUES (?1, 'converted_to_life', 0, 0, 0, 'USDT', 'BYN', ?2)",
        params![uuid(), timestamp],
    )?;
    insert_sync_row(
        conn,
        SyncSeed {
            provider: "okx",
            status: "not_connected",
            mode: "not_connected",
            last_synced_at: Some(&timestamp),
            last_error: None,
            meta: &json!({"docs": "https://www.okx.com/docs-v5/en/"}),
            updated_at: &timestamp,
        },
    )?;
    insert_sync_row(
        conn,
        SyncSeed {
            provider: "freedom",
            status: "not_connected",
            mode: "not_connected",
            last_synced_at: Some(&timestamp),
            last_error: None,
            meta: &json!({"docs": "https://freedom24.com/tradernet-api/auth-login"}),
            updated_at: &timestamp,
        },
    )?;
    insert_sync_row(
        conn,
        SyncSeed {
            provider: "priobank",
            status: "planned",
            mode: "planned",
            last_synced_at: None,
            last_error: None,
            meta: &json!({"note": "Priobank API integration planned after official API access is available"}),
            updated_at: &timestamp,
        },
    )?;
    Ok(())
}

fn ensure_static_integration_rows(conn: &Connection) -> Result<()> {
    let timestamp = now_iso();
    for (provider, status, mode, meta) in [
        ("okx", "not_connected", "not_connected", json!({})),
        ("freedom", "not_connected", "not_connected", json!({})),
        (
            "priobank",
            "planned",
            "planned",
            json!({"note": "Priobank API integration planned after official API access is available"}),
        ),
    ] {
        let exists: Option<String> = conn
            .query_row(
                "SELECT id FROM integration_syncs WHERE provider = ?1 LIMIT 1",
                params![provider],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_none() {
            insert_sync_row(
                conn,
                SyncSeed {
                    provider,
                    status,
                    mode,
                    last_synced_at: Some(&timestamp),
                    last_error: None,
                    meta: &meta,
                    updated_at: &timestamp,
                },
            )?;
        }
    }
    Ok(())
}

fn remove_legacy_sample_data(conn: &Connection) -> Result<()> {
    for source in ["mock_okx", "mock_freedom"] {
        conn.execute("DELETE FROM balances WHERE source = ?1", params![source])?;
        conn.execute(
            "DELETE FROM transactions WHERE source = ?1",
            params![source],
        )?;
    }
    for title in ["Demo daily brief", "Legacy demo data cleanup v2"] {
        conn.execute("DELETE FROM reports WHERE title = ?1", params![title])?;
    }
    Ok(())
}

struct SyncSeed<'a> {
    provider: &'a str,
    status: &'a str,
    mode: &'a str,
    last_synced_at: Option<&'a str>,
    last_error: Option<&'a str>,
    meta: &'a Value,
    updated_at: &'a str,
}

fn insert_sync_row(conn: &Connection, row: SyncSeed<'_>) -> Result<()> {
    conn.execute(
        "INSERT INTO integration_syncs (id, provider, status, mode, last_synced_at, last_error, meta_json, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            uuid(),
            row.provider,
            row.status,
            row.mode,
            row.last_synced_at,
            row.last_error,
            row.meta.to_string(),
            row.updated_at
        ],
    )?;
    Ok(())
}

pub fn app_state(conn: &Connection) -> Result<AppState> {
    ensure_local_data(conn)?;
    let settings = settings(conn)?;
    let goals = goals(conn)?;
    let expenses = transactions_by_type(conn, Some("expense"), None)?;
    let all_provider_transactions = transactions_non_expense(conn, None)?;
    let provider_transactions = all_provider_transactions
        .iter()
        .take(100)
        .cloned()
        .collect();
    let accounts = accounts(conn)?;
    let balances = balances(conn)?;
    let positions = positions(conn)?;
    let syncs = syncs(conn)?;
    let sync_attempts = sync_attempts(conn, 20)?;
    let snapshots = snapshots(conn)?;
    let reports = reports(conn)?;
    let summary = build_summary(
        &settings,
        &goals,
        &expenses,
        &balances,
        &all_provider_transactions,
    );
    Ok(AppState {
        settings,
        goals,
        expenses,
        provider_transactions,
        accounts,
        balances,
        positions,
        syncs,
        sync_attempts,
        snapshots,
        reports,
        summary,
    })
}

pub fn create_goal(conn: &Connection, input: &crate::app::CreateGoal) -> Result<AppState> {
    ensure_local_data(conn)?;
    let timestamp = now_iso();
    let monthly_required_amount = input.monthly_required_amount.unwrap_or_else(|| {
        calculate_monthly_required_amount(
            input.target_amount,
            input.current_amount,
            input.deadline.as_deref(),
            Utc::now(),
        )
    });
    conn.execute(
        "INSERT INTO goals (id, name, target_amount, currency, deadline, priority, current_amount, monthly_required_amount, bucket, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            uuid(),
            input.name,
            input.target_amount,
            input.currency,
            input.deadline,
            input.priority,
            input.current_amount,
            monthly_required_amount,
            input.bucket,
            timestamp,
            timestamp
        ],
    )?;
    app_state(conn)
}

pub fn update_goal(
    conn: &Connection,
    id: &str,
    input: &crate::app::CreateGoal,
) -> Result<AppState> {
    ensure_local_data(conn)?;
    conn.execute(
        "UPDATE goals SET name = ?2, target_amount = ?3, currency = ?4, deadline = ?5, priority = ?6,
         current_amount = ?7, monthly_required_amount = ?8, bucket = ?9, updated_at = ?10 WHERE id = ?1",
        params![
            id,
            input.name,
            input.target_amount,
            input.currency,
            input.deadline,
            input.priority,
            input.current_amount,
            input.monthly_required_amount.unwrap_or_else(|| calculate_monthly_required_amount(
                input.target_amount,
                input.current_amount,
                input.deadline.as_deref(),
                Utc::now()
            )),
            input.bucket,
            now_iso()
        ],
    )?;
    app_state(conn)
}

pub fn delete_goal(conn: &Connection, id: &str) -> Result<AppState> {
    ensure_local_data(conn)?;
    conn.execute("DELETE FROM goals WHERE id = ?1", params![id])?;
    app_state(conn)
}

pub fn create_expense(conn: &Connection, input: &crate::app::CreateExpense) -> Result<AppState> {
    ensure_local_data(conn)?;
    let timestamp = now_iso();
    conn.execute(
        "INSERT INTO transactions (id, date, amount, currency, category, merchant_note, source, source_meta_json, type, account_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'expense', NULL, ?9, ?10)",
        params![
            uuid(),
            normalize_cli_date(&input.date),
            input.amount,
            input.currency,
            input.category,
            input.merchant_note,
            input.source,
            json!({"enteredBy": "local-cli", "createdAt": timestamp}).to_string(),
            timestamp,
            timestamp
        ],
    )?;
    app_state(conn)
}

pub fn delete_expense(conn: &Connection, id: &str) -> Result<AppState> {
    ensure_local_data(conn)?;
    conn.execute(
        "DELETE FROM transactions WHERE id = ?1 AND type = 'expense'",
        params![id],
    )?;
    app_state(conn)
}

pub fn update_settings(conn: &Connection, input: &crate::app::SettingsPatch) -> Result<AppState> {
    ensure_local_data(conn)?;
    let current = settings(conn)?;
    conn.execute(
        "UPDATE settings SET monthly_income = ?1, monthly_taxable_base = ?2, tax_rate = ?3, tax_mode = 'converted_to_life',
         base_currency = ?4, life_currency = ?5, updated_at = ?6 WHERE id = ?7",
        params![
            input.monthly_income.unwrap_or(current.monthly_income),
            input.monthly_taxable_base.unwrap_or(current.monthly_taxable_base),
            input.tax_rate.unwrap_or(current.tax_rate),
            input.base_currency.clone().unwrap_or(current.base_currency),
            input.life_currency.clone().unwrap_or(current.life_currency),
            now_iso(),
            current.id
        ],
    )?;
    app_state(conn)
}

pub fn import_csv(conn: &Connection, input: &crate::app::ImportCsv) -> Result<AppState> {
    ensure_local_data(conn)?;
    let parsed = parse_csv_transactions(input.source, &input.csv)?;
    if parsed.detected_months.is_empty() {
        bail!("CSV does not contain importable dated rows");
    }
    let timestamp = now_iso();
    let source = input.source.as_db_value();
    let tx = conn.unchecked_transaction()?;
    for month in &parsed.detected_months {
        tx.execute(
            "DELETE FROM transactions WHERE source = ?1 AND date LIKE ?2",
            params![source, format!("{month}%")],
        )?;
    }
    for row in &parsed.rows {
        let meta = match &row.meta {
            Value::Object(map) => {
                let mut map = map.clone();
                map.insert("importedAt".to_string(), json!(timestamp));
                map.insert(
                    "importMonth".to_string(),
                    json!(row.date.chars().take(7).collect::<String>()),
                );
                Value::Object(map)
            }
            _ => row.meta.clone(),
        };
        tx.execute(
            "INSERT INTO transactions (id, date, amount, currency, category, merchant_note, source, source_meta_json, type, account_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11)",
            params![
                uuid(),
                row.date,
                row.amount,
                row.currency,
                row.category,
                row.merchant_note,
                source,
                meta.to_string(),
                row.transaction_type,
                timestamp,
                timestamp
            ],
        )?;
    }
    tx.execute(
        "INSERT INTO reports (id, type, title, content_json, created_at) VALUES (?1, 'csv_import', ?2, ?3, ?4)",
        params![
            uuid(),
            format!("CSV import {} {}", source, parsed.detected_months.join(", ")),
            json!({
                "source": source,
                "months": parsed.detected_months,
                "parsedRows": parsed.parsed_rows,
                "importedRows": parsed.imported_rows
            })
            .to_string(),
            timestamp
        ],
    )?;
    tx.commit()?;
    app_state(conn)
}

pub fn create_monthly_snapshot(conn: &Connection) -> Result<AppState> {
    let state = app_state(conn)?;
    let timestamp = now_iso();
    let month = current_month_key();
    let goals_progress = state
        .goals
        .iter()
        .map(|goal| {
            json!({
                "name": goal.name,
                "currentAmount": goal.current_amount,
                "targetAmount": goal.target_amount,
                "progress": progress_ratio(goal.current_amount, goal.target_amount)
            })
        })
        .collect::<Vec<_>>();
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM monthly_snapshots WHERE month = ?1 ORDER BY created_at DESC LIMIT 1",
            params![month],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        conn.execute(
            "UPDATE monthly_snapshots SET income = ?2, taxable_base = ?3, tax_reserve = ?4, expenses_byn = ?5,
             expenses_usdt = ?6, free_surplus = ?7, goals_progress_json = ?8, assets_snapshot_json = ?9, created_at = ?10 WHERE id = ?1",
            params![
                id,
                state.settings.monthly_income,
                state.settings.monthly_taxable_base,
                state.summary.monthly.tax_reserve,
                state.summary.spending.month_to_date.byn,
                state.summary.spending.month_to_date.usdt,
                state.summary.monthly.free_surplus,
                json!(goals_progress).to_string(),
                json!(state.summary.assets.buckets).to_string(),
                timestamp
            ],
        )?;
    } else {
        conn.execute(
            "INSERT INTO monthly_snapshots (id, month, income, taxable_base, tax_reserve, expenses_byn, expenses_usdt, free_surplus, goals_progress_json, assets_snapshot_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                uuid(),
                month,
                state.settings.monthly_income,
                state.settings.monthly_taxable_base,
                state.summary.monthly.tax_reserve,
                state.summary.spending.month_to_date.byn,
                state.summary.spending.month_to_date.usdt,
                state.summary.monthly.free_surplus,
                json!(goals_progress).to_string(),
                json!(state.summary.assets.buckets).to_string(),
                timestamp
            ],
        )?;
    }
    app_state(conn)
}

pub fn generate_daily_brief(conn: &Connection) -> Result<AppState> {
    let state = app_state(conn)?;
    let timestamp = now_iso();
    let content = json!({
        "date": today_key(),
        "assetChange": "Локальный snapshot активов обновлен из доступных read-only источников.",
        "spentToday": state.summary.spending.today,
        "spentMonth": state.summary.spending.month_to_date,
        "taxReserve": {
            "target": state.summary.tax.monthly_target,
            "progress": state.summary.tax.reserve_progress,
            "gap": state.summary.tax.reserve_gap
        },
        "goals": state.goals.iter().map(|goal| json!({
            "name": goal.name,
            "plannedExpense": goal.target_amount,
            "currency": goal.currency,
            "progress": progress_ratio(goal.current_amount, goal.target_amount)
        })).collect::<Vec<_>>(),
        "attentionItems": build_attention_items(&state)
    });
    conn.execute(
        "INSERT INTO reports (id, type, title, content_json, created_at) VALUES (?1, 'daily_brief', ?2, ?3, ?4)",
        params![uuid(), format!("Daily brief {}", today_key()), content.to_string(), timestamp],
    )?;
    app_state(conn)
}

pub fn generate_monthly_report(conn: &Connection) -> Result<AppState> {
    let mut state = app_state(conn)?;
    if state
        .snapshots
        .first()
        .is_none_or(|snapshot| snapshot.month != current_month_key())
    {
        state = create_monthly_snapshot(conn)?;
    }
    let Some(latest) = state.snapshots.first() else {
        bail!("Monthly snapshot unavailable")
    };
    let timestamp = now_iso();
    let content = json!({
        "month": latest.month,
        "income": latest.income,
        "taxableBase": latest.taxable_base,
        "taxReserve": latest.tax_reserve,
        "expenses": {"BYN": latest.expenses_byn, "USDT": latest.expenses_usdt},
        "freeSurplus": latest.free_surplus,
        "goalsProgress": latest.goals_progress,
        "assetsSnapshot": latest.assets_snapshot
    });
    conn.execute(
        "INSERT INTO reports (id, type, title, content_json, created_at) VALUES (?1, 'monthly_close', ?2, ?3, ?4)",
        params![uuid(), format!("Monthly close {}", latest.month), content.to_string(), timestamp],
    )?;
    app_state(conn)
}

pub fn replace_provider_payload(
    conn: &Connection,
    provider: &str,
    payload: &ProviderPayload,
) -> Result<()> {
    ensure_local_data(conn)?;
    let tx = conn.unchecked_transaction()?;
    let mut stmt = conn.prepare("SELECT id, external_id FROM accounts WHERE provider = ?1")?;
    let provider_accounts = stmt
        .query_map(params![provider], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (account_id, _) in &provider_accounts {
        tx.execute(
            "DELETE FROM balances WHERE account_id = ?1",
            params![account_id],
        )?;
    }
    for account in &payload.accounts {
        let existing = provider_accounts
            .iter()
            .find(|(_, external_id)| external_id.as_deref() == Some(account.external_id.as_str()))
            .map(|(id, _)| id.clone());
        let account_id = existing.unwrap_or_else(uuid);
        if provider_accounts.iter().any(|(id, _)| id == &account_id) {
            tx.execute(
                "UPDATE accounts SET status = ?2, updated_at = ?3 WHERE id = ?1",
                params![account_id, payload.mode, payload.as_of],
            )?;
        } else {
            tx.execute(
                "INSERT INTO accounts (id, provider, name, type, currency, status, bucket, external_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    account_id,
                    provider,
                    account.name,
                    account.account_type,
                    account.currency,
                    payload.mode,
                    account.bucket,
                    account.external_id,
                    payload.as_of,
                    payload.as_of
                ],
            )?;
        }
        for balance in &account.balances {
            tx.execute(
                "INSERT INTO balances (id, account_id, currency, amount, usd_equivalent, bucket, source, origin, meta_json, as_of)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'integration', ?8, ?9)",
                params![
                    uuid(),
                    account_id,
                    balance.currency,
                    balance.amount,
                    balance.usd_equivalent,
                    balance.bucket,
                    provider,
                    json!({"provider": provider, "mode": payload.mode, "syncedAt": payload.as_of}).to_string(),
                    payload.as_of
                ],
            )?;
        }
    }
    tx.execute(
        "DELETE FROM asset_positions WHERE provider = ?1",
        params![provider],
    )?;
    for position in &payload.positions {
        tx.execute(
            "INSERT INTO asset_positions (id, provider, symbol, name, quantity, currency, price, value, bucket, as_of)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                uuid(),
                provider,
                position.symbol,
                position.name,
                position.quantity,
                position.currency,
                position.price,
                position.value,
                position.bucket,
                payload.as_of
            ],
        )?;
    }
    tx.execute(
        "DELETE FROM transactions WHERE source = ?1 OR source = ?2",
        params![provider, format!("mock_{provider}")],
    )?;
    for transaction in &payload.transactions {
        let meta = json!({
            "provider": provider,
            "externalId": transaction.external_id,
            "syncedAt": payload.as_of,
            "mode": payload.mode,
            "raw": transaction.meta
        });
        tx.execute(
            "INSERT INTO transactions (id, date, amount, currency, category, merchant_note, source, source_meta_json, type, account_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11)",
            params![
                uuid(),
                transaction.date,
                transaction.amount,
                transaction.currency,
                transaction.category,
                transaction.merchant_note,
                transaction.source,
                meta.to_string(),
                transaction.transaction_type,
                payload.as_of,
                payload.as_of
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn upsert_sync_status(
    conn: &Connection,
    provider: &str,
    status: &str,
    mode: &str,
    last_error: Option<&str>,
    meta: &Value,
) -> Result<()> {
    ensure_database(conn)?;
    let timestamp = now_iso();
    let existing: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT id, last_synced_at FROM integration_syncs WHERE provider = ?1 LIMIT 1",
            params![provider],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((id, last_synced_at)) = existing {
        let new_last_synced = if status == "ok" {
            Some(timestamp.as_str())
        } else {
            last_synced_at.as_deref()
        };
        conn.execute(
            "UPDATE integration_syncs SET status = ?2, mode = ?3, last_synced_at = ?4, last_error = ?5,
             meta_json = ?6, updated_at = ?7 WHERE id = ?1",
            params![id, status, mode, new_last_synced, last_error, meta.to_string(), timestamp],
        )?;
    } else {
        insert_sync_row(
            conn,
            SyncSeed {
                provider,
                status,
                mode,
                last_synced_at: if status == "ok" {
                    Some(timestamp.as_str())
                } else {
                    None
                },
                last_error,
                meta,
                updated_at: &timestamp,
            },
        )?;
    }
    Ok(())
}

pub struct SyncAttemptInput<'a> {
    pub provider: &'a str,
    pub status: &'a str,
    pub mode: &'a str,
    pub started_at: &'a str,
    pub finished_at: &'a str,
    pub duration_ms: i64,
    pub error: Option<&'a str>,
    pub meta: &'a Value,
}

pub fn record_sync_attempt(conn: &Connection, input: SyncAttemptInput<'_>) -> Result<()> {
    ensure_database(conn)?;
    conn.execute(
        "INSERT INTO integration_sync_attempts (id, provider, status, mode, started_at, finished_at, duration_ms, error, meta_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            uuid(),
            input.provider,
            input.status,
            input.mode,
            input.started_at,
            input.finished_at,
            input.duration_ms,
            input.error,
            input.meta.to_string()
        ],
    )?;
    Ok(())
}

fn build_summary(
    settings_row: &SettingsRow,
    goal_rows: &[GoalRow],
    transaction_rows: &[TransactionRow],
    balance_rows: &[BalanceRow],
    provider_transaction_rows: &[TransactionRow],
) -> AppSummary {
    let settings = FinanceSettings {
        monthly_income: settings_row.monthly_income,
        monthly_taxable_base: settings_row.monthly_taxable_base,
        tax_rate: settings_row.tax_rate,
    };
    let goal_likes = goal_rows
        .iter()
        .map(|goal| GoalLike {
            name: goal.name.clone(),
            target_amount: goal.target_amount,
            current_amount: goal.current_amount,
            monthly_required_amount: goal.monthly_required_amount,
            bucket: goal.bucket.clone(),
        })
        .collect::<Vec<_>>();
    let monthly = calculate_monthly_summary(&settings, &goal_likes);
    let income_rows = provider_transaction_rows
        .iter()
        .map(|row| IncomeTransactionLike {
            date: row.date.clone(),
            amount: row.amount,
            currency: row.currency.clone(),
            source: row.source.clone(),
            transaction_type: row.transaction_type.clone(),
        })
        .collect::<Vec<_>>();
    let income_plan = calculate_income_plan(&income_rows, Utc::now());
    let annual_life_burn_usdt = round_money(settings_row.monthly_taxable_base * 12.0);
    let planned_goals = goal_rows
        .iter()
        .map(|goal| PlannedExpenseGoalLike {
            id: goal.id.clone(),
            name: goal.name.clone(),
            target_amount: goal.target_amount,
            current_amount: goal.current_amount,
            monthly_required_amount: goal.monthly_required_amount,
            currency: goal.currency.clone(),
            deadline: goal.deadline.clone(),
            bucket: goal.bucket.clone(),
        })
        .collect::<Vec<_>>();
    let planned_expenses = calculate_planned_expenses(
        &planned_goals,
        &income_plan.projected_annual,
        annual_life_burn_usdt,
    );
    let mut annual = calculate_annual_forecast(&settings, &goal_likes);
    annual.gross = income_plan
        .projected_annual
        .get("USDT")
        .copied()
        .unwrap_or(0.0);
    annual.retained_crypto_investment = annual.gross;
    annual.free_surplus = planned_expenses.projected_after_planned_usdt;
    let month = current_month_key();
    let today = today_key();
    let year = Utc::now().year().to_string();
    let months_elapsed = Utc::now().month() as f64;
    let month_transactions = transaction_rows
        .iter()
        .filter(|row| row.date.starts_with(&month))
        .cloned()
        .collect::<Vec<_>>();
    let today_transactions = transaction_rows
        .iter()
        .filter(|row| row.date.starts_with(&today))
        .cloned()
        .collect::<Vec<_>>();
    let ytd_transactions = transaction_rows
        .iter()
        .filter(|row| row.date.starts_with(&year))
        .cloned()
        .collect::<Vec<_>>();
    let by_category = aggregate_by_category(&month_transactions);
    let mut top_leaks = by_category.clone();
    top_leaks.sort_by(|a, b| b.total_comparable.total_cmp(&a.total_comparable));
    top_leaks.truncate(5);
    let tax_reserve_balance = balance_rows
        .iter()
        .filter(|balance| balance.bucket == "tax")
        .map(|balance| {
            if balance.usd_equivalent != 0.0 {
                balance.usd_equivalent
            } else {
                balance.amount
            }
        })
        .sum::<f64>();
    let annual_tax_target =
        calculate_tax_reserve(settings_row.monthly_taxable_base, settings_row.tax_rate) * 12.0;
    let assets = aggregate_assets(balance_rows);
    AppSummary {
        monthly,
        annual,
        inflow: build_inflow_summary(provider_transaction_rows),
        income_plan: income_plan.clone(),
        planned_expenses,
        ytd_gross: income_plan.actual_ytd.get("USDT").copied().unwrap_or(0.0),
        ytd_taxable: round_money(settings_row.monthly_taxable_base * months_elapsed),
        ytd_tax_reserve: round_money(
            calculate_tax_reserve(settings_row.monthly_taxable_base, settings_row.tax_rate)
                * months_elapsed,
        ),
        ytd_expenses: split_currency_totals(&ytd_transactions),
        tax: TaxSummary {
            mode: settings_row.tax_mode.clone(),
            monthly_target: calculate_tax_reserve(
                settings_row.monthly_taxable_base,
                settings_row.tax_rate,
            ),
            annual_target: round_money(annual_tax_target),
            reserve_progress: round_money(tax_reserve_balance),
            reserve_gap: round_money(
                (calculate_tax_reserve(settings_row.monthly_taxable_base, settings_row.tax_rate)
                    - tax_reserve_balance)
                    .max(0.0),
            ),
            annual_reserve_gap: round_money((annual_tax_target - tax_reserve_balance).max(0.0)),
        },
        goals: GoalsSummary {
            emergency: goal_progress(goal_rows, "emergency"),
            planned_purchases: group_goal_progress(goal_rows, "planned_purchase"),
        },
        spending: SpendingSummary {
            today: split_currency_totals(&today_transactions),
            month_to_date: split_currency_totals(&month_transactions),
            by_category,
            top_leaks,
        },
        assets,
        latest_daily_report_hint: DailyReportHint {
            generated_at: now_iso(),
            attention_items: build_static_attention_items(goal_rows, balance_rows, settings_row),
        },
        months: build_month_rows(provider_transaction_rows, goal_rows, &income_plan),
    }
}

fn build_inflow_summary(rows: &[TransactionRow]) -> InflowSummary {
    let now = Utc::now();
    let year_start = format!("{}-01-01", now.year());
    let current_month = current_month_key();
    let current_year_rows = rows
        .iter()
        .filter(|row| {
            (row.source == "okx" || row.source == "csv_okx")
                && row.transaction_type == "deposit"
                && row.date >= year_start
        })
        .cloned()
        .collect::<Vec<_>>();
    let current_month_rows = current_year_rows
        .iter()
        .filter(|row| row.date.starts_with(&current_month))
        .cloned()
        .collect::<Vec<_>>();
    InflowSummary {
        current_year_start: year_start,
        current_month_key: current_month,
        current_month: split_inflow_currency_totals(&current_month_rows),
        current_year: split_inflow_currency_totals(&current_year_rows),
        deposit_count: current_year_rows.len(),
    }
}

fn build_attention_items(state: &AppState) -> Vec<String> {
    let mut items = Vec::new();
    if state.summary.tax.reserve_gap > 0.0 {
        items.push(format!(
            "Налоговый резерв ниже месячной цели на {} USDT.",
            state.summary.tax.reserve_gap
        ));
    }
    for goal in &state.goals {
        if goal.target_amount > 0.0 && progress_ratio(goal.current_amount, goal.target_amount) < 0.7
        {
            items.push(format!(
                "Цель «{}» съест {} {} из годовой картины.",
                goal.name, goal.target_amount, goal.currency
            ));
        }
    }
    for sync in &state.syncs {
        if sync.provider != "priobank" && sync.status != "ok" {
            items.push(format!(
                "{} не подключен или требует ручной CSV импорт.",
                sync.provider.to_uppercase()
            ));
        }
    }
    if items.is_empty() {
        vec!["Критичных отклонений по локальным данным нет.".to_string()]
    } else {
        items
    }
}

fn split_currency_totals(rows: &[TransactionRow]) -> CurrencySplit {
    rows.iter()
        .fold(CurrencySplit::default(), |mut totals, row| {
            if row.currency == "BYN" {
                totals.byn = round_money(totals.byn + row.amount);
            } else {
                totals.usdt = round_money(totals.usdt + row.amount);
            }
            totals
        })
}

fn split_inflow_currency_totals(rows: &[TransactionRow]) -> CurrencySplit {
    split_currency_totals(rows)
}

fn aggregate_by_category(rows: &[TransactionRow]) -> Vec<CategorySpend> {
    let mut totals = std::collections::BTreeMap::<String, CategorySpend>::new();
    for row in rows {
        let entry = totals
            .entry(row.category.clone())
            .or_insert_with(|| CategorySpend {
                category: row.category.clone(),
                ..CategorySpend::default()
            });
        if row.currency == "BYN" {
            entry.byn = round_money(entry.byn + row.amount);
            entry.total_comparable += row.amount;
        } else {
            entry.usdt = round_money(entry.usdt + row.amount);
            entry.total_comparable += row.amount * 3.2;
        }
    }
    totals.into_values().collect()
}

fn goal_progress(goal_rows: &[GoalRow], bucket: &str) -> GoalProgress {
    goal_rows
        .iter()
        .find(|goal| goal.bucket == bucket)
        .map(|goal| GoalProgress {
            current: goal.current_amount,
            target: goal.target_amount,
            ratio: progress_ratio(goal.current_amount, goal.target_amount),
        })
        .unwrap_or_default()
}

fn group_goal_progress(goal_rows: &[GoalRow], bucket: &str) -> GoalProgress {
    let current = goal_rows
        .iter()
        .filter(|goal| goal.bucket == bucket)
        .map(|goal| goal.current_amount)
        .sum::<f64>();
    let target = goal_rows
        .iter()
        .filter(|goal| goal.bucket == bucket)
        .map(|goal| goal.target_amount)
        .sum::<f64>();
    GoalProgress {
        current,
        target,
        ratio: progress_ratio(current, target),
    }
}

fn aggregate_assets(balance_rows: &[BalanceRow]) -> AssetsSummary {
    let labels = [
        ("life", "Жизнь"),
        ("tax", "Налоговый резерв"),
        ("emergency", "Резервный фонд"),
        ("planned_purchase", "Крупные покупки"),
        ("investment", "Инвестиции"),
        ("crypto_retained", "Crypto retained"),
    ];
    let mut buckets = labels
        .iter()
        .map(|(bucket, label)| {
            let rows = balance_rows
                .iter()
                .filter(|row| row.bucket == *bucket)
                .collect::<Vec<_>>();
            let usdt = rows
                .iter()
                .filter(|row| row.currency != "BYN")
                .map(|row| {
                    if row.usd_equivalent != 0.0 {
                        row.usd_equivalent
                    } else {
                        row.amount
                    }
                })
                .sum::<f64>();
            let byn = rows
                .iter()
                .filter(|row| row.currency == "BYN")
                .map(|row| row.amount)
                .sum::<f64>();
            AssetBucket {
                bucket: (*bucket).to_string(),
                label: (*label).to_string(),
                usdt: round_money(usdt),
                byn: round_money(byn),
                ratio: 0.0,
            }
        })
        .collect::<Vec<_>>();
    let total_usdt = round_money(buckets.iter().map(|bucket| bucket.usdt).sum());
    for bucket in &mut buckets {
        bucket.ratio = if total_usdt > 0.0 {
            bucket.usdt / total_usdt
        } else {
            0.0
        };
    }
    let by_origin = ["manual", "integration"]
        .iter()
        .map(|origin| {
            let rows = balance_rows
                .iter()
                .filter(|row| row.origin == *origin)
                .collect::<Vec<_>>();
            AssetOrigin {
                origin: (*origin).to_string(),
                usdt: round_money(
                    rows.iter()
                        .filter(|row| row.currency != "BYN")
                        .map(|row| {
                            if row.usd_equivalent != 0.0 {
                                row.usd_equivalent
                            } else {
                                row.amount
                            }
                        })
                        .sum(),
                ),
                byn: round_money(
                    rows.iter()
                        .filter(|row| row.currency == "BYN")
                        .map(|row| row.amount)
                        .sum(),
                ),
            }
        })
        .collect();
    AssetsSummary {
        total_usdt,
        by_origin,
        buckets,
    }
}

fn build_static_attention_items(
    goal_rows: &[GoalRow],
    balance_rows: &[BalanceRow],
    settings_row: &SettingsRow,
) -> Vec<String> {
    let has_financial_config = settings_row.monthly_income > 0.0
        || settings_row.monthly_taxable_base > 0.0
        || settings_row.tax_rate > 0.0;
    if !has_financial_config && goal_rows.is_empty() && balance_rows.is_empty() {
        return vec!["Локальные данные еще не заполнены.".to_string()];
    }
    let tax_reserve_balance = balance_rows
        .iter()
        .filter(|balance| balance.bucket == "tax")
        .map(|balance| {
            if balance.usd_equivalent != 0.0 {
                balance.usd_equivalent
            } else {
                balance.amount
            }
        })
        .sum::<f64>();
    let tax_gap = (calculate_tax_reserve(settings_row.monthly_taxable_base, settings_row.tax_rate)
        - tax_reserve_balance)
        .max(0.0);
    let mut items = vec![if tax_gap > 0.0 {
        format!("Tax reserve gap: {} USDT", round_money(tax_gap))
    } else {
        "Tax reserve покрывает месяц".to_string()
    }];
    items.extend(goal_rows.iter().take(2).map(|goal| {
        format!(
            "{}: плановый расход {} {}.",
            goal.name, goal.target_amount, goal.currency
        )
    }));
    items
}

fn build_month_rows(
    provider_transaction_rows: &[TransactionRow],
    goal_rows: &[GoalRow],
    income_plan: &IncomePlan,
) -> Vec<MonthRow> {
    let month_names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    month_names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let planned_expenses = month_planned_expenses(goal_rows, index as u32);
            MonthRow {
                month: (*name).to_string(),
                income: month_income(provider_transaction_rows, index as u32, income_plan),
                income_kind: if index as u32 <= Utc::now().month0() {
                    "actual"
                } else {
                    "forecast"
                }
                .to_string(),
                planned_usdt: planned_expenses.get("USDT").copied().unwrap_or(0.0),
                planned_expenses,
                status: if index as u32 <= Utc::now().month0() {
                    "actual OKX"
                } else {
                    "run-rate forecast"
                }
                .to_string(),
            }
        })
        .collect()
}

fn month_income(rows: &[TransactionRow], month_index: u32, income_plan: &IncomePlan) -> f64 {
    if month_index > Utc::now().month0() {
        return income_plan
            .run_rate_monthly
            .get("USDT")
            .copied()
            .unwrap_or(0.0);
    }
    let month_key = format!("{}-{:02}", Utc::now().year(), month_index + 1);
    let rows = rows
        .iter()
        .filter(|row| {
            (row.source == "okx" || row.source == "csv_okx")
                && row.transaction_type == "deposit"
                && row.date.starts_with(&month_key)
        })
        .cloned()
        .collect::<Vec<_>>();
    split_inflow_currency_totals(&rows).usdt
}

fn month_planned_expenses(
    goal_rows: &[GoalRow],
    month_index: u32,
) -> std::collections::BTreeMap<String, f64> {
    let mut totals = std::collections::BTreeMap::new();
    let year_prefix = format!("{}-", Utc::now().year());
    for goal in goal_rows {
        let Some(deadline) = &goal.deadline else {
            continue;
        };
        if !deadline.starts_with(&year_prefix) {
            continue;
        }
        let deadline_month = deadline
            .get(5..7)
            .and_then(|value| value.parse::<u32>().ok());
        if deadline_month != Some(month_index + 1) {
            continue;
        }
        let currency = if goal.currency == "USD" {
            "USDT"
        } else {
            &goal.currency
        };
        let next = totals.get(currency).copied().unwrap_or(0.0) + goal.target_amount;
        totals.insert(currency.to_string(), round_money(next));
    }
    totals
}

fn settings(conn: &Connection) -> Result<SettingsRow> {
    conn.query_row("SELECT * FROM settings LIMIT 1", [], map_settings)
        .context("settings row missing")
}

fn goals(conn: &Connection) -> Result<Vec<GoalRow>> {
    conn.prepare("SELECT * FROM goals ORDER BY priority, deadline")?
        .query_map([], map_goal)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn transactions_by_type(
    conn: &Connection,
    transaction_type: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<TransactionRow>> {
    let sql = if let Some(limit) = limit {
        format!(
            "SELECT * FROM transactions WHERE type = ?1 ORDER BY date DESC, created_at DESC LIMIT {limit}"
        )
    } else {
        "SELECT * FROM transactions WHERE type = ?1 ORDER BY date DESC, created_at DESC".to_string()
    };
    conn.prepare(&sql)?
        .query_map(
            params![transaction_type.unwrap_or("expense")],
            map_transaction,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn transactions_non_expense(
    conn: &Connection,
    limit: Option<usize>,
) -> Result<Vec<TransactionRow>> {
    let sql = if let Some(limit) = limit {
        format!(
            "SELECT * FROM transactions WHERE type != 'expense' ORDER BY date DESC, created_at DESC LIMIT {limit}"
        )
    } else {
        "SELECT * FROM transactions WHERE type != 'expense' ORDER BY date DESC, created_at DESC"
            .to_string()
    };
    conn.prepare(&sql)?
        .query_map([], map_transaction)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn accounts(conn: &Connection) -> Result<Vec<AccountRow>> {
    conn.prepare("SELECT * FROM accounts ORDER BY provider, name")?
        .query_map([], map_account)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn balances(conn: &Connection) -> Result<Vec<BalanceRow>> {
    conn.prepare("SELECT * FROM balances ORDER BY bucket, currency")?
        .query_map([], map_balance)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn positions(conn: &Connection) -> Result<Vec<PositionRow>> {
    conn.prepare("SELECT * FROM asset_positions ORDER BY provider, symbol")?
        .query_map([], map_position)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn syncs(conn: &Connection) -> Result<Vec<SyncRow>> {
    conn.prepare("SELECT * FROM integration_syncs ORDER BY provider")?
        .query_map([], map_sync)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn sync_attempts(conn: &Connection, limit: usize) -> Result<Vec<SyncAttemptRow>> {
    conn.prepare(&format!(
        "SELECT * FROM integration_sync_attempts ORDER BY started_at DESC LIMIT {limit}"
    ))?
    .query_map([], map_sync_attempt)?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

fn snapshots(conn: &Connection) -> Result<Vec<SnapshotRow>> {
    conn.prepare("SELECT * FROM monthly_snapshots ORDER BY created_at DESC")?
        .query_map([], map_snapshot)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn reports(conn: &Connection) -> Result<Vec<ReportRow>> {
    conn.prepare("SELECT * FROM reports ORDER BY created_at DESC")?
        .query_map([], map_report)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn map_settings(row: &Row<'_>) -> rusqlite::Result<SettingsRow> {
    Ok(SettingsRow {
        id: row.get("id")?,
        tax_mode: row.get("tax_mode")?,
        monthly_income: row.get("monthly_income")?,
        monthly_taxable_base: row.get("monthly_taxable_base")?,
        tax_rate: row.get("tax_rate")?,
        base_currency: row.get("base_currency")?,
        life_currency: row.get("life_currency")?,
        updated_at: row.get("updated_at")?,
    })
}

fn map_goal(row: &Row<'_>) -> rusqlite::Result<GoalRow> {
    Ok(GoalRow {
        id: row.get("id")?,
        name: row.get("name")?,
        target_amount: row.get("target_amount")?,
        currency: row.get("currency")?,
        deadline: row.get("deadline")?,
        priority: row.get("priority")?,
        current_amount: row.get("current_amount")?,
        monthly_required_amount: row.get("monthly_required_amount")?,
        bucket: row.get("bucket")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn map_transaction(row: &Row<'_>) -> rusqlite::Result<TransactionRow> {
    let meta_json: String = row.get("source_meta_json")?;
    Ok(TransactionRow {
        id: row.get("id")?,
        date: row.get("date")?,
        amount: row.get("amount")?,
        currency: row.get("currency")?,
        category: row.get("category")?,
        merchant_note: row.get("merchant_note")?,
        source: row.get("source")?,
        meta: parse_json(&meta_json, json!({})),
        source_meta_json: meta_json,
        transaction_type: row.get("type")?,
        account_id: row.get("account_id")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn map_account(row: &Row<'_>) -> rusqlite::Result<AccountRow> {
    Ok(AccountRow {
        id: row.get("id")?,
        provider: row.get("provider")?,
        name: row.get("name")?,
        account_type: row.get("type")?,
        currency: row.get("currency")?,
        status: row.get("status")?,
        bucket: row.get("bucket")?,
        external_id: row.get("external_id")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn map_balance(row: &Row<'_>) -> rusqlite::Result<BalanceRow> {
    let meta_json: String = row.get("meta_json")?;
    Ok(BalanceRow {
        id: row.get("id")?,
        account_id: row.get("account_id")?,
        currency: row.get("currency")?,
        amount: row.get("amount")?,
        usd_equivalent: row.get("usd_equivalent")?,
        bucket: row.get("bucket")?,
        source: row.get("source")?,
        origin: row.get("origin")?,
        meta: parse_json(&meta_json, json!({})),
        meta_json,
        as_of: row.get("as_of")?,
    })
}

fn map_position(row: &Row<'_>) -> rusqlite::Result<PositionRow> {
    Ok(PositionRow {
        id: row.get("id")?,
        provider: row.get("provider")?,
        symbol: row.get("symbol")?,
        name: row.get("name")?,
        quantity: row.get("quantity")?,
        currency: row.get("currency")?,
        price: row.get("price")?,
        value: row.get("value")?,
        bucket: row.get("bucket")?,
        as_of: row.get("as_of")?,
    })
}

fn map_sync(row: &Row<'_>) -> rusqlite::Result<SyncRow> {
    let meta_json: String = row.get("meta_json")?;
    Ok(SyncRow {
        id: row.get("id")?,
        provider: row.get("provider")?,
        status: row.get("status")?,
        mode: row.get("mode")?,
        last_synced_at: row.get("last_synced_at")?,
        last_error: row.get("last_error")?,
        meta: parse_json(&meta_json, json!({})),
        meta_json,
        updated_at: row.get("updated_at")?,
    })
}

fn map_sync_attempt(row: &Row<'_>) -> rusqlite::Result<SyncAttemptRow> {
    let meta_json: String = row.get("meta_json")?;
    Ok(SyncAttemptRow {
        id: row.get("id")?,
        provider: row.get("provider")?,
        status: row.get("status")?,
        mode: row.get("mode")?,
        started_at: row.get("started_at")?,
        finished_at: row.get("finished_at")?,
        duration_ms: row.get("duration_ms")?,
        error: row.get("error")?,
        meta: parse_json(&meta_json, json!({})),
        meta_json,
    })
}

fn map_snapshot(row: &Row<'_>) -> rusqlite::Result<SnapshotRow> {
    let goals_progress_json: String = row.get("goals_progress_json")?;
    let assets_snapshot_json: String = row.get("assets_snapshot_json")?;
    Ok(SnapshotRow {
        id: row.get("id")?,
        month: row.get("month")?,
        income: row.get("income")?,
        taxable_base: row.get("taxable_base")?,
        tax_reserve: row.get("tax_reserve")?,
        expenses_byn: row.get("expenses_byn")?,
        expenses_usdt: row.get("expenses_usdt")?,
        free_surplus: row.get("free_surplus")?,
        goals_progress: parse_json(&goals_progress_json, json!([])),
        assets_snapshot: parse_json(&assets_snapshot_json, json!([])),
        goals_progress_json,
        assets_snapshot_json,
        created_at: row.get("created_at")?,
    })
}

fn map_report(row: &Row<'_>) -> rusqlite::Result<ReportRow> {
    let content_json: String = row.get("content_json")?;
    Ok(ReportRow {
        id: row.get("id")?,
        report_type: row.get("type")?,
        title: row.get("title")?,
        content: parse_json(&content_json, json!({})),
        content_json,
        created_at: row.get("created_at")?,
    })
}

fn parse_json(value: &str, fallback: Value) -> Value {
    serde_json::from_str(value).unwrap_or(fallback)
}

fn normalize_cli_date(value: &str) -> String {
    if value.len() == 10 && value.chars().nth(4) == Some('-') {
        format!("{value}T00:00:00.000Z")
    } else {
        value.to_string()
    }
}

pub fn validate_isoish_date(value: &str) -> Result<()> {
    if value.len() == 10 && value.chars().nth(4) == Some('-') {
        NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .with_context(|| format!("Invalid date: {value}"))?;
        return Ok(());
    }
    DateTime::parse_from_rfc3339(value).with_context(|| format!("Invalid date: {value}"))?;
    Ok(())
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn today_key() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

fn current_month_key() -> String {
    Utc::now().format("%Y-%m").to_string()
}

fn uuid() -> String {
    Uuid::new_v4().to_string()
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS settings (
  id TEXT PRIMARY KEY NOT NULL,
  tax_mode TEXT NOT NULL DEFAULT 'converted_to_life',
  monthly_income REAL NOT NULL DEFAULT 0,
  monthly_taxable_base REAL NOT NULL DEFAULT 0,
  tax_rate REAL NOT NULL DEFAULT 0,
  base_currency TEXT NOT NULL DEFAULT 'USDT',
  life_currency TEXT NOT NULL DEFAULT 'BYN',
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS accounts (
  id TEXT PRIMARY KEY NOT NULL,
  provider TEXT NOT NULL,
  name TEXT NOT NULL,
  type TEXT NOT NULL,
  currency TEXT NOT NULL DEFAULT 'USDT',
  status TEXT NOT NULL DEFAULT 'not_connected',
  bucket TEXT NOT NULL DEFAULT 'investment',
  external_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS accounts_provider_idx ON accounts (provider);
CREATE TABLE IF NOT EXISTS balances (
  id TEXT PRIMARY KEY NOT NULL,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  currency TEXT NOT NULL,
  amount REAL NOT NULL DEFAULT 0,
  usd_equivalent REAL NOT NULL DEFAULT 0,
  bucket TEXT NOT NULL DEFAULT 'investment',
  source TEXT NOT NULL DEFAULT 'manual',
  origin TEXT NOT NULL DEFAULT 'manual',
  meta_json TEXT NOT NULL DEFAULT '{}',
  as_of TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS balances_account_idx ON balances (account_id);
CREATE TABLE IF NOT EXISTS asset_positions (
  id TEXT PRIMARY KEY NOT NULL,
  provider TEXT NOT NULL,
  symbol TEXT NOT NULL,
  name TEXT NOT NULL,
  quantity REAL NOT NULL DEFAULT 0,
  currency TEXT NOT NULL DEFAULT 'USDT',
  price REAL NOT NULL DEFAULT 0,
  value REAL NOT NULL DEFAULT 0,
  bucket TEXT NOT NULL DEFAULT 'investment',
  as_of TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS asset_positions_provider_idx ON asset_positions (provider);
CREATE TABLE IF NOT EXISTS transactions (
  id TEXT PRIMARY KEY NOT NULL,
  date TEXT NOT NULL,
  amount REAL NOT NULL,
  currency TEXT NOT NULL,
  category TEXT NOT NULL,
  merchant_note TEXT NOT NULL DEFAULT '',
  source TEXT NOT NULL DEFAULT 'manual',
  source_meta_json TEXT NOT NULL DEFAULT '{}',
  type TEXT NOT NULL DEFAULT 'expense',
  account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS transactions_date_idx ON transactions (date);
CREATE INDEX IF NOT EXISTS transactions_type_idx ON transactions (type);
CREATE TABLE IF NOT EXISTS goals (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  target_amount REAL NOT NULL,
  currency TEXT NOT NULL DEFAULT 'USDT',
  deadline TEXT,
  priority INTEGER NOT NULL DEFAULT 2,
  current_amount REAL NOT NULL DEFAULT 0,
  monthly_required_amount REAL NOT NULL DEFAULT 0,
  bucket TEXT NOT NULL DEFAULT 'planned_purchase',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS goals_bucket_idx ON goals (bucket);
CREATE TABLE IF NOT EXISTS monthly_snapshots (
  id TEXT PRIMARY KEY NOT NULL,
  month TEXT NOT NULL,
  income REAL NOT NULL,
  taxable_base REAL NOT NULL,
  tax_reserve REAL NOT NULL,
  expenses_byn REAL NOT NULL DEFAULT 0,
  expenses_usdt REAL NOT NULL DEFAULT 0,
  free_surplus REAL NOT NULL DEFAULT 0,
  goals_progress_json TEXT NOT NULL DEFAULT '[]',
  assets_snapshot_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS monthly_snapshots_month_idx ON monthly_snapshots (month);
CREATE TABLE IF NOT EXISTS integration_syncs (
  id TEXT PRIMARY KEY NOT NULL,
  provider TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'not_connected',
  mode TEXT NOT NULL DEFAULT 'not_connected',
  last_synced_at TEXT,
  last_error TEXT,
  meta_json TEXT NOT NULL DEFAULT '{}',
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS integration_syncs_provider_idx ON integration_syncs (provider);
CREATE TABLE IF NOT EXISTS integration_sync_attempts (
  id TEXT PRIMARY KEY NOT NULL,
  provider TEXT NOT NULL,
  status TEXT NOT NULL,
  mode TEXT NOT NULL,
  started_at TEXT NOT NULL,
  finished_at TEXT,
  duration_ms INTEGER NOT NULL DEFAULT 0,
  error TEXT,
  meta_json TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS integration_sync_attempts_provider_idx ON integration_sync_attempts (provider);
CREATE TABLE IF NOT EXISTS reports (
  id TEXT PRIMARY KEY NOT NULL,
  type TEXT NOT NULL,
  title TEXT NOT NULL,
  content_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS reports_type_idx ON reports (type);
"#;
