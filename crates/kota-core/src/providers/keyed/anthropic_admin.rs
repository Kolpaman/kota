//! Anthropic API: `GET /v1/organizations/cost_report` (belgelenmiş, yönetici
//! anahtarı `sk-ant-admin…` gerekir). Tutarlar en küçük para biriminde
//! (sent) ondalık metin olarak gelir.

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;

use super::{Outcome, balance, month_start};
use crate::http::{self, FetchError};
use crate::jsonx::{f64_of, str_of};
use crate::secret::Secret;

const MAX_PAGES: usize = 5;

pub async fn fetch(
    http_client: &reqwest::Client,
    base: &str,
    key: &Secret,
    now: DateTime<Utc>,
) -> Result<Outcome, FetchError> {
    let start = month_start(now).to_rfc3339_opts(SecondsFormat::Secs, true);
    let mut cents = 0.0;
    let mut page: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let mut url = format!("{base}/v1/organizations/cost_report?starting_at={start}&bucket_width=1d&limit=31");
        if let Some(p) = &page {
            url.push_str(&format!("&page={p}"));
        }
        let v = http::send_json(
            http_client
                .get(&url)
                .header("x-api-key", key.expose())
                .header("anthropic-version", "2023-06-01"),
        )
        .await?;
        cents += sum_page(&v)?;
        page = v.get("next_page").and_then(str_of).map(str::to_string);
        if v.get("has_more") != Some(&Value::Bool(true)) || page.is_none() {
            break;
        }
    }
    Ok(Outcome {
        balance: Some(balance("spend", cents / 100.0, "USD")),
        ..Outcome::default()
    })
}

/// Sayfadaki toplam (sent).
pub fn sum_page(v: &Value) -> Result<f64, FetchError> {
    let data = v
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| FetchError::Parse("no data".into()))?;
    Ok(data
        .iter()
        .flat_map(|b| b.get("results").and_then(Value::as_array).into_iter().flatten())
        .filter_map(|r| r.get("amount").and_then(f64_of))
        .sum())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_cents() {
        let v: Value = serde_json::from_str(r#"{"data":[{"starting_at":"2026-11-01T00:00:00Z","results":[{"currency":"USD","amount":"123.45"},{"currency":"USD","amount":"6.55"}]}],"has_more":false,"next_page":null}"#).unwrap();
        assert_eq!(sum_page(&v).unwrap(), 130.0);
    }
}
