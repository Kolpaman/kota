//! "Bağlan" akışı: sağlayıcının kendi giriş komutunu bir terminalde açar.
//! Kota parola veya token görmez; giriş tamamen CLI'nın kendi akışıdır.
//! Servis giriş dosyasını izlediği için bağlantı kendiliğinden algılanır.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use kota_core::providers;

use crate::i18n::Lang;

pub fn open_login_terminal(provider: &str) -> Result<(), String> {
    let argv = providers::login_command(provider).ok_or_else(|| format!("unknown provider: {provider}"))?;
    let bin = find_in_path(argv[0]).ok_or_else(|| format!("`{}` was not found in PATH", argv[0]))?;
    let mut line = shell_quote(&bin.to_string_lossy());
    for a in &argv[1..] {
        line.push(' ');
        line.push_str(&shell_quote(a));
    }
    let script = format!(
        "{line}; echo; printf '%s' {}; read -r _",
        shell_quote(Lang::detect().press_enter())
    );
    let mut last_err = None;
    for cand in terminal_candidates(&script) {
        let Some(exe) = find_in_path(&cand[0]) else { continue };
        let mut argv: Vec<String> = vec![exe.to_string_lossy().into_owned()];
        argv.extend(cand[1..].iter().cloned());
        match launch_detached(&argv) {
            Ok(how) => {
                log::info!("opened {} for {provider} login ({how})", exe.display());
                return Ok(());
            }
            Err(e) => last_err = Some(format!("{}: {e}", exe.display())),
        }
    }
    Err(last_err.unwrap_or_else(|| "no terminal emulator found; set KOTA_TERMINAL (e.g. \"ptyxis --\")".into()))
}

/// Terminali servisin cgroup'u dışında başlatır. Servisten doğrudan
/// başlatılsaydı `systemctl --user restart kota` terminali (ve ilk açılan
/// terminal sunucusuysa kullanıcının tüm terminal pencerelerini) öldürürdü.
/// Bu yüzden önce `systemd-run --user` ile ayrı, geçici bir birim denenir.
fn launch_detached(argv: &[String]) -> std::io::Result<&'static str> {
    if let Some(run) = find_in_path("systemd-run") {
        let status = Command::new(run)
            .args(["--user", "--collect", "--quiet", "--"])
            .args(argv)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match status {
            Ok(s) if s.success() => return Ok("systemd-run"),
            Ok(s) => log::debug!("systemd-run exited with {s}; spawning directly"),
            Err(e) => log::debug!("systemd-run failed: {e}; spawning directly"),
        }
    }
    let mut child = Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    // Zombi süreç bırakmamak için arka planda bekle.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok("direct")
}

/// Denenecek terminaller; her biri sonuna `sh -c <script>` alır.
fn terminal_candidates(script: &str) -> Vec<Vec<String>> {
    let with = |prefix: &[&str]| -> Vec<String> {
        prefix
            .iter()
            .map(|s| s.to_string())
            .chain(["sh".to_string(), "-c".to_string(), script.to_string()])
            .collect()
    };
    let mut v = Vec::new();
    if let Ok(custom) = std::env::var("KOTA_TERMINAL") {
        let parts: Vec<&str> = custom.split_whitespace().collect();
        if !parts.is_empty() {
            v.push(with(&parts));
        }
    }
    v.push(with(&["xdg-terminal-exec"]));
    // Debian/Ubuntu'nun seçili terminali
    v.push(with(&["x-terminal-emulator", "-e"]));
    v.push(with(&["ptyxis", "--"]));
    v.push(with(&["kgx", "--"]));
    v.push(with(&["gnome-terminal", "--"]));
    v.push(with(&["konsole", "-e"]));
    v.push(with(&["kitty"]));
    v.push(with(&["alacritty", "-e"]));
    v.push(with(&["foot"]));
    v.push(with(&["wezterm", "start", "--"]));
    v.push(with(&["xfce4-terminal", "-x"]));
    v.push(with(&["mate-terminal", "-x"]));
    v.push(with(&["tilix", "-e"]));
    v.push(with(&["terminator", "-x"]));
    v.push(with(&["lxterminal", "-e"]));
    v.push(with(&["qterminal", "-e"]));
    v.push(with(&["ghostty", "-e"]));
    v.push(with(&["xterm", "-e"]));
    v
}

/// PATH'te arar; GNOME Shell/systemd ortamında PATH'e girmeyebilen
/// `~/.local/bin` gibi kullanıcı dizinlerine de bakar.
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let p = PathBuf::from(name);
        return is_executable(&p).then_some(p);
    }
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let home = kota_core::paths::home_dir();
    for extra in [".local/bin", ".npm-global/bin", ".bun/bin", ".cargo/bin"] {
        dirs.push(home.join(extra));
    }
    dirs.into_iter().map(|d| d.join(name)).find(|p| is_executable(p))
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn candidates_end_with_script() {
        let c = terminal_candidates("echo hi");
        assert!(
            c.iter()
                .all(|v| v.ends_with(&["sh".into(), "-c".into(), "echo hi".into()]))
        );
        assert!(c.iter().any(|v| v[0] == "ptyxis" && v[1] == "--"));
    }

    #[test]
    fn unknown_provider_rejected() {
        assert!(open_login_terminal("nope").unwrap_err().contains("unknown provider"));
    }

    #[test]
    fn finds_sh() {
        assert!(find_in_path("sh").is_some());
        assert!(find_in_path("definitely-not-a-binary-kota").is_none());
    }
}
