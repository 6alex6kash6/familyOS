use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinanceSettings {
    pub monthly_income: f64,
    pub monthly_taxable_base: f64,
    pub tax_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalLike {
    pub name: String,
    pub target_amount: f64,
    pub current_amount: f64,
    pub monthly_required_amount: f64,
    pub bucket: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedExpenseGoalLike {
    pub id: String,
    pub name: String,
    pub target_amount: f64,
    pub current_amount: f64,
    pub monthly_required_amount: f64,
    pub currency: String,
    pub deadline: Option<String>,
    pub bucket: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomeTransactionLike {
    pub date: String,
    pub amount: f64,
    pub currency: String,
    pub source: String,
    pub transaction_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsRow {
    pub id: String,
    pub tax_mode: String,
    pub monthly_income: f64,
    pub monthly_taxable_base: f64,
    pub tax_rate: f64,
    pub base_currency: String,
    pub life_currency: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalRow {
    pub id: String,
    pub name: String,
    pub target_amount: f64,
    pub currency: String,
    pub deadline: Option<String>,
    pub priority: i64,
    pub current_amount: f64,
    pub monthly_required_amount: f64,
    pub bucket: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRow {
    pub id: String,
    pub date: String,
    pub amount: f64,
    pub currency: String,
    pub category: String,
    pub merchant_note: String,
    pub source: String,
    pub source_meta_json: String,
    pub meta: Value,
    pub transaction_type: String,
    pub account_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRow {
    pub id: String,
    pub provider: String,
    pub name: String,
    pub account_type: String,
    pub currency: String,
    pub status: String,
    pub bucket: String,
    pub external_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceRow {
    pub id: String,
    pub account_id: String,
    pub currency: String,
    pub amount: f64,
    pub usd_equivalent: f64,
    pub bucket: String,
    pub source: String,
    pub origin: String,
    pub meta_json: String,
    pub meta: Value,
    pub as_of: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionRow {
    pub id: String,
    pub provider: String,
    pub symbol: String,
    pub name: String,
    pub quantity: f64,
    pub currency: String,
    pub price: f64,
    pub value: f64,
    pub bucket: String,
    pub as_of: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRow {
    pub id: String,
    pub provider: String,
    pub status: String,
    pub mode: String,
    pub last_synced_at: Option<String>,
    pub last_error: Option<String>,
    pub meta_json: String,
    pub meta: Value,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncAttemptRow {
    pub id: String,
    pub provider: String,
    pub status: String,
    pub mode: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: i64,
    pub error: Option<String>,
    pub meta_json: String,
    pub meta: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRow {
    pub id: String,
    pub month: String,
    pub income: f64,
    pub taxable_base: f64,
    pub tax_reserve: f64,
    pub expenses_byn: f64,
    pub expenses_usdt: f64,
    pub free_surplus: f64,
    pub goals_progress_json: String,
    pub assets_snapshot_json: String,
    pub goals_progress: Value,
    pub assets_snapshot: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportRow {
    pub id: String,
    pub report_type: String,
    pub title: String,
    pub content_json: String,
    pub content: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CurrencySplit {
    #[serde(rename = "BYN")]
    pub byn: f64,
    #[serde(rename = "USDT")]
    pub usdt: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MonthlySummary {
    pub tax_reserve: f64,
    pub planned_goal_allocations: f64,
    pub free_surplus: f64,
    pub crypto_retained: f64,
    pub savings_rate: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnnualForecast {
    pub gross: f64,
    pub taxable: f64,
    pub tax_reserve: f64,
    pub retained_crypto_investment: f64,
    pub free_surplus: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IncomePlan {
    pub period_start: String,
    pub period_end: String,
    pub actual_ytd: BTreeMap<String, f64>,
    pub projected_annual: BTreeMap<String, f64>,
    pub run_rate_monthly: BTreeMap<String, f64>,
    pub elapsed_days: i64,
    pub days_in_year: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedExpenseImpact {
    pub id: String,
    pub name: String,
    pub target_amount: f64,
    pub currency: String,
    pub normalized_currency: String,
    pub deadline: Option<String>,
    pub bucket: String,
    pub impact_ratio: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlannedExpenses {
    pub total_by_currency: BTreeMap<String, f64>,
    pub annual_life_burn_usdt: f64,
    pub projected_after_planned_usdt: f64,
    pub planned_expense_ratio: f64,
    pub goals: Vec<PlannedExpenseImpact>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoalProgress {
    pub current: f64,
    pub target: f64,
    pub ratio: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoalsSummary {
    pub emergency: GoalProgress,
    pub planned_purchases: GoalProgress,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CategorySpend {
    pub category: String,
    #[serde(rename = "BYN")]
    pub byn: f64,
    #[serde(rename = "USDT")]
    pub usdt: f64,
    pub total_comparable: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpendingSummary {
    pub today: CurrencySplit,
    pub month_to_date: CurrencySplit,
    pub by_category: Vec<CategorySpend>,
    pub top_leaks: Vec<CategorySpend>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaxSummary {
    pub mode: String,
    pub monthly_target: f64,
    pub annual_target: f64,
    pub reserve_progress: f64,
    pub reserve_gap: f64,
    pub annual_reserve_gap: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetBucket {
    pub bucket: String,
    pub label: String,
    #[serde(rename = "USDT")]
    pub usdt: f64,
    #[serde(rename = "BYN")]
    pub byn: f64,
    pub ratio: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetOrigin {
    pub origin: String,
    #[serde(rename = "USDT")]
    pub usdt: f64,
    #[serde(rename = "BYN")]
    pub byn: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetsSummary {
    pub total_usdt: f64,
    pub by_origin: Vec<AssetOrigin>,
    pub buckets: Vec<AssetBucket>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InflowSummary {
    pub current_year_start: String,
    pub current_month_key: String,
    pub current_month: CurrencySplit,
    pub current_year: CurrencySplit,
    pub deposit_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DailyReportHint {
    pub generated_at: String,
    pub attention_items: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MonthRow {
    pub month: String,
    pub income: f64,
    pub income_kind: String,
    pub planned_expenses: BTreeMap<String, f64>,
    pub planned_usdt: f64,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppSummary {
    pub monthly: MonthlySummary,
    pub annual: AnnualForecast,
    pub inflow: InflowSummary,
    pub income_plan: IncomePlan,
    pub planned_expenses: PlannedExpenses,
    pub ytd_gross: f64,
    pub ytd_taxable: f64,
    pub ytd_tax_reserve: f64,
    pub ytd_expenses: CurrencySplit,
    pub tax: TaxSummary,
    pub goals: GoalsSummary,
    pub spending: SpendingSummary,
    pub assets: AssetsSummary,
    pub latest_daily_report_hint: DailyReportHint,
    pub months: Vec<MonthRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub settings: SettingsRow,
    pub goals: Vec<GoalRow>,
    pub expenses: Vec<TransactionRow>,
    pub provider_transactions: Vec<TransactionRow>,
    pub accounts: Vec<AccountRow>,
    pub balances: Vec<BalanceRow>,
    pub positions: Vec<PositionRow>,
    pub syncs: Vec<SyncRow>,
    pub sync_attempts: Vec<SyncAttemptRow>,
    pub snapshots: Vec<SnapshotRow>,
    pub reports: Vec<ReportRow>,
    pub summary: AppSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPayload {
    pub mode: String,
    pub as_of: String,
    pub accounts: Vec<ProviderAccount>,
    pub positions: Vec<ProviderPosition>,
    pub transactions: Vec<ProviderTransaction>,
    pub meta: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAccount {
    pub name: String,
    pub account_type: String,
    pub currency: String,
    pub bucket: String,
    pub external_id: String,
    pub balances: Vec<ProviderBalance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBalance {
    pub currency: String,
    pub amount: f64,
    pub usd_equivalent: f64,
    pub bucket: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPosition {
    pub symbol: String,
    pub name: String,
    pub quantity: f64,
    pub currency: String,
    pub price: f64,
    pub value: f64,
    pub bucket: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderTransaction {
    pub external_id: String,
    pub date: String,
    pub amount: f64,
    pub currency: String,
    pub category: String,
    pub merchant_note: String,
    pub source: String,
    pub transaction_type: String,
    pub meta: Value,
}
