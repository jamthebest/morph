//! Migración única Isla -> Morph -> ID actual (config, datos, autostart). Idempotente: nunca pisa datos
//! nuevos y deja intacto lo antiguo salvo el autostart. Las funciones reciben rutas base.

use std::io;
use std::path::{Path, PathBuf};

/// IDs antiguos, del más reciente al más antiguo (cadena Isla -> Morph -> actual).
pub const LEGACY_APP_IDS: [&str; 2] = ["dev.jam.Morph", "dev.jam.Isla"];
pub const NEW_APP_ID: &str = "io.github.jamthebest.Morph";

/// Copia `src` en `dst` recursivamente (crea `dst`; no sigue symlinks a directorios).
fn copy_dir(src: &Path, dst: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else if ty.is_file() {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// `dir` contiene algún archivo regular (recursivo). Un directorio sin archivos cuenta como
/// ausente: cosmic-config crea `<id>/v1` vacío al leer la configuración.
fn has_files(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    rd.flatten().any(|e| match e.file_type() {
        Ok(t) if t.is_file() => true,
        Ok(t) if t.is_dir() => has_files(&e.path()),
        _ => false,
    })
}

/// Copia `old` a `new` solo si `old` existe y `new` no existe o no tiene archivos.
/// Devuelve si copió.
fn copy_if_absent(old: &Path, new: &Path) -> io::Result<bool> {
    if has_files(new) || !old.is_dir() {
        return Ok(false);
    }
    copy_dir(old, new)?;
    Ok(true)
}

/// `<config_home>/cosmic/<id antiguo>` -> `<config_home>/cosmic/<id nuevo>`: copia desde el
/// ID antiguo más reciente que exista; nunca pisa el directorio nuevo.
pub fn migrate_config(config_home: &Path) -> io::Result<bool> {
    let base = config_home.join("cosmic");
    for old in LEGACY_APP_IDS {
        if copy_if_absent(&base.join(old), &base.join(NEW_APP_ID))? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `<data_home>/isla` -> `<data_home>/morph`.
pub fn migrate_data(data_home: &Path) -> io::Result<bool> {
    copy_if_absent(&data_home.join("isla"), &data_home.join("morph"))
}

/// Autostart: los `.desktop` antiguos se borran siempre (lanzarían una segunda instancia);
/// el nuevo se escribe solo si no existía y había alguno antiguo. Devuelve si hizo algo.
pub fn migrate_autostart(dir: &Path, new_contents: &str) -> io::Result<bool> {
    let olds: Vec<PathBuf> = LEGACY_APP_IDS
        .iter()
        .map(|id| dir.join(format!("{id}.desktop")))
        .filter(|p| p.is_file())
        .collect();
    if olds.is_empty() {
        return Ok(false);
    }
    let new = dir.join(format!("{NEW_APP_ID}.desktop"));
    if !new.exists() {
        std::fs::write(&new, new_contents)?;
    }
    for old in olds {
        std::fs::remove_file(old)?;
    }
    Ok(true)
}

fn env_dir(name: &str) -> Option<PathBuf> {
    crate::envcompat::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn report(what: &str, r: io::Result<bool>) {
    match r {
        Ok(true) => eprintln!("morph: migración: {what} hecho"),
        Ok(false) => {}
        Err(e) => eprintln!("morph: migración: {what} falló: {e}"),
    }
}

/// Ejecuta las migraciones sobre las rutas reales (solo desde `main`).
pub fn run() {
    let home = env_dir("HOME").unwrap_or_default();
    let config_home = env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
    report("configuración", migrate_config(&config_home));

    // Con un directorio de datos forzado por entorno no hay nada que migrar.
    if env_dir("MORPH_DATA_DIR").is_none() {
        let data_home = env_dir("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"));
        report("datos", migrate_data(&data_home));
    }

    report(
        "autostart",
        migrate_autostart(
            &crate::settings::autostart_dir(),
            &crate::settings::autostart_contents(std::env::current_exe().ok().as_deref()),
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const ISLA: &str = "dev.jam.Isla";
    const MORPH: &str = "dev.jam.Morph";

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("morph-mig-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn config_copies_recursively_and_keeps_old() {
        let h = tmp("cfg");
        let old = h.join("cosmic").join(ISLA).join("v1");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("size_percent"), "150").unwrap();
        assert!(migrate_config(&h).unwrap());
        let new = h.join("cosmic").join(NEW_APP_ID).join("v1/size_percent");
        assert_eq!(std::fs::read_to_string(new).unwrap(), "150");
        assert!(old.join("size_percent").exists());
        assert!(!migrate_config(&h).unwrap());
        let _ = std::fs::remove_dir_all(h);
    }

    #[test]
    fn config_never_overwrites_new_or_runs_without_old() {
        let h = tmp("cfg2");
        assert!(!migrate_config(&h).unwrap());
        let old = h.join("cosmic").join(ISLA);
        let new = h.join("cosmic").join(NEW_APP_ID);
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(old.join("a"), "old").unwrap();
        std::fs::write(new.join("mio"), "new").unwrap();
        assert!(!migrate_config(&h).unwrap());
        assert!(!new.join("a").exists());
        let _ = std::fs::remove_dir_all(h);
    }

    #[test]
    fn data_migrates_once() {
        let h = tmp("data");
        std::fs::create_dir_all(h.join("isla")).unwrap();
        std::fs::write(h.join("isla/agenda.json"), "{}").unwrap();
        assert!(migrate_data(&h).unwrap());
        assert_eq!(
            std::fs::read_to_string(h.join("morph/agenda.json")).unwrap(),
            "{}"
        );
        std::fs::write(h.join("isla/agenda.json"), "changed").unwrap();
        assert!(!migrate_data(&h).unwrap());
        assert_eq!(
            std::fs::read_to_string(h.join("morph/agenda.json")).unwrap(),
            "{}"
        );
        let _ = std::fs::remove_dir_all(h);
    }

    #[test]
    fn autostart_moves_old_to_new() {
        let d = tmp("auto");
        let old = d.join("dev.jam.Isla.desktop");
        let new = d.join("io.github.jamthebest.Morph.desktop");
        std::fs::write(&old, "old").unwrap();
        assert!(migrate_autostart(&d, "NEW").unwrap());
        assert_eq!(std::fs::read_to_string(&new).unwrap(), "NEW");
        assert!(!old.exists());
        assert!(!migrate_autostart(&d, "NEW2").unwrap());
        // Con el nuevo ya presente no se pisa, pero el viejo se borra siempre.
        std::fs::write(&old, "old").unwrap();
        assert!(migrate_autostart(&d, "NEW3").unwrap());
        assert!(!old.exists());
        assert_eq!(std::fs::read_to_string(&new).unwrap(), "NEW");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn config_chain_prefers_morph_over_isla_and_keeps_new() {
        let h = tmp("chain");
        let base = h.join("cosmic");
        for (id, v) in [(ISLA, "isla"), (MORPH, "morph")] {
            std::fs::create_dir_all(base.join(id)).unwrap();
            std::fs::write(base.join(id).join("k"), v).unwrap();
        }
        assert!(migrate_config(&h).unwrap());
        assert_eq!(
            std::fs::read_to_string(base.join(NEW_APP_ID).join("k")).unwrap(),
            "morph"
        );
        assert!(!migrate_config(&h).unwrap());
        let _ = std::fs::remove_dir_all(h);
    }

    #[test]
    fn config_from_isla_only_and_existing_new_untouched() {
        let h = tmp("chain2");
        let base = h.join("cosmic");
        std::fs::create_dir_all(base.join(ISLA)).unwrap();
        std::fs::write(base.join(ISLA).join("k"), "isla").unwrap();
        assert!(migrate_config(&h).unwrap());
        assert_eq!(
            std::fs::read_to_string(base.join(NEW_APP_ID).join("k")).unwrap(),
            "isla"
        );
        // Con el nuevo existente y un Morph posterior, no se pisa.
        std::fs::create_dir_all(base.join(MORPH)).unwrap();
        std::fs::write(base.join(MORPH).join("k"), "morph").unwrap();
        assert!(!migrate_config(&h).unwrap());
        assert_eq!(
            std::fs::read_to_string(base.join(NEW_APP_ID).join("k")).unwrap(),
            "isla"
        );
        let _ = std::fs::remove_dir_all(h);
    }

    #[test]
    fn autostart_from_morph_rewrites_and_removes_all_legacy() {
        let d = tmp("auto2");
        let m = d.join(format!("{MORPH}.desktop"));
        let i = d.join(format!("{ISLA}.desktop"));
        std::fs::write(&m, "m").unwrap();
        std::fs::write(&i, "i").unwrap();
        assert!(migrate_autostart(&d, "NEW").unwrap());
        assert_eq!(
            std::fs::read_to_string(d.join(format!("{NEW_APP_ID}.desktop"))).unwrap(),
            "NEW"
        );
        assert!(!m.exists() && !i.exists());
        assert!(!migrate_autostart(&d, "X").unwrap());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn config_migrates_into_existing_empty_new_dir() {
        // cosmic-config crea `<id>/v1` vacío antes de que se migre.
        let h = tmp("empty");
        let base = h.join("cosmic");
        std::fs::create_dir_all(base.join(MORPH).join("v1")).unwrap();
        std::fs::write(base.join(MORPH).join("v1/k"), "viejo").unwrap();
        std::fs::create_dir_all(base.join(NEW_APP_ID).join("v1")).unwrap();
        assert!(migrate_config(&h).unwrap());
        assert_eq!(
            std::fs::read_to_string(base.join(NEW_APP_ID).join("v1/k")).unwrap(),
            "viejo"
        );
        // Ya con archivos, no se vuelve a copiar.
        std::fs::write(base.join(MORPH).join("v1/k"), "otro").unwrap();
        assert!(!migrate_config(&h).unwrap());
        let _ = std::fs::remove_dir_all(h);
    }

    #[test]
    fn has_files_is_recursive_and_ignores_empty_dirs() {
        let h = tmp("hasfiles");
        assert!(!has_files(&h.join("nope")));
        std::fs::create_dir_all(h.join("a/b")).unwrap();
        assert!(!has_files(&h));
        std::fs::write(h.join("a/b/f"), "x").unwrap();
        assert!(has_files(&h));
        let _ = std::fs::remove_dir_all(h);
    }

    #[test]
    fn autostart_removes_legacy_when_new_exists_and_never_writes_without_legacy() {
        let d = tmp("auto3");
        let new = d.join(format!("{NEW_APP_ID}.desktop"));
        assert!(!migrate_autostart(&d, "N").unwrap());
        assert!(!new.exists());
        std::fs::write(&new, "mio").unwrap();
        let m = d.join(format!("{MORPH}.desktop"));
        std::fs::write(&m, "m").unwrap();
        assert!(migrate_autostart(&d, "N").unwrap());
        assert!(!m.exists());
        assert_eq!(std::fs::read_to_string(&new).unwrap(), "mio");
        let _ = std::fs::remove_dir_all(d);
    }
}
