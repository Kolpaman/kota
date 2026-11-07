//! Sağlayıcılar. Yeni bir sağlayıcı eklemek: bu dizine bir modül yazıp
//! [`Provider`] trait'ini uygulamak, [`catalog`]'a bir girdi ve [`build`]
//! içine bir satır eklemek. API anahtarlı sağlayıcılar için `keyed/` altına
//! tek bir dosya yeter.

pub mod claude;
pub mod codex;
mod common;
pub mod copilot;
pub mod gemini;
pub mod keyed;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::config::{Config, Thresholds};
use crate::model::ProviderReport;
use crate::secret::Secret;

pub use common::{pretty_plan, window_identity};

/// Tek bir sorgu turunun ortak bağlamı.
#[derive(Clone)]
pub struct FetchContext {
    pub http: reqwest::Client,
    pub thresholds: Thresholds,
    pub now: DateTime<Utc>,
    /// Yerel saatle bugünün başlangıcı (UTC olarak).
    pub today_start: DateTime<Utc>,
}

#[async_trait]
pub trait Provider: Send + Sync {
    /// Kararlı kimlik (`claude`, `codex`, …).
    fn id(&self) -> &'static str;
    /// Görünen ad.
    fn name(&self) -> &'static str;
    /// Değişince hemen yeniden sorgulanacak dosyalar (giriş bilgileri).
    fn watched_files(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    /// Asla panik yapmaz; her hata durumu rapora yazılır.
    async fn fetch(&self, ctx: &FetchContext) -> ProviderReport;
}

/// API anahtarlarının kaynağı. Linux'ta `kota` binary'si GNOME Anahtarlığı'nı
/// (libsecret) kullanır; çekirdek bundan habersizdir.
pub trait KeyStore: Send + Sync {
    fn get(&self, provider: &str) -> Option<Secret>;
}

/// `KOTA_<ID>_KEY` ortam değişkenleri (ör. `KOTA_DEEPSEEK_KEY`).
pub struct EnvKeys;

impl KeyStore for EnvKeys {
    fn get(&self, provider: &str) -> Option<Secret> {
        let var = format!("KOTA_{}_KEY", provider.to_uppercase());
        std::env::var(var)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(|v| Secret::new(v.trim()))
    }
}

/// Testler ve sabit anahtarlar için.
pub struct StaticKeys(pub HashMap<String, String>);

impl KeyStore for StaticKeys {
    fn get(&self, provider: &str) -> Option<Secret> {
        self.0.get(provider).map(Secret::new)
    }
}

/// Giriş türü (ayarlar penceresi buna göre alan gösterir).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    /// Sağlayıcının CLI'ının giriş dosyası (Claude Code, Codex, Gemini CLI).
    CliLogin,
    /// API anahtarı (anahtarlıkta saklanır).
    ApiKey,
    /// Kuruluş yönetici anahtarı (harcama raporu).
    AdminKey,
    /// GitHub token'ı veya Copilot eklentisinin giriş dosyası.
    GithubToken,
}

/// Desteklenen sağlayıcılar listesi.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub auth: AuthKind,
    /// Ne gösterildiği (İngilizce; arayüz yerelleştirir).
    pub shows: &'static str,
    /// Anahtarın alınacağı sayfa.
    pub key_url: Option<&'static str>,
    /// Aylık bütçe girilebilir (harcama → yüzde).
    pub budget: bool,
    pub login_command: Option<&'static [&'static str]>,
}

pub fn catalog() -> Vec<CatalogEntry> {
    let e = |id, name, auth, shows, key_url, budget, login_command| CatalogEntry {
        id,
        name,
        auth,
        shows,
        key_url,
        budget,
        login_command,
    };
    use AuthKind::*;
    vec![
        e(
            "claude",
            "Claude",
            CliLogin,
            "plan_limits",
            None,
            false,
            Some(claude::LOGIN_COMMAND),
        ),
        e(
            "codex",
            "Codex",
            CliLogin,
            "plan_limits",
            None,
            false,
            Some(codex::LOGIN_COMMAND),
        ),
        e(
            "gemini",
            "Gemini",
            CliLogin,
            "plan_limits",
            None,
            false,
            Some(gemini::LOGIN_COMMAND),
        ),
        e(
            "copilot",
            "GitHub Copilot",
            GithubToken,
            "plan_limits",
            Some("https://github.com/settings/copilot"),
            false,
            None,
        ),
        e(
            "zai",
            "Z.ai (GLM)",
            ApiKey,
            "plan_limits",
            Some("https://z.ai/manage-apikey/apikey-list"),
            false,
            None,
        ),
        e(
            "openrouter",
            "OpenRouter",
            ApiKey,
            "balance",
            Some("https://openrouter.ai/settings/keys"),
            false,
            None,
        ),
        e(
            "deepseek",
            "DeepSeek",
            ApiKey,
            "balance",
            Some("https://platform.deepseek.com/api_keys"),
            false,
            None,
        ),
        e(
            "moonshot",
            "Kimi (Moonshot)",
            ApiKey,
            "balance",
            Some("https://platform.moonshot.ai/console/api-keys"),
            false,
            None,
        ),
        e(
            "siliconflow",
            "SiliconFlow",
            ApiKey,
            "balance",
            Some("https://cloud.siliconflow.cn/account/ak"),
            false,
            None,
        ),
        e(
            "openai",
            "OpenAI API",
            AdminKey,
            "spend",
            Some("https://platform.openai.com/settings/organization/admin-keys"),
            true,
            None,
        ),
        e(
            "anthropic",
            "Anthropic API",
            AdminKey,
            "spend",
            Some("https://console.anthropic.com/settings/admin-keys"),
            true,
            None,
        ),
    ]
}

/// Yapılandırmada açık olan sağlayıcılar, katalog sırasıyla.
pub fn build(config: &Config, keys: Arc<dyn KeyStore>) -> Vec<Arc<dyn Provider>> {
    let mut v: Vec<Arc<dyn Provider>> = Vec::new();
    for entry in catalog() {
        if !config.is_enabled(entry.id) {
            continue;
        }
        let p: Arc<dyn Provider> = match entry.id {
            "claude" => Arc::new(claude::ClaudeProvider::from_config(&config.claude)),
            "codex" => Arc::new(codex::CodexProvider::from_config(&config.codex)),
            "gemini" => Arc::new(gemini::GeminiProvider::from_default()),
            "copilot" => Arc::new(copilot::CopilotProvider::from_default(keys.clone())),
            id => match keyed::KeyedProvider::new(id, keys.clone(), config.budget(id)) {
                Some(k) => Arc::new(k),
                None => continue,
            },
        };
        v.push(p);
    }
    v
}

/// Bilinen sağlayıcıların giriş komutu (terminalde çalıştırılır).
pub fn login_command(id: &str) -> Option<&'static [&'static str]> {
    catalog().into_iter().find(|e| e.id == id).and_then(|e| e.login_command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_unique_and_buildable() {
        let cat = catalog();
        let mut ids: Vec<_> = cat.iter().map(|e| e.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), cat.len());
        let mut c = Config::default();
        for e in &cat {
            c.set_enabled(e.id, true);
        }
        let built = build(&c, Arc::new(EnvKeys));
        assert_eq!(built.len(), cat.len(), "every catalog entry must build");
        let built_ids: Vec<_> = built.iter().map(|p| p.id()).collect();
        assert_eq!(built_ids, cat.iter().map(|e| e.id).collect::<Vec<_>>());
    }

    #[test]
    fn defaults_are_claude_and_codex() {
        let ids: Vec<_> = build(&Config::default(), Arc::new(EnvKeys))
            .iter()
            .map(|p| p.id())
            .collect();
        assert_eq!(ids, ["claude", "codex"]);
    }
}
