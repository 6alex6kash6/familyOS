use anyhow::{Context, Result, bail};
use base64::Engine;
use chrono::{DateTime, Datelike, TimeZone, Utc};
use hmac::{Hmac, Mac};
use reqwest::blocking::Client;
use serde_json::{Value, json};
use sha2::Sha256;

use crate::finance::round_money;
use crate::models::{
    ProviderAccount, ProviderBalance, ProviderPayload, ProviderPosition, ProviderTransaction,
};

pub fn sync_okx_from_env() -> Result<ProviderPayload> {
    let api_key = std::env::var("OKX_API_KEY").ok();
    let api_secret = std::env::var("OKX_API_SECRET").ok();
    let passphrase = std::env::var("OKX_API_PASSPHRASE").ok();
    let base_url =
        std::env::var("OKX_BASE_URL").unwrap_or_else(|_| "https://www.okx.com".to_string());
    if api_key.as_deref().unwrap_or("").is_empty()
        || api_secret.as_deref().unwrap_or("").is_empty()
        || passphrase.as_deref().unwrap_or("").is_empty()
        || api_secret.as_deref().unwrap_or("").contains("put_real")
        || passphrase.as_deref().unwrap_or("").contains("put_real")
    {
        return Ok(not_connected_payload());
    }
    let adapter = LiveOkxAdapter {
        api_key: api_key.unwrap(),
        api_secret: api_secret.unwrap(),
        passphrase: passphrase.unwrap(),
        base_url,
        client: Client::new(),
    };
    adapter.sync()
}

pub fn build_okx_history_paths(
    path: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Vec<String> {
    let mut paths = Vec::new();
    let mut cursor = start;
    while cursor.timestamp_millis() < end.timestamp_millis() {
        let next_month = Utc
            .with_ymd_and_hms(
                if cursor.month() == 12 {
                    cursor.year() + 1
                } else {
                    cursor.year()
                },
                if cursor.month() == 12 {
                    1
                } else {
                    cursor.month() + 1
                },
                1,
                0,
                0,
                0,
            )
            .unwrap();
        let window_end = if next_month < end { next_month } else { end };
        paths.push(format!(
            "{}?before={}&after={}&limit=100",
            path,
            cursor.timestamp_millis(),
            window_end.timestamp_millis()
        ));
        cursor = window_end;
    }
    paths
}

fn not_connected_payload() -> ProviderPayload {
    ProviderPayload {
        mode: "not_connected".to_string(),
        as_of: Utc::now().to_rfc3339(),
        accounts: Vec::new(),
        positions: Vec::new(),
        transactions: Vec::new(),
        meta: json!({
            "status": "not connected",
            "source": "not_connected",
            "readOnlyEndpoints": [
                "/api/v5/account/balance",
                "/api/v5/asset/balances",
                "/api/v5/asset/deposit-history",
                "/api/v5/asset/withdrawal-history",
                "/api/v5/asset/bills"
            ]
        }),
    }
}

struct LiveOkxAdapter {
    api_key: String,
    api_secret: String,
    passphrase: String,
    base_url: String,
    client: Client,
}

impl LiveOkxAdapter {
    fn sync(&self) -> Result<ProviderPayload> {
        let now = Utc::now();
        let history_start = Utc.with_ymd_and_hms(now.year(), 1, 1, 0, 0, 0).unwrap();
        let account_balance = self.get("/api/v5/account/balance")?;
        let funding_balances = self.get("/api/v5/asset/balances")?;
        let deposits = self.get_history("/api/v5/asset/deposit-history", history_start, now)?;
        let withdrawals =
            self.get_history("/api/v5/asset/withdrawal-history", history_start, now)?;
        let funding_bills = self.get_history("/api/v5/asset/bills", history_start, now)?;
        let as_of = Utc::now().to_rfc3339();
        let trading_balances = normalize_trading_balances(&account_balance);
        let funding_rows = normalize_funding_balances(&funding_balances);
        let mut transactions = Vec::new();
        transactions.extend(normalize_deposits(&deposits));
        transactions.extend(normalize_withdrawals(&withdrawals));
        transactions.extend(normalize_funding_bills(&funding_bills));
        transactions.sort_by(|a, b| b.date.cmp(&a.date));
        Ok(ProviderPayload {
            mode: "live".to_string(),
            as_of,
            accounts: vec![
                ProviderAccount {
                    name: "OKX Trading".to_string(),
                    account_type: "crypto_trading".to_string(),
                    currency: "USDT".to_string(),
                    bucket: "crypto_retained".to_string(),
                    external_id: "okx-trading-live".to_string(),
                    balances: trading_balances.clone(),
                },
                ProviderAccount {
                    name: "OKX Funding".to_string(),
                    account_type: "crypto_funding".to_string(),
                    currency: "USDT".to_string(),
                    bucket: "crypto_retained".to_string(),
                    external_id: "okx-funding-live".to_string(),
                    balances: funding_rows.clone(),
                },
            ],
            positions: normalize_positions(&trading_balances, &funding_rows),
            transactions,
            meta: json!({
                "source": "live",
                "historyStart": history_start.to_rfc3339(),
                "historyEnd": now.to_rfc3339(),
                "depositsCount": deposits["data"].as_array().map(|rows| rows.len()).unwrap_or(0),
                "withdrawalsCount": withdrawals["data"].as_array().map(|rows| rows.len()).unwrap_or(0),
                "movementsCount": funding_bills["data"].as_array().map(|rows| rows.len()).unwrap_or(0),
                "readOnly": true
            }),
        })
    }

    fn get_history(&self, path: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Value> {
        let mut data = Vec::new();
        for history_path in build_okx_history_paths(path, start, end) {
            let value = self.get(&history_path)?;
            if let Some(rows) = value["data"].as_array() {
                data.extend(rows.iter().cloned());
            }
        }
        Ok(json!({ "data": data }))
    }

    fn get(&self, path: &str) -> Result<Value> {
        let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let signature = self.sign(&timestamp, "GET", path, "")?;
        let response = self
            .client
            .get(format!("{}{}", self.base_url, path))
            .header("OK-ACCESS-KEY", &self.api_key)
            .header("OK-ACCESS-SIGN", signature)
            .header("OK-ACCESS-TIMESTAMP", timestamp)
            .header("OK-ACCESS-PASSPHRASE", &self.passphrase)
            .header("Content-Type", "application/json")
            .send()
            .with_context(|| format!("OKX {path} request failed"))?;
        let status = response.status();
        if !status.is_success() {
            bail!("OKX {path} failed with HTTP {status}");
        }
        let json: Value = response.json()?;
        if json["code"].as_str().is_some_and(|code| code != "0") {
            bail!(
                "OKX {path} returned code {}: {}",
                json["code"].as_str().unwrap_or("unknown"),
                json["msg"].as_str().unwrap_or("unknown error")
            );
        }
        Ok(json)
    }

    fn sign(&self, timestamp: &str, method: &str, path: &str, body: &str) -> Result<String> {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.api_secret.as_bytes())?;
        mac.update(format!("{timestamp}{method}{path}{body}").as_bytes());
        Ok(base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes()))
    }
}

fn normalize_trading_balances(response: &Value) -> Vec<ProviderBalance> {
    response["data"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| row["details"].as_array().into_iter().flatten())
        .filter_map(|detail| {
            let amount = number_value(
                detail
                    .get("eq")
                    .or_else(|| detail.get("cashBal"))
                    .or_else(|| detail.get("availBal")),
            );
            let ccy = detail["ccy"].as_str().unwrap_or("USDT").to_string();
            let usd_equivalent = number_value(detail.get("eqUsd").or_else(|| detail.get("usdEq")))
                .max(if ccy == "USDT" { amount } else { 0.0 });
            ((amount != 0.0) || (usd_equivalent != 0.0)).then_some(ProviderBalance {
                currency: ccy,
                amount,
                usd_equivalent,
                bucket: "crypto_retained".to_string(),
            })
        })
        .collect()
}

fn normalize_funding_balances(response: &Value) -> Vec<ProviderBalance> {
    response["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|detail| {
            let amount = number_value(
                detail
                    .get("bal")
                    .or_else(|| detail.get("availBal"))
                    .or_else(|| detail.get("cashBal")),
            );
            let ccy = detail["ccy"].as_str().unwrap_or("USDT").to_string();
            let usd_equivalent = if ccy == "USDT" || ccy == "USD" {
                amount
            } else {
                number_value(detail.get("eqUsd"))
            };
            ((amount != 0.0) || (usd_equivalent != 0.0)).then_some(ProviderBalance {
                currency: ccy,
                amount,
                usd_equivalent,
                bucket: "crypto_retained".to_string(),
            })
        })
        .collect()
}

fn normalize_positions(
    trading: &[ProviderBalance],
    funding: &[ProviderBalance],
) -> Vec<ProviderPosition> {
    trading
        .iter()
        .chain(funding.iter())
        .filter(|row| row.currency != "USDT" && row.currency != "USD")
        .map(|row| ProviderPosition {
            symbol: row.currency.clone(),
            name: row.currency.clone(),
            quantity: row.amount,
            currency: "USDT".to_string(),
            price: if row.amount > 0.0 {
                row.usd_equivalent / row.amount
            } else {
                0.0
            },
            value: row.usd_equivalent,
            bucket: "crypto_retained".to_string(),
        })
        .collect()
}

fn normalize_deposits(response: &Value) -> Vec<ProviderTransaction> {
    response["data"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| ProviderTransaction {
            external_id: stable_external_id(
                "deposit",
                row.get("depId")
                    .or_else(|| row.get("txId"))
                    .or_else(|| row.get("ts")),
            ),
            date: iso_from_okx_time(
                row.get("ts")
                    .or_else(|| row.get("insertTime"))
                    .or_else(|| row.get("actualDepBlkConfirmTime")),
            ),
            amount: number_value(row.get("amt")),
            currency: row["ccy"].as_str().unwrap_or("USDT").to_string(),
            category: "OKX deposit".to_string(),
            merchant_note: format!(
                "{} deposit{}",
                row["ccy"].as_str().unwrap_or("Asset"),
                row["chain"]
                    .as_str()
                    .map(|chain| format!(" via {chain}"))
                    .unwrap_or_default()
            ),
            source: "okx".to_string(),
            transaction_type: "deposit".to_string(),
            meta: row.clone(),
        })
        .collect()
}

fn normalize_withdrawals(response: &Value) -> Vec<ProviderTransaction> {
    response["data"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| ProviderTransaction {
            external_id: stable_external_id(
                "withdrawal",
                row.get("wdId")
                    .or_else(|| row.get("txId"))
                    .or_else(|| row.get("ts")),
            ),
            date: iso_from_okx_time(
                row.get("ts")
                    .or_else(|| row.get("cTime"))
                    .or_else(|| row.get("uTime")),
            ),
            amount: number_value(row.get("amt")),
            currency: row["ccy"].as_str().unwrap_or("USDT").to_string(),
            category: "OKX withdrawal".to_string(),
            merchant_note: format!(
                "{} withdrawal{}",
                row["ccy"].as_str().unwrap_or("Asset"),
                row["chain"]
                    .as_str()
                    .map(|chain| format!(" via {chain}"))
                    .unwrap_or_default()
            ),
            source: "okx".to_string(),
            transaction_type: "withdrawal".to_string(),
            meta: row.clone(),
        })
        .collect()
}

fn normalize_funding_bills(response: &Value) -> Vec<ProviderTransaction> {
    response["data"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            let amount = number_value(
                row.get("amt")
                    .or_else(|| row.get("balChg"))
                    .or_else(|| row.get("sz")),
            );
            ProviderTransaction {
                external_id: stable_external_id(
                    "transfer",
                    row.get("billId").or_else(|| row.get("ts")),
                ),
                date: iso_from_okx_time(row.get("ts")),
                amount: amount.abs(),
                currency: row["ccy"].as_str().unwrap_or("USDT").to_string(),
                category: "OKX funding movement".to_string(),
                merchant_note: format!(
                    "{} funding movement",
                    row["ccy"].as_str().unwrap_or("Asset")
                ),
                source: "okx".to_string(),
                transaction_type: "transfer".to_string(),
                meta: row.clone(),
            }
        })
        .collect()
}

fn stable_external_id(prefix: &str, value: Option<&Value>) -> String {
    let value = value.and_then(Value::as_str).unwrap_or("unknown");
    format!("okx-{prefix}-{value}")
}

fn iso_from_okx_time(value: Option<&Value>) -> String {
    let parsed = value
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<i64>().ok())
        .or_else(|| value.and_then(Value::as_i64))
        .unwrap_or(0);
    if parsed > 0 {
        Utc.timestamp_millis_opt(parsed)
            .single()
            .unwrap_or_else(Utc::now)
            .to_rfc3339()
    } else {
        Utc::now().to_rfc3339()
    }
}

fn number_value(value: Option<&Value>) -> f64 {
    value
        .and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
        })
        .filter(|value| value.is_finite())
        .map(round_money)
        .unwrap_or(0.0)
}
