//! Sağlayıcıları paralel sorgulayan ve sonuçları önceki durumla birleştiren
//! motor. Hem servis (`kota daemon`) hem de doğrudan CLI kullanır.
//!
//! Birleştirme kuralı: geçici bir hatada (ağ, hız sınırı, süresi dolmuş
//! oturum) son başarılı pencereler "eski veri" olarak korunur; sıfırlanma
//! zamanı geçmiş pencereler %0'dan başlayan tahmine çevrilir.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Local, Utc};

use crate::config::{Config, Thresholds};
use crate::forecast::History;
use crate::locallogs::{HOURS, hours_start};
use crate::model::{Activity, ProviderReport, Snapshot, Source, Status};
use crate::paths;
use crate::providers::{self, EnvKeys, FetchContext, KeyStore, Provider};

/// Bir sağlayıcının en fazla bu kadar sürmesine izin verilir.
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(60);

pub struct Engine {
    config: Config,
    providers: Vec<Arc<dyn Provider>>,
    keys: Arc<dyn KeyStore>,
    http: reqwest::Client,
    last: Option<Snapshot>,
    history: History,
    /// Günlük geçmiş: (hesaplandığı an, ilk gün, gün başına token).
    daily: Option<(DateTime<Utc>, chrono::NaiveDate, Vec<u64>)>,
}

/// Geçmiş grafiğinin gün sayısı ve yeniden hesaplama aralığı.
pub const HISTORY_DAYS: usize = 30;
const HISTORY_EVERY_MINUTES: i64 = 30;

/// Claude ve Codex yerel loglarından son `days` günün günlük token toplamı.
pub fn daily_history(now: DateTime<Utc>, days: usize) -> (chrono::NaiveDate, Vec<u64>) {
    use crate::locallogs::{claude, codex};
    let today = now.with_timezone(&Local).date_naive();
    let first = today - chrono::Duration::days(days as i64 - 1);
    let since = first
        .and_hms_opt(0, 0, 0)
        .and_then(|n| n.and_local_timezone(Local).earliest())
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or(now - chrono::Duration::days(days as i64));
    let day = |t: DateTime<Utc>| t.with_timezone(&Local).date_naive();
    let mut v = vec![0u64; days];
    let maps = [
        claude::daily_tokens(&paths::claude_config_dir().join("projects"), since, day),
        codex::daily_tokens(&paths::codex_home().join("sessions"), since, day),
    ];
    for m in maps {
        for (d, n) in m {
            let i = (d - first).num_days();
            if (0..days as i64).contains(&i) {
                v[i as usize] += n;
            }
        }
    }
    (first, v)
}

impl Engine {
    /// Anahtarlar yalnızca ortam değişkenlerinden (`KOTA_<ID>_KEY`).
    pub fn new(config: Config) -> Self {
        Self::with_keys(config, Arc::new(EnvKeys))
    }

    pub fn with_keys(config: Config, keys: Arc<dyn KeyStore>) -> Self {
        let providers = providers::build(&config, keys.clone());
        let mut e = Self::with_providers(config, providers);
        e.keys = keys;
        e
    }

    pub fn with_providers(config: Config, providers: Vec<Arc<dyn Provider>>) -> Self {
        Self {
            config,
            providers,
            keys: Arc::new(EnvKeys),
            http: crate::http::build_client(),
            last: None,
            history: History::default(),
            daily: None,
        }
    }

    /// Ayarlar değişince sağlayıcıları yeniden kurar; son değerler korunur,
    /// kapatılan sağlayıcılar anlık görüntüden çıkar.
    pub fn reconfigure(&mut self, config: Config) {
        self.providers = providers::build(&config, self.keys.clone());
        self.config = config;
        let ids = self.provider_ids();
        if let Some(last) = self.last.as_mut() {
            last.providers.retain(|p| ids.contains(&p.id.as_str()));
            last.paused = self.config.paused;
        }
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn set_history(&mut self, history: History) {
        self.history = history;
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn last(&self) -> Option<&Snapshot> {
        self.last.as_ref()
    }

    /// Önceki çalıştırmadan kalan anlık görüntüyü (ör. önbellek) taban alır.
    pub fn seed(&mut self, snapshot: Snapshot) {
        self.last = Some(snapshot);
    }

    pub fn provider_ids(&self) -> Vec<&'static str> {
        self.providers.iter().map(|p| p.id()).collect()
    }

    /// (sağlayıcı kimliği, izlenen dosya) çiftleri.
    pub fn watched_files(&self) -> Vec<(&'static str, PathBuf)> {
        self.providers
            .iter()
            .flat_map(|p| p.watched_files().into_iter().map(move |f| (p.id(), f)))
            .collect()
    }

    /// Sağlayıcıları sorgular. `only` verilirse yalnız o sağlayıcı yenilenir,
    /// diğerleri son değerlerini korur.
    pub async fn refresh(&mut self, only: Option<&str>) -> Snapshot {
        let now = Utc::now();
        let ctx = FetchContext {
            http: self.http.clone(),
            thresholds: self.config.thresholds,
            now,
            today_start: local_midnight(now),
        };
        let mut handles = Vec::new();
        for p in &self.providers {
            if only.is_some_and(|id| id != p.id()) {
                continue;
            }
            let (p, ctx) = (p.clone(), ctx.clone());
            let (id, name) = (p.id(), p.name());
            let handle = tokio::spawn(async move {
                match tokio::time::timeout(PROVIDER_TIMEOUT, p.fetch(&ctx)).await {
                    Ok(r) => r,
                    Err(_) => {
                        let mut r = ProviderReport::new(id, name);
                        r.set_status(Status::Error, "timeout", "Provider did not answer in time");
                        r
                    }
                }
            });
            handles.push((id, name, handle));
        }
        let mut fresh = Vec::new();
        for (id, name, handle) in handles {
            let report = handle.await.unwrap_or_else(|e| {
                let mut r = ProviderReport::new(id, name);
                r.set_status(Status::Error, "internal_error", format!("provider task failed: {e}"));
                r
            });
            fresh.push(report);
        }

        let th = self.config.thresholds;
        let prev = self.last.take();
        let mut reports = Vec::new();
        for p in &self.providers {
            let old = prev.as_ref().and_then(|s| s.provider(p.id()));
            let report = match fresh.iter().position(|r| r.id == p.id()) {
                Some(i) => merge(old, fresh.swap_remove(i), now, &th),
                None => match old {
                    Some(o) => carry(o.clone(), now, &th),
                    None => continue,
                },
            };
            reports.push(report);
        }
        for r in &mut reports {
            self.history.observe(r, now);
        }
        self.history.prune(now);
        let mut snap = Snapshot::new(reports, now);
        snap.activity = activity(&snap.providers, now);
        if snap.activity.is_some() {
            let today = now.with_timezone(&Local).date_naive();
            let stale = self.daily.as_ref().is_none_or(|(at, first, v)| {
                (now - *at).num_minutes() >= HISTORY_EVERY_MINUTES
                    || *first + chrono::Duration::days(v.len() as i64 - 1) != today
            });
            if stale
                && let Ok((first, v)) = tokio::task::spawn_blocking(move || daily_history(now, HISTORY_DAYS)).await
            {
                self.daily = Some((now, first, v));
            }
            if let (Some(act), Some((_, first, v))) = (snap.activity.as_mut(), self.daily.as_ref()) {
                let mut v = v.clone();
                // Bugün canlı tarayıcıdan (her yenilemede güncel).
                if let Some(last) = v.last_mut() {
                    *last = (*last).max(act.today_tokens);
                }
                act.daily = v;
                act.daily_start = Some(first.format("%Y-%m-%d").to_string());
            }
        }
        snap.paused = self.config.paused;
        if let Some(prev) = &prev {
            snap.agents = prev.agents.clone();
        }
        self.last = Some(snap.clone());
        snap
    }
}

/// Sağlayıcıların yerel log etkinliğinin toplamı.
fn activity(reports: &[ProviderReport], now: DateTime<Utc>) -> Option<Activity> {
    let mut act = Activity {
        start: hours_start(now),
        hourly: vec![0; HOURS],
        ..Activity::default()
    };
    let mut any = false;
    for l in reports.iter().filter_map(|r| r.local.as_ref()) {
        any = true;
        for (i, v) in l.hourly.iter().take(HOURS).enumerate() {
            act.hourly[i] += v;
        }
        act.sessions += l.sessions;
        act.today_tokens += l.today.total;
    }
    any.then_some(act)
}

/// Yerel saatle bugünün başlangıcı (UTC).
pub fn local_midnight(now: DateTime<Utc>) -> DateTime<Utc> {
    let local = now.with_timezone(&Local);
    local
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| naive.and_local_timezone(Local).earliest())
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or(now - chrono::Duration::hours(24))
}

/// Yeni raporu öncekiyle birleştirir.
pub fn merge(
    prev: Option<&ProviderReport>,
    mut new: ProviderReport,
    now: DateTime<Utc>,
    th: &Thresholds,
) -> ProviderReport {
    let keeps_old_data = matches!(new.status, Status::Error | Status::Expired);
    if keeps_old_data {
        if let Some(old) = prev.filter(|o| !o.windows.is_empty() && o.fetched_at.is_some()) {
            let old_is_newer = new.windows.is_empty() || new.fetched_at < old.fetched_at;
            if old_is_newer {
                new.windows = old.windows.clone();
                new.fetched_at = old.fetched_at;
                new.source = Some(Source::Cache);
                if new.details.is_empty() {
                    new.details = old.details.clone();
                }
                if new.plan.is_none() {
                    new.plan = old.plan.clone();
                }
            }
        }
        if !new.windows.is_empty() {
            new.stale = true;
        }
    }
    if new.stale {
        for w in &mut new.windows {
            w.roll_over(now, th);
        }
    }
    new
}

/// Bu turda sorgulanmayan sağlayıcının eski raporu (yalnız sıfırlanma güncellenir).
fn carry(mut r: ProviderReport, now: DateTime<Utc>, th: &Thresholds) -> ProviderReport {
    let reset_passed = r.windows.iter().any(|w| w.resets_at.is_some_and(|t| t <= now));
    if reset_passed {
        r.stale = true;
        for w in &mut r.windows {
            w.roll_over(now, th);
        }
    }
    r
}

pub fn default_cache_path() -> PathBuf {
    paths::cache_dir().join("snapshot.json")
}

/// Önbellekteki anlık görüntü; şema uyuşmuyorsa `None`.
pub fn load_cache(path: &Path) -> Option<Snapshot> {
    let text = std::fs::read_to_string(path).ok()?;
    let snap: Snapshot = serde_json::from_str(&text).ok()?;
    (snap.schema == crate::model::SCHEMA_VERSION).then_some(snap)
}

/// Anlık görüntüyü atomik olarak yazar (token içermez).
pub fn save_cache(path: &Path, snap: &Snapshot) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let data = serde_json::to_vec_pretty(snap).map_err(std::io::Error::other)?;
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::UsageWindow;
    use async_trait::async_trait;
    use chrono::Duration as CDuration;

    fn th() -> Thresholds {
        Thresholds::default()
    }

    fn ok_report(now: DateTime<Utc>, pct: f64, resets_in: CDuration) -> ProviderReport {
        let mut r = ProviderReport::new("claude", "Claude");
        r.windows.push(UsageWindow::new(
            "session",
            "s",
            pct,
            Some(now + resets_in),
            Some(18_000),
            &th(),
        ));
        r.source = Some(Source::Api);
        r.fetched_at = Some(now);
        r.plan = Some("Pro".into());
        r
    }

    #[test]
    fn error_keeps_previous_windows_as_stale() {
        let t0 = Utc::now();
        let prev = ok_report(t0, 40.0, CDuration::hours(2));
        let mut err = ProviderReport::new("claude", "Claude");
        err.set_status(Status::Error, "network", "down");
        let m = merge(Some(&prev), err, t0 + CDuration::minutes(5), &th());
        assert_eq!(m.status, Status::Error);
        assert!(m.stale);
        assert_eq!(m.source, Some(Source::Cache));
        assert_eq!(m.windows[0].used_percent, 40.0);
        assert_eq!(m.fetched_at, Some(t0));
        assert_eq!(m.plan.as_deref(), Some("Pro"));
    }

    #[test]
    fn stale_window_rolls_over_after_reset() {
        let t0 = Utc::now();
        let prev = ok_report(t0, 97.0, CDuration::hours(1));
        let mut err = ProviderReport::new("claude", "Claude");
        err.set_status(Status::Expired, "token_expired", "x");
        let m = merge(Some(&prev), err, t0 + CDuration::hours(2), &th());
        assert_eq!(m.windows[0].used_percent, 0.0);
        assert!(m.windows[0].estimated);
        assert!(m.windows[0].resets_at.unwrap() > t0 + CDuration::hours(2));
    }

    #[test]
    fn not_connected_drops_old_data() {
        let t0 = Utc::now();
        let prev = ok_report(t0, 40.0, CDuration::hours(2));
        let mut nc = ProviderReport::new("claude", "Claude");
        nc.set_status(Status::NotConnected, "no_credentials", "signed out");
        let m = merge(Some(&prev), nc, t0, &th());
        assert!(m.windows.is_empty());
        assert!(!m.stale);
    }

    #[test]
    fn newer_local_fallback_beats_older_cache() {
        let t0 = Utc::now();
        let prev = ok_report(t0, 10.0, CDuration::hours(4));
        let mut fb = ok_report(t0 + CDuration::minutes(30), 55.0, CDuration::hours(4));
        fb.status = Status::Error;
        fb.source = Some(Source::LocalLogs);
        fb.stale = true;
        let m = merge(Some(&prev), fb, t0 + CDuration::minutes(31), &th());
        assert_eq!(m.windows[0].used_percent, 55.0);
        assert_eq!(m.source, Some(Source::LocalLogs));
    }

    struct Fixed(&'static str, f64);

    #[async_trait]
    impl Provider for Fixed {
        fn id(&self) -> &'static str {
            self.0
        }
        fn name(&self) -> &'static str {
            self.0
        }
        async fn fetch(&self, ctx: &FetchContext) -> ProviderReport {
            let mut r = ProviderReport::new(self.0, self.0);
            r.windows
                .push(UsageWindow::new("session", "s", self.1, None, None, &ctx.thresholds));
            r.fetched_at = Some(ctx.now);
            r
        }
    }

    #[tokio::test]
    async fn refresh_keeps_order_and_partial_refresh_carries_others() {
        let providers: Vec<Arc<dyn Provider>> = vec![Arc::new(Fixed("a", 1.0)), Arc::new(Fixed("b", 2.0))];
        let mut e = Engine::with_providers(Config::default(), providers);
        let s1 = e.refresh(None).await;
        assert_eq!(
            s1.providers.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        let s2 = e.refresh(Some("b")).await;
        assert_eq!(s2.providers.len(), 2);
        assert_eq!(s2.providers[0].fetched_at, s1.providers[0].fetched_at, "a was carried");
    }

    #[tokio::test]
    async fn activity_sums_local_logs_and_reconfigure_drops_disabled() {
        let mut e = Engine::new(Config::default());
        let now = Utc::now();
        let mut a = ProviderReport::new("claude", "Claude");
        a.local = Some(crate::model::LocalUsage {
            hourly: vec![1; 24],
            sessions: 2,
            ..Default::default()
        });
        let mut b = ProviderReport::new("codex", "Codex");
        b.local = Some(crate::model::LocalUsage {
            hourly: vec![2; 24],
            sessions: 1,
            ..Default::default()
        });
        let act = activity(&[a.clone(), b.clone()], now).unwrap();
        assert_eq!(act.hourly, vec![3; 24]);
        assert_eq!(act.sessions, 3);
        assert!(activity(&[ProviderReport::new("x", "X")], now).is_none());

        e.seed(Snapshot::new(vec![a, b], now));
        let mut c = Config::default();
        c.set_enabled("codex", false);
        c.paused = true;
        e.reconfigure(c);
        assert_eq!(e.provider_ids(), ["claude"]);
        let last = e.last().unwrap();
        assert_eq!(last.providers.len(), 1);
        assert!(last.paused);
    }

    #[test]
    fn cache_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sub/snapshot.json");
        let snap = Snapshot::new(vec![ok_report(Utc::now(), 5.0, CDuration::hours(1))], Utc::now());
        save_cache(&p, &snap).unwrap();
        assert_eq!(load_cache(&p).unwrap(), snap);
    }

    #[test]
    fn midnight_is_before_now() {
        let now = Utc::now();
        let m = local_midnight(now);
        assert!(m <= now && now - m <= CDuration::hours(25));
    }
}
