//! CLI için küçük TR/EN metin tablosu. Dil: `KOTA_LANG`, `LANGUAGE`,
//! `LC_ALL`, `LC_MESSAGES`, `LANG` sırasıyla; `tr` ile başlıyorsa Türkçe.

use chrono::{DateTime, Datelike, Duration, FixedOffset, Utc};
use kota_core::model::{ProviderReport, Status, UsageWindow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Tr,
    En,
}

const TR_MONTHS: [&str; 12] = [
    "Oca", "Şub", "Mar", "Nis", "May", "Haz", "Tem", "Ağu", "Eyl", "Eki", "Kas", "Ara",
];
const EN_MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const TR_DAYS: [&str; 7] = ["Pzt", "Sal", "Çar", "Per", "Cum", "Cmt", "Paz"];
const EN_DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

impl Lang {
    /// Varsayılan Türkçe; `KOTA_LANG=en` ile İngilizce.
    pub fn detect() -> Self {
        match std::env::var("KOTA_LANG").map(|v| v.trim().to_lowercase()) {
            Ok(v) if v.starts_with("en") => return Lang::En,
            Ok(v) if v.starts_with("tr") => return Lang::Tr,
            _ => {}
        }
        Lang::Tr
    }

    /// Sistem diline göre (testler ve gelecekteki "auto" için).
    #[allow(dead_code)]
    pub fn from_system() -> Self {
        for key in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
            let Ok(v) = std::env::var(key) else { continue };
            let v = v.trim().to_lowercase();
            if v.is_empty() || v == "c" || v == "posix" || v.starts_with("c.") {
                continue;
            }
            return if v.starts_with("tr") { Lang::Tr } else { Lang::En };
        }
        Lang::En
    }

    fn tr(self) -> bool {
        self == Lang::Tr
    }

    /// Türkçede yüzde işareti sayıdan önce gelir.
    pub fn percent(self, value: f64, estimated: bool) -> String {
        let n = format!("{value:.0}");
        let tilde = if estimated { "~" } else { "" };
        if self.tr() {
            format!("{tilde}%{n}")
        } else {
            format!("{tilde}{n}%")
        }
    }

    pub fn window_label(self, w: &UsageWindow) -> String {
        if !self.tr() {
            return w.label.clone();
        }
        if let Some(rest) = w.id.strip_prefix("code_review:") {
            return format!("Kod inceleme · {}", tr_base_label(rest, &w.label));
        }
        if let Some(rest) = w.id.strip_prefix("extra:") {
            let name = w.label.split(" · ").next().unwrap_or(&w.label);
            let inner = rest.rsplit(':').next().unwrap_or(rest);
            return format!("{name} · {}", tr_base_label(inner, &w.label));
        }
        tr_base_label(&w.id, &w.label)
    }

    pub fn status(self, p: &ProviderReport) -> String {
        let code = p.code.as_deref().unwrap_or("");
        let s = match (p.status, code, self.tr()) {
            (Status::Ok, _, true) => "bağlı",
            (Status::Ok, _, false) => "connected",
            (Status::NotConnected, _, true) => "bağlı değil",
            (Status::NotConnected, _, false) => "not connected",
            (Status::Expired, "relogin_required" | "token_rejected", true) => "yeniden giriş gerekli",
            (Status::Expired, "relogin_required" | "token_rejected", false) => "sign-in required",
            (Status::Expired, _, true) => "oturum süresi doldu",
            (Status::Expired, _, false) => "session expired",
            (Status::Unsupported, "api_key_mode", true) => "API anahtarı modu: plan limiti yok",
            (Status::Unsupported, "api_key_mode", false) => "API key mode: no plan limits",
            (Status::Unsupported, "ineligible", true) => "hesap bu hizmete uygun değil",
            (Status::Unsupported, "ineligible", false) => "account not eligible",
            (Status::Unsupported, _, true) => "desteklenmiyor",
            (Status::Unsupported, _, false) => "unsupported",
            (Status::Error, "rate_limited", true) => "hız sınırı, bekleniyor",
            (Status::Error, "rate_limited", false) => "rate limited, waiting",
            (Status::Error, "network", true) => "ağ hatası",
            (Status::Error, "network", false) => "network error",
            (Status::Error, "timeout", true) => "zaman aşımı",
            (Status::Error, "timeout", false) => "timed out",
            (Status::Error, "parse_error", true) => "beklenmeyen yanıt",
            (Status::Error, "parse_error", false) => "unexpected response",
            (Status::Error, "refresh_busy", true) => "token yenileniyor",
            (Status::Error, "refresh_busy", false) => "token refresh in progress",
            (Status::Error, "refresh_failed", true) => "token yenilenemedi",
            (Status::Error, "refresh_failed", false) => "token refresh failed",
            (Status::Error, "credentials_unreadable", true) => "giriş dosyası okunamadı",
            (Status::Error, "credentials_unreadable", false) => "credentials unreadable",
            (Status::Error, _, true) => "hata",
            (Status::Error, _, false) => "error",
        };
        s.to_string()
    }

    pub fn connect_hint(self, cmd: &str) -> String {
        if self.tr() {
            format!("bağlanmak için: {cmd}  (veya: kota connect …)")
        } else {
            format!("to connect: {cmd}  (or: kota connect …)")
        }
    }

    pub fn stale(self, when: &str) -> String {
        if self.tr() {
            format!("son veri: {when}")
        } else {
            format!("last data: {when}")
        }
    }

    pub fn source_local(self) -> &'static str {
        if self.tr() { "yerel log" } else { "local log" }
    }

    pub fn resets(self, at: DateTime<Utc>, now: DateTime<FixedOffset>) -> String {
        let left = at - now.with_timezone(&Utc);
        if left <= Duration::zero() {
            return if self.tr() {
                "yenilendi".into()
            } else {
                "renewed".into()
            };
        }
        let when = self.when(at, now);
        let dur = self.duration(left + Duration::seconds(30));
        if self.tr() {
            format!("yenilenme: {when} ({dur})")
        } else {
            format!("resets {when} ({dur})")
        }
    }

    /// Yerel saatle kısa zaman: bugün `23:19`, yarın `yarın 07:59`, bu hafta
    /// `Per 07:59`, daha sonra `13 Kas 07:59`.
    pub fn when(self, at: DateTime<Utc>, now: DateTime<FixedOffset>) -> String {
        // API sıfırlanma anını milisaniye oynamasıyla döndürüyor (…:59.8 / …:00.1); en yakın dakikaya yuvarla.
        let local = (at + Duration::seconds(30)).with_timezone(now.offset());
        let clock = local.format("%H:%M").to_string();
        let days = (local.date_naive() - now.date_naive()).num_days();
        match days {
            0 => clock,
            1 => {
                if self.tr() {
                    format!("yarın {clock}")
                } else {
                    format!("tomorrow {clock}")
                }
            }
            2..=6 => {
                let wd = local.weekday().num_days_from_monday() as usize;
                let name = if self.tr() { TR_DAYS[wd] } else { EN_DAYS[wd] };
                format!("{name} {clock}")
            }
            _ => {
                let m = local.month0() as usize;
                if self.tr() {
                    format!("{} {} {clock}", local.day(), TR_MONTHS[m])
                } else {
                    format!("{} {} {clock}", EN_MONTHS[m], local.day())
                }
            }
        }
    }

    /// `2 sa 50 dk` / `2h 50m`; en fazla iki birim.
    pub fn duration(self, d: Duration) -> String {
        let mins = d.num_minutes().max(0);
        let (days, hours, m) = (mins / 1440, (mins % 1440) / 60, mins % 60);
        let (ud, uh, um) = if self.tr() {
            (" g", " sa", " dk")
        } else {
            ("d", "h", "m")
        };
        if days > 0 {
            if hours > 0 {
                format!("{days}{ud} {hours}{uh}")
            } else {
                format!("{days}{ud}")
            }
        } else if hours > 0 {
            if m > 0 {
                format!("{hours}{uh} {m}{um}")
            } else {
                format!("{hours}{uh}")
            }
        } else if m > 0 {
            format!("{m}{um}")
        } else if self.tr() {
            "<1 dk".into()
        } else {
            "<1m".into()
        }
    }

    /// `1.2M` / `1,2M`
    pub fn tokens(self, n: u64) -> String {
        let (v, unit) = match n {
            0..=999 => return n.to_string(),
            1_000..=999_999 => (n as f64 / 1e3, "K"),
            1_000_000..=999_999_999 => (n as f64 / 1e6, "M"),
            _ => (n as f64 / 1e9, "B"),
        };
        let s = if v >= 100.0 {
            format!("{v:.0}")
        } else {
            format!("{v:.1}")
        };
        let s = if self.tr() { s.replace('.', ",") } else { s };
        format!("{s}{unit}")
    }

    pub fn local_line(self, today: &str, window: Option<&str>) -> String {
        match (self.tr(), window) {
            (true, Some(w)) => format!("yerel log: bugün {today} token · bu oturumda {w}"),
            (true, None) => format!("yerel log: bugün {today} token"),
            (false, Some(w)) => format!("local logs: {today} tokens today · {w} this session"),
            (false, None) => format!("local logs: {today} tokens today"),
        }
    }

    /// Katalogdaki `shows` alanı.
    pub fn shows(self, shows: &str) -> &'static str {
        match (shows, self.tr()) {
            ("plan_limits", true) => "plan limitleri (%)",
            ("plan_limits", false) => "plan limits (%)",
            ("balance", true) => "bakiye",
            ("balance", false) => "balance",
            ("spend", true) => "aylık harcama (+ bütçe %)",
            ("spend", false) => "monthly spend (+ budget %)",
            (_, true) => "kullanım",
            (_, false) => "usage",
        }
    }

    pub fn money(self, amount: f64, currency: &str) -> String {
        let sym = match currency.to_uppercase().as_str() {
            "USD" => "$",
            "CNY" => "¥",
            "EUR" => "€",
            "TRY" => "₺",
            _ => "",
        };
        let n = format!("{amount:.2}");
        let n = if self.tr() { n.replace('.', ",") } else { n };
        if sym.is_empty() {
            format!("{n} {currency}")
        } else {
            format!("{sym}{n}")
        }
    }

    pub fn balance_line(self, b: &kota_core::model::Balance) -> String {
        let amount = self.money(b.amount, &b.currency);
        match (b.kind.as_str(), b.budget, self.tr()) {
            ("spend", Some(budget), true) => format!("bu ay harcama: {amount} / {}", self.money(budget, &b.currency)),
            ("spend", Some(budget), false) => {
                format!("spent this month: {amount} / {}", self.money(budget, &b.currency))
            }
            ("spend", None, true) => format!("bu ay harcama: {amount}"),
            ("spend", None, false) => format!("spent this month: {amount}"),
            (_, _, true) => format!("bakiye: {amount}"),
            (_, _, false) => format!("balance: {amount}"),
        }
    }

    pub fn exhausts(self, at: DateTime<Utc>, now: DateTime<FixedOffset>) -> String {
        let when = self.when(at, now);
        if self.tr() {
            format!("bu hızla biter: {when}")
        } else {
            format!("at this pace, runs out {when}")
        }
    }

    pub fn weekly_share(self) -> &'static str {
        if self.tr() { "haftalık pay" } else { "weekly share" }
    }

    pub fn detail_label(self, id: &str, fallback: &str) -> String {
        match (id, self.tr()) {
            ("extra_usage", true) => "Ek kullanım".into(),
            ("credits", true) => "Kredi".into(),
            _ => fallback.to_string(),
        }
    }

    pub fn no_windows(self) -> &'static str {
        if self.tr() {
            "limit bilgisi yok"
        } else {
            "no limit data"
        }
    }

    pub fn no_providers(self) -> &'static str {
        if self.tr() {
            "Etkin sağlayıcı yok (~/.config/kota/config.toml)."
        } else {
            "No providers enabled (~/.config/kota/config.toml)."
        }
    }

    pub fn press_enter(self) -> &'static str {
        if self.tr() {
            "Kapatmak için Enter'a basın… "
        } else {
            "Press Enter to close… "
        }
    }

    pub fn terminal_opened(self, provider: &str) -> String {
        if self.tr() {
            format!("{provider} girişi için terminal açıldı. Giriş tamamlanınca Kota bağlantıyı kendiliğinden algılar.")
        } else {
            format!("Opened a terminal to sign in to {provider}. Kota detects the new session automatically.")
        }
    }
}

fn tr_base_label(id: &str, fallback: &str) -> String {
    let fixed = match id {
        "session" => Some("5 saatlik oturum"),
        "daily" => Some("Günlük"),
        "weekly" => Some("Haftalık"),
        "weekly_opus" => Some("Haftalık · Opus"),
        "weekly_sonnet" => Some("Haftalık · Sonnet"),
        "weekly_oauth_apps" => Some("Haftalık · OAuth uygulamaları"),
        "weekly_cowork" => Some("Haftalık · Cowork"),
        "monthly" => Some("30 günlük"),
        _ => None,
    };
    if let Some(s) = fixed {
        return s.to_string();
    }
    if let Some(n) = id.strip_prefix("days_") {
        return format!("{n} günlük");
    }
    if let Some(n) = id.strip_prefix("hours_") {
        return format!("{n} saatlik");
    }
    fallback.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use kota_core::config::Thresholds;

    fn now() -> DateTime<FixedOffset> {
        FixedOffset::east_opt(3 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 11, 6, 19, 30, 0)
            .unwrap()
    }

    #[test]
    fn percent_sign_position() {
        assert_eq!(Lang::Tr.percent(21.0, false), "%21");
        assert_eq!(Lang::En.percent(21.4, true), "~21%");
    }

    #[test]
    fn when_and_resets() {
        let n = now();
        let today = Utc.with_ymd_and_hms(2026, 11, 6, 20, 19, 59).unwrap(); // 23:19 yerel
        assert_eq!(Lang::Tr.when(today, n), "23:20");
        assert_eq!(Lang::Tr.resets(today, n), "yenilenme: 23:20 (3 sa 50 dk)");
        let tomorrow = Utc.with_ymd_and_hms(2026, 11, 7, 4, 59, 0).unwrap();
        assert_eq!(Lang::Tr.when(tomorrow, n), "yarın 07:59");
        let thu = Utc.with_ymd_and_hms(2026, 11, 12, 4, 59, 0).unwrap();
        assert_eq!(Lang::Tr.when(thu, n), "Per 07:59");
        assert_eq!(Lang::En.when(thu, n), "Thu 07:59");
        let later = Utc.with_ymd_and_hms(2026, 12, 3, 7, 31, 0).unwrap();
        assert_eq!(Lang::Tr.when(later, n), "3 Ara 10:31");
        assert_eq!(Lang::En.when(later, n), "Dec 3 10:31");
        assert_eq!(
            Lang::En.resets(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(), n),
            "renewed"
        );
    }

    #[test]
    fn durations_and_tokens() {
        assert_eq!(
            Lang::Tr.duration(Duration::minutes(6 * 1440 + 14 * 60 + 5)),
            "6 g 14 sa"
        );
        assert_eq!(Lang::En.duration(Duration::minutes(45)), "45m");
        assert_eq!(Lang::En.duration(Duration::seconds(20)), "<1m");
        assert_eq!(Lang::Tr.tokens(1_234_567), "1,2M");
        assert_eq!(Lang::En.tokens(840_000), "840K");
        assert_eq!(Lang::En.tokens(999), "999");
    }

    #[test]
    fn window_labels() {
        let th = Thresholds::default();
        let w = |id: &str, label: &str| UsageWindow::new(id, label, 1.0, None, None, &th);
        assert_eq!(
            Lang::Tr.window_label(&w("session", "5-hour session")),
            "5 saatlik oturum"
        );
        assert_eq!(
            Lang::Tr.window_label(&w("code_review:weekly", "Code review · Weekly")),
            "Kod inceleme · Haftalık"
        );
        assert_eq!(
            Lang::Tr.window_label(&w("extra:spark:session", "Spark · 5-hour session")),
            "Spark · 5 saatlik oturum"
        );
        assert_eq!(Lang::Tr.window_label(&w("days_3", "3-day")), "3 günlük");
        assert_eq!(Lang::Tr.window_label(&w("mystery", "Mystery")), "Mystery");
        assert_eq!(Lang::En.window_label(&w("session", "5-hour session")), "5-hour session");
    }
}
