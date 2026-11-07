// Kota — tarih/saat açılır penceresine (bildirimler, takvim, hava durumu)
// kullanım bölümü ekler. Konum: sağ sütun üst/alt veya sol sütun üst/alt.

import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {renderSummary} from './summary.js';
import {VERTICAL, label} from './widgets.js';

export class KotaDateMenuSection {
    /**
     * @param {object} ctx {extPath, settings, strings, actions}
     */
    constructor(ctx) {
        this._ctx = ctx;
        this._snapshot = null;
        // Tema ile aynı kart görünümü için hava durumu düğmesinin sınıfları kullanılır.
        this.actor = new St.Button({
            style_class: 'weather-button kota-datemenu',
            x_expand: true,
            can_focus: true,
        });
        const box = new St.BoxLayout({orientation: VERTICAL, x_expand: true, style_class: 'kota-datemenu-box'});
        const head = new St.BoxLayout({x_expand: true, style_class: 'kota-datemenu-head'});
        this._title = label('Kota', 'weather-header kota-datemenu-title', {x_expand: true});
        head.add_child(this._title);
        this._updated = label('', 'kota-sum-small kota-sum-muted');
        head.add_child(this._updated);
        box.add_child(head);
        this._list = new St.BoxLayout({orientation: VERTICAL, x_expand: true, style_class: 'kota-sum'});
        box.add_child(this._list);
        this.actor.set_child(box);
        this.actor.connect('clicked', () => {
            Main.panel.statusArea.dateMenu?.menu.close();
            ctx.actions.openPopup();
        });
        this._placed = null;
        this.place();
    }

    /** Ayardaki konuma yerleştirir. */
    place() {
        const dm = Main.panel.statusArea.dateMenu;
        if (!dm)
            return;
        const pos = this._ctx.settings.get_string('datemenu-position');
        const parent = this.actor.get_parent();
        if (parent)
            parent.remove_child(this.actor);
        let target, index;
        if (pos.startsWith('left')) {
            target = dm._messageList?._scrollView?.get_parent();
            index = pos === 'left-top' ? 0 : Math.max(0, (target?.get_n_children() ?? 1) - 1);
        } else {
            target = dm._weatherItem?.get_parent();
            index = pos === 'right-top' ? 0 : -1;
        }
        if (!target)
            return;
        target.insert_child_at_index(this.actor, index);
        this._placed = pos;
        this.actor.set_style_class_name(`weather-button kota-datemenu kota-datemenu-${pos}`);
    }

    update(snapshot) {
        this._snapshot = snapshot;
        const {extPath, settings, strings: s} = this._ctx;
        if (this._placed !== settings.get_string('datemenu-position'))
            this.place();
        this._updated.text = snapshot?.paused ? s.t('paused') : '';
        renderSummary(this._list, snapshot, {
            extPath, settings, strings: s,
            full: settings.get_string('datemenu-detail') === 'full',
            onlySelected: settings.get_boolean('datemenu-selected-only'),
        });
    }

    destroy() {
        this.actor.destroy();
    }
}

