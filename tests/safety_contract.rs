use familyos::app::{AppService, CreateExpense, CreateGoal, SettingsPatch};
use familyos::models::{ProviderAccount, ProviderBalance, ProviderPayload};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn sqlite_foreign_keys_are_enabled_on_connections() {
    let dir = tempdir().unwrap();
    let service = AppService::connect_path(&dir.path().join("familyos.sqlite")).unwrap();
    let state = service.app_state().unwrap();
    assert_eq!(state.settings.base_currency, "USDT");

    assert!(service.foreign_keys_enabled().unwrap());
}

#[test]
fn not_connected_sync_status_does_not_erase_last_provider_snapshot() {
    let dir = tempdir().unwrap();
    let service = AppService::connect_path(&dir.path().join("familyos.sqlite")).unwrap();

    service
        .apply_sync_payload("okx", live_okx_payload())
        .expect("live payload should store balances");
    assert_eq!(service.app_state().unwrap().balances.len(), 1);

    service
        .apply_sync_payload(
            "okx",
            ProviderPayload {
                mode: "not_connected".to_string(),
                as_of: "2026-06-08T00:00:00.000Z".to_string(),
                accounts: vec![],
                positions: vec![],
                transactions: vec![],
                meta: json!({"status": "not connected"}),
            },
        )
        .expect("not connected should only update sync status");

    let state = service.app_state().unwrap();
    assert_eq!(state.balances.len(), 1);
    assert!(
        state
            .syncs
            .iter()
            .any(|sync| sync.provider == "okx" && sync.status == "not_connected")
    );
}

#[test]
fn rejects_invalid_direct_financial_writes() {
    let dir = tempdir().unwrap();
    let service = AppService::connect_path(&dir.path().join("familyos.sqlite")).unwrap();

    assert!(
        service
            .create_expense(CreateExpense {
                date: "not-a-date".to_string(),
                amount: 10.0,
                currency: "BYN".to_string(),
                category: "Кофе".to_string(),
                merchant_note: "bad date".to_string(),
                source: "manual".to_string(),
            })
            .is_err()
    );
    assert!(
        service
            .create_expense(CreateExpense {
                date: "2026-06-08".to_string(),
                amount: -10.0,
                currency: "BYN".to_string(),
                category: "Кофе".to_string(),
                merchant_note: "negative".to_string(),
                source: "manual".to_string(),
            })
            .is_err()
    );
    assert!(
        service
            .create_goal(CreateGoal {
                name: "Bad".to_string(),
                target_amount: f64::NAN,
                currency: "USDT".to_string(),
                deadline: Some("2026-09-30".to_string()),
                priority: 1,
                current_amount: 0.0,
                monthly_required_amount: None,
                bucket: "planned_purchase".to_string(),
            })
            .is_err()
    );
    assert!(
        service
            .update_settings(SettingsPatch {
                monthly_income: Some(5000.0),
                monthly_taxable_base: Some(3000.0),
                tax_rate: Some(1.5),
                base_currency: None,
                life_currency: None,
            })
            .is_err()
    );
}

fn live_okx_payload() -> ProviderPayload {
    ProviderPayload {
        mode: "live".to_string(),
        as_of: "2026-06-08T00:00:00.000Z".to_string(),
        accounts: vec![ProviderAccount {
            name: "OKX Funding".to_string(),
            account_type: "crypto_funding".to_string(),
            currency: "USDT".to_string(),
            bucket: "crypto_retained".to_string(),
            external_id: "okx-funding-live".to_string(),
            balances: vec![ProviderBalance {
                currency: "USDT".to_string(),
                amount: 1000.0,
                usd_equivalent: 1000.0,
                bucket: "crypto_retained".to_string(),
            }],
        }],
        positions: vec![],
        transactions: vec![],
        meta: json!({"source": "live"}),
    }
}
