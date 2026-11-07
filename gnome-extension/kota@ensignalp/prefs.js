// Kota — tercihler penceresi.
//
// Sağlayıcı ayarları (aç/kapat, API anahtarı, bütçe) ve servis ayarları
// (aralık, eşikler, duraklatma) D-Bus üzerinden servise yazılır; servis
// bunları ~/.config/kota/config.toml'a ve GNOME Anahtarlığı'na kaydeder.
// Görünüm ayarları GSettings'te.

import Adw from 'gi://Adw';
import Gdk from 'gi://Gdk?version=4.0';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Gtk from 'gi://Gtk?version=4.0';

import {ExtensionPreferences} from 'resource:///org/gnome/Shell/Extensions/js/extensions/prefs.js';

import {newProxy} from './lib/client.js';
import {Strings, resolveLanguage} from './lib/i18n.js';

function comboRow(settings, key, title, options, subtitle = null) {
    const row = new Adw.ComboRow({title, model: Gtk.StringList.new(options.map(o => o[1]))});
    if (subtitle)
        row.subtitle = subtitle;
    const sync = () => {
        const i = options.findIndex(o => o[0] === settings.get_string(key));
        row.selected = i < 0 ? 0 : i;
    };
    sync();
    row.connect('notify::selected', () => {
        const value = options[row.selected]?.[0];
        if (value && value !== settings.get_string(key))
            settings.set_string(key, value);
    });
    const id = settings.connect(`changed::${key}`, sync);
    row.connect('destroy', () => settings.disconnect(id));
    return row;
}

function switchRow(settings, key, title, subtitle = null) {
    const row = new Adw.SwitchRow({title});
    if (subtitle)
        row.subtitle = subtitle;
    settings.bind(key, row, 'active', Gio.SettingsBindFlags.DEFAULT);
    return row;
}

function intSpinRow(settings, key, title, min, max, step) {
    const row = Adw.SpinRow.new_with_range(min, max, step);
    row.title = title;
    settings.bind(key, row, 'value', Gio.SettingsBindFlags.DEFAULT);
    return row;
}

/** Son değişiklikten `ms` sonra çalışan geciktirici. */
function debounce(ms, fn) {
    let id = 0;
    return (...args) => {
        if (id)
            GLib.source_remove(id);
        id = GLib.timeout_add(GLib.PRIORITY_DEFAULT, ms, () => {
            id = 0;
            fn(...args);
            return GLib.SOURCE_REMOVE;
        });
    };
}

function openUri(window, uri) {
    new Gtk.UriLauncher({uri}).launch(window, null, null);
}

/** Sayfadaki ayar satırları (arama için): satır, grubu ve varsa açılır üst satırı. */
function collectRows(page) {
    const out = [];
    const walk = (w, group, expander) => {
        for (let c = w.get_first_child(); c; c = c.get_next_sibling()) {
            let g = group, e = expander;
            if (c instanceof Adw.PreferencesGroup)
                g = c;
            if (c instanceof Adw.PreferencesRow && c.title) {
                out.push({row: c, group: g, expander: e});
                if (c instanceof Adw.ExpanderRow)
                    e = c;
            }
            walk(c, g, e);
        }
    };
    walk(page, null, null);
    return out;
}

function headline(p) {
    let best = null;
    for (const w of p?.windows ?? []) {
        if (!best || w.used_percent > best.used_percent)
            best = w;
    }
    return best;
}

export default class KotaPreferences extends ExtensionPreferences {
    fillPreferencesWindow(window) {
        const settings = this.getSettings();
        const s = new Strings(resolveLanguage(settings.get_string('language')));
        window._settings = settings;
        window.set_default_size(780, 860);

        const providersPage = new Adw.PreferencesPage({title: s.t('prefs_providers_page'), icon_name: 'view-grid-symbolic'});
        const placementPage = this._placementPage(settings, s);
        const appearancePage = this._appearancePage(settings, s);
        const notifyPage = new Adw.PreferencesPage({title: s.t('prefs_notify_page'), icon_name: 'preferences-system-notifications-symbolic'});
        const servicePage = new Adw.PreferencesPage({title: s.t('prefs_service_page'), icon_name: 'emblem-system-symbolic'});
        this._tabsOnTop(window, [providersPage, placementPage, appearancePage, notifyPage, servicePage], s);

        const notify = new Adw.PreferencesGroup({title: s.t('prefs_notify_page')});
        notify.add(switchRow(settings, 'notifications', s.t('prefs_notify'), s.t('prefs_notify_sub')));
        notify.add(switchRow(settings, 'notify-reset', s.t('prefs_notify_reset'), s.t('prefs_notify_reset_sub')));
        notify.add(switchRow(settings, 'notify-expired', s.t('prefs_notify_expired'), s.t('prefs_notify_expired_sub')));
        notify.add(switchRow(settings, 'notify-forecast', s.t('prefs_notify_forecast'), s.t('prefs_notify_forecast_sub')));
        const fcMin = intSpinRow(settings, 'notify-forecast-minutes', s.t('prefs_forecast_minutes'), 10, 300, 10);
        settings.bind('notify-forecast', fcMin, 'sensitive', Gio.SettingsBindFlags.GET);
        notify.add(fcMin);
        notifyPage.add(notify);

        const general = new Adw.PreferencesGroup({title: s.t('prefs_general')});
        general.add(comboRow(settings, 'language', s.t('prefs_language'),
            [['auto', s.t('lang_auto')], ['tr', 'Türkçe'], ['en', 'English']]));
        const shortcut = new Adw.EntryRow({title: s.t('prefs_shortcut'), show_apply_button: true,
            text: settings.get_strv('toggle-shortcut')[0] ?? ''});
        shortcut.connect('apply', () => {
            const v = shortcut.text.trim();
            if (!v || Gtk.accelerator_parse(v)[0])
                settings.set_strv('toggle-shortcut', v ? [v] : []);
        });
        general.add(shortcut);
        general.description = GLib.markup_escape_text(s.t('prefs_shortcut_sub'), -1);

        const loading = new Adw.PreferencesGroup({title: s.t('prefs_providers_page'), description: s.t('loading')});
        providersPage.add(loading);

        // Servis gerektiren bölümler: proxy hazır olunca doldurulur.
        newProxy().then(async proxy => {
            const [listJson] = await proxy.ListProvidersAsync();
            const catalog = JSON.parse(listJson);
            let snapshot = null;
            try {
                snapshot = JSON.parse((await proxy.GetSnapshotAsync())[0]);
            } catch {}
            const svc = JSON.parse((await proxy.GetSettingsAsync())[0]);
            providersPage.remove(loading);
            this._accounts(providersPage, proxy, s);
            this._providersPage(window, providersPage, proxy, settings, s, catalog, snapshot);
            this._thresholds(notifyPage, proxy, s, svc);
            this._service(window, servicePage, proxy, s, svc, general);
        }).catch(e => {
            loading.description = `${s.t('prefs_service_down')}\n\n${e.message}`;
            servicePage.add(general);
        });
    }

    /**
     * Başlık çubuğu ("Kota" + arama düğmesi), hemen altında sabit sekme
     * çubuğu ve arama kutusu. Adw.PreferencesWindow sekmeleri dar pencerede
     * alta taşıdığı için kendi düzenimizi kullanırız; arama da burada.
     */
    _tabsOnTop(window, pages, s) {
        const stack = new Adw.ViewStack({vexpand: true, hexpand: true});
        pages.forEach((page, i) => stack.add_titled_with_icon(page, `page${i}`, page.title, page.icon_name));

        const header = new Adw.HeaderBar({title_widget: new Adw.WindowTitle({title: 'Kota', subtitle: this.metadata['version-name'] ?? ''})});
        const searchBtn = new Gtk.ToggleButton({icon_name: 'system-search-symbolic', tooltip_text: s.t('prefs_search')});
        header.pack_start(searchBtn);

        const switcher = new Adw.ViewSwitcher({stack, policy: Adw.ViewSwitcherPolicy.WIDE, halign: Gtk.Align.CENTER});
        const tabs = new Gtk.Box({css_classes: ['kota-tabs'], margin_top: 6, margin_bottom: 6, halign: Gtk.Align.CENTER});
        tabs.append(switcher);

        const entry = new Gtk.SearchEntry({placeholder_text: s.t('prefs_search'), hexpand: true, max_width_chars: 40});
        const searchBar = new Gtk.SearchBar({child: entry, show_close_button: false});
        searchBar.connect_entry(entry);
        searchBar.set_key_capture_widget(window);
        searchBtn.bind_property('active', searchBar, 'search-mode-enabled', 1 | 2); // BIDIRECTIONAL | SYNC_CREATE

        const results = new Adw.PreferencesPage();
        const resultsGroup = new Adw.PreferencesGroup();
        results.add(resultsGroup);
        const outer = new Gtk.Stack({transition_type: Gtk.StackTransitionType.CROSSFADE});
        outer.add_named(stack, 'pages');
        outer.add_named(results, 'search');

        let shown = [];
        const clearResults = () => {
            for (const r of shown)
                resultsGroup.remove(r);
            shown = [];
        };
        const runSearch = () => {
            const q = entry.text.trim().toLocaleLowerCase();
            clearResults();
            if (!q) {
                outer.visible_child_name = 'pages';
                tabs.sensitive = true;
                return;
            }
            outer.visible_child_name = 'search';
            tabs.sensitive = false;
            for (const page of pages) {
                for (const {row, group, expander} of collectRows(page)) {
                    const text = `${row.title ?? ''} ${row.subtitle ?? ''}`.toLocaleLowerCase();
                    if (!text.includes(q))
                        continue;
                    const where = [page.title, expander?.title, group?.title].filter(Boolean).join(' › ');
                    const r = new Adw.ActionRow({title: row.title, subtitle: where, activatable: true, use_markup: false});
                    r.add_suffix(new Gtk.Image({icon_name: 'go-next-symbolic'}));
                    r.connect('activated', () => {
                        searchBtn.active = false;
                        entry.text = '';
                        stack.visible_child = page;
                        if (expander)
                            expander.expanded = true;
                        GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
                            row.grab_focus();
                            return GLib.SOURCE_REMOVE;
                        });
                    });
                    resultsGroup.add(r);
                    shown.push(r);
                    if (shown.length >= 40)
                        break;
                }
            }
            if (!shown.length) {
                const r = new Adw.ActionRow({title: s.t('prefs_search_empty')});
                resultsGroup.add(r);
                shown.push(r);
            }
        };
        entry.connect('changed', debounce(150, runSearch));
        searchBar.connect('notify::search-mode-enabled', () => {
            if (!searchBar.search_mode_enabled) {
                entry.text = '';
                runSearch();
            }
        });

        const toolbar = new Adw.ToolbarView({content: outer, top_bar_style: Adw.ToolbarStyle.RAISED});
        toolbar.add_top_bar(header);
        toolbar.add_top_bar(tabs);
        toolbar.add_top_bar(searchBar);
        window.set_content(toolbar);
    }

    // ------------------------------------------------------------ hesaplar

    _accounts(page, proxy, s) {
        const group = new Adw.PreferencesGroup({title: s.t('prefs_accounts'), description: s.t('prefs_accounts_sub')});
        page.add(group);
        const rows = [];
        const names = {claude: 'Claude', codex: 'Codex'};
        const rebuild = async () => {
            let data = {profiles: [], current: {}};
            try {
                data = JSON.parse((await proxy.ListAccountsAsync())[0]);
            } catch (e) {
                group.description = e.message;
            }
            for (const r of rows.splice(0))
                group.remove(r);
            for (const id of ['claude', 'codex']) {
                const exp = new Adw.ExpanderRow({title: names[id],
                    subtitle: data.current?.[id] ? `${s.t('prefs_account_current')}: ${data.current[id]}` : ''});
                const iconFile = GLib.build_filenamev([this.path, 'icons', 'providers', `${id}.svg`]);
                const img = Gtk.Image.new_from_file(iconFile);
                img.pixel_size = 24;
                exp.add_prefix(img);
                const mine = (data.profiles ?? []).filter(p => p.provider === id);
                if (!mine.length)
                    exp.add_row(new Adw.ActionRow({title: s.t('prefs_account_none')}));
                for (const p of mine) {
                    const r = new Adw.ActionRow({title: p.name, use_markup: false});
                    if (p.active) {
                        const l = new Gtk.Label({label: s.t('prefs_account_active'), css_classes: ['dim-label']});
                        r.add_suffix(l);
                    } else {
                        const use = new Gtk.Button({label: s.t('prefs_account_use'), valign: Gtk.Align.CENTER, css_classes: ['suggested-action']});
                        use.connect('clicked', () => proxy.SwitchAccountAsync(id, p.name).then(rebuild).catch(e => {
                            r.subtitle = e.message;
                        }));
                        r.add_suffix(use);
                    }
                    const del = new Gtk.Button({icon_name: 'user-trash-symbolic', valign: Gtk.Align.CENTER, css_classes: ['flat']});
                    del.connect('clicked', () => proxy.RemoveAccountAsync(id, p.name).then(rebuild).catch(e => {
                        r.subtitle = e.message;
                    }));
                    r.add_suffix(del);
                    exp.add_row(r);
                }
                const save = new Adw.EntryRow({title: `${s.t('prefs_account_save')} — ${s.t('prefs_account_save_sub')}`, show_apply_button: true});
                save.connect('apply', () => proxy.SaveAccountAsync(id, save.text.trim()).then(() => {
                    save.text = '';
                    return rebuild();
                }).catch(e => {
                    save.title = e.message;
                }));
                // EntryRow boşken "uygula" görünmez; boş ad = e-posta için düğme.
                const quick = new Gtk.Button({icon_name: 'document-save-symbolic', valign: Gtk.Align.CENTER,
                    css_classes: ['flat'], tooltip_text: s.t('prefs_account_save')});
                quick.connect('clicked', () => save.emit('apply'));
                save.add_suffix(quick);
                exp.add_row(save);
                group.add(exp);
                rows.push(exp);
            }
        };
        rebuild();
    }

    // ------------------------------------------------------------ sağlayıcılar

    _providersPage(window, page, proxy, settings, s, catalog, snapshot) {
        const enabledIds = () => catalog.filter(c => c.enabled).map(c => c.id);
        const group = new Adw.PreferencesGroup({title: s.t('prefs_providers_page'), description: s.t('prefs_providers_desc')});
        page.add(group);

        for (const entry of catalog) {
            const report = snapshot?.providers?.find(p => p.id === entry.id);
            const row = new Adw.ExpanderRow({title: entry.name});
            const statusText = () => {
                if (!entry.enabled)
                    return s.shows(entry.shows);
                const r = snapshot?.providers?.find(p => p.id === entry.id);
                if (!r)
                    return s.shows(entry.shows);
                const w = headline(r);
                const val = w ? ` · ${s.percent(w.used_percent)}` : r.balance ? ` · ${s.money(r.balance.amount, r.balance.currency)}` : '';
                return `${s.status(r)}${val}`;
            };
            row.subtitle = statusText();
            const iconFile = GLib.build_filenamev([this.path, 'icons', 'providers', `${entry.id}.svg`]);
            if (GLib.file_test(iconFile, GLib.FileTest.EXISTS)) {
                const img = Gtk.Image.new_from_file(iconFile);
                img.pixel_size = 28;
                row.add_prefix(img);
            }
            const toggle = new Gtk.Switch({active: entry.enabled, valign: Gtk.Align.CENTER});
            toggle.connect('notify::active', () => {
                entry.enabled = toggle.active;
                row.subtitle = statusText();
                proxy.SetProviderEnabledAsync(entry.id, toggle.active)
                    .then(() => proxy.GetSnapshotAsync())
                    .then(([json]) => {
                        snapshot = JSON.parse(json);
                        row.subtitle = statusText();
                    })
                    .catch(e => {
                        row.subtitle = e.message;
                    });
            });
            row.add_suffix(toggle);

            // Giriş / anahtar
            if (entry.auth === 'cli_login') {
                const login = new Adw.ActionRow({title: s.t('prefs_login'), subtitle: s.t('prefs_login_sub')});
                const btn = new Gtk.Button({label: s.t('connect'), valign: Gtk.Align.CENTER});
                btn.connect('clicked', () => proxy.ConnectAsync(entry.id).catch(e => {
                    login.subtitle = e.message;
                }));
                login.add_suffix(btn);
                row.add_row(login);
            } else {
                const title = entry.auth === 'admin_key' ? s.t('prefs_admin_key')
                    : entry.auth === 'github_token' ? s.t('prefs_github_token') : s.t('prefs_api_key');
                const keyRow = new Adw.PasswordEntryRow({title, show_apply_button: true});
                const state = new Adw.ActionRow({title: entry.has_key ? s.t('prefs_key_saved') : s.t('prefs_key_missing')});
                keyRow.connect('apply', () => {
                    const key = keyRow.text.trim();
                    if (!key)
                        return;
                    proxy.SetApiKeyAsync(entry.id, key).then(() => {
                        keyRow.text = '';
                        entry.has_key = true;
                        state.title = s.t('prefs_key_saved');
                        if (!toggle.active)
                            toggle.active = true;
                    }).catch(e => {
                        state.title = e.message;
                    });
                });
                row.add_row(keyRow);
                if (entry.key_url) {
                    const get = new Gtk.Button({label: s.t('prefs_key_get'), valign: Gtk.Align.CENTER});
                    get.add_css_class('flat');
                    get.connect('clicked', () => openUri(window, entry.key_url));
                    state.add_suffix(get);
                }
                const clear = new Gtk.Button({icon_name: 'user-trash-symbolic', valign: Gtk.Align.CENTER,
                    tooltip_text: s.t('prefs_key_clear')});
                clear.add_css_class('flat');
                clear.connect('clicked', () => proxy.SetApiKeyAsync(entry.id, '').then(() => {
                    entry.has_key = false;
                    state.title = s.t('prefs_key_missing');
                }).catch(e => {
                    state.title = e.message;
                }));
                state.add_suffix(clear);
                row.add_row(state);
            }

            if (entry.budget_supported) {
                const budget = Adw.SpinRow.new_with_range(0, 1000000, 5);
                budget.title = s.t('prefs_budget');
                budget.subtitle = s.t('prefs_budget_sub');
                budget.value = entry.budget ?? 0;
                const save = debounce(800, v => proxy.SetBudgetAsync(entry.id, v).catch(() => {}));
                budget.connect('notify::value', () => save(budget.value));
                row.add_row(budget);
            }

            // Panel seçimi
            const inPanel = new Adw.SwitchRow({title: s.t('prefs_in_panel')});
            const sel = settings.get_strv('panel-providers');
            inPanel.active = sel.length ? sel.includes(entry.id) : true;
            inPanel.connect('notify::active', () => {
                let list = settings.get_strv('panel-providers');
                if (!list.length)
                    list = enabledIds();
                list = list.filter(x => x !== entry.id);
                if (inPanel.active)
                    list.push(entry.id);
                settings.set_strv('panel-providers', list.length ? list : ['none']);
            });
            row.add_row(inPanel);

            const windows = report?.windows ?? [];
            if (windows.length > 1) {
                const options = [['auto', s.t('follow_global')], ['all', s.t('pw_all')], ['highest', s.t('pw_auto')],
                    ...windows.map(w => [w.id, s.windowLabel(w)])];
                const combo = new Adw.ComboRow({title: s.t('prefs_panel_window'),
                    model: Gtk.StringList.new(options.map(o => o[1]))});
                const current = settings.get_value('panel-windows').deepUnpack()[entry.id] ?? 'auto';
                combo.selected = Math.max(0, options.findIndex(o => o[0] === current));
                combo.connect('notify::selected', () => {
                    const map = settings.get_value('panel-windows').deepUnpack();
                    map[entry.id] = options[combo.selected][0];
                    settings.set_value('panel-windows', new GLib.Variant('a{ss}', map));
                });
                row.add_row(combo);
            }
            group.add(row);
        }
    }

    // ------------------------------------------------------------ yerleşim

    _placementPage(settings, s) {
        const page = new Adw.PreferencesPage({title: s.t('prefs_placement_page'), icon_name: 'view-dual-symbolic'});

        const panel = new Adw.PreferencesGroup({title: s.t('prefs_place_panel'), description: s.t('prefs_place_panel_sub')});
        panel.add(switchRow(settings, 'panel-enabled', s.t('prefs_place_panel')));
        const pos = comboRow(settings, 'panel-position', s.t('prefs_position'),
            [['left', s.t('pos_left')], ['center', s.t('pos_center')], ['right', s.t('pos_right')]]);
        const index = intSpinRow(settings, 'panel-index', s.t('prefs_panel_index'), -1, 20, 1);
        index.subtitle = s.t('prefs_panel_index_sub');
        for (const r of [pos, index]) {
            settings.bind('panel-enabled', r, 'sensitive', Gio.SettingsBindFlags.GET);
            panel.add(r);
        }
        page.add(panel);

        const qs = new Adw.PreferencesGroup({title: s.t('prefs_place_qs'), description: s.t('prefs_place_qs_desc')});
        qs.add(switchRow(settings, 'qs-enabled', s.t('prefs_qs_enabled')));
        const qsRows = [
            comboRow(settings, 'qs-mode', s.t('prefs_qs_mode'),
                [['single', s.t('qs_single')], ['providers', s.t('qs_providers')]], s.t('prefs_qs_mode_sub')),
            switchRow(settings, 'qs-full', s.t('prefs_qs_full'), s.t('prefs_qs_full_sub')),
            switchRow(settings, 'qs-indicator', s.t('prefs_qs_indicator'), s.t('prefs_qs_indicator_sub')),
        ];
        for (const r of qsRows) {
            settings.bind('qs-enabled', r, 'sensitive', Gio.SettingsBindFlags.GET);
            qs.add(r);
        }
        page.add(qs);

        const dm = new Adw.PreferencesGroup({title: s.t('prefs_place_datemenu'), description: s.t('prefs_place_datemenu_desc')});
        dm.add(switchRow(settings, 'datemenu-enabled', s.t('prefs_datemenu_enabled')));
        const dmRows = [
            comboRow(settings, 'datemenu-position', s.t('prefs_datemenu_position'), [
                ['right-top', s.t('dm_right_top')], ['right-bottom', s.t('dm_right_bottom')],
                ['left-top', s.t('dm_left_top')], ['left-bottom', s.t('dm_left_bottom')],
            ]),
            comboRow(settings, 'datemenu-detail', s.t('prefs_datemenu_detail'),
                [['compact', s.t('dm_compact')], ['full', s.t('dm_full')]]),
            switchRow(settings, 'datemenu-selected-only', s.t('prefs_datemenu_selected')),
        ];
        for (const r of dmRows) {
            settings.bind('datemenu-enabled', r, 'sensitive', Gio.SettingsBindFlags.GET);
            dm.add(r);
        }
        page.add(dm);
        return page;
    }

    // ------------------------------------------------------------ görünüm

    _appearancePage(settings, s) {
        const page = new Adw.PreferencesPage({title: s.t('prefs_appearance_page'), icon_name: 'applications-graphics-symbolic'});
        const panel = new Adw.PreferencesGroup({title: s.t('prefs_panel')});
        panel.add(comboRow(settings, 'panel-style', s.t('prefs_style'),
            [['icon', s.t('style_icon')], ['ring', s.t('style_ring')], ['text', s.t('style_text')]]));
        panel.add(comboRow(settings, 'panel-mode', s.t('prefs_mode'),
            [['each', s.t('mode_each')], ['max', s.t('mode_max')]]));
        panel.add(comboRow(settings, 'panel-window-mode', s.t('prefs_panel_windows'),
            [['all', s.t('pw_all')], ['auto', s.t('pw_auto')]], s.t('prefs_panel_windows_sub')));
        panel.add(switchRow(settings, 'panel-letters', s.t('prefs_letters'), s.t('prefs_letters_sub')));
        panel.add(comboRow(settings, 'panel-icon', s.t('prefs_panel_icon'),
            [['provider', s.t('icon_provider')], ['kota', s.t('icon_kota')], ['both', s.t('icon_both')]]));
        panel.add(switchRow(settings, 'panel-colored-icons', s.t('prefs_colored'), s.t('prefs_colored_sub')));
        panel.add(switchRow(settings, 'show-remaining', s.t('prefs_remaining'), s.t('prefs_remaining_sub')));
        page.add(panel);

        const menu = new Adw.PreferencesGroup({title: s.t('prefs_menu')});
        menu.add(comboRow(settings, 'popup-theme', s.t('prefs_theme'),
            [['auto', s.t('theme_auto')], ['dark', s.t('theme_dark')], ['light', s.t('theme_light')]]));
        menu.add(comboRow(settings, 'bar-colors', s.t('prefs_bar_colors'),
            [['usage', s.t('bars_usage')], ['brand', s.t('bars_brand')]], s.t('prefs_bar_colors_sub')));
        menu.add(comboRow(settings, 'popup-background', s.t('prefs_background'),
            [['auto', s.t('bg_auto')], ['translucent', s.t('bg_translucent')], ['opaque', s.t('bg_opaque')]],
            s.t('prefs_background_sub')));
        const opacity = intSpinRow(settings, 'popup-opacity', s.t('prefs_opacity'), 0, 100, 5);
        const syncOpacity = () => {
            opacity.sensitive = settings.get_string('popup-background') !== 'opaque';
        };
        syncOpacity();
        const bgId = settings.connect('changed::popup-background', syncOpacity);
        opacity.connect('destroy', () => settings.disconnect(bgId));
        menu.add(opacity);
        menu.add(comboRow(settings, 'usage-style', s.t('prefs_usage_style'),
            [['rings', s.t('style_rings')], ['bars', s.t('style_bars')]]));
        menu.add(switchRow(settings, 'show-pace', s.t('prefs_pace'), s.t('prefs_pace_sub')));
        menu.add(intSpinRow(settings, 'popup-width', s.t('prefs_width'), 340, 640, 10));
        menu.add(intSpinRow(settings, 'usage-columns', s.t('prefs_columns'), 1, 3, 1));
        const accentRow = new Adw.ActionRow({title: s.t('prefs_accent')});
        const dialog = new Gtk.ColorDialog({with_alpha: false});
        const colorBtn = new Gtk.ColorDialogButton({dialog, valign: Gtk.Align.CENTER});
        const rgba = new Gdk.RGBA();
        rgba.parse(settings.get_string('accent-color'));
        colorBtn.rgba = rgba;
        colorBtn.connect('notify::rgba', () => {
            const c = colorBtn.rgba;
            const hex = x => Math.round(x * 255).toString(16).padStart(2, '0');
            settings.set_string('accent-color', `#${hex(c.red)}${hex(c.green)}${hex(c.blue)}`);
        });
        accentRow.add_suffix(colorBtn);
        menu.add(accentRow);
        page.add(menu);

        const sections = new Adw.PreferencesGroup({title: s.t('prefs_sections')});
        for (const [key, text] of [['show-tiles', 'sec_tiles'], ['show-usage', 'sec_usage'],
            ['show-activity', 'sec_activity'], ['show-agents', 'sec_agents'], ['show-filter', 'sec_filter']])
            sections.add(switchRow(settings, key, s.t(text)));
        page.add(sections);
        return page;
    }

    // ------------------------------------------------------------ servis

    _thresholds(page, proxy, s, svc) {
        const group = new Adw.PreferencesGroup({title: s.t('prefs_thresholds')});
        const save = debounce(700, patch => proxy.SetSettingsAsync(JSON.stringify(patch)).catch(() => {}));
        for (const [key, title] of [['warning', 'prefs_warning'], ['critical', 'prefs_critical']]) {
            const row = Adw.SpinRow.new_with_range(1, 100, 1);
            row.title = s.t(title);
            row.value = svc[key];
            row.connect('notify::value', () => save({[key]: row.value}));
            group.add(row);
        }
        page.add(group);
    }

    _service(window, page, proxy, s, svc, general) {
        const set = patch => proxy.SetSettingsAsync(JSON.stringify(patch)).catch(() => {});
        const group = new Adw.PreferencesGroup({title: s.t('prefs_polling')});
        const interval = Adw.SpinRow.new_with_range(30, 3600, 10);
        interval.title = s.t('prefs_interval');
        interval.value = svc.interval_seconds;
        const saveInterval = debounce(800, v => set({interval_seconds: Math.round(v)}));
        interval.connect('notify::value', () => saveInterval(interval.value));
        group.add(interval);
        for (const [key, title, sub] of [
            ['paused', 'prefs_paused', 'prefs_paused_sub'],
            ['claude_refresh_tokens', 'prefs_claude_refresh', 'prefs_claude_refresh_sub'],
            ['local_logs', 'prefs_local_logs', 'prefs_local_logs_sub'],
        ]) {
            const row = new Adw.SwitchRow({title: s.t(title), subtitle: s.t(sub), active: Boolean(svc[key])});
            row.connect('notify::active', () => set({[key]: row.active}));
            group.add(row);
        }
        const configRow = new Adw.ActionRow({title: s.t('prefs_open_config'), subtitle: svc.config_path, activatable: true});
        configRow.add_suffix(new Gtk.Image({icon_name: 'document-open-symbolic'}));
        configRow.connect('activated', () => {
            const f = Gio.File.new_for_path(svc.config_path);
            if (!f.query_exists(null))
                proxy.SetSettingsAsync('{}').catch(() => {});
            openUri(window, f.get_uri());
        });
        group.add(configRow);
        const restart = new Adw.ActionRow({title: s.t('prefs_restart'), subtitle: 'systemctl --user restart kota', activatable: true});
        restart.add_suffix(new Gtk.Image({icon_name: 'view-refresh-symbolic'}));
        restart.connect('activated', () => {
            try {
                Gio.Subprocess.new(['systemctl', '--user', 'restart', 'kota'], Gio.SubprocessFlags.NONE);
            } catch (e) {
                restart.subtitle = e.message;
            }
        });
        group.add(restart);
        page.add(group);
        page.add(general);

        const about = new Adw.PreferencesGroup({title: s.t('prefs_about')});
        about.add(new Adw.ActionRow({title: 'Kota', subtitle: `${this.metadata['version-name'] ?? ''} · ${this.metadata.url ?? ''}`}));
        page.add(about);
    }
}
