//! Vista de la página To-do (dentro de la isla expandida).

use super::calendar_view::{trash_button, truncate, undo_bar};
use super::form::Form;
use super::{Agenda, Item, Kind};
use crate::app::Message;
use chrono::NaiveDate;
use cosmic::iced::{Alignment, Background, Border, Color, Length};
use cosmic::{Element, widget};

const WHITE: Color = Color::WHITE;
const DIM: Color = Color::from_rgb(0.6, 0.6, 0.6);
const ROW_BG: Color = Color::from_rgb(0.082, 0.082, 0.082); // #151515

fn text(s: impl Into<String>, size: u16, color: Color) -> widget::Text<'static, cosmic::Theme> {
    widget::text(s.into())
        .size(size)
        .wrapping(cosmic::iced::widget::text::Wrapping::None)
        .class(cosmic::theme::Text::Color(color))
}

fn duration_label(min: u32) -> String {
    match (min / 60, min % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// Texto de la derecha de una fila: "hoy 15:00 · 1 h", "mañana", "vie 6 nov", "sin fecha".
pub fn todo_when(item: &Item, today: NaiveDate, h12: bool) -> String {
    let Kind::Task {
        date,
        time,
        duration_min,
        ..
    } = &item.kind
    else {
        return String::new();
    };
    let Some(date) = date else {
        return crate::fl!("todo-no-date");
    };
    let mut s = if *date == today {
        crate::fl!("todo-today")
    } else if today.succ_opt() == Some(*date) {
        crate::fl!("todo-tomorrow")
    } else {
        crate::clock::short_date(*date)
    };
    if let Some(t) = time {
        s.push_str(&format!(" {}", crate::clock::fmt_hm(*t, h12)));
        if let Some(d) = duration_min {
            s.push_str(&format!(" · {}", duration_label(*d)));
        }
    }
    s
}

fn short_title(t: &str) -> String {
    truncate(t, 28)
}

fn row(it: &Item, today: NaiveDate, h12: bool) -> Element<'static, Message> {
    let done = matches!(it.kind, Kind::Task { done: true, .. });
    let color = if done { DIM } else { WHITE };
    let check = widget::button::custom(text(if done { "☑" } else { "☐" }, 15, color))
        .padding([0, 4])
        .class(cosmic::theme::Button::Text)
        .on_press(Message::TaskToggle(it.id));
    let title: Element<'static, Message> = if done {
        let span: cosmic::iced::advanced::text::Span<'_, ()> =
            cosmic::iced::widget::span(short_title(&it.title))
                .size(13)
                .color(color)
                .strikethrough(true);
        cosmic::iced::widget::rich_text([span])
            .wrapping(cosmic::iced::widget::text::Wrapping::None)
            .into()
    } else {
        text(short_title(&it.title), 13, color).into()
    };
    let body = widget::Row::new()
        .align_y(Alignment::Center)
        .spacing(6)
        .push(widget::container(title).width(Length::Fill))
        .push(text(todo_when(it, today, h12), 11, DIM));
    let edit = widget::button::custom(body)
        .padding([4, 4])
        .width(Length::Fill)
        .class(cosmic::theme::Button::Text)
        .on_press(Message::FormOpen(Form::edit(it)));
    widget::container(
        widget::Row::new()
            .align_y(Alignment::Center)
            .push(check)
            .push(edit)
            .push(trash_button(it.id)),
    )
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

/// Título, lista desplazable (llena el alto) y fila de alta rápida al final.
pub fn view<'a>(
    agenda: &Agenda,
    today: NaiveDate,
    draft: &str,
    undo: Option<&str>,
    h12: bool,
) -> Element<'a, Message> {
    let items = agenda.todo_order();
    let list: Element<'a, Message> = if items.is_empty() {
        widget::container(text(crate::fl!("todo-empty"), 12, DIM))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    } else {
        widget::scrollable(
            items
                .into_iter()
                .fold(widget::Column::new().spacing(4), |c, it| {
                    c.push(row(it, today, h12))
                }),
        )
        .height(Length::Fill)
        .into()
    };
    let add: Element<'a, Message> = match undo {
        Some(title) => undo_bar(title),
        None => quick_add(draft),
    };
    widget::Column::new()
        .spacing(6)
        .width(Length::Fill)
        .height(Length::Fill)
        .push(
            widget::container(
                text(crate::fl!("todo-heading"), 16, WHITE).font(cosmic::font::semibold()),
            )
            .width(Length::Fill)
            .align_x(Alignment::Center),
        )
        .push(list)
        .push(add)
        .into()
}

fn quick_add<'a>(draft: &str) -> Element<'a, Message> {
    widget::Row::new()
        .spacing(8)
        .align_y(Alignment::Center)
        .push(
            widget::text_input(crate::fl!("todo-new-placeholder"), draft.to_string())
                .on_input(Message::TodoDraft)
                .on_submit(|_| Message::TodoAdd)
                .width(Length::Fill),
        )
        .push(
            widget::button::text(crate::fl!("todo-add"))
                .class(cosmic::theme::Button::Suggested)
                .padding([4, 12])
                .on_press(Message::TodoAdd),
        )
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }
    fn item(date: Option<NaiveDate>, time: Option<(u32, u32)>, dur: Option<u32>) -> Item {
        Item {
            id: 1,
            title: "x".into(),
            kind: Kind::Task {
                date,
                time: time.map(|(h, m)| NaiveTime::from_hms_opt(h, m, 0).unwrap()),
                duration_min: dur,
                remind_start: true,
                remind_end: false,
                done: false,
            },
            snooze_until: None,
        }
    }
    const TODAY: fn() -> NaiveDate = || d(2026, 10, 30);

    #[test]
    fn hoy_con_hora_y_duracion() {
        crate::i18n::apply_for_test("es");
        let it = item(Some(TODAY()), Some((15, 0)), Some(60));
        assert_eq!(todo_when(&it, TODAY(), false), "hoy 15:00 · 1 h");
    }

    #[test]
    fn hora_12h() {
        crate::i18n::apply_for_test("es");
        let it = item(Some(TODAY()), Some((15, 0)), Some(60));
        assert_eq!(todo_when(&it, TODAY(), true), "hoy 3:00 p. m. · 1 h");
    }

    #[test]
    fn manana_sin_hora() {
        crate::i18n::apply_for_test("es");
        assert_eq!(
            todo_when(&item(Some(d(2026, 10, 31)), None, None), TODAY(), false),
            "mañana"
        );
    }

    #[test]
    fn otra_fecha_y_fin_de_mes() {
        crate::i18n::apply_for_test("es");
        assert_eq!(
            todo_when(&item(Some(d(2026, 11, 6)), None, None), TODAY(), false),
            "vie 6 nov"
        );
        let t = d(2026, 10, 31);
        assert_eq!(
            todo_when(&item(Some(d(2026, 11, 1)), None, None), t, false),
            "mañana"
        );
    }

    #[test]
    fn sin_fecha() {
        crate::i18n::apply_for_test("es");
        assert_eq!(
            todo_when(&item(None, None, None), TODAY(), false),
            "sin fecha"
        );
    }

    #[test]
    fn duraciones() {
        crate::i18n::apply_for_test("es");
        let w = |m| todo_when(&item(Some(TODAY()), Some((9, 5)), Some(m)), TODAY(), false);
        assert_eq!(w(45), "hoy 09:05 · 45 min");
        assert_eq!(w(60), "hoy 09:05 · 1 h");
        assert_eq!(w(90), "hoy 09:05 · 1 h 30 min");
    }
}
