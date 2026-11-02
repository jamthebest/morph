//! Shelf: lista persistente de rutas soltadas en la isla (nunca copia archivos).

pub mod thumb;
pub mod view;

use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::store_util::{LoadIssue, Noun, load_json, save_json};

const FILE: &str = "shelf.json";
const NOUN: Noun = Noun::Shelf;

/// Datos del sistema de archivos cacheados para no consultarlo en cada fotograma
/// (se rellenan al cargar, al soltar y al abrir la página).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Info {
    pub is_dir: bool,
    pub size: Option<u64>,
    pub mtime: Option<std::time::SystemTime>,
    pub thumbnailable: bool,
    /// (nombre específico, genérico de respaldo); ver `icon_name`.
    pub icon: (String, &'static str),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub path: PathBuf,
    pub added: NaiveDateTime,
    #[serde(skip)]
    pub info: Info,
}

impl Entry {
    fn new(path: PathBuf, added: NaiveDateTime) -> Self {
        let mut e = Entry {
            path,
            added,
            info: Info::default(),
        };
        e.refresh();
        e
    }

    /// Vuelve a consultar el disco (tamaño, fecha, tipo, ícono). Devuelve si existe.
    pub fn refresh(&mut self) -> bool {
        match std::fs::metadata(&self.path) {
            Ok(m) => {
                self.info = Info {
                    is_dir: m.is_dir(),
                    size: m.is_file().then_some(m.len()),
                    mtime: m.modified().ok(),
                    thumbnailable: is_thumbnailable(&self.path),
                    icon: icon_name(&self.path),
                };
                true
            }
            Err(e) => !is_gone(&self.path, &e),
        }
    }
}

/// ¿El error de metadatos significa que el archivo ya no existe? Solo `NotFound`
/// (y no si cuelga de una unidad desmontada); EACCES, EIO, ESTALE, ELOOP… conservan.
fn is_gone(path: &Path, err: &std::io::Error) -> bool {
    err.kind() == std::io::ErrorKind::NotFound && !under_unmounted_root(path)
}

/// Heurística de unidad desmontada: la ruta cuelga de /media, /run/media o /mnt y
/// el primer ancestro existente no llega más allá de /media/<usuario> (o /mnt).
fn under_unmounted_root(path: &Path) -> bool {
    let Some(first) = path.ancestors().find(|a| a.exists()) else {
        return false;
    };
    if path == first {
        return false;
    }
    let depth = first.components().count(); // "/" cuenta 1
    [("/media", 1), ("/run/media", 1), ("/mnt", 0)]
        .iter()
        .any(|(base, user_levels)| {
            let base = Path::new(base);
            path.starts_with(base) && depth <= base.components().count() + user_levels
        })
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Shelf {
    pub entries: Vec<Entry>,
}

impl Shelf {
    /// Añade en orden de llegada, ignorando rutas ya presentes (o repetidas en
    /// `paths`). Devuelve cuántas se añadieron.
    pub fn add_paths(&mut self, paths: Vec<PathBuf>, now: NaiveDateTime) -> usize {
        let mut n = 0;
        for path in paths {
            // Una ruta no UTF-8 no se puede serializar y rompería todos los guardados.
            if path.to_str().is_none() {
                eprintln!("morph: Shelf: ruta no UTF-8 ignorada: {}", path.display());
                continue;
            }
            if self.entries.iter().any(|e| e.path == path) {
                continue;
            }
            self.entries.push(Entry::new(path, now));
            n += 1;
        }
        n
    }

    /// Quita la entrada del Shelf (nunca toca el archivo).
    pub fn remove(&mut self, path: &Path) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.path != path);
        self.entries.len() != before
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Refresca los datos de cada entrada y quita las que ya no existen en disco
    /// (p. ej. porque se movieron a otra carpeta). Devuelve cuántas quitó.
    pub fn prune_missing(&mut self) -> usize {
        let before = self.entries.len();
        self.entries.retain_mut(Entry::refresh);
        before - self.entries.len()
    }

    pub fn present_paths(&self) -> Vec<PathBuf> {
        self.entries.iter().map(|e| e.path.clone()).collect()
    }
}

pub fn display_name(p: &Path) -> String {
    match p.file_name() {
        Some(n) => n.to_string_lossy().into_owned(),
        None => p.to_string_lossy().into_owned(),
    }
}

/// (nombre freedesktop específico, genérico de respaldo). El genérico de PDF
/// es `x-office-document`; lo desconocido o sin extensión, `text-x-generic`.
pub fn icon_name(p: &Path) -> (String, &'static str) {
    if p.is_dir() {
        return ("folder".into(), "folder");
    }
    let Some(mime) = mime_guess::from_path(p).first() else {
        return ("text-x-generic".into(), "text-x-generic");
    };
    let generic = match (mime.type_().as_str(), mime.subtype().as_str()) {
        ("image", _) => "image-x-generic",
        ("audio", _) => "audio-x-generic",
        ("video", _) => "video-x-generic",
        ("text", _) => "text-x-generic",
        ("application", "pdf") => "x-office-document",
        _ => "text-x-generic",
    };
    (mime.essence_str().replace('/', "-"), generic)
}

const THUMB_MAX_BYTES: u64 = 20 * 1024 * 1024;

pub fn is_thumbnailable(p: &Path) -> bool {
    let ext_ok = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| matches!(e.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp"));
    ext_ok
        && std::fs::metadata(p)
            .map(|m| m.is_file() && m.len() <= THUMB_MAX_BYTES)
            .unwrap_or(false)
}

/// No poda: el llamador usa `prune_missing` (y guarda si quitó algo).
/// Ausente: vacío. Dañado: respaldo `.bad` y vacío. Si no se pudo leer o
/// respaldar, `writable == false` y el llamador no debe guardar encima.
/// (Se reutiliza `LoadIssue` de la agenda en vez de `Option<String>` para
/// conservar ese bloqueo de guardado.)
pub fn load(dir: &Path) -> (Shelf, Option<LoadIssue>) {
    load_json::<Shelf>(dir, FILE, NOUN)
}

pub fn save(dir: &Path, s: &Shelf) -> Result<(), String> {
    save_json(dir, FILE, s, NOUN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn tmp_dir(tag: &str) -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let p = std::env::temp_dir().join(format!(
            "morph-shelf-test-{}-{}-{tag}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn now() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 30)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap()
    }

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn add_ignora_duplicados_y_cuenta() {
        let mut s = Shelf::default();
        assert_eq!(s.add_paths(vec![p("/a"), p("/b"), p("/a")], now()), 2);
        assert_eq!(s.add_paths(vec![p("/b"), p("/c")], now()), 1);
        let order: Vec<_> = s.entries.iter().map(|e| e.path.clone()).collect();
        assert_eq!(order, vec![p("/a"), p("/b"), p("/c")]);
    }

    #[test]
    fn rutas_no_utf8_se_ignoran() {
        use std::os::unix::ffi::OsStrExt;
        let bad = PathBuf::from(std::ffi::OsStr::from_bytes(b"/tmp/\xff\xfe.txt"));
        let mut s = Shelf::default();
        assert_eq!(s.add_paths(vec![bad, p("/ok")], now()), 1);
        assert_eq!(s.entries.len(), 1);
        let dir = tmp_dir("utf8");
        save(&dir, &s).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn info_cacheada() {
        let dir = tmp_dir("info");
        let f = dir.join("a.png");
        std::fs::write(&f, "12345").unwrap();
        let mut s = Shelf::default();
        s.add_paths(vec![f, dir.clone()], now());
        assert_eq!(s.entries[0].info.size, Some(5));
        assert!(s.entries[0].info.thumbnailable && s.entries[0].info.mtime.is_some());
        assert_eq!(s.entries[0].info.icon.1, "image-x-generic");
        assert!(s.entries[1].info.is_dir && s.entries[1].info.size.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_y_clear() {
        let mut s = Shelf::default();
        s.add_paths(vec![p("/a"), p("/b")], now());
        assert!(s.remove(Path::new("/a")));
        assert!(!s.remove(Path::new("/a")));
        assert_eq!(s.entries.len(), 1);
        s.clear();
        assert!(s.entries.is_empty());
    }

    #[test]
    fn prune_conserva_ante_errores_distintos_de_notfound() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tmp_dir("perm");
        let sub = dir.join("sub");
        std::fs::create_dir(&sub).unwrap();
        let f = sub.join("a.txt");
        std::fs::write(&f, "x").unwrap();
        let mut s = Shelf::default();
        s.add_paths(vec![f.clone()], now());
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o000)).unwrap();
        let root = std::fs::metadata(&f).is_ok();
        let n = s.prune_missing();
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o755)).unwrap();
        if !root {
            assert_eq!(n, 0);
            assert_eq!(s.entries.len(), 1);
        }
    }

    #[test]
    fn prune_conserva_unidades_desmontadas() {
        let mut s = Shelf::default();
        s.add_paths(
            vec![PathBuf::from("/run/media/zz_morph_no_existe/x.txt")],
            now(),
        );
        // Solo aplica si /run/media existe sin ese subdirectorio, o /mnt.
        assert_eq!(s.prune_missing(), 0);
        assert!(under_unmounted_root(Path::new("/mnt/zz_morph/x")));
        assert!(!under_unmounted_root(Path::new("/tmp/zz_morph/x")));
    }

    #[test]
    fn prune_quita_los_que_ya_no_existen() {
        let dir = tmp_dir("miss");
        let (a, b) = (dir.join("a.txt"), dir.join("b.txt"));
        std::fs::write(&a, "x").unwrap();
        std::fs::write(&b, "y").unwrap();
        let mut s = Shelf::default();
        s.add_paths(vec![a.clone(), b.clone()], now());
        assert_eq!(s.prune_missing(), 0);
        assert_eq!(s.entries.len(), 2);
        std::fs::remove_file(&b).unwrap();
        assert_eq!(s.prune_missing(), 1);
        assert_eq!(s.present_paths(), vec![a]);
        assert_eq!(s.prune_missing(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nombres() {
        assert_eq!(
            display_name(Path::new("/x/año 🎵 notas.md")),
            "año 🎵 notas.md"
        );
        assert_eq!(display_name(Path::new("/")), "/");
    }

    #[test]
    fn iconos() {
        assert_eq!(
            icon_name(Path::new("/x/a.pdf")),
            ("application-pdf".to_string(), "x-office-document")
        );
        assert_eq!(
            icon_name(Path::new("/x/a.png")),
            ("image-png".to_string(), "image-x-generic")
        );
        assert_eq!(
            icon_name(Path::new("/x/LEEME")),
            ("text-x-generic".to_string(), "text-x-generic")
        );
        let dir = tmp_dir("icon");
        assert_eq!(icon_name(&dir), ("folder".to_string(), "folder"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn miniaturas() {
        let dir = tmp_dir("thumb");
        let png = dir.join("a.PNG");
        std::fs::write(&png, "x").unwrap();
        let txt = dir.join("a.txt");
        std::fs::write(&txt, "x").unwrap();
        assert!(is_thumbnailable(&png));
        assert!(!is_thumbnailable(&txt));
        assert!(!is_thumbnailable(&dir.join("nada.png")));
        let big = dir.join("big.jpg");
        let f = std::fs::File::create(&big).unwrap();
        f.set_len(20 * 1024 * 1024 + 1).unwrap();
        assert!(!is_thumbnailable(&big));
        f.set_len(20 * 1024 * 1024).unwrap();
        assert!(is_thumbnailable(&big));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ida_y_vuelta_y_poda_al_cargar() {
        let dir = tmp_dir("rt");
        let mut s = Shelf::default();
        let f = dir.join("año 🎵.md");
        std::fs::write(&f, "x").unwrap();
        s.add_paths(vec![f], now());
        save(&dir, &s).unwrap();
        assert!(!dir.join("shelf.json.tmp").exists());
        let (l, issue) = load(&dir);
        assert!(issue.is_none());
        assert_eq!(l.entries.len(), 1);
        assert_eq!(l.entries[0].path, s.entries[0].path);
        assert_eq!(l.entries[0].added, now());
        std::fs::remove_file(&l.entries[0].path).unwrap();
        let mut again = load(&dir).0;
        assert_eq!(again.prune_missing(), 1); // se poda al cargar
        assert!(again.entries.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ausente_es_vacio() {
        let dir = tmp_dir("none");
        let (s, issue) = load(&dir);
        assert!(s.entries.is_empty() && issue.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn danado_se_respalda() {
        let dir = tmp_dir("bad");
        std::fs::write(dir.join("shelf.json"), "{ basura").unwrap();
        let (s, issue) = load(&dir);
        assert!(s.entries.is_empty());
        let issue = issue.unwrap();
        assert!(issue.writable);
        assert!(issue.subtitle.contains("shelf.json.bad"));
        assert!(dir.join("shelf.json.bad").exists());
        assert!(!dir.join("shelf.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ilegible_bloquea_el_guardado() {
        let dir = tmp_dir("unreadable");
        std::fs::create_dir_all(dir.join("shelf.json")).unwrap();
        let (s, issue) = load(&dir);
        assert!(s.entries.is_empty());
        assert!(!issue.unwrap().writable);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn uri_list_ida_y_vuelta() {
        let paths = vec![
            p("/tmp/con espacios/año 🎵 notas.md"),
            p("/tmp/canción #1 100%.mp3"),
        ];
        let text = crate::dnd::to_uri_list(&paths);
        assert_eq!(crate::dnd::parse_uri_list(text.as_bytes()), paths);
    }
}
