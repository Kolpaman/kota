// GNOME'daki GLib'in, i18n.js'in kullandığı küçük bir alt kümesi (tarayıcıda).
// i18n.js iki platformda aynı dosyadır; `gi://GLib` içe aktarması index.html'deki
// import map ile buraya yönlendirilir.

class DateTime {
    constructor(d) {
        this._d = d;
    }

    static new_from_unix_local(secs) {
        return new DateTime(new Date(secs * 1000));
    }

    static new_local(y, mo, d, h, mi, s) {
        return new DateTime(new Date(y, mo - 1, d, h, mi, s));
    }

    get_year() { return this._d.getFullYear(); }
    get_month() { return this._d.getMonth() + 1; }
    get_day_of_month() { return this._d.getDate(); }
    get_hour() { return this._d.getHours(); }
    get_minute() { return this._d.getMinutes(); }
    /** 1 = Pazartesi … 7 = Pazar */
    get_day_of_week() { return ((this._d.getDay() + 6) % 7) + 1; }
    to_unix() { return Math.floor(this._d.getTime() / 1000); }

    format(fmt) {
        const p = n => String(n).padStart(2, '0');
        return fmt.replace(/%H/g, p(this._d.getHours())).replace(/%M/g, p(this._d.getMinutes()))
            .replace(/%d/g, p(this._d.getDate())).replace(/%m/g, p(this._d.getMonth() + 1))
            .replace(/%Y/g, String(this._d.getFullYear()));
    }
}

function get_language_names() {
    return [...(navigator.languages ?? [navigator.language ?? 'en']), 'C'];
}

export default {DateTime, get_language_names};
