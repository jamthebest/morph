//! Variables de entorno `MORPH_*` con respaldo a los nombres antiguos `ISLA_*`.

use std::ffi::OsString;

/// Resuelve `name` con `get`; si es `MORPH_*` y no está definida, prueba `ISLA_*`.
fn lookup(name: &str, get: impl Fn(&str) -> Option<OsString>) -> Option<OsString> {
    get(name).or_else(|| {
        name.strip_prefix("MORPH_")
            .and_then(|rest| get(&format!("ISLA_{rest}")))
    })
}

/// `std::env::var_os`, con respaldo a `ISLA_*` para las variables `MORPH_*`.
pub fn var_os(name: &str) -> Option<OsString> {
    lookup(name, |k| std::env::var_os(k))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let m: HashMap<String, OsString> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        move |k| m.get(k).cloned()
    }

    #[test]
    fn new_name_wins() {
        let g = env(&[("MORPH_DEBUG", "1"), ("ISLA_DEBUG", "old")]);
        assert_eq!(lookup("MORPH_DEBUG", g), Some("1".into()));
    }

    #[test]
    fn falls_back_to_old_name() {
        let g = env(&[("ISLA_DATA_DIR", "/x")]);
        assert_eq!(lookup("MORPH_DATA_DIR", g), Some("/x".into()));
    }

    #[test]
    fn unset_is_none_and_other_names_do_not_fall_back() {
        assert_eq!(lookup("MORPH_MUTE", env(&[])), None);
        assert_eq!(lookup("HOME", env(&[("ISLA_HOME", "x")])), None);
    }
}
