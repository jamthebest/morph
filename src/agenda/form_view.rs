//! Vista del formulario de tarea/alarma (dentro de la isla expandida).

use super::form::{Form, FormKind, Spinner};
use crate::app::Message;
use crate::scroll::Delta;
use chrono::Timelike;
use cosmic::iced::mouse::ScrollDelta;
use cosmic::iced::{Alignment, Background, Border, Color, Length};
use cosmic::{Element, widget};

/// Id del campo de título (foco inicial).
pub fn title_id() -> widget::Id {
    widget::Id::new("morph-form-title")
}

fn header(f: &Form) -> String {
    let date = f.date.map(crate::clock::short_date);
    match (f.editing, f.kind) {
        (Some(_), FormKind::Task) => crate::fl!("form-edit-task"),
        (Some(_), FormKind::Alarm) => crate::fl!("form-edit-alarm"),
        (None, FormKind::Task) => match date {
            Some(date) => crate::fl!("form-new-task-dated", date = date),
            None => crate::fl!("form-new-task"),
        },
        (None, FormKind::Alarm) => match date {
            Some(date) => crate::fl!("form-new-alarm-dated", date = date),
            None => crate::fl!("form-new-alarm"),
        },
    }
}

fn white(s: impl Into<String>, size: u16) -> widget::Text<'static, cosmic::Theme> {
    widget::text(s.into())
        .size(size)
        .class(cosmic::theme::Text::Color(Color::WHITE))
}

const SPIN_BG: Color = Color::from_rgb(0.082, 0.082, 0.082); // #151515
const LIGHT_BLUE: Color = Color::from_rgb(0.4, 0.8, 1.0); // #6cf

/// Campo de hora sobre el fondo oscuro del selector: sin borde, texto blanco.
fn input_style(_: &cosmic::Theme) -> widget::text_input::Appearance {
    widget::text_input::Appearance {
        background: Background::Color(SPIN_BG),
        border_radius: 6.0.into(),
        border_offset: None,
        border_width: 0.0,
        border_color: Color::TRANSPARENT,
        label_color: Color::WHITE,
        placeholder_color: Color::from_rgb(0.5, 0.5, 0.5),
        selected_text_color: Color::WHITE,
        icon_color: None,
        text_color: Some(Color::WHITE),
        selected_fill: Color::from_rgba(0.4, 0.8, 1.0, 0.4),
    }
}

/// Un selector con ▲ arriba y ▼ abajo de un número grande; la rueda encima también lo mueve.
fn spinner(which: Spinner, text: &str, up: Message, down: Message) -> Element<'static, Message> {
    let arrow = |label: &'static str, m: Message| {
        widget::button::custom(
            widget::container(white(label, 11))
                .width(Length::Fill)
                .align_x(Alignment::Center),
        )
        .padding([2, 12])
        .width(Length::Fill)
        .class(cosmic::theme::Button::Text)
        .on_press(m)
    };
    // Separación para que el contorno de foco de ▲ no quede tapado por el campo
    // (el campo se dibuja después y se superpone).
    let col = widget::Column::new()
        .align_x(Alignment::Center)
        .spacing(4)
        .push(arrow("▲", up))
        .push(
            widget::text_input("", text.to_string())
                .on_input(move |v| match which {
                    Spinner::Hour => Message::FormHourText(v),
                    Spinner::Minute => Message::FormMinuteText(v),
                })
                .on_submit(|_| Message::FormSave)
                .size(22)
                .width(Length::Fixed(44.0))
                .padding([2, 10])
                .style(cosmic::theme::TextInput::Custom {
                    active: Box::new(input_style),
                    error: Box::new(input_style),
                    hovered: Box::new(input_style),
                    focused: Box::new(input_style),
                    disabled: Box::new(input_style),
                }),
        )
        .push(arrow("▼", down));
    let boxed =
        widget::container(col)
            .width(Length::Fixed(64.0))
            .class(cosmic::theme::Container::custom(|_| {
                widget::container::Style {
                    background: Some(Background::Color(SPIN_BG)),
                    border: Border {
                        radius: 10.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }));
    widget::mouse_area(boxed)
        .on_scroll(move |d| {
            Message::FormWheel(
                which,
                match d {
                    ScrollDelta::Lines { y, .. } => Delta::Lines(y),
                    ScrollDelta::Pixels { y, .. } => Delta::Pixels(y),
                },
            )
        })
        .into()
}

/// Segmento "a. m. | p. m." del selector de 12 h.
fn ampm(pm: bool) -> Element<'static, Message> {
    let seg = |label: String, on: bool, m: Message| {
        let b = widget::button::text(label).on_press(m).padding([2, 8]);
        if on {
            b.class(cosmic::theme::Button::Suggested)
        } else {
            b.class(cosmic::theme::Button::Standard)
        }
    };
    widget::Column::new()
        .spacing(4)
        .push(seg(crate::fl!("form-am"), !pm, Message::FormPm(false)))
        .push(seg(crate::fl!("form-pm"), pm, Message::FormPm(true)))
        .into()
}

fn link(label: String, m: Message) -> Element<'static, Message> {
    widget::button::custom(
        widget::text(label)
            .size(12)
            .class(cosmic::theme::Text::Color(LIGHT_BLUE)),
    )
    .padding([2, 4])
    .class(cosmic::theme::Button::Text)
    .on_press(m)
    .into()
}

/// Fila de hora: dos selectores y "Quitar hora", o "Sin hora" y "Agregar hora".
fn time_row(f: &Form) -> Element<'static, Message> {
    let mut row = widget::Row::new().spacing(8).align_y(Alignment::Center);
    match f.time {
        Some(_) => {
            row = row
                .push(spinner(
                    Spinner::Hour,
                    &f.hour_text,
                    Message::FormHour(1),
                    Message::FormHour(-1),
                ))
                .push(white(":", 22).font(cosmic::font::semibold()))
                .push(spinner(
                    Spinner::Minute,
                    &f.min_text,
                    Message::FormMinute(1),
                    Message::FormMinute(-1),
                ));
            if f.h12 {
                let pm = f.time.is_some_and(|t| t.hour() >= 12);
                row = row.push(ampm(pm));
            }
            if f.kind == FormKind::Task {
                row = row.push(link(
                    crate::fl!("form-remove-time"),
                    Message::FormRemoveTime,
                ));
            }
        }
        None => {
            row = row
                .push(white(crate::fl!("form-no-time"), 13))
                .push(link(crate::fl!("form-add-time"), Message::FormAddTime));
        }
    }
    row.into()
}

pub fn view(f: &Form) -> Element<'_, Message> {
    let mut col = widget::Column::new().spacing(6).width(Length::Fill);
    let mut head = widget::Row::new().align_y(Alignment::Center).spacing(8);
    head = head.push(white(header(f), 16));
    if f.editing.is_none() {
        let pick = |label: String, k: FormKind| {
            let b = widget::button::text(label)
                .on_press(Message::FormKind(k))
                .padding([2, 8]);
            if f.kind == k {
                b.class(cosmic::theme::Button::Suggested)
            } else {
                b.class(cosmic::theme::Button::Standard)
            }
        };
        head = head
            .push(widget::space().width(Length::Fill))
            .push(pick(crate::fl!("form-kind-task"), FormKind::Task))
            .push(pick(crate::fl!("form-kind-alarm"), FormKind::Alarm));
    }
    col = col.push(head);

    col = col.push(
        widget::text_input(crate::fl!("form-title-placeholder"), &f.title)
            .id(title_id())
            .on_input(Message::FormTitle)
            .on_submit(|_| Message::FormSave),
    );

    col = col.push(time_row(f));

    match f.kind {
        FormKind::Task if f.time.is_some() => {
            col = col.push(
                widget::text_input(crate::fl!("form-duration-placeholder"), &f.duration)
                    .on_input(Message::FormDuration)
                    .on_submit(|_| Message::FormSave)
                    .width(Length::Fixed(140.0)),
            );
            col = col.push(
                widget::Row::new()
                    .spacing(12)
                    .push(
                        widget::checkbox(f.remind_start)
                            .label(crate::fl!("form-remind-start"))
                            .on_toggle(Message::FormRemindStart),
                    )
                    .push(
                        widget::checkbox(f.remind_end)
                            .label(crate::fl!("form-remind-end"))
                            .on_toggle(Message::FormRemindEnd),
                    ),
            );
        }
        FormKind::Alarm => {
            let days = (0..7).fold(
                widget::Row::new().spacing(4).align_y(Alignment::Center),
                |r, i| {
                    let b = widget::button::text(super::month::week_letter(i))
                        .on_press(Message::FormDay(i))
                        .padding([2, 8]);
                    r.push(if f.days[i] {
                        b.class(cosmic::theme::Button::Suggested)
                    } else {
                        b.class(cosmic::theme::Button::Standard)
                    })
                },
            );
            col = col.push(days);
        }
        _ => {}
    }

    if let Some(e) = &f.error {
        col = col.push(
            widget::text(e.clone())
                .size(13)
                .class(cosmic::theme::Text::Color(Color::from_rgb(1.0, 0.4, 0.4))),
        );
    }

    let mut buttons = widget::Row::new().spacing(8).align_y(Alignment::Center);
    if f.editing.is_some() {
        buttons = buttons.push(
            widget::button::text(crate::fl!("form-delete"))
                .on_press(Message::FormDelete)
                .class(cosmic::theme::Button::Destructive)
                .padding([2, 8]),
        );
    }
    buttons = buttons
        .push(widget::space().width(Length::Fill))
        .push(
            widget::button::text(crate::fl!("form-cancel"))
                .on_press(Message::FormCancel)
                .padding([2, 8]),
        )
        .push(
            widget::button::text(crate::fl!("form-save"))
                .on_press(Message::FormSave)
                .class(cosmic::theme::Button::Suggested)
                .padding([2, 8]),
        );
    col.push(buttons).into()
}
