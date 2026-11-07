//! Yerel JSONL oturum loglarını okuma.
//!
//! Ağ gerektirmeyen en sağlam veri kaynağı: Claude Code ve Codex her isteğin
//! token kullanımını yerel dosyalara yazar. Dosyalar yalnızca sona eklenerek
//! büyüdüğü için her taramada yalnızca yeni satırlar okunur.

pub mod claude;
pub mod codex;

use std::fs::File;
use std::io::{self, BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Timelike, Utc};

/// Saatlik etkinlik dizisinin uzunluğu.
pub const HOURS: usize = 24;

/// `now`un içinde bulunduğu saatin başı.
pub fn hour_floor(now: DateTime<Utc>) -> DateTime<Utc> {
    now.with_minute(0)
        .and_then(|t| t.with_second(0))
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(now)
}

/// Saatlik dizinin ilk saati (`HOURS - 1` saat önce).
pub fn hours_start(now: DateTime<Utc>) -> DateTime<Utc> {
    hour_floor(now) - chrono::Duration::hours(HOURS as i64 - 1)
}

/// `ts`in saatlik dizideki yeri (son eleman şu anki saat).
pub fn hour_index(now: DateTime<Utc>, ts: DateTime<Utc>) -> Option<usize> {
    let start = hours_start(now);
    if ts < start || ts > now + chrono::Duration::minutes(5) {
        return None;
    }
    let idx = ((ts - start).num_seconds() / 3600) as usize;
    Some(idx.min(HOURS - 1))
}

/// Bir dosyanın ne kadarının okunduğu.
#[derive(Debug, Clone, Default)]
pub struct TailState {
    pub offset: u64,
}

/// Bulunan log dosyası.
#[derive(Debug, Clone)]
pub struct FoundFile {
    pub path: PathBuf,
    pub modified: SystemTime,
    pub len: u64,
}

/// `root` altında (sembolik bağları izlemeden) `.jsonl` dosyalarını bulur.
/// `modified_since` verilirse daha eski dosyalar atlanır.
pub fn find_jsonl(root: &Path, modified_since: Option<SystemTime>, max_depth: usize) -> Vec<FoundFile> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            let path = entry.path();
            if ft.is_dir() {
                if depth < max_depth {
                    stack.push((path, depth + 1));
                }
            } else if ft.is_file() && path.extension().is_some_and(|e| e == "jsonl") {
                let Ok(meta) = entry.metadata() else { continue };
                let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                if modified_since.is_some_and(|since| modified < since) {
                    continue;
                }
                out.push(FoundFile {
                    path,
                    modified,
                    len: meta.len(),
                });
            }
        }
    }
    out
}

/// Son okumadan bu yana eklenen *tam* satırları `on_line`'a verir. Sonu `\n`
/// ile bitmeyen (hâlâ yazılan) son satır bir sonraki çağrıya bırakılır.
///
/// Çağıran, dosya küçüldüyse (`len < state.offset`) önbelleğini sıfırlamalıdır;
/// bu fonksiyon o durumda baştan okur.
pub fn tail_lines(path: &Path, state: &mut TailState, mut on_line: impl FnMut(&str)) -> io::Result<()> {
    let file = File::open(path)?;
    let len = file.metadata()?.len();
    if len < state.offset {
        state.offset = 0;
    }
    if len == state.offset {
        return Ok(());
    }
    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(state.offset))?;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 || buf.last() != Some(&b'\n') {
            break;
        }
        state.offset += n as u64;
        let line = String::from_utf8_lossy(&buf[..buf.len() - 1]);
        let line = line.trim();
        if !line.is_empty() {
            on_line(line);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn tail_reads_only_complete_new_lines() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.jsonl");
        let mut f = File::create(&p).unwrap();
        write!(f, "one\ntwo\npart").unwrap();
        f.flush().unwrap();

        let mut st = TailState::default();
        let mut got = Vec::new();
        tail_lines(&p, &mut st, |l| got.push(l.to_string())).unwrap();
        assert_eq!(got, ["one", "two"]);

        write!(f, "ial\nthree\n").unwrap();
        f.flush().unwrap();
        got.clear();
        tail_lines(&p, &mut st, |l| got.push(l.to_string())).unwrap();
        assert_eq!(got, ["partial", "three"]);

        got.clear();
        tail_lines(&p, &mut st, |l| got.push(l.to_string())).unwrap();
        assert!(got.is_empty());
    }

    #[test]
    fn hour_indexes() {
        use chrono::TimeZone;
        let now = Utc.with_ymd_and_hms(2026, 11, 7, 14, 25, 0).unwrap();
        assert_eq!(hours_start(now), Utc.with_ymd_and_hms(2026, 11, 6, 15, 0, 0).unwrap());
        assert_eq!(hour_index(now, now), Some(23));
        assert_eq!(
            hour_index(now, Utc.with_ymd_and_hms(2026, 11, 7, 14, 0, 0).unwrap()),
            Some(23)
        );
        assert_eq!(
            hour_index(now, Utc.with_ymd_and_hms(2026, 11, 7, 13, 59, 0).unwrap()),
            Some(22)
        );
        assert_eq!(
            hour_index(now, Utc.with_ymd_and_hms(2026, 11, 6, 15, 0, 0).unwrap()),
            Some(0)
        );
        assert_eq!(
            hour_index(now, Utc.with_ymd_and_hms(2026, 11, 6, 14, 59, 0).unwrap()),
            None
        );
    }

    #[test]
    fn find_respects_mtime_and_extension() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("p/sub")).unwrap();
        std::fs::write(dir.path().join("p/sub/x.jsonl"), "{}\n").unwrap();
        std::fs::write(dir.path().join("p/y.txt"), "").unwrap();
        let all = find_jsonl(dir.path(), None, 6);
        assert_eq!(all.len(), 1);
        let future = SystemTime::now() + std::time::Duration::from_secs(3600);
        assert!(find_jsonl(dir.path(), Some(future), 6).is_empty());
        assert!(find_jsonl(dir.path(), None, 0).is_empty());
    }
}
