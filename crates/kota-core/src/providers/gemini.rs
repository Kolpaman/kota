//! Gemini (Gemini CLI'ın Google girişi, "Code Assist" kotası).
//!
//! Gemini CLI 0.x paketinden doğrulanan akış (bkz. `docs/kesif.md`):
//! 1. `~/.gemini/oauth_creds.json` → `access_token`, `refresh_token`, `expiry_date` (ms)
//! 2. `POST cloudcode-pa.googleapis.com/v1internal:loadCodeAssist` → `cloudaicompanionProject`
//! 3. `POST …/v1internal:retrieveUserQuota {project}` → `buckets[{modelId, remainingFraction, resetTime}]`
//!
//! Google refresh token'ları dönmediği için yenileme yalnızca bellekte
//! yapılır; Gemini CLI'ın dosyasına yazılmaz (yarış riski yok).

use std::path::PathBuf;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};

use super::common::{Failure, pretty_plan};
use super::{FetchContext, Provider};
use crate::http::{self, FetchError};
use crate::jsonx::{f64_of, str_of, time_of, u64_of};
use crate::model::{ProviderReport, Source, Status, UsageWindow};
use crate::paths;
use crate::secret::Secret;

pub const LOGIN_COMMAND: &[&str] = &["gemini"];
pub const API_BASE: &str = "https://cloudcode-pa.googleapis.com/v1internal";
pub const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const MAX_WINDOWS: usize = 6;

/// Gemini CLI'ın "installed app" OAuth istemcisi. Google bu değerleri gizli
/// saymaz ve Gemini CLI paketinde düz metin olarak durur; yine de koda gömülmez,
/// kurulu Gemini CLI'dan okunur (ya da `KOTA_GEMINI_CLIENT_ID` /
/// `KOTA_GEMINI_CLIENT_SECRET` ortam değişkenlerinden). Süreç başına bir kez aranır.
fn oauth_client() -> Option<&'static (String, String)> {
    static CLIENT: std::sync::OnceLock<Option<(String, String)>> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            if let (Ok(id), Ok(secret)) =
                (std::env::var("KOTA_GEMINI_CLIENT_ID"), std::env::var("KOTA_GEMINI_CLIENT_SECRET"))
                && !id.is_empty()
                && !secret.is_empty()
            {
                return Some((id, secret));
            }
            gemini_cli_dirs().iter().find_map(|d| scan_package(d))
        })
        .as_ref()
}

/// Gemini CLI paket dizini adayları: PATH'teki `gemini`nin gerçek yolu ve
/// yaygın global npm kökleri.
fn gemini_cli_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            if let Ok(real) = std::fs::canonicalize(dir.join("gemini"))
                && let Some(pkg) = real.ancestors().find(|a| a.ends_with("@google/gemini-cli"))
            {
                out.push(pkg.to_path_buf());
            }
        }
    }
    let home = paths::home_dir();
    let mut roots = vec![
        PathBuf::from("/usr/local/lib/node_modules"),
        PathBuf::from("/usr/lib/node_modules"),
        home.join(".npm-global/lib/node_modules"),
        home.join(".local/lib/node_modules"),
        home.join(".bun/install/global/node_modules"),
    ];
    if let Ok(entries) = std::fs::read_dir(home.join(".nvm/versions/node")) {
        roots.extend(entries.flatten().map(|e| e.path().join("lib/node_modules")));
    }
    out.extend(roots.into_iter().map(|r| r.join("@google/gemini-cli")).filter(|p| p.is_dir()));
    out
}

/// Paketteki JS dosyalarında `OAUTH_CLIENT_ID = "…"` ve `OAUTH_CLIENT_SECRET = "…"`.
fn scan_package(pkg: &std::path::Path) -> Option<(String, String)> {
    let mut files = Vec::new();
    for sub in ["bundle", "node_modules/@google/gemini-cli-core/dist/src/code_assist"] {
        if let Ok(entries) = std::fs::read_dir(pkg.join(sub)) {
            files.extend(
                entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "js")),
            );
        }
    }
    files.iter().find_map(|f| {
        let text = std::fs::read_to_string(f).ok()?;
        Some((js_const(&text, "OAUTH_CLIENT_ID")?, js_const(&text, "OAUTH_CLIENT_SECRET")?))
    })
}

fn js_const(text: &str, name: &str) -> Option<String> {
    let start = text.find(&format!("{name} = \""))? + name.len() + 4;
    let end = text[start..].find('"')?;
    let v = &text[start..start + end];
    (!v.is_empty() && v.len() < 200).then(|| v.to_string())
}

#[derive(Default)]
struct Cache {
    token: Option<(Secret, DateTime<Utc>)>,
    project: Option<String>,
    plan: Option<String>,
}

pub struct GeminiProvider {
    home: PathBuf,
    api_base: String,
    token_url: String,
    cache: Mutex<Cache>,
}

impl GeminiProvider {
    pub fn from_default() -> Self {
        Self::new(paths::home_dir().join(".gemini"))
    }

    pub fn new(home: PathBuf) -> Self {
        Self {
            home,
            api_base: API_BASE.to_string(),
            token_url: TOKEN_URL.to_string(),
            cache: Mutex::new(Cache::default()),
        }
    }

    pub fn with_endpoints(mut self, api_base: impl Into<String>, token_url: impl Into<String>) -> Self {
        self.api_base = api_base.into();
        self.token_url = token_url.into();
        self
    }

    fn creds_path(&self) -> PathBuf {
        self.home.join("oauth_creds.json")
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Cache> {
        self.cache.lock().unwrap_or_else(|e| e.into_inner())
    }

    async fn access_token(&self, ctx: &FetchContext, creds: &Value) -> Result<Secret, Failure> {
        let file_token = creds.get("access_token").and_then(str_of).map(Secret::new);
        let file_exp = creds.get("expiry_date").and_then(time_of);
        let margin = ctx.now + Duration::seconds(120);
        if let (Some(t), Some(exp)) = (&file_token, file_exp)
            && exp > margin
        {
            return Ok(t.clone());
        }
        if let Some((t, exp)) = self.lock().token.clone()
            && exp > margin
        {
            return Ok(t);
        }
        let Some(refresh) = creds.get("refresh_token").and_then(str_of) else {
            return file_token.ok_or_else(|| Failure::expired("relogin_required", "No Gemini token; run `gemini`"));
        };
        let Some((client_id, client_secret)) = oauth_client() else {
            return file_token.ok_or_else(|| {
                Failure::expired("relogin_required", "Gemini CLI not found; its OAuth client is needed to refresh the token")
            });
        };
        let body = form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh),
            ("grant_type", "refresh_token"),
        ]);
        let resp = ctx
            .http
            .post(&self.token_url)
            .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|e| Failure::error("network", e.to_string()))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if !status.is_success() {
            if v.get("error").and_then(str_of) == Some("invalid_grant") {
                return Err(Failure::expired(
                    "relogin_required",
                    "Google session ended; run `gemini` to sign in",
                ));
            }
            return Err(Failure::error(
                "refresh_failed",
                format!("Token refresh failed: HTTP {status}"),
            ));
        }
        let token = v
            .get("access_token")
            .and_then(str_of)
            .ok_or_else(|| Failure::error("refresh_failed", "token response has no access_token"))?;
        let exp = ctx.now + Duration::seconds(v.get("expires_in").and_then(u64_of).unwrap_or(3600) as i64);
        let secret = Secret::new(token);
        self.lock().token = Some((secret.clone(), exp));
        Ok(secret)
    }

    async fn post(&self, ctx: &FetchContext, method: &str, token: &Secret, body: Value) -> Result<Value, FetchError> {
        http::send_json(
            ctx.http
                .post(format!("{}:{method}", self.api_base))
                .bearer_auth(token.expose())
                .json(&body),
        )
        .await
    }

    /// HTTP hatasını rapor durumuna çevirir.
    fn failure(&self, e: FetchError) -> Failure {
        match e {
            FetchError::Unauthorized(code) => {
                self.lock().token = None;
                Failure::expired(
                    "token_rejected",
                    format!("Google rejected the token (HTTP {code}); run `gemini`"),
                )
            }
            e => Failure::error(e.code(), e.to_string()),
        }
    }

    async fn project(&self, ctx: &FetchContext, token: &Secret) -> Result<String, Failure> {
        if let Some(p) = self.lock().project.clone() {
            return Ok(p);
        }
        let env_project = std::env::var("GOOGLE_CLOUD_PROJECT").ok().filter(|p| !p.is_empty());
        let body = json!({
            "cloudaicompanionProject": env_project,
            "metadata": {"ideType": "IDE_UNSPECIFIED", "platform": "PLATFORM_UNSPECIFIED", "pluginType": "GEMINI", "duetProject": env_project},
        });
        let v = self
            .post(ctx, "loadCodeAssist", token, body)
            .await
            .map_err(|e| self.failure(e))?;
        // Hesap hiçbir katmana uygun değilse Google nedenini söylüyor (ör. 2026-11'de
        // ücretsiz katman için "UNSUPPORTED_CLIENT … migrate to Antigravity").
        if v.get("currentTier").is_none()
            && v.get("cloudaicompanionProject").is_none()
            && let Some(reason) = v
                .get("ineligibleTiers")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(|t| t.get("reasonMessage").and_then(str_of))
        {
            return Err(Failure {
                status: Status::Unsupported,
                code: "ineligible",
                message: reason.to_string(),
            });
        }
        let project = v
            .get("cloudaicompanionProject")
            .and_then(|p| {
                str_of(p)
                    .map(str::to_string)
                    .or_else(|| p.get("id").and_then(str_of).map(str::to_string))
            })
            .or(env_project)
            .ok_or_else(|| {
                Failure::error(
                    "parse_error",
                    "no Code Assist project (run `gemini` once to finish setup)",
                )
            })?;
        let tier = v.get("paidTier").or_else(|| v.get("currentTier"));
        let plan = tier
            .and_then(|t| t.get("name").and_then(str_of).or_else(|| t.get("id").and_then(str_of)))
            .map(|n| pretty_plan(n.trim_end_matches("-tier"), None));
        let mut c = self.lock();
        c.project = Some(project.clone());
        c.plan = plan;
        Ok(project)
    }
}

#[async_trait]
impl Provider for GeminiProvider {
    fn id(&self) -> &'static str {
        "gemini"
    }

    fn name(&self) -> &'static str {
        "Gemini"
    }

    fn watched_files(&self) -> Vec<PathBuf> {
        vec![self.creds_path()]
    }

    async fn fetch(&self, ctx: &FetchContext) -> ProviderReport {
        let mut report = ProviderReport::new(self.id(), self.name());
        report.login_command = Some(LOGIN_COMMAND.iter().map(|s| s.to_string()).collect());
        let creds: Value = match std::fs::read_to_string(self.creds_path()) {
            Ok(t) => match serde_json::from_str(&t) {
                Ok(v) => v,
                Err(e) => {
                    report.set_status(Status::Error, "credentials_unreadable", e.to_string());
                    return report;
                }
            },
            Err(_) => {
                report.set_status(
                    Status::NotConnected,
                    "no_credentials",
                    "Gemini CLI is not signed in with Google",
                );
                return report;
            }
        };
        let token = match self.access_token(ctx, &creds).await {
            Ok(t) => t,
            Err(f) => {
                f.apply(&mut report);
                return report;
            }
        };
        let project = match self.project(ctx, &token).await {
            Ok(p) => p,
            Err(f) => {
                f.apply(&mut report);
                return report;
            }
        };
        match self
            .post(ctx, "retrieveUserQuota", &token, json!({ "project": project }))
            .await
        {
            Ok(v) => {
                report.windows = parse_quota(&v, ctx);
                report.plan = self.lock().plan.clone();
                report.source = Some(Source::Api);
                report.fetched_at = Some(ctx.now);
            }
            Err(e) => self.failure(e).apply(&mut report),
        }
        report
    }
}

/// Model başına günlük kota → pencereler (en dolu ilk, en fazla 6).
pub fn parse_quota(v: &Value, ctx: &FetchContext) -> Vec<UsageWindow> {
    let mut best: Vec<UsageWindow> = Vec::new();
    for b in v.get("buckets").and_then(Value::as_array).into_iter().flatten() {
        let (Some(model), Some(frac)) = (
            b.get("modelId").and_then(str_of),
            b.get("remainingFraction").and_then(f64_of),
        ) else {
            continue;
        };
        let used = ((1.0 - frac) * 100.0).clamp(0.0, 100.0);
        let id = format!("model:{model}");
        let resets_at = b.get("resetTime").and_then(time_of);
        match best.iter_mut().find(|w| w.id == id) {
            Some(w) if w.used_percent >= used => {}
            Some(w) => *w = UsageWindow::new(id, model, used, resets_at, Some(86_400), &ctx.thresholds),
            None => best.push(UsageWindow::new(
                id,
                model,
                used,
                resets_at,
                Some(86_400),
                &ctx.thresholds,
            )),
        }
    }
    best.sort_by(|a, b| b.used_percent.total_cmp(&a.used_percent).then_with(|| a.id.cmp(&b.id)));
    best.truncate(MAX_WINDOWS);
    best
}

/// `application/x-www-form-urlencoded` gövde.
fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_js_constants() {
        let js = r#"var OAUTH_CLIENT_ID = "abc.apps.example.com"; var OAUTH_CLIENT_SECRET = "s3cr3t";"#;
        assert_eq!(js_const(js, "OAUTH_CLIENT_ID").as_deref(), Some("abc.apps.example.com"));
        assert_eq!(js_const(js, "OAUTH_CLIENT_SECRET").as_deref(), Some("s3cr3t"));
        assert_eq!(js_const(js, "MISSING"), None);
    }
    use crate::testutil::{FakeServer, ctx};

    #[test]
    fn form_encoding() {
        assert_eq!(form(&[("a", "x y/z"), ("b", "1")]), "a=x%20y%2Fz&b=1");
    }

    #[tokio::test]
    async fn full_flow_with_in_memory_refresh() {
        // Gemini CLI kurulu olmayan makinelerde de çalışsın (istemci ortamdan).
        // SAFETY: bu süreçte ortamı yalnızca bu test yazar; değer sabittir.
        unsafe {
            std::env::set_var("KOTA_GEMINI_CLIENT_ID", "test-client.apps.example.com");
            std::env::set_var("KOTA_GEMINI_CLIENT_SECRET", "test-secret");
        }
        let dir = tempfile::tempdir().unwrap();
        let expired = (Utc::now() - Duration::hours(1)).timestamp_millis();
        std::fs::write(
            dir.path().join("oauth_creds.json"),
            format!(r#"{{"access_token":"old","refresh_token":"rt","expiry_date":{expired},"token_type":"Bearer"}}"#),
        )
        .unwrap();
        let before = std::fs::read_to_string(dir.path().join("oauth_creds.json")).unwrap();
        let srv = FakeServer::start(|r| {
            match r.path.as_str() {
            "/token" => (200, r#"{"access_token":"fresh","expires_in":3599}"#.into()),
            "/api:loadCodeAssist" if r.header("authorization") == Some("Bearer fresh") => (
                200,
                r#"{"currentTier":{"id":"free-tier","name":"Gemini Code Assist for individuals"},"cloudaicompanionProject":"proj-123"}"#.into(),
            ),
            "/api:retrieveUserQuota" if r.body.contains("proj-123") => (
                200,
                r#"{"buckets":[{"modelId":"gemini-2.5-pro","remainingFraction":0.25,"resetTime":"2026-11-08T00:00:00Z","tokenType":"REQUESTS"},{"modelId":"gemini-2.5-flash","remainingFraction":0.9,"resetTime":"2026-11-08T00:00:00Z"},{"modelId":"gemini-2.5-pro","remainingFraction":0.5}]}"#.into(),
            ),
            _ => (401, "{}".into()),
        }
        });
        let p = GeminiProvider::new(dir.path().to_path_buf()).with_endpoints(srv.url("/api"), srv.url("/token"));
        let r = p.fetch(&ctx(Utc::now())).await;
        assert_eq!(r.status, Status::Ok, "{:?}", r.message);
        assert_eq!(r.windows.len(), 2);
        assert_eq!(r.windows[0].id, "model:gemini-2.5-pro");
        assert_eq!(r.windows[0].used_percent, 75.0);
        assert_eq!(r.plan.as_deref(), Some("Gemini code assist for individuals"));
        assert!(srv.requests()[0].body.contains("grant_type=refresh_token"));
        // Gemini CLI'ın dosyasına dokunulmaz.
        assert_eq!(
            std::fs::read_to_string(dir.path().join("oauth_creds.json")).unwrap(),
            before
        );
        // İkinci sorgu: önbellekteki token ve proje kullanılır.
        let _ = p.fetch(&ctx(Utc::now())).await;
        assert_eq!(srv.count("/token"), 1);
        assert_eq!(srv.count("/api:loadCodeAssist"), 1);
    }

    #[tokio::test]
    async fn ineligible_account_is_unsupported_with_reason() {
        let dir = tempfile::tempdir().unwrap();
        let future = (Utc::now() + Duration::hours(1)).timestamp_millis();
        std::fs::write(
            dir.path().join("oauth_creds.json"),
            format!(r#"{{"access_token":"t","expiry_date":{future}}}"#),
        )
        .unwrap();
        // 2026-11-07'de bu makinede alınan gerçek yanıtın biçimi.
        let srv = FakeServer::start(|_| {
            (200, r#"{"allowedTiers":[{"id":"standard-tier","name":"Gemini Code Assist","userDefinedCloudaicompanionProject":true}],"ineligibleTiers":[{"reasonCode":"UNSUPPORTED_CLIENT","reasonMessage":"This client is no longer supported for Gemini Code Assist for individuals.","tierId":"free-tier"}]}"#.into())
        });
        let r = GeminiProvider::new(dir.path().to_path_buf())
            .with_endpoints(srv.url("/api"), srv.url("/token"))
            .fetch(&ctx(Utc::now()))
            .await;
        assert_eq!(r.status, Status::Unsupported);
        assert_eq!(r.code.as_deref(), Some("ineligible"));
        assert!(r.message.unwrap().contains("no longer supported"));
    }

    #[tokio::test]
    async fn not_connected_without_creds() {
        let dir = tempfile::tempdir().unwrap();
        let r = GeminiProvider::new(dir.path().to_path_buf())
            .fetch(&ctx(Utc::now()))
            .await;
        assert_eq!(r.status, Status::NotConnected);
    }
}
