//! Persistencia atómica de la agenda en JSON.
use std::path::{Path, PathBuf};

use super::Agenda;

const FILE: &str = "agenda.json";

/// `$MORPH_DATA_DIR`, o `$XDG_DATA_HOME/morph`, o `~/.local/share/morph`.
pub fn data_dir() -> PathBuf {
    let var = |k: &str| crate::envcompat::var_os(k).filter(|v| !v.is_empty());
    if let Some(d) = var("MORPH_DATA_DIR") {
        return PathBuf::from(d);
    }
    if let Some(x) = var("XDG_DATA_HOME") {
        return PathBuf::from(x).join("morph");
    }
    let home = var("HOME").map(PathBuf::from).unwrap_or_default();
    home.join(".local/share/morph")
}

pub use crate::store_util::LoadIssue;
use crate::store_util::{Noun, load_json, save_json};

const NOUN: Noun = Noun::Agenda;

/// Carga la agenda. Ausente: valor por defecto. Dañado (incluye UTF-8 no
/// válido): lo renombra a `agenda.json.bad` (o `.bad.<fecha>` si ya existe) y
/// devuelve el valor por defecto. Si no se pudo leer ni respaldar,
/// `writable` es `false` para no pisar el archivo original.
pub fn load(dir: &Path) -> (Agenda, Option<LoadIssue>) {
    load_json(dir, FILE, NOUN)
}

#[cfg(test)]
fn load_at(dir: &Path, stamp: &str) -> (Agenda, Option<LoadIssue>) {
    crate::store_util::load_json_at(dir, FILE, NOUN, stamp)
}

/// Guarda de forma atómica y durable (ver `store_util::write_atomic`).
pub fn save(dir: &Path, a: &Agenda) -> Result<(), String> {
    save_json(dir, FILE, a, NOUN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agenda::Kind;
    use chrono::{NaiveDate, NaiveTime};
    use std::sync::atomic::{AtomicU32, Ordering};

    fn tmp_dir(tag: &str) -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let p = std::env::temp_dir().join(format!(
            "morph-test-{}-{}-{tag}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn ida_y_vuelta() {
        let dir = tmp_dir("rt");
        let mut a = Agenda::default();
        a.add(
            "t".into(),
            Kind::Task {
                date: NaiveDate::from_ymd_opt(2026, 10, 30),
                time: NaiveTime::from_hms_opt(9, 0, 0),
                duration_min: Some(30),
                remind_start: true,
                remind_end: true,
                done: false,
            },
        );
        a.add(
            "al".into(),
            Kind::Alarm {
                time: NaiveTime::from_hms_opt(7, 30, 0).unwrap(),
                days: [true, false, true, false, true, false, false],
                once_on: None,
                enabled: true,
            },
        );
        a.last_check = NaiveDate::from_ymd_opt(2026, 10, 30).and_then(|d| d.and_hms_opt(8, 0, 0));
        save(&dir, &a).unwrap();
        assert!(!dir.join("agenda.json.tmp").exists());
        let (b, warn) = load(&dir);
        assert_eq!(a, b);
        assert!(warn.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sin_archivo_es_default() {
        let dir = tmp_dir("none");
        let (a, warn) = load(&dir);
        assert_eq!(a, Agenda::default());
        assert!(warn.is_none());
    }

    #[test]
    fn danado_se_respalda() {
        let dir = tmp_dir("bad");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("agenda.json"), "{ basura").unwrap();
        let (a, warn) = load(&dir);
        assert_eq!(a, Agenda::default());
        assert!(warn.is_some());
        assert!(dir.join("agenda.json.bad").exists());
        assert!(!dir.join("agenda.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn utf8_invalido_se_respalda() {
        let dir = tmp_dir("utf8");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("agenda.json"), [0x7b, 0xff, 0xfe, 0x80]).unwrap();
        let (a, warn) = load(&dir);
        assert_eq!(a, Agenda::default());
        assert!(warn.is_some());
        assert!(dir.join("agenda.json.bad").exists());
        assert!(!dir.join("agenda.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn segundo_dano_no_pisa_el_primer_respaldo() {
        let dir = tmp_dir("bad2");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("agenda.json.bad"), "primero").unwrap();
        std::fs::write(dir.join("agenda.json"), "{ segundo").unwrap();
        let (_, warn) = load_at(&dir, "20261030-101500");
        let warn = warn.unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("agenda.json.bad")).unwrap(),
            "primero"
        );
        let name = "agenda.json.bad.20261030-101500";
        assert_eq!(
            std::fs::read_to_string(dir.join(name)).unwrap(),
            "{ segundo"
        );
        assert!(warn.subtitle.contains(name), "{}", warn.subtitle);
        assert!(warn.writable);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn danado_respaldado_es_escribible() {
        let dir = tmp_dir("bad3");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("agenda.json"), "{ x").unwrap();
        let (_, warn) = load(&dir);
        let warn = warn.unwrap();
        assert!(warn.writable);
        assert!(warn.subtitle.contains("agenda.json.bad"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ilegible_bloquea_el_guardado() {
        crate::i18n::apply_for_test("es");
        let dir = tmp_dir("unreadable");
        // Un directorio llamado agenda.json: leerlo falla con un error que no es NotFound.
        std::fs::create_dir_all(dir.join("agenda.json")).unwrap();
        let (a, warn) = load(&dir);
        assert_eq!(a, Agenda::default());
        let warn = warn.unwrap();
        assert!(!warn.writable);
        assert!(!warn.subtitle.contains("copia"), "{}", warn.subtitle);
        assert_eq!(
            warn.subtitle,
            "No se pudo leer la agenda; no se guardarán cambios"
        );
        crate::i18n::apply_for_test("en");
        let (_, warn) = load(&dir);
        assert_eq!(
            warn.unwrap().subtitle,
            "Could not read the agenda; changes will not be saved"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn respaldo_fallido_bloquea_el_guardado() {
        crate::i18n::apply_for_test("es");
        let dir = tmp_dir("nobackup");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("agenda.json"), "{ x").unwrap();
        // Destinos ocupados por directorios no vacíos: el rename falla.
        for n in ["agenda.json.bad", "agenda.json.bad.S"] {
            std::fs::create_dir_all(dir.join(n).join("x")).unwrap();
        }
        let (_, warn) = load_at(&dir, "S");
        let warn = warn.unwrap();
        assert!(!warn.writable);
        assert!(warn.subtitle.contains("No se pudo"), "{}", warn.subtitle);
        assert!(dir.join("agenda.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
