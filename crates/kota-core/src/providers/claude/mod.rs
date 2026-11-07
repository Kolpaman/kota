//! Claude (claude.ai aboneliği: Pro / Max / Team).
//!
//! Kaynaklar, öncelik sırasıyla:
//! 1. `GET api.anthropic.com/api/oauth/usage`: Claude Code'un OAuth token'ı ile.
//!    Belgelenmemiş; `anthropic-beta: oauth-2025-04-20` başlığı gerekir.
//! 2. Yerel loglar (`~/.claude/projects/**/*.jsonl`): token sayıları; ağ gerekmez.

pub mod auth;
pub mod usage;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

use super::common::{Backoff, Failure, pretty_plan};
use super::{FetchContext, Provider};
use crate::config::ClaudeConfig;
use crate::http::{self, FetchError};
use crate::locallogs::claude::ClaudeLogScanner;
use crate::model::{LocalUsage, ProviderReport, Source, Status};
use crate::paths;
use crate::secret::Secret;
use auth::{ClaudeCredentials, CredsError, RefreshOutcome, RefreshRequest};

pub const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
pub const BETA_HEADER: &str = "oauth-2025-04-20";
pub const LOGIN_COMMAND: &[&str] = &["claude", "auth", "login"];

/// Hız sınırında `Retry-After` yoksa beklenecek süre.
const DEFAULT_BACKOFF_SECS: i64 = 300;
/// Kilit meşgulken (Claude Code yeniliyor) kaç kez beklenecek.
const BUSY_RETRIES: usize = 4;

pub struct ClaudeProvider {
    config_dir: PathBuf,
    refresh_tokens: bool,
    usage_url: String,
    token_url: String,
    scanner: Option<Arc<Mutex<ClaudeLogScanner>>>,
    backoff: Backoff,
}

impl ClaudeProvider {
    pub fn from_config(c: &ClaudeConfig) -> Self {
        let config_dir = c.config_dir.clone().unwrap_or_else(paths::claude_config_dir);
        Self::new(config_dir, c.refresh_tokens, c.local_logs)
    }

    pub fn new(config_dir: PathBuf, refresh_tokens: bool, local_logs: bool) -> Self {
        let scanner = local_logs.then(|| Arc::new(Mutex::new(ClaudeLogScanner::new(config_dir.join("projects")))));
        Self {
            config_dir,
            refresh_tokens,
            usage_url: USAGE_URL.to_string(),
            token_url: auth::TOKEN_URL.to_string(),
            scanner,
            backoff: Backoff::default(),
        }
    }

    /// Testler için: sahte sunucu adresleri.
    pub fn with_endpoints(mut self, usage_url: impl Into<String>, token_url: impl Into<String>) -> Self {
        self.usage_url = usage_url.into();
        self.token_url = token_url.into();
        self
    }

    pub fn credentials_path(&self) -> PathBuf {
        self.config_dir.join(auth::CREDENTIALS_FILE)
    }

    async fn request_usage(&self, http: &reqwest::Client, token: &Secret) -> Result<Value, FetchError> {
        http::send_json(
            http.get(&self.usage_url)
                .bearer_auth(token.expose())
                .header("anthropic-beta", BETA_HEADER)
                .header(reqwest::header::ACCEPT, "application/json"),
        )
        .await
    }

    /// Token'ı Claude Code ile uyumlu biçimde yeniler (bkz. [`auth`]).
    async fn refresh(
        &self,
        ctx: &FetchContext,
        creds: &ClaudeCredentials,
        force: bool,
    ) -> Result<ClaudeCredentials, Failure> {
        if !self.refresh_tokens {
            return Err(Failure::expired(
                "token_expired",
                "Access token expired; open Claude Code once to renew it (token refresh is disabled in config)",
            ));
        }
        if creds.refresh_token.is_none() {
            return Err(Failure::expired(
                "relogin_required",
                "No refresh token stored; run `claude auth login`",
            ));
        }
        let path = self.credentials_path();
        for _ in 0..BUSY_RETRIES {
            let outcome = auth::refresh(RefreshRequest {
                http: &ctx.http,
                claude_dir: &self.config_dir,
                creds_path: &path,
                token_url: &self.token_url,
                entry: creds,
                force,
            })
            .await;
            match outcome {
                RefreshOutcome::Refreshed(c) => {
                    log::info!("claude: access token refreshed");
                    return Ok(c);
                }
                RefreshOutcome::Adopted(c) => return Ok(c),
                RefreshOutcome::Busy => {
                    // Claude Code yeniliyor olabilir: bekle, sonra dosyaya bak.
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    if let Ok(c) = auth::read(&path)
                        && c.access_token != creds.access_token
                    {
                        return Ok(c);
                    }
                }
                RefreshOutcome::InvalidGrant(msg) => {
                    return Err(Failure::expired(
                        "relogin_required",
                        format!("Session ended ({msg}); run `claude auth login`"),
                    ));
                }
                RefreshOutcome::Failed(e) => {
                    return Err(Failure::error("refresh_failed", format!("Token refresh failed: {e}")));
                }
                RefreshOutcome::Io(e) => {
                    return Err(Failure::error("refresh_failed", format!("Token refresh failed: {e}")));
                }
            }
        }
        Err(Failure::error(
            "refresh_busy",
            "Another process is refreshing the token; will retry",
        ))
    }

    async fn fetch_usage(&self, ctx: &FetchContext, mut creds: ClaudeCredentials) -> Result<Value, Failure> {
        if let Some(until) = self.backoff.active_until(ctx.now) {
            return Err(Failure::error(
                "rate_limited",
                format!("Rate limited; next attempt after {}", until.format("%H:%M UTC")),
            ));
        }
        let mut refreshed = false;
        if creds.is_expiring(ctx.now) {
            match self.refresh(ctx, &creds, false).await {
                Ok(c) => {
                    creds = c;
                    refreshed = true;
                }
                // Token hâlâ birkaç dakika geçerliyse onunla devam et.
                Err(f) if creds.is_expired(ctx.now) => return Err(f),
                Err(f) => log::debug!("claude: early refresh skipped: {}", f.message),
            }
        }
        let mut result = self.request_usage(&ctx.http, &creds.access_token).await;
        if matches!(result, Err(FetchError::Unauthorized(_))) && !refreshed && self.refresh_tokens {
            // Token iptal edilmiş veya başka süreç döndürmüş olabilir: bir kez yenile.
            if let Ok(c) = self.refresh(ctx, &creds, true).await {
                result = self.request_usage(&ctx.http, &c.access_token).await;
            }
        }
        match result {
            Ok(v) => {
                self.backoff.clear();
                Ok(v)
            }
            Err(FetchError::Unauthorized(code)) => Err(Failure::expired(
                "token_rejected",
                format!("Claude rejected the token (HTTP {code}); run `claude auth login`"),
            )),
            Err(FetchError::RateLimited { retry_after_secs }) => {
                let wait = retry_after_secs
                    .map(|s| s as i64)
                    .unwrap_or(DEFAULT_BACKOFF_SECS)
                    .clamp(60, 3600);
                self.backoff.set(ctx.now + Duration::seconds(wait));
                Err(Failure::error(
                    "rate_limited",
                    format!("Rate limited by Anthropic; retrying in {wait}s"),
                ))
            }
            Err(e) => Err(Failure::error(e.code(), e.to_string())),
        }
    }

    async fn local_usage(&self, ctx: &FetchContext, window_start: DateTime<Utc>) -> Option<LocalUsage> {
        let scanner = self.scanner.clone()?;
        let (now, today) = (ctx.now, ctx.today_start);
        tokio::task::spawn_blocking(move || {
            let mut s = scanner.lock().unwrap_or_else(|e| e.into_inner());
            s.summarize(now, today, window_start)
        })
        .await
        .ok()
        .flatten()
    }
}

#[async_trait]
impl Provider for ClaudeProvider {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn name(&self) -> &'static str {
        "Claude"
    }

    fn watched_files(&self) -> Vec<PathBuf> {
        vec![self.credentials_path()]
    }

    async fn fetch(&self, ctx: &FetchContext) -> ProviderReport {
        let mut report = ProviderReport::new(self.id(), self.name());
        report.login_command = Some(LOGIN_COMMAND.iter().map(|s| s.to_string()).collect());

        match auth::read(&self.credentials_path()) {
            Err(CredsError::NotFound | CredsError::NoOauth) => {
                report.set_status(Status::NotConnected, "no_credentials", "Claude Code is not signed in");
            }
            Err(CredsError::Invalid(e) | CredsError::Io(e)) => {
                report.set_status(Status::Error, "credentials_unreadable", e);
            }
            Ok(creds) => {
                report.plan = creds
                    .subscription_type
                    .as_deref()
                    .map(|p| pretty_plan(p, creds.rate_limit_tier.as_deref()));
                match self.fetch_usage(ctx, creds).await {
                    Ok(v) => match usage::parse(&v, &ctx.thresholds) {
                        Ok(parsed) => {
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

        // Oturum penceresinin başlangıcı: sıfırlanma − 5 saat (bilinmiyorsa son 5 saat).
        let window_start = report
            .windows
            .iter()
            .find(|w| w.id == "session")
            .and_then(|w| Some(w.resets_at? - Duration::seconds(w.window_seconds? as i64)))
            .filter(|s| *s <= ctx.now)
            .unwrap_or(ctx.now - Duration::hours(5));
        report.local = self.local_usage(ctx, window_start).await;
        report
    }
}

#[cfg(test)]
mod tests;
