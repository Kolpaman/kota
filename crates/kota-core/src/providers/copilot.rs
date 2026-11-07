//! GitHub Copilot: `GET api.github.com/copilot_internal/user` (belgelenmemiş;
//! Copilot eklentilerinin kullandığı uç nokta). Aylık "premium istek" ve
//! (ücretsiz planda) sohbet/tamamlama kotaları.
//!
//! Token sırası: anahtarlıktaki GitHub token'ı → Copilot eklentilerinin
//! giriş dosyaları (`~/.config/github-copilot/{apps,hosts}.json`).

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;

use super::common::{Failure, pretty_plan};
use super::{FetchContext, KeyStore, Provider};
use crate::config::Thresholds;
use crate::http::{self, FetchError};
use crate::jsonx::{f64_of, str_of, time_of};
use crate::model::{Detail, ProviderReport, Source, Status, UsageWindow};
use crate::secret::Secret;

pub const API_URL: &str = "https://api.github.com/copilot_internal/user";

pub struct CopilotProvider {
    keys: Arc<dyn KeyStore>,
    config_dir: PathBuf,
    url: String,
}

impl CopilotProvider {
    pub fn from_default(keys: Arc<dyn KeyStore>) -> Self {
        let dir = dirs::config_dir()
            .unwrap_or_else(|| crate::paths::home_dir().join(".config"))
            .join("github-copilot");
        Self::new(keys, dir)
    }

    pub fn new(keys: Arc<dyn KeyStore>, config_dir: PathBuf) -> Self {
        Self {
            keys,
            config_dir,
            url: API_URL.to_string(),
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    fn token(&self) -> Option<Secret> {
        if let Some(k) = self.keys.get("copilot") {
            return Some(k);
        }
        for file in ["apps.json", "hosts.json"] {
            let Ok(text) = std::fs::read_to_string(self.config_dir.join(file)) else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            let found = v.as_object().into_iter().flatten().find_map(|(host, entry)| {
                host.starts_with("github.com")
                    .then(|| entry.get("oauth_token").and_then(str_of))
                    .flatten()
            });
            if let Some(t) = found {
                return Some(Secret::new(t));
            }
        }
        None
    }
}

#[async_trait]
impl Provider for CopilotProvider {
    fn id(&self) -> &'static str {
        "copilot"
    }

    fn name(&self) -> &'static str {
        "GitHub Copilot"
    }

    fn watched_files(&self) -> Vec<PathBuf> {
        vec![self.config_dir.join("apps.json"), self.config_dir.join("hosts.json")]
    }

    async fn fetch(&self, ctx: &FetchContext) -> ProviderReport {
        let mut report = ProviderReport::new(self.id(), self.name());
        let Some(token) = self.token() else {
            report.set_status(
                Status::NotConnected,
                "no_api_key",
                "No GitHub token; add one in Kota settings",
            );
            return report;
        };
        let req = ctx
            .http
            .get(&self.url)
            .header(reqwest::header::AUTHORIZATION, format!("token {}", token.expose()))
            .header(reqwest::header::ACCEPT, "application/json")
            .header("X-GitHub-Api-Version", "2025-04-01")
            .header("Editor-Version", "vscode/1.105.0")
            .header("Editor-Plugin-Version", "copilot-chat/0.32.0");
        match http::send_json(req).await {
            Ok(v) => {
                let (plan, windows, details) = parse(&v, &ctx.thresholds, ctx.now);
                report.plan = plan;
                report.windows = windows;
                report.details = details;
                report.source = Some(Source::Api);
                report.fetched_at = Some(ctx.now);
            }
            Err(FetchError::Unauthorized(code)) => {
                Failure::expired("key_rejected", format!("GitHub rejected the token (HTTP {code})")).apply(&mut report)
            }
            Err(e) => Failure::error(e.code(), e.to_string()).apply(&mut report),
        }
        report
    }
}

fn reset_of(v: &Value) -> Option<DateTime<Utc>> {
    for k in ["quota_reset_date_utc", "quota_reset_date", "limited_user_reset_date"] {
        let Some(x) = v.get(k) else { continue };
        if let Some(t) = time_of(x) {
            return Some(t);
        }
        if let Some(d) = str_of(x).and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()) {
            return d.and_hms_opt(0, 0, 0).map(|n| n.and_utc());
        }
    }
    None
}

pub fn parse(v: &Value, th: &Thresholds, _now: DateTime<Utc>) -> (Option<String>, Vec<UsageWindow>, Vec<Detail>) {
    let plan = v.get("copilot_plan").and_then(str_of).map(|p| pretty_plan(p, None));
    let reset = reset_of(v);
    let mut windows = Vec::new();
    let mut details = Vec::new();
    let labels = [
        ("premium_interactions", "Premium requests"),
        ("chat", "Chat"),
        ("completions", "Completions"),
    ];
    if let Some(snaps) = v.get("quota_snapshots") {
        for (key, label) in labels {
            let Some(s) = snaps.get(key) else { continue };
            if s.get("unlimited") == Some(&Value::Bool(true)) {
                continue;
            }
            let Some(rem) = s.get("percent_remaining").and_then(f64_of) else {
                continue;
            };
            windows.push(UsageWindow::new(
                key,
                label,
                (100.0 - rem).clamp(0.0, 100.0),
                reset,
                None,
                th,
            ));
            if let (Some(ent), Some(left)) = (
                s.get("entitlement").and_then(f64_of),
                s.get("remaining").and_then(f64_of),
            ) {
                details.push(Detail {
                    id: format!("{key}_left"),
                    label: label.into(),
                    value: format!("{left:.0}/{ent:.0}"),
                });
            }
        }
    }
    // Eski ücretsiz plan biçimi.
    if windows.is_empty()
        && let (Some(rem), Some(monthly)) = (v.get("limited_user_quotas"), v.get("monthly_quotas"))
    {
        for (key, label) in &labels[1..] {
            let (Some(left), Some(total)) = (rem.get(*key).and_then(f64_of), monthly.get(*key).and_then(f64_of)) else {
                continue;
            };
            if total > 0.0 {
                windows.push(UsageWindow::new(
                    *key,
                    *label,
                    (total - left) / total * 100.0,
                    reset,
                    None,
                    th,
                ));
            }
        }
    }
    (plan, windows, details)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::StaticKeys;
    use crate::testutil::{FakeServer, ctx};
    use std::collections::HashMap;

    #[test]
    fn parses_snapshots_and_legacy() {
        let v: Value = serde_json::from_str(r#"{"copilot_plan":"individual","quota_reset_date":"2026-12-01","quota_snapshots":{"chat":{"unlimited":true,"percent_remaining":100},"completions":{"unlimited":true},"premium_interactions":{"entitlement":300,"remaining":87,"percent_remaining":29.0,"unlimited":false}}}"#).unwrap();
        let (plan, w, d) = parse(&v, &Thresholds::default(), Utc::now());
        assert_eq!(plan.as_deref(), Some("Individual"));
        assert_eq!(w.len(), 1);
        assert_eq!((w[0].id.as_str(), w[0].used_percent), ("premium_interactions", 71.0));
        assert_eq!(w[0].resets_at.unwrap().to_rfc3339(), "2026-12-01T00:00:00+00:00");
        assert_eq!(d[0].value, "87/300");

        let legacy: Value = serde_json::from_str(r#"{"copilot_plan":"free","limited_user_reset_date":"2026-11-20","limited_user_quotas":{"chat":10,"completions":1500},"monthly_quotas":{"chat":50,"completions":2000}}"#).unwrap();
        let (_, w, _) = parse(&legacy, &Thresholds::default(), Utc::now());
        assert_eq!(w.iter().map(|x| x.used_percent).collect::<Vec<_>>(), [80.0, 25.0]);
    }

    #[tokio::test]
    async fn token_from_editor_login_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("apps.json"),
            r#"{"github.com:Iv1.b507a08c87ecfe98":{"user":"u","oauth_token":"gho_file"}}"#,
        )
        .unwrap();
        let srv = FakeServer::start(|r| {
            if r.header("authorization") == Some("token gho_file") {
                (
                    200,
                    r#"{"copilot_plan":"free","quota_snapshots":{"premium_interactions":{"percent_remaining":50}}}"#
                        .into(),
                )
            } else {
                (401, r#"{"message":"Bad credentials"}"#.into())
            }
        });
        let keys = Arc::new(StaticKeys(HashMap::new()));
        let p = CopilotProvider::new(keys, dir.path().to_path_buf()).with_url(srv.url("/u"));
        let r = p.fetch(&ctx(Utc::now())).await;
        assert_eq!(r.status, Status::Ok, "{:?}", r.message);
        assert_eq!(r.windows[0].used_percent, 50.0);
    }
}
