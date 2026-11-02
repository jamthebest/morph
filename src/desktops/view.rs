//! Vista de la página Escritorios: filas (B) y columnas (A) del boceto.

use cosmic::iced::{Alignment, Background, Border, Color, Length};
use cosmic::widget::icon::IconFallback;
use cosmic::{Element, widget};

use super::apps::AppIndex;
use super::pins::{Row, rows};
use super::{Snapshot, visible};

use crate::agenda::calendar_view::truncate;
use crate::app::Message;
use crate::config::{DesktopsLayout, Pin};

const WHITE: Color = Color::WHITE;
const DIM: Color = Color::from_rgb(0.6, 0.6, 0.6);
const ROW_BG: Color = Color::from_rgb(0.082, 0.082, 0.082); // #151515
const NUM_OFF: Color = Color::from_rgb(0.133, 0.133, 0.133); // #222
const BLUE: Color = Color::from_rgb(0.176, 0.424, 0.875); // #2d6cdf
const ICON: u16 = 22;
const FALLBACK_ICON: &str = "application-x-executable";
/// Ancho de un panel en columnas (4 visibles dentro de los 488 px útiles).
/// Íconos visibles por fila antes de «+k».
const MAX_ICONS: usize = 8;
const PANEL_W: f32 = 116.0;

/// Títulos de las ventanas (app_id, título), recortados y separados por coma.
/// Sin título usa el app_id.
pub fn titles_text(wins: &[(String, String)], max_each: usize) -> String {
    wins.iter()
        .map(|(app, title)| {
            let t = title.trim();
            truncate(if t.is_empty() { app } else { t }, max_each)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Texto atenuado de una fila: títulos y, tras ellos, las fijadas sin abrir
/// ("GitHub Desktop (sin abrir)").
pub fn row_label(titles: &str, missing_names: &[String]) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !titles.is_empty() {
        parts.push(titles.to_string());
    }
    parts.extend(
        missing_names
            .iter()
            .map(|n| crate::fl!("desk-not-open", name = n.as_str())),
    );
    parts.join(", ")
}

/// Aviso al alternar una fijación: `pinned` es el escritorio actual de la app, si sigue fijada.
pub fn pin_notice(name: &str, pinned: Option<u32>) -> String {
    match pinned {
        Some(n) => crate::fl!("desk-pinned", name = name, n = n),
        None => crate::fl!("desk-unpinned", name = name),
    }
}

/// Nombre para mostrar de un app_id (el del .desktop si se conoce).
pub fn display_name(app_id: &str, apps: &AppIndex) -> String {
    apps.get(app_id)
        .map_or_else(|| app_id.to_string(), |a| a.name.clone())
}

fn bg(color: Color, border: Option<Color>, radius: f32) -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::custom(move |_| widget::container::Style {
        background: Some(Background::Color(color)),
        border: Border {
            color: border.unwrap_or(Color::TRANSPARENT),
            width: if border.is_some() { 1.5 } else { 0.0 },
            radius: radius.into(),
        },
        ..Default::default()
    })
}

fn label(text: String, size: u16, color: Color) -> widget::Text<'static, cosmic::Theme> {
    widget::text(text)
        .size(size)
        .wrapping(cosmic::iced::widget::text::Wrapping::None)
        .class(cosmic::theme::Text::Color(color))
}

/// Ícono de 22 px de la app; `dim` lo atenúa (~45 %) con una capa del color del fondo y
/// `pinned` dibuja un puntito de acento en la esquina (app abierta y fijada aquí).
fn app_icon(app_id: &str, apps: &AppIndex, dim: bool, pinned: bool) -> Element<'static, Message> {
    let name = apps
        .get(app_id)
        .and_then(|a| a.icon.clone())
        .unwrap_or_else(|| FALLBACK_ICON.to_string());
    let icon: Element<'static, Message> = if name.starts_with('/') {
        widget::icon(widget::icon::from_path(std::path::PathBuf::from(name)))
            .width(Length::Fixed(ICON as f32))
            .height(Length::Fixed(ICON as f32))
            .into()
    } else {
        widget::icon::from_name(name)
            .fallback(Some(IconFallback::Names(vec![FALLBACK_ICON.into()])))
            .size(ICON)
            .icon()
            .width(Length::Fixed(ICON as f32))
            .height(Length::Fixed(ICON as f32))
            .into()
    };
    let mut layers: Vec<Element<'static, Message>> = vec![icon];
    if dim {
        layers.push(
            widget::container(widget::space())
                .width(Length::Fixed(ICON as f32))
                .height(Length::Fixed(ICON as f32))
                .class(bg(Color { a: 0.55, ..ROW_BG }, None, 0.0))
                .into(),
        );
    }
    if pinned {
        let dot = widget::container(widget::space())
            .width(Length::Fixed(7.0))
            .height(Length::Fixed(7.0))
            .class(cosmic::theme::Container::custom(|_| {
                widget::container::Style {
                    background: Some(Background::Color(BLUE)),
                    border: Border {
                        color: WHITE,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                }
            }));
        layers.push(
            widget::container(dot)
                .width(Length::Fixed(ICON as f32))
                .height(Length::Fixed(ICON as f32))
                .align_x(Alignment::End)
                .align_y(Alignment::End)
                .into(),
        );
    }
    if layers.len() == 1 {
        return layers.remove(0);
    }
    cosmic::iced::widget::Stack::with_children(layers).into()
}

/// Una app del escritorio: ventana abierta o fijada sin abrir.
enum Item<'a> {
    Open(&'a super::Window),
    Pinned(&'a str),
}

impl Item<'_> {
    fn app_id(&self) -> &str {
        match self {
            Item::Open(w) => &w.app_id,
            Item::Pinned(a) => a,
        }
    }
}

fn items(row: &Row) -> Vec<Item<'_>> {
    row.windows
        .iter()
        .map(Item::Open)
        .chain(row.pinned_missing.iter().map(|a| Item::Pinned(a)))
        .collect()
}

/// Envuelve un elemento con clic izquierdo y derecho (fijar/quitar).
fn clickable(
    content: Element<'static, Message>,
    left: Message,
    app_id: &str,
    ws_index: u32,
) -> Element<'static, Message> {
    let area = widget::mouse_area(content).on_press(left);
    if app_id.is_empty() {
        return area.into();
    }
    area.on_right_press(Message::DeskTogglePin(app_id.to_string(), ws_index))
        .into()
}

fn left_msg(item: &Item, ws_id: u32) -> Message {
    match item {
        Item::Open(w) => Message::DeskActivateWindow(w.id),
        Item::Pinned(a) => Message::DeskLaunch(a.to_string(), ws_id),
    }
}

fn number_dot(row: &Row) -> Element<'static, Message> {
    let dot = widget::container(label(row.ws.index.to_string(), 12, WHITE))
        .width(Length::Fixed(22.0))
        .height(Length::Fixed(22.0))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .class(bg(if row.ws.active { BLUE } else { NUM_OFF }, None, 11.0));
    widget::mouse_area(dot)
        .on_press(Message::DeskActivateWorkspace(row.ws.id))
        .into()
}

fn row_view(row: &Row, apps: &AppIndex, pins: &[Pin]) -> Element<'static, Message> {
    let mut line = widget::Row::new().spacing(8).align_y(Alignment::Center);
    line = line.push(number_dot(row));
    let its = items(row);
    let (shown, extra) = split_cap(its.len(), MAX_ICONS);
    for it in its.iter().take(shown) {
        let active = matches!(it, Item::Open(w) if w.active);
        let open = matches!(it, Item::Open(_));
        let mark = open && is_pinned(pins, it.app_id(), row.ws.index);
        let icon = widget::container(app_icon(it.app_id(), apps, !open, mark))
            .padding(2)
            .class(bg(Color::TRANSPARENT, active.then_some(BLUE), 6.0));
        line = line.push(clickable(
            icon.into(),
            left_msg(it, row.ws.id),
            it.app_id(),
            row.ws.index,
        ));
    }
    if extra > 0 {
        line = line.push(label(format!("+{extra}"), 12, DIM));
    }
    let wins: Vec<(String, String)> = row
        .windows
        .iter()
        .map(|w| (w.app_id.clone(), w.title.clone()))
        .collect();
    let missing: Vec<String> = row
        .pinned_missing
        .iter()
        .map(|a| display_name(a, apps))
        .collect();
    let text = row_label(&titles_text(&wins, 28), &missing);
    line = line.push(
        widget::container(label(text, 12, DIM))
            .width(Length::Fill)
            .clip(true),
    );
    widget::container(line)
        .width(Length::Fill)
        .padding([4, 8])
        .class(bg(ROW_BG, row.ws.active.then_some(BLUE), 8.0))
        .into()
}

fn panel_view(row: &Row, apps: &AppIndex, pins: &[Pin]) -> Element<'static, Message> {
    let head = widget::Row::new()
        .align_y(Alignment::Center)
        .push(number_dot(row));
    let mut col = widget::Column::new().spacing(4).push(head);
    for it in items(row) {
        let (title, dim, active) = match &it {
            Item::Open(w) => (
                if w.title.trim().is_empty() {
                    w.app_id.clone()
                } else {
                    w.title.trim().to_string()
                },
                false,
                w.active,
            ),
            Item::Pinned(a) => (display_name(a, apps), true, false),
        };
        let line = widget::Row::new()
            .spacing(6)
            .align_y(Alignment::Center)
            .push(app_icon(
                it.app_id(),
                apps,
                dim,
                !dim && is_pinned(pins, it.app_id(), row.ws.index),
            ))
            .push(label(
                truncate(&title, 12),
                11,
                if dim { DIM } else { WHITE },
            ));
        let cell = widget::container(line)
            .width(Length::Fill)
            .padding([2, 4])
            .class(bg(Color::TRANSPARENT, active.then_some(BLUE), 6.0));
        col = col.push(clickable(
            cell.into(),
            left_msg(&it, row.ws.id),
            it.app_id(),
            row.ws.index,
        ));
    }
    widget::container(col)
        .width(Length::Fixed(PANEL_W))
        .height(Length::Fill)
        .padding(6)
        .class(bg(ROW_BG, row.ws.active.then_some(BLUE), 10.0))
        .into()
}

/// Filas de los escritorios del monitor. Se pasan todas las ventanas: una app fijada que
/// está abierta en otro monitor no cuenta como «sin abrir».
pub fn build_rows(snap: &Snapshot, surface_output: Option<&str>, pins: &[Pin]) -> Vec<Row> {
    let (ws, _) = visible(snap, surface_output);
    rows(&ws, &snap.windows, pins)
}

/// ¿La app está fijada a este escritorio (posición `index`)?
pub fn is_pinned(pins: &[Pin], app_id: &str, index: u32) -> bool {
    !app_id.is_empty()
        && pins
            .iter()
            .any(|p| p.workspace == index && p.app_id.eq_ignore_ascii_case(app_id))
}

/// Cuántos íconos se dibujan y cuántos quedan ocultos («+k»).
pub fn split_cap(total: usize, max: usize) -> (usize, usize) {
    (total.min(max), total.saturating_sub(max))
}

/// Contenido de la página (sin título genérico ni puntos).
pub fn view(
    snap: &Snapshot,
    surface_output: Option<&str>,
    layout: DesktopsLayout,
    pins: &[Pin],
    apps: &AppIndex,
) -> Element<'static, Message> {
    let rows = build_rows(snap, surface_output, pins);
    match layout {
        DesktopsLayout::Rows => {
            let col = rows.iter().fold(
                widget::Column::new().spacing(4).width(Length::Fill),
                |c, r| c.push(row_view(r, apps, pins)),
            );
            widget::scrollable(col).height(Length::Fill).into()
        }
        DesktopsLayout::Columns => {
            let row = rows.iter().fold(widget::Row::new().spacing(8), |c, r| {
                c.push(panel_view(r, apps, pins))
            });
            widget::scrollable::horizontal(row)
                .height(Length::Fill)
                .width(Length::Fill)
                .into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(app: &str, title: &str) -> (String, String) {
        (app.into(), title.into())
    }

    #[test]
    fn titulos_recortados_y_con_coma() {
        let t = titles_text(&[w("brave", "Brave · GitHub"), w("kitty", "kitty")], 40);
        assert_eq!(t, "Brave · GitHub, kitty");
        let t = titles_text(&[w("a", "un título larguísimo de ventana")], 10);
        assert_eq!(t.chars().count(), 10);
        assert!(t.ends_with('…'));
    }

    #[test]
    fn titulo_vacio_usa_app_id() {
        assert_eq!(titles_text(&[w("kitty", "  ")], 20), "kitty");
        assert_eq!(titles_text(&[], 20), "");
    }

    #[test]
    fn etiqueta_sin_abrir() {
        crate::i18n::apply_for_test("es");
        let m = |s: &str| vec![s.to_string()];
        assert_eq!(
            row_label("", &m("GitHub Desktop")),
            "GitHub Desktop (sin abrir)"
        );
        assert_eq!(row_label("kitty", &m("Code")), "kitty, Code (sin abrir)");
        assert_eq!(row_label("kitty", &[]), "kitty");
        assert_eq!(
            row_label("", &["A".into(), "B".into()]),
            "A (sin abrir), B (sin abrir)"
        );
        assert_eq!(row_label("", &[]), "");
    }

    #[test]
    fn avisos_de_fijacion() {
        crate::i18n::apply_for_test("es");
        assert_eq!(
            pin_notice("VS Code", Some(2)),
            "VS Code fijada en el escritorio 2"
        );
        assert_eq!(pin_notice("VS Code", None), "VS Code ya no está fijada");
    }

    fn ws(id: u32, index: u32, output: &str, active: bool) -> super::super::Workspace {
        super::super::Workspace {
            id,
            name: index.to_string(),
            active,
            output: Some(output.into()),
            index,
        }
    }

    #[test]
    fn abierta_en_otro_monitor_no_es_faltante() {
        use super::super::{Snapshot, Window};
        let snap = Snapshot {
            workspaces: vec![
                ws(1, 1, "A", true),
                ws(2, 2, "A", false),
                ws(3, 1, "B", false),
            ],
            windows: vec![Window {
                id: 9,
                app_id: "kitty".into(),
                title: "kitty".into(),
                workspace: Some(3),
                active: false,
            }],
        };
        let pins = vec![Pin {
            app_id: "kitty".into(),
            workspace: 2,
        }];
        let r = build_rows(&snap, Some("A"), &pins);
        assert_eq!(r.len(), 2);
        assert!(r[1].pinned_missing.is_empty());
        assert!(r.iter().all(|x| x.windows.is_empty()));
    }

    #[test]
    fn marca_de_fijada() {
        let pins = vec![Pin {
            app_id: "Code".into(),
            workspace: 2,
        }];
        assert!(is_pinned(&pins, "code", 2));
        assert!(!is_pinned(&pins, "code", 1));
        assert!(!is_pinned(&pins, "", 2));
    }

    #[test]
    fn tope_de_iconos() {
        assert_eq!(split_cap(3, 8), (3, 0));
        assert_eq!(split_cap(8, 8), (8, 0));
        assert_eq!(split_cap(11, 8), (8, 3));
    }
}
