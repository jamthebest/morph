//! Piezas comunes de persistencia JSON (agenda y Shelf): lectura con respaldo
//! `.bad` de archivos dañados y escritura atómica con fsync.
use std::path::Path;

use serde::{Serialize, de::DeserializeOwned};

/// Problema al cargar: qué se loguea, qué se muestra en el aviso y si es
/// seguro seguir guardando (si no, el original puede seguir en disco).
#[derive(Debug, Clone, PartialEq)]
pub struct LoadIssue {
    pub log: String,
    pub subtitle: String,
    pub writable: bool,
}

/// Dato que se guarda: define el nombre en los mensajes ("la agenda", "el Shelf").
#[derive(Clone, Copy)]
pub enum Noun {
    Agenda,
    Shelf,
}

impl Noun {
    /// Nombre para el registro (siempre en español).
    fn name(self) -> &'static str {
        match self {
            Noun::Agenda => "la agenda",
            Noun::Shelf => "el Shelf",
        }
    }
    /// Nombre para mostrar al usuario, en el idioma activo.
    fn what(self) -> String {
        match self {
            Noun::Agenda => crate::fl!("store-noun-agenda"),
            Noun::Shelf => crate::fl!("store-noun-shelf"),
        }
    }
    fn damaged(self) -> &'static str {
        match self {
            Noun::Agenda => "dañada",
            Noun::Shelf => "dañado",
        }
    }
    fn capitalized(self) -> String {
        let mut c = self.name().chars();
        c.next()
            .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
            .unwrap_or_default()
    }
}

pub fn load_json<T: Default + DeserializeOwned>(
    dir: &Path,
    file: &str,
    noun: Noun,
) -> (T, Option<LoadIssue>) {
    load_json_at(
        dir,
        file,
        noun,
        &chrono::Local::now().format("%Y%m%d-%H%M%S").to_string(),
    )
}

/// Ausente: `T::default()`. Dañado: renombra a `<file>.bad` (o `.bad.<stamp>`
/// si ya existe) y devuelve el valor por defecto. Si no se pudo leer ni
/// respaldar, `writable` es `false`.
pub fn load_json_at<T: Default + DeserializeOwned>(
    dir: &Path,
    file: &str,
    noun: Noun,
    stamp: &str,
) -> (T, Option<LoadIssue>) {
    let path = dir.join(file);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (T::default(), None),
        Err(e) => {
            return (
                T::default(),
                Some(LoadIssue {
                    log: format!("No se pudo leer {}: {e}", noun.name()),
                    subtitle: crate::fl!("store-read-failed", what = noun.what()),
                    writable: false,
                }),
            );
        }
    };
    match serde_json::from_slice::<T>(&bytes) {
        Ok(v) => (v, None),
        Err(e) => {
            let mut name = format!("{file}.bad");
            if dir.join(&name).exists() {
                name = format!("{file}.bad.{stamp}");
            }
            let dmg = noun.damaged();
            let issue = match std::fs::rename(&path, dir.join(&name)) {
                Ok(()) => LoadIssue {
                    log: format!("{} estaba {dmg} ({e}); copia en {name}", noun.capitalized()),
                    subtitle: crate::fl!("store-backup-saved", name = name.as_str()),
                    writable: true,
                },
                Err(re) => LoadIssue {
                    log: format!(
                        "{} estaba {dmg} ({e}) y no se pudo respaldar: {re}",
                        noun.capitalized()
                    ),
                    subtitle: match noun {
                        Noun::Agenda => crate::fl!("store-backup-failed-agenda"),
                        Noun::Shelf => crate::fl!("store-backup-failed-shelf"),
                    },
                    writable: false,
                },
            };
            (T::default(), Some(issue))
        }
    }
}

/// Escribe `<dir>/<file>.tmp`, lo sincroniza a disco y lo renombra a `file`
/// (luego sincroniza el directorio, si se puede).
pub fn write_atomic(dir: &Path, file: &str, bytes: &[u8], noun: Noun) -> Result<(), String> {
    use std::io::Write;
    std::fs::create_dir_all(dir).map_err(|e| {
        crate::fl!(
            "store-create-failed",
            path = dir.display().to_string(),
            err = e.to_string()
        )
    })?;
    let tmp = dir.join(format!("{file}.tmp"));
    let werr = |e: std::io::Error| {
        crate::fl!(
            "store-write-failed",
            what = noun.what(),
            err = e.to_string()
        )
    };
    let mut f = std::fs::File::create(&tmp).map_err(werr)?;
    f.write_all(bytes).map_err(werr)?;
    f.sync_all().map_err(werr)?;
    drop(f);
    std::fs::rename(&tmp, dir.join(file))
        .map_err(|e| crate::fl!("store-save-failed", what = noun.what(), err = e.to_string()))?;
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

/// Serializa y guarda con `write_atomic`.
pub fn save_json<T: Serialize>(dir: &Path, file: &str, v: &T, noun: Noun) -> Result<(), String> {
    let json = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    write_atomic(dir, file, json.as_bytes(), noun)
}
