// Kota — Hızlı Ayarlar menüsü ve tarih menüsü için sade kullanım listesi:
// sağlayıcı başına logo, ad, değer ve ilerleme çizgisi.

import Clutter from 'gi://Clutter';
import St from 'gi://St';

import {panelWindows} from './panel.js';
import {paceDelta, paceOf, progressLine} from './progress.js';
import {VERTICAL, label, providerIcon, ringColor} from './widgets.js';

function small(text, extra = '') {
    const l = label(text, `kota-sum-small ${extra}`.trim());
    l.clutter_text.line_wrap = true;
    return l;
}

/** Sağlayıcının gösterilecek pencereleri (kısa görünümde paneldekiyle aynı). */
function windowsFor(p, settings, full, strings = null) {
    if (full)
        return p.windows ?? [];
    return panelWindows(p, settings, strings).filter(Boolean);
}

function highest(ws) {
    return ws.reduce((best, w) => (!best || w.used_percent > best.used_percent ? w : best), null);
}

export function paceText(strings, w) {
    if (w.used_percent >= 100)
        return strings.t('limit_reached');
    const d = paceDelta(w);
    if (d === null || Math.abs(d) < 10)
        return '';
    return strings.t(d > 0 ? 'pace_fast' : 'pace_slow');
}

/**
 * Kullanım listesini `box` içine çizer.
 * @param {St.BoxLayout} box
 * @param {object|null} snapshot
 * @param {object} o
 * @param {boolean} [o.full] tüm pencereler (yoksa yalnız paneldeki değer)
 * @param {boolean} [o.onlySelected] yalnız panelde seçili sağlayıcılar
 */
export function renderSummary(box, snapshot, {extPath, settings, strings, full = false, onlySelected = false, onConnect = null, hideHead = false}) {
    box.destroy_all_children();
    const s = strings;
    if (!snapshot) {
        box.add_child(small(s.t('loading')));
        return;
    }
    const selected = settings.get_strv('panel-providers');
    const remaining = settings.get_boolean('show-remaining');
    const showPace = settings.get_boolean('show-pace');
    let providers = snapshot.providers ?? [];
    if (onlySelected && selected.length)
        providers = providers.filter(p => selected.includes(p.id));
    if (!providers.length) {
        box.add_child(small(s.t('no_providers')));
        return;
    }
    for (const p of providers) {
        const row = new St.BoxLayout({orientation: VERTICAL, style_class: 'kota-sum-row', x_expand: true});
        const head = new St.BoxLayout({x_expand: true, style_class: 'kota-sum-head'});
        if (!hideHead) {
            head.add_child(providerIcon(extPath, p.id, {size: 16, name: p.name}));
            head.add_child(label(p.name, 'kota-sum-name', {x_expand: true}));
        }
        const ws = windowsFor(p, settings, full, strings);
        if (!ws.length) {
            let value;
            if (p.balance)
                value = s.money(p.balance.amount, p.balance.currency);
            else
                value = s.status(p);
            if (hideHead)
                head.add_child(new St.Widget({x_expand: true}));
            head.add_child(label(value, p.status === 'ok' ? 'kota-sum-value' : 'kota-sum-small kota-sum-muted'));
            row.add_child(head);
            if (onConnect && (p.status === 'not_connected' || p.status === 'expired')) {
                const b = new St.Button({style_class: 'kota-sum-link', label: `${s.connectLabel(p)}…`, x_align: Clutter.ActorAlign.START});
                b.connect('clicked', () => onConnect(p.id));
                row.add_child(b);
            }
            box.add_child(row);
            continue;
        }
        if (ws.length === 1) {
            const w = ws[0];
            if (hideHead)
                head.add_child(label(s.windowLabel(w), 'kota-sum-small', {x_expand: true}));
            const v = remaining ? Math.max(0, 100 - w.used_percent) : w.used_percent;
            head.add_child(label(s.percent(v, w.estimated), `kota-sum-value kota-sev-${p.stale || w.estimated ? 'stale' : w.severity}`));
        }
        if (head.get_n_children())
            row.add_child(head);
        for (const w of ws) {
            if (ws.length > 1) {
                const wl = new St.BoxLayout({x_expand: true});
                wl.add_child(label(s.windowLabel(w), 'kota-sum-small', {x_expand: true}));
                const v = remaining ? Math.max(0, 100 - w.used_percent) : w.used_percent;
                wl.add_child(label(s.percent(v, w.estimated), `kota-sum-pct kota-sev-${p.stale || w.estimated ? 'stale' : w.severity}`));
                row.add_child(wl);
            }
            row.add_child(progressLine({
                fraction: w.used_percent / 100,
                color: ringColor(p.id, w, p.stale),
                pace: showPace && !p.stale ? paceOf(w) : null,
                height: 6,
                track: [0.5, 0.5, 0.5, 0.28],
            }));
            const bits = [];
            if (p.stale && p.fetched_at)
                bits.push(s.staleSince(p.fetched_at));
            else if (w.resets_at)
                bits.push(s.resetsIn(w.resets_at));
            if (w.exhausts_at && !p.stale)
                bits.push(s.exhausts(w.exhausts_at));
            else if (showPace && !p.stale && paceText(s, w))
                bits.push(paceText(s, w));
            if (bits.length)
                row.add_child(small(bits.join(' · '), w.exhausts_at && !p.stale ? 'kota-warn-text' : ''));
        }
        box.add_child(row);
    }
}

/** QS kutucuğu ve sistem göstergesi için kısa özet metni. */
export function shortSummary(snapshot, settings, strings, max = 3) {
    const remaining = settings.get_boolean('show-remaining');
    const selected = settings.get_strv('panel-providers');
    const parts = [];
    for (const p of snapshot?.providers ?? []) {
        if (selected.length && !selected.includes(p.id))
            continue;
        const ws = windowsFor(p, settings, false, strings);
        if (ws.length) {
            const vals = ws.map(w => {
                const v = strings.percent(remaining ? Math.max(0, 100 - w.used_percent) : w.used_percent, w.estimated);
                return ws.length > 1 ? `${strings.windowLetter(w)} ${v}` : v;
            });
            parts.push(`${p.name} ${vals.join(' ')}`);
        }
        else if (p.balance)
            parts.push(`${p.name} ${strings.money(p.balance.amount, p.balance.currency)}`);
        if (parts.length >= max)
            break;
    }
    return parts.join(' · ');
}

/** En dolu sağlayıcı ve penceresi (logo ve sistem göstergesi için). */
export function topProvider(snapshot, settings) {
    const selected = settings.get_strv('panel-providers');
    let best = null;
    for (const p of snapshot?.providers ?? []) {
        if (selected.length && !selected.includes(p.id))
            continue;
        const w = highest(windowsFor(p, settings, false));
        if (w && (!best || w.used_percent > best.w.used_percent))
            best = {p, w};
    }
    return best;
}
