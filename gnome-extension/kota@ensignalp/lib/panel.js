// Kota — üst paneldeki göstergeler.

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import St from 'gi://St';

import {label, providerIcon, ring, ringColor} from './widgets.js';

/** En çok dolmuş pencere. */
export function headline(p) {
    let best = null;
    for (const w of p.windows ?? []) {
        if (!best || w.used_percent > best.used_percent)
            best = w;
    }
    return best;
}

/**
 * Sağlayıcının panelde gösterilecek pencereleri. Sağlayıcıya özel seçim
 * ('auto' = genel ayar, 'all', ya da pencere kimliği) genel ayarı geçersiz kılar.
 * 'all' kipinde aynı harfe düşen pencerelerden (ör. Gemini model kotaları)
 * yalnızca en dolu olanı kalır.
 */
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
    const byLetter = new Map();
    for (const w of ws) {
        const k = strings.windowLetter(w);
        const cur = byLetter.get(k);
        if (!cur || w.used_percent > cur.used_percent)
            byLetter.set(k, w);
    }
    // Kısa pencere önce: G, H, A.
    return [...byLetter.values()].sort((a, b) => (a.window_seconds ?? 0) - (b.window_seconds ?? 0));
}

/** Ayarlara göre panelde gösterilecek öğeler. */
export function panelItems(snapshot, settings, strings = null) {
    const selected = settings.get_strv('panel-providers');
    const items = [];
    for (const p of snapshot?.providers ?? []) {
        if (selected.length && !selected.includes(p.id))
            continue;
        const ws = panelWindows(p, settings, strings);
        if (ws.length)
            items.push({p, w: headlineOf(ws), ws});
        else if (p.balance)
            items.push({p, balance: p.balance});
        else if (p.status === 'expired' || p.status === 'error')
            items.push({p, problem: true});
    }
    // Seçim sırası = panel sırası.
    if (selected.length)
        items.sort((a, b) => selected.indexOf(a.p.id) - selected.indexOf(b.p.id));
    return items;
}

function headlineOf(ws) {
    return ws.reduce((best, w) => (!best || w.used_percent > best.used_percent ? w : best), null);
}

function sevClass(w, stale) {
    return `kota-sev-${stale || w.estimated ? 'stale' : w.severity}`;
}

function compactMoney(strings, b) {
    const s = strings.money(b.amount, b.currency);
    return b.amount >= 10 ? s.replace(/[.,]\d\d$/, '') : s;
}

function kotaIcon(extPath, colored) {
    return new St.Icon({
        gicon: Gio.icon_new_for_string(`${extPath}/icons/${colored ? 'kota.svg' : 'kota-symbolic.svg'}`),
        icon_size: 16,
        style_class: 'kota-panel-icon kota-panel-logo',
        y_align: Clutter.ActorAlign.CENTER,
    });
}

export function renderPanel(box, snapshot, error, settings, strings, extPath) {
    box.destroy_all_children();
    if (!snapshot) {
        box.add_child(providerIcon(extPath, 'kota', {name: 'K', size: 16}));
        box.add_child(label(error ? '–' : '…', 'kota-panel-pct kota-sev-stale'));
        return;
    }
    let items = panelItems(snapshot, settings, strings);
    if (!items.length) {
        box.add_child(label('Kota', 'kota-panel-pct kota-sev-stale'));
        return;
    }
    if (settings.get_string('panel-mode') === 'max') {
        const withPct = items.filter(i => i.w).sort((a, b) => b.w.used_percent - a.w.used_percent);
        items = withPct.length ? [{...withPct[0], ws: [withPct[0].w]}] : [items[0]];
    }
    const letters = settings.get_boolean('panel-letters');
    const style = settings.get_string('panel-style');
    const colored = settings.get_boolean('panel-colored-icons');
    const remaining = settings.get_boolean('show-remaining');
    const iconMode = settings.get_string('panel-icon');
    // Kota logosu: tek başına (AI logoları yerine) ya da AI logolarının önünde.
    if (iconMode !== 'provider' && style !== 'text')
        box.add_child(kotaIcon(extPath, colored));
    items.forEach((item, idx) => {
        const {p, w} = item;
        const group = new St.BoxLayout({style_class: 'kota-panel-item', y_align: Clutter.ActorAlign.CENTER});
        if (idx > 0 || iconMode !== 'provider')
            group.add_style_class_name('kota-panel-item-gap');
        if (iconMode === 'kota' && idx === 0 && style !== 'text')
            group.add_style_class_name('kota-panel-item-near');
        const showProvider = iconMode !== 'kota';
        const icon = providerIcon(extPath, p.id, {colored, size: 15, name: p.name, styleClass: 'kota-panel-icon'});
        if (!showProvider) {
            // Kota logosu kipinde AI logosu gösterilmez; halka biçiminde halka yine çizilir.
            if (style === 'ring' && w)
                group.add_child(ring({size: 16, thickness: 2.4, fraction: Math.min(1, w.used_percent / 100),
                    color: ringColor(p.id, w, p.stale)}));
        } else if (style === 'ring') {
            const frac = w ? Math.min(1, w.used_percent / 100) : null;
            group.add_child(ring({size: 18, thickness: 2.4, fraction: frac, color: ringColor(p.id, w, p.stale),
                child: providerIcon(extPath, p.id, {colored, size: 9, name: p.name})}));
        } else if (style === 'icon') {
            group.add_child(icon);
        }
        if (w) {
            const ws = item.ws ?? [w];
            ws.forEach((x, i) => {
                const v = remaining ? Math.max(0, 100 - x.used_percent) : x.used_percent;
                if (ws.length > 1 || letters) {
                    // Harf rozeti logo ile aynı boyda (15 px kare).
                    const l = new St.Label({
                        text: strings.windowLetter(x),
                        style_class: 'kota-panel-letter',
                        y_align: Clutter.ActorAlign.CENTER,
                        x_align: Clutter.ActorAlign.CENTER,
                    });
                    l.clutter_text.x_align = Clutter.ActorAlign.CENTER;
                    if (i > 0)
                        l.add_style_class_name('kota-panel-letter-gap');
                    group.add_child(l);
                }
                group.add_child(label(strings.percent(v, x.estimated), `kota-panel-pct ${sevClass(x, p.stale)}`));
            });
        } else if (item.balance) {
            group.add_child(label(compactMoney(strings, item.balance), 'kota-panel-pct kota-sev-normal'));
        } else {
            group.add_child(label('!', 'kota-panel-pct kota-sev-critical'));
        }
        box.add_child(group);
    });
    if (snapshot.paused)
        box.add_child(label('⏸', 'kota-panel-paused'));
}
