//! Görev çubuğu (sistem tepsisi) simgesi: GNOME panelindeki "halka + logo"
//! göstergesinin karşılığı. Simge her anlık görüntüde yeniden çizilir:
//! en dolu pencerenin halkası (limite duyarlı renk) ve ortasında logo.

use kota_core::model::{Snapshot, UsageWindow};
use resvg::tiny_skia::{self, Color, LineCap, Paint, PathBuilder, Pixmap, Stroke, Transform};
use resvg::usvg;

pub const SIZE: u32 = 32;

const KOTA_SVG: &str = include_str!("../../../gnome-extension/kota@ensignalp/icons/kota.svg");
const KOTA_SYMBOLIC: &str = include_str!("../../../gnome-extension/kota@ensignalp/icons/kota-symbolic.svg");

macro_rules! provider_icons {
    ($($id:literal),*) => {
        fn provider_svg(id: &str) -> Option<&'static str> {
            match id {
                $($id => Some(include_str!(concat!("../../../gnome-extension/kota@ensignalp/icons/providers/", $id, ".svg"))),)*
                _ => None,
            }
        }
    };
}
provider_icons!(
    "claude", "codex", "gemini", "copilot", "deepseek", "openrouter", "moonshot", "zai", "siliconflow", "openai",
    "anthropic"
);

/// Doluluğa göre yeşil → sarı → turuncu → kırmızı (eklentideki `usageColor`).
pub fn usage_color(percent: f64) -> [u8; 3] {
    const STOPS: [(f64, [u8; 3]); 5] = [
        (0.0, [0x3f, 0xb9, 0x50]),
        (50.0, [0x3f, 0xb9, 0x50]),
        (70.0, [0xe3, 0xb3, 0x41]),
        (85.0, [0xf0, 0x88, 0x3e]),
        (95.0, [0xff, 0x5a, 0x4f]),
    ];
    let v = percent.clamp(0.0, 100.0);
    for w in STOPS.windows(2) {
        let ((p0, c0), (p1, c1)) = (w[0], w[1]);
        if v <= p1 {
            let t = if p1 == p0 { 1.0 } else { (v - p0) / (p1 - p0) };
            let mix = |i: usize| (c0[i] as f64 + (c1[i] as f64 - c0[i] as f64) * t).round() as u8;
            return [mix(0), mix(1), mix(2)];
        }
    }
    STOPS[4].1
}

/// Pencerenin dile göre baş harfi: G/H/A (TR) · D/W/M (EN).
pub fn window_letter(w: &UsageWindow, tr: bool) -> &'static str {
    let (d, wk, m) = if tr { ("G", "H", "A") } else { ("D", "W", "M") };
    match w.window_seconds {
        Some(s) if s as f64 <= 86400.0 * 1.5 => d,
        Some(s) if s as f64 <= 86400.0 * 10.0 => wk,
        Some(_) => m,
        None if w.id == "weekly" => wk,
        None if w.id == "monthly" || w.id.starts_with("days_") => m,
        None => d,
    }
}

fn render_svg(svg: &str, pixmap: &mut Pixmap, x: f32, y: f32, size: f32) {
    let Ok(tree) = usvg::Tree::from_str(svg, &usvg::Options::default()) else { return };
    let s = tree.size();
    let k = size / s.width().max(s.height());
    resvg::render(&tree, Transform::from_row(k, 0.0, 0.0, k, x, y), &mut pixmap.as_mut());
}

fn arc(cx: f32, cy: f32, r: f32, start: f32, sweep: f32) -> Option<tiny_skia::Path> {
    let steps = ((sweep.abs() / std::f32::consts::TAU) * 64.0).ceil().max(2.0) as usize;
    let mut pb = PathBuilder::new();
    for i in 0..=steps {
        let a = start + sweep * i as f32 / steps as f32;
        let (x, y) = (cx + r * a.cos(), cy + r * a.sin());
        if i == 0 { pb.move_to(x, y) } else { pb.line_to(x, y) }
    }
    pb.finish()
}

/// Tepsi simgesi (RGBA, SIZE×SIZE).
/// `style`: `ring` (halka + AI logosu), `kota` (halka + Kota logosu), `logo` (yalnız Kota logosu).
pub fn render(top: Option<(&str, &UsageWindow, bool)>, style: &str, light_taskbar: bool) -> Vec<u8> {
    let mut pm = Pixmap::new(SIZE, SIZE).expect("pixmap");
    let sz = SIZE as f32;
    let logo = |pm: &mut Pixmap, id: Option<&str>, inner: f32| {
        let off = (sz - inner) / 2.0;
        match id.and_then(provider_svg) {
            Some(svg) => render_svg(svg, pm, off, off, inner),
            None => render_svg(KOTA_SVG, pm, off, off, inner),
        }
    };
    if style == "logo" || top.is_none() {
        if light_taskbar {
            render_svg(KOTA_SVG, &mut pm, 0.0, 0.0, sz);
        } else {
            // Koyu görev çubuğunda tek renkli beyaz logo daha okunur.
            let white = KOTA_SYMBOLIC.replace("#2e3436", "#ffffff");
            render_svg(&white, &mut pm, 0.0, 0.0, sz);
        }
        return pm.take();
    }
    let (provider, w, stale) = top.expect("checked");
    let thick = 3.6;
    let r = sz / 2.0 - thick / 2.0 - 0.5;
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    let stroke = Stroke {
        width: thick,
        line_cap: LineCap::Round,
        ..Default::default()
    };
    // İz
    let track = if light_taskbar { Color::from_rgba8(0, 0, 0, 50) } else { Color::from_rgba8(255, 255, 255, 60) };
    paint.set_color(track);
    if let Some(p) = arc(sz / 2.0, sz / 2.0, r, 0.0, std::f32::consts::TAU) {
        pm.stroke_path(&p, &paint, &stroke, Transform::identity(), None);
    }
    // Dolgu
    let frac = (w.used_percent / 100.0).clamp(0.0, 1.0) as f32;
    if frac > 0.0 {
        let [cr, cg, cb] = if stale { [0x7d, 0x85, 0x90] } else { usage_color(w.used_percent) };
        paint.set_color(Color::from_rgba8(cr, cg, cb, 255));
        let start = -std::f32::consts::FRAC_PI_2;
        if let Some(p) = arc(sz / 2.0, sz / 2.0, r, start, std::f32::consts::TAU * frac.max(0.03)) {
            pm.stroke_path(&p, &paint, &stroke, Transform::identity(), None);
        }
    }
    let inner = sz - 2.0 * thick - 6.0;
    logo(&mut pm, if style == "kota" { None } else { Some(provider) }, inner);
    pm.take()
}

/// Simgenin ipucu metni: "Claude  G %25 · H %24" satırları (Windows sınırı 127 karakter).
pub fn tooltip(snap: Option<&Snapshot>, lines: &[(String, Vec<String>)], paused: bool, tr: bool) -> String {
    let mut out = String::from("Kota");
    if snap.is_none() {
        out.push_str(if tr { " — yükleniyor…" } else { " — loading…" });
    }
    if paused {
        out.push_str(if tr { " (duraklatıldı)" } else { " (paused)" });
    }
    for (name, vals) in lines {
        let line = format!("\n{name}  {}", vals.join(" · "));
        if out.chars().count() + line.chars().count() > 127 {
            break;
        }
        out.push_str(&line);
    }
    out
}
