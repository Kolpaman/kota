// Kota — açılır pencere: başlık, özet kutucukları, AI kullanımı halkaları,
// son 24 saat grafiği, çalışan ajanlar ve filtre.

import Cairo from 'cairo';
import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import St from 'gi://St';

import {parseTime} from './i18n.js';
import {headline} from './panel.js';
import {paceOf, progressLine} from './progress.js';
import {paceText} from './summary.js';
import {
    VERTICAL, agentIconId, barChart, card, iconButton, ink, label, miniBars, providerIcon, ring, ringColor,
} from './widgets.js';

const TILE_BARS = 12;

/** Ağaç çizgileri: girinti genişliği, çizginin x konumu, kavis yarıçapı. */
const GUTTER = 22;
const LINE_X = 11;
const CURVE = 7;
/** Dirseğin satır üstünden uzaklığı (simgenin ortası). */
const NODE_ELBOW = 15;
const AGENT_ELBOW = 22;

/**
 * Yorum dizisi tarzı girinti. `cont`: atanın altında kardeş var mı (düz çizgi),
 * `last`: bu satır son çocuk mu (dirsekten sonra çizgi biter).
 */
function gutter({cont = false, elbow = null, last = false}) {
    const area = new St.DrawingArea({width: GUTTER, y_expand: true, style_class: 'kota-gutter'});
    area.connect('repaint', a => {
        const cr = a.get_context();
        const [w, h] = a.get_surface_size();
        const k = w / GUTTER;
        const x = LINE_X * k;
        cr.setLineWidth(1.5 * k);
        cr.setSourceRGBA(...ink(0.26));
        cr.setLineCap(Cairo.LineCap.ROUND);
        if (elbow === null) {
            if (cont) {
                cr.moveTo(x, 0);
                cr.lineTo(x, h);
                cr.stroke();
            }
        } else {
            const ey = elbow * k;
            const r = CURVE * k;
            cr.moveTo(x, 0);
            cr.lineTo(x, ey - r);
            cr.arcNegative(x + r, ey - r, r, Math.PI, Math.PI / 2);
            cr.lineTo(w - 2 * k, ey);
            cr.stroke();
            if (!last) {
                cr.moveTo(x, ey - r);
                cr.lineTo(x, h);
                cr.stroke();
            }
        }
        cr.$dispose();
    });
    return area;
}

/** Simge sütunu; `stem` ise simgenin altından satır sonuna çocuklara inen çizgi. */
function nodeIcon(icon, stem) {
    const col = new St.BoxLayout({orientation: VERTICAL, style_class: 'kota-node-col'});
    icon.x_align = Clutter.ActorAlign.CENTER;
    col.add_child(icon);
    if (stem) {
        const line = new St.DrawingArea({width: GUTTER, y_expand: true, style_class: 'kota-stem'});
        line.connect('repaint', a => {
            const cr = a.get_context();
            const [w, h] = a.get_surface_size();
            cr.setLineWidth(1.5 * (w / GUTTER));
            cr.setSourceRGBA(...ink(0.26));
            cr.moveTo(w / 2, 0);
            cr.lineTo(w / 2, h);
            cr.stroke();
            cr.$dispose();
        });
        col.add_child(line);
    }
    return col;
}

/** Ağaç satırı: atalar için düz çizgiler + kendi dirseği + içerik. */
function treeRow(ancestors, last, elbow, content) {
    const row = new St.BoxLayout({x_expand: true, style_class: 'kota-tree-row'});
    for (const cont of ancestors)
        row.add_child(gutter({cont}));
    row.add_child(gutter({elbow, last}));
    content.x_expand = true;
    row.add_child(content);
    return row;
}

function dim(text, extra = '') {
    const l = label(text, `kota-dim ${extra}`.trim());
    l.clutter_text.line_wrap = true;
    return l;
}

function needsAction(p) {
    return p.status === 'not_connected' || p.status === 'expired';
}

export class KotaPopup {
    /**
     * @param {object} o
     * @param {string} o.extPath
     * @param {Gio.Settings} o.settings
     * @param {import('./i18n.js').Strings} o.strings
     * @param {object} o.actions refresh, togglePause, openSettings, connect(id), stopAgent(pid), openPath(p), openUrl(u)
     */
    constructor({extPath, settings, strings, actions}) {
        this._extPath = extPath;
        this._settings = settings;
        this._strings = strings;
        this._actions = actions;
        this._snapshot = null;
        this._error = null;
        this._selected = null;
        this._filter = '';
        this._hoverHour = null;

        this.actor = new St.BoxLayout({orientation: VERTICAL, style_class: 'kota-root'});
        this._build();
    }

    setStrings(strings) {
        this._strings = strings;
        this.actor.destroy_all_children();
        this._build();
        this.update(this._snapshot, this._error);
    }

    _build() {
        const s = this._strings;
        this.actor.style = `width: ${this._settings.get_int('popup-width')}px;`;

        // Başlık
        const header = new St.BoxLayout({style_class: 'kota-header', x_expand: true});
        header.add_child(new St.Icon({
            gicon: Gio.icon_new_for_string(`${this._extPath}/icons/kota.svg`),
            icon_size: 18,
            style_class: 'kota-logo',
        }));
        header.add_child(label('Kota', 'kota-title'));
        this._subtitle = label(s.t('paused'), 'kota-subtitle kota-paused', {visible: false});
        header.add_child(this._subtitle);
        header.add_child(new St.Widget({x_expand: true}));
        this._pauseBtn = iconButton('media-playback-pause-symbolic', s.t('pause'), () => this._actions.togglePause());
        header.add_child(this._pauseBtn);
        this._refreshBtn = iconButton('view-refresh-symbolic', s.t('refresh_now'), () => this._actions.refresh());
        header.add_child(this._refreshBtn);
        header.add_child(iconButton('emblem-system-symbolic', s.t('settings'), () => this._actions.openSettings()));
        this.actor.add_child(header);

        this._tiles = new St.BoxLayout({style_class: 'kota-tiles', x_expand: true});
        this.actor.add_child(this._tiles);

        this._usage = card(s.t('card_usage'));
        this.actor.add_child(this._usage.card);

        this._activity = card(s.t('card_activity'));
        // Aralık seçici: 24 sa · 7 gün · 30 gün
        this._range = new St.BoxLayout({style_class: 'kota-range', y_align: Clutter.ActorAlign.CENTER});
        this._rangeBtns = {};
        for (const [id, key] of [['24h', 'range_24h'], ['7d', 'range_7d'], ['30d', 'range_30d']]) {
            const b = new St.Button({style_class: 'kota-range-btn', label: s.t(key), toggle_mode: true, can_focus: true});
            b.connect('clicked', () => {
                this._settings.set_string('activity-range', id);
                this._renderActivity();
            });
            this._rangeBtns[id] = b;
            this._range.add_child(b);
        }
        this._activity.head.insert_child_at_index(this._range, 1);
        this.actor.add_child(this._activity.card);

        this._live = card(s.t('card_live'));
        this._liveScroll = new St.ScrollView({
            style_class: 'kota-scroll',
            hscrollbar_policy: St.PolicyType.NEVER,
            vscrollbar_policy: St.PolicyType.AUTOMATIC,
            overlay_scrollbars: true,
            x_expand: true,
        });
        this._liveList = new St.BoxLayout({orientation: VERTICAL, x_expand: true, style_class: 'kota-live-list'});
        this._liveScroll.set_child(this._liveList);
        this._live.body.add_child(this._liveScroll);
        this.actor.add_child(this._live.card);

        this._entry = new St.Entry({
            style_class: 'kota-filter',
            hint_text: s.t('filter_hint'),
            can_focus: true,
            x_expand: true,
            primary_icon: new St.Icon({icon_name: 'edit-find-symbolic', icon_size: 14}),
            text: this._filter,
        });
        this._entry.clutter_text.connect('text-changed', () => {
            this._filter = this._entry.get_text().trim().toLowerCase();
            this._renderLive();
        });
        this.actor.add_child(this._entry);

        this._footer = dim('', 'kota-footer');
        this.actor.add_child(this._footer);
        this._applyVisibility();
    }

    _applyVisibility() {
        const st = this._settings;
        this._tiles.visible = st.get_boolean('show-tiles');
        this._usage.card.visible = st.get_boolean('show-usage');
        this._activity.card.visible = st.get_boolean('show-activity');
        this._live.card.visible = st.get_boolean('show-agents');
        this._entry.visible = st.get_boolean('show-filter') && st.get_boolean('show-agents');
    }

    /** Ayar değişince (genişlik, bölümler, sütunlar, renk). */
    relayout() {
        this.actor.style = `width: ${this._settings.get_int('popup-width')}px;`;
        this._applyVisibility();
        this.update(this._snapshot, this._error);
    }

    focusFilter() {
        if (this._entry.visible)
            this._entry.grab_key_focus();
    }

    update(snapshot, error) {
        this._snapshot = snapshot;
        this._error = error;
        const s = this._strings;
        const paused = Boolean(snapshot?.paused);
        this._pauseBtn.child.icon_name = paused ? 'media-playback-start-symbolic' : 'media-playback-pause-symbolic';
        this._pauseBtn.accessible_name = paused ? s.t('resume') : s.t('pause');
        this._subtitle.visible = paused;
        this._renderTiles();
        this._renderUsage();
        this._renderActivity();
        this._renderLive();
        this._renderFooter();
    }

    // ---------------------------------------------------------------- kutucuklar

    _tile(title, value, opts = {}) {
        const t = new St.BoxLayout({orientation: VERTICAL, style_class: 'kota-tile', x_expand: true});
        const top = new St.BoxLayout({x_expand: true, style_class: 'kota-tile-top'});
        top.add_child(label(title, 'kota-tile-title', {x_expand: true}));
        if (opts.badge)
            top.add_child(label(opts.badge, `kota-tile-badge ${opts.badgeClass ?? ''}`));
        t.add_child(top);
        const bottom = new St.BoxLayout({x_expand: true, style_class: 'kota-tile-bottom'});
        bottom.add_child(label(value, 'kota-tile-value', {y_align: Clutter.ActorAlign.END}));
        if (opts.unit)
            bottom.add_child(label(opts.unit, 'kota-tile-unit', {y_align: Clutter.ActorAlign.END}));
        bottom.add_child(new St.Widget({x_expand: true}));
        if (opts.side)
            bottom.add_child(opts.side);
        t.add_child(bottom);
        return t;
    }

    _renderTiles() {
        this._tiles.destroy_all_children();
        const s = this._strings;
        const snap = this._snapshot;
        const agents = snap?.agents ?? [];
        const mem = agents.reduce((a, x) => a + (x.rss_bytes ?? 0), 0);
        const working = agents.filter(a => s.idleFor(a.active_at) === s.t('working')).length;
        this._tiles.add_child(this._tile(s.t('tile_running'), String(agents.length), {
            badge: agents.length ? (working ? s.t('tile_live') : s.t('idle')) : '',
            badgeClass: working ? 'kota-live' : '',
            side: agents.length ? label(s.bytes(mem), 'kota-tile-side') : null,
        }));

        const act = snap?.activity;
        const hourly = act?.hourly ?? [];
        const accent = this._settings.get_string('accent-color');
        this._tiles.add_child(this._tile(s.t('tile_tokens'), act ? s.tokens(act.today_tokens) : s.t('tile_none'), {
            side: hourly.length ? miniBars(hourly.slice(-TILE_BARS), {accent}) : null,
        }));

        // En yakın sıfırlama (veri olan pencereler arasında)
        let next = null;
        for (const p of snap?.providers ?? []) {
            for (const w of p.windows ?? []) {
                const ms = parseTime(w.resets_at);
                if (!Number.isNaN(ms) && ms > Date.now() && (!next || ms < next.ms))
                    next = {ms, p, w};
            }
        }
        const value = next ? s.shortDuration((next.ms - Date.now()) / 1000 + 30) : s.t('tile_none');
        const side = next ? providerIcon(this._extPath, next.p.id, {size: 14, name: next.p.name}) : null;
        const tile = this._tile(s.t('tile_reset'), value, {side});
        if (next)
            tile.add_child(dim(`${next.p.name} · ${s.windowShort(next.w)} · ${s.when(next.ms)}`, 'kota-tile-foot'));
        this._tiles.add_child(tile);
    }

    // ---------------------------------------------------------------- kullanım

    _usageCell(p) {
        const s = this._strings;
        const w = headline(p);
        const icon = providerIcon(this._extPath, p.id, {size: 18, name: p.name});
        let frac = null, color = ringColor(p.id, w, p.stale), value, sub, subClass = 'kota-dim';
        let caption = '';
        if (w) {
            frac = w.used_percent / 100;
            const remaining = this._settings.get_boolean('show-remaining');
            value = s.percent(remaining ? Math.max(0, 100 - w.used_percent) : w.used_percent, w.estimated);
            caption = s.windowShort(w);
            sub = p.stale && p.fetched_at ? s.asOf(p.fetched_at) : w.resets_at ? s.resetsIn(w.resets_at) : '';
        } else if (p.balance) {
            const b = p.balance;
            value = s.money(b.amount, b.currency);
            caption = b.kind === 'spend' ? s.t('spend') : s.t('balance');
            frac = b.kind === 'spend' && b.budget ? b.amount / b.budget : 1;
            color = '#8b949e';
            sub = s.status(p);
        } else {
            value = p.name;
            caption = s.status(p);
        }
        if (needsAction(p)) {
            sub = `${s.connectLabel(p)}…`;
            subClass = 'kota-link';
        } else if (p.status === 'error' && !w) {
            sub = s.status(p);
        }

        const btn = new St.Button({style_class: 'kota-usage-cell', x_expand: true, can_focus: true});
        if (this._selected === p.id)
            btn.add_style_class_name('selected');
        const row = new St.BoxLayout({x_expand: true, style_class: 'kota-usage-row'});
        row.add_child(ring({size: 52, thickness: 4, fraction: frac, color, child: icon, dashed: frac === null}));
        const text = new St.BoxLayout({orientation: VERTICAL, x_expand: true, y_align: Clutter.ActorAlign.CENTER});
        text.add_child(label(p.name, 'kota-usage-name'));
        text.add_child(label(value, `kota-usage-value ${w && !p.stale ? `kota-sev-${w.severity}` : ''}`));
        if (caption)
            text.add_child(label(caption, 'kota-usage-caption'));
        if (sub)
            text.add_child(label(sub, `kota-usage-sub ${subClass}`));
        row.add_child(text);
        btn.set_child(row);
        btn.connect('clicked', () => {
            if (needsAction(p) && !w) {
                this._actions.connect(p.id);
                return;
            }
            this._selected = this._selected === p.id ? null : p.id;
            this._renderUsage();
        });
        return btn;
    }

    _renderUsage() {
        const s = this._strings;
        const body = this._usage.body;
        body.destroy_all_children();
        const providers = this._snapshot?.providers ?? [];
        this._usage.right.text = '';
        if (!this._snapshot) {
            if (this._error?.kind === 'service-missing') {
                body.add_child(label(s.t('service_missing'), 'kota-strong'));
                body.add_child(dim(s.t('service_missing_hint')));
            } else if (this._error) {
                body.add_child(label(s.t('service_error'), 'kota-strong'));
                body.add_child(dim(this._error.message));
            } else {
                body.add_child(dim(s.t('loading')));
            }
            return;
        }
        if (!providers.length) {
            body.add_child(dim(s.t('no_providers')));
            return;
        }
        const filtered = this._filter
            ? providers.filter(p => `${p.id} ${p.name}`.toLowerCase().includes(this._filter))
            : providers;
        if (this._settings.get_string('usage-style') === 'bars') {
            for (const p of filtered)
                body.add_child(this._usageBar(p));
        } else {
            this._usageGrid(body, filtered);
        }
        const sel = providers.find(p => p.id === this._selected);
        if (sel)
            body.add_child(this._detail(sel));
        else
            this._selected = null;
    }

    /** Çizgi düzeni: sağlayıcı başına bir satır (logo, ad, değer, ilerleme çizgisi). */
    _usageBar(p) {
        const s = this._strings;
        const w = headline(p);
        const btn = new St.Button({style_class: 'kota-usage-cell kota-usage-bar', x_expand: true, can_focus: true});
        if (this._selected === p.id)
            btn.add_style_class_name('selected');
        const col = new St.BoxLayout({orientation: VERTICAL, x_expand: true, style_class: 'kota-usage-bar-box'});
        const top = new St.BoxLayout({x_expand: true, style_class: 'kota-usage-bar-top'});
        top.add_child(providerIcon(this._extPath, p.id, {size: 18, name: p.name}));
        const names = new St.BoxLayout({orientation: VERTICAL, x_expand: true, y_align: Clutter.ActorAlign.CENTER});
        names.add_child(label(p.name, 'kota-usage-bar-name'));
        let sub;
        if (needsAction(p))
            sub = label(`${s.connectLabel(p)}…`, 'kota-usage-sub kota-link');
        else if (w)
            sub = label(`${s.windowShort(w)} · ${p.stale && p.fetched_at ? s.asOf(p.fetched_at) : s.resetsIn(w.resets_at)}`, 'kota-usage-sub');
        else
            sub = label(s.status(p), 'kota-usage-sub');
        names.add_child(sub);
        top.add_child(names);
        const remaining = this._settings.get_boolean('show-remaining');
        let value = s.status(p), cls = 'kota-usage-bar-value';
        if (w) {
            value = s.percent(remaining ? Math.max(0, 100 - w.used_percent) : w.used_percent, w.estimated);
            cls += ` kota-sev-${p.stale || w.estimated ? 'stale' : w.severity}`;
        } else if (p.balance) {
            value = s.money(p.balance.amount, p.balance.currency);
        }
        top.add_child(label(value, cls));
        col.add_child(top);
        if (w) {
            col.add_child(progressLine({
                fraction: w.used_percent / 100,
                color: ringColor(p.id, w, p.stale),
                pace: this._settings.get_boolean('show-pace') && !p.stale ? paceOf(w) : null,
                height: 7,
            }));
        }
        btn.set_child(col);
        btn.connect('clicked', () => {
            if (needsAction(p) && !w) {
                this._actions.connect(p.id);
                return;
            }
            this._selected = this._selected === p.id ? null : p.id;
            this._renderUsage();
        });
        return btn;
    }

    _usageGrid(body, filtered) {
        const cols = Math.max(1, Math.min(3, this._settings.get_int('usage-columns')));
        let row = null;
        filtered.forEach((p, i) => {
            if (i % cols === 0) {
                row = new St.BoxLayout({x_expand: true, style_class: 'kota-usage-grid-row'});
                body.add_child(row);
            }
            row.add_child(this._usageCell(p));
        });
        // Son satırı hizalamak için boş hücreler
        const rest = filtered.length % cols;
        if (row && rest)
            for (let k = rest; k < cols; k++)
                row.add_child(new St.Widget({x_expand: true, style_class: 'kota-usage-cell kota-empty'}));
    }

    _detail(p) {
        const s = this._strings;
        const box = new St.BoxLayout({orientation: VERTICAL, style_class: 'kota-detail', x_expand: true});
        const head = new St.BoxLayout({x_expand: true, style_class: 'kota-detail-head'});
        head.add_child(providerIcon(this._extPath, p.id, {size: 16, name: p.name}));
        head.add_child(label(p.name, 'kota-strong'));
        if (p.plan)
            head.add_child(label(p.plan, 'kota-chip'));
        head.add_child(new St.Widget({x_expand: true}));
        head.add_child(label(`● ${s.status(p)}`, `kota-status kota-status-${p.status}`));
        box.add_child(head);
        if (p.stale && p.fetched_at) {
            const src = p.source === 'local_logs' ? `${s.t('local_log')} · ` : '';
            box.add_child(dim(src + s.staleSince(p.fetched_at)));
        }
        if ((p.status === 'error' || p.status === 'unsupported') && p.message)
            box.add_child(dim(p.message));
        const remaining = this._settings.get_boolean('show-remaining');
        for (const w of p.windows ?? []) {
            const wr = new St.BoxLayout({orientation: VERTICAL, style_class: 'kota-window', x_expand: true});
            const top = new St.BoxLayout({x_expand: true});
            top.add_child(label(s.windowLabel(w), 'kota-window-label', {x_expand: true}));
            const shown = remaining ? Math.max(0, 100 - w.used_percent) : w.used_percent;
            top.add_child(label(s.percent(shown, w.estimated) + (remaining ? ` ${s.t('left')}` : ''),
                `kota-window-pct kota-sev-${p.stale || w.estimated ? 'stale' : w.severity}`));
            wr.add_child(top);
            const showPace = this._settings.get_boolean('show-pace') && !p.stale;
            wr.add_child(progressLine({
                fraction: w.used_percent / 100,
                color: ringColor(p.id, w, p.stale),
                pace: showPace ? paceOf(w) : null,
                height: 8,
            }));
            const parts = [];
            if (w.resets_at)
                parts.push(s.resets(w.resets_at));
            if (w.exhausts_at && !p.stale)
                parts.push(s.exhausts(w.exhausts_at));
            else if (showPace && paceText(s, w))
                parts.push(paceText(s, w));
            if (parts.length)
                wr.add_child(dim(parts.join(' · '), w.exhausts_at && !p.stale ? 'kota-warn-text' : ''));
            box.add_child(wr);
        }
        if (p.balance) {
            const b = p.balance;
            let t = `${b.kind === 'spend' ? s.t('spend') : s.t('balance')}: ${s.money(b.amount, b.currency)}`;
            if (b.budget)
                t += ` / ${s.money(b.budget, b.currency)} ${s.t('budget')}`;
            box.add_child(label(t, 'kota-strong-small'));
        }
        const shares = (p.details ?? []).filter(d => d.id.startsWith('weekly_share:'));
        if (shares.length)
            box.add_child(dim(`${s.t('weekly_share')}: ${shares.map(d => `${d.label} ${s.detailValue(d.value)}`).join(' · ')}`));
        for (const d of (p.details ?? []).filter(x => !x.id.startsWith('weekly_share:')))
            box.add_child(dim(`${s.detailLabel(d)}: ${s.detailValue(d.value)}`));
        const local = s.localLine(p.local);
        if (local)
            box.add_child(dim(local));
        if (needsAction(p)) {
            const b = new St.Button({style_class: 'kota-pill-btn', label: `${s.connectLabel(p)}…`, x_align: Clutter.ActorAlign.START});
            b.connect('clicked', () => this._actions.connect(p.id));
            box.add_child(b);
        }
        return box;
    }

    // ---------------------------------------------------------------- 24 saat

    _renderActivity() {
        const s = this._strings;
        const body = this._activity.body;
        body.destroy_all_children();
        const act = this._snapshot?.activity;
        let range = this._settings.get_string('activity-range');
        const daily = act?.daily ?? [];
        // Günlük geçmiş yoksa (yerel log kapalı/eski servis) seçici gizlenir.
        this._range.visible = daily.length > 0;
        if (!daily.length)
            range = '24h';
        for (const [id, b] of Object.entries(this._rangeBtns))
            b.checked = id === range;
        if (range !== '24h') {
            this._renderDaily(body, act, range === '7d' ? 7 : 30);
            return;
        }
        this._activity.title.text = s.t('card_activity');
        const values = act?.hourly ?? [];
        if (!values.length || values.every(v => !v)) {
            this._activity.right.text = '';
            body.add_child(dim(s.t('no_activity')));
            return;
        }
        const startMs = parseTime(act.start);
        const setRight = i => {
            if (i === null) {
                this._activity.right.text = `${act.sessions} ${s.t('sessions')}`;
            } else {
                const h = startMs + i * 3600000;
                this._activity.right.text = `${s.hourLabel(h)} · ${s.tokens(values[i])}`;
            }
        };
        setRight(null);
        body.add_child(barChart(values, {
            height: 92,
            accent: this._settings.get_string('accent-color'),
            onHover: i => setRight(i),
        }));
        const axis = new St.BoxLayout({x_expand: true, style_class: 'kota-axis'});
        axis.add_child(dim(s.hourLabel(startMs), 'kota-axis-label'));
        axis.add_child(new St.Widget({x_expand: true}));
        axis.add_child(dim(s.t('now'), 'kota-axis-label'));
        body.add_child(axis);
    }

    /** Son `days` günün günlük token grafiği ve haftalık karşılaştırma. */
    _renderDaily(body, act, days) {
        const s = this._strings;
        this._activity.title.text = s.t('card_history');
        const all = act.daily;
        const values = all.slice(-days);
        const [y, m, d] = (act.daily_start ?? '').split('-').map(Number);
        const firstMs = new Date(y, (m || 1) - 1, d || 1).getTime() + (all.length - values.length) * 86400000;
        const total = values.reduce((a, b) => a + b, 0);
        const setRight = i => {
            this._activity.right.text = i === null ? s.tokens(total)
                : `${s.dayLabel(firstMs + i * 86400000, true)} · ${s.tokens(values[i])}`;
        };
        setRight(null);
        if (!total) {
            body.add_child(dim(s.t('no_activity')));
            return;
        }
        body.add_child(barChart(values, {
            height: 92,
            accent: this._settings.get_string('accent-color'),
            highlight: 1,
            onHover: i => setRight(i),
        }));
        const axis = new St.BoxLayout({x_expand: true, style_class: 'kota-axis'});
        if (days === 7) {
            values.forEach((_v, i) => axis.add_child(label(s.dayLabel(firstMs + i * 86400000),
                'kota-dim kota-axis-label kota-axis-day', {x_expand: true})));
        } else {
            axis.add_child(dim(s.dayLabel(firstMs, true), 'kota-axis-label'));
            axis.add_child(new St.Widget({x_expand: true}));
            axis.add_child(dim(s.t('now'), 'kota-axis-label'));
        }
        body.add_child(axis);
        // Bu hafta ile geçen haftanın karşılaştırması (son 7 gün / önceki 7 gün).
        const week = all.slice(-7).reduce((a, b) => a + b, 0);
        const prev = all.slice(-14, -7).reduce((a, b) => a + b, 0);
        const parts = [`${s.t('this_week')}: ${s.tokens(week)}`];
        let cls = '';
        if (all.length >= 14 && prev > 0) {
            const pct = Math.round(((week - prev) / prev) * 100);
            if (Math.abs(pct) < 3) {
                parts.push(s.t('vs_last_week_same'));
            } else {
                parts.push(s.fmt(pct > 0 ? 'vs_last_week_up' : 'vs_last_week_down', {n: Math.abs(pct)}));
                cls = pct > 0 ? 'kota-warn-text' : 'kota-good-text';
            }
        }
        parts.push(`${s.t('daily_avg')} ${s.tokens(Math.round(total / days))}`);
        body.add_child(dim(parts.join(' · '), cls));
    }

    // ---------------------------------------------------------------- ajanlar

    _renderLive() {
        const s = this._strings;
        const list = this._liveList;
        const adj = this._liveScroll.vadjustment;
        const keep = adj?.value ?? 0;
        list.destroy_all_children();
        const agents = this._snapshot?.agents ?? [];
        const f = this._filter;
        const shown = f
            ? agents.filter(a => `${a.name} ${a.kind} ${a.project} ${a.project_path} ${a.pid} ${(a.ports ?? []).join(' ')}`
                .toLowerCase().includes(f))
            : agents;
        this._live.right.text = '';
        this._live.head.get_first_child().text = `${s.t('card_live')}  ${agents.length}`;
        if (!agents.length) {
            list.add_child(dim(s.t('no_agents')));
            return;
        }
        if (!shown.length) {
            list.add_child(dim(s.t('no_match')));
            return;
        }
        // Klasör → alt proje → ajan ağacı. Bir klasörde birden fazla proje
        // olabilir: ajan, en son dokunduğu alt projenin altında gösterilir.
        const groups = new Map();
        for (const a of shown) {
            const k = a.project_path;
            if (!groups.has(k))
                groups.set(k, {project: a.project, nodes: new Map(), loose: []});
            const grp = groups.get(k);
            const top = a.focus?.[0];
            if (top) {
                if (!grp.nodes.has(top.path))
                    grp.nodes.set(top.path, {focus: top, agents: []});
                grp.nodes.get(top.path).agents.push(a);
            } else {
                grp.loose.push(a);
            }
        }
        for (const [path, grp] of groups) {
            const total = grp.loose.length + [...grp.nodes.values()].reduce((n, x) => n + x.agents.length, 0);
            const g = new St.BoxLayout({orientation: VERTICAL, style_class: 'kota-group', x_expand: true});
            const gh = new St.BoxLayout({x_expand: true, style_class: 'kota-group-head'});
            gh.add_child(nodeIcon(new St.Bin({
                style_class: 'kota-folder',
                child: new St.Icon({icon_name: 'folder-symbolic', icon_size: 14}),
            }), true));
            const names = new St.BoxLayout({x_expand: true, style_class: 'kota-group-names', y_align: Clutter.ActorAlign.START});
            names.add_child(label(grp.project, 'kota-strong'));
            names.add_child(label(path, 'kota-dim kota-path', {x_expand: true}));
            names.add_child(label(`${total} ${s.t('agents_unit')}`, 'kota-dim'));
            gh.add_child(names);
            g.add_child(gh);

            const children = [
                ...[...grp.nodes.values()].map(n => ({node: n})),
                ...grp.loose.map(a => ({agent: a})),
            ];
            children.forEach((c, i) => {
                const last = i === children.length - 1;
                if (c.agent) {
                    g.add_child(treeRow([], last, AGENT_ELBOW, this._agentRow(c.agent, null)));
                    return;
                }
                g.add_child(treeRow([], last, NODE_ELBOW, this._focusRow(c.node, path)));
                c.node.agents.forEach((a, j) => {
                    g.add_child(treeRow([!last], j === c.node.agents.length - 1, AGENT_ELBOW,
                        this._agentRow(a, c.node.focus)));
                });
            });
            list.add_child(g);
        }
        if (adj)
            adj.value = keep;
    }

    /** Alt proje düğümü: ad, klasöre göre yol, son dokunuş. */
    _focusRow(node, root) {
        const s = this._strings;
        const row = new St.BoxLayout({x_expand: true, style_class: 'kota-node'});
        row.add_child(nodeIcon(new St.Bin({
            style_class: 'kota-node-icon',
            child: new St.Icon({icon_name: 'folder-symbolic', icon_size: 12}),
        }), true));
        const text = new St.BoxLayout({x_expand: true, style_class: 'kota-node-text', y_align: Clutter.ActorAlign.START});
        text.add_child(label(node.focus.name, 'kota-node-name'));
        const rel = node.focus.path.startsWith(`${root}/`) ? node.focus.path.slice(root.length + 1) : node.focus.path;
        if (rel !== node.focus.name)
            text.add_child(label(rel, 'kota-dim kota-path'));
        text.add_child(new St.Widget({x_expand: true}));
        if (node.focus.at)
            text.add_child(label(s.ago(node.focus.at), 'kota-dim kota-path'));
        row.add_child(text);
        const open = iconButton('folder-open-symbolic', s.t('open_folder'), () => this._actions.openPath(node.focus.path),
            'kota-icon-btn kota-node-open');
        row.add_child(open);
        return row;
    }

    _agentRow(a, focus) {
        const s = this._strings;
        const row = new St.BoxLayout({x_expand: true, style_class: 'kota-agent'});
        const iconBox = new St.Bin({style_class: 'kota-agent-icon',
            child: providerIcon(this._extPath, agentIconId(a.kind), {size: 18, name: a.name}),
            y_align: Clutter.ActorAlign.START});
        row.add_child(iconBox);
        const text = new St.BoxLayout({orientation: VERTICAL, x_expand: true, y_align: Clutter.ActorAlign.CENTER});
        text.add_child(label(`${a.name}`, 'kota-agent-name'));
        const state = s.idleFor(a.active_at);
        const bits = [state, s.bytes(a.rss_bytes)];
        if (a.tokens)
            bits.push(`${s.tokens(a.tokens)} ${s.t('tokens_unit')}`);
        if (a.ports?.length)
            bits.push(`${a.ports.length} ${s.t('ports_unit')}`);
        text.add_child(label(bits.join(' · '), `kota-dim ${state === s.t('working') ? 'kota-working' : ''}`));
        const others = (a.focus ?? []).filter(f => f.path !== focus?.path).map(f => f.name);
        if (others.length)
            text.add_child(dim(`${s.t('also')}: ${others.join(' · ')}`, 'kota-path'));
        if (a.ports?.length) {
            const chips = new St.BoxLayout({style_class: 'kota-chips'});
            for (const port of a.ports.slice(0, 6)) {
                const c = new St.Button({style_class: 'kota-port', label: `:${port}`});
                c.connect('clicked', () => this._actions.openUrl(`http://localhost:${port}`));
                chips.add_child(c);
            }
            text.add_child(chips);
        }
        row.add_child(text);
        row.add_child(iconButton('media-playback-stop-symbolic', s.t('stop'), () => this._actions.stopAgent(a.pid),
            'kota-icon-btn kota-stop'));
        row.add_child(iconButton('folder-open-symbolic', s.t('open_folder'),
            () => this._actions.openPath(focus?.path ?? a.cwd), 'kota-icon-btn kota-go'));
        return row;
    }

    _renderFooter() {
        const s = this._strings;
        if (!this._snapshot) {
            this._footer.text = '';
            return;
        }
        let t = `${s.t('updated_at')} ${s.when(parseTime(this._snapshot.generated_at))}`;
        if (this._error)
            t += ` · ${s.t('service_error')}: ${this._error.message}`;
        this._footer.text = t;
    }

    destroy() {
        this.actor.destroy();
    }
}
