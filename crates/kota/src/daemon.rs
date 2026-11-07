//! `kota daemon`: sağlayıcıları düzenli sorgulayan, çalışan ajanları izleyen
//! ve sonucu D-Bus'ta yayınlayan servis.
//!
//! - `interval_seconds`'ta bir tüm sağlayıcılar sorgulanır (duraklatılmadıysa).
//! - Giriş dosyaları ve `config.toml` 5 sn'de bir `stat` ile izlenir; değişince
//!   ilgili sağlayıcı hemen yeniden sorgulanır / ayarlar yeniden yüklenir.
//! - Ajanlar 5 sn'de bir `/proc`'tan taranır; yapı değişince yayınlanır.
//! - Son anlık görüntü ve tahmin geçmişi `~/.cache/kota/`'ya yazılır.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use chrono::Utc;
use kota_core::config::Config;
use kota_core::engine::{self, Engine};
use kota_core::forecast::History;
use kota_core::model::{Agent, Snapshot};
use kota_core::providers;
use serde::Deserialize;
use serde_json::json;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::{Mutex, RwLock};
use zbus::object_server::SignalEmitter;

use crate::accounts;
use crate::agents::{self, AgentScanner};
use crate::connect;
use crate::dbus::{BUS_NAME, OBJECT_PATH};
use crate::keyring::{self, SecretToolKeys};

const WATCH_TICK: Duration = Duration::from_secs(5);
/// Bu süre içinde tekrarlanan `Refresh()` çağrıları ağa gitmez.
const MANUAL_DEBOUNCE: Duration = Duration::from_secs(10);
/// Ajan "çalışıyor" sayılır: son CPU etkinliği bu kadar yeniyse.
const ACTIVE_SECS: i64 = 90;

type Stamp = Option<(SystemTime, u64)>;

fn stamp(p: &PathBuf) -> Stamp {
    std::fs::metadata(p)
        .ok()
        .map(|m| (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len()))
}

struct Shared {
    engine: Mutex<Engine>,
    keys: Arc<SecretToolKeys>,
    latest: RwLock<Option<Snapshot>>,
    agents: std::sync::Mutex<Vec<Agent>>,
    scanner: std::sync::Mutex<AgentScanner>,
    cache_path: PathBuf,
    history_path: PathBuf,
    config_path: PathBuf,
    config_stamp: std::sync::Mutex<Stamp>,
    watched: std::sync::Mutex<Vec<(&'static str, PathBuf)>>,
    stamps: std::sync::Mutex<HashMap<PathBuf, Stamp>>,
    last_done: std::sync::Mutex<Option<Instant>>,
}

fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Shared {
    fn snapshot_json(&self, snap: &Snapshot) -> String {
        let mut s = snap.clone();
        s.agents = lock(&self.agents).clone();
        serde_json::to_string(&s).unwrap_or_else(|_| "{}".to_string())
    }

    async fn current_json(&self) -> String {
        if let Some(s) = self.latest.read().await.clone() {
            return self.snapshot_json(&s);
        }
        self.refresh(None).await
    }

    async fn refresh(&self, only: Option<&str>) -> String {
        let mut eng = self.engine.lock().await;
        self.refresh_locked(&mut eng, only).await
    }

    async fn refresh_locked(&self, eng: &mut Engine, only: Option<&str>) -> String {
        let snap = eng.refresh(only).await;
        if let Err(e) = engine::save_cache(&self.cache_path, &snap) {
            log::warn!("cache write failed ({}): {e}", self.cache_path.display());
        }
        if let Ok(h) = serde_json::to_vec(eng.history()) {
            let _ = std::fs::write(&self.history_path, h);
        }
        *self.latest.write().await = Some(snap.clone());
        // Kendi token yazımımız "dosya değişti" diye ikinci sorgu tetiklemesin.
        let watched = lock(&self.watched).clone();
        *lock(&self.stamps) = watched.iter().map(|(_, p)| (p.clone(), stamp(p))).collect();
        *lock(&self.last_done) = Some(Instant::now());
        for p in &snap.providers {
            log::info!(
                "{}: {:?}{} {}",
                p.id,
                p.status,
                p.code.as_deref().map(|c| format!(" ({c})")).unwrap_or_default(),
                p.windows
                    .iter()
                    .map(|w| format!("{}={:.0}%", w.id, w.used_percent))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
        self.snapshot_json(&snap)
    }

    async fn refresh_manual(&self) -> (String, bool) {
        let mut eng = self.engine.lock().await;
        let recent = lock(&self.last_done).is_some_and(|t| t.elapsed() < MANUAL_DEBOUNCE);
        if recent && let Some(s) = self.latest.read().await.clone() {
            return (self.snapshot_json(&s), false);
        }
        (self.refresh_locked(&mut eng, None).await, true)
    }

    /// Ayarı değiştirip kaydeder, motoru yeniden kurar ve (gerekirse) sorgular.
    async fn update_config(&self, refresh: Option<&str>, f: impl FnOnce(&mut Config)) -> zbus::fdo::Result<String> {
        let mut eng = self.engine.lock().await;
        let mut cfg = eng.config().clone();
        f(&mut cfg);
        cfg.save()
            .map_err(|e| zbus::fdo::Error::Failed(format!("config write failed: {e}")))?;
        *lock(&self.config_stamp) = stamp(&self.config_path);
        eng.reconfigure(cfg);
        *lock(&self.watched) = eng.watched_files();
        let json = match refresh {
            Some("*") => self.refresh_locked(&mut eng, None).await,
            Some(id) if eng.provider_ids().contains(&id) => self.refresh_locked(&mut eng, Some(id)).await,
            _ => {
                let snap = eng
                    .last()
                    .cloned()
                    .unwrap_or_else(|| Snapshot::new(Vec::new(), Utc::now()));
                *self.latest.write().await = Some(snap.clone());
                self.snapshot_json(&snap)
            }
        };
        Ok(json)
    }

    /// config.toml dışarıdan (ör. `kota enable`) değiştiyse yeniden yükle.
    async fn reload_config_if_changed(&self) -> Option<String> {
        let now = stamp(&self.config_path);
        if *lock(&self.config_stamp) == now {
            return None;
        }
        *lock(&self.config_stamp) = now;
        let cfg = match Config::load_from(&self.config_path) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("config reload skipped: {e}");
                return None;
            }
        };
        log::info!("config changed on disk, reloading");
        let mut eng = self.engine.lock().await;
        eng.reconfigure(cfg);
        *lock(&self.watched) = eng.watched_files();
        Some(self.refresh_locked(&mut eng, None).await)
    }

    fn changed_providers(&self) -> Vec<&'static str> {
        let watched = lock(&self.watched).clone();
        let old = lock(&self.stamps);
        let mut ids: Vec<&'static str> = watched
            .iter()
            .filter(|(_, path)| old.get(path).copied().flatten() != stamp(path))
            .map(|(id, _)| *id)
            .collect();
        ids.dedup();
        ids
    }

    /// Ajanları tarar; görünür bir değişiklik varsa `true`.
    fn scan_agents(&self) -> bool {
        let now = Utc::now();
        let fresh = lock(&self.scanner).scan(now);
        let mut cur = lock(&self.agents);
        let active = |list: &[Agent]| -> Vec<bool> {
            list.iter()
                .map(|a| a.active_at.is_some_and(|t| (now - t).num_seconds() < ACTIVE_SECS))
                .collect()
        };
        let changed = agents::structurally_differs(&cur, &fresh) || active(&cur) != active(&fresh);
        *cur = fresh;
        changed
    }
}

#[derive(Deserialize, Default)]
struct SettingsPatch {
    interval_seconds: Option<u64>,
    warning: Option<f64>,
    critical: Option<f64>,
    paused: Option<bool>,
    claude_refresh_tokens: Option<bool>,
    local_logs: Option<bool>,
}

pub struct KotaService {
    shared: Arc<Shared>,
}

async fn emit(emitter: &SignalEmitter<'_>, json: &str) {
    if let Err(e) = KotaService::updated(emitter, json).await {
        log::warn!("Updated signal failed: {e}");
    }
}

#[zbus::interface(name = "io.github.ensignalp.Kota1")]
impl KotaService {
    /// Son anlık görüntü (JSON). Henüz yoksa ilk sorgu beklenir.
    async fn get_snapshot(&self) -> String {
        self.shared.current_json().await
    }

    /// Sağlayıcıları şimdi sorgular ve yeni anlık görüntüyü döndürür.
    async fn refresh(&self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) -> String {
        let (json, fresh) = self.shared.refresh_manual().await;
        if fresh {
            emit(&emitter, &json).await;
        }
        json
    }

    /// Sağlayıcıya giriş için terminal açar.
    async fn connect(&self, provider: &str) -> zbus::fdo::Result<()> {
        connect::open_login_terminal(provider).map_err(zbus::fdo::Error::Failed)
    }

    /// Hesap profilleri (JSON): `[{provider, name, active}]` + etkin e-postalar.
    async fn list_accounts(&self) -> String {
        tokio::task::spawn_blocking(|| {
            let current: serde_json::Map<String, serde_json::Value> = accounts::PROVIDERS
                .iter()
                .map(|p| (p.to_string(), json!(accounts::current_email(p))))
                .collect();
            json!({"profiles": accounts::list(), "current": current}).to_string()
        })
        .await
        .unwrap_or_else(|_| "{}".into())
    }

    /// Etkin hesabı profil olarak kaydeder (boş ad: e-posta). Dönüş: profil adı.
    async fn save_account(&self, provider: &str, name: &str) -> zbus::fdo::Result<String> {
        let (p, n) = (provider.to_string(), name.trim().to_string());
        tokio::task::spawn_blocking(move || accounts::save(&p, (!n.is_empty()).then_some(n.as_str())))
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?
            .map_err(zbus::fdo::Error::Failed)
    }

    async fn remove_account(&self, provider: &str, name: &str) -> zbus::fdo::Result<()> {
        let (p, n) = (provider.to_string(), name.to_string());
        tokio::task::spawn_blocking(move || accounts::remove(&p, &n))
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?
            .map_err(zbus::fdo::Error::Failed)
    }

    /// Profile geçer ve sağlayıcıyı yeniden sorgular.
    async fn switch_account(
        &self,
        provider: &str,
        name: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let (p, n) = (provider.to_string(), name.to_string());
        tokio::task::spawn_blocking(move || accounts::switch(&p, &n))
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?
            .map_err(zbus::fdo::Error::Failed)?;
        let id = accounts::PROVIDERS.iter().find(|x| **x == provider).copied();
        let json = self.shared.update_config(id, |_| {}).await?;
        emit(&emitter, &json).await;
        Ok(())
    }

    /// Desteklenen sağlayıcılar + açık mı + anahtarı var mı (anahtarın kendisi asla).
    async fn list_providers(&self) -> String {
        let cfg = self.shared.engine.lock().await.config().clone();
        let stored = tokio::task::spawn_blocking(keyring::stored_providers)
            .await
            .unwrap_or_default();
        let list: Vec<_> = providers::catalog()
            .into_iter()
            .map(|e| {
                let needs_key = matches!(
                    e.auth,
                    providers::AuthKind::ApiKey | providers::AuthKind::AdminKey | providers::AuthKind::GithubToken
                );
                json!({
                    "id": e.id, "name": e.name, "auth": e.auth, "shows": e.shows, "key_url": e.key_url,
                    "budget_supported": e.budget, "budget": cfg.budget(e.id),
                    "login_command": e.login_command, "enabled": cfg.is_enabled(e.id),
                    "has_key": needs_key && (stored.contains(e.id) || keyring::env_key(e.id)),
                })
            })
            .collect();
        serde_json::to_string(&list).unwrap_or_else(|_| "[]".into())
    }

    async fn set_provider_enabled(
        &self,
        id: &str,
        enabled: bool,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        if !providers::catalog().iter().any(|e| e.id == id) {
            return Err(zbus::fdo::Error::InvalidArgs(format!("unknown provider {id}")));
        }
        let json = self
            .shared
            .update_config(enabled.then_some(id), |c| c.set_enabled(id, enabled))
            .await?;
        emit(&emitter, &json).await;
        Ok(())
    }

    /// Anahtarı anahtarlığa yazar (boş metin siler) ve sağlayıcıyı yeniden sorgular.
    async fn set_api_key(
        &self,
        id: &str,
        key: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        if !providers::catalog().iter().any(|e| e.id == id) {
            return Err(zbus::fdo::Error::InvalidArgs(format!("unknown provider {id}")));
        }
        keyring::store(id, key).map_err(zbus::fdo::Error::Failed)?;
        self.shared.keys.invalidate();
        let json = self.shared.update_config(Some(id), |_| {}).await?;
        emit(&emitter, &json).await;
        Ok(())
    }

    /// Aylık bütçe; 0 veya negatif kaldırır.
    async fn set_budget(
        &self,
        id: &str,
        amount: f64,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let json = self
            .shared
            .update_config(Some(id), |c| c.set_budget(id, Some(amount)))
            .await?;
        emit(&emitter, &json).await;
        Ok(())
    }

    /// Servis ayarları (JSON).
    async fn get_settings(&self) -> String {
        let c = self.shared.engine.lock().await.config().clone();
        json!({
            "interval_seconds": c.interval_seconds, "warning": c.thresholds.warning,
            "critical": c.thresholds.critical, "paused": c.paused,
            "claude_refresh_tokens": c.claude.refresh_tokens,
            "local_logs": c.claude.local_logs && c.codex.local_logs,
            "config_path": kota_core::paths::config_file(),
        })
        .to_string()
    }

    /// Kısmi ayar güncellemesi (JSON; yalnızca verilen alanlar değişir).
    async fn set_settings(
        &self,
        patch: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let p: SettingsPatch = serde_json::from_str(patch).map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let needs_refresh = p.warning.is_some() || p.critical.is_some() || p.local_logs.is_some();
        let json = self
            .shared
            .update_config(needs_refresh.then_some("*"), |c| {
                if let Some(v) = p.interval_seconds {
                    c.interval_seconds = v.clamp(kota_core::config::MIN_INTERVAL_SECS, 3600);
                }
                if let Some(v) = p.warning {
                    c.thresholds.warning = v.clamp(1.0, 100.0);
                }
                if let Some(v) = p.critical {
                    c.thresholds.critical = v.clamp(1.0, 100.0);
                }
                if let Some(v) = p.paused {
                    c.paused = v;
                }
                if let Some(v) = p.claude_refresh_tokens {
                    c.claude.refresh_tokens = v;
                }
                if let Some(v) = p.local_logs {
                    c.claude.local_logs = v;
                    c.codex.local_logs = v;
                }
            })
            .await?;
        emit(&emitter, &json).await;
        Ok(())
    }

    /// Bulunan bir ajan sürecine SIGTERM gönderir.
    async fn stop_agent(&self, pid: u32, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) -> zbus::fdo::Result<()> {
        let known = lock(&self.shared.agents).clone();
        agents::stop(pid, &known).map_err(zbus::fdo::Error::Failed)?;
        tokio::time::sleep(Duration::from_millis(600)).await;
        self.shared.scan_agents();
        emit(&emitter, &self.shared.current_json().await).await;
        Ok(())
    }

    #[zbus(property)]
    async fn version(&self) -> String {
        kota_core::VERSION.to_string()
    }

    #[zbus(signal)]
    async fn updated(emitter: &SignalEmitter<'_>, snapshot: &str) -> zbus::Result<()>;
}

pub async fn run() -> Result<(), String> {
    let config_path = kota_core::paths::config_file();
    let (config, warning) = Config::load();
    if let Some(w) = warning {
        log::warn!("config file ignored, using defaults: {w}");
    }
    let keys = Arc::new(SecretToolKeys::new());
    let mut eng = Engine::with_keys(config, keys.clone());
    let cache_path = engine::default_cache_path();
    let history_path = cache_path.with_file_name("history.json");
    let mut initial = None;
    if let Some(snap) = engine::load_cache(&cache_path) {
        initial = Some(snap.clone());
        eng.seed(snap);
    }
    if let Some(h) = std::fs::read(&history_path)
        .ok()
        .and_then(|b| serde_json::from_slice::<History>(&b).ok())
    {
        eng.set_history(h);
    }
    let watched = eng.watched_files();
    log::info!(
        "kota {} starting: providers [{}], every {}s{}",
        kota_core::VERSION,
        eng.provider_ids().join(", "),
        eng.config().interval().as_secs(),
        if eng.config().paused { " (paused)" } else { "" }
    );
    let shared = Arc::new(Shared {
        engine: Mutex::new(eng),
        keys,
        latest: RwLock::new(initial),
        agents: std::sync::Mutex::new(Vec::new()),
        scanner: std::sync::Mutex::new(AgentScanner::new()),
        cache_path,
        history_path,
        config_stamp: std::sync::Mutex::new(stamp(&config_path)),
        config_path,
        stamps: std::sync::Mutex::new(watched.iter().map(|(_, p)| (p.clone(), stamp(p))).collect()),
        watched: std::sync::Mutex::new(watched),
        last_done: std::sync::Mutex::new(None),
    });
    shared.scan_agents();

    let conn = zbus::connection::Builder::session()
        .name(BUS_NAME)
        .serve_at(OBJECT_PATH, KotaService { shared: shared.clone() })
        .build()
        .await
        .map_err(|e| format!("cannot own {BUS_NAME} on the session bus (already running?): {e}"))?;
    let iface = conn
        .object_server()
        .interface::<_, KotaService>(OBJECT_PATH)
        .await
        .map_err(|e| format!("interface lookup failed: {e}"))?;
    log::info!("listening on {BUS_NAME}");

    emit(iface.signal_emitter(), &shared.refresh(None).await).await;
    let mut last_full = Utc::now();
    let mut tick = tokio::time::interval(WATCH_TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut sigterm = signal(SignalKind::terminate()).map_err(|e| e.to_string())?;

    loop {
        tokio::select! {
            _ = tick.tick() => {
                if let Some(json) = shared.reload_config_if_changed().await {
                    emit(iface.signal_emitter(), &json).await;
                    last_full = Utc::now();
                    continue;
                }
                let (paused, interval) = {
                    let eng = shared.engine.lock().await;
                    (eng.config().paused, eng.config().interval())
                };
                let step = chrono::Duration::from_std(interval).unwrap_or(chrono::Duration::seconds(120));
                // Duvar saati: bilgisayar uykudan uyanınca gecikmiş sorgu hemen yapılır.
                if !paused && Utc::now() >= last_full + step {
                    emit(iface.signal_emitter(), &shared.refresh(None).await).await;
                    last_full = Utc::now();
                    continue;
                }
                let changed = shared.changed_providers();
                if !changed.is_empty() {
                    log::info!("credentials changed: {}", changed.join(", "));
                    let mut json = None;
                    for id in changed {
                        json = Some(shared.refresh(Some(id)).await);
                    }
                    if let Some(j) = json {
                        emit(iface.signal_emitter(), &j).await;
                    }
                    continue;
                }
                if shared.scan_agents() {
                    emit(iface.signal_emitter(), &shared.current_json().await).await;
                }
            }
            _ = tokio::signal::ctrl_c() => break,
            _ = sigterm.recv() => break,
        }
    }
    log::info!("stopping");
    Ok(())
}
