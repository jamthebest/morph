//! Traducciones (Fluent): cargador global, macro `fl!` y selección de idioma.
//!
//! Uso: `crate::fl!("clave")` o `crate::fl!("clave", n = 3, host = "x")`. Las claves van en
//! `i18n/en/morph.ftl` y `i18n/es/morph.ftl` (mismas claves en ambos; hay una prueba de paridad).
//! En pruebas el cargador es por hilo, así que `apply_for_test("es")` no afecta a otras pruebas.

use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
use i18n_embed::{DesktopLanguageRequester, LanguageLoader};
use rust_embed::RustEmbed;

use crate::config::Language;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

/// Sin marcas de aislamiento bidi alrededor de las variables (el texto sale limpio).
/// Afecta solo a los paquetes ya cargados, así que hay que repetirlo tras cada carga.
fn no_isolation(loader: &FluentLanguageLoader) {
    loader.set_use_isolating(false);
}

fn new_loader() -> FluentLanguageLoader {
    let loader: FluentLanguageLoader = fluent_language_loader!();
    loader
        .load_fallback_language(&Localizations)
        .expect("no se pudo cargar el idioma de respaldo");
    no_isolation(&loader);
    loader
}

/// Cargador global (en pruebas, uno por hilo: cada prueba corre en su propio hilo).
#[cfg(not(test))]
pub fn loader() -> &'static FluentLanguageLoader {
    static LOADER: std::sync::LazyLock<FluentLanguageLoader> = std::sync::LazyLock::new(new_loader);
    &LOADER
}

#[cfg(test)]
pub fn loader() -> &'static FluentLanguageLoader {
    thread_local! {
        static LOADER: &'static FluentLanguageLoader = Box::leak(Box::new(new_loader()));
    }
    LOADER.with(|l| *l)
}

#[macro_export]
macro_rules! fl {
    ($message_id:literal) => {
        i18n_embed_fl::fl!($crate::i18n::loader(), $message_id)
    };
    ($message_id:literal, $($args:tt)*) => {
        i18n_embed_fl::fl!($crate::i18n::loader(), $message_id, $($args)*)
    };
}

/// Código de idioma efectivo (`"en"` o `"es"`). `System` toma el primero de `requested`
/// que tenga traducción; si ninguno la tiene, inglés.
pub fn resolve(lang: Language, requested: &[String]) -> &'static str {
    match lang {
        Language::En => "en",
        Language::Es => "es",
        Language::System => requested
            .iter()
            .find_map(|r| {
                let base = r.split(['-', '_']).next().unwrap_or("").to_lowercase();
                match base.as_str() {
                    "en" => Some("en"),
                    "es" => Some("es"),
                    _ => None,
                }
            })
            .unwrap_or("en"),
    }
}

/// Idiomas pedidos por el escritorio (`LANGUAGE`, `LC_ALL`, `LANG`…), como cadenas.
fn desktop_languages() -> Vec<String> {
    DesktopLanguageRequester::requested_languages()
        .iter()
        .map(|l| l.to_string())
        .collect()
}

fn select(code: &str) {
    let id = code.parse().expect("código de idioma válido");
    if let Err(e) = loader().load_languages(&Localizations, &[id]) {
        eprintln!("morph: no se pudo cargar el idioma {code}: {e}");
    }
    no_isolation(loader());
}

/// Activa el idioma elegido; las vistas lo recogen en el siguiente dibujado.
pub fn apply(lang: Language) {
    let requested = if lang == Language::System {
        desktop_languages()
    } else {
        Vec::new()
    };
    select(resolve(lang, &requested));
}

/// Fija un idioma por código (`"en"`, `"es"`) sin mirar el locale de la máquina.
#[cfg(test)]
pub fn apply_for_test(code: &str) {
    select(code);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn keys(lang: &str) -> BTreeSet<String> {
        let f = Localizations::get(&format!("{lang}/morph.ftl")).expect("falta el .ftl");
        std::str::from_utf8(&f.data)
            .unwrap()
            .lines()
            .filter(|l| l.starts_with(|c: char| c.is_ascii_lowercase()))
            .filter_map(|l| l.split_once('=').map(|(k, _)| k.trim().to_string()))
            .collect()
    }

    #[test]
    fn en_and_es_have_the_same_keys() {
        let (en, es) = (keys("en"), keys("es"));
        let only_en: Vec<_> = en.difference(&es).collect();
        let only_es: Vec<_> = es.difference(&en).collect();
        assert!(only_en.is_empty(), "solo en en: {only_en:?}");
        assert!(only_es.is_empty(), "solo en es: {only_es:?}");
    }

    #[test]
    fn system_resolves_requested_languages() {
        let r = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(resolve(Language::System, &r(&["es-MX"])), "es");
        assert_eq!(resolve(Language::System, &r(&["fr"])), "en");
        assert_eq!(resolve(Language::System, &r(&["fr", "es"])), "es");
        assert_eq!(resolve(Language::System, &[]), "en");
        assert_eq!(resolve(Language::System, &r(&["en-US"])), "en");
        assert_eq!(resolve(Language::System, &r(&["es_ES"])), "es");
        assert_eq!(resolve(Language::System, &r(&["es-ES"])), "es");
        assert_eq!(resolve(Language::Es, &r(&["en-US"])), "es");
        assert_eq!(resolve(Language::En, &r(&["es"])), "en");
    }

    #[test]
    fn tests_pin_their_own_language() {
        apply_for_test("es");
        assert_eq!(crate::fl!("general-language"), "Language / Idioma");
        assert_eq!(loader().current_language().language.as_str(), "es");
    }
}
