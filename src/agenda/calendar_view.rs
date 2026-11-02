//! Vista de la página Calendario (dentro de la isla expandida).

use super::form::Form;
use super::month::{long_date, month_grid, month_title, week_letter};
use super::{Agenda, Item, ItemId, Kind};
use crate::app::Message;
use crate::clock::fmt_hm;
use chrono::{DateTime, Datelike, Local, NaiveDate};
use cosmic::iced::{Alignment, Background, Border, Color, Length};
use cosmic::{Element, widget};

const WHITE: Color = Color::WHITE;
const DIM: Color = Color::from_rgb(0.6, 0.6, 0.6);
const FAINT: Color = Color::from_rgb(0.33, 0.33, 0.33);
const BLUE: Color = Color::from_rgb(0.176, 0.424, 0.875); // #2d6cdf
const ORANGE: Color = Color::from_rgb(0.933, 0.6, 0.4); // #e96
const LIGHT_BLUE: Color = Color::from_rgb(0.4, 0.8, 1.0); // #6cf
const ROW_BG: Color = Color::from_rgb(0.082, 0.082, 0.082); // #151515
const TRASH: Color = Color::from_rgb(0.467, 0.467, 0.467); // #777

fn text(s: impl Into<String>, size: u16, color: Color) -> widget::Text<'static, cosmic::Theme> {
    widget::text(s.into())
        .size(size)
        .wrapping(cosmic::iced::widget::text::Wrapping::None)
        .class(cosmic::theme::Text::Color(color))
}

/// Resumen de los días de una alarma: "L–V", "Diario", "Una vez" o las letras sueltas.
pub fn days_label(days: &[bool; 7]) -> String {
    if days.iter().all(|&d| d) {
        crate::fl!("cal-days-daily")
    } else if !days.iter().any(|&d| d) {
        crate::fl!("cal-days-once")
    } else if days[..5].iter().all(|&d| d) && !days[5] && !days[6] {
        crate::fl!("cal-days-weekdays")
    } else {
        days.iter()
            .enumerate()
            .filter(|(_, on)| **on)
            .map(|(i, _)| day_short(i))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn day_short(i: usize) -> String {
    match i {
        0 => crate::fl!("cal-day-short-1"),
        1 => crate::fl!("cal-day-short-2"),
        2 => crate::fl!("cal-day-short-3"),
        3 => crate::fl!("cal-day-short-4"),
        4 => crate::fl!("cal-day-short-5"),
        5 => crate::fl!("cal-day-short-6"),
        _ => crate::fl!("cal-day-short-7"),
    }
}

fn day_cell(
    date: NaiveDate,
    view_month: u32,
    today: NaiveDate,
    selected: NaiveDate,
    has_items: bool,
) -> Element<'static, Message> {
    let is_today = date == today;
    let is_sel = date == selected;
    let color = if is_today || date.month() == view_month {
        WHITE
    } else {
        FAINT
    };
    let dot = widget::container(widget::space())
        .width(Length::Fixed(3.0))
        .height(Length::Fixed(3.0))
        .class(cosmic::theme::Container::custom(move |_| {
            widget::container::Style {
                background: has_items.then_some(Background::Color(ORANGE)),
                border: Border {
                    radius: 1.5.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }));
    let content = widget::Column::new()
        .align_x(Alignment::Center)
        .push(text(date.day().to_string(), 12, color))
        .push(dot);
    let cell = widget::container(content)
        .width(Length::Fixed(26.0))
        .height(Length::Fixed(22.0))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .class(cosmic::theme::Container::custom(move |_| {
            widget::container::Style {
                background: is_today.then_some(Background::Color(BLUE)),
                border: Border {
                    radius: 5.0.into(),
                    width: if is_sel { 1.0 } else { 0.0 },
                    color: WHITE,
                },
                ..Default::default()
            }
        }));
    widget::button::custom(cell)
        .padding(0)
        .class(cosmic::theme::Button::Text)
        .on_press(Message::CalSelect(date))
        .into()
}

fn nav_button(label: &'static str, delta: i32) -> Element<'static, Message> {
    widget::button::custom(text(label, 16, WHITE))
        .padding([0, 8])
        .class(cosmic::theme::Button::Text)
        .on_press(Message::CalShift(delta))
        .into()
}

const GRID_W: f32 = 7.0 * 26.0 + 6.0 * 3.0;

fn left_column(
    agenda: &Agenda,
    today: NaiveDate,
    view: (i32, u32),
    selected: NaiveDate,
) -> Element<'static, Message> {
    let head = widget::Row::new()
        .align_y(Alignment::Center)
        .push(nav_button("‹", -1))
        .push(
            widget::container(text(month_title(view.0, view.1), 14, WHITE))
                .width(Length::Fill)
                .align_x(Alignment::Center),
        )
        .push(nav_button("›", 1));
    let mut grid = widget::Column::new().spacing(3);
    grid = grid.push((0..7).fold(widget::Row::new().spacing(3), |r, i| {
        r.push(
            widget::container(text(week_letter(i), 11, DIM))
                .width(Length::Fixed(26.0))
                .align_x(Alignment::Center),
        )
    }));
    for week in month_grid(view.0, view.1) {
        grid = grid.push(week.iter().fold(widget::Row::new().spacing(3), |r, &d| {
            r.push(day_cell(d, view.1, today, selected, agenda.has_items(d)))
        }));
    }
    // Ancho fijo = rejilla (7 celdas de 26 + 6 huecos de 3): con `Shrink`, el título
    // `Fill` de la cabecera estiraba la columna y la rejilla quedaba a la izquierda.
    widget::Column::new()
        .spacing(4)
        .width(Length::Fixed(GRID_W))
        .push(head)
        .push(grid)
        .into()
}

/// Botón de papelera (atenuado) que borra la fila.
pub fn trash_button(id: ItemId) -> Element<'static, Message> {
    let icon = widget::icon::from_name("edit-delete-symbolic")
        .size(16)
        .symbolic(true)
        .icon()
        .class(cosmic::theme::Svg::custom(|_| {
            cosmic::iced::widget::svg::Style { color: Some(TRASH) }
        }));
    widget::button::custom(icon)
        .padding([0, 6])
        .class(cosmic::theme::Button::Text)
        .on_press(Message::RowDelete(id))
        .into()
}

/// Barra «Borrado «título» · Deshacer» (sustituye a la fila de alta durante 5 s).
pub fn undo_bar(title: &str) -> Element<'static, Message> {
    widget::Row::new()
        .align_y(Alignment::Center)
        .spacing(4)
        .push(text(
            crate::fl!("cal-deleted", title = truncate(title, 24)),
            12,
            DIM,
        ))
        .push(
            widget::button::custom(text(crate::fl!("cal-undo"), 12, LIGHT_BLUE))
                .padding([4, 4])
                .class(cosmic::theme::Button::Text)
                .on_press(Message::UndoDelete),
        )
        .into()
}

/// Recorta a `max` caracteres añadiendo "…" (la ventana no hace elipsis por sí sola).
pub fn truncate(t: &str, max: usize) -> String {
    if t.chars().count() > max {
        let head: String = t.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", head.trim_end())
    } else {
        t.to_string()
    }
}

fn item_row(it: &Item, h12: bool) -> Element<'static, Message> {
    let (done, info) = match &it.kind {
        Kind::Task { time, done, .. } => (*done, time.map(|t| fmt_hm(t, h12)).unwrap_or_default()),
        Kind::Alarm { time, days, .. } => (
            false,
            format!("{} · {}", fmt_hm(*time, h12), days_label(days)),
        ),
    };
    // Una alarma pospuesta sigue sonando: se muestra como activa.
    let off = matches!(it.kind, Kind::Alarm { enabled: false, .. }) && it.snooze_until.is_none();
    let color = if done || off { DIM } else { WHITE };
    let lead: Element<'static, Message> = match &it.kind {
        Kind::Task { .. } => widget::button::custom(text(if done { "☑" } else { "☐" }, 15, color))
            .padding([0, 4])
            .class(cosmic::theme::Button::Text)
            .on_press(Message::TaskToggle(it.id))
            .into(),
        Kind::Alarm { .. } => widget::container(text("⏰", 13, if off { DIM } else { ORANGE }))
            .padding([0, 4])
            .into(),
    };
    let mut title_col = widget::Column::new().push(text(short_title(&it.title), 13, color));
    if let Some(s) = it.snooze_until {
        title_col = title_col.push(text(
            crate::fl!("cal-snoozed-until", time = fmt_hm(s.time(), h12)),
            11,
            DIM,
        ));
    }
    let body = widget::Row::new()
        .align_y(Alignment::Center)
        .spacing(6)
        .push(widget::container(title_col).width(Length::Fill))
        .push(text(info, 11, DIM));
    let edit = widget::button::custom(body)
        .padding([4, 4])
        .width(Length::Fill)
        .class(cosmic::theme::Button::Text)
        .on_press(Message::FormOpen(Form::edit(it)));
    let mut line = widget::Row::new()
        .align_y(Alignment::Center)
        .push(lead)
        .push(edit);
    if let Kind::Alarm { enabled, .. } = &it.kind {
        let id = it.id;
        line = line.push(
            widget::toggler(*enabled || it.snooze_until.is_some())
                .on_toggle(move |v| Message::AlarmToggle(id, v)),
        );
    }
    widget::container(line.push(trash_button(it.id)))
        .width(Length::Fill)
        .class(cosmic::theme::Container::custom(|_| {
            widget::container::Style {
                background: Some(Background::Color(ROW_BG)),
                border: Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

fn short_title(t: &str) -> String {
    truncate(t, 22)
}

fn right_column(
    agenda: &Agenda,
    now: DateTime<Local>,
    selected: NaiveDate,
    undo: Option<&str>,
    h12: bool,
) -> Element<'static, Message> {
    let items = agenda.items_on(selected);
    let list: Element<'static, Message> = if items.is_empty() {
        widget::container(text(crate::fl!("cal-empty-day"), 12, DIM))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        widget::scrollable(
            items
                .into_iter()
                .fold(widget::Column::new().spacing(4), |c, it| {
                    c.push(item_row(it, h12))
                }),
        )
        .height(Length::Fill)
        .into()
    };
    let add: Element<'static, Message> = match undo {
        Some(title) => undo_bar(title),
        None => widget::button::custom(text(crate::fl!("cal-add"), 12, LIGHT_BLUE))
            .padding([4, 4])
            .class(cosmic::theme::Button::Text)
            .on_press(Message::FormOpen(Form::new_task(Some(selected))))
            .into(),
    };
    widget::Column::new()
        .spacing(4)
        .width(Length::Fill)
        .height(Length::Fill)
        .push(text(fmt_hm(now.time(), h12), 22, WHITE).font(cosmic::font::semibold()))
        .push(text(long_date(selected), 12, DIM))
        .push(list)
        .push(add)
        .into()
}

pub fn view(
    agenda: &Agenda,
    now: DateTime<Local>,
    view: (i32, u32),
    selected: NaiveDate,
    undo: Option<&str>,
    h12: bool,
) -> Element<'static, Message> {
    widget::Row::new()
        .spacing(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .push(left_column(agenda, now.date_naive(), view, selected))
        .push(right_column(agenda, now, selected, undo, h12))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_sin_panico() {
        assert_eq!(truncate("corto", 10), "corto");
        assert_eq!(truncate("abcdef", 0), "…");
        assert_eq!(truncate("abcdef", 3).chars().count(), 3);
    }

    #[test]
    fn etiquetas_de_dias() {
        crate::i18n::apply_for_test("es");
        assert_eq!(days_label(&[true; 7]), "Diario");
        assert_eq!(days_label(&[false; 7]), "Una vez");
        assert_eq!(
            days_label(&[true, true, true, true, true, false, false]),
            "L–V"
        );
        assert_eq!(
            days_label(&[true, false, true, false, false, false, true]),
            "L X D"
        );
        crate::i18n::apply_for_test("en");
        assert_eq!(days_label(&[true; 7]), "Daily");
        assert_eq!(days_label(&[false; 7]), "Once");
        assert_eq!(
            days_label(&[true, true, true, true, true, false, false]),
            "Mon–Fri"
        );
        assert_eq!(
            days_label(&[true, false, true, false, false, false, true]),
            "Mo We Su"
        );
    }
}
