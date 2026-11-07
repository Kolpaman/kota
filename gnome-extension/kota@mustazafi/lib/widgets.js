// Kota — çizim yardımcıları: logolar, halka göstergeler, çubuk grafikler.

import Cairo from 'cairo';
import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';

/** Logosu olan sağlayıcılar (icons/providers/<id>.svg ve <id>-symbolic.svg). */
const ICONS = new Set(['claude', 'codex', 'gemini', 'copilot', 'deepseek', 'openrouter', 'moonshot', 'zai',
    'siliconflow', 'openai', 'anthropic']);

/** Ajan türü → logo. */
const AGENT_ICON = {claude: 'claude', codex: 'codex', gemini: 'gemini', copilot: 'copilot', kimi: 'moonshot'};

/** Halkalar için marka renkleri (koyu zeminde okunur tonlar). */
export const BRAND = {
    claude: '#d97757', codex: '#5b9cff', gemini: '#7b8cff', copilot: '#b18cff', deepseek: '#5b7bff',
    openrouter: '#8f9bff', moonshot: '#e6e6e6', zai: '#4d8dff', siliconflow: '#8a63ff', openai: '#19c37d',
    anthropic: '#d4a27f',
};

const SEVERITY = {warning: '#f5a623', critical: '#ff5a4f', stale: '#7d8590'};

export const VERTICAL = Clutter.Orientation.VERTICAL;

export function hasIcon(id) {
    return ICONS.has(id);
}

export function agentIconId(kind) {
    return AGENT_ICON[kind] ?? null;
}

const gicons = new Map();

/** Sağlayıcı logosu; `colored` false ise tema rengine boyanan sembolik sürüm. */
export function providerGicon(extPath, id, colored) {
    const key = `${id}:${colored}`;
    if (!gicons.has(key)) {
        const file = colored ? `${id}.svg` : `${id}-symbolic.svg`;
        gicons.set(key, Gio.icon_new_for_string(GLib.build_filenamev([extPath, 'icons', 'providers', file])));
    }
    return gicons.get(key);
}

/** Logo ya da (yoksa) baş harfli rozet. */
export function providerIcon(extPath, id, {colored = true, size = 16, name = id, styleClass = ''} = {}) {
    if (id && hasIcon(id))
        return new St.Icon({gicon: providerGicon(extPath, id, colored), icon_size: size, style_class: styleClass});
    return new St.Label({
        text: (name || '?').slice(0, 1).toUpperCase(),
        style_class: `kota-letter ${styleClass}`,
        style: `font-size: ${Math.round(size * 0.7)}px; min-width: ${size}px; min-height: ${size}px;`,
        x_align: Clutter.ActorAlign.CENTER,
        y_align: Clutter.ActorAlign.CENTER,
    });
}

export function hexToRgba(hex, alpha = 1) {
    const m = String(hex).replace('#', '').match(/^([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i);
    if (!m)
        return [1, 1, 1, alpha];
    return [parseInt(m[1], 16) / 255, parseInt(m[2], 16) / 255, parseInt(m[3], 16) / 255, alpha];
}

/** Limite duyarlı renk durakları: (doluluk %, renk). */
const USAGE_STOPS = [[0, '#3fb950'], [50, '#3fb950'], [70, '#e3b341'], [85, '#f0883e'], [95, '#ff5a4f']];

let barMode = 'usage';
let light = false;

/** Açık tema: Cairo çizimleri ve satır içi renkler koyu mürekkep kullanır. */
export function setLight(v) {
    light = Boolean(v);
}

export function isLight() {
    return light;
}

/** Temaya göre yarı saydam mürekkep [r, g, b, a] (koyu temada beyaz, açıkta siyah). */
export function ink(alpha) {
    return light ? [0, 0, 0, alpha * 0.9] : [1, 1, 1, alpha];
}

export function inkCss(alpha) {
    const [r, g, b, a] = ink(alpha);
    return `rgba(${r * 255}, ${g * 255}, ${b * 255}, ${a.toFixed(3)})`;
}

/** 'usage' (yeşil → kırmızı) ya da 'brand' (marka rengi, eşikte turuncu/kırmızı). */
export function setBarMode(mode) {
    barMode = mode === 'brand' ? 'brand' : 'usage';
}

/** Doluluğa göre yeşil → sarı → turuncu → kırmızı arası renk. */
export function usageColor(percent) {
    const v = Math.max(0, Math.min(100, percent ?? 0));
    for (let i = 1; i < USAGE_STOPS.length; i++) {
        const [p1, c1] = USAGE_STOPS[i];
        if (v <= p1) {
            const [p0, c0] = USAGE_STOPS[i - 1];
            const t = p1 === p0 ? 1 : (v - p0) / (p1 - p0);
            const a = hexToRgba(c0), b = hexToRgba(c1);
            const hex = k => Math.round((a[k] + (b[k] - a[k]) * t) * 255).toString(16).padStart(2, '0');
            return `#${hex(0)}${hex(1)}${hex(2)}`;
        }
    }
    return USAGE_STOPS[USAGE_STOPS.length - 1][1];
}

/** Pencere için halka/çubuk rengi. */
export function ringColor(providerId, w, stale) {
    if (stale || w?.estimated)
        return SEVERITY.stale;
    if (barMode === 'usage' && w)
        return usageColor(w.used_percent);
    if (w?.severity === 'critical')
        return SEVERITY.critical;
    if (w?.severity === 'warning')
        return SEVERITY.warning;
    return BRAND[providerId] ?? '#5b9cff';
}

/**
 * Yüzde halkası. `fraction` 0–1; `null` ise yalnız iz (veri yok).
 * Ortasına `child` (logo) konur.
 */
export function ring({size = 56, thickness = 4, fraction = null, color = '#5b9cff', child = null, dashed = false}) {
    const bin = new St.Widget({
        layout_manager: new Clutter.BinLayout(),
        width: size,
        height: size,
        y_align: Clutter.ActorAlign.CENTER,
    });
    const area = new St.DrawingArea({width: size, height: size});
    area.connect('repaint', a => {
        const cr = a.get_context();
        const [w, h] = a.get_surface_size();
        const scale = w / size;
        const lw = thickness * scale;
        const r = Math.min(w, h) / 2 - lw / 2 - 0.5;
        cr.setLineCap(Cairo.LineCap.ROUND);
        cr.setLineWidth(lw);
        cr.setSourceRGBA(...ink(0.1));
        if (dashed)
            cr.setDash([3 * scale, 3 * scale], 0);
        cr.arc(w / 2, h / 2, r, 0, 2 * Math.PI);
        cr.stroke();
        if (fraction !== null && fraction > 0) {
            cr.setDash([], 0);
            const [cR, cG, cB, cA] = hexToRgba(color);
            cr.setSourceRGBA(cR, cG, cB, cA);
            const f = Math.min(1, Math.max(0.012, fraction));
            cr.arc(w / 2, h / 2, r, -Math.PI / 2, -Math.PI / 2 + f * 2 * Math.PI);
            cr.stroke();
        }
        cr.$dispose();
    });
    bin.add_child(area);
    if (child) {
        child.x_align = Clutter.ActorAlign.CENTER;
        child.y_align = Clutter.ActorAlign.CENTER;
        bin.add_child(child);
    }
    return bin;
}

/** Kutucuktaki küçük çubuklar (son N saat). Son çubuk vurgu renginde. */
export function miniBars(values, {height = 22, barWidth = 3, gap = 2, accent = '#f5a623'} = {}) {
    const box = new St.BoxLayout({style: `spacing: ${gap}px;`, y_align: Clutter.ActorAlign.END});
    const max = Math.max(1, ...values);
    values.forEach((v, i) => {
        const hpx = v > 0 ? Math.max(3, Math.round((v / max) * height)) : 2;
        const last = i === values.length - 1;
        box.add_child(new St.Widget({
            width: barWidth,
            height: hpx,
            y_align: Clutter.ActorAlign.END,
            style: `background-color: ${last && v > 0 ? accent : inkCss(0.45)}; border-radius: 1px;`,
        }));
    });
    return box;
}

/**
 * 24 saatlik çubuk grafik. Üzerine gelinen çubuğun değeri `onHover(i|null)` ile bildirilir.
 * Son `highlight` saat vurgu renginde.
 */
export function barChart(values, {height = 96, accent = '#f5a623', highlight = 3, onHover = null} = {}) {
    const box = new St.BoxLayout({style_class: 'kota-chart', x_expand: true, height});
    const max = Math.max(1, ...values);
    values.forEach((v, i) => {
        const recent = i >= values.length - highlight;
        const hpx = v > 0 ? Math.max(4, Math.round((v / max) * (height - 4))) : 3;
        // Dikey kutu: üstte esnek boşluk, altta çubuk → çubuklar tabana oturur.
        const col = new St.BoxLayout({
            orientation: VERTICAL,
            x_expand: true,
            y_expand: true,
            reactive: true,
            track_hover: true,
            style_class: 'kota-chart-col',
        });
        col.add_child(new St.Widget({y_expand: true}));
        const bar = new St.Widget({
            height: hpx,
            x_expand: true,
            style_class: 'kota-chart-bar',
            style: `background-color: ${recent && v > 0 ? accent : v > 0 ? inkCss(0.32) : inkCss(0.08)};`,
        });
        col.add_child(bar);
        if (onHover)
            col.connect('notify::hover', () => onHover(col.hover ? i : null));
        box.add_child(col);
    });
    return box;
}

export function label(text, styleClass = '', props = {}) {
    return new St.Label({text: String(text ?? ''), style_class: styleClass, y_align: Clutter.ActorAlign.CENTER, ...props});
}

/** Simgeli yuvarlak düğme. */
export function iconButton(iconName, accessibleName, onClick, styleClass = 'kota-icon-btn') {
    const b = new St.Button({
        style_class: styleClass,
        can_focus: true,
        accessible_name: accessibleName,
        child: new St.Icon({icon_name: iconName, icon_size: 14}),
        y_align: Clutter.ActorAlign.CENTER,
    });
    b.connect('clicked', onClick);
    return b;
}

/** Başlıklı kart. Dönüş: {card, body, right} */
export function card(title, rightText = '') {
    const c = new St.BoxLayout({orientation: VERTICAL, style_class: 'kota-card', x_expand: true});
    const head = new St.BoxLayout({style_class: 'kota-card-head', x_expand: true});
    const titleLabel = label(title, 'kota-card-title', {x_expand: true});
    head.add_child(titleLabel);
    const right = label(rightText, 'kota-card-right');
    head.add_child(right);
    c.add_child(head);
    const body = new St.BoxLayout({orientation: VERTICAL, x_expand: true, style_class: 'kota-card-body'});
    c.add_child(body);
    return {card: c, body, right, head, title: titleLabel};
}
