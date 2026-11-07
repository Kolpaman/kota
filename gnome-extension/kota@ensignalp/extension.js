// Kota — AI kullanım limitleri için GNOME Shell eklentisi.
//
// Eklenti hiçbir sağlayıcıya bağlanmaz ve token görmez: tüm veri ayrı
// paketlenen `kota` servisinden (D-Bus: io.github.ensignalp.Kota) gelir.
//
// Görünüm yerleri (hepsi ayarlardan açılıp kapanır):
//   - üst panel göstergesi (konum + sıra)
//   - Hızlı Ayarlar kutucuğu/kutucukları ve sistem simgesi
//   - tarih/saat açılır penceresinde bölüm

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';

import {BlurWatcher} from './lib/blur.js';
import {KotaClient} from './lib/client.js';
import {KotaDateMenuSection} from './lib/datemenu.js';
import {Strings, parseTime, resolveLanguage} from './lib/i18n.js';
import {renderPanel} from './lib/panel.js';
import {KotaPopup} from './lib/popup.js';
import {KotaQuickSettings} from './lib/quicksettings.js';
import {setBarMode, setLight} from './lib/widgets.js';

/** Menü açıkken göreli zamanların ("3 sa 18 dk") yenilenme aralığı. */
const TICK_SECONDS = 30;
/** Sıfırlanma zamanı bu kadar oynarsa hâlâ aynı dönem sayılır (API milisaniye oynatıyor). */
const SAME_PERIOD_MS = 30 * 60 * 1000;
const SEVERITY_LEVEL = {normal: 0, warning: 1, critical: 2};
const PANEL_KEYS = ['panel-mode', 'panel-style', 'panel-providers', 'panel-windows', 'panel-window-mode', 'panel-icon',
    'panel-letters', 'panel-colored-icons',
    'show-remaining'];
const LAYOUT_KEYS = ['popup-width', 'usage-columns', 'usage-style', 'accent-color', 'show-tiles', 'show-usage',
    'show-activity', 'show-agents', 'show-filter', 'show-pace', 'show-remaining', 'activity-range'];
const BACKGROUND_KEYS = ['popup-background', 'popup-opacity'];
const SURFACE_KEYS = ['qs-mode', 'qs-indicator', 'qs-full', 'datemenu-position', 'datemenu-detail',
    'datemenu-selected-only', 'panel-providers', 'panel-windows', 'panel-window-mode', 'show-remaining', 'show-pace'];

const KotaIndicator = GObject.registerClass(
class KotaIndicator extends PanelMenu.Button {
    _init(ctx) {
        super._init(0.5, 'Kota', false);
        this._ctx = ctx;
        this._tickId = 0;

        this._labels = new St.BoxLayout({style_class: 'kota-panel', y_align: Clutter.ActorAlign.CENTER});
        this.add_child(this._labels);

        this.menu.actor.add_style_class_name('kota-menu');
        this._popup = new KotaPopup({
            extPath: ctx.extPath,
            settings: ctx.settings,
            strings: ctx.strings,
            actions: ctx.actions,
        });
        this.menu.box.add_child(this._popup.actor);

        this.menu.connectObject('open-state-changed', (_menu, open) => {
            if (open) {
                this._popup.update(ctx.state.snapshot, ctx.state.error);
                this._startTick();
            } else {
                this._stopTick();
            }
        }, this);
        this.applyBackground();
        this.applyTheme();
        this.update();
    }

    /** Açık/koyu tema: ayardan ya da GNOME'un renk tercihinden. */
    applyTheme() {
        const light = this._ctx.isLight();
        if (light)
            this.menu.actor.add_style_class_name('kota-light');
        else
            this.menu.actor.remove_style_class_name('kota-light');
    }

    /**
     * Blur My Shell uyumlu arka plan. Zemin `menu.box` (popup-menu-content)
     * üzerindedir; BMS bulanıklığı tam bu öğenin arkasına, kendi köşe
     * yarıçapıyla koyar. Yarı saydam kipte aynı yarıçap satır içi verilir ki
     * zemin ile bulanıklık kenarları üst üste otursun.
     */
    applyBackground() {
        const st = this._ctx.settings;
        const mode = st.get_string('popup-background');
        const bms = this._ctx.blur.state;
        const box = this.menu.box;
        this.menu.actor.bin.style = null;
        const glass = mode === 'translucent' || (mode === 'auto' && bms.active);
        const light = this._ctx.isLight();
        if (!glass) {
            this.menu.actor.remove_style_class_name('kota-glass');
            box.style = bms.active && bms.radius !== null ? `border-radius: ${bms.radius}px;` : null;
            return;
        }
        this.menu.actor.add_style_class_name('kota-glass');
        const opacity = Math.max(0, Math.min(100, st.get_int('popup-opacity'))) / 100;
        let style = light ? `background-color: rgba(246, 247, 249, ${Math.max(0.35, opacity).toFixed(2)});`
            : `background-color: rgba(18, 21, 28, ${opacity.toFixed(2)});`;
        if (bms.active && bms.radius !== null)
            style += ` border-radius: ${bms.radius}px;`;
        box.style = style;
    }

    update() {
        const {settings, strings, state, extPath} = this._ctx;
        renderPanel(this._labels, state.snapshot, state.error, settings, strings, extPath);
        if (this.menu.isOpen)
            this._popup.update(state.snapshot, state.error);
    }

    setStrings(strings) {
        this._popup.setStrings(strings);
        this.update();
    }

    relayout() {
        this._popup.relayout();
    }

    _startTick() {
        this._stopTick();
        this._tickId = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, TICK_SECONDS, () => {
            this._popup.update(this._ctx.state.snapshot, this._ctx.state.error);
            return GLib.SOURCE_CONTINUE;
        });
    }

    _stopTick() {
        if (this._tickId) {
            GLib.source_remove(this._tickId);
            this._tickId = 0;
        }
    }

    destroy() {
        this._stopTick();
        super.destroy();
    }
});

export default class KotaExtension extends Extension {
    enable() {
        this._settings = this.getSettings();
        const settings = this._settings;
        this._ctx = {
            extPath: this.path,
            settings,
            strings: new Strings(resolveLanguage(settings.get_string('language'))),
            state: {snapshot: null, error: null},
            actions: {
                refresh: () => this._client?.refresh(),
                togglePause: () => this._client?.setPaused(!this._ctx.state.snapshot?.paused).catch(e => this._fail(e)),
                openSettings: () => {
                    this._closeMenus();
                    this.openPreferences();
                },
                openPopup: () => this._openPopup(),
                connect: id => this._connect(id),
                stopAgent: pid => this._client?.stopAgent(pid).catch(e => this._fail(e)),
                openPath: p => this._open(`file://${p.replace(/^~/, GLib.get_home_dir())}`),
                openUrl: u => this._open(u),
            },
        };
        this._ctx.isLight = () => {
            const pref = settings.get_string('popup-theme');
            if (pref !== 'auto')
                return pref === 'light';
            const st = St.Settings.get();
            return 'color_scheme' in st && St.SystemColorScheme !== undefined &&
                st.color_scheme === St.SystemColorScheme.PREFER_LIGHT;
        };
        setBarMode(settings.get_string('bar-colors'));
        setLight(this._ctx.isLight());
        St.Settings.get().connectObject('notify::color-scheme', () => this._onSettingChanged('popup-theme'), this);
        this._ctx.blur = new BlurWatcher(Main.extensionManager, () => this._indicator?.applyBackground());

        this._syncPlacement();
        settings.connectObject('changed', (_s, key) => this._onSettingChanged(key), this);

        this._client = new KotaClient({
            onSnapshot: snapshot => this._onSnapshot(snapshot),
            onError: error => this._onError(error),
        });
        this._client.start();

        Main.wm.addKeybinding('toggle-shortcut', settings, Meta.KeyBindingFlags.NONE,
            Shell.ActionMode.NORMAL | Shell.ActionMode.OVERVIEW, () => this._togglePopup());
    }

    disable() {
        Main.wm.removeKeybinding('toggle-shortcut');
        St.Settings.get().disconnectObject(this);
        this._settings?.disconnectObject(this);
        this._client?.destroy();
        this._client = null;
        this._destroyIndicator();
        this._destroyQuickSettings();
        this._destroyDateMenu();
        this._ctx?.blur?.destroy();
        this._ctx = null;
        this._settings = null;
    }

    // ------------------------------------------------------------ yerleşim

    _syncPlacement() {
        const st = this._settings;
        if (st.get_boolean('panel-enabled'))
            this._createIndicator();
        else
            this._destroyIndicator();
        if (st.get_boolean('qs-enabled'))
            this._createQuickSettings();
        else
            this._destroyQuickSettings();
        if (st.get_boolean('datemenu-enabled'))
            this._createDateMenu();
        else
            this._destroyDateMenu();
    }

    _createIndicator() {
        this._destroyIndicator();
        const st = this._settings;
        const box = st.get_string('panel-position');
        const index = st.get_int('panel-index');
        this._indicator = new KotaIndicator(this._ctx);
        Main.panel.addToStatusArea(this.uuid, this._indicator, index, box);
    }

    _destroyIndicator() {
        this._indicator?.destroy();
        this._indicator = null;
    }

    _createQuickSettings() {
        if (this._qs)
            return;
        const qs = Main.panel.statusArea.quickSettings;
        this._qs = new KotaQuickSettings(this._ctx);
        this._qs.update(this._ctx.state.snapshot);
        // Hızlı Ayarlar bazı öğelerini eşzamansız kurar; hazır olana kadar bekle.
        const attach = () => {
            if (!this._qs)
                return GLib.SOURCE_REMOVE;
            if (!qs._indicators)
                return GLib.SOURCE_CONTINUE;
            this._qs.attach(qs);
            this._qsAttachId = 0;
            return GLib.SOURCE_REMOVE;
        };
        if (attach() === GLib.SOURCE_CONTINUE)
            this._qsAttachId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 200, attach);
    }

    _destroyQuickSettings() {
        if (this._qsAttachId) {
            GLib.source_remove(this._qsAttachId);
            this._qsAttachId = 0;
        }
        this._qs?.destroy();
        this._qs = null;
    }

    _createDateMenu() {
        if (this._dateMenu || !Main.panel.statusArea.dateMenu)
            return;
        this._dateMenu = new KotaDateMenuSection(this._ctx);
        this._dateMenu.update(this._ctx.state.snapshot);
    }

    _destroyDateMenu() {
        this._dateMenu?.destroy();
        this._dateMenu = null;
    }

    _onSettingChanged(key) {
        const st = this._settings;
        if (key === 'popup-theme' || key === 'bar-colors') {
            setBarMode(st.get_string('bar-colors'));
            setLight(this._ctx.isLight());
            this._indicator?.applyTheme();
            this._indicator?.applyBackground();
            this._indicator?.relayout();
            this._broadcast();
            return;
        }
        if (key === 'language') {
            this._ctx.strings = new Strings(resolveLanguage(st.get_string('language')));
            this._indicator?.setStrings(this._ctx.strings);
            this._broadcast();
            return;
        }
        if (key === 'panel-enabled' || key === 'panel-position' || key === 'panel-index') {
            if (st.get_boolean('panel-enabled'))
                this._createIndicator();
            else
                this._destroyIndicator();
            return;
        }
        if (key === 'qs-enabled' || key === 'qs-mode') {
            this._destroyQuickSettings();
            if (st.get_boolean('qs-enabled'))
                this._createQuickSettings();
            return;
        }
        if (key === 'datemenu-enabled') {
            this._destroyDateMenu();
            if (st.get_boolean('datemenu-enabled'))
                this._createDateMenu();
            return;
        }
        if (BACKGROUND_KEYS.includes(key))
            this._indicator?.applyBackground();
        if (LAYOUT_KEYS.includes(key))
            this._indicator?.relayout();
        if (PANEL_KEYS.includes(key))
            this._indicator?.update();
        if (SURFACE_KEYS.includes(key)) {
            this._qs?.update(this._ctx.state.snapshot);
            this._dateMenu?.update(this._ctx.state.snapshot);
        }
    }

    // ------------------------------------------------------------ veri

    _broadcast() {
        this._indicator?.update();
        this._qs?.update(this._ctx.state.snapshot);
        this._dateMenu?.update(this._ctx.state.snapshot);
    }

    _onSnapshot(snapshot) {
        const previous = this._ctx.state.snapshot;
        this._ctx.state = {snapshot, error: null};
        this._broadcast();
        this._notify(previous, snapshot);
    }

    _onError(error) {
        this._ctx.state = {snapshot: this._ctx.state.snapshot, error};
        this._broadcast();
    }

    // ------------------------------------------------------------ eylemler

    _closeMenus() {
        this._indicator?.menu.close();
        Main.panel.statusArea.quickSettings?.menu.close();
        Main.panel.statusArea.dateMenu?.menu.close();
    }

    /** Ana açılır pencere: panel göstergesi varsa o, yoksa Hızlı Ayarlar menüsü. */
    _openPopup() {
        if (this._indicator) {
            Main.panel.statusArea.quickSettings?.menu.close();
            Main.panel.statusArea.dateMenu?.menu.close();
            this._indicator.menu.open();
            return;
        }
        const qs = Main.panel.statusArea.quickSettings;
        const toggle = this._qs?.quickSettingsItems?.[0] ?? [...(this._qs?._toggles?.values() ?? [])][0];
        if (qs && toggle) {
            Main.panel.statusArea.dateMenu?.menu.close();
            qs.menu.open();
            toggle.menu.open();
        }
    }

    _togglePopup() {
        if (this._indicator?.menu.isOpen) {
            this._indicator.menu.close();
            return;
        }
        this._openPopup();
    }

    _fail(e) {
        Main.notify('Kota', String(e?.message ?? e));
    }

    _open(uri) {
        try {
            Gio.AppInfo.launch_default_for_uri(uri, global.create_app_launch_context(0, -1));
            this._closeMenus();
        } catch (e) {
            this._fail(e);
        }
    }

    async _connect(id) {
        const p = this._ctx.state.snapshot?.providers?.find(x => x.id === id);
        // Anahtarlı sağlayıcılar: anahtar ayarlardan girilir.
        if (p && (p.code === 'no_api_key' || p.code === 'key_rejected' || !p.login_command)) {
            this._closeMenus();
            this.openPreferences();
            return;
        }
        const s = this._ctx.strings;
        try {
            await this._client?.connectProvider(id);
            Main.notify(s.t('connect_opened'), s.t('connect_opened_body'));
            this._closeMenus();
        } catch (e) {
            Main.notify(s.t('connect_failed'), e.message);
        }
    }

    /** Eşik, sıfırlanma ve oturum bildirimleri. Durum GSettings'te saklanır
     *  (kilit ekranı eklentiyi kapatıp açtığında tekrar bildirmemek için). */
    _notify(previous, snapshot) {
        const st = this._settings;
        const wantThreshold = st.get_boolean('notifications');
        const wantReset = st.get_boolean('notify-reset');
        const wantExpired = st.get_boolean('notify-expired');
        const wantForecast = st.get_boolean('notify-forecast');
        const aheadMs = st.get_int('notify-forecast-minutes') * 60000;
        if (!wantThreshold && !wantReset && !wantExpired && !wantForecast)
            return;
        let state;
        try {
            state = JSON.parse(st.get_string('notify-state')) ?? {};
        } catch {
            state = {};
        }
        const s = this._ctx.strings;
        const now = Date.now();
        let changed = false;
        for (const p of snapshot.providers ?? []) {
            const before = previous?.providers?.find(x => x.id === p.id);
            if (wantExpired && p.status === 'expired' && before?.status === 'ok')
                Main.notify(`${p.name}: ${s.t('notify_expired')}`, s.status(p));
            if (p.status !== 'ok' || p.stale)
                continue;
            for (const w of p.windows ?? []) {
                if (w.estimated)
                    continue;
                const level = SEVERITY_LEVEL[w.severity] ?? 0;
                const key = `${p.id}|${w.id}`;
                const resetMs = parseTime(w.resets_at);
                const resets = Number.isNaN(resetMs) ? null : resetMs;
                const prev = state[key];
                const samePeriod = prev !== undefined && (resets === null || prev.resets === null
                    ? resets === prev.resets
                    : Math.abs(prev.resets - resets) < SAME_PERIOD_MS);
                if (prev !== undefined && !samePeriod && prev.level >= 1 && wantReset)
                    Main.notify(`${p.name}: ${s.windowLabel(w)} ${s.t('notify_reset')}`, s.percent(w.used_percent));
                const prevLevel = samePeriod ? prev.level : 0;
                if (level > prevLevel && wantThreshold) {
                    const body = [w.resets_at ? s.resets(w.resets_at) : '', w.exhausts_at ? s.exhausts(w.exhausts_at) : '']
                        .filter(Boolean).join(' · ');
                    Main.notify(s.notifyTitle(p, w), body);
                }
                // Tükenme uyarısı: bu hızla limit, yenilenmeden önce ve yakında bitecekse (dönem başına bir kez).
                let fc = samePeriod ? Boolean(prev.fc) : false;
                const outMs = parseTime(w.exhausts_at);
                if (wantForecast && !fc && !Number.isNaN(outMs) && outMs > now && outMs - now <= aheadMs &&
                    (resets === null || outMs < resets) && w.used_percent < 100) {
                    Main.notify(`${p.name}: ${s.windowLabel(w)} ${s.t('notify_forecast')}`,
                        s.fmt('notify_forecast_body', {when: s.when(outMs), reset: resets ? s.when(resets) : '—'}));
                    fc = true;
                }
                const next = {level: Math.max(level, prevLevel), resets: samePeriod ? prev.resets : resets, fc};
                if (!prev || prev.level !== next.level || prev.resets !== next.resets || Boolean(prev.fc) !== fc) {
                    state[key] = next;
                    changed = true;
                }
            }
        }
        for (const [k, v] of Object.entries(state)) {
            if (v?.resets !== null && v?.resets < now - 7 * 86400000) {
                delete state[k];
                changed = true;
            }
        }
        if (changed)
            st.set_string('notify-state', JSON.stringify(state));
    }
}
