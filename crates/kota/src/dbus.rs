//! D-Bus sözleşmesi (oturum veriyolu).
//!
//! ```text
//! servis:   io.github.ensignalp.Kota
//! nesne:    /io/github/ensignalp/Kota
//! arayüz:   io.github.ensignalp.Kota1
//!   GetSnapshot() → s        son anlık görüntü (JSON, bkz. kota_core::model)
//!   Refresh()     → s        şimdi sorgula (10 sn içinde tekrarlanırsa son sonuç)
//!   Connect(s provider)      giriş için terminal aç
//!   ListProviders() → s      katalog + açık mı + anahtarı var mı (JSON)
//!   SetProviderEnabled(s, b) · SetApiKey(s, s) · SetBudget(s, d)
//!   GetSettings() → s · SetSettings(s json)   aralık, eşikler, duraklatma…
//!   StopAgent(u pid)         bulunan bir ajana SIGTERM
//!   ListAccounts() → s · SaveAccount(s, s) → s · SwitchAccount(s, s) · RemoveAccount(s, s)
//!   signal Updated(s)        her yeni anlık görüntüde
//!   property Version s
//! ```

use std::time::Duration;

use kota_core::model::{SCHEMA_VERSION, Snapshot};

pub const BUS_NAME: &str = "io.github.ensignalp.Kota";
pub const OBJECT_PATH: &str = "/io/github/ensignalp/Kota";

#[zbus::proxy(
    interface = "io.github.ensignalp.Kota1",
    default_service = "io.github.ensignalp.Kota",
    default_path = "/io/github/ensignalp/Kota"
)]
pub trait Kota {
    fn get_snapshot(&self) -> zbus::Result<String>;
    fn refresh(&self) -> zbus::Result<String>;
    fn connect(&self, provider: &str) -> zbus::Result<()>;
    fn list_providers(&self) -> zbus::Result<String>;
    fn set_provider_enabled(&self, id: &str, enabled: bool) -> zbus::Result<()>;
    fn set_api_key(&self, id: &str, key: &str) -> zbus::Result<()>;
    fn set_budget(&self, id: &str, amount: f64) -> zbus::Result<()>;
    fn get_settings(&self) -> zbus::Result<String>;
    fn set_settings(&self, patch: &str) -> zbus::Result<()>;
    fn stop_agent(&self, pid: u32) -> zbus::Result<()>;
    #[zbus(property)]
    fn version(&self) -> zbus::Result<String>;
}

/// Servisten anlık görüntü alır. Servis kurulu ama kapalıysa D-Bus onu
/// başlatır (activation); ilk sorgu bitene kadar beklenir.
pub async fn fetch(refresh: bool) -> Result<Snapshot, String> {
    let timeout = Duration::from_secs(if refresh { 90 } else { 30 });
    let json = tokio::time::timeout(timeout, async {
        let conn = zbus::Connection::session().await?;
        let proxy = KotaProxy::new(&conn).await?;
        if refresh {
            proxy.refresh().await
        } else {
            proxy.get_snapshot().await
        }
    })
    .await
    .map_err(|_| "daemon did not answer in time".to_string())?
    .map_err(|e| e.to_string())?;
    let snap: Snapshot = serde_json::from_str(&json).map_err(|e| format!("daemon sent invalid JSON: {e}"))?;
    if snap.schema != SCHEMA_VERSION {
        return Err(format!("daemon schema {} != {}", snap.schema, SCHEMA_VERSION));
    }
    Ok(snap)
}

/// Çalışan servise bağlanır (yoksa `None`; servisi başlatmaz).
pub async fn proxy() -> Option<KotaProxy<'static>> {
    let conn = zbus::Connection::session().await.ok()?;
    let dbus = zbus::fdo::DBusProxy::new(&conn).await.ok()?;
    let name = zbus::names::BusName::try_from(BUS_NAME).ok()?;
    if !dbus.name_has_owner(name).await.ok()? {
        return None;
    }
    KotaProxy::new(&conn).await.ok()
}
