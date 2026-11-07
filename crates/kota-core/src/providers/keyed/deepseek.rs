//! DeepSeek: `GET /user/balance` (belgelenmiş). Kota değil bakiye döner.

use serde_json::Value;

use super::{Outcome, balance, get_bearer};
use crate::http::FetchError;
use crate::jsonx::{f64_of, str_of};
use crate::model::Detail;
use crate::secret::Secret;

pub async fn fetch(http: &reqwest::Client, base: &str, key: &Secret) -> Result<Outcome, FetchError> {
    parse(&get_bearer(http, &format!("{base}/user/balance"), key).await?)
}

pub fn parse(v: &Value) -> Result<Outcome, FetchError> {
    let infos = v
        .get("balance_infos")
        .and_then(Value::as_array)
        .ok_or_else(|| FetchError::Parse("no balance_infos".into()))?;
    // USD varsa onu, yoksa ilkini göster; diğerleri ayrıntıya.
    let pick = infos
        .iter()
        .position(|i| i.get("currency").and_then(str_of) == Some("USD"))
        .unwrap_or(0);
    let mut out = Outcome::default();
    for (i, info) in infos.iter().enumerate() {
        let cur = info.get("currency").and_then(str_of).unwrap_or("CNY");
        let Some(total) = info.get("total_balance").and_then(f64_of) else {
            continue;
        };
        if i == pick {
            out.balance = Some(balance("balance", total, cur));
        } else {
            out.details.push(Detail {
                id: format!("balance:{cur}"),
                label: cur.into(),
                value: format!("{total:.2}"),
            });
        }
    }
    if v.get("is_available") == Some(&Value::Bool(false)) {
        out.details.push(Detail {
            id: "unavailable".into(),
            label: "Status".into(),
            value: "insufficient balance".into(),
        });
    }
    if out.balance.is_none() {
        return Err(FetchError::Parse("no usable balance".into()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_usd_and_flags_unavailable() {
        let v: Value = serde_json::from_str(r#"{"is_available":false,"balance_infos":[{"currency":"CNY","total_balance":"3.00"},{"currency":"USD","total_balance":"0.42"}]}"#).unwrap();
        let o = parse(&v).unwrap();
        let b = o.balance.unwrap();
        assert_eq!((b.amount, b.currency.as_str()), (0.42, "USD"));
        assert_eq!(o.details.len(), 2);
        assert!(parse(&serde_json::json!({"balance_infos": []})).is_err());
    }
}
