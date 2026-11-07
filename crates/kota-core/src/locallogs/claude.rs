//! Claude Code oturum logları: `~/.claude/projects/**/*.jsonl`.
//!
//! Her asistan mesajı `message.usage` taşır. Akış sırasında aynı mesaj
//! birden çok satıra yazılabildiği ve devam ettirilen oturumlar geçmişi yeni
//! dosyaya kopyalayabildiği için toplama `message.id` ile tekilleştirilir.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

use super::{HOURS, TailState, find_jsonl, hour_index, hours_start, tail_lines};
use crate::jsonx;
use crate::model::{LocalUsage, ModelTokens, TokenTotals};

#[derive(Debug, Clone)]
struct Entry {
    ts: DateTime<Utc>,
    model: String,
    tokens: TokenTotals,
}

#[derive(Debug, Default)]
struct FileCache {
    tail: TailState,
    entries: HashMap<String, Entry>,
}

/// Artımlı tarayıcı: dosya başına okunan konumu ve son 24 saatin girdilerini tutar.
#[derive(Debug)]
pub struct ClaudeLogScanner {
    root: PathBuf,
    files: HashMap<PathBuf, FileCache>,
}

impl ClaudeLogScanner {
    /// `root`: genellikle `~/.claude/projects`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            files: HashMap::new(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Bugünkü ve `window_start`tan beri olan kullanımı hesaplar. Log dizini
    /// yoksa `None`.
    pub fn summarize(
        &mut self,
        now: DateTime<Utc>,
        today_start: DateTime<Utc>,
        window_start: DateTime<Utc>,
    ) -> Option<LocalUsage> {
        if !self.root.is_dir() {
            self.files.clear();
            return None;
        }
        let horizon = today_start.min(window_start).min(hours_start(now));
        let since: SystemTime = (horizon - Duration::minutes(1)).into();
        let found = find_jsonl(&self.root, Some(since), 6);
        let keep: HashSet<&Path> = found.iter().map(|f| f.path.as_path()).collect();
        self.files.retain(|p, _| keep.contains(p.as_path()));

        for f in &found {
            let cache = self.files.entry(f.path.clone()).or_default();
            if f.len < cache.tail.offset {
                *cache = FileCache::default();
            }
            let FileCache { tail, entries } = cache;
            if let Err(e) = tail_lines(&f.path, tail, |line| {
                if let Some((id, entry)) = parse_line(line) {
                    entries.insert(id, entry);
                }
            }) {
                log::debug!("claude log {} okunamadı: {e}", f.path.display());
            }
            entries.retain(|_, e| e.ts >= horizon);
        }

        let mut unique: HashMap<&str, &Entry> = HashMap::new();
        let mut sessions = 0u32;
        let day_ago = hours_start(now);
        for cache in self.files.values() {
            if cache.entries.values().any(|e| e.ts >= day_ago) {
                sessions += 1;
            }
            for (id, e) in &cache.entries {
                unique.insert(id.as_str(), e);
            }
        }
        let mut hourly = vec![0u64; HOURS];

        let mut usage = LocalUsage {
            window_start: Some(window_start),
            ..LocalUsage::default()
        };
        let mut window = TokenTotals::default();
        let mut models: HashMap<&str, u64> = HashMap::new();
        for e in unique.values() {
            if e.ts > now + Duration::minutes(5) {
                continue; // saati bozuk satırlar
            }
            if e.ts >= today_start {
                usage.today.add(&e.tokens);
                *models.entry(e.model.as_str()).or_default() += e.tokens.total;
            }
            if e.ts >= window_start {
                window.add(&e.tokens);
            }
            if let Some(i) = hour_index(now, e.ts) {
                hourly[i] += e.tokens.total;
            }
            usage.last_activity = usage.last_activity.max(Some(e.ts));
        }
        usage.window = Some(window);
        let mut top: Vec<ModelTokens> = models
            .into_iter()
            .map(|(model, total)| ModelTokens {
                model: model.to_string(),
                total,
            })
            .collect();
        top.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.model.cmp(&b.model)));
        top.truncate(3);
        usage.top_models = top;
        usage.hourly = hourly;
        usage.sessions = sessions;
        Some(usage)
    }
}

/// Günlük toplam token (geçmiş grafiği): `since`ten beri değişen tüm dosyalar
/// baştan okunur, mesajlar `message.id` ile tekilleştirilir. `day` bir anı
/// yerel güne çevirir.
pub fn daily_tokens(
    root: &Path,
    since: DateTime<Utc>,
    day: impl Fn(DateTime<Utc>) -> chrono::NaiveDate,
) -> HashMap<chrono::NaiveDate, u64> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: HashMap<chrono::NaiveDate, u64> = HashMap::new();
    for f in find_jsonl(root, Some(since.into()), 6) {
        let mut tail = TailState::default();
        let _ = tail_lines(&f.path, &mut tail, |line| {
            if let Some((id, e)) = parse_line(line)
                && e.ts >= since
                && seen.insert(id)
            {
                *out.entry(day(e.ts)).or_default() += e.tokens.total;
            }
        });
    }
    out
}

/// Tek satırdaki asistan mesajının kimliği ve toplam token'ı (ajan başına sayım).
pub fn message_tokens(line: &str) -> Option<(String, u64)> {
    parse_line(line).map(|(id, e)| (id, e.tokens.total))
}

fn parse_line(line: &str) -> Option<(String, Entry)> {
    // Ucuz ön eleme: satırların çoğu kullanıcı mesajı, araç çıktısı vb.
    if !line.contains("\"usage\"") || !line.contains("\"assistant\"") {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("type")?.as_str()? != "assistant" {
        return None;
    }
    let msg = v.get("message")?;
    let model = jsonx::str_of(msg.get("model")?)?;
    if model.starts_with('<') {
        return None; // "<synthetic>": hata yer tutucuları
    }
    let usage = msg.get("usage")?;
    let get = |k: &str| usage.get(k).and_then(jsonx::u64_of).unwrap_or(0);
    let tokens = TokenTotals::new(
        get("input_tokens"),
        get("output_tokens"),
        get("cache_creation_input_tokens"),
        get("cache_read_input_tokens"),
    );
    if tokens.is_zero() {
        return None;
    }
    let ts = jsonx::time_of(v.get("timestamp")?)?;
    let id = msg
        .get("id")
        .and_then(jsonx::str_of)
        .or_else(|| v.get("uuid").and_then(jsonx::str_of))?
        .to_string();
    Some((
        id,
        Entry {
            ts,
            model: model.to_string(),
            tokens,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::io::Write;

    fn line(id: &str, ts: &str, model: &str, input: u64, output: u64, cw: u64, cr: u64) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","requestId":"req_1","message":{{"id":"{id}","model":"{model}","role":"assistant","content":[{{"type":"text","text":"gizli içerik"}}],"usage":{{"input_tokens":{input},"output_tokens":{output},"cache_creation_input_tokens":{cw},"cache_read_input_tokens":{cr}}}}}}}"#
        )
    }

    #[test]
    fn aggregates_dedupes_and_windows() {
        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path().join("-home-u-proj");
        std::fs::create_dir_all(&proj).unwrap();
        let mut f = std::fs::File::create(proj.join("s1.jsonl")).unwrap();
        // dün: sayılmaz
        writeln!(
            f,
            "{}",
            line("m0", "2026-11-05T10:00:00Z", "claude-opus-5-5", 100, 100, 0, 0)
        )
        .unwrap();
        // bugün, pencere dışı
        writeln!(
            f,
            "{}",
            line("m1", "2026-11-06T08:00:00Z", "claude-opus-5-5", 10, 20, 30, 40)
        )
        .unwrap();
        // pencere içi; aynı mesaj iki satır (akış) → bir kez sayılır
        writeln!(
            f,
            "{}",
            line("m2", "2026-11-06T15:00:00Z", "claude-sonnet-5-5", 1, 2, 3, 4)
        )
        .unwrap();
        writeln!(
            f,
            "{}",
            line("m2", "2026-11-06T15:00:00Z", "claude-sonnet-5-5", 1, 2, 3, 4)
        )
        .unwrap();
        // sentetik hata ve kullanıcı satırı → yok sayılır
        writeln!(f, "{}", line("m3", "2026-11-06T15:01:00Z", "<synthetic>", 0, 0, 0, 0)).unwrap();
        writeln!(f, r#"{{"type":"user","message":{{"role":"user","content":"selam"}}}}"#).unwrap();
        f.flush().unwrap();
        // devam ettirilen oturum geçmişi kopyalar → tekilleştirilir
        std::fs::write(
            proj.join("s2.jsonl"),
            format!(
                "{}\n",
                line("m2", "2026-11-06T15:00:00Z", "claude-sonnet-5-5", 1, 2, 3, 4)
            ),
        )
        .unwrap();

        let now = Utc.with_ymd_and_hms(2026, 11, 6, 16, 0, 0).unwrap();
        let today = Utc.with_ymd_and_hms(2026, 11, 6, 0, 0, 0).unwrap();
        let window_start = Utc.with_ymd_and_hms(2026, 11, 6, 12, 0, 0).unwrap();
        let mut sc = ClaudeLogScanner::new(dir.path());
        let u = sc.summarize(now, today, window_start).unwrap();
        assert_eq!(u.today.total, 100 + 10);
        assert_eq!(u.today.cache_read, 44);
        assert_eq!(u.window.unwrap().total, 10);
        assert_eq!(u.top_models[0].model, "claude-opus-5-5");
        assert_eq!(
            u.last_activity,
            Some(Utc.with_ymd_and_hms(2026, 11, 6, 15, 0, 0).unwrap())
        );
        // saatlik: 15:00 → 22 (16:00 = 23), 08:00 → 15; dünkü 10:00 son 24 saatin dışında
        assert_eq!(u.hourly.len(), 24);
        assert_eq!(u.hourly[22], 10);
        assert_eq!(u.hourly[15], 100);
        assert_eq!(u.hourly.iter().sum::<u64>(), 110);
        assert_eq!(u.sessions, 2);

        // Artımlı okuma: yeni satır eklenince yalnızca o işlenir.
        writeln!(
            f,
            "{}",
            line("m4", "2026-11-06T15:30:00Z", "claude-opus-5-5", 5, 5, 0, 0)
        )
        .unwrap();
        f.flush().unwrap();
        let u2 = sc.summarize(now, today, window_start).unwrap();
        assert_eq!(u2.today.total, 120);
        assert_eq!(u2.window.unwrap().total, 20);
    }

    #[test]
    fn missing_root_is_none() {
        let mut sc = ClaudeLogScanner::new("/nonexistent/kota-test");
        let now = Utc::now();
        assert!(sc.summarize(now, now, now).is_none());
    }
}
