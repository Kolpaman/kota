//! "Bu hızla X'te dolar" tahmini. Her başarılı sorgudan sonra pencere
//! yüzdeleri örneklenir; aynı dönemdeki son 90 dakikanın eğiminden pencerenin
//! %100'e ne zaman ulaşacağı hesaplanır.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::model::{ProviderReport, Status, UsageWindow};

const KEEP: Duration = Duration::hours(6);
const LOOKBACK: Duration = Duration::minutes(90);
const MIN_SPAN: Duration = Duration::minutes(10);
/// Bundan küçük artışlar gürültü sayılır (yüzde puanı).
const MIN_DELTA: f64 = 1.0;
const SAME_PERIOD: Duration = Duration::minutes(30);
const MAX_PER_KEY: usize = 400;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Sample {
    pub at: DateTime<Utc>,
    pub pct: f64,
    #[serde(default)]
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct History {
    samples: HashMap<String, Vec<Sample>>,
}

fn key(provider: &str, window: &str) -> String {
    format!("{provider}|{window}")
}

fn same_period(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => (a - b).abs() < SAME_PERIOD,
        (None, None) => true,
        _ => false,
    }
}

impl History {
    /// Canlı ölçülen pencereleri kaydeder, sonra hepsine tahmin yazar.
    pub fn observe(&mut self, report: &mut ProviderReport, now: DateTime<Utc>) {
        let live = report.status == Status::Ok && !report.stale;
        for w in &mut report.windows {
            let k = key(&report.id, &w.id);
            if live && !w.estimated {
                let v = self.samples.entry(k.clone()).or_default();
                v.push(Sample {
                    at: now,
                    pct: w.used_percent,
                    resets_at: w.resets_at,
                });
                if v.len() > MAX_PER_KEY {
                    v.drain(..v.len() - MAX_PER_KEY);
                }
            }
            w.exhausts_at = if w.estimated { None } else { self.forecast(&k, w, now) };
        }
    }

    fn forecast(&self, k: &str, w: &UsageWindow, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        if w.used_percent >= 100.0 {
            return None;
        }
        let first = self
            .samples
            .get(k)?
            .iter()
            .find(|s| s.at >= now - LOOKBACK && same_period(s.resets_at, w.resets_at))?;
        let span = now - first.at;
        let delta = w.used_percent - first.pct;
        if span < MIN_SPAN || delta < MIN_DELTA {
            return None;
        }
        let per_sec = delta / span.num_seconds() as f64;
        let secs = ((100.0 - w.used_percent) / per_sec).min(30.0 * 86_400.0);
        let eta = now + Duration::seconds(secs as i64);
        match w.resets_at {
            Some(reset) if eta >= reset => None,
            _ => Some(eta),
        }
    }

    pub fn prune(&mut self, now: DateTime<Utc>) {
        for v in self.samples.values_mut() {
            v.retain(|s| s.at >= now - KEEP);
        }
        self.samples.retain(|_, v| !v.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Thresholds;
    use chrono::TimeZone;

    fn report(pct: f64, reset: DateTime<Utc>) -> ProviderReport {
        let mut r = ProviderReport::new("claude", "Claude");
        r.windows.push(UsageWindow::new(
            "session",
            "s",
            pct,
            Some(reset),
            Some(18_000),
            &Thresholds::default(),
        ));
        r
    }

    #[test]
    fn predicts_exhaustion_before_reset() {
        let t0 = Utc.with_ymd_and_hms(2026, 11, 7, 10, 0, 0).unwrap();
        let reset = t0 + Duration::hours(4);
        let mut h = History::default();
        h.observe(&mut report(20.0, reset), t0);
        // 30 dakikada +20 puan → kalan 60 puan 90 dakikada dolar.
        let mut r = report(40.0, reset);
        h.observe(&mut r, t0 + Duration::minutes(30));
        assert_eq!(r.windows[0].exhausts_at, Some(t0 + Duration::minutes(120)));
    }

    #[test]
    fn no_prediction_when_reset_comes_first_or_flat_or_new_period() {
        let t0 = Utc.with_ymd_and_hms(2026, 11, 7, 10, 0, 0).unwrap();
        let reset = t0 + Duration::minutes(60);
        let mut h = History::default();
        h.observe(&mut report(20.0, reset), t0);
        let mut slow = report(22.0, reset);
        h.observe(&mut slow, t0 + Duration::minutes(30));
        assert_eq!(slow.windows[0].exhausts_at, None, "fills after reset");

        let mut h = History::default();
        h.observe(&mut report(50.0, reset), t0);
        let mut flat = report(50.5, reset);
        h.observe(&mut flat, t0 + Duration::minutes(30));
        assert_eq!(flat.windows[0].exhausts_at, None, "noise");

        let mut h = History::default();
        h.observe(&mut report(90.0, reset), t0);
        let mut fresh = report(30.0, reset + Duration::hours(5));
        h.observe(&mut fresh, t0 + Duration::minutes(30));
        assert_eq!(fresh.windows[0].exhausts_at, None, "previous period ignored");
    }

    #[test]
    fn stale_reports_are_not_sampled_and_prune_works() {
        let t0 = Utc::now();
        let mut h = History::default();
        let mut r = report(10.0, t0 + Duration::hours(1));
        r.stale = true;
        h.observe(&mut r, t0);
        assert!(h.samples.is_empty());
        h.observe(&mut report(10.0, t0 + Duration::hours(1)), t0 - Duration::hours(7));
        h.prune(t0);
        assert!(h.samples.is_empty());
    }
}
