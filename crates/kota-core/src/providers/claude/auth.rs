//! Claude Code giriş bilgileri (`~/.claude/.credentials.json`) ve token yenileme.
//!
//! Token yenileme yarışı planın en önemli risklerinden biri: refresh token her
//! yenilemede döndürülür (eskisi geçersiz olur). Kota bu yüzden Claude Code'un
//! kendi protokolünü birebir izler (Claude Code 2.1.x'te doğrulandı):
//!
//! 1. Yeni kilit: `<claude_dir>/.oauth_refresh.lock`, ardından eski kilit:
//!    `<realpath(claude_dir)>.lock`. İkisi de `mkdir` ile alınan
//!    proper-lockfile dizinleridir; mtime'ı 60 sn'den eskiyse bayattır.
//! 2. Kilit altında dosya yeniden okunur. Erişim token'ı girişte görülenden
//!    farklıysa başka bir süreç (ör. Claude Code) zaten yenilemiştir: o
//!    benimsenir, istek atılmaz.
//! 3. Yenileme sonrası compare-and-swap: dosyadaki refresh token hâlâ bizim
//!    gönderdiğimizse yazılır; değilse dosyadaki yeni değer benimsenir.
//! 4. Yazma atomiktir (geçici dosya + rename, izin 0600) ve dosyadaki
//!    bilinmeyen tüm alanlar korunur.
//!
//! Claude Code da yenilemeden önce dosyayı kilit altında yeniden okuduğu için
//! Kota'nın yazdığı token'ları "kardeş süreç yenilemiş" diye benimser.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use crate::http::{self, FetchError};
use crate::jsonx;
use crate::secret::Secret;

/// Claude Code'un OAuth istemci kimliği (herkese açık, CLI içinde sabit).
pub const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
pub const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
pub const CREDENTIALS_FILE: &str = ".credentials.json";
pub const LOCK_DIR: &str = ".oauth_refresh.lock";
/// proper-lockfile varsayılanı ile aynı.
pub const LOCK_STALE: Duration = Duration::from_secs(60);
/// Bitişe bu kadar kala yenilenir.
pub const REFRESH_MARGIN_SECS: i64 = 120;

#[derive(Debug, Clone)]
pub struct ClaudeCredentials {
    pub access_token: Secret,
    pub refresh_token: Option<Secret>,
    pub expires_at: Option<DateTime<Utc>>,
    pub scopes: Vec<String>,
    pub subscription_type: Option<String>,
    pub rate_limit_tier: Option<String>,
}

impl ClaudeCredentials {
    /// Süresi dolmuş veya `REFRESH_MARGIN_SECS` içinde dolacak.
    pub fn is_expiring(&self, now: DateTime<Utc>) -> bool {
        self.expires_at
            .is_some_and(|t| t <= now + chrono::Duration::seconds(REFRESH_MARGIN_SECS))
    }

    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_some_and(|t| t <= now)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CredsError {
    /// Dosya yok.
    NotFound,
    /// Dosya var ama claude.ai OAuth girişi yok.
    NoOauth,
    Invalid(String),
    Io(String),
}

pub fn read_raw(path: &Path) -> Result<Value, CredsError> {
    let text = fs::read_to_string(path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => CredsError::NotFound,
        _ => CredsError::Io(e.to_string()),
    })?;
    serde_json::from_str(&text).map_err(|e| CredsError::Invalid(format!("credentials JSON: {e}")))
}

pub fn parse(raw: &Value) -> Result<ClaudeCredentials, CredsError> {
    let o = raw
        .get("claudeAiOauth")
        .filter(|o| o.is_object())
        .ok_or(CredsError::NoOauth)?;
    let access = o
        .get("accessToken")
        .and_then(jsonx::str_of)
        .ok_or(CredsError::NoOauth)?;
    let scopes = o
        .get("scopes")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    Ok(ClaudeCredentials {
        access_token: Secret::new(access),
        refresh_token: o.get("refreshToken").and_then(jsonx::str_of).map(Secret::new),
        expires_at: o.get("expiresAt").and_then(jsonx::time_of),
        scopes,
        subscription_type: o.get("subscriptionType").and_then(jsonx::str_of).map(str::to_string),
        rate_limit_tier: o.get("rateLimitTier").and_then(jsonx::str_of).map(str::to_string),
    })
}

pub fn read(path: &Path) -> Result<ClaudeCredentials, CredsError> {
    parse(&read_raw(path)?)
}

/// proper-lockfile uyumlu dizin kilidi. Bırakılınca (drop) silinir.
#[derive(Debug)]
pub struct DirLock {
    path: PathBuf,
}

impl DirLock {
    /// Kilit alınamazsa (başka süreç tutuyor) `Ok(None)`.
    pub fn try_acquire(path: &Path, stale: Duration) -> io::Result<Option<DirLock>> {
        for attempt in 0..2 {
            match fs::create_dir(path) {
                Ok(()) => {
                    return Ok(Some(DirLock {
                        path: path.to_path_buf(),
                    }));
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    if attempt == 0 && is_stale(path, stale) {
                        // proper-lockfile de bayat kilidi silip yeniden dener.
                        let _ = fs::remove_dir(path);
                        continue;
                    }
                    return Ok(None);
                }
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }
}

impl Drop for DirLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.path);
    }
}

fn is_stale(path: &Path, stale: Duration) -> bool {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age > stale)
}

/// Claude Code'un eski kilidi: `realpath(claude_dir) + ".lock"`.
pub fn legacy_lock_path(claude_dir: &Path) -> PathBuf {
    let real = fs::canonicalize(claude_dir).unwrap_or_else(|_| claude_dir.to_path_buf());
    let mut s = real.into_os_string();
    s.push(".lock");
    PathBuf::from(s)
}

#[derive(Debug)]
pub enum RefreshOutcome {
    /// Kota yeniledi ve dosyaya yazdı.
    Refreshed(ClaudeCredentials),
    /// Başka bir süreç zaten yenilemiş; onun token'ı benimsendi.
    Adopted(ClaudeCredentials),
    /// Kilit şu an başka bir süreçte (büyük olasılıkla Claude Code yeniliyor).
    Busy,
    /// Refresh token reddedildi: yeniden giriş gerekli.
    InvalidGrant(String),
    Failed(FetchError),
    Io(String),
}

/// Token yenileme parametreleri.
pub struct RefreshRequest<'a> {
    pub http: &'a reqwest::Client,
    pub claude_dir: &'a Path,
    pub creds_path: &'a Path,
    pub token_url: &'a str,
    /// Yenileme kararının verildiği an görülen giriş bilgisi.
    pub entry: &'a ClaudeCredentials,
    /// Süresi dolmamış olsa da yenile (API token'ı reddettiyse).
    pub force: bool,
}

pub async fn refresh(req: RefreshRequest<'_>) -> RefreshOutcome {
    let lock_path = req.claude_dir.join(LOCK_DIR);
    let _lock = match DirLock::try_acquire(&lock_path, LOCK_STALE) {
        Ok(Some(l)) => l,
        Ok(None) => return RefreshOutcome::Busy,
        Err(e) => return RefreshOutcome::Io(format!("lock {}: {e}", lock_path.display())),
    };
    // Eski kilit de alınır; başka bir süreçteyse geri çekil. (Claude Code bu
    // kilidin ELOCKED dışındaki hatalarını yok sayıyor; biz de öyle.)
    let _legacy = match DirLock::try_acquire(&legacy_lock_path(req.claude_dir), LOCK_STALE) {
        Ok(Some(l)) => Some(l),
        Ok(None) => return RefreshOutcome::Busy,
        Err(e) => {
            log::debug!("legacy lock could not be taken: {e}");
            None
        }
    };

    let current = match read(req.creds_path) {
        Ok(c) => c,
        Err(e) => return RefreshOutcome::Io(format!("credentials re-read: {e:?}")),
    };
    if current.access_token != req.entry.access_token {
        return RefreshOutcome::Adopted(current);
    }
    if !req.force && !current.is_expiring(Utc::now()) {
        return RefreshOutcome::Adopted(current);
    }
    let Some(refresh_token) = current.refresh_token.clone() else {
        return RefreshOutcome::InvalidGrant("no refresh token stored".into());
    };

    let mut body = json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token.expose(),
        "client_id": CLIENT_ID,
    });
    if !current.scopes.is_empty() {
        body["scope"] = Value::String(current.scopes.join(" "));
    }
    let response = match req.http.post(req.token_url).json(&body).send().await {
        Ok(r) => r,
        Err(e) => return RefreshOutcome::Failed(FetchError::Network(e.to_string())),
    };
    let status = response.status();
    let headers = response.headers().clone();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        if status.as_u16() == 400 || status.as_u16() == 401 {
            let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            let code = v.get("error").and_then(jsonx::str_of).unwrap_or("");
            if code == "invalid_grant" || status.as_u16() == 401 {
                let desc = v
                    .get("error_description")
                    .and_then(jsonx::str_of)
                    .unwrap_or("refresh token rejected");
                return RefreshOutcome::InvalidGrant(desc.to_string());
            }
        }
        return RefreshOutcome::Failed(http::classify_status(status, &headers, &text));
    }
    let tokens: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => return RefreshOutcome::Failed(FetchError::Parse(format!("token response: {e}"))),
    };
    let Some(new_access) = tokens.get("access_token").and_then(jsonx::str_of) else {
        return RefreshOutcome::Failed(FetchError::Parse("token response has no access_token".into()));
    };
    let received_at = Utc::now();

    // Compare-and-swap yazma.
    let mut raw = match read_raw(req.creds_path) {
        Ok(v) => v,
        Err(e) => return RefreshOutcome::Io(format!("credentials re-read: {e:?}")),
    };
    let on_disk = parse(&raw).ok();
    let still_ours = on_disk
        .as_ref()
        .and_then(|c| c.refresh_token.as_ref())
        .is_some_and(|rt| rt == &refresh_token);
    if !still_ours {
        return match on_disk {
            Some(c) => RefreshOutcome::Adopted(c),
            None => RefreshOutcome::Io("credentials changed unexpectedly during refresh".into()),
        };
    }
    patch_tokens(&mut raw, new_access, &tokens, received_at);
    let data = match serde_json::to_vec(&raw) {
        Ok(d) => d,
        Err(e) => return RefreshOutcome::Io(e.to_string()),
    };
    if let Err(e) = write_atomic(req.creds_path, &data) {
        return RefreshOutcome::Io(format!("credentials write: {e}"));
    }
    match parse(&raw) {
        Ok(c) => RefreshOutcome::Refreshed(c),
        Err(e) => RefreshOutcome::Io(format!("{e:?}")),
    }
}

/// Yanıttaki token'ları `claudeAiOauth` içine yazar; diğer alanlara dokunmaz.
fn patch_tokens(raw: &mut Value, access: &str, tokens: &Value, received_at: DateTime<Utc>) {
    let Some(o) = raw.get_mut("claudeAiOauth").and_then(Value::as_object_mut) else {
        return;
    };
    let now_ms = received_at.timestamp_millis();
    o.insert("accessToken".into(), Value::String(access.to_string()));
    if let Some(rt) = tokens.get("refresh_token").and_then(jsonx::str_of) {
        o.insert("refreshToken".into(), Value::String(rt.to_string()));
    }
    if let Some(secs) = tokens.get("expires_in").and_then(jsonx::u64_of) {
        o.insert("expiresAt".into(), json!(now_ms + (secs as i64) * 1000));
    }
    if let Some(scope) = tokens.get("scope").and_then(jsonx::str_of) {
        let scopes: Vec<Value> = scope.split_whitespace().map(|s| Value::String(s.into())).collect();
        if !scopes.is_empty() {
            o.insert("scopes".into(), Value::Array(scopes));
        }
    }
    // Claude Code gibi: refresh token bitişi yalnızca öne çekilebilir.
    if let Some(secs) = tokens.get("refresh_token_expires_in").and_then(jsonx::u64_of) {
        let new_exp = now_ms + (secs as i64) * 1000;
        if let Some(old) = o.get("refreshTokenExpiresAt").and_then(Value::as_i64) {
            o.insert("refreshTokenExpiresAt".into(), json!(old.min(new_exp)));
        }
    }
}

/// Geçici dosyaya yazıp yerine taşır; izinler 0600. Sembolik bağ ise hedefine yazar.
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "credentials".into());
    let tmp = dir.join(format!(".{name}.kota-{}.tmp", std::process::id()));
    let _ = fs::remove_file(&tmp);
    let result = (|| {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(access: &str, refresh: &str, expires_ms: i64) -> Value {
        json!({
            "claudeAiOauth": {
                "accessToken": access,
                "refreshToken": refresh,
                "expiresAt": expires_ms,
                "refreshTokenExpiresAt": 1_900_000_000_000_i64,
                "scopes": ["user:inference", "user:profile"],
                "subscriptionType": "pro",
                "subscriptionTypeSource": "profile",
                "rateLimitTier": "default_claude_ai"
            },
            "mcpOAuth": {"server": {"token": "keep-me"}}
        })
    }

    #[test]
    fn parses_and_detects_expiry() {
        let now = Utc::now();
        let v = sample("a1", "r1", (now - chrono::Duration::minutes(1)).timestamp_millis());
        let c = parse(&v).unwrap();
        assert!(c.is_expired(now));
        assert!(c.is_expiring(now));
        assert_eq!(c.subscription_type.as_deref(), Some("pro"));
        let fresh = parse(&sample("a", "r", (now + chrono::Duration::hours(3)).timestamp_millis())).unwrap();
        assert!(!fresh.is_expiring(now));
        assert_eq!(parse(&json!({"mcpOAuth": {}})).unwrap_err(), CredsError::NoOauth);
    }

    #[test]
    fn lock_is_exclusive_and_released() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(LOCK_DIR);
        let a = DirLock::try_acquire(&p, LOCK_STALE).unwrap();
        assert!(a.is_some());
        assert!(DirLock::try_acquire(&p, LOCK_STALE).unwrap().is_none());
        drop(a);
        assert!(!p.exists());
        assert!(DirLock::try_acquire(&p, LOCK_STALE).unwrap().is_some());
    }

    #[test]
    fn stale_lock_is_taken_over() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(LOCK_DIR);
        fs::create_dir(&p).unwrap();
        // mtime "şimdi"; 0 sn bayatlık eşiğiyle hemen bayat sayılır.
        std::thread::sleep(Duration::from_millis(20));
        let l = DirLock::try_acquire(&p, Duration::from_millis(1)).unwrap();
        assert!(l.is_some());
    }

    #[test]
    fn legacy_lock_path_appends_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join(".claude");
        fs::create_dir(&claude).unwrap();
        let p = legacy_lock_path(&claude);
        assert!(p.to_string_lossy().ends_with("/.claude.lock"));
    }

    #[test]
    fn patch_preserves_unknown_fields() {
        let mut raw = sample("a1", "r1", 1);
        let tokens = json!({"access_token": "a2", "refresh_token": "r2", "expires_in": 28800, "scope": "user:inference user:profile user:sessions:claude_code", "refresh_token_expires_in": 999_999_999});
        let at = Utc::now();
        patch_tokens(&mut raw, "a2", &tokens, at);
        let o = &raw["claudeAiOauth"];
        assert_eq!(o["accessToken"], "a2");
        assert_eq!(o["refreshToken"], "r2");
        assert_eq!(o["expiresAt"], json!(at.timestamp_millis() + 28_800_000));
        assert_eq!(o["scopes"].as_array().unwrap().len(), 3);
        assert_eq!(o["subscriptionTypeSource"], "profile");
        assert_eq!(
            o["refreshTokenExpiresAt"],
            json!(1_900_000_000_000_i64),
            "only moves earlier"
        );
        assert_eq!(raw["mcpOAuth"]["server"]["token"], "keep-me");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_sets_private_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(CREDENTIALS_FILE);
        fs::write(&p, "{}").unwrap();
        write_atomic(&p, b"{\"x\":1}").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "{\"x\":1}");
        assert_eq!(fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        // geçici dosya kalmamalı
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
