//! Ajanın proje klasöründe **hangi alt projede** çalıştığını bulur.
//!
//! Bir klasör (ör. `~/YAZILIM`) birden fazla proje barındırabilir; ajanın
//! başlatıldığı dizin bunu söylemez. Kaynaklar:
//!   - ajanın ve alt süreçlerinin (kabuk, derleyici, sunucu) çalışma dizinleri,
//!   - Claude Code oturum dökümü (`~/.claude/sessions/<pid>.json` →
//!     `~/.claude/projects/<dizin>/<oturum>.jsonl`) ve Codex oturum dökümü
//!     (sürecin açık tuttuğu `~/.codex/sessions/**/rollout-*.jsonl`): son
//!     mesajların `cwd`/`workdir` alanları, araç çağrılarındaki dosya yolları
//!     ve kabuk komutlarındaki yollar.
//!
//! Aynı dökümden ajanın oturumda harcadığı toplam token da sayılır.
//!
//! Her yol, kök altındaki ilk "proje işaretli" klasöre (Cargo.toml,
//! package.json, .git…) ya da işaret yoksa kökün ilk alt klasörüne eşlenir.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use kota_core::locallogs::{TailState, claude as claude_logs, codex as codex_logs, tail_lines};
use kota_core::model::Focus;

/// Bir klasörü proje yapan dosyalar.
const MARKERS: &[&str] = &[
    ".git",
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "setup.py",
    "requirements.txt",
    "go.mod",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "pubspec.yaml",
    "composer.json",
    "Gemfile",
    "CMakeLists.txt",
    "meson.build",
    "Makefile",
    "deno.json",
    "metadata.json",
];

/// Proje sayılmayan klasörler (araç/önbellek dizinleri).
const IGNORED: &[&str] = &["node_modules", "target", "dist", "build", "__pycache__", "venv"];

/// Dökümün yalnızca sonu okunur (son etkinlik yeter). Görsel içeren araç
/// sonuçları satırları çok büyütebildiği için pencere geniş, büyük satırlar atlanır.
const TAIL_BYTES: u64 = 3 * 1024 * 1024;
const MAX_LINE: usize = 192 * 1024;
const MAX_FOCUS: usize = 4;

#[derive(Debug, Clone)]
struct Hit {
    path: PathBuf,
    at: Option<DateTime<Utc>>,
}

/// Döküm başına artımlı token sayacı.
#[derive(Debug, Default)]
struct Counter {
    tail: TailState,
    seen: HashSet<String>,
    total: u64,
}

#[derive(Debug, Default)]
pub struct FocusFinder {
    /// döküm yolu → (boyut, değişme zamanı, isabetler)
    transcripts: HashMap<PathBuf, (u64, Option<SystemTime>, Vec<Hit>)>,
    counters: HashMap<PathBuf, Counter>,
}

/// Ajanın bulguları.
pub struct Findings {
    pub focus: Vec<Focus>,
    pub tokens: Option<u64>,
}

impl FocusFinder {
    /// `root`: ajanın proje kökü; `cwds`: ajan ve alt süreçlerinin dizinleri;
    /// `tree`: süreç ağacındaki pid'ler (açık döküm dosyasını bulmak için).
    pub fn find(&mut self, kind: &str, pid: u32, tree: &[u32], root: &Path, cwds: &[PathBuf], now: DateTime<Utc>) -> Findings {
        let mut hits: Vec<Hit> = cwds
            .iter()
            .map(|p| Hit {
                path: p.clone(),
                at: Some(now),
            })
            .collect();
        let transcript = match kind {
            "claude" => claude_transcript(pid),
            "codex" => open_codex_rollout(tree),
            _ => None,
        };
        let mut tokens = None;
        if let Some(t) = &transcript {
            hits.extend(self.transcript_hits(t));
            tokens = self.count_tokens(kind, t);
        }
        if self.counters.len() > 32 {
            let keep = transcript.clone();
            self.counters.retain(|p, _| Some(p) == keep.as_ref());
        }
        Findings {
            focus: rank(root, &hits),
            tokens,
        }
    }

    /// Dökümü kaldığı yerden okuyup oturum toplamını günceller.
    fn count_tokens(&mut self, kind: &str, path: &Path) -> Option<u64> {
        let c = self.counters.entry(path.to_path_buf()).or_default();
        let Counter { tail, seen, total } = c;
        let res = tail_lines(path, tail, |line| match kind {
            "claude" => {
                if let Some((id, n)) = claude_logs::message_tokens(line)
                    && seen.insert(id)
                {
                    *total += n;
                }
            }
            _ => {
                // Codex sayacı kümülatiftir: son değer oturum toplamıdır.
                if let Some(n) = codex_logs::line_total(line) {
                    *total = n;
                }
            }
        });
        res.ok()?;
        (c.total > 0).then_some(c.total)
    }

    fn transcript_hits(&mut self, path: &Path) -> Vec<Hit> {
        let Ok(meta) = fs::metadata(path) else {
            return Vec::new();
        };
        let (len, mtime) = (meta.len(), meta.modified().ok());
        if let Some((l, m, hits)) = self.transcripts.get(path)
            && *l == len
            && *m == mtime
        {
            return hits.clone();
        }
        // Kapanan oturumların dökümleri birikmesin.
        if self.transcripts.len() > 32 {
            self.transcripts.clear();
        }
        let hits = read_tail(path, len).map(|t| parse_transcript(&t)).unwrap_or_default();
        self.transcripts.insert(path.to_path_buf(), (len, mtime, hits.clone()));
        hits
    }
}

/// Claude Code'un pid kaydından oturum dökümünün yolu.
fn claude_transcript(pid: u32) -> Option<PathBuf> {
    let base = kota_core::paths::home_dir().join(".claude");
    let reg: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(base.join("sessions").join(format!("{pid}.json"))).ok()?).ok()?;
    let session = reg.get("sessionId")?.as_str()?;
    if session.contains('/') || session.contains("..") {
        return None;
    }
    let file = format!("{session}.jsonl");
    if let Some(cwd) = reg.get("cwd").and_then(|v| v.as_str()) {
        let p = base.join("projects").join(encode_project_dir(cwd)).join(&file);
        if p.is_file() {
            return Some(p);
        }
    }
    // Kodlama değişirse: tüm proje dizinlerinde ara.
    fs::read_dir(base.join("projects"))
        .ok()?
        .flatten()
        .map(|e| e.path().join(&file))
        .find(|p| p.is_file())
}

/// Claude Code proje dizini adı: harf/rakam dışındaki her karakter `-` olur.
fn encode_project_dir(cwd: &str) -> String {
    cwd.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

fn read_tail(path: &Path, len: u64) -> Option<String> {
    let mut f = fs::File::open(path).ok()?;
    let start = len.saturating_sub(TAIL_BYTES);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::with_capacity((len - start) as usize);
    f.read_to_end(&mut buf).ok()?;
    let mut text = String::from_utf8_lossy(&buf).into_owned();
    if start > 0 {
        // Yarım kalan ilk satırı at.
        let cut = text.find('\n').map_or(text.len(), |i| i + 1);
        text.drain(..cut);
    }
    Some(text)
}

/// Döküm satırlarından yollar: `cwd`/`workdir`, araç girdilerindeki dosya
/// yolları ve kabuk komutlarındaki yollar. Claude ve Codex biçimlerinin ikisi
/// de aynı yürüyüşle okunur (Codex araç argümanları JSON metni olarak gelir).
fn parse_transcript(text: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    let home = kota_core::paths::home_dir();
    for line in text.lines() {
        if line.len() > MAX_LINE
            || !(line.contains("\"cwd\"") || line.contains("tool_use") || line.contains("function_call"))
        {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let at = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&Utc));
        let mut paths = Vec::new();
        // Claude: yalnız mesaj üst bilgisi ve araç çağrıları (araç *sonuçları* değil).
        if let Some(cwd) = v.get("cwd").and_then(|c| c.as_str()) {
            paths.push(PathBuf::from(cwd));
        }
        if let Some(content) = v.pointer("/message/content").and_then(|c| c.as_array()) {
            for item in content.iter().filter(|i| i.get("type").and_then(|t| t.as_str()) == Some("tool_use")) {
                if let Some(input) = item.get("input") {
                    walk(input, &home, &mut paths, 0);
                }
            }
        }
        // Codex: oturum/tur bağlamı ve işlev çağrıları.
        if let Some(payload) = v.get("payload") {
            let ty = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if matches!(ty, "turn_context" | "session_meta" | "function_call" | "custom_tool_call" | "local_shell_call")
                || v.get("type").and_then(|t| t.as_str()) == Some("turn_context")
            {
                walk(payload, &home, &mut paths, 0);
            }
        }
        paths.truncate(16);
        hits.extend(paths.into_iter().map(|path| Hit { path, at }));
    }
    hits
}

/// JSON içinde yol taşıyan alanları toplar.
fn walk(v: &serde_json::Value, home: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    use serde_json::Value;
    if depth > 6 {
        return;
    }
    match v {
        Value::Object(map) => {
            for (k, val) in map {
                match (k.as_str(), val) {
                    ("cwd" | "workdir" | "file_path" | "notebook_path" | "path", Value::String(p)) if p.starts_with('/') => {
                        out.push(PathBuf::from(p));
                    }
                    ("command" | "cmd" | "input" | "patch", Value::String(c)) => out.extend(command_paths(c, home).into_iter().take(8)),
                    ("command" | "cmd", Value::Array(parts)) => {
                        let joined: Vec<&str> = parts.iter().filter_map(|x| x.as_str()).collect();
                        out.extend(command_paths(&joined.join(" "), home).into_iter().take(8));
                    }
                    ("arguments", Value::String(a)) => {
                        if let Ok(inner) = serde_json::from_str::<Value>(a) {
                            walk(&inner, home, out, depth + 1);
                        }
                    }
                    (_, Value::Object(_) | Value::Array(_)) => walk(val, home, out, depth + 1),
                    _ => {}
                }
            }
        }
        Value::Array(items) => {
            for i in items {
                walk(i, home, out, depth + 1);
            }
        }
        _ => {}
    }
}

/// Codex'in süreç ağacında açık tuttuğu oturum dökümü.
fn open_codex_rollout(tree: &[u32]) -> Option<PathBuf> {
    let sessions = kota_core::paths::codex_home().join("sessions");
    for pid in tree.iter().take(32) {
        let Ok(dir) = fs::read_dir(format!("/proc/{pid}/fd")) else { continue };
        for e in dir.flatten() {
            if let Ok(target) = fs::read_link(e.path())
                && target.starts_with(&sessions)
                && target.extension().is_some_and(|x| x == "jsonl")
            {
                return Some(target);
            }
        }
    }
    None
}

/// Komut metnindeki `/…` ve `~/…` yolları (tırnak ve noktalama ayıklanır).
fn command_paths(cmd: &str, home: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for tok in cmd.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ';' | '|' | '&' | '(' | ')' | '<' | '>' | '=' | ',')) {
        let tok = tok.trim_end_matches([':', '.']);
        let p = if let Some(rest) = tok.strip_prefix("~/") {
            home.join(rest)
        } else if tok.starts_with('/') && tok.len() > 1 {
            PathBuf::from(tok)
        } else {
            continue;
        };
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// Kök altındaki alt proje klasörü (yoksa `None`: kökün kendisi ya da dışı).
pub fn subproject(root: &Path, path: &Path) -> Option<PathBuf> {
    let rel = path.strip_prefix(root).ok()?;
    let parts: Vec<&std::ffi::OsStr> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s),
            _ => None,
        })
        .collect();
    let first = parts.first()?;
    let first_s = first.to_string_lossy();
    if first_s.starts_with('.') || IGNORED.contains(&first_s.as_ref()) {
        return None;
    }
    // Yolun klasör kısmı: son bileşen dosyaysa (ya da uzantılı ve yoksa) dahil edilmez.
    let last_is_file = path.is_file() || (!path.exists() && path.extension().is_some());
    let dirs = if last_is_file { &parts[..parts.len() - 1] } else { &parts[..] };
    if dirs.is_empty() {
        return None;
    }
    let mut cur = root.to_path_buf();
    for (i, d) in dirs.iter().enumerate() {
        let name = d.to_string_lossy();
        if i > 0 && (name.starts_with('.') || IGNORED.contains(&name.as_ref())) {
            break;
        }
        cur.push(d);
        if MARKERS.iter().any(|m| cur.join(m).exists()) {
            return Some(cur);
        }
        if i >= 3 {
            break;
        }
    }
    Some(root.join(first))
}

/// İsabetleri alt projelere toplar; en son dokunulan önce.
fn rank(root: &Path, hits: &[Hit]) -> Vec<Focus> {
    let mut map: HashMap<PathBuf, (Option<DateTime<Utc>>, u32)> = HashMap::new();
    for h in hits {
        let Some(sub) = subproject(root, &h.path) else { continue };
        let e = map.entry(sub).or_insert((None, 0));
        e.0 = e.0.max(h.at);
        e.1 += 1;
    }
    let mut v: Vec<(PathBuf, Option<DateTime<Utc>>, u32)> = map.into_iter().map(|(p, (a, n))| (p, a, n)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)).then(a.0.cmp(&b.0)));
    v.truncate(MAX_FOCUS);
    v.into_iter()
        .map(|(p, at, hits)| Focus {
            name: p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            path: crate::agents::tilde(&p),
            at,
            hits,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_paths_to_subprojects() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("kota/crates/kota/src")).unwrap();
        fs::write(root.join("kota/Cargo.toml"), "").unwrap();
        fs::write(root.join("kota/crates/kota/Cargo.toml"), "").unwrap();
        fs::write(root.join("kota/crates/kota/src/main.rs"), "").unwrap();
        fs::create_dir_all(root.join("1 - Projects/serrat")).unwrap();
        fs::write(root.join("1 - Projects/serrat/setup.py"), "").unwrap();
        fs::create_dir_all(root.join("notes")).unwrap();

        // En dıştaki işaretli klasör kazanır.
        assert_eq!(subproject(root, &root.join("kota/crates/kota/src/main.rs")), Some(root.join("kota")));
        // İşaretsiz kategori klasörünün altındaki proje.
        assert_eq!(
            subproject(root, &root.join("1 - Projects/serrat/app.py")),
            Some(root.join("1 - Projects/serrat"))
        );
        // İşaret yoksa ilk alt klasör.
        assert_eq!(subproject(root, &root.join("notes/a.md")), Some(root.join("notes")));
        // Kök, kökteki dosya, gizli klasör ve dışarısı sayılmaz.
        assert_eq!(subproject(root, root), None);
        assert_eq!(subproject(root, &root.join("README.md")), None);
        assert_eq!(subproject(root, &root.join(".idea/x.xml")), None);
        assert_eq!(subproject(root, Path::new("/etc/hosts")), None);
    }

    #[test]
    fn parses_transcript_and_ranks_by_recency() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for p in ["a", "b"] {
            fs::create_dir_all(root.join(p)).unwrap();
            fs::write(root.join(p).join("package.json"), "{}").unwrap();
        }
        let r = root.display();
        let text = format!(
            "{{\"timestamp\":\"2026-11-07T10:00:00Z\",\"cwd\":\"{r}\",\"message\":{{\"content\":[{{\"type\":\"tool_use\",\"input\":{{\"file_path\":\"{r}/a/x.js\"}}}}]}}}}\n\
             not json\n\
             {{\"timestamp\":\"2026-11-07T11:00:00Z\",\"cwd\":\"{r}/b\",\"message\":{{\"content\":\"text\"}}}}\n"
        );
        let hits = parse_transcript(&text);
        assert_eq!(hits.len(), 3);
        let focus = rank(root, &hits);
        assert_eq!(focus.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["b", "a"]);
        assert_eq!(focus[1].hits, 1);
    }

    #[test]
    fn parses_codex_rollout() {
        let text = r#"{"timestamp":"2026-11-07T10:00:00Z","type":"turn_context","payload":{"cwd":"/w/root","model":"x"}}
{"timestamp":"2026-11-07T10:01:00Z","type":"response_item","payload":{"type":"function_call","name":"shell","arguments":"{\"command\":[\"bash\",\"-lc\",\"cat /w/root/app/src/a.rs\"],\"workdir\":\"/w/root/app\"}"}}
{"timestamp":"2026-11-07T10:02:00Z","type":"response_item","payload":{"type":"function_call_output","output":"/w/root/ignored"}}
"#;
        let hits = parse_transcript(text);
        let paths: Vec<_> = hits.iter().map(|h| h.path.display().to_string()).collect();
        assert_eq!(paths, ["/w/root", "/w/root/app/src/a.rs", "/w/root/app"]);
    }

    #[test]
    fn extracts_command_paths() {
        let home = Path::new("/home/u");
        let v = command_paths("cd /home/u/YAZILIM/kota && cat ~/notes/a.md | grep 'x' > /tmp/o.txt; echo hi", home);
        assert_eq!(
            v,
            [
                PathBuf::from("/home/u/YAZILIM/kota"),
                PathBuf::from("/home/u/notes/a.md"),
                PathBuf::from("/tmp/o.txt")
            ]
        );
    }

    #[test]
    fn encodes_claude_project_dir() {
        assert_eq!(encode_project_dir("/home/u/YAZILIM"), "-home-u-YAZILIM");
        assert_eq!(encode_project_dir("/home/u/my.app_x"), "-home-u-my-app-x");
    }
}
