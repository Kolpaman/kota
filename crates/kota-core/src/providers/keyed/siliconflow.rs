//! SiliconFlow: `GET /v1/user/info` (belgelenmiş). Bakiye CNY.

use serde_json::Value;

use super::{Outcome, balance, get_bearer};
use crate::http::FetchError;
use crate::jsonx::{f64_of, u64_of};
use crate::secret::Secret;

pub async fn fetch(http: &reqwest::Client, base: &str, key: &Secret) -> Result<Outcome, FetchError> {
    parse(&get_bearer(http, &format!("{base}/v1/user/info"), key).await?)
}

pub fn parse(v: &Value) -> Result<Outcome, FetchError> {
    // Gövdede hata kodu: 20000 başarılı; 30014 vb. geçersiz token.
    if let Some(code) = v.get("code").and_then(u64_of)
        && code != 20000
        && code != 0
    {
        return Err(if (30000..31000).contains(&code) {
            FetchError::Unauthorized(401)
        } else {
            FetchError::Parse(format!("error code {code}"))
        });
    }
    let d = v.get("data").ok_or_else(|| FetchError::Parse("no data".into()))?;
    let total = d
        .get("totalBalance")
        .and_then(f64_of)
        .or_else(|| d.get("balance").and_then(f64_of))
        .ok_or_else(|| FetchError::Parse("no balance".into()))?;
    Ok(Outcome {
        balance: Some(balance("balance", total, "CNY")),
        ..Outcome::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_detects_bad_token() {
        let v: Value = serde_json::from_str(r#"{"code":20000,"message":"OK","status":true,"data":{"id":"u","balance":"0.88","chargeBalance":"88.00","totalBalance":"88.88"}}"#).unwrap();
        assert_eq!(parse(&v).unwrap().balance.unwrap().amount, 88.88);
        let bad: Value = serde_json::from_str(r#"{"code":30014,"data":null,"message":"Token is invalid."}"#).unwrap();
        assert_eq!(parse(&bad).unwrap_err(), FetchError::Unauthorized(401));
    }
}
