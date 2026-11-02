//! Vista de la página Shelf (boceto A «Cuadrícula» y variante lista).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use cosmic::iced::{Alignment, Background, Border, Color, ContentFit, Length};
use cosmic::widget::icon::IconFallback;
use cosmic::widget::image::Handle;
use cosmic::{Element, widget};

use cosmic::iced::advanced::widget::tree::State as TreeState;

use super::{Entry, Shelf, display_name};
use crate::agenda::calendar_view::truncate;
use crate::app::Message;
use crate::config::ShelfLayout;
use crate::dnd::{DragPaths, is_copy_only};
use cosmic::iced::clipboard::dnd::DndAction;

const WHITE: Color = Color::WHITE;
const DIM: Color = Color::from_rgb(0.6, 0.6, 0.6);
const LIGHT_BLUE: Color = Color::from_rgb(0.4, 0.8, 1.0); // #6cf
const TILE_BG: Color = Color::from_rgb(0.082, 0.082, 0.082); // #151515
const BADGE_BG: Color = Color::from_rgb(0.133, 0.133, 0.133); // #222
const COLUMNS: usize = 5;
/// Alto fijo de mosaico y de fila: todos miden lo mismo (y su resalte también).
const TILE_H: f32 = 76.0;
const ROW_H: f32 = 40.0;

/// Miniaturas ya generadas (con la fecha del archivo de la que salieron).
pub type Thumbs = HashMap<PathBuf, (Option<SystemTime>, Handle)>;

/// Texto de la cápsula de la pastilla: "📎 5"; `None` si el Shelf está vacío.
pub fn badge_text(n: usize) -> Option<String> {
    (n > 0).then(|| format!("📎 {n}"))
}

/// Tamaño legible: "240 KB", "1,2 MB" (coma decimal, 1 decimal si hace falta).
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1000.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if v >= 100.0 || (v - v.round()).abs() < 0.05 {
        format!("{} {}", v.round() as u64, UNITS[i])
    } else {
        format!("{:.1} {}", v, UNITS[i]).replace('.', &crate::fl!("shelf-decimal-sep"))
    }
}

/// Carpeta contenedora, con el directorio personal abreviado a `~`.
pub fn parent_label(p: &Path, home: Option<&Path>) -> String {
    let Some(parent) = p.parent() else {
        return String::new();
    };
    if let Some(h) = home
        && let Ok(rest) = parent.strip_prefix(h)
    {
        return if rest.as_os_str().is_empty() {
            "~".into()
        } else {
            format!("~/{}", rest.display())
        };
    }
    parent.display().to_string()
}

fn text(s: impl Into<String>, size: u16, color: Color) -> widget::Text<'static, cosmic::Theme> {
    widget::text(s.into())
        .size(size)
        .wrapping(cosmic::iced::widget::text::Wrapping::None)
        .class(cosmic::theme::Text::Color(color))
}

fn bg(color: Color, radius: f32) -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::custom(move |_| widget::container::Style {
        background: Some(Background::Color(color)),
        border: Border {
            radius: radius.into(),
            ..Default::default()
        },
        ..Default::default()
    })
}

fn icon_of(e: &Entry, size: u16, thumbs: &Thumbs) -> Element<'static, Message> {
    if e.info.thumbnailable
        && let Some((_, h)) = thumbs.get(&e.path)
    {
        return widget::image(h.clone())
            .width(Length::Fixed(size as f32))
            .height(Length::Fixed(size as f32))
            .content_fit(ContentFit::Cover)
            .into();
    }
    generic_icon(e.info.icon.clone(), size)
}

fn generic_icon<M: 'static>(icon: (String, &'static str), size: u16) -> Element<'static, M> {
    let (specific, generic) = icon;
    widget::icon::from_name(specific)
        .fallback(Some(IconFallback::Names(vec![generic.into()])))
        .size(size)
        .icon()
        .width(Length::Fixed(size as f32))
        .height(Length::Fixed(size as f32))
        .into()
}

/// Imagen de arrastre (sin mensajes): ícono o miniatura de la entrada y su nombre.
fn drag_card(
    e: &Entry,
    size: u16,
    thumbs: &Thumbs,
    wide: bool,
    copying: bool,
) -> Element<'static, ()> {
    let icon: Element<'static, ()> = match thumbs.get(&e.path) {
        Some((_, h)) if e.info.thumbnailable => widget::image(h.clone())
            .width(Length::Fixed(size as f32))
            .height(Length::Fixed(size as f32))
            .content_fit(ContentFit::Cover)
            .into(),
        _ => generic_icon(e.info.icon.clone(), size),
    };
    let name = text(truncate(&display_name(&e.path), 14), 11, WHITE);
    // Pista «Copiar» bajo el nombre cuando el arrastre copiará.
    let label: Element<'static, ()> = if copying {
        widget::Column::new()
            .align_x(if wide {
                Alignment::Start
            } else {
                Alignment::Center
            })
            .push(name)
            .push(text(crate::fl!("shelf-copy"), 9, LIGHT_BLUE))
            .into()
    } else {
        name.into()
    };
    drag_frame(icon, label, wide)
}

fn drag_frame(
    icon: Element<'static, ()>,
    label: Element<'static, ()>,
    wide: bool,
) -> Element<'static, ()> {
    let inner: Element<'static, ()> = if wide {
        widget::Row::new()
            .align_y(Alignment::Center)
            .spacing(8)
            .push(icon)
            .push(label)
            .into()
    } else {
        widget::Column::new()
            .align_x(Alignment::Center)
            .spacing(3)
            .push(icon)
            .push(label)
            .into()
    };
    widget::container(inner)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .class(cosmic::theme::Container::custom(|_| {
            widget::container::Style {
                background: Some(Background::Color(TILE_BG)),
                border: Border {
                    color: LIGHT_BLUE,
                    width: 1.0,
                    radius: 10.0.into(),
                },
                ..Default::default()
            }
        }))
        .into()
}

fn remove_button(e: &Entry) -> Element<'static, Message> {
    widget::button::custom(text("✕", 10, DIM))
        .padding([0, 4])
        .class(cosmic::theme::Button::Text)
        .on_press(Message::ShelfRemove(e.path.clone()))
        .into()
}

/// Hace de la entrada una fuente de arrastre de su propia ruta, con imagen de
/// arrastre y origen atenuado con borde de acento mientras se arrastra.
fn draggable<'a>(
    e: &Entry,
    content: Element<'a, Message>,
    card: impl Fn() -> Element<'static, ()> + 'static,
    active: bool,
    action: DndAction,
) -> Element<'a, Message> {
    let path = e.path.clone();
    let start = vec![e.path.clone()];
    // Estructura constante (sin cambiar el árbol de widgets en pleno arrastre):
    // un velo encima que solo se ve (atenuado + borde de acento) cuando `active`.
    let veil = widget::container(widget::space())
        .width(Length::Fill)
        .height(Length::Fill)
        .class(cosmic::theme::Container::custom(move |_| {
            widget::container::Style {
                background: active.then(|| Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.5))),
                border: Border {
                    color: if active {
                        LIGHT_BLUE
                    } else {
                        Color::TRANSPARENT
                    },
                    width: if active { 2.0 } else { 0.0 },
                    radius: 10.0.into(),
                },
                ..Default::default()
            }
        }));
    let content: Element<'a, Message> = cosmic::iced::widget::stack![content, veil].into();
    widget::dnd_source::<Message, DragPaths>(content)
        .drag_content(move || DragPaths(vec![path.clone()]))
        .action(action)
        .drag_icon(move |offset| (card(), TreeState::None, offset))
        .on_start(Some(Message::DragStart(start)))
        .on_finish(Some(Message::Dragging(false)))
        .on_cancel(Some(Message::Dragging(false)))
        .into()
}

fn open_button<'a>(e: &Entry, content: Element<'a, Message>) -> Element<'a, Message> {
    widget::button::custom(content)
        .padding(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .class(cosmic::theme::Button::Text)
        .on_press(Message::ShelfOpen(e.path.clone()))
        .into()
}

fn tile(
    e: &Entry,
    thumbs: &Thumbs,
    dragged: &[PathBuf],
    action: DndAction,
) -> Element<'static, Message> {
    let body = widget::container(
        widget::Column::new()
            .align_x(Alignment::Center)
            .spacing(3)
            .push(icon_of(e, 34, thumbs))
            .push(text(truncate(&display_name(&e.path), 12), 11, WHITE)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center);
    let close = widget::container(remove_button(e))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::End)
        .align_y(Alignment::Start);
    // El botón ocupa todo el mosaico: mismo tamaño y mismo resalte para todos.
    let tile = widget::container(cosmic::iced::widget::stack![
        open_button(e, body.into()),
        close
    ])
    .width(Length::Fill)
    .height(Length::Fixed(TILE_H))
    .clip(true)
    .class(bg(TILE_BG, 10.0));
    let card_e = e.clone();
    let card_thumbs: Thumbs = thumbs
        .get(&e.path)
        .map(|v| HashMap::from([(e.path.clone(), v.clone())]))
        .unwrap_or_default();
    let copying = is_copy_only(action);
    draggable(
        e,
        tile.into(),
        move || drag_card(&card_e, 40, &card_thumbs, false, copying),
        dragged.contains(&e.path),
        action,
    )
}

fn row(
    e: &Entry,
    home: Option<&Path>,
    thumbs: &Thumbs,
    dragged: &[PathBuf],
    action: DndAction,
) -> Element<'static, Message> {
    let dir = parent_label(&e.path, home);
    let detail = match e.info.size {
        Some(n) => format!("{} · {}", truncate(&dir, 30), human_size(n)),
        None => truncate(&dir, 36),
    };
    let body = widget::container(
        widget::Row::new()
            .align_y(Alignment::Center)
            .spacing(8)
            .push(icon_of(e, 24, thumbs))
            .push(
                widget::Column::new()
                    .width(Length::Fill)
                    .push(text(truncate(&display_name(&e.path), 40), 12, WHITE))
                    .push(text(detail, 10, DIM)),
            ),
    )
    .padding([0, 8])
    .width(Length::Fill)
    .height(Length::Fill)
    .align_y(Alignment::Center);
    let close = widget::container(remove_button(e))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::End)
        .align_y(Alignment::Center)
        .padding([0, 4]);
    let r = widget::container(cosmic::iced::widget::stack![
        open_button(e, body.into()),
        close
    ])
    .width(Length::Fill)
    .height(Length::Fixed(ROW_H))
    .clip(true)
    .class(bg(TILE_BG, 8.0));
    let card_e = e.clone();
    let card_thumbs: Thumbs = thumbs
        .get(&e.path)
        .map(|v| HashMap::from([(e.path.clone(), v.clone())]))
        .unwrap_or_default();
    let copying = is_copy_only(action);
    draggable(
        e,
        r.into(),
        move || drag_card(&card_e, 32, &card_thumbs, true, copying),
        dragged.contains(&e.path),
        action,
    )
}

/// Texto de la imagen de arrastre de «Arrastrar todo».
pub fn drag_all_label(n: usize) -> String {
    crate::fl!("shelf-items", n = n)
}

fn header(shelf: &Shelf, dragged: &[PathBuf], action: DndAction) -> Element<'static, Message> {
    let mut r = widget::Row::new()
        .align_y(Alignment::Center)
        .push(
            text(crate::fl!("shelf-title"), 15, WHITE).font(cosmic::iced::Font {
                weight: cosmic::iced::font::Weight::Semibold,
                ..cosmic::iced::Font::DEFAULT
            }),
        )
        .push(widget::space().width(Length::Fill));
    if !shelf.entries.is_empty() {
        let all = shelf.present_paths();
        let n = all.len();
        let all_dragging = n > 1 && dragged.len() == n;
        let label = text(
            crate::fl!("shelf-drag-all"),
            12,
            if all_dragging { DIM } else { LIGHT_BLUE },
        );
        let start = all.clone();
        let copying = is_copy_only(action);
        let drag_all: Element<'static, Message> = widget::dnd_source::<Message, DragPaths>(
            // Tamaño fijo: la imagen de arrastre toma el tamaño del origen.
            widget::container(label)
                .width(Length::Fixed(104.0))
                .height(Length::Fixed(28.0))
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .drag_content(move || DragPaths(all.clone()))
        .action(action)
        .drag_icon(move |offset| {
            let stack = widget::icon::from_name("edit-copy-symbolic")
                .size(18)
                .icon()
                .width(Length::Fixed(18.0))
                .height(Length::Fixed(18.0));
            let card = drag_frame(
                stack.into(),
                text(
                    if copying {
                        crate::fl!("shelf-drag-all-copy", label = drag_all_label(n))
                    } else {
                        drag_all_label(n)
                    },
                    11,
                    WHITE,
                )
                .into(),
                true,
            );
            (card, TreeState::None, offset)
        })
        .on_start(Some(Message::DragStart(start)))
        .on_finish(Some(Message::Dragging(false)))
        .on_cancel(Some(Message::Dragging(false)))
        .into();
        r = r.push(drag_all).push(text(" · ", 12, DIM)).push(
            widget::button::custom(text(crate::fl!("shelf-clear"), 12, LIGHT_BLUE))
                .padding([2, 4])
                .class(cosmic::theme::Button::Text)
                .on_press(Message::ShelfClear),
        );
    }
    r.into()
}

fn empty_box() -> Element<'static, Message> {
    widget::container(
        widget::Column::new()
            .align_x(Alignment::Center)
            .spacing(4)
            .push(text("⇩", 24, DIM))
            .push(text(crate::fl!("shelf-empty-title"), 13, WHITE))
            .push(text(crate::fl!("shelf-empty-hint"), 11, DIM)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .class(cosmic::theme::Container::custom(|_| {
        // iced no dibuja bordes punteados: borde fino gris.
        widget::container::Style {
            border: Border {
                color: Color::from_rgb(0.3, 0.3, 0.3),
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        }
    }))
    .into()
}

/// Borde de acento de 2 px y letrero centrado mientras un arrastre externo está encima.
fn drop_highlight(body: Element<'static, Message>) -> Element<'static, Message> {
    let frame = widget::container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(2)
        .class(cosmic::theme::Container::custom(|_| {
            widget::container::Style {
                border: Border {
                    color: LIGHT_BLUE,
                    width: 2.0,
                    radius: 12.0.into(),
                },
                ..Default::default()
            }
        }));
    let label = widget::container(text(crate::fl!("shelf-drop-hint"), 13, WHITE))
        .padding([6, 14])
        .class(bg(Color::from_rgba(0.0, 0.0, 0.0, 0.85), 10.0));
    let center = widget::container(label)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);
    cosmic::iced::widget::stack![frame, center].into()
}

pub fn view(
    shelf: &Shelf,
    layout: ShelfLayout,
    home: Option<&Path>,
    thumbs: &Thumbs,
    dragged: &[PathBuf],
    drop_hover: bool,
    // (modo Copiar activo, Ctrl pulsado)
    (copy_mode, ctrl): (bool, bool),
) -> Element<'static, Message> {
    let action = crate::dnd::offered_action(copy_mode, ctrl);
    let body: Element<'static, Message> = if shelf.entries.is_empty() {
        empty_box()
    } else if layout == ShelfLayout::List {
        widget::scrollable(shelf.entries.iter().fold(
            widget::Column::new().spacing(4).padding([0, 8, 0, 0]),
            |c, e| c.push(row(e, home, thumbs, dragged, action)),
        ))
        .height(Length::Fill)
        .into()
    } else {
        let mut grid = widget::Column::new().spacing(6).padding([0, 8, 0, 0]);
        for chunk in shelf.entries.chunks(COLUMNS) {
            let mut r = widget::Row::new().spacing(6);
            for e in chunk {
                r = r.push(
                    widget::container(tile(e, thumbs, dragged, action))
                        .width(Length::FillPortion(1)),
                );
            }
            for _ in chunk.len()..COLUMNS {
                r = r.push(widget::space().width(Length::FillPortion(1)));
            }
            grid = grid.push(r);
        }
        widget::scrollable(grid).height(Length::Fill).into()
    };
    // Margen de 1 px para que el borde del recuadro no se recorte.
    let body: Element<'static, Message> = if drop_hover {
        drop_highlight(body)
    } else {
        widget::container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(1)
            .into()
    };
    widget::Column::new()
        .spacing(8)
        .width(Length::Fill)
        .height(Length::Fill)
        .push(header(shelf, dragged, action))
        .push(body)
        .into()
}

/// Cápsula "📎 N" de la pastilla cerrada.
pub fn badge(n: usize) -> Option<Element<'static, Message>> {
    let t = badge_text(n)?;
    Some(
        widget::container(text(t, 11, WHITE))
            .padding([2, 8])
            .class(bg(BADGE_BG, 9.0))
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn badge_vacio_es_none() {
        assert_eq!(badge_text(0), None);
        assert_eq!(badge_text(5).as_deref(), Some("📎 5"));
    }

    #[test]
    fn etiqueta_de_arrastrar_todo() {
        crate::i18n::apply_for_test("en");
        assert_eq!(drag_all_label(1), "1 item");
        assert_eq!(drag_all_label(3), "3 items");
        crate::i18n::apply_for_test("es");
        assert_eq!(drag_all_label(1), "1 elemento");
        assert_eq!(drag_all_label(3), "3 elementos");
    }

    #[test]
    fn tamanos_legibles() {
        crate::i18n::apply_for_test("es");
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(240 * 1024), "240 KB");
        assert_eq!(human_size(1_258_291), "1,2 MB");
        assert_eq!(human_size(3 * 1024 * 1024), "3 MB");
        crate::i18n::apply_for_test("en");
        assert_eq!(human_size(1_258_291), "1.2 MB");
    }

    #[test]
    fn carpeta_con_tilde() {
        let home = PathBuf::from("/home/jam");
        assert_eq!(
            parent_label(Path::new("/home/jam/Docs/a.txt"), Some(&home)),
            "~/Docs"
        );
        assert_eq!(parent_label(Path::new("/home/jam/a.txt"), Some(&home)), "~");
        assert_eq!(parent_label(Path::new("/tmp/a.txt"), Some(&home)), "/tmp");
        assert_eq!(
            parent_label(Path::new("/home/jamie/a"), Some(&home)),
            "/home/jamie"
        );
    }
}
