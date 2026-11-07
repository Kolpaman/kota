//! OpenAI API: `GET /v1/organization/costs` (belgelenmiş, yönetici anahtarı
//! gerekir). Bu ayki toplam harcama.

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::{Outcome, balance, get_bearer, month_start};
use crate::http::FetchError;
use crate::jsonx::{f64_of, str_of};
use crate::secret::Secret;

const MAX_PAGES: usize = 5;

pub async fn fetch(
    http: &reqwest::Client,
    base: &str,
    key: &Secret,
    now: DateTime<Utc>,
) -> Result<Outcome, FetchError> {
    let start = month_start(now).timestamp();
    let mut total = 0.0;
    let mut page: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let mut url = format!("{base}/v1/organization/costs?start_time={start}&bucket_width=1d&limit=31");
        if let Some(p) = &page {
            url.push_str(&format!("&page={p}"));
        }
        let v = get_bearer(http, &url, key).await?;
        total += sum_page(&v)?;
        page = v.get("next_page").and_then(str_of).map(str::to_string);
        if v.get("has_more") != Some(&Value::Bool(true)) || page.is_none() {
            break;
        }
    }
    Ok(Outcome {
        balance: Some(balance("spend", total, "USD")),
        ..Outcome::default()
    })
}

pub fn sum_page(v: &Value) -> Result<f64, FetchError> {
    let data = v
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| FetchError::Parse("no data".into()))?;
    Ok(data
        .iter()
        .flat_map(|b| b.get("results").and_then(Value::as_array).into_iter().flatten())
        .filter_map(|r| r.get("amount").and_then(|a| a.get("value")).and_then(f64_of))
        .sum())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_buckets() {
        let v: Value = serde_json::from_str(r#"{"data":[{"results":[{"amount":{"value":1.25,"currency":"usd"}},{"amount":{"value":"0.75"}}]},{"results":[]}],"has_more":false}"#).unwrap();
        assert_eq!(sum_page(&v).unwrap(), 2.0);
        assert!(sum_page(&serde_json::json!({"error": {}})).is_err());
    }
}
