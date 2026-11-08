// Kota — açılır pencere (GNOME eklentisindeki popup.js'in birebir karşılığı):
// başlık, özet kutucukları, AI kullanımı, geçmiş grafiği, çalışan ajanlar.

import {parseTime} from './i18n.js';
import {Settings, h, invoke, listen, makeStrings} from './api.js';
import {notify} from './notify.js';
import {
    agentIconId, barChart, headline, iconButton, miniBars, paceOf, paceText, progressLine, providerIcon, ring,
    ringColor, setBarMode,
} from './widgets.js';

const TILE_BARS = 12;
const root = document.getElementById('app');
let settings, s;
let snapshot = null;
let selected = null;
let filter = '';
let previous = null;

const needsAction = p => p.status === 'not_connected' || p.status === 'expired';
const dim = (text, cls = '') => h(`div.kota-dim${cls ? `.${cls.split(' ').join('.')}` : ''}`, {}, text);
const sev = (p, w) => `kota-sev-${p.stale || w.estimated ? 'stale' : w.severity}`;

function card(title, rightText = '') {
    const right = h('span.kota-card-right', {}, rightText);
    const titleEl = h('span.kota-card-title', {}, title);
    const head = h('div.kota-card-head', {}, titleEl, h('span.kota-grow'), right);
    const body = h('div.kota-card-body');
    return {card: h('section.kota-card', {}, head, body), head, body, right, title: titleEl};
}

// ---------------------------------------------------------------- tema ve arka plan

if (/Windows/.test(navigator.userAgent))
    document.documentElement.classList.add('kota-windows');

function applyLook() {
    const theme = settings.get_string('popup-theme');
    const light = theme === 'light' || (theme === 'auto' && window.matchMedia('(prefers-color-scheme: light)').matches);
    document.documentElement.classList.toggle('kota-light', light);
    const bg = settings.get_string('popup-background');
    document.documentElement.classList.toggle('kota-glass', bg !== 'opaque');
    const op = Math.max(0, Math.min(100, settings.get_int('popup-opacity'))) / 100;
    document.documentElement.style.setProperty('--glass-alpha', String(bg === 'opaque' ? 1 : Math.max(0.25, op + 0.25)));
    document.documentElement.style.setProperty('--accent', settings.get_string('accent-color') || '#f5a623');
    setBarMode(settings.get_string('bar-colors'));
}

// ---------------------------------------------------------------- yerleşim

let els = {};

function build() {
    root.replaceChildren();
    root.style.width = `${settings.get_int('popup-width')}px`;
    const paused = Boolean(snapshot?.paused);
    const header = h('header.kota-header', {'data-tauri-drag-region': ''},
        h('img.kota-logo', {src: 'icons/kota.svg', width: 18, height: 18, alt: ''}),
        h('span.kota-title', {}, 'Kota'),
        h('span.kota-subtitle.kota-paused', {hidden: !paused}, s.t('paused')),
        h('span.kota-grow'),
        els.pause = iconButton(paused ? 'play' : 'pause', paused ? s.t('resume') : s.t('pause'),
            () => invoke('set_paused', {paused: !snapshot?.paused})),
        iconButton('refresh', s.t('refresh_now'), async () => update(await invoke('refresh'))),
        iconButton('settings', s.t('settings'), () => invoke('open_settings_window')));
    els.tiles = h('div.kota-tiles');
    els.usage = card(s.t('card_usage'));
    els.activity = card(s.t('card_activity'));
    els.range = h('div.kota-range');
    els.rangeBtns = {};
    for (const [id, key] of [['24h', 'range_24h'], ['7d', 'range_7d'], ['30d', 'range_30d']]) {
        const b = h('button.kota-range-btn', {onclick: async () => {
            await settings.set_string('activity-range', id);
            renderActivity();
            fit();
        }}, s.t(key));
        els.rangeBtns[id] = b;
        els.range.append(b);
    }
    els.activity.head.insertBefore(els.range, els.activity.head.children[1]);
    els.live = card(s.t('card_live'));
    els.liveList = h('div.kota-live-list');
    els.live.body.append(h('div.kota-scroll', {}, els.liveList));
    els.entry = h('label.kota-filter', {},
        h('span.kota-glyph', {'data-icon': 'search'}),
        h('input', {type: 'search', placeholder: s.t('filter_hint'), value: filter, oninput: e => {
            filter = e.target.value.trim().toLowerCase();
            renderLive();
            renderUsage();
            fit();
        }}));
    els.footer = dim('', 'kota-footer');
    root.append(header, els.tiles, els.usage.card, els.activity.card, els.live.card, els.entry, els.footer);
    applyVisibility();
}

function applyVisibility() {
    els.tiles.hidden = !settings.get_boolean('show-tiles');
    els.usage.card.hidden = !settings.get_boolean('show-usage');
    els.activity.card.hidden = !settings.get_boolean('show-activity');
    els.live.card.hidden = !settings.get_boolean('show-agents');
    els.entry.hidden = !(settings.get_boolean('show-filter') && settings.get_boolean('show-agents'));
}

function update(snap) {
    if (snap !== undefined)
        snapshot = snap;
    const paused = Boolean(snapshot?.paused);
    const pauseGlyph = els.pause.querySelector('.kota-glyph');
    pauseGlyph.dataset.icon = paused ? 'play' : 'pause';
    els.pause.title = paused ? s.t('resume') : s.t('pause');
    root.querySelector('.kota-paused').hidden = !paused;
    renderTiles();
    renderUsage();
    renderActivity();
    renderLive();
    renderFooter();
    fit();
}

/** Pencereyi içeriğe göre boyutlandır. */
let fitTimer = 0;
function fit() {
    cancelAnimationFrame(fitTimer);
    fitTimer = requestAnimationFrame(() => {
        const r = root.getBoundingClientRect();
        const pad = parseFloat(getComputedStyle(document.body).paddingTop) || 0;
        // min-height: 100vh ölçümü bozmasın: içeriğin doğal yüksekliği.
        const shown = [...root.children].filter(c => !c.hidden);
        const natural = shown.reduce((sum, c) => sum + c.getBoundingClientRect().height, 0) + (shown.length - 1) * 10 + 20;
        invoke('fit_popup', {width: Math.ceil(r.width + 2 * pad), height: Math.ceil(Math.max(natural, 120) + 2 * pad)});
    });
}

// ---------------------------------------------------------------- kutucuklar

function tile(title, value, opts = {}) {
    return h('div.kota-tile', {},
        h('div.kota-tile-top', {}, h('span.kota-tile-title', {}, title),
            opts.badge ? h(`span.kota-tile-badge${opts.badgeClass ? `.${opts.badgeClass}` : ''}`, {}, opts.badge) : null),
        h('div.kota-tile-bottom', {}, h('span.kota-tile-value', {}, value),
            opts.unit ? h('span.kota-tile-unit', {}, opts.unit) : null, h('span.kota-grow'), opts.side ?? null),
        opts.foot ? dim(opts.foot, 'kota-tile-foot') : null);
}

function renderTiles() {
    const agents = snapshot?.agents ?? [];
    const mem = agents.reduce((a, x) => a + (x.rss_bytes ?? 0), 0);
    const working = agents.filter(a => s.idleFor(a.active_at) === s.t('working')).length;
    const act = snapshot?.activity;
    const hourly = act?.hourly ?? [];
    let next = null;
    for (const p of snapshot?.providers ?? []) {
        for (const w of p.windows ?? []) {
            const ms = parseTime(w.resets_at);
            if (!Number.isNaN(ms) && ms > Date.now() && (!next || ms < next.ms))
                next = {ms, p, w};
        }
    }
    els.tiles.replaceChildren(
        tile(s.t('tile_running'), String(agents.length), {
            badge: agents.length ? (working ? s.t('tile_live') : s.t('idle')) : '',
            badgeClass: working ? 'kota-live' : '',
            side: agents.length ? h('span.kota-tile-side', {}, s.bytes(mem)) : null,
        }),
        tile(s.t('tile_tokens'), act ? s.tokens(act.today_tokens) : s.t('tile_none'), {
            side: hourly.length ? miniBars(hourly.slice(-TILE_BARS), {accent: settings.get_string('accent-color')}) : null,
        }),
        tile(s.t('tile_reset'), next ? s.shortDuration((next.ms - Date.now()) / 1000 + 30) : s.t('tile_none'), {
            side: next ? providerIcon(next.p.id, {size: 14, name: next.p.name}) : null,
            foot: next ? `${next.p.name} · ${s.windowShort(next.w)} · ${s.when(next.ms)}` : '',
        }));
}

// ---------------------------------------------------------------- kullanım

function select(p, w) {
    if (needsAction(p) && !w) {
        invoke('connect', {id: p.id}).catch(e => console.warn(e));
        return;
    }
    selected = selected === p.id ? null : p.id;
    renderUsage();
    fit();
}

function usageCell(p) {
    const w = headline(p);
    let frac = null, color = ringColor(p.id, w, p.stale), value, sub = '', subLink = false, caption = '';
    const remaining = settings.get_boolean('show-remaining');
    if (w) {
        frac = w.used_percent / 100;
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
        subLink = true;
    } else if (p.status === 'error' && !w) {
        sub = s.status(p);
    }
    return h('button.kota-usage-cell', {class: selected === p.id ? 'selected' : '', onclick: () => select(p, w)},
        h('div.kota-usage-row', {},
            ring({size: 52, thickness: 4, fraction: frac, color, child: providerIcon(p.id, {size: 18, name: p.name}), dashed: frac === null}),
            h('div.kota-usage-text', {},
                h('div.kota-usage-name', {}, p.name),
                h('div.kota-usage-value', {class: w && !p.stale ? `kota-sev-${w.severity}` : ''}, value),
                caption ? h('div.kota-usage-caption', {}, caption) : null,
                sub ? h('div.kota-usage-sub', {class: subLink ? 'kota-link' : ''}, sub) : null)));
}

function usageBar(p) {
    const w = headline(p);
    const remaining = settings.get_boolean('show-remaining');
    let value = s.status(p), cls = '';
    if (w) {
        value = s.percent(remaining ? Math.max(0, 100 - w.used_percent) : w.used_percent, w.estimated);
        cls = sev(p, w);
    } else if (p.balance) {
        value = s.money(p.balance.amount, p.balance.currency);
    }
    const sub = needsAction(p) ? h('div.kota-usage-sub.kota-link', {}, `${s.connectLabel(p)}…`)
        : w ? h('div.kota-usage-sub', {}, `${s.windowShort(w)} · ${p.stale && p.fetched_at ? s.asOf(p.fetched_at) : s.resetsIn(w.resets_at)}`)
            : h('div.kota-usage-sub', {}, s.status(p));
    return h('button.kota-usage-cell.kota-usage-bar', {class: selected === p.id ? 'selected' : '', onclick: () => select(p, w)},
        h('div.kota-usage-bar-top', {}, providerIcon(p.id, {size: 18, name: p.name}),
            h('div.kota-grow', {}, h('div.kota-usage-bar-name', {}, p.name), sub),
            h('span.kota-usage-bar-value', {class: cls}, value)),
        w ? progressLine({fraction: w.used_percent / 100, color: ringColor(p.id, w, p.stale),
            pace: settings.get_boolean('show-pace') && !p.stale ? paceOf(w) : null, height: 7}) : null);
}

function detail(p) {
    const box = h('div.kota-detail');
    box.append(h('div.kota-detail-head', {}, providerIcon(p.id, {size: 16, name: p.name}),
        h('span.kota-strong', {}, p.name), p.plan ? h('span.kota-chip', {}, p.plan) : null, h('span.kota-grow'),
        h(`span.kota-status.kota-status-${p.status}`, {}, `● ${s.status(p)}`)));
    if (p.stale && p.fetched_at)
        box.append(dim((p.source === 'local_logs' ? `${s.t('local_log')} · ` : '') + s.staleSince(p.fetched_at)));
    if ((p.status === 'error' || p.status === 'unsupported') && p.message)
        box.append(dim(p.message));
    const remaining = settings.get_boolean('show-remaining');
    const showPace = settings.get_boolean('show-pace') && !p.stale;
    for (const w of p.windows ?? []) {
        const shown = remaining ? Math.max(0, 100 - w.used_percent) : w.used_percent;
        const parts = [];
        if (w.resets_at)
            parts.push(s.resets(w.resets_at));
        if (w.exhausts_at && !p.stale)
            parts.push(s.exhausts(w.exhausts_at));
        else if (showPace && paceText(s, w))
            parts.push(paceText(s, w));
        box.append(h('div.kota-window', {},
            h('div.kota-window-top', {}, h('span.kota-window-label', {}, s.windowLabel(w)), h('span.kota-grow'),
                h('span.kota-window-pct', {class: sev(p, w)}, s.percent(shown, w.estimated) + (remaining ? ` ${s.t('left')}` : ''))),
            progressLine({fraction: w.used_percent / 100, color: ringColor(p.id, w, p.stale), pace: showPace ? paceOf(w) : null, height: 8}),
            parts.length ? dim(parts.join(' · '), w.exhausts_at && !p.stale ? 'kota-warn-text' : '') : null));
    }
    if (p.balance) {
        const b = p.balance;
        let t = `${b.kind === 'spend' ? s.t('spend') : s.t('balance')}: ${s.money(b.amount, b.currency)}`;
        if (b.budget)
            t += ` / ${s.money(b.budget, b.currency)} ${s.t('budget')}`;
        box.append(h('div.kota-strong-small', {}, t));
    }
    const shares = (p.details ?? []).filter(d => d.id.startsWith('weekly_share:'));
    if (shares.length)
        box.append(dim(`${s.t('weekly_share')}: ${shares.map(d => `${d.label} ${s.detailValue(d.value)}`).join(' · ')}`));
    for (const d of (p.details ?? []).filter(x => !x.id.startsWith('weekly_share:')))
        box.append(dim(`${s.detailLabel(d)}: ${s.detailValue(d.value)}`));
    const local = s.localLine(p.local);
    if (local)
        box.append(dim(local));
    if (needsAction(p))
        box.append(h('button.kota-pill-btn', {onclick: () => invoke('connect', {id: p.id})}, `${s.connectLabel(p)}…`));
    return box;
}

function renderUsage() {
    const body = els.usage.body;
    body.replaceChildren();
    if (!snapshot) {
        body.append(dim(s.t('loading')));
        return;
    }
    const providers = snapshot.providers ?? [];
    if (!providers.length) {
        body.append(dim(s.t('no_providers')));
        return;
    }
    const list = filter ? providers.filter(p => `${p.id} ${p.name}`.toLowerCase().includes(filter)) : providers;
    if (settings.get_string('usage-style') === 'bars') {
        list.forEach(p => body.append(usageBar(p)));
    } else {
        const cols = Math.max(1, Math.min(3, settings.get_int('usage-columns')));
        body.append(h('div.kota-usage-grid', {style: {gridTemplateColumns: `repeat(${cols}, 1fr)`}}, list.map(usageCell)));
    }
    const sel = providers.find(p => p.id === selected);
    if (sel)
        body.append(detail(sel));
    else
        selected = null;
}

// ---------------------------------------------------------------- geçmiş

function renderActivity() {
    const body = els.activity.body;
    body.replaceChildren();
    const act = snapshot?.activity;
    let range = settings.get_string('activity-range');
    const daily = act?.daily ?? [];
    els.range.hidden = !daily.length;
    if (!daily.length)
        range = '24h';
    for (const [id, b] of Object.entries(els.rangeBtns))
        b.classList.toggle('checked', id === range);
    const accent = settings.get_string('accent-color');
    if (range !== '24h') {
        renderDaily(body, act, range === '7d' ? 7 : 30, accent);
        return;
    }
    els.activity.title.textContent = s.t('card_activity');
    const values = act?.hourly ?? [];
    if (!values.length || values.every(v => !v)) {
        els.activity.right.textContent = '';
        body.append(dim(s.t('no_activity')));
        return;
    }
    const startMs = parseTime(act.start);
    const setRight = i => {
        els.activity.right.textContent = i === null ? `${act.sessions} ${s.t('sessions')}`
            : `${s.hourLabel(startMs + i * 3600000)} · ${s.tokens(values[i])}`;
    };
    setRight(null);
    body.append(barChart(values, {accent, onHover: setRight}),
        h('div.kota-axis', {}, dim(s.hourLabel(startMs), 'kota-axis-label'), h('span.kota-grow'), dim(s.t('now'), 'kota-axis-label')));
}

function renderDaily(body, act, days, accent) {
    els.activity.title.textContent = s.t('card_history');
    const all = act.daily;
    const values = all.slice(-days);
    const [y, m, d] = (act.daily_start ?? '').split('-').map(Number);
    const firstMs = new Date(y, (m || 1) - 1, d || 1).getTime() + (all.length - values.length) * 86400000;
    const total = values.reduce((a, b) => a + b, 0);
    const setRight = i => {
        els.activity.right.textContent = i === null ? s.tokens(total)
            : `${s.dayLabel(firstMs + i * 86400000, true)} · ${s.tokens(values[i])}`;
    };
    setRight(null);
    if (!total) {
        body.append(dim(s.t('no_activity')));
        return;
    }
    body.append(barChart(values, {accent, highlight: 1, onHover: setRight}));
    const axis = h('div.kota-axis');
    if (days === 7)
        values.forEach((_v, i) => axis.append(h('span.kota-dim.kota-axis-label.kota-axis-day', {}, s.dayLabel(firstMs + i * 86400000))));
    else
        axis.append(dim(s.dayLabel(firstMs, true), 'kota-axis-label'), h('span.kota-grow'), dim(s.t('now'), 'kota-axis-label'));
    body.append(axis);
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
    body.append(dim(parts.join(' · '), cls));
}

// ---------------------------------------------------------------- ajanlar (ağaç)

/** Yorum dizisi tarzı girinti: `cont` düz çizgi, `elbow` kavisli dirsek. */
function gutter({cont = false, elbow = null, last = false}) {
    return h('div.kota-gutter', {class: [cont ? 'cont' : '', elbow !== null ? 'elbow' : '', last ? 'last' : ''].join(' '),
        style: elbow !== null ? {'--elbow': `${elbow}px`} : {}});
}

function treeRow(ancestors, last, elbow, content) {
    return h('div.kota-tree-row', {}, ancestors.map(cont => gutter({cont})), gutter({elbow, last}), content);
}

function nodeIcon(icon, stem) {
    return h('div.kota-node-col', {}, icon, stem ? h('div.kota-stem') : null);
}

function agentRow(a, focus) {
    const state = s.idleFor(a.active_at);
    const bits = [state, s.bytes(a.rss_bytes)];
    if (a.tokens)
        bits.push(`${s.tokens(a.tokens)} ${s.t('tokens_unit')}`);
    if (a.ports?.length)
        bits.push(`${a.ports.length} ${s.t('ports_unit')}`);
    const others = (a.focus ?? []).filter(f => f.path !== focus?.path).map(f => f.name);
    return h('div.kota-agent', {},
        h('div.kota-agent-icon', {}, providerIcon(agentIconId(a.kind), {size: 18, name: a.name})),
        h('div.kota-grow', {},
            h('div.kota-agent-name', {}, a.name),
            h('div.kota-dim', {class: state === s.t('working') ? 'kota-working' : ''}, bits.join(' · ')),
            others.length ? dim(`${s.t('also')}: ${others.join(' · ')}`, 'kota-path') : null,
            a.ports?.length ? h('div.kota-chips', {}, a.ports.slice(0, 6).map(port =>
                h('button.kota-port', {onclick: () => window.__TAURI__.opener.openUrl(`http://localhost:${port}`)}, `:${port}`))) : null),
        iconButton('stop', s.t('stop'), () => invoke('stop_agent', {pid: a.pid}), 'kota-stop'),
        iconButton('folder-open', s.t('open_folder'), () => invoke('open_path', {path: focus?.path ?? a.cwd}), 'kota-go'));
}

function focusRow(node, base) {
    const rel = node.focus.path.startsWith(`${base}\\`) || node.focus.path.startsWith(`${base}/`)
        ? node.focus.path.slice(base.length + 1) : node.focus.path;
    return h('div.kota-node', {},
        nodeIcon(h('div.kota-node-icon', {}, h('span.kota-glyph', {'data-icon': 'folder'})), true),
        h('div.kota-node-text', {}, h('span.kota-node-name', {}, node.focus.name),
            rel !== node.focus.name ? h('span.kota-dim.kota-path', {}, rel) : null, h('span.kota-grow'),
            node.focus.at ? h('span.kota-dim.kota-path', {}, s.ago(node.focus.at)) : null),
        iconButton('folder-open', s.t('open_folder'), () => invoke('open_path', {path: node.focus.path}), 'kota-node-open'));
}

function renderLive() {
    const list = els.liveList;
    list.replaceChildren();
    const agents = snapshot?.agents ?? [];
    const shown = filter ? agents.filter(a => `${a.name} ${a.kind} ${a.project} ${a.project_path} ${a.pid} ${(a.ports ?? []).join(' ')}`
        .toLowerCase().includes(filter)) : agents;
    els.live.title.textContent = `${s.t('card_live')}  ${agents.length}`;
    if (!agents.length) {
        list.append(dim(s.t('no_agents')));
        return;
    }
    if (!shown.length) {
        list.append(dim(s.t('no_match')));
        return;
    }
    const groups = new Map();
    for (const a of shown) {
        if (!groups.has(a.project_path))
            groups.set(a.project_path, {project: a.project, nodes: new Map(), loose: []});
        const g = groups.get(a.project_path);
        const top = a.focus?.[0];
        if (top) {
            if (!g.nodes.has(top.path))
                g.nodes.set(top.path, {focus: top, agents: []});
            g.nodes.get(top.path).agents.push(a);
        } else {
            g.loose.push(a);
        }
    }
    for (const [path, g] of groups) {
        const total = g.loose.length + [...g.nodes.values()].reduce((n, x) => n + x.agents.length, 0);
        const box = h('div.kota-group', {},
            h('div.kota-group-head', {},
                nodeIcon(h('div.kota-folder', {}, h('span.kota-glyph', {'data-icon': 'folder'})), true),
                h('div.kota-group-names', {}, h('span.kota-strong', {}, g.project), h('span.kota-dim.kota-path.kota-grow', {}, path),
                    h('span.kota-dim', {}, `${total} ${s.t('agents_unit')}`))));
        const children = [...[...g.nodes.values()].map(n => ({node: n})), ...g.loose.map(a => ({agent: a}))];
        children.forEach((c, i) => {
            const last = i === children.length - 1;
            if (c.agent) {
                box.append(treeRow([], last, 22, agentRow(c.agent, null)));
                return;
            }
            box.append(treeRow([], last, 15, focusRow(c.node, path)));
            c.node.agents.forEach((a, j) => box.append(treeRow([!last], j === c.node.agents.length - 1, 22, agentRow(a, c.node.focus))));
        });
        list.append(box);
    }
}

function renderFooter() {
    els.footer.textContent = snapshot ? `${s.t('updated_at')} ${s.when(parseTime(snapshot.generated_at))}` : '';
}

// ---------------------------------------------------------------- başlangıç

async function main() {
    settings = await Settings.load();
    s = makeStrings(settings);
    applyLook();
    snapshot = await invoke('get_snapshot');
    build();
    update();
    listen('snapshot', snap => {
        notify(previous, snap, settings, s);
        previous = snap;
        update(snap);
    });
    previous = snapshot;
    listen('popup-shown', () => update());
    settings.onChange(key => {
        if (key === 'language') {
            s = makeStrings(settings);
            build();
            update();
            return;
        }
        if (['popup-theme', 'popup-background', 'popup-opacity', 'accent-color', 'bar-colors'].includes(key))
            applyLook();
        if (key === 'popup-width')
            root.style.width = `${settings.get_int('popup-width')}px`;
        applyVisibility();
        update();
    });
    window.matchMedia('(prefers-color-scheme: light)').addEventListener('change', applyLook);
    // Göreli zamanlar ("3 sa 18 dk") için düzenli yeniden çizim.
    setInterval(() => {
        if (document.visibilityState === 'visible')
            update();
    }, 30000);
    document.addEventListener('keydown', e => {
        if (e.key === 'Escape')
            invoke('hide_popup');
    });
}

main().catch(e => {
    root.textContent = `Kota: ${e?.message ?? e}`;
});
