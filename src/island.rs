use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Page {
    Musica,
    Calendario,
    Todo,
    Shelf,
    Escritorios,
    Dev,
}

impl Page {
    /// Orden por defecto de las páginas.
    pub const ALL: [Page; 6] = [
        Page::Musica,
        Page::Calendario,
        Page::Todo,
        Page::Shelf,
        Page::Escritorios,
        Page::Dev,
    ];

    /// Páginas activas de serie: Dev (Desarrollo) se activa a mano en Ajustes → Páginas.
    pub const DEFAULT: [Page; 5] = [
        Page::Musica,
        Page::Calendario,
        Page::Todo,
        Page::Shelf,
        Page::Escritorios,
    ];

    pub fn title(self) -> String {
        match self {
            Page::Musica => crate::fl!("page-music"),
            Page::Calendario => crate::fl!("page-calendar"),
            Page::Todo => crate::fl!("page-todo"),
            Page::Shelf => crate::fl!("page-shelf"),
            Page::Escritorios => crate::fl!("page-desktops"),
            Page::Dev => crate::fl!("page-dev"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IslandState {
    Collapsed,
    Expanded { page: Page },
}

/// Qué contenido dibujar dentro de la caja visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Content {
    Collapsed,
    Expanded,
}

/// Al cerrar se sigue mostrando el contenido expandido (recortado) hasta que la
/// animación termina; así el contenido no se recoloca mientras se encoge.
pub fn content_for(state: IslandState, closing: bool) -> Content {
    match state {
        IslandState::Expanded { .. } => Content::Expanded,
        IslandState::Collapsed if closing => Content::Expanded,
        IslandState::Collapsed => Content::Collapsed,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Click,
    PointerLeft,
    Escape,
    Scroll(i32),
    Toggle,
    OpenPage(Page),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardMode {
    None,
    OnDemand,
}

impl IslandState {
    /// `resume` es la última página mostrada: al abrir con clic o alternar se retoma
    /// (si ya no está en `pages`, se usa la primera).
    pub fn on(self, e: Event, pages: &[Page], resume: Page) -> IslandState {
        let first = pages.first().copied().unwrap_or(Page::Musica);
        match (self, e) {
            (IslandState::Collapsed, Event::Click | Event::Toggle) => IslandState::Expanded {
                page: if pages.contains(&resume) {
                    resume
                } else {
                    first
                },
            },
            (IslandState::Collapsed, Event::OpenPage(page)) => IslandState::Expanded { page },
            (IslandState::Collapsed, _) => IslandState::Collapsed,
            (IslandState::Expanded { .. }, Event::PointerLeft | Event::Escape | Event::Toggle) => {
                IslandState::Collapsed
            }
            (IslandState::Expanded { .. }, Event::OpenPage(page)) => IslandState::Expanded { page },
            (IslandState::Expanded { page }, Event::Scroll(d)) => {
                let page = match pages.iter().position(|p| *p == page) {
                    None => first,
                    Some(i) => {
                        let last = pages.len() - 1;
                        let j = (i as i64 + d.signum() as i64).clamp(0, last as i64) as usize;
                        pages[j]
                    }
                };
                IslandState::Expanded { page }
            }
            (s @ IslandState::Expanded { .. }, Event::Click) => s,
        }
    }

    pub fn keyboard(&self) -> KeyboardMode {
        match self {
            IslandState::Collapsed => KeyboardMode::None,
            IslandState::Expanded { .. } => KeyboardMode::OnDemand,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: &[Page] = &Page::ALL;
    const EXP: IslandState = IslandState::Expanded { page: Page::Musica };

    #[test]
    fn click_opens_on_first_page() {
        assert_eq!(
            IslandState::Collapsed.on(Event::Click, P, Page::Musica),
            EXP
        );
    }

    #[test]
    fn escape_and_pointer_left_close() {
        assert_eq!(
            EXP.on(Event::Escape, P, Page::Musica),
            IslandState::Collapsed
        );
        assert_eq!(
            EXP.on(Event::PointerLeft, P, Page::Musica),
            IslandState::Collapsed
        );
    }

    #[test]
    fn toggle_alternates() {
        assert_eq!(
            IslandState::Collapsed.on(Event::Toggle, P, Page::Musica),
            EXP
        );
        assert_eq!(
            EXP.on(Event::Toggle, P, Page::Musica),
            IslandState::Collapsed
        );
    }

    #[test]
    fn scroll_moves_without_wrapping() {
        assert_eq!(
            EXP.on(Event::Scroll(1), P, Page::Musica),
            IslandState::Expanded {
                page: Page::Calendario
            }
        );
        assert_eq!(EXP.on(Event::Scroll(-1), P, Page::Musica), EXP);
        let last = IslandState::Expanded { page: Page::Dev };
        assert_eq!(last.on(Event::Scroll(1), P, Page::Musica), last);
    }

    #[test]
    fn scroll_uses_given_order_and_falls_back_to_first() {
        let pages = [Page::Dev, Page::Todo];
        let s = IslandState::Expanded { page: Page::Dev };
        assert_eq!(
            s.on(Event::Scroll(1), &pages, Page::Musica),
            IslandState::Expanded { page: Page::Todo }
        );
        assert_eq!(
            EXP.on(Event::Scroll(1), &pages, Page::Musica),
            IslandState::Expanded { page: Page::Dev }
        );
    }

    #[test]
    fn open_page_from_collapsed() {
        assert_eq!(
            IslandState::Collapsed.on(Event::OpenPage(Page::Escritorios), P, Page::Musica),
            IslandState::Expanded {
                page: Page::Escritorios
            }
        );
    }

    #[test]
    fn click_and_toggle_resume_last_page() {
        let todo = IslandState::Expanded { page: Page::Todo };
        assert_eq!(IslandState::Collapsed.on(Event::Click, P, Page::Todo), todo);
        assert_eq!(
            IslandState::Collapsed.on(Event::Toggle, P, Page::Todo),
            todo
        );
    }

    #[test]
    fn scroll_from_hidden_page_with_one_active_page() {
        // Se ocultó la página actual y solo queda una activa: nunca hay pánico ni vacío.
        let pages = [Page::Musica];
        let s = IslandState::Expanded { page: Page::Todo };
        assert_eq!(
            s.on(Event::Scroll(1), &pages, Page::Todo),
            IslandState::Expanded { page: Page::Musica }
        );
    }

    #[test]
    fn resume_page_removed_from_pages_falls_back_to_first() {
        let pages = [Page::Dev, Page::Musica];
        let dev = IslandState::Expanded { page: Page::Dev };
        assert_eq!(
            IslandState::Collapsed.on(Event::Click, &pages, Page::Todo),
            dev
        );
        assert_eq!(
            IslandState::Collapsed.on(Event::Toggle, &pages, Page::Todo),
            dev
        );
    }

    #[test]
    fn open_page_ignores_resume() {
        assert_eq!(
            IslandState::Collapsed.on(Event::OpenPage(Page::Dev), P, Page::Todo),
            IslandState::Expanded { page: Page::Dev }
        );
    }

    #[test]
    fn content_stays_expanded_while_closing() {
        assert_eq!(content_for(EXP, false), Content::Expanded);
        assert_eq!(content_for(EXP, true), Content::Expanded);
        assert_eq!(
            content_for(IslandState::Collapsed, false),
            Content::Collapsed
        );
        assert_eq!(content_for(IslandState::Collapsed, true), Content::Expanded);
    }

    #[test]
    fn keyboard_mode() {
        assert_eq!(IslandState::Collapsed.keyboard(), KeyboardMode::None);
        assert_eq!(EXP.keyboard(), KeyboardMode::OnDemand);
    }
}
