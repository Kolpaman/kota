//! `GET api.anthropic.com/api/oauth/usage` yanıtının toleranslı ayrıştırılması.
//!
//! İki şema biliniyor (bkz. `docs/kesif.md`):
//! - Yeni (2026): `limits: [{kind, group, percent, resets_at, scope, …}]`
//! - Eski: `five_hour`, `seven_day`, `seven_day_opus`, … → `{utilization, resets_at}`
//!
//! Yeni şema varsa o kullanılır; yoksa eski anahtarlar ve `utilization`
//! taşıyan bilinmeyen her nesne pencere sayılır.

use serde_json::{Map, Value};

use super::super::common::prettify;
use crate::config::Thresholds;
use crate::jsonx::{f64_of, str_of, time_of};
use crate::model::{Detail, UsageWindow};

const SESSION_SECS: u64 = 5 * 3600;
const WEEK_SECS: u64 = 7 * 86_400;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedUsage {
    pub windows: Vec<UsageWindow>,
    pub details: Vec<Detail>,
}

/// (eski anahtar, kimlik, etiket, süre)
const LEGACY: &[(&str, &str, &str, Option<u64>)] = &[
    ("five_hour", "session", "5-hour session", Some(SESSION_SECS)),
    ("seven_day", "weekly", "Weekly (all models)", Some(WEEK_SECS)),
    ("seven_day_opus", "weekly_opus", "Weekly · Opus", Some(WEEK_SECS)),
    ("seven_day_sonnet", "weekly_sonnet", "Weekly · Sonnet", Some(WEEK_SECS)),
    (
        "seven_day_oauth_apps",
        "weekly_oauth_apps",
        "Weekly · OAuth apps",
        Some(WEEK_SECS),
    ),
    ("seven_day_cowork", "weekly_cowork", "Weekly · Cowork", Some(WEEK_SECS)),
];

/// Pencere olmayan, bilinen üst düzey nesneler.
const NOT_WINDOWS: &[&str] = &["extra_usage", "spend", "seven_day_breakdown", "limits", "pace"];

pub fn parse(v: &Value, th: &Thresholds) -> Result<ParsedUsage, String> {
    let obj = v.as_object().ok_or("usage response is not a JSON object")?;
    let mut windows = Vec::new();

    if let Some(limits) = obj.get("limits").and_then(Value::as_array) {
        windows.extend(limits.iter().filter_map(|item| window_from_limit(item, th)));
    }
    if windows.is_empty() {
        for (key, id, label, secs) in LEGACY {
            if let Some(w) = obj.get(*key).and_then(|x| legacy_window(x, id, label, *secs, th)) {
                windows.push(w);
            }
        }
        for (key, val) in obj {
            if LEGACY.iter().any(|l| l.0 == key) || NOT_WINDOWS.contains(&key.as_str()) {
                continue;
            }
            let secs = key.starts_with("seven_day").then_some(WEEK_SECS);
            if let Some(w) = legacy_window(val, key, &prettify(key), secs, th) {
                windows.push(w);
            }
        }
    }
    dedupe_ids(&mut windows);

    let known_shape = obj.contains_key("limits") || LEGACY.iter().any(|l| obj.contains_key(l.0));
    if windows.is_empty() && !known_shape {
        return Err("no usage windows found in response".into());
    }
    Ok(ParsedUsage {
        windows,
        details: details(obj),
    })
}

fn window_from_limit(item: &Value, th: &Thresholds) -> Option<UsageWindow> {
    let pct = item
        .get("percent")
        .and_then(f64_of)
        .or_else(|| item.get("utilization").and_then(f64_of))?;
    let kind = item
        .get("kind")
        .and_then(str_of)
        .or_else(|| item.get("group").and_then(str_of))
        .unwrap_or("limit");
    let (mut id, mut label, secs) = match kind {
        "session" | "five_hour" => ("session".to_string(), "5-hour session".to_string(), Some(SESSION_SECS)),
        "weekly_all" | "weekly" | "seven_day" => {
            ("weekly".to_string(), "Weekly (all models)".to_string(), Some(WEEK_SECS))
        }
        "weekly_opus" => ("weekly_opus".to_string(), "Weekly · Opus".to_string(), Some(WEEK_SECS)),
        "weekly_sonnet" => (
            "weekly_sonnet".to_string(),
            "Weekly · Sonnet".to_string(),
            Some(WEEK_SECS),
        ),
        k if k.starts_with("weekly") => (k.to_string(), prettify(k), Some(WEEK_SECS)),
        k => (k.to_string(), prettify(k), None),
    };
    if let Some(scope) = scope_name(item.get("scope")) {
        id = format!("{id}:{}", scope.to_lowercase().replace(char::is_whitespace, "_"));
        label = format!("{label} · {scope}");
    }
    let resets_at = item.get("resets_at").and_then(time_of);
    Some(UsageWindow::new(id, label, pct, resets_at, secs, th))
}

fn scope_name(v: Option<&Value>) -> Option<String> {
    let v = v?;
    if let Some(s) = str_of(v) {
        return Some(s.to_string());
    }
    ["display_name", "name", "model"]
        .iter()
        .find_map(|k| v.get(*k).and_then(str_of))
        .map(str::to_string)
}

fn legacy_window(v: &Value, id: &str, label: &str, secs: Option<u64>, th: &Thresholds) -> Option<UsageWindow> {
    let pct = v.get("utilization").and_then(f64_of)?;
    let resets_at = v.get("resets_at").and_then(time_of);
    Some(UsageWindow::new(id, label, pct, resets_at, secs, th))
}

fn dedupe_ids(windows: &mut [UsageWindow]) {
    let mut seen = std::collections::HashMap::<String, usize>::new();
    for w in windows.iter_mut() {
        let n = seen.entry(w.id.clone()).or_insert(0);
        *n += 1;
        if *n > 1 {
            w.id = format!("{}#{}", w.id, n);
        }
    }
}

fn details(obj: &Map<String, Value>) -> Vec<Detail> {
    let mut out = Vec::new();
    if let Some(extra) = obj
        .get("extra_usage")
        .filter(|e| e.get("is_enabled") == Some(&Value::Bool(true)))
    {
        let value = match extra.get("utilization").and_then(f64_of) {
            Some(u) => format!("{u:.0}%"),
            None => "on".to_string(),
        };
        out.push(Detail {
            id: "extra_usage".into(),
            label: "Extra usage".into(),
            value,
        });
    }
    if let Some(rows) = obj
        .get("seven_day_breakdown")
        .and_then(|b| b.get("rows"))
        .and_then(Value::as_array)
    {
        for row in rows {
            let (Some(name), Some(pct)) = (
                row.get("display_name").and_then(str_of),
                row.get("percent").and_then(f64_of),
            ) else {
                continue;
            };
            if pct <= 0.0 {
                continue;
            }
            let key = row.get("key").and_then(str_of).unwrap_or(name);
            out.push(Detail {
                id: format!("weekly_share:{key}"),
                label: name.to_string(),
                value: format!("{pct:.0}%"),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Severity;
    use chrono::{TimeZone, Utc};

    fn th() -> Thresholds {
        Thresholds::default()
    }

    #[test]
    fn real_2026_11_response() {
        let v: Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/claude_usage_2026-11-06.json")).unwrap();
        let p = parse(&v, &th()).unwrap();
        assert_eq!(p.windows.len(), 2);
        let s = &p.windows[0];
        assert_eq!(s.id, "session");
        assert_eq!(s.used_percent, 21.0);
        assert_eq!(s.window_seconds, Some(18_000));
        assert_eq!(
            s.resets_at,
            Some(Utc.with_ymd_and_hms(2026, 11, 6, 20, 19, 59).unwrap() + chrono::Duration::microseconds(813_876))
        );
        let w = &p.windows[1];
        assert_eq!(w.id, "weekly");
        assert_eq!(w.used_percent, 3.0);
        // Haftalık dağılım satırları (sıfır olanlar hariç)
        let shares: Vec<_> = p.details.iter().map(|d| (d.label.as_str(), d.value.as_str())).collect();
        assert_eq!(shares, [("Claude Code", "53%"), ("Cowork", "47%")]);
    }

    #[test]
    fn legacy_response_without_limits() {
        let v: Value = serde_json::from_str(include_str!("../../../tests/fixtures/claude_usage_legacy.json")).unwrap();
        let p = parse(&v, &th()).unwrap();
        let ids: Vec<_> = p.windows.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["session", "weekly", "weekly_opus", "seven_day_future_model"]);
        assert_eq!(p.windows[0].severity, Severity::Critical);
        assert_eq!(p.windows[2].resets_at, None);
        assert_eq!(p.windows[3].window_seconds, Some(WEEK_SECS));
        assert_eq!(p.details[0].id, "extra_usage");
        assert_eq!(p.details[0].value, "12%");
    }

    #[test]
    fn scoped_limit_and_unknown_kind() {
        let v = serde_json::json!({"limits": [
            {"kind": "weekly_scoped", "percent": 40, "scope": {"display_name": "Opus 5"}, "resets_at": null},
            {"kind": "daily_burst", "percent": "5"},
            {"kind": "session", "percent": null}
        ]});
        let p = parse(&v, &th()).unwrap();
        assert_eq!(p.windows.len(), 2);
        assert_eq!(p.windows[0].id, "weekly_scoped:opus_5");
        assert_eq!(p.windows[0].label, "Weekly scoped · Opus 5");
        assert_eq!(p.windows[1].id, "daily_burst");
        assert_eq!(p.windows[1].window_seconds, None);
    }

    #[test]
    fn rejects_unrelated_json() {
        assert!(parse(&serde_json::json!({"error": "nope"}), &th()).is_err());
        assert!(parse(&serde_json::json!([1, 2]), &th()).is_err());
        // Bilinen şekil ama boş pencereler: hata değil (ör. yeni hesap).
        assert!(
            parse(&serde_json::json!({"five_hour": null, "seven_day": null}), &th())
                .unwrap()
                .windows
                .is_empty()
        );
    }
}
