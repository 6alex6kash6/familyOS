use chrono::{TimeZone, Utc};
use familyos::finance::{
    calculate_annual_forecast, calculate_income_plan, calculate_monthly_required_amount,
    calculate_monthly_summary, calculate_planned_expenses, calculate_tax_reserve,
};
use familyos::models::{FinanceSettings, GoalLike, IncomeTransactionLike, PlannedExpenseGoalLike};

#[test]
fn calculates_tax_reserve_from_configurable_tax_rate() {
    assert_eq!(calculate_tax_reserve(3000.0, 0.13), 390.0);
}

#[test]
fn calculates_dashboard_monthly_summary() {
    let summary = calculate_monthly_summary(
        &FinanceSettings {
            monthly_income: 5500.0,
            monthly_taxable_base: 3000.0,
            tax_rate: 0.13,
        },
        &[GoalLike {
            name: "Ноутбук".to_string(),
            target_amount: 2200.0,
            current_amount: 600.0,
            monthly_required_amount: 320.0,
            bucket: "planned_purchase".to_string(),
        }],
    );

    assert_eq!(summary.crypto_retained, 2500.0);
    assert_eq!(summary.tax_reserve, 390.0);
    assert_eq!(summary.planned_goal_allocations, 0.0);
    assert_eq!(summary.free_surplus, 2110.0);
}

#[test]
fn calculates_annual_forecast() {
    let annual = calculate_annual_forecast(
        &FinanceSettings {
            monthly_income: 5500.0,
            monthly_taxable_base: 3000.0,
            tax_rate: 0.13,
        },
        &[],
    );

    assert_eq!(annual.gross, 66_000.0);
    assert_eq!(annual.taxable, 36_000.0);
    assert_eq!(annual.tax_reserve, 4680.0);
}

#[test]
fn derives_monthly_required_amount_through_the_deadline_month() {
    let amount = calculate_monthly_required_amount(
        2200.0,
        600.0,
        Some("2026-09-30"),
        Utc.with_ymd_and_hms(2026, 5, 14, 0, 0, 0).unwrap(),
    );

    assert_eq!(amount, 320.0);
}

#[test]
fn annualizes_okx_deposits_from_year_to_date_income_only() {
    let income_plan = calculate_income_plan(
        &[
            IncomeTransactionLike {
                date: "2026-01-01T08:00:00.000Z".to_string(),
                amount: 1000.0,
                currency: "USDT".to_string(),
                source: "okx".to_string(),
                transaction_type: "deposit".to_string(),
            },
            IncomeTransactionLike {
                date: "2026-01-05T08:00:00.000Z".to_string(),
                amount: 500.0,
                currency: "USD".to_string(),
                source: "csv_okx".to_string(),
                transaction_type: "deposit".to_string(),
            },
            IncomeTransactionLike {
                date: "2026-01-06T08:00:00.000Z".to_string(),
                amount: 200.0,
                currency: "USDT".to_string(),
                source: "okx".to_string(),
                transaction_type: "withdrawal".to_string(),
            },
            IncomeTransactionLike {
                date: "2026-01-07T08:00:00.000Z".to_string(),
                amount: 300.0,
                currency: "USDT".to_string(),
                source: "okx".to_string(),
                transaction_type: "transfer".to_string(),
            },
            IncomeTransactionLike {
                date: "2025-12-31T08:00:00.000Z".to_string(),
                amount: 700.0,
                currency: "USDT".to_string(),
                source: "okx".to_string(),
                transaction_type: "deposit".to_string(),
            },
        ],
        Utc.with_ymd_and_hms(2026, 1, 10, 0, 0, 0).unwrap(),
    );

    assert_eq!(income_plan.period_start, "2026-01-01");
    assert_eq!(income_plan.period_end, "2026-12-31");
    assert_eq!(income_plan.elapsed_days, 10);
    assert_eq!(income_plan.days_in_year, 365);
    assert_eq!(income_plan.actual_ytd.get("USDT"), Some(&1500.0));
    assert_eq!(income_plan.projected_annual.get("USDT"), Some(&54_750.0));
    assert_eq!(income_plan.run_rate_monthly.get("USDT"), Some(&4562.5));
}

#[test]
fn counts_all_goals_as_full_planned_burn_without_fx_conversion() {
    let planned_expenses = calculate_planned_expenses(
        &[
            PlannedExpenseGoalLike {
                id: "trip".to_string(),
                name: "Trip".to_string(),
                target_amount: 2000.0,
                current_amount: 500.0,
                monthly_required_amount: 100.0,
                currency: "USDT".to_string(),
                deadline: Some("2026-06-30".to_string()),
                bucket: "planned_purchase".to_string(),
            },
            PlannedExpenseGoalLike {
                id: "laptop".to_string(),
                name: "Laptop".to_string(),
                target_amount: 1000.0,
                current_amount: 0.0,
                monthly_required_amount: 200.0,
                currency: "USD".to_string(),
                deadline: Some("2026-07-31".to_string()),
                bucket: "investment".to_string(),
            },
            PlannedExpenseGoalLike {
                id: "georgia".to_string(),
                name: "Georgia".to_string(),
                target_amount: 3500.0,
                current_amount: 0.0,
                monthly_required_amount: 1750.0,
                currency: "BYN".to_string(),
                deadline: Some("2026-06-30".to_string()),
                bucket: "planned_purchase".to_string(),
            },
            PlannedExpenseGoalLike {
                id: "europe".to_string(),
                name: "Europe".to_string(),
                target_amount: 800.0,
                current_amount: 0.0,
                monthly_required_amount: 0.0,
                currency: "EUR".to_string(),
                deadline: None,
                bucket: "life".to_string(),
            },
        ],
        &[("USDT".to_string(), 12_000.0), ("BYN".to_string(), 0.0)]
            .into_iter()
            .collect(),
        3000.0,
    );

    assert_eq!(
        planned_expenses.total_by_currency.get("USDT"),
        Some(&3000.0)
    );
    assert_eq!(planned_expenses.total_by_currency.get("BYN"), Some(&3500.0));
    assert_eq!(planned_expenses.total_by_currency.get("EUR"), Some(&800.0));
    assert_eq!(planned_expenses.annual_life_burn_usdt, 3000.0);
    assert_eq!(planned_expenses.projected_after_planned_usdt, 6000.0);
    assert_eq!(planned_expenses.planned_expense_ratio, 0.25);
    let trip = planned_expenses
        .goals
        .iter()
        .find(|goal| goal.name == "Trip")
        .unwrap();
    assert!((trip.impact_ratio.unwrap() - 2000.0 / 12_000.0).abs() < 0.0001);
    let georgia = planned_expenses
        .goals
        .iter()
        .find(|goal| goal.name == "Georgia")
        .unwrap();
    assert_eq!(georgia.impact_ratio, None);
}
