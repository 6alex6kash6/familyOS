use crate::models::AppState;

pub fn render_overview(state: &AppState) -> String {
    let mut output = String::new();
    output.push_str("Обзор\n");
    output.push_str(&format!(
        "OKX факт с начала года: {:.2} USDT\n",
        state
            .summary
            .income_plan
            .actual_ytd
            .get("USDT")
            .copied()
            .unwrap_or(0.0)
    ));
    output.push_str(&format!(
        "Прогноз годового инкама: {:.2} USDT\n",
        state
            .summary
            .income_plan
            .projected_annual
            .get("USDT")
            .copied()
            .unwrap_or(0.0)
    ));
    output.push_str(&format!(
        "Плановые расходы: {:.2} USDT\n",
        state
            .summary
            .planned_expenses
            .total_by_currency
            .get("USDT")
            .copied()
            .unwrap_or(0.0)
    ));
    output.push_str(&format!(
        "Останется после жизни и планов: {:.2} USDT\n",
        state.summary.planned_expenses.projected_after_planned_usdt
    ));
    output.push_str("\nМесяцы:\n");
    for month in &state.summary.months {
        output.push_str(&format!(
            "{} | {:.2} | {:.2} planned | {}\n",
            month.month, month.income, month.planned_usdt, month.status
        ));
    }
    output
}

pub fn render_expenses(state: &AppState) -> String {
    let mut output = String::new();
    output.push_str("Расходы\n");
    output.push_str(&format!(
        "Сегодня: {:.2} BYN / {:.2} USDT\nМесяц: {:.2} BYN / {:.2} USDT\n",
        state.summary.spending.today.byn,
        state.summary.spending.today.usdt,
        state.summary.spending.month_to_date.byn,
        state.summary.spending.month_to_date.usdt
    ));
    for row in state.expenses.iter().take(20) {
        output.push_str(&format!(
            "{} | {:.2} {} | {} | {}\n",
            row.date.chars().take(10).collect::<String>(),
            row.amount,
            row.currency,
            row.category,
            row.merchant_note
        ));
    }
    output
}

pub fn render_goals(state: &AppState) -> String {
    let mut output = String::new();
    output.push_str("Цели\n");
    output.push_str("Доля от прогноза\n");
    for goal in &state.goals {
        let impact = state
            .summary
            .planned_expenses
            .goals
            .iter()
            .find(|item| item.id == goal.id)
            .and_then(|item| item.impact_ratio)
            .map(|value| format!("{:.1}%", value * 100.0))
            .unwrap_or_else(|| "n/a".to_string());
        output.push_str(&format!(
            "{} | {:.2} {} | {} | {}\n",
            goal.name, goal.target_amount, goal.currency, goal.bucket, impact
        ));
    }
    output
}

pub fn render_assets(state: &AppState) -> String {
    let mut output = String::new();
    output.push_str("Активы\n");
    output.push_str(&format!(
        "Всего: {:.2} USDT\n",
        state.summary.assets.total_usdt
    ));
    for bucket in &state.summary.assets.buckets {
        output.push_str(&format!(
            "{}: {:.2} USDT / {:.2} BYN\n",
            bucket.label, bucket.usdt, bucket.byn
        ));
    }
    output
}

pub fn render_taxes(state: &AppState) -> String {
    format!(
        "Налоги\nМесячная цель: {:.2} USDT\nГодовая цель: {:.2} USDT\nПрогресс: {:.2} USDT\nGap: {:.2} USDT\n",
        state.summary.tax.monthly_target,
        state.summary.tax.annual_target,
        state.summary.tax.reserve_progress,
        state.summary.tax.reserve_gap
    )
}

pub fn render_reports(state: &AppState) -> String {
    let mut output = String::new();
    output.push_str("Отчеты\n");
    for report in &state.reports {
        output.push_str(&format!(
            "{} | {} | {}\n",
            report.created_at, report.report_type, report.title
        ));
    }
    output
}

pub fn render_integrations(state: &AppState) -> String {
    let mut output = String::new();
    output.push_str("Интеграции\nCSV importer\n");
    output.push_str("Priobank API integration planned after official API access is available\n");
    for sync in &state.syncs {
        output.push_str(&format!(
            "{} | {} | {}\n",
            sync.provider, sync.status, sync.mode
        ));
    }
    output
}

pub fn render_settings(state: &AppState) -> String {
    format!(
        "Настройки\nLocal-first SQLite\nRead-only integrations\nBase: {} / Life: {}\nTax mode: {}\n",
        state.settings.base_currency, state.settings.life_currency, state.settings.tax_mode
    )
}
