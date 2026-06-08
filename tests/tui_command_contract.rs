use familyos::app::AppService;
use familyos::tui::execute_tui_command;
use tempfile::tempdir;

#[test]
fn tui_command_mode_can_create_expenses_and_goals() {
    let dir = tempdir().unwrap();
    let service = AppService::connect_path(&dir.path().join("familyos.sqlite")).unwrap();

    execute_tui_command(&service, "expense add 2026-06-08 30 BYN Кофе cafe").unwrap();
    execute_tui_command(&service, "goal add Trip 2000 USDT 2026-09-30").unwrap();

    let state = service.app_state().unwrap();
    assert_eq!(state.expenses.len(), 1);
    assert_eq!(state.expenses[0].category, "Кофе");
    assert_eq!(state.goals.len(), 1);
    assert_eq!(state.goals[0].name, "Trip");
}

#[test]
fn tui_command_mode_can_generate_reports_and_reject_unknown_commands() {
    let dir = tempdir().unwrap();
    let service = AppService::connect_path(&dir.path().join("familyos.sqlite")).unwrap();

    execute_tui_command(&service, "daily").unwrap();
    execute_tui_command(&service, "snapshot").unwrap();
    execute_tui_command(&service, "close").unwrap();
    assert!(execute_tui_command(&service, "buy btc").is_err());

    let state = service.app_state().unwrap();
    assert!(
        state
            .reports
            .iter()
            .any(|report| report.report_type == "daily_brief")
    );
    assert!(
        state
            .reports
            .iter()
            .any(|report| report.report_type == "monthly_close")
    );
    assert!(!state.snapshots.is_empty());
}
