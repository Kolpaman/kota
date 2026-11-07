//! `GET chatgpt.com/backend-api/wham/usage` yanıtının ayrıştırılması.
//!
//! Şema (2026-11, bkz. `docs/kesif.md`):
//! `rate_limit.{primary_window,secondary_window}: {used_percent,
//! limit_window_seconds, reset_after_seconds, reset_at}`; ayrıca
//! `code_review_rate_limit`, `additional_rate_limits[]`, `credits`.

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

use super::super::common::window_identity;
use crate::config::Thresholds;
use crate::jsonx::{f64_of, str_of, time_of, u64_of};
use crate::locallogs::codex::RateSample;
use crate::model::{Detail, UsageWindow};

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedUsage {
    pub plan: Option<String>,
    pub windows: Vec<UsageWindow>,
    pub details: Vec<Detail>,
}

pub fn parse(v: &Value, th: &Thresholds, now: DateTime<Utc>) -> Result<ParsedUsage, String> {
    let obj = v.as_object().ok_or("usage response is not a JSON object")?;
    let rate = obj.get("rate_limit").ok_or("usage response has no rate_limit")?;
    let mut windows = Vec::new();
    push_limit(&mut windows, rate, None, th, now);
    if let Some(cr) = obj.get("code_review_rate_limit") {
        push_limit(&mut windows, cr, Some(("code_review", "Code review")), th, now);
    }
    if let Some(extra) = obj.get("additional_rate_limits").and_then(Value::as_array) {
        for item in extra {
            let name = item
                .get("limit_name")
                .and_then(str_of)
                .or_else(|| item.get("metered_feature").and_then(str_of))
                .unwrap_or("extra");
            let slug = format!(
                "extra:{}",
                name.to_lowercase().replace(|c: char| !c.is_alphanumeric(), "_")
            );
            if let Some(rl) = item.get("rate_limit") {
                push_limit(&mut windows, rl, Some((&slug, name)), th, now);
            }
        }
    }

    let mut details = Vec::new();
    if let Some(c) = obj.get("credits") {
        if c.get("unlimited") == Some(&Value::Bool(true)) {
            details.push(Detail {
                id: "credits".into(),
                label: "Credits".into(),
                value: "unlimited".into(),
            });
        } else if c.get("has_credits") == Some(&Value::Bool(true))
            && let Some(b) = c.get("balance").and_then(f64_of)
        {
            details.push(Detail {
                id: "credits".into(),
                label: "Credits".into(),
                value: format!("{b:.2}"),
            });
        }
    }
    Ok(ParsedUsage {
        plan: obj.get("plan_type").and_then(str_of).map(str::to_string),
        windows,
        details,
    })
}

/// `{primary_window, secondary_window}` içeren bir limit nesnesini ekler.
/// `prefix` verilirse kimlik `prefix:<süre>` ve etiket `Ad · <süre>` olur.
fn push_limit(
    out: &mut Vec<UsageWindow>,
    rl: &Value,
    prefix: Option<(&str, &str)>,
    th: &Thresholds,
    now: DateTime<Utc>,
) {
    for slot in ["primary_window", "secondary_window"] {
        let Some(w) = rl.get(slot).filter(|w| w.is_object()) else {
            continue;
        };
        let Some(used) = w.get("used_percent").and_then(f64_of) else {
            continue;
        };
        let secs = w.get("limit_window_seconds").and_then(u64_of);
        let resets_at = w.get("reset_at").and_then(time_of).or_else(|| {
            w.get("reset_after_seconds")
                .and_then(u64_of)
                .map(|s| now + Duration::seconds(s as i64))
        });
        let fallback = slot.trim_end_matches("_window");
        let (base_id, base_label) = window_identity(secs, fallback);
        let (id, label) = match prefix {
            Some((pid, pname)) => (format!("{pid}:{base_id}"), format!("{pname} · {base_label}")),
            None => (base_id, base_label),
        };
        if out.iter().any(|x| x.id == id) {
            continue;
        }
        out.push(UsageWindow::new(id, label, used, resets_at, secs, th));
    }
}

/// Yerel logdaki son limit durumundan pencereler (API'ye ulaşılamazsa).
pub fn windows_from_sample(sample: &RateSample, th: &Thresholds) -> Vec<UsageWindow> {
    sample
        .windows
        .iter()
        .map(|w| {
            let secs = w.window_minutes.map(|m| m * 60);
            let (id, label) = window_identity(secs, w.slot);
            UsageWindow::new(id, label, w.used_percent, w.resets_at, secs, th)
        })
        .collect()
}

/// Bilinmeyen plan adlarını okunur yapar.
pub fn plan_label(plan: &str) -> String {
    super::super::common::pretty_plan(plan, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Severity;
    use chrono::TimeZone;

    fn th() -> Thresholds {
        Thresholds::default()
    }

    #[test]
    fn real_free_plan_response() {
        let v: Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/codex_usage_free_2026-11-06.json")).unwrap();
        let p = parse(&v, &th(), Utc::now()).unwrap();
        assert_eq!(p.plan.as_deref(), Some("free"));
        assert_eq!(p.windows.len(), 1);
        let w = &p.windows[0];
        assert_eq!(w.id, "monthly");
        assert_eq!(w.used_percent, 100.0);
        assert_eq!(w.severity, Severity::Critical);
        assert_eq!(w.resets_at, Some(Utc.timestamp_opt(1_796_328_666, 0).unwrap()));
        assert!(p.details.is_empty());
    }

    #[test]
    fn plus_plan_with_extras() {
        let v: Value = serde_json::from_str(include_str!("../../../tests/fixtures/codex_usage_plus.json")).unwrap();
        let p = parse(&v, &th(), Utc::now()).unwrap();
        let ids: Vec<_> = p.windows.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "session",
                "weekly",
                "code_review:weekly",
                "extra:gpt_5_3_codex_spark:session"
            ]
        );
        assert_eq!(p.windows[0].used_percent, 42.0);
        assert_eq!(p.windows[2].label, "Code review · Weekly");
        assert_eq!(p.windows[3].label, "GPT-5.3-Codex-Spark · 5-hour session");
        assert_eq!(p.details[0].value, "4.50");
    }

    #[test]
    fn relative_reset_and_missing_rate_limit() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let v = serde_json::json!({"rate_limit": {"primary_window": {"used_percent": 1, "limit_window_seconds": 18000, "reset_after_seconds": 60}, "secondary_window": null}});
        let p = parse(&v, &th(), now).unwrap();
        assert_eq!(p.windows[0].resets_at, Some(now + Duration::seconds(60)));
        assert!(parse(&serde_json::json!({"detail": "Unauthorized"}), &th(), now).is_err());
    }
}
