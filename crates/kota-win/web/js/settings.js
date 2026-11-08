// Kota — ayarlar penceresi (GNOME'daki prefs.js'in karşılığı): başlık,
// hemen altında sekmeler, arama; Sağlayıcılar · Görev çubuğu · Görünüm ·
// Bildirimler · Servis.

import {Settings, h, invoke, listen, makeStrings} from './api.js';

let settings, s;
const app = document.getElementById('settings');

// ---------------------------------------------------------------- bileşenler

function group(title, description = '') {
    const list = h('div.p-list');
    const el = h('section.p-group', {}, h('h2.p-group-title', {}, title), description ? h('p.p-group-desc', {}, description) : null, list);
    el.add = row => {
        list.append(row);
        return row;
    };
    return el;
}

function row(title, subtitle = '', ...suffix) {
    const sub = h('div.p-row-sub', {}, subtitle);
    const el = h('div.p-row', {'data-search': `${title} ${subtitle}`.toLocaleLowerCase()},
        h('div.p-row-text', {}, h('div.p-row-title', {}, title), sub), ...suffix);
    el.setSubtitle = t => {
        sub.textContent = t;
    };
    return el;
}

function toggle(checked, onChange) {
    const input = h('input', {type: 'checkbox', role: 'switch'});
    input.checked = Boolean(checked);
    input.addEventListener('change', () => onChange(input.checked));
    return h('label.p-switch', {}, input, h('span'));
}

function select(value, options, onChange) {
    const el = h('select.p-select', {onchange: e => onChange(e.target.value)},
        options.map(([v, label]) => h('option', {value: v}, label)));
    el.value = value;
    return el;
}

function spin(value, min, max, step, onChange) {
    const input = h('input.p-spin-input', {type: 'number', min, max, step, value});
    let timer = 0;
    const commit = v => {
        const n = Math.max(min, Math.min(max, Number(v) || min));
        input.value = n;
        clearTimeout(timer);
        timer = setTimeout(() => onChange(n), 500);
    };
    input.addEventListener('change', () => commit(input.value));
    return h('div.p-spin', {},
        input,
        h('button.p-round', {onclick: () => commit(Number(input.value) - step)}, '−'),
        h('button.p-round', {onclick: () => commit(Number(input.value) + step)}, '+'));
}

const setSwitch = (key, title, sub = '') => row(title, sub, toggle(settings.get_boolean(key), v => settings.set_boolean(key, v)));
const setSelect = (key, title, options, sub = '') => row(title, sub, select(settings.get_string(key), options, v => settings.set_string(key, v)));
const setSpin = (key, title, min, max, step, sub = '') => row(title, sub, spin(settings.get_int(key), min, max, step, v => settings.set_int(key, v)));

function expander(title, subtitle, prefix, suffix) {
    const body = h('div.p-expander-body', {hidden: true});
    const chev = h('span.p-chevron');
    const head = h('div.p-row.p-expander-head', {'data-search': `${title} ${subtitle}`.toLocaleLowerCase()},
        prefix ?? null, h('div.p-row-text', {}, h('div.p-row-title', {}, title), h('div.p-row-sub', {}, subtitle)), suffix ?? null, chev);
    const el = h('div.p-expander', {}, head, body);
    head.addEventListener('click', e => {
        if (e.target.closest('input, select, button, label'))
            return;
        el.toggle();
    });
    el.toggle = open => {
        body.hidden = open === undefined ? !body.hidden : !open;
        el.classList.toggle('open', !body.hidden);
    };
    el.add = r => body.append(r);
    el.setSubtitle = t => {
        head.querySelector('.p-row-sub').textContent = t;
    };
    return el;
}

const icon = (id, size = 28) => h('img.p-icon', {src: `icons/providers/${id}.svg`, width: size, height: size, alt: '', onerror: e => e.target.remove()});

function headline(p) {
    return (p?.windows ?? []).reduce((b, w) => (!b || w.used_percent > b.used_percent ? w : b), null);
}

// ---------------------------------------------------------------- sayfalar

async function accountsGroup(page) {
    const g = group(s.t('prefs_accounts'), s.t('win_accounts_sub'));
    page.append(g);
    const names = {claude: 'Claude', codex: 'Codex'};
    const rebuild = async () => {
        let data = {profiles: [], current: {}};
        try {
            data = await invoke('list_accounts');
        } catch (e) {
            g.querySelector('.p-group-desc').textContent = String(e);
        }
        g.querySelector('.p-list').replaceChildren();
        for (const id of ['claude', 'codex']) {
            const cur = data.current?.[id];
            const exp = expander(names[id], cur ? `${s.t('prefs_account_current')}: ${cur}` : '', icon(id, 24));
            const mine = (data.profiles ?? []).filter(p => p.provider === id);
            if (!mine.length)
                exp.add(row(s.t('prefs_account_none')));
            for (const p of mine) {
                const r = row(p.name, '');
                if (p.active) {
                    r.append(h('span.p-dim', {}, s.t('prefs_account_active')));
                } else {
                    r.append(h('button.p-btn.p-suggested', {onclick: () => invoke('switch_account', {provider: id, name: p.name})
                        .then(rebuild).catch(e => r.setSubtitle(String(e)))}, s.t('prefs_account_use')));
                }
                r.append(h('button.p-btn.p-flat', {title: s.t('prefs_key_clear'), onclick: () => invoke('remove_account', {provider: id, name: p.name})
                    .then(rebuild).catch(e => r.setSubtitle(String(e)))}, '🗑'));
                exp.add(r);
            }
            const input = h('input.p-entry', {placeholder: s.t('prefs_account_save_sub')});
            const save = row(s.t('prefs_account_save'), '', input, h('button.p-btn', {onclick: () => invoke('save_account', {provider: id, name: input.value.trim()})
                .then(rebuild).catch(e => save.setSubtitle(String(e)))}, '✓'));
            exp.add(save);
            g.add(exp);
        }
    };
    await rebuild();
}

async function providersPage(page) {
    await accountsGroup(page);
    const catalog = await invoke('list_providers');
    let snapshot = await invoke('get_snapshot');
    const g = group(s.t('prefs_providers_page'), s.t('win_providers_desc'));
    page.append(g);
    const enabledIds = () => catalog.filter(c => c.enabled).map(c => c.id);
    for (const entry of catalog) {
        const status = () => {
            const r = snapshot?.providers?.find(p => p.id === entry.id);
            if (!entry.enabled || !r)
                return s.shows(entry.shows);
            const w = headline(r);
            const val = w ? ` · ${s.percent(w.used_percent)}` : r.balance ? ` · ${s.money(r.balance.amount, r.balance.currency)}` : '';
            return `${s.status(r)}${val}`;
        };
        const sw = toggle(entry.enabled, async v => {
            entry.enabled = v;
            exp.setSubtitle(status());
            try {
                await invoke('set_provider_enabled', {id: entry.id, enabled: v});
                snapshot = await invoke('get_snapshot');
                exp.setSubtitle(status());
            } catch (e) {
                exp.setSubtitle(String(e));
            }
        });
        const exp = expander(entry.name, status(), icon(entry.id), sw);
        if (entry.auth === 'cli_login') {
            const r = row(s.t('prefs_login'), s.t('prefs_login_sub'));
            r.append(h('button.p-btn', {onclick: () => invoke('connect', {id: entry.id}).catch(e => r.setSubtitle(String(e)))}, s.t('connect')));
            exp.add(r);
        } else {
            const title = entry.auth === 'admin_key' ? s.t('prefs_admin_key') : entry.auth === 'github_token' ? s.t('prefs_github_token') : s.t('prefs_api_key');
            const input = h('input.p-entry', {type: 'password', placeholder: title, autocomplete: 'off'});
            const state = row(entry.has_key ? s.t('prefs_key_saved') : s.t('prefs_key_missing'), '');
            const apply = async () => {
                const key = input.value.trim();
                if (!key)
                    return;
                try {
                    await invoke('set_api_key', {id: entry.id, key});
                    input.value = '';
                    state.querySelector('.p-row-title').textContent = s.t('prefs_key_saved');
                    if (!entry.enabled)
                        sw.querySelector('input').click();
                } catch (e) {
                    state.setSubtitle(String(e));
                }
            };
            input.addEventListener('keydown', e => e.key === 'Enter' && apply());
            exp.add(row(title, '', input, h('button.p-btn', {onclick: apply}, '✓')));
            if (entry.key_url)
                state.append(h('button.p-btn.p-flat', {onclick: () => window.__TAURI__.opener.openUrl(entry.key_url)}, s.t('prefs_key_get')));
            state.append(h('button.p-btn.p-flat', {title: s.t('prefs_key_clear'), onclick: () => invoke('set_api_key', {id: entry.id, key: ''})
                .then(() => {
                    state.querySelector('.p-row-title').textContent = s.t('prefs_key_missing');
                })}, '🗑'));
            exp.add(state);
        }
        if (entry.budget_supported)
            exp.add(row(s.t('prefs_budget'), s.t('prefs_budget_sub'), spin(entry.budget ?? 0, 0, 1000000, 5, v => invoke('set_budget', {id: entry.id, amount: v}))));
        const sel = settings.get_strv('panel-providers');
        exp.add(row(s.t('win_in_tray'), '', toggle(sel.length ? sel.includes(entry.id) : true, v => {
            let list = settings.get_strv('panel-providers');
            if (!list.length)
                list = enabledIds();
            list = list.filter(x => x !== entry.id);
            if (v)
                list.push(entry.id);
            settings.set_strv('panel-providers', list.length ? list : ['none']);
        })));
        const windows = snapshot?.providers?.find(p => p.id === entry.id)?.windows ?? [];
        if (windows.length > 1) {
            const options = [['auto', s.t('follow_global')], ['all', s.t('pw_all')], ['highest', s.t('pw_auto')], ...windows.map(w => [w.id, s.windowLabel(w)])];
            const cur = settings.get_value('panel-windows').deepUnpack()[entry.id] ?? 'auto';
            exp.add(row(s.t('prefs_panel_window'), '', select(cur, options, v => {
                const map = {...settings.get_value('panel-windows').deepUnpack(), [entry.id]: v};
                settings.set('panel-windows', map);
            })));
        }
        g.add(exp);
    }
}

function taskbarPage(page) {
    const tray = group(s.t('win_tray_group'), s.t('win_tray_group_sub'));
    tray.add(setSelect('tray-style', s.t('win_tray_style'), [['ring', s.t('win_tray_ring')], ['kota', s.t('win_tray_kota')], ['logo', s.t('win_tray_logo')]]));
    tray.add(setSelect('panel-window-mode', s.t('prefs_panel_windows'), [['all', s.t('pw_all')], ['auto', s.t('pw_auto')]], s.t('prefs_panel_windows_sub')));
    tray.add(setSwitch('panel-letters', s.t('prefs_letters'), s.t('win_letters_sub')));
    tray.add(setSwitch('show-remaining', s.t('prefs_remaining'), s.t('prefs_remaining_sub')));
    page.append(tray);
    const start = group(s.t('win_startup'), s.t('win_tray_hint'));
    start.add(row(s.t('win_autostart'), s.t('win_autostart_sub'), toggle(settings.get('autostart') !== false, v => settings.set('autostart', v))));
    page.append(start);
}

function appearancePage(page) {
    const menu = group(s.t('prefs_menu'));
    menu.add(setSelect('popup-theme', s.t('prefs_theme'), [['auto', s.t('theme_auto')], ['dark', s.t('theme_dark')], ['light', s.t('theme_light')]]));
    menu.add(setSelect('bar-colors', s.t('prefs_bar_colors'), [['usage', s.t('bars_usage')], ['brand', s.t('bars_brand')]], s.t('prefs_bar_colors_sub')));
    menu.add(setSelect('popup-background', s.t('prefs_background'), [['auto', s.t('win_bg_acrylic')], ['opaque', s.t('bg_opaque')]], s.t('win_bg_sub')));
    menu.add(setSpin('popup-opacity', s.t('prefs_opacity'), 0, 100, 5));
    menu.add(setSelect('usage-style', s.t('prefs_usage_style'), [['rings', s.t('style_rings')], ['bars', s.t('style_bars')]]));
    menu.add(setSwitch('show-pace', s.t('prefs_pace'), s.t('prefs_pace_sub')));
    menu.add(setSpin('popup-width', s.t('prefs_width'), 340, 640, 10));
    menu.add(setSpin('usage-columns', s.t('prefs_columns'), 1, 3, 1));
    const color = h('input.p-color', {type: 'color', value: settings.get_string('accent-color') || '#f5a623', onchange: e => settings.set_string('accent-color', e.target.value)});
    menu.add(row(s.t('prefs_accent'), '', color));
    page.append(menu);
    const sec = group(s.t('prefs_sections'));
    for (const [key, text] of [['show-tiles', 'sec_tiles'], ['show-usage', 'sec_usage'], ['show-activity', 'sec_activity'], ['show-agents', 'sec_agents'], ['show-filter', 'sec_filter']])
        sec.add(setSwitch(key, s.t(text)));
    page.append(sec);
}

async function notifyPage(page) {
    const g = group(s.t('prefs_notify_page'));
    g.add(setSwitch('notifications', s.t('prefs_notify'), s.t('prefs_notify_sub')));
    g.add(setSwitch('notify-reset', s.t('prefs_notify_reset'), s.t('prefs_notify_reset_sub')));
    g.add(setSwitch('notify-expired', s.t('prefs_notify_expired'), s.t('prefs_notify_expired_sub')));
    g.add(setSwitch('notify-forecast', s.t('prefs_notify_forecast'), s.t('prefs_notify_forecast_sub')));
    g.add(setSpin('notify-forecast-minutes', s.t('prefs_forecast_minutes'), 10, 300, 10));
    page.append(g);
    const svc = await invoke('get_settings');
    const th = group(s.t('prefs_thresholds'));
    for (const [key, title] of [['warning', 'prefs_warning'], ['critical', 'prefs_critical']])
        th.add(row(s.t(title), '', spin(svc[key], 1, 100, 1, v => invoke('set_settings', {patch: {[key]: v}}))));
    page.append(th);
}

async function servicePage(page) {
    const svc = await invoke('get_settings');
    const set = patch => invoke('set_settings', {patch});
    const g = group(s.t('prefs_polling'));
    g.add(row(s.t('prefs_interval'), '', spin(svc.interval_seconds, 30, 3600, 10, v => set({interval_seconds: v}))));
    for (const [key, title, sub] of [['paused', 'prefs_paused', 'prefs_paused_sub'], ['claude_refresh_tokens', 'prefs_claude_refresh', 'prefs_claude_refresh_sub'],
        ['local_logs', 'prefs_local_logs', 'prefs_local_logs_sub']])
        g.add(row(s.t(title), s.t(sub), toggle(svc[key], v => set({[key]: v}))));
    const cfg = row(s.t('prefs_open_config'), svc.config_path, h('button.p-btn.p-flat', {onclick: () => invoke('open_path', {path: svc.config_path})}, '↗'));
    g.add(cfg);
    page.append(g);
    const gen = group(s.t('prefs_general'));
    gen.add(row(s.t('prefs_language'), '', select(settings.get_string('language'), [['auto', s.t('lang_auto')], ['tr', 'Türkçe'], ['en', 'English']], async v => {
        await settings.set_string('language', v);
        location.reload();
    })));
    page.append(gen);
    const about = group(s.t('prefs_about'));
    about.add(row('Kota', `${await invoke('app_version')} · https://github.com/ensignalp/kota`,
        h('button.p-btn.p-flat', {onclick: () => window.__TAURI__.opener.openUrl('https://github.com/ensignalp/kota')}, '↗')));
    page.append(about);
}

// ---------------------------------------------------------------- iskelet + arama

async function main() {
    settings = await Settings.load();
    s = makeStrings(settings);
    const theme = settings.get_string('popup-theme');
    const light = theme === 'light' || (theme === 'auto' && matchMedia('(prefers-color-scheme: light)').matches);
    document.documentElement.classList.toggle('kota-light', light);
    document.title = `Kota — ${s.t('settings')}`;
    const pages = [
        ['providers', s.t('prefs_providers_page'), 'grid', providersPage],
        ['taskbar', s.t('win_taskbar_page'), 'taskbar', taskbarPage],
        ['appearance', s.t('prefs_appearance_page'), 'palette', appearancePage],
        ['notify', s.t('prefs_notify_page'), 'bell', notifyPage],
        ['service', s.t('prefs_service_page'), 'gear', servicePage],
    ];
    const tabs = h('nav.p-tabs');
    const content = h('div.p-content');
    const results = h('div.p-page', {hidden: true});
    const pageEls = {};
    const show = id => {
        for (const [pid, el] of Object.entries(pageEls))
            el.hidden = pid !== id;
        tabs.querySelectorAll('.p-tab').forEach(t => t.classList.toggle('active', t.dataset.page === id));
        results.hidden = true;
        content.scrollTop = 0;
    };
    for (const [id, title, ic] of pages) {
        tabs.append(h('button.p-tab', {'data-page': id, onclick: () => show(id)}, h('span.p-tab-icon', {'data-icon': ic}), title));
        pageEls[id] = h('div.p-page', {hidden: true});
        content.append(pageEls[id]);
    }
    content.append(results);
    const search = h('input.p-search', {type: 'search', placeholder: s.t('prefs_search')});
    const searchBar = h('div.p-searchbar', {hidden: true}, search);
    const header = h('header.p-header', {},
        h('button.p-icon-btn', {title: s.t('prefs_search'), onclick: () => {
            searchBar.hidden = !searchBar.hidden;
            if (!searchBar.hidden)
                search.focus();
            else {
                search.value = '';
                runSearch();
            }
        }}, h('span.p-tab-icon', {'data-icon': 'search'})),
        h('div.p-title', {}, h('div.p-title-main', {}, 'Kota'), h('div.p-title-sub', {}, await invoke('app_version'))),
        h('span.p-header-spacer'));
    app.append(header, tabs, searchBar, content);
    const runSearch = () => {
        const q = search.value.trim().toLocaleLowerCase();
        results.replaceChildren();
        if (!q) {
            show(tabs.querySelector('.p-tab.active')?.dataset.page ?? 'providers');
            tabs.classList.remove('dim');
            return;
        }
        tabs.classList.add('dim');
        Object.values(pageEls).forEach(el => {
            el.hidden = true;
        });
        results.hidden = false;
        const list = h('div.p-list');
        for (const [id, title] of pages) {
            pageEls[id].querySelectorAll('[data-search]').forEach(r => {
                if (!r.dataset.search.includes(q))
                    return;
                const name = r.querySelector('.p-row-title')?.textContent ?? '';
                const grp = r.closest('.p-group')?.querySelector('.p-group-title')?.textContent ?? '';
                list.append(h('button.p-row.p-result', {onclick: () => {
                    search.value = '';
                    searchBar.hidden = true;
                    tabs.classList.remove('dim');
                    show(id);
                    r.closest('.p-expander')?.toggle(true);
                    r.scrollIntoView({block: 'center'});
                    r.classList.add('flash');
                    setTimeout(() => r.classList.remove('flash'), 1200);
                }}, h('div.p-row-text', {}, h('div.p-row-title', {}, name), h('div.p-row-sub', {}, [title, grp].filter(Boolean).join(' › '))), h('span.p-chevron.right')));
            });
        }
        results.append(list.children.length ? h('section.p-group', {}, list) : h('p.p-empty', {}, s.t('prefs_search_empty')));
    };
    search.addEventListener('input', runSearch);
    document.addEventListener('keydown', e => {
        if ((e.ctrlKey && e.key.toLowerCase() === 'f') || (e.key.length === 1 && searchBar.hidden && !e.ctrlKey && !e.altKey &&
            !['INPUT', 'SELECT', 'TEXTAREA'].includes(document.activeElement?.tagName))) {
            searchBar.hidden = false;
            search.focus();
        } else if (e.key === 'Escape' && !searchBar.hidden) {
            search.value = '';
            searchBar.hidden = true;
            runSearch();
        }
    });
    show('providers');
    for (const [id, , , fill] of pages) {
        try {
            await fill(pageEls[id]);
        } catch (e) {
            pageEls[id].append(h('p.p-empty', {}, `${s.t('prefs_service_down')}\n${e}`));
        }
    }
    listen('ui-changed', ({key}) => {
        if (key === 'popup-theme')
            location.reload();
    });
}

main().catch(e => {
    app.textContent = String(e?.message ?? e);
});
