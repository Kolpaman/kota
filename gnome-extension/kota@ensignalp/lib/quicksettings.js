// Kota — Hızlı Ayarlar (Quick Settings) entegrasyonu.
//
// Biçimler:
//   single     tek "Kota" kutucuğu: tıklama takibi duraklatır/sürdürür,
//              ok düğmesi tüm sağlayıcıların ilerleme çizgilerini açar.
//   providers  sağlayıcı başına bir kutucuk: tıklama o sağlayıcının
//              pencerelerini (5 saat, haftalık…) gösteren menüyü açar.
// İsteğe bağlı olarak sistem simgeleri arasında (Wi-Fi/pil yanında) logo + yüzde.

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GObject from 'gi://GObject';
import St from 'gi://St';

import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {QuickMenuToggle, SystemIndicator} from 'resource:///org/gnome/shell/ui/quickSettings.js';

import {renderSummary, shortSummary, topProvider} from './summary.js';
import {VERTICAL, hasIcon, providerGicon} from './widgets.js';

function kotaGicon(extPath) {
    return Gio.icon_new_for_string(`${extPath}/icons/kota-symbolic.svg`);
}

/** Menüye özet listesini taşıyan, tıklanamayan öğe ekler. */
function summaryItem(menu) {
    const item = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false, style_class: 'kota-qs-item'});
    const box = new St.BoxLayout({orientation: VERTICAL, x_expand: true, style_class: 'kota-sum'});
    item.add_child(box);
    menu.addMenuItem(item);
    return box;
}

const KotaToggle = GObject.registerClass(
class KotaToggle extends QuickMenuToggle {
    _init(ctx, providerId = null) {
        super._init({title: providerId ? '' : 'Kota', toggleMode: false});
        this._ctx = ctx;
        this._providerId = providerId;
        this.menu.setHeader(kotaGicon(ctx.extPath), providerId ?? 'Kota', '');
        this._box = summaryItem(this.menu);
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this._refreshItem = this.menu.addAction('', () => ctx.actions.refresh(), 'view-refresh-symbolic');
        this._openItem = this.menu.addAction('', () => ctx.actions.openPopup(), 'view-reveal-symbolic');
        this._settingsItem = this.menu.addAction('', () => ctx.actions.openSettings(), 'emblem-system-symbolic');
        this.connect('clicked', () => {
            if (this._providerId)
                this.menu.open();
            else
                ctx.actions.togglePause();
        });
    }

    update(snapshot) {
        const {strings: s, settings, extPath} = this._ctx;
        this._refreshItem.label.text = s.t('refresh_now');
        this._openItem.label.text = s.t('open_kota');
        this._openItem.visible = settings.get_boolean('panel-enabled');
        this._settingsItem.label.text = s.t('settings');
        if (this._providerId) {
            const p = snapshot?.providers?.find(x => x.id === this._providerId);
            let sub = shortSummary({providers: p ? [p] : []}, settings, s, 1).replace(`${p?.name ?? ''} `, '');
            const hw = p?.windows?.length ? p.windows.reduce((a, b) => (b.used_percent > a.used_percent ? b : a)) : null;
            if (sub && hw?.resets_at && !p.stale)
                sub += ` · ${s.resetsIn(hw.resets_at)}`;
            this.title = p?.name ?? this._providerId;
            this.subtitle = p ? sub || s.status(p) : '';
            this.gicon = hasIcon(this._providerId) ? providerGicon(extPath, this._providerId, false) : kotaGicon(extPath);
            this.checked = Boolean(p && p.status === 'ok' && !p.stale);
            this.menu.setHeader(this.gicon, this.title, p?.plan ? `${p.plan} · ${s.status(p)}` : p ? s.status(p) : '');
            renderSummary(this._box, p ? {...snapshot, providers: [p]} : snapshot, {
                extPath, settings, strings: s, full: true, hideHead: true, onConnect: id => this._ctx.actions.connect(id),
            });
        } else {
            const top = topProvider(snapshot, settings);
            this.title = 'Kota';
            this.subtitle = snapshot?.paused ? s.t('paused') : shortSummary(snapshot, settings, s, 2);
            this.gicon = top && hasIcon(top.p.id) ? providerGicon(extPath, top.p.id, false) : kotaGicon(extPath);
            this.checked = Boolean(snapshot && !snapshot.paused);
            this.menu.setHeader(kotaGicon(extPath), 'Kota', snapshot?.paused ? s.t('paused') : '');
            renderSummary(this._box, snapshot, {
                extPath, settings, strings: s, full: settings.get_boolean('qs-full'),
                onConnect: id => this._ctx.actions.connect(id),
            });
        }
    }
});

export const KotaQuickSettings = GObject.registerClass(
class KotaQuickSettings extends SystemIndicator {
    /**
     * @param {object} ctx {extPath, settings, strings, actions}
     */
    _init(ctx) {
        super._init();
        this._ctx = ctx;
        this._toggles = new Map();
        this._mode = null;

        // Sistem simgeleri arasındaki gösterge: logo + yüzde.
        this._icon = this._addIndicator();
        this._label = new St.Label({style_class: 'kota-qs-indicator-label', y_align: Clutter.ActorAlign.CENTER});
        this.add_child(this._label);
        this._icon.visible = false;
        this._label.visible = false;
    }

    /** Kutucukları gerekiyorsa yeniden kurar. Dönüş: eklenmesi gereken yeni öğeler. */
    _sync(snapshot) {
        const st = this._ctx.settings;
        const mode = st.get_string('qs-mode');
        let ids;
        if (mode === 'providers') {
            const selected = st.get_strv('panel-providers');
            ids = (snapshot?.providers ?? [])
                .filter(p => (!selected.length || selected.includes(p.id)) && p.status !== 'not_connected')
                .map(p => p.id);
        } else {
            ids = ['__kota__'];
        }
        const same = this._mode === mode && ids.length === this._toggles.size && ids.every(id => this._toggles.has(id));
        // Hızlı Ayarlar açıkken kutucukları değiştirme (açık alt menü yok olurdu); kapanınca yapılır.
        if (same || (this._qs?.menu.isOpen && this._toggles.size))
            return false;
        this._clearToggles();
        this._mode = mode;
        for (const id of ids) {
            const t = new KotaToggle(this._ctx, id === '__kota__' ? null : id);
            this._toggles.set(id, t);
            this.quickSettingsItems.push(t);
        }
        return true;
    }

    _clearToggles() {
        for (const t of this._toggles.values())
            t.destroy();
        this._toggles.clear();
        this.quickSettingsItems.length = 0;
    }

    /** Hızlı Ayarlar ızgarasına ekler (ilk çağrıda ve kutucuk kümesi değişince). */
    attach(qs) {
        this._qs = qs;
        qs.addExternalIndicator(this);
    }

    update(snapshot) {
        const st = this._ctx.settings;
        if (this._sync(snapshot) && this._qs) {
            // Yeni kutucukları ızgaraya ekle (QuickSettings yalnızca ilk eklemede okur).
            const sibling = this._qs._backgroundApps?.quickSettingsItems?.at(-1) ?? null;
            this._qs._addItemsBefore(this.quickSettingsItems, sibling, 1);
        }
        for (const t of this._toggles.values())
            t.update(snapshot);

        const showIndicator = st.get_boolean('qs-indicator') && snapshot;
        const top = showIndicator ? topProvider(snapshot, st) : null;
        if (top) {
            this._icon.gicon = hasIcon(top.p.id) ? providerGicon(this._ctx.extPath, top.p.id, false) : kotaGicon(this._ctx.extPath);
            this._icon.visible = true;
            const v = st.get_boolean('show-remaining') ? Math.max(0, 100 - top.w.used_percent) : top.w.used_percent;
            this._label.text = this._ctx.strings.percent(v, top.w.estimated);
            this._label.style_class = `kota-qs-indicator-label kota-sev-${top.p.stale ? 'stale' : top.w.severity}`;
            this._label.visible = true;
        } else {
            this._icon.visible = false;
            this._label.visible = false;
        }
        this._syncIndicatorsVisible();
    }

    destroy() {
        this._clearToggles();
        super.destroy();
    }
});
