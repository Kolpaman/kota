//! Codex (ChatGPT aboneliğiyle giriş yapılmış Codex CLI / uygulaması).
//!
//! Kaynaklar:
//! 1. `GET chatgpt.com/backend-api/wham/usage`: `~/.codex/auth.json` içindeki
//!    ChatGPT erişim token'ı ve hesap kimliğiyle. Belgelenmemiş.
//! 2. Yerel loglar (`~/.codex/sessions`): bugünkü token'lar ve API'ye
//!    ulaşılamazsa son bilinen limitler.
//!
//! Token yenilenmez: Codex erişim token'ları ~10 gün geçerli ve Codex her
//! açılışta kendisi yeniler. Süre dolarsa "yeniden bağlan" gösterilir.

pub mod usage;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use base64::Engine as _;
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

use super::common::{Backoff, Failure};
use super::{FetchContext, Provider};
use crate::config::CodexConfig;
use crate::http::{self, FetchError};
use crate::jsonx;
use crate::locallogs::codex::{CodexLogScanner, RateSample};
use crate::model::{LocalUsage, ProviderReport, Source, Status};
use crate::paths;
use crate::secret::Secret;

pub const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
pub const LOGIN_COMMAND: &[&str] = &["codex", "login"];
const DEFAULT_BACKOFF_SECS: i64 = 300;

/// `auth.json` içinden okunanlar.
#[derive(Debug, Clone)]
pub struct CodexAuth {
    pub access_token: Secret,
    pub account_id: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub plan: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AuthError {
    NotFound,
    /// ChatGPT girişi yok; `api_key` true ise API anahtarı modunda.
    NoTokens {
        api_key: bool,
    },
    Invalid(String),
}

pub fn read_auth(path: &std::path::Path) -> Result<CodexAuth, AuthError> {
    let text = std::fs::read_to_string(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => AuthError::NotFound,
        _ => AuthError::Invalid(e.to_string()),
    })?;
    let v: Value = serde_json::from_str(&text).map_err(|e| AuthError::Invalid(format!("auth.json: {e}")))?;
    let api_key = v.get("OPENAI_API_KEY").and_then(jsonx::str_of).is_some();
    let tokens = v.get("tokens").filter(|t| t.is_object());
    let Some(access) = tokens.and_then(|t| t.get("access_token")).and_then(jsonx::str_of) else {
        return Err(AuthError::NoTokens { api_key });
    };
    let claims = jwt_claims(access);
    let id_claims = tokens
        .and_then(|t| t.get("id_token"))
        .and_then(jsonx::str_of)
        .and_then(jwt_claims);
    let plan = [claims.as_ref(), id_claims.as_ref()]
        .into_iter()
        .flatten()
        .find_map(|c| {
            c.get("https://api.openai.com/auth")?
                .get("chatgpt_plan_type")
                .and_then(jsonx::str_of)
        })
        .map(str::to_string);
    Ok(CodexAuth {
        access_token: Secret::new(access),
        account_id: tokens
            .and_then(|t| t.get("account_id"))
            .and_then(jsonx::str_of)
            .map(str::to_string),
        expires_at: claims.as_ref().and_then(|c| c.get("exp")).and_then(jsonx::time_of),
        plan,
    })
}

/// JWT gövdesini imza doğrulamadan çözer (yalnızca `exp` ve plan için).
fn jwt_claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD_INDIFFERENT
        .decode(payload)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub struct CodexProvider {
    home: PathBuf,
    usage_url: String,
    scanner: Option<Arc<Mutex<CodexLogScanner>>>,
    backoff: Backoff,
}

impl CodexProvider {
    pub fn from_config(c: &CodexConfig) -> Self {
        Self::new(c.home.clone().unwrap_or_else(paths::codex_home), c.local_logs)
    }

    pub fn new(home: PathBuf, local_logs: bool) -> Self {
        let scanner = local_logs.then(|| Arc::new(Mutex::new(CodexLogScanner::new(home.join("sessions")))));
        Self {
            home,
            usage_url: USAGE_URL.to_string(),
            scanner,
            backoff: Backoff::default(),
        }
    }

    pub fn with_endpoint(mut self, usage_url: impl Into<String>) -> Self {
        self.usage_url = usage_url.into();
        self
    }

    pub fn auth_path(&self) -> PathBuf {
        self.home.join("auth.json")
    }

    async fn local(&self, ctx: &FetchContext) -> (Option<LocalUsage>, Option<RateSample>) {
        let Some(scanner) = self.scanner.clone() else {
            return (None, None);
        };
        let (now, today) = (ctx.now, ctx.today_start);
        tokio::task::spawn_blocking(move || scanner.lock().unwrap_or_else(|e| e.into_inner()).summarize(now, today))
            .await
            .unwrap_or((None, None))
    }

    async fn fetch_usage(&self, ctx: &FetchContext, auth: &CodexAuth) -> Result<Value, Failure> {
        if auth.expires_at.is_some_and(|t| t <= ctx.now) {
            return Err(Failure::expired(
                "token_expired",
                "Codex session expired; open Codex once (it renews automatically) or run `codex login`",
            ));
        }
        if let Some(until) = self.backoff.active_until(ctx.now) {
            return Err(Failure::error(
                "rate_limited",
                format!("Rate limited; next attempt after {}", until.format("%H:%M UTC")),
            ));
        }
        let mut req = ctx
            .http
            .get(&self.usage_url)
            .bearer_auth(auth.access_token.expose())
            .header(reqwest::header::ACCEPT, "application/json");
        if let Some(acc) = &auth.account_id {
            req = req.header("ChatGPT-Account-Id", acc);
        }
        match http::send_json(req).await {
            Ok(v) => {
                self.backoff.clear();
                Ok(v)
            }
            Err(FetchError::Unauthorized(code)) => Err(Failure::expired(
                "token_rejected",
                format!("ChatGPT rejected the Codex token (HTTP {code}); run `codex login`"),
            )),
            Err(FetchError::RateLimited { retry_after_secs }) => {
                let wait = retry_after_secs
                    .map(|s| s as i64)
                    .unwrap_or(DEFAULT_BACKOFF_SECS)
                    .clamp(60, 3600);
                self.backoff.set(ctx.now + Duration::seconds(wait));
                Err(Failure::error(
                    "rate_limited",
                    format!("Rate limited by ChatGPT; retrying in {wait}s"),
                ))
            }
            Err(e) => Err(Failure::error(e.code(), e.to_string())),
        }
    }
}

#[async_trait]
impl Provider for CodexProvider {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn name(&self) -> &'static str {
        "Codex"
    }

    fn watched_files(&self) -> Vec<PathBuf> {
        vec![self.auth_path()]
    }

    async fn fetch(&self, ctx: &FetchContext) -> ProviderReport {
        let mut report = ProviderReport::new(self.id(), self.name());
        report.login_command = Some(LOGIN_COMMAND.iter().map(|s| s.to_string()).collect());
        let (local, sample) = self.local(ctx).await;
        report.local = local;

        match read_auth(&self.auth_path()) {
            Err(AuthError::NotFound | AuthError::NoTokens { api_key: false }) => {
                report.set_status(
                    Status::NotConnected,
                    "no_credentials",
                    "Codex is not signed in with ChatGPT",
                );
            }
            Err(AuthError::NoTokens { api_key: true }) => {
                report.set_status(
                    Status::Unsupported,
                    "api_key_mode",
                    "Codex uses an API key; plan limits exist only for ChatGPT sign-in",
                );
            }
            Err(AuthError::Invalid(e)) => report.set_status(Status::Error, "credentials_unreadable", e),
            Ok(auth) => {
                report.plan = auth.plan.as_deref().map(usage::plan_label);
                match self.fetch_usage(ctx, &auth).await {
                    Ok(v) => match usage::parse(&v, &ctx.thresholds, ctx.now) {
                        Ok(parsed) => {
                            if let Some(p) = parsed.plan.as_deref() {
                                report.plan = Some(usage::plan_label(p));
                            }
                            report.windows = parsed.windows;
                            report.details = parsed.details;
                            report.source = Some(Source::Api);
                            report.fetched_at = Some(ctx.now);
                        }
                        Err(e) => report.set_status(Status::Error, "parse_error", e),
                    },
                    Err(f) => f.apply(&mut report),
                }
            }
        }

        // API yoksa yerel logdaki son limitler (eski veri olarak işaretli).
        if report.status != Status::Ok
            && report.status != Status::NotConnected
            && report.windows.is_empty()
            && let Some(s) = sample
        {
            report.windows = usage::windows_from_sample(&s, &ctx.thresholds);
            report.source = Some(Source::LocalLogs);
            report.fetched_at = Some(s.at);
            report.stale = true;
            if report.plan.is_none() {
                report.plan = s.plan.as_deref().map(usage::plan_label);
            }
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{FakeServer, ctx};
    use serde_json::json;
    use std::fs;

    fn jwt(claims: Value) -> String {
        let enc = |v: &Value| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v.to_string());
        format!("{}.{}.sig", enc(&json!({"alg": "RS256"})), enc(&claims))
    }

    fn write_auth(home: &std::path::Path, exp: DateTime<Utc>) {
        let access = jwt(json!({"exp": exp.timestamp(), "https://api.openai.com/auth": {"chatgpt_plan_type": "plus"}}));
        let v = json!({"auth_mode": "chatgpt", "OPENAI_API_KEY": null, "tokens": {"access_token": access, "account_id": "acc-1", "refresh_token": "rt"}});
        fs::write(home.join("auth.json"), v.to_string()).unwrap();
    }

    fn server() -> FakeServer {
        FakeServer::start(|r| {
            if r.header("chatgpt-account-id") == Some("acc-1")
                && r.header("authorization").is_some_and(|a| a.starts_with("Bearer "))
            {
                (
                    200,
                    include_str!("../../../tests/fixtures/codex_usage_free_2026-11-06.json").to_string(),
                )
            } else {
                (401, "{}".to_string())
            }
        })
    }

    #[tokio::test]
    async fn fetches_with_account_header() {
        let dir = tempfile::tempdir().unwrap();
        write_auth(dir.path(), Utc::now() + Duration::days(5));
        let srv = server();
        let p = CodexProvider::new(dir.path().to_path_buf(), false).with_endpoint(srv.url("/usage"));
        let r = p.fetch(&ctx(Utc::now())).await;
        assert_eq!(r.status, Status::Ok, "{:?}", r.message);
        assert_eq!(r.plan.as_deref(), Some("Free"), "API plan wins over JWT claim");
        assert_eq!(r.windows[0].id, "monthly");
    }

    #[tokio::test]
    async fn expired_token_falls_back_to_local_logs() {
        let dir = tempfile::tempdir().unwrap();
        write_auth(dir.path(), Utc::now() - Duration::hours(1));
        let day = dir.path().join("sessions/2026/11/06");
        fs::create_dir_all(&day).unwrap();
        fs::write(
            day.join("rollout-x.jsonl"),
            r#"{"timestamp":"2026-11-06T09:00:00Z","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"primary":{"used_percent":64.0,"window_minutes":300,"resets_at":1794000000},"secondary":null,"plan_type":"plus"}}}
"#,
        )
        .unwrap();
        let srv = server();
        let p = CodexProvider::new(dir.path().to_path_buf(), true).with_endpoint(srv.url("/usage"));
        let r = p.fetch(&ctx(Utc::now())).await;
        assert_eq!(r.status, Status::Expired);
        assert_eq!(r.code.as_deref(), Some("token_expired"));
        assert!(r.stale);
        assert_eq!(r.source, Some(Source::LocalLogs));
        assert_eq!(r.windows[0].id, "session");
        assert_eq!(r.windows[0].used_percent, 64.0);
        assert!(srv.requests().is_empty());
    }

    #[tokio::test]
    async fn not_connected_and_api_key_mode() {
        let dir = tempfile::tempdir().unwrap();
        let p = CodexProvider::new(dir.path().to_path_buf(), false);
        assert_eq!(p.fetch(&ctx(Utc::now())).await.status, Status::NotConnected);
        fs::write(
            dir.path().join("auth.json"),
            r#"{"OPENAI_API_KEY":"sk-test","tokens":null}"#,
        )
        .unwrap();
        let r = p.fetch(&ctx(Utc::now())).await;
        assert_eq!(r.status, Status::Unsupported);
        assert_eq!(r.code.as_deref(), Some("api_key_mode"));
    }

    #[test]
    fn jwt_claims_handles_padding() {
        let t = jwt(json!({"exp": 1}));
        assert_eq!(jwt_claims(&t).unwrap()["exp"], 1);
        assert!(jwt_claims("not-a-jwt").is_none());
    }
}
