use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};

use crate::models::{
    AnnualForecast, FinanceSettings, GoalLike, IncomePlan, IncomeTransactionLike, MonthlySummary,
    PlannedExpenseGoalLike, PlannedExpenseImpact, PlannedExpenses,
};

pub fn calculate_tax_reserve(taxable_base: f64, tax_rate: f64) -> f64 {
    round_money(taxable_base * tax_rate)
}

pub fn calculate_monthly_required_amount(
    target_amount: f64,
    current_amount: f64,
    deadline: Option<&str>,
    now: DateTime<Utc>,
) -> f64 {
    let remaining = (target_amount - current_amount).max(0.0);
    if deadline.is_none() || remaining == 0.0 {
        return 0.0;
    }
    let Some(deadline_date) =
        deadline.and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
    else {
        return 0.0;
    };
    let months = (deadline_date.year() - now.year()) * 12 + deadline_date.month() as i32
        - now.month() as i32
        + 1;
    if months > 0 {
        round_money(remaining / f64::from(months))
    } else {
        remaining
    }
}

pub fn calculate_monthly_summary(
    settings: &FinanceSettings,
    _goals: &[GoalLike],
) -> MonthlySummary {
    let tax_reserve = calculate_tax_reserve(settings.monthly_taxable_base, settings.tax_rate);
    let free_surplus = settings.monthly_income - settings.monthly_taxable_base - tax_reserve;
    let crypto_retained = settings.monthly_income - settings.monthly_taxable_base;
    MonthlySummary {
        tax_reserve: round_money(tax_reserve),
        planned_goal_allocations: 0.0,
        free_surplus: round_money(free_surplus),
        crypto_retained: round_money(crypto_retained),
        savings_rate: if settings.monthly_income > 0.0 {
            crypto_retained / settings.monthly_income
        } else {
            0.0
        },
    }
}

pub fn calculate_income_plan(rows: &[IncomeTransactionLike], now: DateTime<Utc>) -> IncomePlan {
    let year = now.year();
    let period_start_date = Utc.with_ymd_and_hms(year, 1, 1, 0, 0, 0).unwrap();
    let period_end_date = Utc.with_ymd_and_hms(year, 12, 31, 0, 0, 0).unwrap();
    let period_start = period_start_date.format("%Y-%m-%d").to_string();
    let period_end = period_end_date.format("%Y-%m-%d").to_string();
    let elapsed_days = (now.date_naive() - period_start_date.date_naive()).num_days() + 1;
    let days_in_year =
        (Utc.with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0).unwrap() - period_start_date).num_days();
    let actual_rows: Vec<_> = rows
        .iter()
        .filter(|row| {
            (row.source == "okx" || row.source == "csv_okx")
                && row.transaction_type == "deposit"
                && row.date >= period_start
        })
        .cloned()
        .collect();
    let actual_ytd = split_plan_currency_totals(&actual_rows);
    let projected_annual = map_currency_totals(&actual_ytd, |amount, _currency| {
        round_money((amount / elapsed_days as f64) * days_in_year as f64)
    });
    let run_rate_monthly = map_currency_totals(&projected_annual, |amount, _currency| {
        round_money(amount / 12.0)
    });
    IncomePlan {
        period_start,
        period_end,
        actual_ytd,
        projected_annual,
        run_rate_monthly,
        elapsed_days,
        days_in_year,
    }
}

pub fn calculate_planned_expenses(
    goals: &[PlannedExpenseGoalLike],
    projected_annual: &BTreeMap<String, f64>,
    annual_life_burn_usdt: f64,
) -> PlannedExpenses {
    let mut total_by_currency = BTreeMap::new();
    for goal in goals {
        let currency = normalize_plan_currency(&goal.currency);
        let next = total_by_currency.get(&currency).copied().unwrap_or(0.0) + goal.target_amount;
        total_by_currency.insert(currency, round_money(next));
    }
    let projected_annual_usdt = projected_annual.get("USDT").copied().unwrap_or(0.0);
    let planned_usdt = total_by_currency.get("USDT").copied().unwrap_or(0.0);
    PlannedExpenses {
        total_by_currency,
        annual_life_burn_usdt: round_money(annual_life_burn_usdt),
        projected_after_planned_usdt: round_money(
            projected_annual_usdt - annual_life_burn_usdt - planned_usdt,
        ),
        planned_expense_ratio: if projected_annual_usdt > 0.0 {
            planned_usdt / projected_annual_usdt
        } else {
            0.0
        },
        goals: goals
            .iter()
            .map(|goal| {
                let normalized_currency = normalize_plan_currency(&goal.currency);
                let impact_ratio = if normalized_currency == "USDT" && projected_annual_usdt > 0.0 {
                    Some(goal.target_amount / projected_annual_usdt)
                } else {
                    None
                };
                PlannedExpenseImpact {
                    id: goal.id.clone(),
                    name: goal.name.clone(),
                    target_amount: goal.target_amount,
                    currency: goal.currency.clone(),
                    normalized_currency,
                    deadline: goal.deadline.clone(),
                    bucket: goal.bucket.clone(),
                    impact_ratio,
                }
            })
            .collect(),
    }
}

pub fn calculate_annual_forecast(settings: &FinanceSettings, goals: &[GoalLike]) -> AnnualForecast {
    let monthly = calculate_monthly_summary(settings, goals);
    AnnualForecast {
        gross: round_money(settings.monthly_income * 12.0),
        taxable: round_money(settings.monthly_taxable_base * 12.0),
        tax_reserve: round_money(monthly.tax_reserve * 12.0),
        retained_crypto_investment: round_money(monthly.crypto_retained * 12.0),
        free_surplus: round_money(monthly.free_surplus * 12.0),
    }
}

pub fn progress_ratio(current: f64, target: f64) -> f64 {
    if target <= 0.0 {
        0.0
    } else {
        (current / target).clamp(0.0, 1.0)
    }
}

pub fn round_money(value: f64) -> f64 {
    ((value + f64::EPSILON) * 100.0).round() / 100.0
}

fn split_plan_currency_totals(rows: &[IncomeTransactionLike]) -> BTreeMap<String, f64> {
    let mut totals = BTreeMap::new();
    for row in rows {
        let currency = normalize_plan_currency(&row.currency);
        let next = totals.get(&currency).copied().unwrap_or(0.0) + row.amount;
        totals.insert(currency, round_money(next));
    }
    totals
}

fn map_currency_totals(
    totals: &BTreeMap<String, f64>,
    mapper: impl Fn(f64, &str) -> f64,
) -> BTreeMap<String, f64> {
    totals
        .iter()
        .map(|(currency, amount)| (currency.clone(), mapper(*amount, currency)))
        .collect()
}

pub fn normalize_plan_currency(currency: &str) -> String {
    if currency == "USD" {
        "USDT".to_string()
    } else {
        currency.to_string()
    }
}
