use familyos::app::{AppService, CreateExpense, CreateGoal, ImportCsv};
use familyos::csv_import::CsvImportSource;
use tempfile::tempdir;

#[test]
fn creates_local_sqlite_state_and_static_integrations() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("familyos.sqlite");
    let service = AppService::connect_path(&db_path).unwrap();
    let state = service.app_state().unwrap();

    assert_eq!(state.settings.base_currency, "USDT");
    assert_eq!(state.settings.life_currency, "BYN");
    assert!(state.syncs.iter().any(|sync| sync.provider == "okx"));
    assert!(state.syncs.iter().any(|sync| sync.provider == "freedom"));
    assert!(
        state
            .syncs
            .iter()
            .any(|sync| sync.provider == "priobank" && sync.status == "planned")
    );
}

#[test]
fn imports_csv_by_detected_months_and_replaces_only_that_source_month() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("familyos.sqlite");
    let service = AppService::connect_path(&db_path).unwrap();

    service
        .import_csv(ImportCsv {
            source: CsvImportSource::Manual,
            csv: "date,amount,currency,category,note\n2026-04-30,-12.50,BYN,Такси,ride\n2026-05-01,-21,BYN,Продукты,shop".to_string(),
        })
        .unwrap();
    service
        .import_csv(ImportCsv {
            source: CsvImportSource::Manual,
            csv: "date,amount,currency,category,note\n2026-05-02,-9,BYN,Кофе,cafe".to_string(),
        })
        .unwrap();
    let state = service.app_state().unwrap();

    assert!(
        state
            .expenses
            .iter()
            .any(|row| row.date.starts_with("2026-04-30"))
    );
    assert!(
        state
            .expenses
            .iter()
            .any(|row| row.date.starts_with("2026-05-02"))
    );
    assert!(
        !state
            .expenses
            .iter()
            .any(|row| row.date.starts_with("2026-05-01"))
    );
    assert_eq!(state.reports.len(), 2);
}

#[test]
fn creates_goal_and_expense_then_reflects_annual_dashboard_summary() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("familyos.sqlite");
    let service = AppService::connect_path(&db_path).unwrap();

    service
        .create_goal(CreateGoal {
            name: "Trip".to_string(),
            target_amount: 2000.0,
            currency: "USDT".to_string(),
            deadline: Some("2026-09-30".to_string()),
            priority: 1,
            current_amount: 500.0,
            monthly_required_amount: None,
            bucket: "planned_purchase".to_string(),
        })
        .unwrap();
    service
        .create_expense(CreateExpense {
            date: "2026-06-08".to_string(),
            amount: 30.0,
            currency: "BYN".to_string(),
            category: "Кофе".to_string(),
            merchant_note: "cafe".to_string(),
            source: "manual".to_string(),
        })
        .unwrap();
    let state = service.app_state().unwrap();

    assert_eq!(state.goals.len(), 1);
    assert_eq!(state.expenses.len(), 1);
    assert_eq!(
        state.summary.planned_expenses.total_by_currency.get("USDT"),
        Some(&2000.0)
    );
}
