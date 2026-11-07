//! Z.ai (GLM Coding Plan): `GET /api/monitor/usage/quota/limit`.
//! Belgelenmemiş; hata durumunda bile HTTP 200 + `{code, success:false}` döner
//! (2026-11-07'de doğrulandı). Toleranslı ayrıştırılır.

use serde_json::Value;

use super::{Outcome, get_bearer};
use crate::http::FetchError;
use crate::jsonx::{f64_of, str_of, time_of, u64_of};
use crate::model::UsageWindow;
use crate::providers::FetchContext;
use crate::secret::Secret;

pub async fn fetch(
    http: &reqwest::Client,
    base: &str,
    key: &Secret,
    ctx: &FetchContext,
) -> Result<Outcome, FetchError> {
    parse(
        &get_bearer(http, &format!("{base}/api/monitor/usage/quota/limit"), key).await?,
        ctx,
    )
}

pub fn parse(v: &Value, ctx: &FetchContext) -> Result<Outcome, FetchError> {
    if v.get("success") == Some(&Value::Bool(false)) {
        let code = v.get("code").and_then(u64_of).unwrap_or(0);
        let msg = v.get("msg").and_then(str_of).unwrap_or("error");
        return Err(if code == 401 || code == 403 {
            FetchError::Unauthorized(code as u16)
        } else {
            FetchError::Http {
                status: code as u16,
                snippet: msg.to_string(),
            }
        });
    }
    let d = v.get("data").ok_or_else(|| FetchError::Parse("no data".into()))?;
    let limits = d
        .get("limits")
        .and_then(Value::as_array)
        .ok_or_else(|| FetchError::Parse("no limits".into()))?;
    let mut out = Outcome {
        plan: d
            .get("level")
            .and_then(str_of)
            .map(|p| crate::providers::pretty_plan(p, None)),
        ..Outcome::default()
    };
    for l in limits {
        let kind = l.get("type").and_then(str_of).unwrap_or("limit");
        let Some(pct) = l.get("percentage").and_then(f64_of).or_else(|| {
            let total = l.get("usage").and_then(f64_of)?;
            let used = l.get("currentValue").and_then(f64_of)?;
            (total > 0.0).then(|| used / total * 100.0)
        }) else {
            continue;
        };
        let (id, label, secs) = match kind {
            "TOKENS_LIMIT" => ("session", "5-hour session", Some(5 * 3600)),
            "TIME_LIMIT" => ("tools_monthly", "Tool calls (monthly)", None),
            other => (other, other, None),
        };
        let resets_at = l.get("nextResetTime").and_then(time_of);
        out.windows.push(UsageWindow::new(
            id.to_lowercase(),
            label,
            pct,
            resets_at,
            secs,
            &ctx.thresholds,
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::ctx;
    use chrono::Utc;

    #[test]
    fn parses_limits_and_body_errors() {
        let v: Value = serde_json::from_str(r#"{"code":200,"msg":"ok","success":true,"data":{"level":"pro","limits":[{"type":"TIME_LIMIT","unit":5,"number":1,"usage":1000,"currentValue":250,"remaining":750,"percentage":25,"nextResetTime":1795000000000},{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":61,"nextResetTime":1794100000000}]}}"#).unwrap();
        let o = parse(&v, &ctx(Utc::now())).unwrap();
        assert_eq!(o.plan.as_deref(), Some("Pro"));
        assert_eq!(o.windows.len(), 2);
        assert_eq!((o.windows[1].id.as_str(), o.windows[1].used_percent), ("session", 61.0));
        assert!(o.windows[0].resets_at.is_some());
        let bad: Value =
            serde_json::from_str(r#"{"code":401,"msg":"token expired or incorrect","success":false}"#).unwrap();
        assert_eq!(
            parse(&bad, &ctx(Utc::now())).unwrap_err(),
            FetchError::Unauthorized(401)
        );
    }
}
