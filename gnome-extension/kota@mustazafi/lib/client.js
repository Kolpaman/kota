// Kota — D-Bus istemcisi. Eklenti ve tercihler penceresi yalnızca bu servisi
// konuşur; sağlayıcılara kendileri bağlanmaz, token görmez.

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

export const BUS_NAME = 'io.github.mustazafi.Kota';
export const OBJECT_PATH = '/io/github/mustazafi/Kota';
const SCHEMA_VERSION = 1;
const RETRY_SECONDS = 30;

const KotaIface = `
<node>
  <interface name="io.github.mustazafi.Kota1">
    <method name="GetSnapshot"><arg type="s" name="snapshot" direction="out"/></method>
    <method name="Refresh"><arg type="s" name="snapshot" direction="out"/></method>
    <method name="Connect"><arg type="s" name="provider" direction="in"/></method>
    <method name="ListProviders"><arg type="s" name="providers" direction="out"/></method>
    <method name="SetProviderEnabled">
      <arg type="s" name="id" direction="in"/><arg type="b" name="enabled" direction="in"/>
    </method>
    <method name="SetApiKey">
      <arg type="s" name="id" direction="in"/><arg type="s" name="key" direction="in"/>
    </method>
    <method name="SetBudget">
      <arg type="s" name="id" direction="in"/><arg type="d" name="amount" direction="in"/>
    </method>
    <method name="GetSettings"><arg type="s" name="settings" direction="out"/></method>
    <method name="SetSettings"><arg type="s" name="patch" direction="in"/></method>
    <method name="StopAgent"><arg type="u" name="pid" direction="in"/></method>
    <method name="ListAccounts"><arg type="s" name="accounts" direction="out"/></method>
    <method name="SaveAccount">
      <arg type="s" name="provider" direction="in"/><arg type="s" name="name" direction="in"/>
      <arg type="s" name="saved" direction="out"/>
    </method>
    <method name="SwitchAccount">
      <arg type="s" name="provider" direction="in"/><arg type="s" name="name" direction="in"/>
    </method>
    <method name="RemoveAccount">
      <arg type="s" name="provider" direction="in"/><arg type="s" name="name" direction="in"/>
    </method>
    <signal name="Updated"><arg type="s" name="snapshot"/></signal>
    <property name="Version" type="s" access="read"/>
  </interface>
</node>`;

export const KotaProxy = Gio.DBusProxy.makeProxyWrapper(KotaIface);

export function describe(error) {
    if (error instanceof GLib.Error) {
        const missing = error.matches(Gio.DBusError, Gio.DBusError.SERVICE_UNKNOWN) ||
            error.matches(Gio.DBusError, Gio.DBusError.NAME_HAS_NO_OWNER) ||
            error.matches(Gio.DBusError, Gio.DBusError.SPAWN_SERVICE_NOT_FOUND);
        if (Gio.DBusError.is_remote_error(error))
            Gio.DBusError.strip_remote_error(error);
        return {kind: missing ? 'service-missing' : 'dbus', message: error.message};
    }
    return {kind: 'dbus', message: String(error?.message ?? error)};
}

function isCancelled(error) {
    return error instanceof GLib.Error && error.matches(Gio.IOErrorEnum, Gio.IOErrorEnum.CANCELLED);
}

/** Tercihler penceresi için: tek seferlik proxy (servis kapalıysa D-Bus başlatır). */
export function newProxy(cancellable = null) {
    return KotaProxy.newAsync(Gio.DBus.session, BUS_NAME, OBJECT_PATH, cancellable,
        Gio.DBusProxyFlags.DO_NOT_LOAD_PROPERTIES);
}

export class KotaClient {
    /**
     * @param {object} handlers
     * @param {(snapshot: object) => void} handlers.onSnapshot
     * @param {(error: {kind: string, message: string}) => void} handlers.onError
     */
    constructor({onSnapshot, onError}) {
        this._onSnapshot = onSnapshot;
        this._onError = onError;
        this._cancellable = new Gio.Cancellable();
        this._proxy = null;
        this._signalId = 0;
        this._retryId = 0;
        this._destroyed = false;
    }

    async start() {
        if (this._destroyed || this._proxy)
            return;
        try {
            this._proxy = await newProxy(this._cancellable);
        } catch (e) {
            if (!this._destroyed && !isCancelled(e)) {
                this._onError(describe(e));
                this._scheduleRetry();
            }
            return;
        }
        if (this._destroyed) {
            this._proxy = null;
            return;
        }
        this._signalId = this._proxy.connectSignal('Updated', (_proxy, _sender, [json]) => this._deliver(json));
        this._proxy.connectObject('notify::g-name-owner', () => {
            if (this._proxy?.g_name_owner)
                this.fetch();
        }, this);
        await this.fetch();
    }

    async fetch() {
        if (!this._proxy)
            return this.start();
        try {
            const [json] = await this._proxy.GetSnapshotAsync();
            this._deliver(json);
        } catch (e) {
            if (!this._destroyed && !isCancelled(e)) {
                this._onError(describe(e));
                this._scheduleRetry();
            }
        }
        return undefined;
    }

    async refresh() {
        if (!this._proxy)
            return this.start();
        try {
            const [json] = await this._proxy.RefreshAsync();
            this._deliver(json);
        } catch (e) {
            if (!this._destroyed && !isCancelled(e))
                this._onError(describe(e));
        }
        return undefined;
    }

    /** Hata mesajını fırlatan çağrı sarmalayıcısı. */
    async call(method, ...args) {
        if (!this._proxy)
            throw new Error('service unavailable');
        try {
            const out = await this._proxy[`${method}Async`](...args);
            return out?.[0];
        } catch (e) {
            throw new Error(describe(e).message);
        }
    }

    connectProvider(id) {
        return this.call('Connect', id);
    }

    stopAgent(pid) {
        return this.call('StopAgent', pid);
    }

    setPaused(paused) {
        return this.call('SetSettings', JSON.stringify({paused}));
    }

    _deliver(json) {
        let snapshot;
        try {
            snapshot = JSON.parse(json);
        } catch (e) {
            this._onError({kind: 'invalid', message: String(e)});
            return;
        }
        if (snapshot?.schema !== SCHEMA_VERSION) {
            this._onError({kind: 'schema', message: `unsupported snapshot schema ${snapshot?.schema}`});
            return;
        }
        this._clearRetry();
        this._onSnapshot(snapshot);
    }

    _scheduleRetry() {
        if (this._retryId || this._destroyed)
            return;
        this._retryId = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, RETRY_SECONDS, () => {
            this._retryId = 0;
            this.fetch();
            return GLib.SOURCE_REMOVE;
        });
    }

    _clearRetry() {
        if (this._retryId) {
            GLib.source_remove(this._retryId);
            this._retryId = 0;
        }
    }

    destroy() {
        this._destroyed = true;
        this._cancellable.cancel();
        this._clearRetry();
        if (this._proxy) {
            if (this._signalId)
                this._proxy.disconnectSignal(this._signalId);
            this._proxy.disconnectObject(this);
            this._proxy = null;
        }
        this._onSnapshot = () => {};
        this._onError = () => {};
    }
}
