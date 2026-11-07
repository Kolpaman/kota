//! Loglara sızmayan gizli değer sarmalayıcısı.

use std::fmt;

/// Token gibi gizli bir değer. `Debug` çıktısı her zaman maskelidir;
/// `Display` ve `Serialize` bilerek uygulanmamıştır.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Ham değeri yalnızca HTTP başlığına koyarken kullanın.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_is_redacted() {
        let s = Secret::new("sk-ant-oat01-supersecret");
        let dbg = format!("{s:?}");
        assert!(!dbg.contains("supersecret"));
        assert_eq!(dbg, "Secret(***)");
    }
}
