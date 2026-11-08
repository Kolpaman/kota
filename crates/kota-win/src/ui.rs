//! Arayüz ayarları (GNOME'daki GSettings şemasının karşılığı):
//! `%APPDATA%\kota\ui.json`. Anahtar adları ve varsayılanlar
//! `org.gnome.shell.extensions.kota` şemasıyla aynıdır; web arayüzü iki
//! platformda aynı anahtarları kullanır.

use std::path::PathBuf;

use kota_core::model::{ProviderReport, UsageWindow};
use serde_json::{Map, Value, json};

fn path() -> PathBuf {
    kota_core::paths::config_file().with_file_name("ui.json")
}

pub fn defaults() -> Map<String, Value> {
    let v = json!({
        "language": "auto",
        "panel-providers": [],
        "panel-windows": {},
        "panel-window-mode": "all",
        "panel-letters": false,
        "show-remaining": false,
        "tray-style": "ring",
        "popup-width": 460,
        "usage-columns": 2,
        "usage-style": "rings",
        "accent-color": "#f5a623",
        "show-tiles": true,
        "show-usage": true,
        "show-activity": true,
        "show-agents": true,
        "show-filter": true,
        "show-pace": true,
        "bar-colors": "usage",
        "popup-theme": "auto",
        "popup-background": "auto",
        "popup-opacity": 45,
        "activity-range": "24h",
        "notifications": true,
        "notify-reset": true,
        "notify-expired": true,
        "notify-forecast": true,
        "notify-forecast-minutes": 60,
        "notify-state": "{}"
    });
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

/// Varsayılanlarla birleştirilmiş ayarlar. `autostart` yalnızca dosyada varsa bulunur
/// (ilk açılışı ayırt etmek için).
pub fn load() -> Value {
    let mut m = defaults();
    if let Some(Value::Object(saved)) = std::fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()) {
        for (k, v) in saved {
            m.insert(k, v);
        }
    }
    Value::Object(m)
}

pub fn set(key: &str, value: Value) -> Result<(), String> {
    let mut saved: Map<String, Value> = std::fs::read_to_string(path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    saved.insert(key.to_string(), value);
    let p = path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, serde_json::to_string_pretty(&saved).unwrap_or_default()).map_err(|e| e.to_string())
}

pub fn flag(ui: &Value, key: &str) -> bool {
    ui.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub fn string(ui: &Value, key: &str, default: &str) -> String {
    ui.get(key).and_then(Value::as_str).unwrap_or(default).to_string()
}

pub fn strv(ui: &Value, key: &str) -> Vec<String> {
    ui.get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

pub fn is_turkish(ui: &Value) -> bool {
    match string(ui, "language", "auto").as_str() {
        "tr" => true,
        "en" => false,
        _ => sys_locale::get_locale().is_some_and(|l| l.to_ascii_lowercase().starts_with("tr")),
    }
}

/// Paneldeki (tepsideki) pencereler: eklentideki `panelWindows` ile aynı kural.
pub fn panel_windows(p: &ProviderReport, ui: &Value, tr: bool) -> Vec<UsageWindow> {
    let ws = &p.windows;
    if ws.is_empty() {
        return Vec::new();
    }
    let pick = ui
        .get("panel-windows")
        .and_then(|m| m.get(&p.id))
        .and_then(Value::as_str)
        .unwrap_or("auto")
        .to_string();
    let mode = if pick == "auto" { string(ui, "panel-window-mode", "all") } else { pick };
    let highest = || ws.iter().max_by(|a, b| a.used_percent.total_cmp(&b.used_percent)).cloned();
    if mode != "all"
        && mode != "auto"
        && let Some(w) = ws.iter().find(|w| w.id == mode)
    {
        return vec![w.clone()];
    }
    if mode != "all" {
        return highest().into_iter().collect();
    }
    let mut by: Vec<(&str, UsageWindow)> = Vec::new();
    for w in ws {
        let k = crate::tray::window_letter(w, tr);
        match by.iter_mut().find(|(l, _)| *l == k) {
            Some((_, cur)) if w.used_percent > cur.used_percent => *cur = w.clone(),
            Some(_) => {}
            None => by.push((k, w.clone())),
        }
    }
    let mut v: Vec<UsageWindow> = by.into_iter().map(|(_, w)| w).collect();
    v.sort_by_key(|w| w.window_seconds.unwrap_or(0));
    v
}

/// Görev çubuğu açık temada mı (simge rengini seçmek için).
#[cfg(windows)]
pub fn taskbar_is_light() -> bool {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|k| k.get_value::<u32, _>("SystemUsesLightTheme"))
        .is_ok_and(|v| v == 1)
}

#[cfg(not(windows))]
pub fn taskbar_is_light() -> bool {
    false
}
