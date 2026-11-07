//! Kota çekirdeği.
//!
//! Platformdan bağımsız katman: sağlayıcıların (Claude, Codex, …) kullanım
//! verisini okur ve tek bir [`model::Snapshot`] olarak döndürür. D-Bus, GNOME
//! veya başka bir arayüze bağımlılığı yoktur; bunlar `kota` binary'sinde yaşar.
//!
//! Gizlilik kuralı: token'lar yalnızca [`secret::Secret`] içinde taşınır,
//! hiçbir zaman loglanmaz, serileştirilmez ve sağlayıcının kendi API'si dışında
//! hiçbir yere gönderilmez.

pub mod config;
pub mod engine;
pub mod forecast;
pub mod http;
pub mod jsonx;
pub mod locallogs;
pub mod model;
pub mod paths;
pub mod providers;
pub mod secret;

/// Kota sürümü (Cargo.toml'dan).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
pub(crate) mod testutil;
