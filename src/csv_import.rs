use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum CsvImportSource {
    Freedom,
    Okx,
    Manual,
    Priorbank,
}

impl CsvImportSource {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "csv_freedom" | "freedom" => Ok(Self::Freedom),
            "csv_okx" | "okx" => Ok(Self::Okx),
            "csv_manual" | "manual" => Ok(Self::Manual),
            "csv_priobank" | "priobank" | "priorbank" => Ok(Self::Priorbank),
            _ => bail!("Unsupported CSV source"),
        }
    }

    pub fn as_db_value(self) -> &'static str {
        match self {
            Self::Freedom => "csv_freedom",
            Self::Okx => "csv_okx",
            Self::Manual => "csv_manual",
            Self::Priorbank => "csv_priobank",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsvTransaction {
    pub date: String,
    pub amount: f64,
    pub currency: String,
    pub category: String,
    pub merchant_note: String,
    pub source: CsvImportSource,
    pub transaction_type: String,
    pub meta: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsvParseResult {
    pub rows: Vec<CsvTransaction>,
    pub detected_months: Vec<String>,
    pub parsed_rows: usize,
    pub imported_rows: usize,
}

pub fn parse_csv_transactions(source: CsvImportSource, csv: &str) -> Result<CsvParseResult> {
    if source == CsvImportSource::Priorbank {
        return parse_priorbank_csv(csv);
    }
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(csv.as_bytes());
    let headers = reader.headers().context("CSV parse failed")?.clone();
    let normalized_headers: Vec<String> = headers.iter().map(normalize_key).collect();
    let mut parsed_rows = 0usize;
    let mut rows = Vec::new();
    for (index, record) in reader.records().enumerate() {
        let record = record.context("CSV parse failed")?;
        parsed_rows += 1;
        let mut row = BTreeMap::new();
        for (idx, value) in record.iter().enumerate() {
            if let Some(key) = normalized_headers.get(idx) {
                row.insert(key.clone(), value.trim().to_string());
            }
        }
        if let Some(transaction) = normalize_csv_row(&row, index, source) {
            rows.push(transaction);
        }
    }
    Ok(result(rows, parsed_rows))
}

fn parse_priorbank_csv(csv: &str) -> Result<CsvParseResult> {
    let mut rows = Vec::new();
    let mut inside_operations_block = false;
    for (index, line) in csv.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("Дата транзакции;Операция;") {
            inside_operations_block = true;
            continue;
        }
        if trimmed.starts_with("Дата транзакции;")
            || trimmed.starts_with("Всего")
            || trimmed.is_empty()
        {
            inside_operations_block = false;
            continue;
        }
        if !inside_operations_block || !looks_like_priorbank_operation(trimmed) {
            continue;
        }
        if let Some(row) = normalize_priorbank_row(trimmed, index) {
            rows.push(row);
        }
    }
    let parsed_rows = rows.len();
    Ok(result(rows, parsed_rows))
}

fn normalize_priorbank_row(line: &str, index: usize) -> Option<CsvTransaction> {
    let columns: Vec<&str> = line.split(';').collect();
    let transaction_date = columns.first().copied().unwrap_or("");
    let operation = columns.get(1).copied().unwrap_or("");
    let amount_raw = columns.get(2).copied().unwrap_or("");
    let currency_raw = columns.get(3).copied().unwrap_or("");
    let account_date = columns.get(4).copied().unwrap_or("");
    let commission_raw = columns.get(5).copied().unwrap_or("");
    let account_turnover_raw = columns.get(6).copied().unwrap_or("");
    let digital_card = columns.get(7).copied().unwrap_or("");
    let operation_category = columns.get(8).copied().unwrap_or("");
    let date = parse_date(Some(transaction_date))?;
    let amount = parse_amount(if account_turnover_raw.trim().is_empty() {
        Some(amount_raw)
    } else {
        Some(account_turnover_raw)
    })?;
    let currency = currency_raw.trim().to_uppercase();
    if currency.is_empty() {
        return None;
    }
    let category = if operation_category.trim().is_empty() {
        default_priorbank_category(operation, amount)
    } else {
        operation_category.trim().to_string()
    };
    Some(CsvTransaction {
        date,
        amount: amount.abs(),
        currency,
        category: category.clone(),
        merchant_note: if operation.trim().is_empty() {
            category
        } else {
            operation.trim().to_string()
        },
        source: CsvImportSource::Priorbank,
        transaction_type: if amount < 0.0 { "expense" } else { "deposit" }.to_string(),
        meta: json!({
            "csvRow": index + 1,
            "importedFrom": "csv_priobank",
            "accountDate": account_date.trim(),
            "commission": commission_raw.trim(),
            "digitalCard": digital_card.trim(),
            "originalAmount": amount,
            "original": {
                "transactionDate": transaction_date,
                "operation": operation,
                "amount": amount_raw,
                "currency": currency_raw,
                "accountDate": account_date,
                "commission": commission_raw,
                "accountTurnover": account_turnover_raw,
                "digitalCard": digital_card,
                "operationCategory": operation_category,
            }
        }),
    })
}

fn normalize_csv_row(
    row: &BTreeMap<String, String>,
    index: usize,
    source: CsvImportSource,
) -> Option<CsvTransaction> {
    let date = parse_date(pick(
        row,
        &["date", "operation_date", "timestamp", "time", "дата"],
    ))?;
    let amount = parse_amount(pick(row, &["amount", "sum", "value", "amt", "сумма"]))?;
    let currency = pick(row, &["currency", "ccy", "curr", "валюта"])?.to_uppercase();
    let category = pick(
        row,
        &["category", "operation", "operation_type", "type", "тип"],
    )
    .map(ToOwned::to_owned)
    .unwrap_or_else(|| default_category(source, amount));
    let merchant_note = pick(
        row,
        &[
            "merchant_note",
            "merchant",
            "note",
            "description",
            "ticker",
            "instrument",
        ],
    )
    .map(ToOwned::to_owned)
    .unwrap_or_else(|| category.clone());
    let transaction_type = if source == CsvImportSource::Manual {
        "expense"
    } else if amount < 0.0 {
        "withdrawal"
    } else {
        "deposit"
    };
    Some(CsvTransaction {
        date,
        amount: amount.abs(),
        currency,
        category,
        merchant_note,
        source,
        transaction_type: transaction_type.to_string(),
        meta: json!({
            "csvRow": index + 1,
            "importedFrom": source.as_db_value(),
            "originalAmount": amount,
            "original": row,
        }),
    })
}

fn result(rows: Vec<CsvTransaction>, parsed_rows: usize) -> CsvParseResult {
    let detected_months = rows
        .iter()
        .map(|row| row.date.chars().take(7).collect::<String>())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let imported_rows = rows.len();
    CsvParseResult {
        rows,
        detected_months,
        parsed_rows,
        imported_rows,
    }
}

fn pick<'a>(row: &'a BTreeMap<String, String>, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        if let Some(value) = row
            .get(&normalize_key(key))
            .filter(|value| !value.is_empty())
        {
            return Some(value);
        }
    }
    None
}

fn parse_amount(value: Option<&str>) -> Option<f64> {
    let normalized = value?.replace(char::is_whitespace, "").replace(',', ".");
    normalized
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

fn parse_date(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if let Ok(parsed) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(
            parsed
                .with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Millis, true),
        );
    }
    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return Some(
            date.and_hms_opt(0, 0, 0)?
                .and_utc()
                .to_rfc3339_opts(SecondsFormat::Millis, true),
        );
    }
    if let Some(parsed) = parse_day_month_year(trimmed) {
        return Some(
            parsed
                .and_utc()
                .to_rfc3339_opts(SecondsFormat::Millis, true),
        );
    }
    None
}

fn parse_day_month_year(value: &str) -> Option<NaiveDateTime> {
    let mut parts = value.split_whitespace();
    let date_part = parts.next()?;
    let time_part = parts.next().unwrap_or("00:00:00");
    let separator = if date_part.contains('.') {
        '.'
    } else if date_part.contains('/') {
        '/'
    } else if date_part.contains('-') {
        '-'
    } else {
        return None;
    };
    let pieces: Vec<&str> = date_part.split(separator).collect();
    if pieces.len() != 3 || pieces[2].len() != 4 {
        return None;
    }
    let day = pieces[0].parse::<u32>().ok()?;
    let month = pieces[1].parse::<u32>().ok()?;
    let year = pieces[2].parse::<i32>().ok()?;
    let date = NaiveDate::from_ymd_opt(year, month, day)?;
    let time = NaiveTime::parse_from_str(time_part, "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(time_part, "%H:%M"))
        .unwrap_or(NaiveTime::MIN);
    Some(date.and_time(time))
}

fn looks_like_priorbank_operation(line: &str) -> bool {
    let Some(prefix) = line.get(0..10) else {
        return false;
    };
    prefix.chars().enumerate().all(|(index, ch)| match index {
        0 | 1 | 3 | 4 | 6 | 7 | 8 | 9 => ch.is_ascii_digit(),
        2 | 5 => ch == '.',
        _ => false,
    })
}

fn normalize_key(value: &str) -> String {
    value.trim().to_lowercase()
}

fn default_category(source: CsvImportSource, amount: f64) -> String {
    match source {
        CsvImportSource::Manual => "Прочее".to_string(),
        CsvImportSource::Priorbank => if amount < 0.0 {
            "Прочее"
        } else {
            "Пополнение"
        }
        .to_string(),
        CsvImportSource::Freedom | CsvImportSource::Okx => if amount < 0.0 {
            "Broker withdrawal"
        } else {
            "Broker deposit"
        }
        .to_string(),
    }
}

fn default_priorbank_category(operation: &str, amount: f64) -> String {
    if amount >= 0.0 {
        "Пополнение".to_string()
    } else if operation.to_lowercase().contains("yandex") {
        "Транспорт - Такси".to_string()
    } else {
        "Прочее".to_string()
    }
}
