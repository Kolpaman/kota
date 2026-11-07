//! Kimi / Moonshot: `GET /v1/users/me/balance` (belgelenmiş).
//! Uluslararası platform (api.moonshot.ai) USD, Çin platformu (.cn) CNY.

use serde_json::Value;

use super::{Outcome, balance, get_bearer};
use crate::http::FetchError;
use crate::jsonx::f64_of;
use crate::model::Detail;
use crate::secret::Secret;

pub async fn fetch(http: &reqwest::Client, base: &str, key: &Secret) -> Result<Outcome, FetchError> {
    let currency = if base.contains(".cn") { "CNY" } else { "USD" };
    parse(
        &get_bearer(http, &format!("{base}/v1/users/me/balance"), key).await?,
        currency,
    )
}

pub fn parse(v: &Value, currency: &str) -> Result<Outcome, FetchError> {
    let d = v.get("data").ok_or_else(|| FetchError::Parse("no data".into()))?;
    let available = d
        .get("available_balance")
        .and_then(f64_of)
        .ok_or_else(|| FetchError::Parse("no available_balance".into()))?;
    let mut out = Outcome {
        balance: Some(balance("balance", available, currency)),
        ..Outcome::default()
    };
    if let Some(v) = d.get("voucher_balance").and_then(f64_of).filter(|v| *v > 0.0) {
        out.details.push(Detail {
            id: "voucher".into(),
            label: "Voucher".into(),
            value: format!("{v:.2}"),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_balance() {
        let v: Value = serde_json::from_str(r#"{"code":0,"data":{"available_balance":49.58894,"voucher_balance":46.58893,"cash_balance":3.00001},"scode":"0x0","status":true}"#).unwrap();
        let o = parse(&v, "USD").unwrap();
        assert_eq!(o.balance.unwrap().amount, 49.58894);
        assert_eq!(o.details[0].value, "46.59");
    }
}
