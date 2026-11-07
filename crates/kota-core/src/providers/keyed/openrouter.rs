//! OpenRouter: `GET /api/v1/key` (anahtar limiti) ve `GET /api/v1/credits`
//! (hesap kredisi). İkisi de belgelenmiş.

use serde_json::Value;

use super::{Outcome, balance, get_bearer};
use crate::config::Thresholds;
use crate::http::FetchError;
use crate::jsonx::{f64_of, str_of};
use crate::model::{Detail, UsageWindow};
use crate::secret::Secret;

pub async fn fetch(http: &reqwest::Client, base: &str, key: &Secret) -> Result<Outcome, FetchError> {
    let key_info = get_bearer(http, &format!("{base}/api/v1/key"), key).await?;
    // Kredi uç noktası bazı anahtar türlerinde kapalı olabilir; o zaman yalnız limit.
    let credits = get_bearer(http, &format!("{base}/api/v1/credits"), key).await.ok();
    parse(&key_info, credits.as_ref(), &Thresholds::default())
}

pub fn parse(key_info: &Value, credits: Option<&Value>, th: &Thresholds) -> Result<Outcome, FetchError> {
    let d = key_info
        .get("data")
        .ok_or_else(|| FetchError::Parse("no data".into()))?;
    let mut out = Outcome::default();
    let usage = d.get("usage").and_then(f64_of).unwrap_or(0.0);
    if let Some(limit) = d.get("limit").and_then(f64_of).filter(|l| *l > 0.0) {
        let reset = d.get("limit_reset").and_then(str_of).unwrap_or("");
        let (id, label) = match reset {
            "daily" => ("key_daily", "Key limit (daily)"),
            "weekly" => ("key_weekly", "Key limit (weekly)"),
            "monthly" => ("key_monthly", "Key limit (monthly)"),
            _ => ("key_limit", "Key limit"),
        };
        out.windows
            .push(UsageWindow::new(id, label, usage / limit * 100.0, None, None, th));
    }
    if d.get("is_free_tier") == Some(&Value::Bool(true)) {
        out.plan = Some("Free".into());
    }
    if let Some(c) = credits.and_then(|c| c.get("data")) {
        let total = c.get("total_credits").and_then(f64_of).unwrap_or(0.0);
        let used = c.get("total_usage").and_then(f64_of).unwrap_or(0.0);
        out.balance = Some(balance("balance", total - used, "USD"));
    }
    out.details.push(Detail {
        id: "key_usage".into(),
        label: "Key usage".into(),
        value: format!("${usage:.2}"),
    });
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn limit_and_credits() {
        let k =
            json!({"data": {"label": "x", "usage": 2.5, "limit": 10, "limit_reset": "monthly", "is_free_tier": false}});
        let c = json!({"data": {"total_credits": 20, "total_usage": 7.25}});
        let o = parse(&k, Some(&c), &Thresholds::default()).unwrap();
        assert_eq!(o.windows[0].id, "key_monthly");
        assert_eq!(o.windows[0].used_percent, 25.0);
        assert_eq!(o.balance.unwrap().amount, 12.75);
    }

    #[test]
    fn unlimited_key_without_credits() {
        let k = json!({"data": {"usage": 1, "limit": null, "is_free_tier": true}});
        let o = parse(&k, None, &Thresholds::default()).unwrap();
        assert!(o.windows.is_empty() && o.balance.is_none());
        assert_eq!(o.plan.as_deref(), Some("Free"));
    }
}
