//! Vista «asomada»: batería, RAM, CPU y volumen a los lados de la pastilla con el cursor encima.

use cosmic::Element;
use cosmic::iced::{Alignment, Color, Length};
use cosmic::widget;
use cosmic::widget::icon::IconFallback;

use crate::geometry::{PEEK_EDGE, PEEK_ITEM, Size, peek_left_items};
use crate::sys::rules::HudValue;

const GREY: Color = Color::from_rgb(0.6, 0.6, 0.6);

/// Datos de la vista asomada (todo opcional: «—» hasta tener lectura).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    /// (porcentaje, enchufada); None = sin batería.
    pub battery: Option<(f64, bool)>,
    pub ram: Option<u8>,
    pub cpu: Option<u8>,
    pub volume: Option<HudValue>,
}

/// Condiciones para mostrar la vista asomada (el HUD y los avisos tienen prioridad).
pub struct PeekGate {
    pub hovered: bool,
    pub collapsed: bool,
    pub dragging: bool,
    pub notice: bool,
    pub hud: bool,
    pub open_on_hover: bool,
}

pub fn should_peek(g: &PeekGate) -> bool {
    g.hovered && g.collapsed && !g.dragging && !g.notice && !g.hud && !g.open_on_hover
}

/// ¿Se muestra solo la línea (modo «Ocultar en una línea»)? Cerrada y sin hover; el HUD,
/// los avisos y el arrastre de la pastilla la reemplazan por la pastilla.
pub fn show_line(
    hide_line: bool,
    collapsed: bool,
    hovered: bool,
    notice: bool,
    hud: bool,
    dragging: bool,
) -> bool {
    hide_line && collapsed && !hovered && !notice && !hud && !dragging
}

/// ¿Corre el temporizador de 1 s que devuelve la pastilla a la línea? Solo con la pastilla
/// mostrada por hover, el puntero fuera y sin arrastre en curso.
pub fn leave_timer_needed(
    hide_line: bool,
    line_hover: bool,
    pointer_inside: bool,
    pill_drag: bool,
) -> bool {
    hide_line && line_hover && !pointer_inside && !pill_drag
}

/// Tiempo que le queda a un aviso tras una pausa: lo que faltaba al pausar.
pub fn remaining_after_pause(
    deadline: std::time::Instant,
    paused_at: std::time::Instant,
) -> std::time::Duration {
    deadline.saturating_duration_since(paused_at)
}

/// `battery-level-NN[-charging]-symbolic`, con NN redondeado a decenas.
pub fn battery_icon_name(percent: f64, charging: bool) -> String {
    let p = if percent.is_finite() {
        percent.clamp(0.0, 100.0)
    } else {
        0.0
    };
    let level = (p / 10.0).round() as u32 * 10;
    if charging {
        format!("battery-level-{level}-charging-symbolic")
    } else {
        format!("battery-level-{level}-symbolic")
    }
}

pub fn battery_label(percent: f64) -> String {
    crate::fl!(
        "general-percent",
        n = u32::from(percent.clamp(0.0, 100.0).round() as u8)
    )
}

pub fn percent_label(p: Option<u8>) -> String {
    p.map_or_else(
        || "—".into(),
        |p| crate::fl!("general-percent", n = u32::from(p)),
    )
}

pub fn volume_label(v: Option<HudValue>) -> String {
    match v {
        Some(v) if v.muted => crate::fl!("peek-muted"),
        Some(v) => crate::fl!("general-percent", n = u32::from(v.percent)),
        None => "—".into(),
    }
}

pub fn volume_icon(v: Option<HudValue>) -> &'static str {
    match v {
        Some(v) => crate::hud::icon_name(&v),
        None => "audio-volume-medium-symbolic",
    }
}

pub fn px(base: u32, scale: u16) -> f32 {
    (base as f32 * f32::from(scale) / 100.0).round()
}

/// Un dato: ícono simbólico (con alternativas) y texto al lado.
/// Ícono del tema por nombre, con respaldos si el tema no lo trae.
pub fn named(name: &str, fallbacks: &[&str]) -> widget::icon::Handle {
    widget::icon::from_name(name.to_owned())
        .fallback(Some(IconFallback::Names(
            fallbacks
                .iter()
                .map(|s| std::borrow::Cow::Owned((*s).to_owned()))
                .collect(),
        )))
        .size(14)
        .symbolic(true)
        .handle()
}

/// Ícono propio (SVG incluido en el binario) para lo que los temas de COSMIC/Pop no traen.
fn own_icon(svg: &'static [u8]) -> widget::icon::Handle {
    let mut h = widget::icon::from_svg_bytes(svg);
    h.symbolic = true;
    h
}

/// Chip: los temas de COSMIC/Pop no traen un ícono de CPU que se vea a este tamaño.
fn cpu_icon() -> widget::icon::Handle {
    own_icon(include_bytes!("../resources/icons/cpu-symbolic.svg"))
}

/// Módulo de memoria: los temas no traen `memory-symbolic`.
fn ram_icon() -> widget::icon::Handle {
    own_icon(include_bytes!("../resources/icons/memory-symbolic.svg"))
}

pub fn item<'a, M: 'a>(
    handle: widget::icon::Handle,
    label: String,
    dim: bool,
    scale: u16,
) -> Element<'a, M> {
    let color = if dim { GREY } else { Color::WHITE };
    let icon = widget::icon(handle)
        .size(14)
        .class(cosmic::theme::Svg::custom(move |_| {
            cosmic::iced::widget::svg::Style { color: Some(color) }
        }));
    let text = widget::text(label)
        .size(11)
        .wrapping(cosmic::iced::widget::text::Wrapping::None)
        .class(cosmic::theme::Text::Color(color));
    widget::container(
        widget::Row::new()
            .spacing(4)
            .align_y(Alignment::Center)
            .push(icon)
            .push(text),
    )
    .width(Length::Fixed(px(PEEK_ITEM, scale)))
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .into()
}

/// Izquierda: batería (si hay) y RAM. Derecha: CPU y volumen. Al centro, lo de siempre.
pub fn view<'a, M: 'a>(
    stats: &Stats,
    center: Element<'a, M>,
    center_size: Size,
    scale: u16,
) -> Element<'a, M> {
    let has_battery = stats.battery.is_some();
    let mut left: Vec<Element<'a, M>> = Vec::new();
    if let Some((pct, charging)) = stats.battery {
        left.push(item(
            named(&battery_icon_name(pct, charging), &["battery-symbolic"]),
            battery_label(pct),
            false,
            scale,
        ));
    }
    left.push(item(
        ram_icon(),
        percent_label(stats.ram),
        stats.ram.is_none(),
        scale,
    ));
    let right = vec![
        item(
            cpu_icon(),
            percent_label(stats.cpu),
            stats.cpu.is_none(),
            scale,
        ),
        item(
            named(volume_icon(stats.volume), &["audio-volume-medium-symbolic"]),
            volume_label(stats.volume),
            stats.volume.is_none_or(|v| v.muted),
            scale,
        ),
    ];
    let side = |items: Vec<Element<'a, M>>, n: u32| -> Element<'a, M> {
        widget::container(widget::row::with_children(items).align_y(Alignment::Center))
            .width(Length::Fixed(px(PEEK_ITEM * n, scale)))
            .height(Length::Fill)
            .align_y(Alignment::Center)
            .into()
    };
    let center: Element<'a, M> = widget::container(center)
        .width(Length::Fixed(center_size.w as f32))
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into();
    widget::row::with_children(vec![
        side(left, peek_left_items(has_battery)),
        center,
        side(right, 2),
    ])
    .align_y(Alignment::Center)
    .padding([0.0, px(PEEK_EDGE, scale)])
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leave_timer_cases() {
        assert!(leave_timer_needed(true, true, false, false));
        assert!(!leave_timer_needed(true, true, false, true));
        assert!(!leave_timer_needed(true, true, true, false));
        assert!(!leave_timer_needed(true, false, false, false));
        assert!(!leave_timer_needed(false, true, false, false));
    }

    #[test]
    fn remaining_after_pause_cases() {
        use std::time::{Duration, Instant};
        let t = Instant::now();
        assert_eq!(
            remaining_after_pause(t + Duration::from_secs(3), t),
            Duration::from_secs(3)
        );
        assert_eq!(
            remaining_after_pause(t, t + Duration::from_secs(2)),
            Duration::ZERO
        );
    }

    #[test]
    fn show_line_cases() {
        assert!(show_line(true, true, false, false, false, false));
        assert!(!show_line(true, true, true, false, false, false));
        assert!(!show_line(true, true, false, true, false, false));
        assert!(!show_line(true, true, false, false, true, false));
        assert!(!show_line(true, true, false, false, false, true));
        assert!(!show_line(false, true, false, false, false, false));
        assert!(!show_line(true, false, false, false, false, false));
    }

    #[test]
    fn battery_icons() {
        assert_eq!(battery_icon_name(47.0, false), "battery-level-50-symbolic");
        assert_eq!(
            battery_icon_name(47.0, true),
            "battery-level-50-charging-symbolic"
        );
        assert_eq!(battery_icon_name(0.0, false), "battery-level-0-symbolic");
        assert_eq!(battery_icon_name(4.0, false), "battery-level-0-symbolic");
        assert_eq!(battery_icon_name(5.0, false), "battery-level-10-symbolic");
        assert_eq!(
            battery_icon_name(100.0, true),
            "battery-level-100-charging-symbolic"
        );
        // Fuera de rango: se acota.
        assert_eq!(
            battery_icon_name(250.0, false),
            "battery-level-100-symbolic"
        );
        assert_eq!(battery_icon_name(-3.0, false), "battery-level-0-symbolic");
        assert_eq!(
            battery_icon_name(f64::NAN, false),
            "battery-level-0-symbolic"
        );
    }

    #[test]
    fn peek_rules() {
        let ok = PeekGate {
            hovered: true,
            collapsed: true,
            dragging: false,
            notice: false,
            hud: false,
            open_on_hover: false,
        };
        assert!(should_peek(&ok));
        assert!(!should_peek(&PeekGate {
            hovered: false,
            ..ok
        }));
        assert!(!should_peek(&PeekGate {
            collapsed: false,
            ..ok
        }));
        assert!(!should_peek(&PeekGate {
            dragging: true,
            ..ok
        }));
        assert!(!should_peek(&PeekGate { notice: true, ..ok }));
        assert!(!should_peek(&PeekGate { hud: true, ..ok }));
        assert!(!should_peek(&PeekGate {
            open_on_hover: true,
            ..ok
        }));
    }

    #[test]
    fn labels() {
        crate::i18n::apply_for_test("es");
        assert_eq!(battery_label(47.4), "47 %");
        assert_eq!(battery_label(100.0), "100 %");
        assert_eq!(percent_label(Some(7)), "7 %");
        assert_eq!(percent_label(None), "—");
        crate::i18n::apply_for_test("en");
        assert_eq!(battery_label(47.4), "47%");
    }

    #[test]
    fn muted_label_follows_language() {
        let muted = HudValue {
            kind: crate::sys::rules::HudKind::Volume,
            percent: 30,
            muted: true,
        };
        crate::i18n::apply_for_test("es");
        assert_eq!(volume_label(Some(muted)), "mudo");
        crate::i18n::apply_for_test("en");
        assert_eq!(volume_label(Some(muted)), "muted");
    }
}
