// Eşik, sıfırlanma, oturum ve tükenme bildirimleri (GNOME eklentisindeki
// _notify ile aynı kural; durum `notify-state` ayarında saklanır).

import {parseTime} from './i18n.js';

const SAME_PERIOD_MS = 30 * 60 * 1000;
const LEVEL = {normal: 0, warning: 1, critical: 2};
let permission = null;

async function send(title, body) {
    const n = window.__TAURI__?.notification;
    if (!n)
        return;
    if (permission === null) {
        permission = await n.isPermissionGranted();
        if (!permission)
            permission = (await n.requestPermission()) === 'granted';
    }
    if (permission)
        n.sendNotification({title, body});
}

export function notify(previous, snapshot, settings, s) {
    if (!snapshot)
        return;
    const wantThreshold = settings.get_boolean('notifications');
    const wantReset = settings.get_boolean('notify-reset');
    const wantExpired = settings.get_boolean('notify-expired');
    const wantForecast = settings.get_boolean('notify-forecast');
    const aheadMs = settings.get_int('notify-forecast-minutes') * 60000;
    if (!wantThreshold && !wantReset && !wantExpired && !wantForecast)
        return;
    let state;
    try {
        state = JSON.parse(settings.get_string('notify-state')) ?? {};
    } catch {
        state = {};
    }
    const now = Date.now();
    let changed = false;
    for (const p of snapshot.providers ?? []) {
        const before = previous?.providers?.find(x => x.id === p.id);
        if (wantExpired && p.status === 'expired' && before?.status === 'ok')
            send(`${p.name}: ${s.t('notify_expired')}`, s.status(p));
        if (p.status !== 'ok' || p.stale)
            continue;
        for (const w of p.windows ?? []) {
            if (w.estimated)
                continue;
            const level = LEVEL[w.severity] ?? 0;
            const key = `${p.id}|${w.id}`;
            const r = parseTime(w.resets_at);
            const resets = Number.isNaN(r) ? null : r;
            const prev = state[key];
            const same = prev !== undefined && (resets === null || prev.resets === null
                ? resets === prev.resets : Math.abs(prev.resets - resets) < SAME_PERIOD_MS);
            if (prev !== undefined && !same && prev.level >= 1 && wantReset)
                send(`${p.name}: ${s.windowLabel(w)} ${s.t('notify_reset')}`, s.percent(w.used_percent));
            const prevLevel = same ? prev.level : 0;
            if (level > prevLevel && wantThreshold) {
                const body = [w.resets_at ? s.resets(w.resets_at) : '', w.exhausts_at ? s.exhausts(w.exhausts_at) : '']
                    .filter(Boolean).join(' · ');
                send(s.notifyTitle(p, w), body);
            }
            let fc = same ? Boolean(prev.fc) : false;
            const out = parseTime(w.exhausts_at);
            if (wantForecast && !fc && !Number.isNaN(out) && out > now && out - now <= aheadMs &&
                (resets === null || out < resets) && w.used_percent < 100) {
                send(`${p.name}: ${s.windowLabel(w)} ${s.t('notify_forecast')}`,
                    s.fmt('notify_forecast_body', {when: s.when(out), reset: resets ? s.when(resets) : '—'}));
                fc = true;
            }
            const next = {level: Math.max(level, prevLevel), resets: same ? prev.resets : resets, fc};
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
        settings.set_string('notify-state', JSON.stringify(state));
}
