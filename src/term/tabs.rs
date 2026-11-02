//! Modelo puro de pestañas de la terminal.
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabMeta {
    pub id: u64,
    pub title: String,
}

#[derive(Clone, Debug, Default)]
pub struct Tabs {
    pub items: Vec<TabMeta>,
    pub active: usize,
    next_id: u64,
}

/// Resultado de cerrar una pestaña.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Closed {
    /// Posición que ocupaba (para quitar la sesión alineada).
    pub index: usize,
    /// No quedan pestañas: hay que esconder la terminal.
    pub hide: bool,
}

impl Tabs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Añade una pestaña al final y la activa. Devuelve su id.
    pub fn open(&mut self, title: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(TabMeta {
            id,
            title: title.to_string(),
        });
        self.active = self.items.len() - 1;
        id
    }

    pub fn close(&mut self, id: u64) -> Option<Closed> {
        let index = self.index_of(id)?;
        self.items.remove(index);
        if self.items.is_empty() {
            self.active = 0;
            return Some(Closed { index, hide: true });
        }
        if index < self.active {
            self.active -= 1;
        }
        self.active = self.active.min(self.items.len() - 1);
        Some(Closed { index, hide: false })
    }

    /// Siguiente pestaña (circular).
    pub fn next(&mut self) {
        if !self.items.is_empty() {
            self.active = (self.active + 1) % self.items.len();
        }
    }

    /// Pestaña anterior (circular).
    pub fn prev(&mut self) {
        if !self.items.is_empty() {
            self.active = (self.active + self.items.len() - 1) % self.items.len();
        }
    }

    pub fn select(&mut self, id: u64) {
        if let Some(i) = self.index_of(id) {
            self.active = i;
        }
    }

    pub fn set_title(&mut self, id: u64, title: &str) {
        if let Some(t) = self.items.iter_mut().find(|t| t.id == id) {
            t.title = title.to_string();
        }
    }

    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.items.iter().position(|t| t.id == id)
    }

    pub fn active_id(&self) -> Option<u64> {
        self.items.get(self.active).map(|t| t.id)
    }
}

/// Título de pestaña: el del shell si no está vacío; si no, la carpeta actual; si no, `fallback`.
pub fn tab_title(title: Option<&str>, cwd: Option<&Path>, fallback: &str) -> String {
    if let Some(t) = title.map(str::trim).filter(|t| !t.is_empty()) {
        return t.to_string();
    }
    if let Some(p) = cwd {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
        return name.unwrap_or_else(|| p.to_string_lossy().into_owned());
    }
    fallback.to_string()
}

/// Etiqueta visible «N · título» (N desde 1, título acortado a `max` caracteres).
pub fn label(index: usize, title: &str, max: usize) -> String {
    let t: String = if title.chars().count() > max {
        title
            .chars()
            .take(max)
            .chain(std::iter::once('…'))
            .collect()
    } else {
        title.to_string()
    };
    format!("{} · {t}", index + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three() -> (Tabs, [u64; 3]) {
        let mut t = Tabs::new();
        let a = t.open("a");
        let b = t.open("b");
        let c = t.open("c");
        (t, [a, b, c])
    }

    #[test]
    fn open_activa_la_nueva() {
        let (t, [_, _, c]) = three();
        assert_eq!(t.items.len(), 3);
        assert_eq!(t.active_id(), Some(c));
        assert_eq!(t.active, 2);
    }

    #[test]
    fn ids_unicos_aunque_se_cierre() {
        let mut t = Tabs::new();
        let a = t.open("a");
        t.close(a);
        assert_ne!(t.open("b"), a);
    }

    #[test]
    fn cerrar_la_activa_elige_la_vecina() {
        let (mut t, [a, b, c]) = three();
        t.select(b);
        let r = t.close(b).unwrap();
        assert_eq!(
            r,
            Closed {
                index: 1,
                hide: false
            }
        );
        assert_eq!(t.active_id(), Some(c));
        // La última activa cae a la anterior.
        let r = t.close(c).unwrap();
        assert_eq!(r.index, 1);
        assert_eq!(t.active_id(), Some(a));
    }

    #[test]
    fn cerrar_otra_mantiene_la_activa() {
        let (mut t, [a, _, c]) = three();
        t.close(a);
        assert_eq!(t.active_id(), Some(c));
        assert_eq!(t.active, 1);
    }

    #[test]
    fn cerrar_la_ultima_oculta() {
        let mut t = Tabs::new();
        let a = t.open("a");
        assert_eq!(
            t.close(a),
            Some(Closed {
                index: 0,
                hide: true
            })
        );
        assert!(t.items.is_empty());
        assert_eq!(t.active_id(), None);
        assert_eq!(t.close(a), None);
    }

    #[test]
    fn ctrl_tab_es_circular() {
        let (mut t, [a, b, c]) = three();
        t.next();
        assert_eq!(t.active_id(), Some(a));
        t.prev();
        assert_eq!(t.active_id(), Some(c));
        t.prev();
        assert_eq!(t.active_id(), Some(b));
        let mut vacia = Tabs::new();
        vacia.next();
        vacia.prev();
        assert_eq!(vacia.active_id(), None);
    }

    #[test]
    fn titulo_desde_title_o_carpeta() {
        assert_eq!(tab_title(Some("vim"), None, "zsh"), "vim");
        assert_eq!(
            tab_title(Some("  "), Some(Path::new("/home/jam/Code")), "zsh"),
            "Code"
        );
        assert_eq!(
            tab_title(None, Some(Path::new("/home/jam/Code")), "zsh"),
            "Code"
        );
        assert_eq!(tab_title(None, Some(Path::new("/")), "zsh"), "/");
        assert_eq!(tab_title(None, None, "zsh"), "zsh");
    }

    #[test]
    fn set_title_y_label() {
        let (mut t, [a, ..]) = three();
        t.set_title(a, "nuevo");
        assert_eq!(t.items[0].title, "nuevo");
        assert_eq!(label(0, "zsh", 12), "1 · zsh");
        assert_eq!(label(1, "un título muy largo", 8), "2 · un títul…");
    }
}
