//! API anahtarları GNOME Anahtarlığı'nda (Secret Service / libsecret).
//! `secret-tool` (libsecret-tools) kullanılır; anahtar komut satırına
//! değil stdin'e yazılır, böylece `ps` çıktısında görünmez.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use kota_core::providers::{EnvKeys, KeyStore};
use kota_core::secret::Secret;

const SERVICE: &str = "io.github.ensignalp.Kota";
/// 0.5.0 öncesi kayıtlar bu adla tutuluyordu; okunurken yeni ada taşınır.
const LEGACY_SERVICE: &str = "io.github.mustazafi.Kota";
/// Anahtarlığı her sorguda çağırmamak için kısa önbellek.
const CACHE_TTL: Duration = Duration::from_secs(60);

pub struct SecretToolKeys {
    cache: Mutex<Vec<(String, Option<Secret>, Instant)>>,
}

impl SecretToolKeys {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(Vec::new()),
        }
    }

    pub fn invalidate(&self) {
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

impl KeyStore for SecretToolKeys {
    fn get(&self, provider: &str) -> Option<Secret> {
        if let Some(k) = EnvKeys.get(provider) {
            return Some(k);
        }
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.retain(|(_, _, at)| at.elapsed() < CACHE_TTL);
        if let Some((_, v, _)) = cache.iter().find(|(p, _, _)| p == provider) {
            return v.clone();
        }
        let v = lookup(provider);
        cache.push((provider.to_string(), v.clone(), Instant::now()));
        v
    }
}

/// Anahtarlık kilitliyse veya Secret Service yoksa secret-tool süresiz
/// bekleyebilir; servisi kilitlememek için süre sınırı.
const TOOL_TIMEOUT: Duration = Duration::from_secs(4);

/// secret-tool'u çalıştırır; zaman aşımında öldürür. `stderr` true ise yalnız
/// stderr okunur (stdout — gizli değerler — /dev/null'a gider). (çıktı, başarılı mı)
fn run_tool(args: &[&str], stderr: bool) -> Option<(Vec<u8>, bool)> {
    let (out, err) = if stderr {
        (Stdio::null(), Stdio::piped())
    } else {
        (Stdio::piped(), Stdio::null())
    };
    let mut child = Command::new("secret-tool")
        .args(args)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .ok()?;
    let mut stdout: Box<dyn std::io::Read + Send> = if stderr {
        Box::new(child.stderr.take()?)
    } else {
        Box::new(child.stdout.take()?)
    };
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stdout, &mut buf);
        buf
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if start.elapsed() < TOOL_TIMEOUT => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                log::warn!("secret-tool timed out (keyring locked or unavailable)");
                return None;
            }
        }
    };
    Some((reader.join().unwrap_or_default(), status.success()))
}

fn lookup_in(service: &str, provider: &str) -> Option<String> {
    let (out, ok) = run_tool(&["lookup", "service", service, "provider", provider], false)?;
    let s = String::from_utf8(out).ok()?;
    let s = s.trim();
    (ok && !s.is_empty()).then(|| s.to_string())
}

fn lookup(provider: &str) -> Option<Secret> {
    if let Some(s) = lookup_in(SERVICE, provider) {
        return Some(Secret::new(s));
    }
    // Eski ad: yeni ada taşı (başarısız olursa yine de değeri kullan).
    let old = lookup_in(LEGACY_SERVICE, provider)?;
    if store(provider, &old).is_ok() {
        let _ = Command::new("secret-tool")
            .args(["clear", "service", LEGACY_SERVICE, "provider", provider])
            .stdin(Stdio::null())
            .status();
    }
    Some(Secret::new(old))
}

pub fn env_key(provider: &str) -> bool {
    EnvKeys.get(provider).is_some()
}

pub fn has_key(provider: &str) -> bool {
    EnvKeys.get(provider).is_some() || stored_providers().contains(provider)
}

/// Anahtarı kayıtlı sağlayıcılar (tek sorgu). secret-tool öznitelikleri
/// stderr'e, gizli değeri stdout'a yazar; yalnız stderr okunur.
pub fn stored_providers() -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    for service in [SERVICE, LEGACY_SERVICE] {
        let Some((out, _)) = run_tool(&["search", "--all", "service", service], true) else {
            continue;
        };
        set.extend(
            String::from_utf8_lossy(&out)
                .lines()
                .filter_map(|l| l.strip_prefix("attribute.provider = "))
                .map(|p| p.trim().to_string()),
        );
    }
    set
}

pub fn store(provider: &str, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return clear(provider);
    }
    let mut child = Command::new("secret-tool")
        .args([
            "store",
            "--label",
            &format!("Kota: {provider} API key"),
            "service",
            SERVICE,
            "provider",
            provider,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("secret-tool could not be started (install libsecret-tools?): {e}"))?;
    child
        .stdin
        .take()
        .ok_or("no stdin")?
        .write_all(key.as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "keyring refused the key: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

pub fn clear(provider: &str) -> Result<(), String> {
    let _ = Command::new("secret-tool")
        .args(["clear", "service", LEGACY_SERVICE, "provider", provider])
        .stdin(Stdio::null())
        .status();
    let status = Command::new("secret-tool")
        .args(["clear", "service", SERVICE, "provider", provider])
        .stdin(Stdio::null())
        .status()
        .map_err(|e| e.to_string())?;
    // Kayıt yoksa da secret-tool 0 döner; başka bir hata ise bildir.
    if status.success() {
        Ok(())
    } else {
        Err(format!("secret-tool clear failed: {status}"))
    }
}

// ------------------------------------------------------------ hesap profilleri
//
// Ayrı bir `service` özniteliği: API anahtarı aramaları profilleri görmez.

const PROFILE_SERVICE: &str = "io.github.ensignalp.Kota.account";
const LEGACY_PROFILE_SERVICE: &str = "io.github.mustazafi.Kota.account";

pub fn profile_store(provider: &str, name: &str, data: &str) -> Result<(), String> {
    let mut child = Command::new("secret-tool")
        .args([
            "store",
            "--label",
            &format!("Kota: {provider} account {name}"),
            "service",
            PROFILE_SERVICE,
            "provider",
            provider,
            "profile",
            name,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("secret-tool could not be started (install libsecret-tools?): {e}"))?;
    child
        .stdin
        .take()
        .ok_or("no stdin")?
        .write_all(data.as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("keyring refused the profile: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

pub fn profile_get(provider: &str, name: &str) -> Option<String> {
    [PROFILE_SERVICE, LEGACY_PROFILE_SERVICE].iter().find_map(|service| {
        let (out, ok) = run_tool(&["lookup", "service", service, "provider", provider, "profile", name], false)?;
        let s = String::from_utf8(out).ok()?;
        (ok && !s.trim().is_empty()).then(|| s.trim().to_string())
    })
}

/// (sağlayıcı, profil adı) çiftleri; gizli değerler okunmaz.
pub fn profile_list() -> Vec<(String, String)> {
    let mut text = String::new();
    for service in [PROFILE_SERVICE, LEGACY_PROFILE_SERVICE] {
        if let Some((out, _)) = run_tool(&["search", "--all", "service", service], true) {
            text.push_str(&String::from_utf8_lossy(&out));
            text.push_str("\n[end]\n");
        }
    }
    let mut v = Vec::new();
    let (mut provider, mut profile) = (None, None);
    for l in text.lines() {
        if l.starts_with('[') {
            if let (Some(p), Some(n)) = (provider.take(), profile.take()) {
                v.push((p, n));
            }
        } else if let Some(p) = l.strip_prefix("attribute.provider = ") {
            provider = Some(p.trim().to_string());
        } else if let Some(n) = l.strip_prefix("attribute.profile = ") {
            profile = Some(n.trim().to_string());
        }
    }
    if let (Some(p), Some(n)) = (provider, profile) {
        v.push((p, n));
    }
    v.sort();
    v.dedup();
    v
}

pub fn profile_clear(provider: &str, name: &str) -> Result<(), String> {
    let _ = Command::new("secret-tool")
        .args(["clear", "service", LEGACY_PROFILE_SERVICE, "provider", provider, "profile", name])
        .stdin(Stdio::null())
        .status();
    let status = Command::new("secret-tool")
        .args(["clear", "service", PROFILE_SERVICE, "provider", provider, "profile", name])
        .stdin(Stdio::null())
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("secret-tool clear failed: {status}"))
    }
}
