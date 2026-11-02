//! Migración única Isla -> Morph (config, datos, autostart). Idempotente: nunca pisa datos
//! nuevos y deja intacto lo antiguo salvo el autostart. Las funciones reciben rutas base.

use std::io;
use std::path::{Path, PathBuf};

pub const OLD_APP_ID: &str = "dev.jam.Isla";
pub const NEW_APP_ID: &str = "dev.jam.Morph";

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

/// Copia `old` a `new` solo si `old` existe y `new` no. Devuelve si copió.
fn copy_if_absent(old: &Path, new: &Path) -> io::Result<bool> {
    if new.exists() || !old.is_dir() {
        return Ok(false);
    }
    copy_dir(old, new)?;
    Ok(true)
}

/// `<config_home>/cosmic/dev.jam.Isla` -> `<config_home>/cosmic/dev.jam.Morph`.
pub fn migrate_config(config_home: &Path) -> io::Result<bool> {
    let base = config_home.join("cosmic");
    copy_if_absent(&base.join(OLD_APP_ID), &base.join(NEW_APP_ID))
}

/// `<data_home>/isla` -> `<data_home>/morph`.
pub fn migrate_data(data_home: &Path) -> io::Result<bool> {
    copy_if_absent(&data_home.join("isla"), &data_home.join("morph"))
}

/// Autostart: si existe el `.desktop` antiguo y no el nuevo, escribe el nuevo y borra el viejo.
pub fn migrate_autostart(dir: &Path, new_contents: &str) -> io::Result<bool> {
    let old = dir.join(format!("{OLD_APP_ID}.desktop"));
    let new = dir.join(format!("{NEW_APP_ID}.desktop"));
    if !old.is_file() || new.exists() {
        return Ok(false);
    }
    std::fs::write(&new, new_contents)?;
    std::fs::remove_file(&old)?;
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

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("morph-mig-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn config_copies_recursively_and_keeps_old() {
        let h = tmp("cfg");
        let old = h.join("cosmic").join(OLD_APP_ID).join("v1");
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
        let old = h.join("cosmic").join(OLD_APP_ID);
        let new = h.join("cosmic").join(NEW_APP_ID);
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(old.join("a"), "old").unwrap();
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
        let new = d.join("dev.jam.Morph.desktop");
        std::fs::write(&old, "old").unwrap();
        assert!(migrate_autostart(&d, "NEW").unwrap());
        assert_eq!(std::fs::read_to_string(&new).unwrap(), "NEW");
        assert!(!old.exists());
        assert!(!migrate_autostart(&d, "NEW2").unwrap());
        // Con el nuevo ya presente no se toca ni se borra el viejo.
        std::fs::write(&old, "old").unwrap();
        assert!(!migrate_autostart(&d, "NEW3").unwrap());
        assert!(old.exists());
        assert_eq!(std::fs::read_to_string(&new).unwrap(), "NEW");
        let _ = std::fs::remove_dir_all(d);
    }
}
