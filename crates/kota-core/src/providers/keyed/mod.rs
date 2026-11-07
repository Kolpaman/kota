//! API anahtarıyla çalışan sağlayıcılar. Her sağlayıcı tek dosya:
//! `fetch(http, base, key, ctx) -> Result<Outcome, FetchError>`.
//!
//! Anahtar [`KeyStore`]'dan (GNOME Anahtarlığı / ortam değişkeni) okunur;
//! asla config dosyasına yazılmaz.

mod anthropic_admin;
mod deepseek;
mod moonshot;
mod openai_admin;
mod openrouter;
mod siliconflow;
mod zai;

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use serde_json::Value;

use super::common::{Backoff, Failure};
use super::{FetchContext, KeyStore, Provider};
use crate::http::{self, FetchError};
use crate::model::{Balance, Detail, ProviderReport, Source, Status, UsageWindow};
use crate::secret::Secret;

const DEFAULT_BACKOFF_SECS: i64 = 300;

/// Bir sorgunun sonucu.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Outcome {
    pub plan: Option<String>,
    pub windows: Vec<UsageWindow>,
    pub balance: Option<Balance>,
    pub details: Vec<Detail>,
}

struct Spec {
    id: &'static str,
    name: &'static str,
    base: &'static str,
}

const SPECS: &[Spec] = &[
    Spec {
        id: "deepseek",
        name: "DeepSeek",
        base: "https://api.deepseek.com",
    },
    Spec {
        id: "openrouter",
        name: "OpenRouter",
        base: "https://openrouter.ai",
    },
    Spec {
        id: "moonshot",
        name: "Kimi (Moonshot)",
        base: "https://api.moonshot.ai",
    },
    Spec {
        id: "siliconflow",
        name: "SiliconFlow",
        base: "https://api.siliconflow.cn",
    },
    Spec {
        id: "zai",
        name: "Z.ai (GLM)",
        base: "https://api.z.ai",
    },
    Spec {
        id: "openai",
        name: "OpenAI API",
        base: "https://api.openai.com",
    },
    Spec {
        id: "anthropic",
        name: "Anthropic API",
        base: "https://api.anthropic.com",
    },
];

pub struct KeyedProvider {
    spec: &'static Spec,
    base: String,
    keys: Arc<dyn KeyStore>,
    budget: Option<f64>,
    backoff: Backoff,
}

impl KeyedProvider {
    pub fn new(id: &str, keys: Arc<dyn KeyStore>, budget: Option<f64>) -> Option<Self> {
        let spec = SPECS.iter().find(|s| s.id == id)?;
        Some(Self {
            spec,
            base: spec.base.to_string(),
            keys,
            budget,
            backoff: Backoff::default(),
        })
    }

    /// Testler için: sahte sunucu.
    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        self.base = base.into();
        self
    }

    async fn query(&self, ctx: &FetchContext, key: &Secret) -> Result<Outcome, FetchError> {
        let (http, base) = (&ctx.http, self.base.as_str());
        match self.spec.id {
            "deepseek" => deepseek::fetch(http, base, key).await,
            "openrouter" => openrouter::fetch(http, base, key).await,
            "moonshot" => moonshot::fetch(http, base, key).await,
            "siliconflow" => siliconflow::fetch(http, base, key).await,
            "zai" => zai::fetch(http, base, key, ctx).await,
            "openai" => openai_admin::fetch(http, base, key, ctx.now).await,
            "anthropic" => anthropic_admin::fetch(http, base, key, ctx.now).await,
            other => Err(FetchError::Parse(format!("unknown provider {other}"))),
        }
    }
}

#[async_trait]
impl Provider for KeyedProvider {
    fn id(&self) -> &'static str {
        self.spec.id
    }

    fn name(&self) -> &'static str {
        self.spec.name
    }

    async fn fetch(&self, ctx: &FetchContext) -> ProviderReport {
        let mut report = ProviderReport::new(self.spec.id, self.spec.name);
        let Some(key) = self.keys.get(self.spec.id) else {
            report.set_status(
                Status::NotConnected,
                "no_api_key",
                "No API key; add one in Kota settings",
            );
            return report;
        };
        if let Some(until) = self.backoff.active_until(ctx.now) {
            Failure::error(
                "rate_limited",
                format!("Rate limited; next attempt after {}", until.format("%H:%M UTC")),
            )
            .apply(&mut report);
            return report;
        }
        match self.query(ctx, &key).await {
            Ok(mut out) => {
                self.backoff.clear();
                if let (Some(b), Some(budget)) = (out.balance.as_mut(), self.budget)
                    && b.kind == "spend"
                {
                    b.budget = Some(budget);
                    out.windows.insert(0, budget_window(b.amount, budget, ctx));
                }
                report.plan = out.plan;
                report.windows = out.windows;
                report.balance = out.balance;
                report.details = out.details;
                report.source = Some(Source::Api);
                report.fetched_at = Some(ctx.now);
            }
            Err(FetchError::Unauthorized(code)) => {
                Failure::expired("key_rejected", format!("The API key was rejected (HTTP {code})")).apply(&mut report)
            }
            Err(FetchError::RateLimited { retry_after_secs }) => {
                let wait = retry_after_secs
                    .map(|s| s as i64)
                    .unwrap_or(DEFAULT_BACKOFF_SECS)
                    .clamp(60, 3600);
                self.backoff.set(ctx.now + Duration::seconds(wait));
                Failure::error("rate_limited", format!("Rate limited; retrying in {wait}s")).apply(&mut report);
            }
            Err(e) => Failure::error(e.code(), e.to_string()).apply(&mut report),
        }
        report
    }
}

/// Aylık bütçe penceresi (ay başında sıfırlanır, UTC).
fn budget_window(spend: f64, budget: f64, ctx: &FetchContext) -> UsageWindow {
    let pct = if budget > 0.0 { spend / budget * 100.0 } else { 0.0 };
    UsageWindow::new(
        "budget",
        "Monthly budget",
        pct,
        Some(next_month_start(ctx.now)),
        None,
        &ctx.thresholds,
    )
}

pub(crate) fn month_start(now: DateTime<Utc>) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()
        .unwrap_or(now)
}

pub(crate) fn next_month_start(now: DateTime<Utc>) -> DateTime<Utc> {
    let (y, m) = if now.month() == 12 {
        (now.year() + 1, 1)
    } else {
        (now.year(), now.month() + 1)
    };
    Utc.with_ymd_and_hms(y, m, 1, 0, 0, 0).single().unwrap_or(now)
}

/// Bearer anahtarlı GET.
pub(crate) async fn get_bearer(http: &reqwest::Client, url: &str, key: &Secret) -> Result<Value, FetchError> {
    http::send_json(
        http.get(url)
            .bearer_auth(key.expose())
            .header(reqwest::header::ACCEPT, "application/json"),
    )
    .await
}

pub(crate) fn balance(kind: &str, amount: f64, currency: &str) -> Balance {
    Balance {
        kind: kind.to_string(),
        amount,
        currency: currency.to_string(),
        budget: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::StaticKeys;
    use crate::testutil::{FakeServer, ctx};
    use std::collections::HashMap;

    fn keys(id: &str) -> Arc<dyn KeyStore> {
        Arc::new(StaticKeys(HashMap::from([(id.to_string(), "sk-test".to_string())])))
    }

    #[tokio::test]
    async fn missing_key_is_not_connected() {
        let p = KeyedProvider::new("deepseek", Arc::new(StaticKeys(HashMap::new())), None).unwrap();
        let r = p.fetch(&ctx(Utc::now())).await;
        assert_eq!(r.status, Status::NotConnected);
        assert_eq!(r.code.as_deref(), Some("no_api_key"));
    }

    #[tokio::test]
    async fn deepseek_end_to_end_and_rejected_key() {
        let srv = FakeServer::start(|r| {
            if r.header("authorization") == Some("Bearer sk-test") {
                (200, r#"{"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":"110.00","granted_balance":"10.00","topped_up_balance":"100.00"}]}"#.into())
            } else {
                (401, r#"{"error":{"message":"Authentication Fails"}}"#.into())
            }
        });
        let p = KeyedProvider::new("deepseek", keys("deepseek"), None)
            .unwrap()
            .with_base(srv.url(""));
        let r = p.fetch(&ctx(Utc::now())).await;
        assert_eq!(r.status, Status::Ok, "{:?}", r.message);
        let b = r.balance.unwrap();
        assert_eq!(
            (b.kind.as_str(), b.amount, b.currency.as_str()),
            ("balance", 110.0, "CNY")
        );
        assert_eq!(srv.requests()[0].path, "/user/balance");

        let bad = Arc::new(StaticKeys(HashMap::from([(
            "deepseek".to_string(),
            "nope".to_string(),
        )])));
        let p = KeyedProvider::new("deepseek", bad, None)
            .unwrap()
            .with_base(srv.url(""));
        let r = p.fetch(&ctx(Utc::now())).await;
        assert_eq!(r.status, Status::Expired);
        assert_eq!(r.code.as_deref(), Some("key_rejected"));
    }

    #[tokio::test]
    async fn spend_with_budget_becomes_window() {
        let srv = FakeServer::start(|_| {
            (200, r#"{"object":"page","data":[{"results":[{"amount":{"value":12.5,"currency":"usd"}}]},{"results":[{"amount":{"value":7.5,"currency":"usd"}}]}],"has_more":false}"#.into())
        });
        let p = KeyedProvider::new("openai", keys("openai"), Some(40.0))
            .unwrap()
            .with_base(srv.url(""));
        let now = Utc.with_ymd_and_hms(2026, 11, 7, 12, 0, 0).unwrap();
        let r = p.fetch(&ctx(now)).await;
        assert_eq!(r.status, Status::Ok, "{:?}", r.message);
        assert_eq!(r.balance.as_ref().unwrap().amount, 20.0);
        assert_eq!(r.balance.as_ref().unwrap().budget, Some(40.0));
        let w = &r.windows[0];
        assert_eq!((w.id.as_str(), w.used_percent), ("budget", 50.0));
        assert_eq!(w.resets_at, Some(Utc.with_ymd_and_hms(2026, 12, 1, 0, 0, 0).unwrap()));
        let path = &srv.requests()[0].path;
        assert!(
            path.starts_with("/v1/organization/costs?start_time=1793491200"),
            "{path}"
        );
    }

    #[test]
    fn month_math() {
        let dec = Utc.with_ymd_and_hms(2026, 12, 31, 23, 0, 0).unwrap();
        assert_eq!(
            next_month_start(dec),
            Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap()
        );
        assert_eq!(month_start(dec), Utc.with_ymd_and_hms(2026, 12, 1, 0, 0, 0).unwrap());
    }
}
