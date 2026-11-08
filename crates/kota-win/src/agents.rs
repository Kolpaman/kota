//! Çalışan AI ajanları (Windows): süreç listesi `sysinfo` ile okunur.
//! Linux sürümündeki `/proc` taramasının karşılığı; alt proje tespiti ve
//! oturum token sayımı ortak `focus.rs` ile yapılır.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use kota_core::model::Agent;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// (tür, görünen ad, tanıma anahtarları): exe adı (uzantısız) tam eşleşir ya
/// da node/bun/python betiğinin yolunda geçer.
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
const INTERPRETERS: &[&str] = &["node", "bun", "deno", "python", "python3", "pythonw", "uv", "npx", "pnpm"];
const ADAPTERS: &[&str] = &["acp", "-mcp", "language-server", "lsp"];

fn stem(s: &str) -> String {
    let base = s.rsplit(['/', '\\']).next().unwrap_or(s).to_ascii_lowercase();
    base.strip_suffix(".exe")
        .or_else(|| base.strip_suffix(".cmd"))
        .or_else(|| base.strip_suffix(".ps1"))
        .unwrap_or(&base)
        .to_string()
}

fn classify(name: &str, cmd: &[String]) -> Option<(&'static str, &'static str)> {
    let exe = stem(name);
    if ADAPTERS.iter().any(|a| exe.contains(a)) {
        return None;
    }
    for (kind, label, keys) in KINDS {
        if keys.iter().any(|k| !k.contains('/') && exe == *k) {
            return Some((kind, label));
        }
    }
    if INTERPRETERS.contains(&exe.as_str()) {
        for arg in cmd.iter().skip(1).take(3) {
            let a = arg.replace('\\', "/").to_ascii_lowercase();
            if ADAPTERS.iter().any(|x| a.contains(x)) {
                return None;
            }
            for (kind, label, keys) in KINDS {
                if keys.iter().any(|k| {
                    if k.contains('/') {
                        a.contains(&format!("/{k}/")) || a.contains(&format!("node_modules/{k}"))
                    } else {
                        stem(&a) == *k
                    }
                }) {
                    return Some((kind, label));
                }
            }
        }
    }
    None
}

/// Git kökü (yoksa dizinin kendisi) ve adı.
fn project_of(cwd: &Path) -> (String, PathBuf) {
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        if d.join(".git").exists() {
            let name = d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            return (name, d.to_path_buf());
        }
        dir = d.parent();
    }
    let name = cwd.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    (name, cwd.to_path_buf())
}

/// Ev dizinini `~` ile kısaltır.
pub(crate) fn tilde(p: &Path) -> String {
    let home = kota_core::paths::home_dir();
    match p.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~\\{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

pub struct AgentScanner {
    sys: System,
    /// pid → (son CPU süresi ms, son etkinlik anı)
    activity: HashMap<u32, (u64, DateTime<Utc>)>,
    focus: crate::focus::FocusFinder,
}

impl AgentScanner {
    pub fn new() -> Self {
        Self {
            sys: System::new(),
            activity: HashMap::new(),
            focus: Default::default(),
        }
    }

    pub fn scan(&mut self, now: DateTime<Utc>) -> Vec<Agent> {
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cmd(UpdateKind::OnlyIfNotSet)
                .with_cwd(UpdateKind::Always)
                .with_memory()
                .with_cpu()
                .with_exe(UpdateKind::OnlyIfNotSet),
        );
        let procs = self.sys.processes();
        let mut children: HashMap<Pid, Vec<Pid>> = HashMap::new();
        for (pid, p) in procs {
            if let Some(pp) = p.parent() {
                children.entry(pp).or_default().push(*pid);
            }
        }
        let matched: HashMap<Pid, (&str, &str)> = procs
            .iter()
            .filter_map(|(pid, p)| {
                let cmd: Vec<String> = p.cmd().iter().map(|s| s.to_string_lossy().into_owned()).collect();
                classify(&p.name().to_string_lossy(), &cmd).map(|k| (*pid, k))
            })
            .collect();
        let nested = |pid: Pid| {
            let mut cur = procs.get(&pid).and_then(|p| p.parent());
            for _ in 0..64 {
                let Some(pp) = cur else { return false };
                if matched.contains_key(&pp) {
                    return true;
                }
                cur = procs.get(&pp).and_then(|p| p.parent());
            }
            false
        };
        let mut agents = Vec::new();
        let mut alive = HashSet::new();
        for (&pid, &(kind, name)) in &matched {
            if nested(pid) {
                continue;
            }
            let mut tree = vec![pid];
            let mut i = 0;
            while i < tree.len() && tree.len() < 2000 {
                if let Some(c) = children.get(&tree[i]) {
                    for ch in c {
                        if !tree.contains(ch) {
                            tree.push(*ch);
                        }
                    }
                }
                i += 1;
            }
            let cpu: u64 = tree.iter().filter_map(|p| procs.get(p)).map(|p| p.accumulated_cpu_time()).sum();
            let rss: u64 = tree.iter().filter_map(|p| procs.get(p)).map(|p| p.memory()).sum();
            let proc = &procs[&pid];
            let started_at = DateTime::from_timestamp(proc.start_time() as i64, 0);
            let id = pid.as_u32();
            let active_at = match self.activity.get(&id) {
                Some((prev, at)) if *prev == cpu => *at,
                Some(_) => now,
                None => started_at.unwrap_or(now).max(now - Duration::seconds(60)),
            };
            self.activity.insert(id, (cpu, active_at));
            alive.insert(id);
            let cwd = proc.cwd().map(Path::to_path_buf).unwrap_or_else(kota_core::paths::home_dir);
            let (project, root) = project_of(&cwd);
            let cwds: Vec<PathBuf> = tree
                .iter()
                .take(64)
                .filter_map(|p| procs.get(p).and_then(|x| x.cwd()).map(Path::to_path_buf))
                .collect();
            let ids: Vec<u32> = tree.iter().map(|p| p.as_u32()).collect();
            let found = self.focus.find(kind, id, &ids, &root, &cwds, now);
            agents.push(Agent {
                pid: id,
                kind: kind.to_string(),
                name: name.to_string(),
                cwd: tilde(&cwd),
                project,
                project_path: tilde(&root),
                started_at,
                active_at: Some(active_at),
                rss_bytes: rss,
                ports: Vec::new(),
                children: (tree.len() - 1) as u32,
                focus: found.focus,
                tokens: found.tokens,
            });
        }
        self.activity.retain(|pid, _| alive.contains(pid));
        agents.sort_by(|a, b| a.project.cmp(&b.project).then(a.started_at.cmp(&b.started_at)).then(a.pid.cmp(&b.pid)));
        agents
    }

    /// Yalnızca bu tarayıcının bulduğu bir ajan durdurulabilir.
    pub fn stop(&mut self, pid: u32, known: &[Agent]) -> Result<(), String> {
        if !known.iter().any(|a| a.pid == pid) {
            return Err(format!("pid {pid} is not a detected agent"));
        }
        match self.sys.process(Pid::from_u32(pid)) {
            Some(p) if p.kill() => Ok(()),
            Some(_) => Err("the process could not be stopped".into()),
            None => Ok(()),
        }
    }
}

/// Paneli gereksiz yere yeniden çizmemek için: yapısal değişiklik var mı?
pub fn structurally_differs(a: &[Agent], b: &[Agent]) -> bool {
    let key = |x: &Agent| {
        (
            x.pid,
            x.children,
            x.rss_bytes / (64 * 1024 * 1024),
            x.focus.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
            x.tokens.map(|t| t / 100_000),
        )
    };
    a.len() != b.len() || a.iter().zip(b).any(|(x, y)| key(x) != key(y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_windows_launchers() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(classify("claude.exe", &s(&["claude.exe"])).unwrap().0, "claude");
        assert_eq!(
            classify(
                "node.exe",
                &s(&["node.exe", r"C:\Users\u\AppData\Roaming\npm\node_modules\@openai\codex\bin\codex.js"])
            )
            .unwrap()
            .0,
            "codex"
        );
        assert_eq!(classify("codex.exe", &s(&["codex.exe"])).unwrap().0, "codex");
        assert!(classify("node.exe", &s(&["node.exe", "server.js"])).is_none());
        assert!(classify("claude-code-acp.exe", &s(&[])).is_none());
    }
}
