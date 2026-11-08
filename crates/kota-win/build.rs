// Tauri derleme adımı + GNOME eklentisinin TR/EN metinlerini web arayüzüne kopyalar
// (tek kaynak: gnome-extension/kota@ensignalp/lib/i18n.js).
fn main() {
    let src = std::path::Path::new("../../gnome-extension/kota@ensignalp/lib/i18n.js");
    let dst = std::path::Path::new("web/js/i18n.js");
    println!("cargo:rerun-if-changed={}", src.display());
    if let Ok(text) = std::fs::read_to_string(src) {
        let _ = std::fs::create_dir_all("web/js");
        // Tarayıcıda `gi://GLib` yok: küçük karşılığına yönlendir.
        let text = text.replace("import GLib from 'gi://GLib';", "import GLib from './glib-shim.js';");
        let _ = std::fs::write(dst, text);
    }
    // Sağlayıcı logoları ve Kota logosu da eklentiyle aynı dosyalardır.
    let icons = std::path::Path::new("../../gnome-extension/kota@ensignalp/icons");
    println!("cargo:rerun-if-changed={}", icons.display());
    copy_dir(icons, std::path::Path::new("web/icons"));
    tauri_build::build()
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
    let _ = std::fs::create_dir_all(dst);
    let Ok(entries) = std::fs::read_dir(src) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let to = dst.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &to);
        } else {
            let _ = std::fs::copy(&p, &to);
        }
    }
}
