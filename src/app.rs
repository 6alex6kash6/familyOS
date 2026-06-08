use std::path::Path;
use std::time::Instant;

use anyhow::{Result, bail};
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::csv_import::CsvImportSource;
use crate::db;
use crate::integrations::{freedom, okx};
use crate::models::{AppState, ProviderPayload};

pub struct AppService {
    conn: Connection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGoal {
    pub name: String,
    pub target_amount: f64,
    pub currency: String,
    pub deadline: Option<String>,
    pub priority: i64,
    pub current_amount: f64,
    pub monthly_required_amount: Option<f64>,
    pub bucket: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateExpense {
    pub date: String,
    pub amount: f64,
    pub currency: String,
    pub category: String,
    pub merchant_note: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsPatch {
    pub monthly_income: Option<f64>,
    pub monthly_taxable_base: Option<f64>,
    pub tax_rate: Option<f64>,
    pub base_currency: Option<String>,
    pub life_currency: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportCsv {
    pub source: CsvImportSource,
    pub csv: String,
}

impl AppService {
    pub fn connect_database_url(database_url: &str) -> Result<Self> {
        Ok(Self {
            conn: db::connect_database_url(database_url)?,
        })
    }

    pub fn connect_path(path: &Path) -> Result<Self> {
        Ok(Self {
            conn: db::connect_path(path)?,
        })
    }

    pub fn app_state(&self) -> Result<AppState> {
        db::app_state(&self.conn)
    }

    pub fn foreign_keys_enabled(&self) -> Result<bool> {
        db::foreign_keys_enabled(&self.conn)
    }

    pub fn create_goal(&self, input: CreateGoal) -> Result<AppState> {
        validate_goal(&input)?;
        db::create_goal(&self.conn, &input)
    }

    pub fn update_goal(&self, id: &str, input: CreateGoal) -> Result<AppState> {
        validate_goal(&input)?;
        db::update_goal(&self.conn, id, &input)
    }

    pub fn delete_goal(&self, id: &str) -> Result<AppState> {
        db::delete_goal(&self.conn, id)
    }

    pub fn create_expense(&self, input: CreateExpense) -> Result<AppState> {
        validate_expense(&input)?;
        db::create_expense(&self.conn, &input)
    }

    pub fn delete_expense(&self, id: &str) -> Result<AppState> {
        db::delete_expense(&self.conn, id)
    }

    pub fn update_settings(&self, input: SettingsPatch) -> Result<AppState> {
        validate_settings(&input)?;
        db::update_settings(&self.conn, &input)
    }

    pub fn import_csv(&self, input: ImportCsv) -> Result<AppState> {
        db::import_csv(&self.conn, &input)
    }

    pub fn create_monthly_snapshot(&self) -> Result<AppState> {
        db::create_monthly_snapshot(&self.conn)
    }

    pub fn generate_daily_brief(&self) -> Result<AppState> {
        db::generate_daily_brief(&self.conn)
    }

    pub fn generate_monthly_report(&self) -> Result<AppState> {
        db::generate_monthly_report(&self.conn)
    }

    pub fn sync_provider(&self, provider: &str) -> Result<ProviderPayload> {
        let started = Instant::now();
        let started_at = Utc::now().to_rfc3339();
        let result = match provider {
            "okx" => okx::sync_okx_from_env(),
            "freedom" => freedom::sync_freedom_from_env(),
            "priobank" => {
                bail!("Priobank API integration planned after official API access is available")
            }
            _ => bail!("Unsupported provider"),
        };
        match result {
            Ok(payload) => {
                self.apply_sync_payload_with_timing(
                    provider,
                    &payload,
                    &started_at,
                    started.elapsed().as_millis() as i64,
                )?;
                Ok(payload)
            }
            Err(error) => {
                let safe_error = error.to_string();
                let meta = serde_json::json!({"connected": false});
                db::upsert_sync_status(
                    &self.conn,
                    provider,
                    "error",
                    "error",
                    Some(&safe_error),
                    &meta,
                )?;
                let finished_at = Utc::now().to_rfc3339();
                db::record_sync_attempt(
                    &self.conn,
                    db::SyncAttemptInput {
                        provider,
                        status: "error",
                        mode: "error",
                        started_at: &started_at,
                        finished_at: &finished_at,
                        duration_ms: started.elapsed().as_millis() as i64,
                        error: Some(&safe_error),
                        meta: &meta,
                    },
                )?;
                Err(error)
            }
        }
    }

    pub fn apply_sync_payload(&self, provider: &str, payload: ProviderPayload) -> Result<AppState> {
        self.apply_sync_payload_with_timing(provider, &payload, &Utc::now().to_rfc3339(), 0)?;
        self.app_state()
    }

    fn apply_sync_payload_with_timing(
        &self,
        provider: &str,
        payload: &ProviderPayload,
        started_at: &str,
        duration_ms: i64,
    ) -> Result<()> {
        if payload.mode == "live" {
            db::replace_provider_payload(&self.conn, provider, payload)?;
        }
        let status = if payload.mode == "live" {
            "ok"
        } else {
            "not_connected"
        };
        db::upsert_sync_status(
            &self.conn,
            provider,
            status,
            &payload.mode,
            None,
            &payload.meta,
        )?;
        let finished_at = Utc::now().to_rfc3339();
        db::record_sync_attempt(
            &self.conn,
            db::SyncAttemptInput {
                provider,
                status,
                mode: &payload.mode,
                started_at,
                finished_at: &finished_at,
                duration_ms,
                error: None,
                meta: &payload.meta,
            },
        )?;
        Ok(())
    }
}

fn validate_expense(input: &CreateExpense) -> Result<()> {
    validate_positive_money(input.amount, "amount")?;
    validate_currency(&input.currency)?;
    validate_non_empty(&input.category, "category")?;
    validate_non_empty(&input.source, "source")?;
    db::validate_isoish_date(&input.date)?;
    Ok(())
}

fn validate_goal(input: &CreateGoal) -> Result<()> {
    validate_non_empty(&input.name, "name")?;
    validate_positive_money(input.target_amount, "target_amount")?;
    validate_non_negative_money(input.current_amount, "current_amount")?;
    if let Some(value) = input.monthly_required_amount {
        validate_non_negative_money(value, "monthly_required_amount")?;
    }
    validate_currency(&input.currency)?;
    validate_bucket(&input.bucket)?;
    if let Some(deadline) = &input.deadline {
        db::validate_isoish_date(deadline)?;
    }
    Ok(())
}

fn validate_settings(input: &SettingsPatch) -> Result<()> {
    if let Some(value) = input.monthly_income {
        validate_non_negative_money(value, "monthly_income")?;
    }
    if let Some(value) = input.monthly_taxable_base {
        validate_non_negative_money(value, "monthly_taxable_base")?;
    }
    if let Some(value) = input.tax_rate
        && (!value.is_finite() || !(0.0..=1.0).contains(&value))
    {
        bail!("tax_rate must be finite and between 0 and 1");
    }
    if let Some(currency) = &input.base_currency {
        validate_currency(currency)?;
    }
    if let Some(currency) = &input.life_currency {
        validate_currency(currency)?;
    }
    Ok(())
}

fn validate_positive_money(value: f64, field: &str) -> Result<()> {
    if !value.is_finite() || value <= 0.0 {
        bail!("{field} must be a finite positive number");
    }
    Ok(())
}

fn validate_non_negative_money(value: f64, field: &str) -> Result<()> {
    if !value.is_finite() || value < 0.0 {
        bail!("{field} must be a finite non-negative number");
    }
    Ok(())
}

fn validate_currency(value: &str) -> Result<()> {
    if !(2..=8).contains(&value.len()) || !value.chars().all(|ch| ch.is_ascii_uppercase()) {
        bail!("currency must be an uppercase 2-8 character code");
    }
    Ok(())
}

fn validate_bucket(value: &str) -> Result<()> {
    let allowed = [
        "life",
        "tax",
        "emergency",
        "planned_purchase",
        "investment",
        "crypto_retained",
    ];
    if !allowed.contains(&value) {
        bail!("Unsupported bucket");
    }
    Ok(())
}

fn validate_non_empty(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{field} must not be empty");
    }
    Ok(())
}
