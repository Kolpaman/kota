//! Çalışan AI ajanlarını `/proc` taramasıyla bulur: hangi projede, ne
//! kadar bellek, ne zamandan beri boşta, hangi portları dinliyor
//! (ajanın başlattığı geliştirme sunucuları dahil).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use kota_core::model::Agent;

/// (tür, görünen ad, tanıma anahtarları). Anahtar; exe/argv[0] adıyla tam
/// eşleşmeli ya da node/bun/python betiğinin yolunda geçmelidir.
const KINDS: &[(&str, &str, &[&str])] = &[
    ("claude", "Claude Code", &["claude", "@anthropic-ai/claude-code"]),
    ("codex", "Codex", &["codex", "@openai/codex"]),
    ("gemini", "Gemini CLI", &["gemini", "@google/gemini-cli"]),
    ("copilot", "Copilot CLI", &["copilot", "@github/copilot"]),
    ("cursor", "Cursor Agent", &["cursor-agent"]),
    ("qwen", "Qwen Code", &["qwen", "@qwen-code/qwen-code"]),
    ("opencode", "opencode", &["opencode", "opencode-ai"]),
    ("aider", "Aider", &["aider"]),
    ("goose", "Goose", &["goose"]),
    ("amp", "Amp", &["amp", "@sourcegraph/amp"]),
    ("crush", "Crush", &["crush"]),
    ("kimi", "Kimi CLI", &["kimi"]),
    ("droid", "Factory Droid", &["droid"]),
];

const INTERPRETERS: &[&str] = &["node", "bun", "deno", "python", "python3", "uv", "npx", "pnpm"];
/// Editör köprüleri ajan sayılmaz (asıl ajan alt süreçtir).
const ADAPTERS: &[&str] = &["acp", "-mcp", "language-server", "lsp"];

#[derive(Debug, Clone)]
struct Proc {
    pid: u32,
    ppid: u32,
    argv: Vec<String>,
    exe: String,
    cpu_ticks: u64,
    start_ticks: u64,
    rss_kb: u64,
}

#[derive(Debug, Default)]
pub struct AgentScanner {
    /// pid → (son CPU tick toplamı, son etkinlik anı)
    activity: HashMap<u32, (u64, DateTime<Utc>)>,
    boot_time: Option<i64>,
    focus: crate::focus::FocusFinder,
}

fn basename(s: &str) -> &str {
    s.rsplit('/').next().unwrap_or(s)
}

fn read_proc(pid: u32) -> Option<Proc> {
    let base = PathBuf::from(format!("/proc/{pid}"));
    let stat = fs::read_to_string(base.join("stat")).ok()?;
    // comm parantez içinde ve boşluk içerebilir: son ')' sonrasını böl.
    let rest = &stat[stat.rfind(')')? + 2..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    let num = |i: usize| f.get(i).and_then(|x| x.parse::<u64>().ok()).unwrap_or(0);
    let argv: Vec<String> = fs::read(base.join("cmdline"))
        .ok()?
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    let rss_kb = fs::read_to_string(base.join("status"))
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
        })
        .unwrap_or(0);
    Some(Proc {
        pid,
        ppid: num(1) as u32,
        exe: fs::read_link(base.join("exe"))
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        argv,
        // utime(11) + stime(12) + cutime(13) + cstime(14), alanlar state'ten (0) sayılır
        cpu_ticks: num(11) + num(12) + num(13) + num(14),
        start_ticks: num(19),
        rss_kb,
    })
}

/// Sürecin ajan türü (yoksa `None`).
fn classify(p: &Proc) -> Option<(&'static str, &'static str)> {
    let arg0 = p.argv.first().map(|a| basename(a)).unwrap_or("");
    let exe = basename(&p.exe);
    let joined = p.argv.iter().take(3).cloned().collect::<Vec<_>>().join(" ");
    // Yalnızca süreç adına bak: tam yol (ör. …/claude-acp/node_modules/…/claude) asıl ajanı da içerebilir.
    if ADAPTERS.iter().any(|a| arg0.contains(a) || exe.contains(a)) {
        return None;
    }
    for (kind, name, keys) in KINDS {
        for key in *keys {
            if key.starts_with('@') || key.contains('-') {
                if joined.contains(key) {
                    return Some((kind, name));
                }
            } else if arg0 == *key || exe == *key {
                return Some((kind, name));
            }
        }
    }
    // `node /…/bin/codex` gibi: yorumlayıcının çalıştırdığı betiğin adı.
    if INTERPRETERS.contains(&arg0) || INTERPRETERS.iter().any(|i| exe.starts_with(i)) {
        let script = p
            .argv
            .iter()
            .skip(1)
            .find(|a| !a.starts_with('-'))
            .map(|a| basename(a))?;
        return KINDS
            .iter()
            .find(|(_, _, keys)| keys.iter().any(|k| !k.starts_with('@') && script == *k))
            .map(|(kind, name, _)| (*kind, *name));
    }
    None
}

/// Dinlenen TCP portları: soket inode → port.
fn listening_sockets() -> HashMap<u64, u16> {
    let mut map = HashMap::new();
    for file in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let Ok(text) = fs::read_to_string(file) else { continue };
        for line in text.lines().skip(1) {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 10 || f[3] != "0A" {
                continue; // 0A = LISTEN
            }
            let Some(port) = f[1].rsplit(':').next().and_then(|h| u16::from_str_radix(h, 16).ok()) else {
                continue;
            };
            if let Ok(inode) = f[9].parse::<u64>() {
                map.insert(inode, port);
            }
        }
    }
    map
}

fn ports_of(pids: &[u32], sockets: &HashMap<u64, u16>) -> Vec<u16> {
    let mut ports = HashSet::new();
    for pid in pids {
        let Ok(dir) = fs::read_dir(format!("/proc/{pid}/fd")) else {
            continue;
        };
        for fd in dir.flatten() {
            let Ok(target) = fs::read_link(fd.path()) else { continue };
            let t = target.to_string_lossy();
            if let Some(inode) = t.strip_prefix("socket:[").and_then(|s| s.strip_suffix(']'))
                && let Some(p) = inode.parse::<u64>().ok().and_then(|i| sockets.get(&i))
            {
                ports.insert(*p);
            }
        }
    }
    let mut v: Vec<u16> = ports.into_iter().collect();
    v.sort_unstable();
    v
}

/// Git kökü (yoksa dizinin kendisi) ve adı.
fn project_of(cwd: &Path) -> (String, PathBuf) {
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        if d.join(".git").exists() {
            let name = d
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "/".into());
            return (name, d.to_path_buf());
        }
        dir = d.parent();
    }
    let name = cwd
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".into());
    (name, cwd.to_path_buf())
}

pub(crate) fn tilde(p: &Path) -> String {
    let home = kota_core::paths::home_dir();
    match p.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

fn boot_time() -> Option<i64> {
    fs::read_to_string("/proc/stat")
        .ok()?
        .lines()
        .find_map(|l| l.strip_prefix("btime ")?.trim().parse().ok())
}

impl AgentScanner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scan(&mut self, now: DateTime<Utc>) -> Vec<Agent> {
        let uid = fs::metadata("/proc/self")
            .map(|m| std::os::unix::fs::MetadataExt::uid(&m))
            .ok();
        let mut procs: HashMap<u32, Proc> = HashMap::new();
        if let Ok(dir) = fs::read_dir("/proc") {
            for e in dir.flatten() {
                let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
                    continue;
                };
                // Yalnızca kullanıcının kendi süreçleri.
                if uid.is_some() && e.metadata().map(|m| std::os::unix::fs::MetadataExt::uid(&m)).ok() != uid {
                    continue;
                }
                if let Some(p) = read_proc(pid) {
                    procs.insert(pid, p);
                }
            }
        }
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for p in procs.values() {
            children.entry(p.ppid).or_default().push(p.pid);
        }
        let matched: HashMap<u32, (&str, &str)> =
            procs.values().filter_map(|p| classify(p).map(|k| (p.pid, k))).collect();
        // Atası da ajan olan eşleşmeler (sarmalayıcı + yerel binary) tek sayılır.
        let is_nested = |pid: u32| {
            let mut cur = procs.get(&pid).map(|p| p.ppid);
            let mut guard = 0;
            while let Some(pp) = cur {
                if pp <= 1 || guard > 64 {
                    return false;
                }
                if matched.contains_key(&pp) {
                    return true;
                }
                cur = procs.get(&pp).map(|p| p.ppid);
                guard += 1;
            }
            false
        };
        let sockets = listening_sockets();
        self.boot_time = self.boot_time.or_else(boot_time);
        let mut agents = Vec::new();
        let mut alive = HashSet::new();
        for (&pid, &(kind, name)) in &matched {
            if is_nested(pid) {
                continue;
            }
            // Süreç ağacı
            let mut tree = vec![pid];
            let mut i = 0;
            while i < tree.len() {
                if let Some(c) = children.get(&tree[i]) {
                    for &child in c {
                        if !tree.contains(&child) {
                            tree.push(child);
                        }
                    }
                }
                i += 1;
                if tree.len() > 2000 {
                    break;
                }
            }
            let ticks: u64 = tree.iter().filter_map(|p| procs.get(p)).map(|p| p.cpu_ticks).sum();
            let rss: u64 = tree.iter().filter_map(|p| procs.get(p)).map(|p| p.rss_kb).sum::<u64>() * 1024;
            let started_at = self.boot_time.and_then(|bt| {
                let secs = procs.get(&pid)?.start_ticks / 100;
                DateTime::from_timestamp(bt + secs as i64, 0)
            });
            let active_at = match self.activity.get(&pid) {
                Some((prev, at)) if *prev == ticks => *at,
                Some(_) => now,
                // İlk görüş: geçmiş bilinmiyor; "az önce etkin" say (en geç başlangıç anı).
                None => started_at.unwrap_or(now).max(now - Duration::seconds(60)),
            };
            self.activity.insert(pid, (ticks, active_at));
            alive.insert(pid);
            let cwd = fs::read_link(format!("/proc/{pid}/cwd")).unwrap_or_else(|_| PathBuf::from("/"));
            let (project, root) = project_of(&cwd);
            // Ajanın ve alt süreçlerinin dizinleri (kabuklar, derleyiciler, sunucular).
            let cwds: Vec<PathBuf> = tree
                .iter()
                .take(64)
                .filter_map(|p| fs::read_link(format!("/proc/{p}/cwd")).ok())
                .collect();
            let found = self.focus.find(kind, pid, &tree, &root, &cwds, now);
            agents.push(Agent {
                pid,
                kind: kind.to_string(),
                name: name.to_string(),
                cwd: tilde(&cwd),
                project,
                project_path: tilde(&root),
                started_at,
                active_at: Some(active_at),
                rss_bytes: rss,
                ports: ports_of(&tree, &sockets),
                children: (tree.len() - 1) as u32,
                focus: found.focus,
                tokens: found.tokens,
            });
        }
        self.activity.retain(|pid, _| alive.contains(pid));
        agents.sort_by(|a, b| {
            a.project
                .cmp(&b.project)
                .then(a.started_at.cmp(&b.started_at))
                .then(a.pid.cmp(&b.pid))
        });
        agents
    }
}

/// Paneli gereksiz yere yeniden çizmemek için: yapısal değişiklik var mı?
pub fn structurally_differs(a: &[Agent], b: &[Agent]) -> bool {
    let key = |x: &Agent| {
        (
            x.pid,
            x.ports.clone(),
            x.children,
            x.rss_bytes / (64 * 1024 * 1024),
            x.focus.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
            x.tokens.map(|t| t / 100_000),
        )
    };
    a.len() != b.len() || a.iter().zip(b).any(|(x, y)| key(x) != key(y))
}

/// Yalnızca bu tarayıcının bulduğu bir ajan durdurulabilir.
pub fn stop(pid: u32, known: &[Agent]) -> Result<(), String> {
    if !known.iter().any(|a| a.pid == pid) {
        return Err(format!("pid {pid} is not a detected agent"));
    }
    let status = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("kill failed: {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(argv: &[&str], exe: &str) -> Proc {
        Proc {
            pid: 1,
            ppid: 0,
            argv: argv.iter().map(|s| s.to_string()).collect(),
            exe: exe.into(),
            cpu_ticks: 0,
            start_ticks: 0,
            rss_kb: 0,
        }
    }

    #[test]
    fn classifies_common_launchers() {
        assert_eq!(
            classify(&p(&["claude"], "/home/u/.local/share/claude/versions/2.1.319"))
                .unwrap()
                .0,
            "claude"
        );
        assert_eq!(
            classify(&p(
                &["/usr/bin/node", "/usr/lib/node_modules/@openai/codex/bin/codex.js"],
                "/usr/bin/node"
            ))
            .unwrap()
            .0,
            "codex"
        );
        assert_eq!(
            classify(&p(&["node", "/usr/local/bin/gemini"], "/usr/bin/node-22"))
                .unwrap()
                .0,
            "gemini"
        );
        assert_eq!(
            classify(&p(
                &["/usr/bin/python3", "/home/u/.local/bin/aider", "--model", "x"],
                "/usr/bin/python3.13"
            ))
            .unwrap()
            .0,
            "aider"
        );
        assert_eq!(
            classify(&p(&["cursor-agent"], "/opt/cursor-agent")).unwrap().0,
            "cursor"
        );
        // Editör köprüsü ve alakasız süreçler
        assert!(classify(&p(&["node", "/x/npx/claude-acp/dist/index.js"], "/usr/bin/node")).is_none());
        // Zed'in ACP köprüsünün başlattığı asıl Claude Code binary'si sayılır.
        assert_eq!(
            classify(&p(
                &["/home/u/.var/app/dev.zed.Zed/data/zed/external_agents/registry/npx/claude-acp/node_modules/@anthropic-ai/claude-agent-sdk-linux-x64/claude"],
                "/home/u/.var/app/dev.zed.Zed/data/zed/external_agents/registry/npx/claude-acp/node_modules/@anthropic-ai/claude-agent-sdk-linux-x64/claude"
            ))
            .unwrap()
            .0,
            "claude"
        );
        assert!(classify(&p(&["claude-code-acp"], "/usr/bin/claude-code-acp")).is_none());
        assert!(classify(&p(&["zsh"], "/usr/bin/zsh")).is_none());
        assert!(classify(&p(&["node", "server.js"], "/usr/bin/node")).is_none());
    }

    #[test]
    fn project_root_from_git() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("rueya-studio");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("src/app")).unwrap();
        let (name, root) = project_of(&repo.join("src/app"));
        assert_eq!(name, "rueya-studio");
        assert_eq!(root, repo);
    }

    #[test]
    fn scan_runs_on_this_machine() {
        let mut s = AgentScanner::new();
        let a = s.scan(Utc::now());
        // Bu test bir ajan içinde çalışıyorsa en az biri bulunur; değilse boş liste de geçerli.
        for x in &a {
            assert!(x.pid > 0 && !x.project.is_empty());
        }
    }

    #[test]
    fn stop_refuses_unknown_pid() {
        assert!(stop(1, &[]).is_err());
    }
}
