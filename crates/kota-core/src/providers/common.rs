//! Sağlayıcılar arasında paylaşılan küçük yardımcılar.

use std::sync::Mutex;

use chrono::{DateTime, Utc};

use crate::model::{ProviderReport, Status};

/// Hız sınırına (HTTP 429) takılınca API'yi bir süre rahat bırakmak için.
#[derive(Debug, Default)]
pub(crate) struct Backoff(Mutex<Option<DateTime<Utc>>>);

impl Backoff {
    pub fn active_until(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let guard = self.0.lock().unwrap_or_else(|e| e.into_inner());
        guard.filter(|until| *until > now)
    }

    pub fn set(&self, until: DateTime<Utc>) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(until);
    }

    pub fn clear(&self) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// Bir sorgunun başarısızlık nedeni.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Failure {
    pub status: Status,
    pub code: &'static str,
    pub message: String,
}

impl Failure {
    pub fn error(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: Status::Error,
            code,
            message: message.into(),
        }
    }

    pub fn expired(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: Status::Expired,
            code,
            message: message.into(),
        }
    }

    pub fn apply(&self, report: &mut ProviderReport) {
        report.set_status(self.status, self.code, self.message.clone());
    }
}

/// Pencere süresinden kararlı kimlik ve İngilizce etiket üretir.
pub fn window_identity(window_seconds: Option<u64>, fallback_id: &str) -> (String, String) {
    match window_seconds {
        Some(18_000) => ("session".into(), "5-hour session".into()),
        Some(86_400) => ("daily".into(), "Daily".into()),
        Some(604_800) => ("weekly".into(), "Weekly".into()),
        Some(2_592_000) => ("monthly".into(), "30-day".into()),
        Some(s) if s > 0 && s % 86_400 == 0 => (format!("days_{}", s / 86_400), format!("{}-day", s / 86_400)),
        Some(s) if s > 0 && s % 3_600 == 0 => (format!("hours_{}", s / 3_600), format!("{}-hour", s / 3_600)),
        _ => (fallback_id.to_string(), prettify(fallback_id)),
    }
}

/// `seven_day_opus` → `Seven day opus`
pub fn prettify(key: &str) -> String {
    let words = key.replace(['_', '-'], " ");
    let mut chars = words.trim().chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// `pro` → `Pro`, `max` + `default_claude_max_20x` → `Max 20x`.
pub fn pretty_plan(plan: &str, tier: Option<&str>) -> String {
    let p = plan.trim().to_lowercase();
    let base = match p.as_str() {
        "pro" => "Pro".to_string(),
        "max" => "Max".to_string(),
        "plus" => "Plus".to_string(),
        "free" => "Free".to_string(),
        "team" => "Team".to_string(),
        "business" => "Business".to_string(),
        "enterprise" => "Enterprise".to_string(),
        "edu" => "Edu".to_string(),
        other => prettify(other),
    };
    if let Some(tier) = tier {
        for mult in ["20x", "5x"] {
            if tier.contains(mult) && !base.contains(mult) {
                return format!("{base} {mult}");
            }
        }
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities() {
        assert_eq!(window_identity(Some(18_000), "primary").0, "session");
        assert_eq!(window_identity(Some(2_592_000), "primary").0, "monthly");
        assert_eq!(window_identity(Some(3 * 86_400), "x").1, "3-day");
        assert_eq!(window_identity(None, "code_review").1, "Code review");
    }

    #[test]
    fn plans() {
        assert_eq!(pretty_plan("pro", Some("default_claude_ai")), "Pro");
        assert_eq!(pretty_plan("max", Some("default_claude_max_20x")), "Max 20x");
        assert_eq!(pretty_plan("free", None), "Free");
    }

    #[test]
    fn backoff_expires() {
        let b = Backoff::default();
        let now = Utc::now();
        assert!(b.active_until(now).is_none());
        b.set(now + chrono::Duration::seconds(10));
        assert!(b.active_until(now).is_some());
        assert!(b.active_until(now + chrono::Duration::seconds(11)).is_none());
        b.clear();
        assert!(b.active_until(now).is_none());
    }
}
