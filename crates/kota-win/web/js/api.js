// Tauri köprüsü: Rust komutları, olaylar ve arayüz ayarları (GSettings karşılığı).

import {Strings, resolveLanguage} from './i18n.js';

const T = window.__TAURI__;
export const invoke = (cmd, args) => T.core.invoke(cmd, args);
export const listen = (ev, fn) => T.event.listen(ev, e => fn(e.payload));

/** GSettings benzeri ayar nesnesi: get_* okur, set_* Rust'a yazar ve olay yayar. */
export class Settings {
    constructor(values) {
        this._v = values ?? {};
        this._handlers = [];
        listen('ui-changed', ({key, value}) => {
            this._v[key] = value;
            this._handlers.forEach(h => h(key));
        });
    }

    static async load() {
        return new Settings(await invoke('get_ui'));
    }

    onChange(fn) { this._handlers.push(fn); }
    get(key) { return this._v[key]; }
    get_boolean(k) { return Boolean(this._v[k]); }
    get_string(k) { return String(this._v[k] ?? ''); }
    get_int(k) { return Number(this._v[k] ?? 0); }
    get_strv(k) { return Array.isArray(this._v[k]) ? this._v[k] : []; }
    /** GSettings `a{ss}` değerleri için (`panel-windows`). */
    get_value(k) { const v = this._v[k] ?? {}; return {deepUnpack: () => v}; }

    async set(k, v) {
        this._v[k] = v;
        await invoke('set_ui', {key: k, value: v});
    }

    set_boolean(k, v) { return this.set(k, Boolean(v)); }
    set_string(k, v) { return this.set(k, String(v)); }
    set_int(k, v) { return this.set(k, Math.round(Number(v))); }
    set_strv(k, v) { return this.set(k, [...v]); }
}

export function makeStrings(settings) {
    return new Strings(resolveLanguage(settings.get_string('language')));
}

/** Küçük DOM yardımcısı: h('div.a.b', {onclick}, child…) */
export function h(tag, props = {}, ...children) {
    const [name, ...classes] = tag.split('.');
    const el = document.createElement(name || 'div');
    if (classes.length)
        el.className = classes.join(' ');
    for (const [k, v] of Object.entries(props ?? {})) {
        if (v === undefined || v === null || v === false)
            continue;
        if (k.startsWith('on'))
            el.addEventListener(k.slice(2), v);
        else if (k === 'style' && typeof v === 'object')
            Object.assign(el.style, v);
        else if (k === 'class')
            el.className += ` ${v}`;
        else if (k === 'html')
            el.innerHTML = v;
        else
            el.setAttribute(k, v === true ? '' : v);
    }
    for (const c of children.flat()) {
        if (c === null || c === undefined || c === false)
            continue;
        el.append(c instanceof Node ? c : document.createTextNode(String(c)));
    }
    return el;
}
