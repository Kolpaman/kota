//! Dosya konumları. CLI'ların kendi ortam değişkenlerine saygı gösterilir.

use std::path::PathBuf;

pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Claude Code yapılandırma dizini (`$CLAUDE_CONFIG_DIR` veya `~/.claude`).
pub fn claude_config_dir() -> PathBuf {
    env_path("CLAUDE_CONFIG_DIR").unwrap_or_else(|| home_dir().join(".claude"))
}

/// Codex dizini (`$CODEX_HOME` veya `~/.codex`).
pub fn codex_home() -> PathBuf {
    env_path("CODEX_HOME").unwrap_or_else(|| home_dir().join(".codex"))
}

/// `~/.config/kota/config.toml`
pub fn config_file() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| home_dir().join(".config"))
        .join("kota")
        .join("config.toml")
}

/// `~/.cache/kota` — yalnızca token içermeyen son anlık görüntü burada tutulur.
pub fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| home_dir().join(".cache"))
        .join("kota")
}
