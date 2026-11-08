//! Windows Kimlik Bilgisi Yöneticisi (Credential Manager).
//!
//! Linux sürümündeki `keyring.rs` ile aynı işlevleri sunar; `accounts.rs`
//! iki platformda da aynı dosyadır. Kimlik Bilgisi Yöneticisi kayıtları
//! listelemeye izin vermediği için profil *adları* (gizli değil) ayrıca
//! `%APPDATA%\kota\profiles.json` içinde tutulur.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use kota_core::providers::{EnvKeys, KeyStore};
use kota_core::secret::Secret;

const SERVICE: &str = "io.github.ensignalp.Kota";
const PROFILE_SERVICE: &str = "io.github.ensignalp.Kota.account";
const CACHE_TTL: Duration = Duration::from_secs(60);

#[cfg(windows)]
fn get(service: &str, user: &str) -> Option<String> {
    keyring::Entry::new(service, user).ok()?.get_password().ok()
}

#[cfg(windows)]
fn set(service: &str, user: &str, value: &str) -> Result<(), String> {
    keyring::Entry::new(service, user)
        .and_then(|e| e.set_password(value))
        .map_err(|e| format!("Credential Manager refused the value: {e}"))
}

#[cfg(windows)]
fn delete(service: &str, user: &str) -> Result<(), String> {
    match keyring::Entry::new(service, user).and_then(|e| e.delete_credential()) {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

// Windows dışı derlemeler (yalnızca geliştirme): kalıcı depo yok.
#[cfg(not(windows))]
fn get(_: &str, _: &str) -> Option<String> {
    None
}
#[cfg(not(windows))]
fn set(_: &str, _: &str, _: &str) -> Result<(), String> {
    Err("credential store is only available on Windows".into())
}
#[cfg(not(windows))]
fn delete(_: &str, _: &str) -> Result<(), String> {
    Ok(())
}

pub struct WinKeys {
    cache: Mutex<Vec<(String, Option<Secret>, Instant)>>,
}

impl WinKeys {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(Vec::new()),
        }
    }

    pub fn invalidate(&self) {
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

impl KeyStore for WinKeys {
    fn get(&self, provider: &str) -> Option<Secret> {
        if let Some(k) = EnvKeys.get(provider) {
            return Some(k);
        }
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.retain(|(_, _, at)| at.elapsed() < CACHE_TTL);
        if let Some((_, v, _)) = cache.iter().find(|(p, _, _)| p == provider) {
            return v.clone();
        }
        let v = get(SERVICE, provider).filter(|s| !s.is_empty()).map(Secret::new);
        cache.push((provider.to_string(), v.clone(), Instant::now()));
        v
    }
}

pub fn env_key(provider: &str) -> bool {
    EnvKeys.get(provider).is_some()
}

pub fn has_key(provider: &str) -> bool {
    env_key(provider) || get(SERVICE, provider).is_some_and(|s| !s.is_empty())
}

pub fn store(provider: &str, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return delete(SERVICE, provider);
    }
    set(SERVICE, provider, key)
}

// ------------------------------------------------------------ hesap profilleri

fn index_path() -> PathBuf {
    kota_core::paths::config_file().with_file_name("profiles.json")
}

fn load_index() -> Vec<(String, String)> {
    std::fs::read_to_string(index_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_index(v: &[(String, String)]) {
    let p = index_path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(p, serde_json::to_string(v).unwrap_or_default());
}

fn profile_user(provider: &str, name: &str) -> String {
    format!("{provider}/{name}")
}

pub fn profile_store(provider: &str, name: &str, data: &str) -> Result<(), String> {
    set(PROFILE_SERVICE, &profile_user(provider, name), data)?;
    let mut idx = load_index();
    let item = (provider.to_string(), name.to_string());
    if !idx.contains(&item) {
        idx.push(item);
        idx.sort();
        save_index(&idx);
    }
    Ok(())
}

pub fn profile_get(provider: &str, name: &str) -> Option<String> {
    get(PROFILE_SERVICE, &profile_user(provider, name))
}

pub fn profile_list() -> Vec<(String, String)> {
    load_index()
}

pub fn profile_clear(provider: &str, name: &str) -> Result<(), String> {
    delete(PROFILE_SERVICE, &profile_user(provider, name))?;
    let mut idx = load_index();
    idx.retain(|(p, n)| !(p == provider && n == name));
    save_index(&idx);
    Ok(())
}
