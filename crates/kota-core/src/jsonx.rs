//! Toleranslı JSON yardımcıları.
//!
//! Okunan endpoint'lerin çoğu belgelenmemiş ve şemaları haber vermeden
//! değişiyor. Bu yüzden alanlar katı `serde` yapıları yerine bu yardımcılarla
//! okunur: sayı yerine sayısal metin, saniye yerine milisaniye gibi küçük
//! farklar sessizce tolere edilir.

use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;

/// Sayı veya sayısal metin → `f64` (sonsuz/NaN reddedilir).
pub fn f64_of(v: &Value) -> Option<f64> {
    let x = match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    };
    x.filter(|x| x.is_finite())
}

/// Negatif olmayan tam sayı (ondalıklı gelirse aşağı yuvarlanır).
pub fn u64_of(v: &Value) -> Option<u64> {
    match v {
        Value::Number(n) => n
            .as_u64()
            .or_else(|| n.as_f64().filter(|f| f.is_finite() && *f >= 0.0).map(|f| f as u64)),
        Value::String(s) => s.trim().parse::<u64>().ok(),
        _ => None,
    }
}

/// Boş olmayan metin.
pub fn str_of(v: &Value) -> Option<&str> {
    v.as_str().map(str::trim).filter(|s| !s.is_empty())
}

/// RFC 3339 metni ya da Unix zamanı (saniye veya milisaniye) → UTC zaman.
pub fn time_of(v: &Value) -> Option<DateTime<Utc>> {
    match v {
        Value::String(s) => {
            let s = s.trim();
            if let Ok(t) = DateTime::parse_from_rfc3339(s) {
                return Some(t.with_timezone(&Utc));
            }
            s.parse::<f64>().ok().and_then(epoch_to_time)
        }
        Value::Number(n) => n.as_f64().and_then(epoch_to_time),
        _ => None,
    }
}

/// 1e12'den büyükse milisaniye, değilse saniye kabul edilir.
pub fn epoch_to_time(x: f64) -> Option<DateTime<Utc>> {
    if !x.is_finite() || x <= 0.0 {
        return None;
    }
    let millis = if x > 1e12 { x } else { x * 1000.0 };
    Utc.timestamp_millis_opt(millis as i64).single()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numbers_and_strings() {
        assert_eq!(f64_of(&json!(21.0)), Some(21.0));
        assert_eq!(f64_of(&json!("3.5")), Some(3.5));
        assert_eq!(f64_of(&json!(null)), None);
        assert_eq!(u64_of(&json!(12)), Some(12));
        assert_eq!(u64_of(&json!(12.9)), Some(12));
        assert_eq!(u64_of(&json!(-1)), None);
        assert_eq!(str_of(&json!("  ")), None);
    }

    #[test]
    fn times() {
        let iso = time_of(&json!("2026-11-06T20:19:59.813876+00:00")).unwrap();
        let expected = Utc.with_ymd_and_hms(2026, 11, 6, 20, 19, 59).unwrap();
        assert_eq!(iso.timestamp(), expected.timestamp());
        assert_eq!(
            time_of(&json!("2026-11-06T23:19:59+03:00")).unwrap().timestamp(),
            expected.timestamp()
        );
        let secs = time_of(&json!(1_796_328_666)).unwrap();
        let millis = time_of(&json!(1_796_328_666_000_i64)).unwrap();
        assert_eq!(secs, millis);
        assert_eq!(time_of(&json!(0)), None);
        assert_eq!(time_of(&json!("garbage")), None);
    }
}
