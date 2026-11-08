// Kota — çizim yardımcıları ve ortak hesaplar (GNOME eklentisindeki
// widgets.js / panel.js / progress.js / summary.js'in tarayıcı karşılığı).

import {parseTime} from './i18n.js';
import {h} from './api.js';

const ICONS = new Set(['claude', 'codex', 'gemini', 'copilot', 'deepseek', 'openrouter', 'moonshot', 'zai',
    'siliconflow', 'openai', 'anthropic']);
const AGENT_ICON = {claude: 'claude', codex: 'codex', gemini: 'gemini', copilot: 'copilot', kimi: 'moonshot'};
export const BRAND = {
    claude: '#d97757', codex: '#5b9cff', gemini: '#7b8cff', copilot: '#b18cff', deepseek: '#5b7bff',
    openrouter: '#8f9bff', moonshot: '#e6e6e6', zai: '#4d8dff', siliconflow: '#8a63ff', openai: '#19c37d',
    anthropic: '#d4a27f',
};
const SEVERITY = {warning: '#f5a623', critical: '#ff5a4f', stale: '#7d8590'};
const USAGE_STOPS = [[0, '#3fb950'], [50, '#3fb950'], [70, '#e3b341'], [85, '#f0883e'], [95, '#ff5a4f']];
const SVGNS = 'http://www.w3.org/2000/svg';

let barMode = 'usage';
export function setBarMode(m) { barMode = m === 'brand' ? 'brand' : 'usage'; }

export const agentIconId = kind => AGENT_ICON[kind] ?? null;

function hexToRgb(hex) {
    const m = String(hex).replace('#', '').match(/^([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i);
    return m ? [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16)] : [255, 255, 255];
}

export function lighten(hex, amount) {
    const [r, g, b] = hexToRgb(hex);
    const mix = c => Math.round(c + (255 - c) * amount).toString(16).padStart(2, '0');
    return `#${mix(r)}${mix(g)}${mix(b)}`;
}

export function usageColor(percent) {
    const v = Math.max(0, Math.min(100, percent ?? 0));
    for (let i = 1; i < USAGE_STOPS.length; i++) {
        const [p1, c1] = USAGE_STOPS[i];
        if (v <= p1) {
            const [p0, c0] = USAGE_STOPS[i - 1];
            const t = p1 === p0 ? 1 : (v - p0) / (p1 - p0);
            const a = hexToRgb(c0), b = hexToRgb(c1);
            return `#${[0, 1, 2].map(k => Math.round(a[k] + (b[k] - a[k]) * t).toString(16).padStart(2, '0')).join('')}`;
        }
    }
    return USAGE_STOPS.at(-1)[1];
}

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

/** Logo ya da (yoksa) baş harfli rozet. */
export function providerIcon(id, {size = 16, name = id, cls = ''} = {}) {
    if (id && ICONS.has(id))
        return h(`img.kota-icon${cls ? `.${cls}` : ''}`, {src: `icons/providers/${id}.svg`, width: size, height: size, alt: name});
    return h(`span.kota-letter${cls ? `.${cls}` : ''}`, {style: {width: `${size}px`, height: `${size}px`, fontSize: `${Math.round(size * 0.62)}px`}},
        (name || '?').slice(0, 1).toUpperCase());
}

/** En çok dolmuş pencere. */
export function headline(p) {
    let best = null;
    for (const w of p.windows ?? []) {
        if (!best || w.used_percent > best.used_percent)
            best = w;
    }
    return best;
}

/** Panelde (tepside) gösterilecek pencereler: eklentideki panelWindows ile aynı. */
export function panelWindows(p, settings, strings) {
    const ws = p.windows ?? [];
    if (!ws.length)
        return [];
    const pick = settings.get_value('panel-windows').deepUnpack()[p.id] ?? 'auto';
    const mode = pick === 'auto' ? settings.get_string('panel-window-mode') : pick;
    if (mode !== 'all' && mode !== 'auto') {
        const w = ws.find(x => x.id === mode);
        if (w)
            return [w];
    }
    if (mode !== 'all' || !strings)
        return [headline(p)];
    const by = new Map();
    for (const w of ws) {
        const k = strings.windowLetter(w);
        const cur = by.get(k);
        if (!cur || w.used_percent > cur.used_percent)
            by.set(k, w);
    }
    return [...by.values()].sort((a, b) => (a.window_seconds ?? 0) - (b.window_seconds ?? 0));
}

/** Pencerenin geçen süre oranı (0–1). */
export function paceOf(w, nowMs = Date.now()) {
    const reset = parseTime(w?.resets_at);
    const secs = w?.window_seconds;
    if (Number.isNaN(reset) || !secs)
        return null;
    const start = reset - secs * 1000;
    return Math.max(0, Math.min(1, (nowMs - start) / (secs * 1000)));
}

export function paceText(strings, w) {
    if (w.used_percent >= 100)
        return strings.t('limit_reached');
    const p = paceOf(w);
    if (p === null || w.estimated)
        return '';
    const d = w.used_percent - p * 100;
    if (Math.abs(d) < 10)
        return '';
    return strings.t(d > 0 ? 'pace_fast' : 'pace_slow');
}

/** Yüzde halkası (SVG); ortasına logo. */
export function ring({size = 52, thickness = 4, fraction = null, color = '#5b9cff', child = null, dashed = false}) {
    const r = size / 2 - thickness / 2 - 0.5;
    const c = 2 * Math.PI * r;
    const svg = document.createElementNS(SVGNS, 'svg');
    svg.setAttribute('width', size);
    svg.setAttribute('height', size);
    svg.setAttribute('viewBox', `0 0 ${size} ${size}`);
    const track = document.createElementNS(SVGNS, 'circle');
    Object.entries({cx: size / 2, cy: size / 2, r, fill: 'none', 'stroke-width': thickness, class: 'kota-ring-track'})
        .forEach(([k, v]) => track.setAttribute(k, v));
    if (dashed)
        track.setAttribute('stroke-dasharray', '3 3');
    svg.append(track);
    if (fraction !== null && fraction > 0) {
        const f = Math.min(1, Math.max(0.012, fraction));
        const arc = document.createElementNS(SVGNS, 'circle');
        Object.entries({
            cx: size / 2, cy: size / 2, r, fill: 'none', stroke: color, 'stroke-width': thickness,
            'stroke-linecap': 'round', 'stroke-dasharray': `${f * c} ${c}`,
            transform: `rotate(-90 ${size / 2} ${size / 2})`,
        }).forEach(([k, v]) => arc.setAttribute(k, v));
        svg.append(arc);
    }
    return h('div.kota-ring', {style: {width: `${size}px`, height: `${size}px`}}, svg, child ? h('div.kota-ring-child', {}, child) : null);
}

/** Yuvarlak uçlu, degrade dolgulu ilerleme çizgisi ve tempo işareti. */
export function progressLine({fraction, color, pace = null, height = 6}) {
    const f = Math.max(0, Math.min(1, fraction ?? 0));
    const fill = h('div.kota-progress-fill', {style: {
        width: f > 0 ? `max(${height}px, ${f * 100}%)` : '0',
        background: `linear-gradient(90deg, ${lighten(color, 0.38)}, ${color})`,
        boxShadow: `0 0 0 0.5px ${color}2e`,
    }});
    const track = h('div.kota-progress', {style: {height: `${height}px`, borderRadius: `${height / 2}px`}}, fill);
    if (pace !== null)
        track.append(h('div.kota-progress-pace', {style: {left: `calc(${pace * 100}% - 1.5px)`, height: `${height + 6}px`}}));
    return track;
}

/** Kutucuktaki küçük çubuklar. */
export function miniBars(values, {height = 22, accent = '#f5a623'} = {}) {
    const max = Math.max(1, ...values);
    return h('div.kota-minibars', {style: {height: `${height}px`}}, values.map((v, i) => h('div.kota-minibar', {
        class: i === values.length - 1 && v > 0 ? 'accent' : '',
        style: {height: `${v > 0 ? Math.max(3, Math.round((v / max) * height)) : 2}px`,
            background: i === values.length - 1 && v > 0 ? accent : ''},
    })));
}

/** Çubuk grafik; üzerine gelinen çubuk `onHover(i|null)`. */
export function barChart(values, {height = 92, accent = '#f5a623', highlight = 3, onHover = null} = {}) {
    const max = Math.max(1, ...values);
    const box = h('div.kota-chart', {style: {height: `${height}px`}});
    values.forEach((v, i) => {
        const recent = i >= values.length - highlight;
        const bar = h('div.kota-chart-bar', {class: v > 0 ? (recent ? 'recent' : 'some') : 'none', style: {
            height: `${v > 0 ? Math.max(4, Math.round((v / max) * (height - 4))) : 3}px`,
            background: recent && v > 0 ? accent : '',
        }});
        const col = h('div.kota-chart-col', {}, bar);
        if (onHover) {
            col.addEventListener('mouseenter', () => onHover(i));
            col.addEventListener('mouseleave', () => onHover(null));
        }
        box.append(col);
    });
    return box;
}

/** Simgeli yuvarlak düğme (SVG simgeleri icons/ui altında). */
export function iconButton(icon, title, onClick, cls = '') {
    return h(`button.kota-icon-btn${cls ? `.${cls.split(' ').join('.')}` : ''}`, {title, 'aria-label': title, onclick: e => {
        e.stopPropagation();
        onClick();
    }}, h('span.kota-glyph', {'data-icon': icon}));
}
