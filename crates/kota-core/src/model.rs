//! Arayüzlerin (GNOME paneli, CLI, Waybar) okuduğu veri modeli.
//!
//! JSON şeması `schema` alanıyla sürümlenir. Alan eklemek geriye uyumludur;
//! alan silmek veya anlamını değiştirmek `SCHEMA_VERSION` artırmayı gerektirir.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::Thresholds;

pub const SCHEMA_VERSION: u32 = 1;

/// Tüm sağlayıcıların anlık durumu.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub schema: u32,
    pub version: String,
    pub generated_at: DateTime<Utc>,
    pub providers: Vec<ProviderReport>,
    /// Son 24 saatin saatlik token etkinliği (yerel loglardan).
    #[serde(default)]
    pub activity: Option<Activity>,
    /// Çalışan AI ajanları (yalnızca servis doldurur).
    #[serde(default)]
    pub agents: Vec<Agent>,
    /// Düzenli sorgu duraklatıldı.
    #[serde(default)]
    pub paused: bool,
}

impl Snapshot {
    pub fn new(providers: Vec<ProviderReport>, now: DateTime<Utc>) -> Self {
        Self {
            schema: SCHEMA_VERSION,
            version: crate::VERSION.to_string(),
            generated_at: now,
            providers,
            activity: None,
            agents: Vec::new(),
            paused: false,
        }
    }

    pub fn provider(&self, id: &str) -> Option<&ProviderReport> {
        self.providers.iter().find(|p| p.id == id)
    }
}

/// Sağlayıcının bağlantı durumu.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Veri canlı olarak okundu.
    Ok,
    /// Giriş bilgisi yok: "Bağlan" gösterilir.
    NotConnected,
    /// Oturum süresi doldu veya reddedildi: "Yeniden bağlan" gösterilir.
    Expired,
    /// Geçici hata (ağ, 5xx, hız sınırı, beklenmeyen yanıt). Varsa eski veri korunur.
    Error,
    /// Bu giriş türü desteklenmiyor (ör. Codex API-key modu).
    Unsupported,
}

/// Pencere verisinin nereden geldiği.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Api,
    LocalLogs,
    Cache,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    #[default]
    Normal,
    Warning,
    Critical,
}

/// Bir kota penceresi (5 saatlik oturum, haftalık, 30 günlük, …).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageWindow {
    /// Kararlı kimlik: `session`, `weekly`, `weekly_opus`, `monthly`, … Arayüz
    /// etiketi bununla yerelleştirir; bilinmeyen kimliklerde `label` kullanılır.
    pub id: String,
    /// İngilizce varsayılan etiket.
    pub label: String,
    /// Kullanılan yüzde (0–100; sağlayıcı 100'ü aşan değer dönebilir).
    pub used_percent: f64,
    #[serde(default)]
    pub resets_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub window_seconds: Option<u64>,
    #[serde(default)]
    pub severity: Severity,
    /// `true` ise değer ölçülmedi, tahmin edildi (ör. eski verinin sıfırlanma
    /// zamanı geçti, pencerenin %0'dan başladığı varsayıldı).
    #[serde(default)]
    pub estimated: bool,
    /// Son kullanım hızıyla pencerenin %100'e ulaşacağı tahmini an
    /// (sıfırlanmadan önce dolmayacaksa `None`).
    #[serde(default)]
    pub exhausts_at: Option<DateTime<Utc>>,
}

impl UsageWindow {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        used_percent: f64,
        resets_at: Option<DateTime<Utc>>,
        window_seconds: Option<u64>,
        thresholds: &Thresholds,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            used_percent,
            resets_at,
            window_seconds,
            severity: thresholds.classify(used_percent),
            estimated: false,
            exhausts_at: None,
        }
    }

    /// Sıfırlanma zamanı geçmiş eski bir pencereyi bir sonraki döneme taşır.
    /// Dönüş: pencere değiştiyse `true`.
    pub fn roll_over(&mut self, now: DateTime<Utc>, thresholds: &Thresholds) -> bool {
        let Some(reset) = self.resets_at else {
            return false;
        };
        if reset > now {
            return false;
        }
        self.used_percent = 0.0;
        self.estimated = true;
        self.exhausts_at = None;
        self.severity = thresholds.classify(0.0);
        self.resets_at = self.window_seconds.filter(|s| *s > 0).map(|secs| {
            let step = secs as i64;
            let elapsed = (now - reset).num_seconds().max(0);
            reset + chrono::Duration::seconds((elapsed / step + 1) * step)
        });
        true
    }
}

/// Token toplamları. `total` her zaman bileşenlerin toplamıdır.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenTotals {
    /// Önbellekten okunmayan girdi token'ları.
    pub input: u64,
    pub output: u64,
    pub cache_write: u64,
    pub cache_read: u64,
    pub total: u64,
}

impl TokenTotals {
    pub fn new(input: u64, output: u64, cache_write: u64, cache_read: u64) -> Self {
        Self {
            input,
            output,
            cache_write,
            cache_read,
            total: input
                .saturating_add(output)
                .saturating_add(cache_write)
                .saturating_add(cache_read),
        }
    }

    pub fn add(&mut self, other: &TokenTotals) {
        *self = Self::new(
            self.input.saturating_add(other.input),
            self.output.saturating_add(other.output),
            self.cache_write.saturating_add(other.cache_write),
            self.cache_read.saturating_add(other.cache_read),
        );
    }

    /// Kümülatif sayaç farkı (sayaç sıfırlanırsa negatif yerine sıfır).
    pub fn saturating_sub(&self, other: &TokenTotals) -> Self {
        Self::new(
            self.input.saturating_sub(other.input),
            self.output.saturating_sub(other.output),
            self.cache_write.saturating_sub(other.cache_write),
            self.cache_read.saturating_sub(other.cache_read),
        )
    }

    pub fn is_zero(&self) -> bool {
        self.total == 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelTokens {
    pub model: String,
    pub total: u64,
}

/// Yerel JSONL loglarından çıkarılan kullanım (ağ gerektirmez).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LocalUsage {
    /// Yerel saatle bugün (gece yarısından beri).
    pub today: TokenTotals,
    /// Geçerli oturum penceresi içinde (biliniyorsa).
    #[serde(default)]
    pub window: Option<TokenTotals>,
    #[serde(default)]
    pub window_start: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_activity: Option<DateTime<Utc>>,
    /// Bugün en çok token harcayan modeller (en fazla 3).
    #[serde(default)]
    pub top_models: Vec<ModelTokens>,
    /// Son 24 saatin saatlik toplam token'ı; eskiden yeniye, son eleman şu anki saat.
    #[serde(default)]
    pub hourly: Vec<u64>,
    /// Son 24 saatte etkinliği olan oturum (log dosyası) sayısı.
    #[serde(default)]
    pub sessions: u32,
}

/// Saatlik etkinlik özeti (tüm sağlayıcıların yerel logları toplamı).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Activity {
    /// `hourly[0]`'ın başladığı saat.
    pub start: DateTime<Utc>,
    pub hourly: Vec<u64>,
    pub sessions: u32,
    pub today_tokens: u64,
    /// Son `daily.len()` günün yerel gün başına token toplamı; son eleman bugün.
    #[serde(default)]
    pub daily: Vec<u64>,
    /// `daily[0]`ın yerel tarihi (YYYY-MM-DD).
    #[serde(default)]
    pub daily_start: Option<String>,
}

/// Bakiye veya harcama (API anahtarlı sağlayıcılar).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Balance {
    /// `balance`: kalan bakiye · `spend`: bu ayki harcama
    pub kind: String,
    pub amount: f64,
    /// ISO para birimi (`USD`, `CNY`) veya `credits`.
    pub currency: String,
    /// Aylık bütçe (yalnızca `spend`; yüzde penceresi buradan üretilir).
    #[serde(default)]
    pub budget: Option<f64>,
}

/// Çalışan bir AI ajanı süreci.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Agent {
    pub pid: u32,
    /// `claude`, `codex`, `gemini`, `cursor`, `aider`, …
    pub kind: String,
    pub name: String,
    pub cwd: String,
    /// Git kökü varsa onun adı, yoksa dizin adı.
    pub project: String,
    pub project_path: String,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    /// Son CPU etkinliği (boşta süresi buradan hesaplanır).
    #[serde(default)]
    pub active_at: Option<DateTime<Utc>>,
    /// Süreç ağacının toplam bellek kullanımı.
    pub rss_bytes: u64,
    /// Süreç ağacının dinlediği TCP portları (geliştirme sunucuları).
    #[serde(default)]
    pub ports: Vec<u16>,
    /// Alt süreç sayısı.
    #[serde(default)]
    pub children: u32,
    /// Proje klasöründe üzerinde çalışılan alt projeler (en son dokunulan önce).
    #[serde(default)]
    pub focus: Vec<Focus>,
    /// Bu oturumda harcanan toplam token (oturum dökümü bulunabildiyse).
    #[serde(default)]
    pub tokens: Option<u64>,
}

/// Ajanın çalıştığı alt proje (ör. `~/YAZILIM` içindeki `kota`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Focus {
    pub name: String,
    pub path: String,
    /// Son dokunuş anı.
    #[serde(default)]
    pub at: Option<DateTime<Utc>>,
    /// Kaç kez dokunuldu (son döküm penceresinde).
    #[serde(default)]
    pub hits: u32,
}

/// Panelde ek satır olarak gösterilen serbest bilgi.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Detail {
    pub id: String,
    pub label: String,
    pub value: String,
}

/// Tek sağlayıcının raporu.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderReport {
    pub id: String,
    pub name: String,
    pub status: Status,
    /// Makine okunur neden: `no_credentials`, `token_expired`,
    /// `relogin_required`, `rate_limited`, `network`, `http_error`,
    /// `parse_error`, `refresh_busy`, `refresh_failed`, `api_key_mode`, …
    #[serde(default)]
    pub code: Option<String>,
    /// İngilizce açıklama (arayüz `code`'u yerelleştiremezse gösterir).
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub source: Option<Source>,
    /// Pencerelerin ölçüldüğü an.
    #[serde(default)]
    pub fetched_at: Option<DateTime<Utc>>,
    /// Pencereler son başarılı okumadan kalma (canlı değil).
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub windows: Vec<UsageWindow>,
    #[serde(default)]
    pub local: Option<LocalUsage>,
    #[serde(default)]
    pub details: Vec<Detail>,
    #[serde(default)]
    pub balance: Option<Balance>,
    /// Bağlanmak için çalıştırılacak komut (ör. `["claude","auth","login"]`).
    #[serde(default)]
    pub login_command: Option<Vec<String>>,
}

impl ProviderReport {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            status: Status::Ok,
            code: None,
            message: None,
            plan: None,
            source: None,
            fetched_at: None,
            stale: false,
            windows: Vec::new(),
            local: None,
            details: Vec::new(),
            balance: None,
            login_command: None,
        }
    }

    pub fn set_status(&mut self, status: Status, code: &str, message: impl Into<String>) {
        self.status = status;
        self.code = Some(code.to_string());
        self.message = Some(message.into());
    }

    /// En çok dolmuş pencere: panelde gösterilen değer.
    pub fn headline(&self) -> Option<&UsageWindow> {
        self.windows
            .iter()
            .max_by(|a, b| a.used_percent.total_cmp(&b.used_percent))
    }

    pub fn severity(&self) -> Severity {
        self.windows.iter().map(|w| w.severity).max().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn th() -> Thresholds {
        Thresholds::default()
    }

    #[test]
    fn severity_thresholds() {
        let t = th();
        assert_eq!(
            UsageWindow::new("a", "a", 79.9, None, None, &t).severity,
            Severity::Normal
        );
        assert_eq!(
            UsageWindow::new("a", "a", 80.0, None, None, &t).severity,
            Severity::Warning
        );
        assert_eq!(
            UsageWindow::new("a", "a", 90.0, None, None, &t).severity,
            Severity::Critical
        );
    }

    #[test]
    fn roll_over_moves_to_next_period() {
        let t = th();
        let reset = Utc.with_ymd_and_hms(2026, 11, 6, 10, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 11, 6, 21, 0, 0).unwrap();
        let mut w = UsageWindow::new("session", "s", 95.0, Some(reset), Some(5 * 3600), &t);
        assert!(w.roll_over(now, &t));
        assert_eq!(w.used_percent, 0.0);
        assert!(w.estimated);
        assert_eq!(w.severity, Severity::Normal);
        // 10:00 + 3×5 sa = 01:00 ertesi gün (ilk > 21:00 olan sınır)
        assert_eq!(w.resets_at, Some(Utc.with_ymd_and_hms(2026, 11, 7, 1, 0, 0).unwrap()));
        // Sıfırlanma geçmemişse dokunulmaz.
        let mut fresh = UsageWindow::new("s", "s", 50.0, Some(now + chrono::Duration::hours(1)), None, &t);
        assert!(!fresh.roll_over(now, &t));
        assert_eq!(fresh.used_percent, 50.0);
    }

    #[test]
    fn headline_picks_max() {
        let t = th();
        let mut r = ProviderReport::new("x", "X");
        r.windows.push(UsageWindow::new("a", "a", 10.0, None, None, &t));
        r.windows.push(UsageWindow::new("b", "b", 92.0, None, None, &t));
        assert_eq!(r.headline().unwrap().id, "b");
        assert_eq!(r.severity(), Severity::Critical);
    }

    #[test]
    fn token_totals_math() {
        let mut a = TokenTotals::new(1, 2, 3, 4);
        assert_eq!(a.total, 10);
        a.add(&TokenTotals::new(1, 1, 1, 1));
        assert_eq!(a.total, 14);
        assert_eq!(TokenTotals::new(1, 1, 1, 1).saturating_sub(&a).total, 0);
    }
}
