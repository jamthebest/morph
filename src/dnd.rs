//! Utilidades de arrastrar y soltar: conversión entre rutas y `text/uri-list`.

use std::borrow::Cow;
use std::path::PathBuf;

use cosmic::iced::clipboard::dnd::DndAction;
use cosmic::iced::clipboard::mime::AsMimeTypes;
use url::Url;

/// Acción que ofrece una fuente de arrastre del Shelf: solo Copiar si el modo
/// Copiar está activo o Ctrl está pulsado; si no, Mover y Copiar (el destino elige).
pub fn offered_action(copy_mode: bool, ctrl: bool) -> DndAction {
    if copy_mode || ctrl {
        DndAction::Copy
    } else {
        DndAction::Move | DndAction::Copy
    }
}

/// `true` si la acción ofrecida es únicamente Copiar.
pub fn is_copy_only(a: DndAction) -> bool {
    a == DndAction::Copy
}

pub const URI_LIST_MIME: &str = "text/uri-list";

/// Extrae las rutas de archivo de un `text/uri-list` (ignora comentarios,
/// líneas vacías y URLs que no sean `file://`).
pub fn parse_uri_list(data: &[u8]) -> Vec<PathBuf> {
    String::from_utf8_lossy(data)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| Url::parse(l).ok())
        .filter_map(|u| u.to_file_path().ok())
        .collect()
}

/// Serializa rutas absolutas como `text/uri-list` (una URL `file://` por línea, CRLF).
pub fn to_uri_list(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .filter_map(|p| Url::from_file_path(p).ok())
        .map(|u| format!("{u}\r\n"))
        .collect()
}

/// Carga útil de un arrastre saliente: una lista de rutas como `text/uri-list`.
pub struct DragPaths(pub Vec<PathBuf>);

impl AsMimeTypes for DragPaths {
    fn available(&self) -> Cow<'static, [String]> {
        Cow::Owned(vec![URI_LIST_MIME.to_string()])
    }

    fn as_bytes(&self, mime_type: &str) -> Option<Cow<'static, [u8]>> {
        (mime_type == URI_LIST_MIME).then(|| Cow::Owned(to_uri_list(&self.0).into_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_uri_con_escapes() {
        let got = parse_uri_list(b"file:///home/jam/Mis%20Docs/a%C3%B1o.txt\r\n");
        assert_eq!(got, vec![PathBuf::from("/home/jam/Mis Docs/año.txt")]);
    }

    #[test]
    fn ignora_comentarios_vacias_y_no_file() {
        let got =
            parse_uri_list(b"# hola\n\nhttps://x.com/a\nfile:///tmp/a\r\n\r\nfile:///tmp/b\n");
        assert_eq!(got, vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]);
    }

    #[test]
    fn acepta_host_localhost() {
        assert_eq!(
            parse_uri_list(b"file://localhost/tmp/a\n"),
            vec![PathBuf::from("/tmp/a")]
        );
    }

    #[test]
    fn serializa_con_escapes() {
        assert_eq!(
            to_uri_list(&[PathBuf::from("/tmp/a b.txt")]),
            "file:///tmp/a%20b.txt\r\n"
        );
    }

    #[test]
    fn ida_y_vuelta() {
        let paths = vec![
            PathBuf::from("/home/jam/Mis Docs/año.txt"),
            PathBuf::from("/tmp/x#y?z.txt"),
        ];
        assert_eq!(parse_uri_list(to_uri_list(&paths).as_bytes()), paths);
    }
}

#[cfg(test)]
mod action_tests {
    use super::*;
    use cosmic::iced::clipboard::dnd::DndAction;

    #[test]
    fn offered_action_cases() {
        let both = DndAction::Move | DndAction::Copy;
        assert_eq!(offered_action(false, false), both);
        assert_eq!(offered_action(true, false), DndAction::Copy);
        assert_eq!(offered_action(false, true), DndAction::Copy);
        assert_eq!(offered_action(true, true), DndAction::Copy);
        assert!(!is_copy_only(offered_action(false, false)));
        assert!(is_copy_only(offered_action(true, false)));
    }
}
