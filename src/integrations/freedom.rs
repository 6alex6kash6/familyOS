use anyhow::{Context, Result, bail};
use hmac::{Hmac, Mac};
use reqwest::blocking::Client;
use serde_json::{Value, json};
use sha2::Sha256;

use crate::models::{
    ProviderAccount, ProviderBalance, ProviderPayload, ProviderPosition, ProviderTransaction,
};

pub fn sync_freedom_from_env() -> Result<ProviderPayload> {
    let public_key = std::env::var("FREEDOM_PUBLIC_KEY").ok();
    let private_key = std::env::var("FREEDOM_PRIVATE_KEY").ok();
    let docs = std::env::var("FREEDOM_API_DOCS_URL")
        .unwrap_or_else(|_| "https://freedom24.com/tradernet-api/auth-login".to_string());
    let api_base_url = std::env::var("FREEDOM_TRADERNET_API_URL")
        .unwrap_or_else(|_| "https://tradernet.ru/api".to_string());
    if public_key.as_deref().unwrap_or("").is_empty()
        || private_key.as_deref().unwrap_or("").is_empty()
    {
        return Ok(ProviderPayload {
            mode: "not_connected".to_string(),
            as_of: chrono::Utc::now().to_rfc3339(),
            accounts: Vec::new(),
            positions: Vec::new(),
            transactions: Vec::new(),
            meta: json!({
                "status": "not connected",
                "source": "not_connected",
                "docs": docs,
                "latestBrokerReportStatus": "not connected"
            }),
        });
    }
    let adapter = TradernetFreedomAdapter {
        public_key: public_key.unwrap(),
        private_key: private_key.unwrap(),
        api_base_url,
        client: Client::new(),
    };
    adapter.sync()
}

struct TradernetFreedomAdapter {
    public_key: String,
    private_key: String,
    api_base_url: String,
    client: Client,
}

impl TradernetFreedomAdapter {
    fn sync(&self) -> Result<ProviderPayload> {
        let portfolio = self.call_v2("getPositionJson", None)?;
        let orders = self.call_v2("getNotifyOrderJson", None).ok();
        let broker_report = self.call_v2("getBrokerReport", None).ok();
        let cash_flows = self.call_v2("getCashflows", None).ok();
        Ok(ProviderPayload {
            mode: "live".to_string(),
            as_of: chrono::Utc::now().to_rfc3339(),
            accounts: normalize_accounts(&portfolio),
            positions: normalize_positions(&portfolio),
            transactions: normalize_transactions(orders.as_ref()),
            meta: json!({
                "source": "live",
                "readOnly": true,
                "portfolioStatus": "ok",
                "ordersStatus": if orders.is_some() { "ok" } else { "error" },
                "latestBrokerReportStatus": if broker_report.is_some() { "ok" } else { "error" },
                "moneyFundsMovementsStatus": if cash_flows.is_some() { "ok" } else { "error" },
                "moneyFundsMovementsCount": cash_flows.as_ref().and_then(|value| value["items"].as_array()).map(|rows| rows.len()).unwrap_or(0)
            }),
        })
    }

    fn call_v2(&self, command: &str, params: Option<Value>) -> Result<Value> {
        let nonce = chrono::Utc::now().timestamp_millis() * 10;
        let mut payload = vec![
            ("apiKey".to_string(), self.public_key.clone()),
            ("cmd".to_string(), command.to_string()),
            ("nonce".to_string(), nonce.to_string()),
        ];
        if let Some(params) = params {
            payload.push(("params".to_string(), params.to_string()));
        }
        payload.sort_by(|a, b| a.0.cmp(&b.0));
        let query = payload
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("&");
        let signature = self.sign(&query)?;
        let response = self
            .client
            .post(format!(
                "{}/v2/cmd/{}?{}",
                self.api_base_url, command, query
            ))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("X-NtApi-Sig", signature)
            .body(query)
            .send()
            .with_context(|| format!("Freedom {command} request failed"))?;
        let status = response.status();
        if !status.is_success() {
            bail!("Freedom {command} failed with HTTP {status}");
        }
        let json: Value = response.json()?;
        if !json["error"].is_null() {
            bail!("Freedom {command} returned API error: {}", json["error"]);
        }
        Ok(json.get("result").cloned().unwrap_or(json))
    }

    fn sign(&self, payload: &str) -> Result<String> {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.private_key.as_bytes())?;
        mac.update(payload.as_bytes());
        Ok(hex::encode(mac.finalize().into_bytes()))
    }
}

fn portfolio_scope(portfolio: &Value) -> &Value {
    portfolio
        .get("ps")
        .filter(|value| value.is_object())
        .unwrap_or(portfolio)
}

fn normalize_accounts(portfolio: &Value) -> Vec<ProviderAccount> {
    let ps = portfolio_scope(portfolio);
    let cash = ps
        .get("cash")
        .or_else(|| ps.get("money"))
        .or_else(|| ps.get("currencies"))
        .or_else(|| ps.get("acc"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let balances = cash
        .iter()
        .map(|row| ProviderBalance {
            currency: text(row, &["curr", "currency"], "USD"),
            amount: number(row, &["value", "amount", "balance", "s"]),
            usd_equivalent: number(row, &["usdValue", "valueUsd", "value", "amount", "s"]),
            bucket: "investment".to_string(),
        })
        .collect();
    vec![ProviderAccount {
        name: "Freedom Portfolio".to_string(),
        account_type: "brokerage".to_string(),
        currency: "USD".to_string(),
        bucket: "investment".to_string(),
        external_id: "freedom-portfolio-live".to_string(),
        balances,
    }]
}

fn normalize_positions(portfolio: &Value) -> Vec<ProviderPosition> {
    let ps = portfolio_scope(portfolio);
    ps.get("positions")
        .or_else(|| ps.get("securities"))
        .or_else(|| ps.get("pos"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|row| ProviderPosition {
            symbol: text(row, &["ticker", "symbol", "i"], "UNKNOWN"),
            name: text(row, &["name", "name2", "ticker", "symbol", "i"], "Position"),
            quantity: number(row, &["qty", "quantity", "q"]),
            currency: text(row, &["curr", "currency", "base_currency"], "USD"),
            price: number(row, &["price", "lastPrice", "p", "mkt_price"]),
            value: number(row, &["value", "marketValue", "usdValue", "market_value"]),
            bucket: "investment".to_string(),
        })
        .collect()
}

fn normalize_transactions(result: Option<&Value>) -> Vec<ProviderTransaction> {
    result
        .and_then(|value| value.pointer("/orders/order"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|row| {
            let price = number(row, &["p"]);
            let quantity = number(row, &["q"]);
            ProviderTransaction {
                external_id: format!(
                    "freedom-order-{}",
                    text(row, &["id", "order_id", "date"], "unknown")
                ),
                date: parse_date(row.get("date").or_else(|| row.get("stat_d"))),
                amount: number(row, &["sum", "v", "p"]).max(price * quantity),
                currency: text(row, &["cur", "curr"], "USD"),
                category: "Freedom order".to_string(),
                merchant_note: format!("{} order", text(row, &["instr", "name"], "Instrument")),
                source: "freedom".to_string(),
                transaction_type: "transfer".to_string(),
                meta: json!({ "provider": "freedom", "raw": row }),
            }
        })
        .collect()
}

fn text(row: &Value, keys: &[&str], default: &str) -> String {
    keys.iter()
        .find_map(|key| row.get(*key).and_then(Value::as_str))
        .unwrap_or(default)
        .to_string()
}

fn number(row: &Value, keys: &[&str]) -> f64 {
    keys.iter()
        .find_map(|key| row.get(*key))
        .and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
        .filter(|value: &f64| value.is_finite())
        .unwrap_or(0.0)
}

fn parse_date(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
        .map(|date| date.with_timezone(&chrono::Utc).to_rfc3339())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339())
}
