//! Codex oturum logları: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`.
//!
//! `token_count` olayları oturum başına *kümülatif* token sayacı ve o anki
//! limit durumunu (`rate_limits`) taşır. Bugünkü kullanım = son sayaç −
//! gece yarısından önceki son sayaç. Limitler API'ye ulaşılamadığında
//! "son bilinen değer" olarak kullanılır.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

use super::{HOURS, TailState, find_jsonl, hour_index, hours_start, tail_lines};
use crate::jsonx;
use crate::model::{LocalUsage, TokenTotals};

/// Logdaki bir limit penceresi.
#[derive(Debug, Clone, PartialEq)]
pub struct RawWindow {
    /// `primary` / `secondary`
    pub slot: &'static str,
    pub used_percent: f64,
    pub window_minutes: Option<u64>,
    pub resets_at: Option<DateTime<Utc>>,
}

/// Logdan okunan son limit durumu.
#[derive(Debug, Clone, PartialEq)]
pub struct RateSample {
    pub at: DateTime<Utc>,
    pub plan: Option<String>,
    pub windows: Vec<RawWindow>,
}

#[derive(Debug, Default)]
struct FileCache {
    tail: TailState,
    /// Ufuktan (bugün veya 24 saat öncesi, hangisi erkense) beri kümülatif sayaç noktaları.
    points: Vec<(DateTime<Utc>, TokenTotals)>,
    /// Ufuktan önceki son kümülatif sayaç.
    baseline: Option<TokenTotals>,
    rate: Option<RateSample>,
}

#[derive(Debug)]
pub struct CodexLogScanner {
    root: PathBuf,
    files: HashMap<PathBuf, FileCache>,
}

/// Limit yedeği için bakılan en yeni dosya sayısı.
const RATE_FALLBACK_FILES: usize = 3;

impl CodexLogScanner {
    /// `root`: genellikle `~/.codex/sessions`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            files: HashMap::new(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn summarize(
        &mut self,
        now: DateTime<Utc>,
        today_start: DateTime<Utc>,
    ) -> (Option<LocalUsage>, Option<RateSample>) {
        if !self.root.is_dir() {
            self.files.clear();
            return (None, None);
        }
        let horizon = today_start.min(hours_start(now));
        let since: SystemTime = (horizon - Duration::minutes(1)).into();
        let mut found = find_jsonl(&self.root, None, 6);
        found.sort_by_key(|f| std::cmp::Reverse(f.modified));
        let selected: Vec<_> = found
            .into_iter()
            .enumerate()
            .filter(|(i, f)| *i < RATE_FALLBACK_FILES || f.modified >= since)
            .map(|(_, f)| f)
            .collect();
        let keep: HashSet<&Path> = selected.iter().map(|f| f.path.as_path()).collect();
        self.files.retain(|p, _| keep.contains(p.as_path()));

        for f in &selected {
            let cache = self.files.entry(f.path.clone()).or_default();
            if f.len < cache.tail.offset {
                *cache = FileCache::default();
            }
            let FileCache {
                tail,
                points,
                baseline,
                rate,
            } = cache;
            if let Err(e) = tail_lines(&f.path, tail, |line| {
                let Some(ev) = parse_line(line) else { return };
                if let Some(totals) = ev.totals {
                    points.push((ev.ts, totals));
                }
                if let Some(sample) = ev.rate
                    && rate.as_ref().is_none_or(|r| sample.at >= r.at)
                {
                    *rate = Some(sample);
                }
            }) {
                log::debug!("codex log {} okunamadı: {e}", f.path.display());
            }
            // Ufuktan önceki noktaları tek bir taban değere indir.
            let split = points.partition_point(|(ts, _)| *ts < horizon);
            if split > 0 {
                *baseline = Some(points[split - 1].1);
                points.drain(..split);
            }
        }

        let mut usage = LocalUsage {
            hourly: vec![0; HOURS],
            ..LocalUsage::default()
        };
        let mut latest_rate: Option<&RateSample> = None;
        let day_ago = hours_start(now);
        for cache in self.files.values() {
            if let Some((ts, last)) = cache.points.last() {
                // bugünün tabanı: gece yarısından önceki son nokta
                let before_today = cache.points.partition_point(|(t, _)| *t < today_start);
                let base = match before_today {
                    0 => cache.baseline.unwrap_or_default(),
                    n => cache.points[n - 1].1,
                };
                usage.today.add(&last.saturating_sub(&base));
                usage.last_activity = usage.last_activity.max(Some(*ts));
                let mut prev = cache.baseline.unwrap_or_default();
                for (t, p) in &cache.points {
                    if let Some(i) = hour_index(now, *t) {
                        usage.hourly[i] += p.saturating_sub(&prev).total;
                    }
                    prev = *p;
                }
                if *ts >= day_ago {
                    usage.sessions += 1;
                }
            }
            if let Some(r) = &cache.rate
                && latest_rate.is_none_or(|l| r.at > l.at)
            {
                latest_rate = Some(r);
            }
        }
        (Some(usage), latest_rate.cloned())
    }
}

/// Günlük toplam token: oturum başına kümülatif sayaç farkları, farkın
/// oluştuğu yerel güne yazılır.
pub fn daily_tokens(
    root: &Path,
    since: DateTime<Utc>,
    day: impl Fn(DateTime<Utc>) -> chrono::NaiveDate,
) -> HashMap<chrono::NaiveDate, u64> {
    let mut out: HashMap<chrono::NaiveDate, u64> = HashMap::new();
    for f in find_jsonl(root, Some(since.into()), 6) {
        let mut tail = TailState::default();
        let mut prev: Option<TokenTotals> = None;
        let _ = tail_lines(&f.path, &mut tail, |line| {
            let Some(Event { ts, totals: Some(t), .. }) = parse_line(line) else { return };
            if let Some(p) = prev
                && ts >= since
            {
                *out.entry(day(ts)).or_default() += t.saturating_sub(&p).total;
            }
            prev = Some(t);
        });
    }
    out
}

/// `token_count` satırındaki oturum toplamı (kümülatif).
pub fn line_total(line: &str) -> Option<u64> {
    parse_line(line)?.totals.map(|t| t.total)
}

struct Event {
    ts: DateTime<Utc>,
    totals: Option<TokenTotals>,
    rate: Option<RateSample>,
}

fn parse_line(line: &str) -> Option<Event> {
    if !line.contains("\"token_count\"") {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let payload = v.get("payload")?;
    if payload.get("type")?.as_str()? != "token_count" {
        return None;
    }
    let ts = jsonx::time_of(v.get("timestamp")?)?;
    let totals = payload
        .get("info")
        .and_then(|i| i.get("total_token_usage"))
        .and_then(parse_totals);
    let rate = payload.get("rate_limits").and_then(|rl| parse_rate(rl, ts));
    Some(Event { ts, totals, rate })
}

fn parse_totals(u: &Value) -> Option<TokenTotals> {
    let get = |k: &str| u.get(k).and_then(jsonx::u64_of);
    let input = get("input_tokens")?;
    let cached = get("cached_input_tokens").unwrap_or(0);
    Some(TokenTotals::new(
        input.saturating_sub(cached),
        get("output_tokens").unwrap_or(0),
        get("cache_write_input_tokens").unwrap_or(0),
        cached,
    ))
}

/// `rate_limits` nesnesi: `{primary: {used_percent, window_minutes, resets_at}, secondary, plan_type}`.
/// Eski sürümler `resets_at` yerine olaya göreli `resets_in_seconds` yazıyordu.
pub fn parse_rate(rl: &Value, at: DateTime<Utc>) -> Option<RateSample> {
    let mut windows = Vec::new();
    for slot in ["primary", "secondary"] {
        let Some(w) = rl.get(slot).filter(|w| w.is_object()) else {
            continue;
        };
        let Some(used) = w.get("used_percent").and_then(jsonx::f64_of) else {
            continue;
        };
        let resets_at = w.get("resets_at").and_then(jsonx::time_of).or_else(|| {
            w.get("resets_in_seconds")
                .and_then(jsonx::u64_of)
                .map(|s| at + Duration::seconds(s as i64))
        });
        windows.push(RawWindow {
            slot,
            used_percent: used,
            window_minutes: w.get("window_minutes").and_then(jsonx::u64_of),
            resets_at,
        });
    }
    if windows.is_empty() {
        return None;
    }
    Some(RateSample {
        at,
        plan: rl.get("plan_type").and_then(jsonx::str_of).map(str::to_string),
        windows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::io::Write;

    fn tc(ts: &str, input: u64, cached: u64, output: u64, primary: &str) -> String {
        format!(
            r#"{{"timestamp":"{ts}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{input},"cached_input_tokens":{cached},"output_tokens":{output},"total_tokens":{}}}}},"rate_limits":{{"limit_id":"codex","primary":{primary},"secondary":null,"plan_type":"plus"}}}}}}"#,
            input + output
        )
    }

    #[test]
    fn today_delta_and_latest_rate() {
        let dir = tempfile::tempdir().unwrap();
        let day = dir.path().join("2026/11/06");
        std::fs::create_dir_all(&day).unwrap();
        let mut f = std::fs::File::create(day.join("rollout-a.jsonl")).unwrap();
        writeln!(
            f,
            r#"{{"timestamp":"2026-11-05T22:00:00Z","type":"session_meta","payload":{{}}}}"#
        )
        .unwrap();
        // dün: kümülatif 1000 girdi (800 önbellek), 100 çıktı
        writeln!(f, "{}", tc("2026-11-05T23:00:00Z", 1000, 800, 100, "null")).unwrap();
        // bugün: kümülatif 1500/1100/160 → fark: girdi 200, önbellek 300, çıktı 60
        writeln!(
            f,
            "{}",
            tc(
                "2026-11-06T09:00:00Z",
                1500,
                1100,
                160,
                r#"{"used_percent":42.0,"window_minutes":300,"resets_at":1794000000}"#
            )
        )
        .unwrap();
        // limit bilgisi olmayan sonraki olay limiti silmez
        writeln!(f, "{}", tc("2026-11-06T09:05:00Z", 1500, 1100, 160, "null")).unwrap();
        f.flush().unwrap();

        let today = Utc.with_ymd_and_hms(2026, 11, 6, 0, 0, 0).unwrap();
        let mut sc = CodexLogScanner::new(dir.path());
        let now = Utc.with_ymd_and_hms(2026, 11, 6, 10, 0, 0).unwrap();
        let (usage, rate) = sc.summarize(now, today);
        let usage = usage.unwrap();
        assert_eq!(usage.today.input, 200);
        assert_eq!(usage.today.cache_read, 300);
        assert_eq!(usage.today.output, 60);
        assert_eq!(usage.today.total, 560);
        // 24 saat ufku dünü 11:00'den kapsar: dünkü 23:00 noktası (1000+100, sıfırdan) saat 12'de,
        // bugünkü 09:00 farkı (560) saat 22'de.
        assert_eq!(usage.hourly[12], 1100);
        assert_eq!(usage.hourly[22], 560);
        assert_eq!(usage.sessions, 1);
        let rate = rate.unwrap();
        assert_eq!(rate.plan.as_deref(), Some("plus"));
        assert_eq!(rate.windows.len(), 1);
        assert_eq!(rate.windows[0].used_percent, 42.0);
        assert_eq!(rate.windows[0].window_minutes, Some(300));
        assert_eq!(rate.at, Utc.with_ymd_and_hms(2026, 11, 6, 9, 0, 0).unwrap());
    }

    #[test]
    fn legacy_relative_reset() {
        let at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let v: Value = serde_json::from_str(
            r#"{"primary":{"used_percent":"7","window_minutes":10080,"resets_in_seconds":3600},"secondary":null}"#,
        )
        .unwrap();
        let s = parse_rate(&v, at).unwrap();
        assert_eq!(s.windows[0].used_percent, 7.0);
        assert_eq!(s.windows[0].resets_at, Some(at + Duration::hours(1)));
    }
}
