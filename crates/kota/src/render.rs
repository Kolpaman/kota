//! İnsan okunur terminal çıktısı ve Waybar özel modülü JSON'u.

use chrono::{DateTime, FixedOffset};
use kota_core::model::{ProviderReport, Severity, Snapshot, Source, Status, UsageWindow};
use serde_json::json;

use crate::i18n::Lang;

const BAR_WIDTH: usize = 20;
const LABEL_MAX: usize = 30;

struct Style {
    color: bool,
}

impl Style {
    fn paint(&self, code: &str, s: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    fn bold(&self, s: &str) -> String {
        self.paint("1", s)
    }
    fn dim(&self, s: &str) -> String {
        self.paint("2", s)
    }
}

fn severity_color(sev: Severity, stale: bool) -> &'static str {
    match (stale, sev) {
        (true, _) => "90",
        (false, Severity::Normal) => "32",
        (false, Severity::Warning) => "33",
        (false, Severity::Critical) => "31",
    }
}

fn status_color(p: &ProviderReport) -> &'static str {
    match p.status {
        Status::Ok => "32",
        Status::NotConnected | Status::Unsupported => "90",
        Status::Expired => "31",
        Status::Error => "33",
    }
}

/// `█████▍░░░░` biçiminde, sekizde bir hassasiyetli çubuk.
pub fn bar(percent: f64, width: usize) -> String {
    const PARTIAL: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
    let eighths = ((percent.clamp(0.0, 100.0) / 100.0) * (width * 8) as f64).round() as usize;
    let full = eighths / 8;
    let rem = eighths % 8;
    let mut s = "█".repeat(full);
    let mut used = full;
    if rem > 0 && used < width {
        s.push_str(PARTIAL[rem]);
        used += 1;
    }
    s.push_str(&"░".repeat(width.saturating_sub(used)));
    s
}

fn header_suffix(p: &ProviderReport, lang: Lang, now: DateTime<FixedOffset>) -> String {
    let Some(at) = p.fetched_at else {
        return String::new();
    };
    let when = lang.when(at, now);
    let mut parts = Vec::new();
    if p.source == Some(Source::LocalLogs) {
        parts.push(lang.source_local().to_string());
    }
    if p.stale {
        parts.push(lang.stale(&when));
    } else if p.status == Status::Ok {
        parts.push(when);
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" · {}", parts.join(", "))
    }
}

pub fn human(snap: &Snapshot, lang: Lang, color: bool, now: DateTime<FixedOffset>) -> String {
    let st = Style { color };
    let mut out = String::new();
    if snap.providers.is_empty() {
        out.push_str(lang.no_providers());
        out.push('\n');
        return out;
    }
    let label_w = snap
        .providers
        .iter()
        .flat_map(|p| p.windows.iter())
        .map(|w| lang.window_label(w).chars().count())
        .max()
        .unwrap_or(10)
        .min(LABEL_MAX);

    for (i, p) in snap.providers.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let plan = p.plan.as_deref().map(|pl| format!(" · {pl}")).unwrap_or_default();
        out.push_str(&format!(
            "{}{}  {} {}{}\n",
            st.bold(&p.name),
            plan,
            st.paint(status_color(p), "●"),
            lang.status(p),
            st.dim(&header_suffix(p, lang, now)),
        ));

        match p.status {
            Status::NotConnected | Status::Expired => {
                if let Some(cmd) = &p.login_command {
                    out.push_str(&format!("  {}\n", st.dim(&lang.connect_hint(&cmd.join(" ")))));
                }
            }
            Status::Error | Status::Unsupported => {
                if let Some(m) = &p.message {
                    out.push_str(&format!("  {}\n", st.dim(m)));
                }
            }
            _ => {}
        }

        for w in &p.windows {
            out.push_str(&window_line(w, p.stale, label_w, lang, &st, now));
        }
        if let Some(b) = &p.balance {
            out.push_str(&format!("  {}\n", lang.balance_line(b)));
        }
        if p.windows.is_empty() && p.balance.is_none() && p.status == Status::Ok {
            out.push_str(&format!("  {}\n", st.dim(lang.no_windows())));
        }

        let (shares, others): (Vec<_>, Vec<_>) = p.details.iter().partition(|d| d.id.starts_with("weekly_share:"));
        if !shares.is_empty() {
            let joined = shares
                .iter()
                .map(|d| format!("{} {}", d.label, percent_value(lang, &d.value)))
                .collect::<Vec<_>>()
                .join(" · ");
            out.push_str(&format!(
                "  {}\n",
                st.dim(&format!("{}: {joined}", lang.weekly_share()))
            ));
        }
        for d in others {
            out.push_str(&format!(
                "  {}\n",
                st.dim(&format!("{}: {}", lang.detail_label(&d.id, &d.label), d.value))
            ));
        }

        if let Some(l) = &p.local {
            let window = l.window.filter(|w| !w.is_zero());
            if !l.today.is_zero() || window.is_some() {
                let today = lang.tokens(l.today.total);
                let w = window.map(|w| lang.tokens(w.total));
                out.push_str(&format!("  {}\n", st.dim(&lang.local_line(&today, w.as_deref()))));
            }
        }
    }
    out
}

fn window_line(
    w: &UsageWindow,
    stale: bool,
    label_w: usize,
    lang: Lang,
    st: &Style,
    now: DateTime<FixedOffset>,
) -> String {
    let mut label = lang.window_label(w);
    if label.chars().count() > label_w {
        label = label.chars().take(label_w.saturating_sub(1)).collect::<String>() + "…";
    }
    let pct = lang.percent(w.used_percent, w.estimated);
    let mut reset = w.resets_at.map(|t| lang.resets(t, now)).unwrap_or_default();
    if let Some(eta) = w.exhausts_at.filter(|_| !stale) {
        reset = format!("{reset} · {}", lang.exhausts(eta, now));
    }
    format!(
        "  {label:<label_w$}  {}  {pct:>5}  {}\n",
        st.paint(severity_color(w.severity, stale), &bar(w.used_percent, BAR_WIDTH)),
        st.dim(&reset)
    )
}

/// Sağlayıcı ayrıntılarındaki `"53%"` değerini dile göre yazar.
fn percent_value(lang: Lang, v: &str) -> String {
    match v.strip_suffix('%').and_then(|n| n.parse::<f64>().ok()) {
        Some(n) => lang.percent(n, false),
        None => v.to_string(),
    }
}

/// Waybar `custom` modülü için tek satırlık JSON (`return-type: json`).
pub fn waybar(snap: &Snapshot, lang: Lang, now: DateTime<FixedOffset>) -> String {
    let mut parts = Vec::new();
    let mut worst = Severity::Normal;
    let mut max_pct: f64 = 0.0;
    let mut any_problem = false;
    for p in &snap.providers {
        let short = short_name(&p.id, &p.name);
        match p.headline() {
            Some(w) => {
                parts.push(format!("{short} {}", lang.percent(w.used_percent, w.estimated)));
                if !p.stale {
                    worst = worst.max(w.severity);
                }
                max_pct = max_pct.max(w.used_percent);
            }
            None if p.status == Status::NotConnected => {}
            None => parts.push(format!("{short} !")),
        }
        any_problem |= matches!(p.status, Status::Error | Status::Expired);
    }
    let text = if parts.is_empty() {
        "Kota –".to_string()
    } else {
        parts.join("  ")
    };
    let class = match worst {
        Severity::Critical => "critical",
        Severity::Warning => "warning",
        Severity::Normal if any_problem => "problem",
        Severity::Normal => "normal",
    };
    json!({
        "text": text,
        "tooltip": human(snap, lang, false, now).trim_end(),
        "class": class,
        "alt": class,
        "percentage": max_pct.round().clamp(0.0, 100.0) as u64,
    })
    .to_string()
}

fn short_name(id: &str, name: &str) -> String {
    match id {
        "claude" => "Cl".into(),
        "codex" => "Cx".into(),
        _ => name.chars().take(2).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use kota_core::config::Thresholds;
    use kota_core::model::{Detail, LocalUsage, TokenTotals};

    fn now() -> DateTime<FixedOffset> {
        FixedOffset::east_opt(3 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 11, 6, 19, 30, 0)
            .unwrap()
    }

    fn snap() -> Snapshot {
        let th = Thresholds::default();
        let t = now().with_timezone(&Utc);
        let mut c = ProviderReport::new("claude", "Claude");
        c.plan = Some("Pro".into());
        c.source = Some(Source::Api);
        c.fetched_at = Some(t);
        c.windows.push(UsageWindow::new(
            "session",
            "5-hour session",
            21.0,
            Some(Utc.with_ymd_and_hms(2026, 11, 6, 20, 19, 59).unwrap()),
            Some(18_000),
            &th,
        ));
        c.windows.push(UsageWindow::new(
            "weekly",
            "Weekly (all models)",
            3.0,
            Some(Utc.with_ymd_and_hms(2026, 11, 13, 4, 59, 59).unwrap()),
            Some(604_800),
            &th,
        ));
        c.details.push(Detail {
            id: "weekly_share:claude_code".into(),
            label: "Claude Code".into(),
            value: "53%".into(),
        });
        c.local = Some(LocalUsage {
            today: TokenTotals::new(1000, 200_000, 0, 1_000_000),
            window: Some(TokenTotals::new(0, 0, 0, 840_000)),
            ..Default::default()
        });
        let mut x = ProviderReport::new("codex", "Codex");
        x.plan = Some("Free".into());
        x.fetched_at = Some(t);
        x.windows.push(UsageWindow::new(
            "monthly",
            "30-day",
            100.0,
            Some(Utc.with_ymd_and_hms(2026, 12, 3, 7, 31, 6).unwrap()),
            Some(2_592_000),
            &th,
        ));
        Snapshot::new(vec![c, x], t)
    }

    #[test]
    fn bar_rendering() {
        assert_eq!(bar(0.0, 4), "░░░░");
        assert_eq!(bar(100.0, 4), "████");
        assert_eq!(bar(50.0, 4), "██░░");
        assert_eq!(bar(10.0, 4).chars().count(), 4);
        assert_eq!(bar(150.0, 4), "████");
    }

    #[test]
    fn human_turkish() {
        let out = human(&snap(), Lang::Tr, false, now());
        let expected = "\
Claude · Pro  ● bağlı · 19:30
  5 saatlik oturum  ████▎░░░░░░░░░░░░░░░    %21  yenilenme: 23:20 (3 sa 50 dk)
  Haftalık          ▋░░░░░░░░░░░░░░░░░░░     %3  yenilenme: 13 Kas 08:00 (6 g 12 sa)
  haftalık pay: Claude Code %53
  yerel log: bugün 1,2M token · bu oturumda 840K

Codex · Free  ● bağlı · 19:30
  30 günlük         ████████████████████   %100  yenilenme: 3 Ara 10:31 (26 g 15 sa)
";
        assert_eq!(out, expected);
    }

    #[test]
    fn human_shows_connect_hint_and_stale() {
        let mut s = snap();
        s.providers[0].set_status(Status::Expired, "relogin_required", "x");
        s.providers[0].stale = true;
        s.providers[1].set_status(Status::NotConnected, "no_credentials", "x");
        s.providers[1].windows.clear();
        s.providers[1].login_command = Some(vec!["codex".into(), "login".into()]);
        let out = human(&s, Lang::En, false, now());
        assert!(
            out.contains("Claude · Pro  ● sign-in required · last data: 19:30"),
            "{out}"
        );
        assert!(out.contains("to connect: codex login"), "{out}");
    }

    #[test]
    fn waybar_json() {
        let v: serde_json::Value = serde_json::from_str(&waybar(&snap(), Lang::En, now())).unwrap();
        assert_eq!(v["text"], "Cl 21%  Cx 100%");
        assert_eq!(v["class"], "critical");
        assert_eq!(v["percentage"], 100);
        assert!(v["tooltip"].as_str().unwrap().contains("Claude · Pro"));
    }
}
