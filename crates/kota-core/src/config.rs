//! `~/.config/kota/config.toml` yapılandırması. Dosya yoksa varsayılanlar
//! kullanılır; her alan isteğe bağlıdır.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::model::Severity;
use crate::paths;

pub const MIN_INTERVAL_SECS: u64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Sağlayıcıların sorgulanma aralığı (saniye, en az 30).
    pub interval_seconds: u64,
    pub thresholds: Thresholds,
    /// Düzenli sorguyu duraklat (elle yenileme çalışır).
    pub paused: bool,
    pub claude: ClaudeConfig,
    pub codex: CodexConfig,
    /// Diğer sağlayıcılar: `[providers.deepseek] enabled = true`.
    pub providers: BTreeMap<String, ProviderToggle>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            interval_seconds: 120,
            thresholds: Thresholds::default(),
            paused: false,
            claude: ClaudeConfig::default(),
            codex: CodexConfig::default(),
            providers: BTreeMap::new(),
        }
    }
}

/// Claude ve Codex dışındaki sağlayıcıların ortak ayarı.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProviderToggle {
    pub enabled: bool,
    /// Aylık bütçe (harcama gösteren sağlayıcılarda yüzde buradan hesaplanır).
    pub budget: Option<f64>,
}

/// Renk ve bildirim eşikleri (yüzde).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Thresholds {
    pub warning: f64,
    pub critical: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            warning: 80.0,
            critical: 90.0,
        }
    }
}

impl Thresholds {
    pub fn classify(&self, used_percent: f64) -> Severity {
        if used_percent >= self.critical {
            Severity::Critical
        } else if used_percent >= self.warning {
            Severity::Warning
        } else {
            Severity::Normal
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ClaudeConfig {
    pub enabled: bool,
    /// Süresi dolan erişim token'ını Claude Code ile aynı kilit protokolüyle
    /// yenile ve dosyaya geri yaz. Kapalıysa süre dolunca "yeniden bağlan" gösterilir.
    pub refresh_tokens: bool,
    /// `~/.claude/projects/**/*.jsonl` loglarından token say.
    pub local_logs: bool,
    /// Varsayılan: `$CLAUDE_CONFIG_DIR` veya `~/.claude`.
    pub config_dir: Option<PathBuf>,
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            refresh_tokens: true,
            local_logs: true,
            config_dir: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CodexConfig {
    pub enabled: bool,
    /// `~/.codex/sessions/**/rollout-*.jsonl` loglarından token ve son limitleri oku.
    pub local_logs: bool,
    /// Varsayılan: `$CODEX_HOME` veya `~/.codex`.
    pub home: Option<PathBuf>,
}

impl Default for CodexConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            local_logs: true,
            home: None,
        }
    }
}

impl Config {
    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_seconds.max(MIN_INTERVAL_SECS))
    }

    /// Dosya yoksa varsayılanları döndürür; bozuksa hata verir.
    pub fn load_from(path: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// Varsayılan konumdan yükler. Dosya bozuksa varsayılanlara düşer ve
    /// uyarıyı ikinci değer olarak döndürür (servis yine de çalışır).
    pub fn load() -> (Self, Option<String>) {
        match Self::load_from(&paths::config_file()) {
            Ok(c) => (c, None),
            Err(e) => (Self::default(), Some(e)),
        }
    }

    /// Örnek, yorumlu yapılandırma dosyası.
    pub fn example() -> &'static str {
        include_str!("../../../config.example.toml")
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        match id {
            "claude" => self.claude.enabled,
            "codex" => self.codex.enabled,
            other => self.providers.get(other).is_some_and(|p| p.enabled),
        }
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) {
        match id {
            "claude" => self.claude.enabled = enabled,
            "codex" => self.codex.enabled = enabled,
            other => self.providers.entry(other.to_string()).or_default().enabled = enabled,
        }
    }

    pub fn budget(&self, id: &str) -> Option<f64> {
        self.providers.get(id).and_then(|p| p.budget).filter(|b| *b > 0.0)
    }

    pub fn set_budget(&mut self, id: &str, budget: Option<f64>) {
        self.providers.entry(id.to_string()).or_default().budget = budget.filter(|b| *b > 0.0);
    }

    /// Atomik olarak yazar (yorumlar korunmaz).
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let body = format!(
            "# Kota yapılandırması. Kota ayarlar penceresi ve `kota` komutları bu dosyayı yeniden yazar.\n# Yorumlu örnek: kota config --example\n\n{}",
            self.to_toml()
        );
        let tmp = path.with_extension(format!("toml.tmp-{}", std::process::id()));
        std::fs::write(&tmp, body)?;
        std::fs::rename(&tmp, path)
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&paths::config_file())
    }

    /// Geçerli ayarlar TOML olarak.
    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_file_uses_defaults() {
        let c: Config = toml::from_str("interval_seconds = 300\n[claude]\nrefresh_tokens = false\n").unwrap();
        assert_eq!(c.interval_seconds, 300);
        assert!(!c.claude.refresh_tokens);
        assert!(c.claude.enabled);
        assert!(c.codex.enabled);
        assert_eq!(c.thresholds.critical, 90.0);
    }

    #[test]
    fn interval_has_floor() {
        let c = Config {
            interval_seconds: 1,
            ..Config::default()
        };
        assert_eq!(c.interval(), Duration::from_secs(MIN_INTERVAL_SECS));
    }

    #[test]
    fn example_parses_to_defaults() {
        let c: Config = toml::from_str(Config::example()).unwrap();
        assert_eq!(c, Config::default());
    }

    #[test]
    fn toggles_and_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("kota/config.toml");
        let mut c = Config::default();
        assert!(c.is_enabled("claude") && !c.is_enabled("deepseek"));
        c.set_enabled("deepseek", true);
        c.set_enabled("codex", false);
        c.set_budget("openai", Some(50.0));
        c.set_budget("anthropic", Some(0.0));
        c.paused = true;
        c.save_to(&p).unwrap();
        let back = Config::load_from(&p).unwrap();
        assert_eq!(back, c);
        assert!(back.is_enabled("deepseek") && !back.is_enabled("codex"));
        assert_eq!(back.budget("openai"), Some(50.0));
        assert_eq!(back.budget("anthropic"), None);
    }

    #[test]
    fn missing_file_is_default() {
        let c = Config::load_from(Path::new("/nonexistent/kota/config.toml")).unwrap();
        assert_eq!(c, Config::default());
    }
}
