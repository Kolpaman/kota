//! Hesap profilleri: birden fazla Claude / Codex hesabı arasında geçiş.
//!
//! Profil = CLI'nin kimlik dosyasının tam kopyası (Claude için ayrıca
//! `~/.claude.json` içindeki `oauthAccount` bilgisi). Kopyalar GNOME
//! Anahtarlığı'nda saklanır; diskte yalnızca hangi profilin etkin olduğu
//! (`~/.config/kota/accounts.json`, gizli bilgi içermez) tutulur.
//!
//! Geçişte önce etkin profil, dosyanın *güncel* hâliyle yeniden kaydedilir:
//! CLI'ler token'ları yenileyip dosyaya yazdığından eski kopya bayatlamış
//! olabilir. Claude dosyası, Claude Code'un yenileme kilidi alınarak yazılır.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use base64::Engine as _;
use kota_core::paths;
use kota_core::providers::claude::auth::{self, DirLock};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::keyring;

pub const PROVIDERS: &[&str] = &["claude", "codex"];

#[derive(Debug, Serialize, Deserialize)]
struct Stored {
    /// Kimlik dosyasının içeriği.
    file: String,
    /// Claude: `~/.claude.json` → `oauthAccount`.
    #[serde(default)]
    account: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct Profile {
    pub provider: String,
    pub name: String,
    pub active: bool,
}

fn state_path() -> PathBuf {
    paths::config_file().with_file_name("accounts.json")
}

fn load_state() -> BTreeMap<String, String> {
    fs::read_to_string(state_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_state(state: &BTreeMap<String, String>) -> Result<(), String> {
    let p = state_path();
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&p, serde_json::to_string_pretty(state).unwrap_or_default()).map_err(|e| e.to_string())
}

fn check_provider(provider: &str) -> Result<(), String> {
    if PROVIDERS.contains(&provider) {
        Ok(())
    } else {
        Err(format!("accounts are supported for: {}", PROVIDERS.join(", ")))
    }
}

fn check_name(name: &str) -> Result<(), String> {
    let ok = !name.trim().is_empty() && name.len() <= 64 && !name.chars().any(|c| c.is_control());
    if ok { Ok(()) } else { Err("invalid profile name".into()) }
}

fn cred_path(provider: &str) -> PathBuf {
    match provider {
        "claude" => paths::claude_config_dir().join(auth::CREDENTIALS_FILE),
        _ => paths::codex_home().join("auth.json"),
    }
}

/// `~/.claude.json` (CLAUDE_CONFIG_DIR verilmişse onun içinde).
fn claude_json_path() -> PathBuf {
    match std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir).join(".claude.json"),
        None => paths::home_dir().join(".claude.json"),
    }
}

fn read_current(provider: &str) -> Result<Stored, String> {
    let file = fs::read_to_string(cred_path(provider))
        .map_err(|e| format!("{provider} is not logged in ({}): {e}", cred_path(provider).display()))?;
    serde_json::from_str::<Value>(&file).map_err(|e| format!("credentials file is not JSON: {e}"))?;
    let account = (provider == "claude")
        .then(|| {
            fs::read_to_string(claude_json_path())
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                .and_then(|v| v.get("oauthAccount").cloned())
        })
        .flatten();
    Ok(Stored { file, account })
}

/// Etkin hesabın e-postası (profil adı önerisi).
pub fn current_email(provider: &str) -> Option<String> {
    let cur = read_current(provider).ok()?;
    match provider {
        "claude" => cur.account?.get("emailAddress")?.as_str().map(String::from),
        _ => {
            let v: Value = serde_json::from_str(&cur.file).ok()?;
            let jwt = v.pointer("/tokens/id_token")?.as_str()?;
            let payload = jwt.split('.').nth(1)?;
            let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(payload.trim_end_matches('='))
                .ok()?;
            let claims: Value = serde_json::from_slice(&bytes).ok()?;
            claims.get("email")?.as_str().map(String::from)
        }
    }
}

pub fn list() -> Vec<Profile> {
    let state = load_state();
    let mut v: Vec<Profile> = keyring::profile_list()
        .into_iter()
        .map(|(provider, name)| Profile {
            active: state.get(&provider) == Some(&name),
            provider,
            name,
        })
        .collect();
    v.sort_by(|a, b| a.provider.cmp(&b.provider).then(a.name.cmp(&b.name)));
    v
}

/// Etkin kimlik dosyasını `name` adıyla kaydeder ve onu etkin profil yapar.
pub fn save(provider: &str, name: Option<&str>) -> Result<String, String> {
    check_provider(provider)?;
    let name = match name {
        Some(n) => n.trim().to_string(),
        None => current_email(provider).ok_or("give a profile name")?,
    };
    check_name(&name)?;
    let cur = read_current(provider)?;
    let data = serde_json::to_string(&cur).map_err(|e| e.to_string())?;
    keyring::profile_store(provider, &name, &data)?;
    let mut state = load_state();
    state.insert(provider.to_string(), name.clone());
    save_state(&state)?;
    Ok(name)
}

pub fn remove(provider: &str, name: &str) -> Result<(), String> {
    check_provider(provider)?;
    keyring::profile_clear(provider, name)?;
    let mut state = load_state();
    if state.get(provider).map(String::as_str) == Some(name) {
        state.remove(provider);
        save_state(&state)?;
    }
    Ok(())
}

/// `name` profiline geçer.
pub fn switch(provider: &str, name: &str) -> Result<(), String> {
    check_provider(provider)?;
    let raw = keyring::profile_get(provider, name).ok_or_else(|| format!("no profile {provider}/{name}"))?;
    let target: Stored = serde_json::from_str(&raw).map_err(|e| format!("damaged profile: {e}"))?;
    let mut state = load_state();
    // Etkin profilin güncel token'larını kaybetme.
    if let Some(active) = state.get(provider).cloned()
        && active != name
        && let Ok(cur) = read_current(provider)
    {
        let data = serde_json::to_string(&cur).map_err(|e| e.to_string())?;
        keyring::profile_store(provider, &active, &data)?;
    }
    let path = cred_path(provider);
    if provider == "claude" {
        let dir = paths::claude_config_dir();
        let lock = acquire(&dir.join(auth::LOCK_DIR))?;
        let legacy = acquire(&auth::legacy_lock_path(&dir))?;
        auth::write_atomic(&path, target.file.as_bytes()).map_err(|e| e.to_string())?;
        drop((lock, legacy));
        if let Some(account) = target.account {
            set_oauth_account(account)?;
        }
    } else {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        auth::write_atomic(&path, target.file.as_bytes()).map_err(|e| e.to_string())?;
    }
    state.insert(provider.to_string(), name.to_string());
    save_state(&state)
}

/// Claude Code'un kilidini birkaç kez dener (o sırada token yeniliyor olabilir).
fn acquire(path: &std::path::Path) -> Result<DirLock, String> {
    for _ in 0..20 {
        match DirLock::try_acquire(path, auth::LOCK_STALE) {
            Ok(Some(l)) => return Ok(l),
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
            Err(e) => return Err(format!("lock {}: {e}", path.display())),
        }
    }
    Err("Claude Code is refreshing its token; try again in a few seconds".into())
}

/// `~/.claude.json` içinde yalnızca `oauthAccount` alanını değiştirir.
fn set_oauth_account(account: Value) -> Result<(), String> {
    let p = claude_json_path();
    let Ok(text) = fs::read_to_string(&p) else {
        return Ok(()); // dosya yoksa Claude Code ilk açılışta oluşturur
    };
    let mut v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
    let Some(obj) = v.as_object_mut() else { return Ok(()) };
    obj.insert("oauthAccount".into(), account);
    let out = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    auth::write_atomic(&p, out.as_bytes()).map_err(|e| e.to_string())
}
