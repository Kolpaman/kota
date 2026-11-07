// Kota — ilerleme çizgileri: yuvarlak iz, degrade dolgu ve tempo işareti.
//
// Tempo işareti, pencerenin geçen süresine göre "eşit hızla kullansaydın
// şu an nerede olurdun" noktasını gösterir: dolgu işareti geçtiyse
// beklenenden hızlı tüketiyorsun demektir.

import Cairo from 'cairo';
import St from 'gi://St';

import {parseTime} from './i18n.js';
import {hexToRgba, ink} from './widgets.js';

/** Pencerenin geçen süre oranı (0–1); bilinmiyorsa `null`. */
export function paceOf(w, nowMs = Date.now()) {
    const reset = parseTime(w?.resets_at);
    const secs = w?.window_seconds;
    if (Number.isNaN(reset) || !secs)
        return null;
    const start = reset - secs * 1000;
    return Math.max(0, Math.min(1, (nowMs - start) / (secs * 1000)));
}

function lighten(hex, amount) {
    const [r, g, b] = hexToRgba(hex);
    const mix = c => Math.round((c + (1 - c) * amount) * 255).toString(16).padStart(2, '0');
    return `#${mix(r)}${mix(g)}${mix(b)}`;
}

/** Tam yuvarlatılmış dikdörtgen (kısa kenarın yarısı kadar yarıçap). */
function roundedBar(cr, x, y, w, h) {
    const r = Math.min(h, w) / 2;
    cr.newSubPath();
    cr.arc(x + w - r, y + r, r, -Math.PI / 2, 0);
    cr.arc(x + w - r, y + h - r, r, 0, Math.PI / 2);
    cr.arc(x + r, y + h - r, r, Math.PI / 2, Math.PI);
    cr.arc(x + r, y + r, r, Math.PI, Math.PI * 1.5);
    cr.closePath();
}

/**
 * Yuvarlak uçlu ilerleme çizgisi (Cairo ile çizilir: uçlar her doluluk
 * oranında tam yarım daire kalır).
 * @param {object} o
 * @param {number} o.fraction 0–1 (1'i aşan değerler kırpılır)
 * @param {string} o.color dolgu rengi (#rrggbb)
 * @param {number|null} [o.pace] tempo işareti konumu (0–1)
 * @param {number} [o.height] çizgi kalınlığı (px)
 * @param {number[]} [o.track] iz rengi [r, g, b, a] (0–1)
 */
export function progressLine({fraction, color, pace = null, height = 6, track = null}) {
    track ??= ink(0.09);
    const pad = pace !== null ? 3 : 1;
    const f = Math.max(0, Math.min(1, fraction ?? 0));
    const area = new St.DrawingArea({
        style_class: 'kota-progress',
        x_expand: true,
        height: height + pad * 2,
    });
    const [r, g, b] = hexToRgba(color);
    const [lr, lg, lb] = hexToRgba(lighten(color, 0.38));
    area.connect('repaint', a => {
        const cr = a.get_context();
        const [w, hh] = a.get_surface_size();
        const scale = hh / (height + pad * 2);
        const h = height * scale;
        const y = (hh - h) / 2;
        // İz
        roundedBar(cr, 0, y, w, h);
        cr.setSourceRGBA(...track);
        cr.fill();
        if (f > 0) {
            const fw = Math.max(h, w * f);
            // Hafif hale
            roundedBar(cr, -0.5 * scale, y - 0.5 * scale, fw + scale, h + scale);
            cr.setSourceRGBA(r, g, b, 0.18);
            cr.fill();
            // Degrade dolgu
            const grad = new Cairo.LinearGradient(0, 0, fw, 0);
            grad.addColorStopRGBA(0, lr, lg, lb, 1);
            grad.addColorStopRGBA(1, r, g, b, 1);
            roundedBar(cr, 0, y, fw, h);
            cr.setSource(grad);
            cr.fillPreserve();
            // Üst parlaklık (kavisli cam etkisi)
            const gloss = new Cairo.LinearGradient(0, y, 0, y + h);
            gloss.addColorStopRGBA(0, 1, 1, 1, 0.28);
            gloss.addColorStopRGBA(0.55, 1, 1, 1, 0.0);
            cr.setSource(gloss);
            cr.fill();
        }
        if (pace !== null) {
            const mw = 3 * scale;
            const mx = Math.max(scale, Math.min(w - mw - scale, w * pace - mw / 2));
            roundedBar(cr, mx - scale, 0, mw + 2 * scale, hh);
            cr.setSourceRGBA(0.06, 0.07, 0.09, 0.75);
            cr.fill();
            roundedBar(cr, mx, scale, mw, hh - 2 * scale);
            cr.setSourceRGBA(1, 1, 1, 0.95);
            cr.fill();
        }
        cr.$dispose();
    });
    return area;
}

/** Tempoya göre fark (yüzde puanı): pozitifse beklenenden hızlı. */
export function paceDelta(w, nowMs = Date.now()) {
    const p = paceOf(w, nowMs);
    if (p === null || w.estimated)
        return null;
    return w.used_percent - p * 100;
}
