// Kota — Blur My Shell uyumu.
//
// Blur My Shell (BMS) açılır pencereleri `popup-menu-content` hedefinden
// bulanıklaştırır ve kendi köşe yarıçapını uygular. Kota'nın opak arka planı
// bu bulanıklığı örtüyordu; BMS'in açılır pencere bulanıklığı açıksa Kota
// yarı saydam arka plan ve BMS'in köşe yarıçapını kullanır.

import Gio from 'gi://Gio';

const BMS_UUID = 'blur-my-shell@aunetx';
const ENABLED = 1; // ExtensionState.ACTIVE

export class BlurWatcher {
    /**
     * @param {object} extensionManager Main.extensionManager
     * @param {() => void} onChange
     */
    constructor(extensionManager, onChange) {
        this._manager = extensionManager;
        this._onChange = onChange;
        this._settings = null;
        this._stateId = extensionManager.connect('extension-state-changed', (_m, ext) => {
            if (ext?.uuid === BMS_UUID) {
                this._load();
                this._onChange();
            }
        });
        this._load();
    }

    _load() {
        this._settings?.disconnectObject(this);
        this._settings = null;
        const ext = this._manager.lookup(BMS_UUID);
        if (!ext || ext.state !== ENABLED)
            return;
        try {
            const dir = ext.dir.get_child('schemas');
            const src = Gio.SettingsSchemaSource.new_from_directory(dir.get_path(),
                Gio.SettingsSchemaSource.get_default(), false);
            const schema = src.lookup('org.gnome.shell.extensions.blur-my-shell.popup', true);
            if (!schema)
                return;
            this._settings = new Gio.Settings({settings_schema: schema});
            this._settings.connectObject('changed', () => this._onChange(), this);
        } catch (e) {
            console.warn(`Kota: Blur My Shell settings unavailable: ${e.message}`);
            this._settings = null;
        }
    }

    /** {active, radius} — active: BMS açılır pencere bulanıklığı açık mı. */
    get state() {
        const s = this._settings;
        if (!s)
            return {active: false, radius: null};
        const has = k => s.settings_schema.has_key(k);
        const active = has('blur') ? s.get_boolean('blur') : false;
        const radius = has('menu-corner-radius') ? s.get_int('menu-corner-radius')
            : has('corner-radius') ? s.get_int('corner-radius') : null;
        return {active, radius};
    }

    destroy() {
        this._settings?.disconnectObject(this);
        this._settings = null;
        if (this._stateId)
            this._manager.disconnect(this._stateId);
        this._stateId = 0;
    }
}
