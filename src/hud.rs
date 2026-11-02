//! HUD de volumen y brillo en la pastilla cerrada.

use cosmic::Element;
use cosmic::iced::{Alignment, Background, Color, Length};
use cosmic::widget;

use crate::config::{HudStyle, MorphConfig};
use crate::sys::rules::{HudKind, HudValue};

/// Duración del HUD tras el último cambio.
pub const HUD_MILLIS: u64 = 1500;

const AMBER: Color = Color::from_rgb(0.914, 0.698, 0.227);
const GREY: Color = Color::from_rgb(0.6, 0.6, 0.6);

/// ¿Se muestra este valor ahora? Isla cerrada, sin aviso fijo y habilitado en la config.
pub fn should_show(v: &HudValue, collapsed: bool, sticky_notice: bool, cfg: &MorphConfig) -> bool {
    let enabled = match v.kind {
        HudKind::Volume => cfg.hud_volume,
        HudKind::Brightness => cfg.hud_brightness,
    };
    enabled && collapsed && !sticky_notice
}

/// Nombre del ícono simbólico de COSMIC.
pub fn icon_name(v: &HudValue) -> &'static str {
    match v.kind {
        HudKind::Brightness => "display-brightness-symbolic",
        HudKind::Volume if v.muted || v.percent == 0 => "audio-volume-muted-symbolic",
        HudKind::Volume if v.percent < 34 => "audio-volume-low-symbolic",
        HudKind::Volume if v.percent < 67 => "audio-volume-medium-symbolic",
        HudKind::Volume => "audio-volume-high-symbolic",
    }
}

fn label(v: &HudValue) -> String {
    if v.kind == HudKind::Volume && v.muted {
        crate::fl!("hud-muted")
    } else {
        crate::fl!("general-percent", n = u32::from(v.percent))
    }
}

fn title(v: &HudValue) -> String {
    match v.kind {
        HudKind::Volume => crate::fl!("hud-volume"),
        HudKind::Brightness => crate::fl!("hud-brightness"),
    }
}

fn bar_color(v: &HudValue) -> Color {
    match v.kind {
        HudKind::Brightness => AMBER,
        HudKind::Volume if v.muted => GREY,
        HudKind::Volume => Color::WHITE,
    }
}

fn bar<'a, M: 'a>(v: &HudValue, width: f32) -> Element<'a, M> {
    let frac = f32::from(v.percent.min(100)) / 100.0;
    let color = bar_color(v);
    let fill = widget::container(widget::space())
        .width(Length::Fixed(width * frac))
        .height(Length::Fixed(4.0))
        .class(cosmic::theme::Container::custom(move |_| {
            widget::container::Style {
                background: Some(Background::Color(color)),
                border: cosmic::iced::Border {
                    radius: 2.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }));
    widget::container(fill)
        .width(Length::Fixed(width))
        .height(Length::Fixed(4.0))
        .class(cosmic::theme::Container::custom(|_| {
            widget::container::Style {
                background: Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.2))),
                border: cosmic::iced::Border {
                    radius: 2.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

fn text<'a>(s: String, size: u16, c: Color) -> widget::Text<'a, cosmic::Theme> {
    widget::text(s)
        .size(size)
        .wrapping(cosmic::iced::widget::text::Wrapping::None)
        .class(cosmic::theme::Text::Color(c))
}

fn icon<'a, M: 'a>(v: &HudValue, size: u16, c: Color) -> Element<'a, M> {
    widget::icon::from_name(icon_name(v))
        .size(size)
        .symbolic(true)
        .icon()
        .class(cosmic::theme::Svg::custom(move |_| {
            cosmic::iced::widget::svg::Style { color: Some(c) }
        }))
        .into()
}

pub fn view<'a, M: 'a>(v: &HudValue, style: HudStyle) -> Element<'a, M> {
    match style {
        HudStyle::Wide => widget::Row::new()
            .spacing(10)
            .align_y(Alignment::Center)
            .padding([0, 14])
            .push(icon(v, 18, Color::WHITE))
            .push(bar(v, 200.0))
            .push(text(label(v), 13, Color::WHITE))
            .into(),
        HudStyle::Tall => widget::Column::new()
            .spacing(6)
            .align_x(Alignment::Center)
            .push(
                widget::Row::new()
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .push(icon(v, 16, Color::WHITE))
                    .push(text(title(v), 12, GREY))
                    .push(text(label(v), 13, Color::WHITE)),
            )
            .push(bar(v, 190.0))
            .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(kind: HudKind, percent: u8, muted: bool) -> HudValue {
        HudValue {
            kind,
            percent,
            muted,
        }
    }

    #[test]
    fn icons() {
        let vol = |p, m| icon_name(&v(HudKind::Volume, p, m));
        assert_eq!(vol(80, false), "audio-volume-high-symbolic");
        assert_eq!(vol(50, false), "audio-volume-medium-symbolic");
        assert_eq!(vol(10, false), "audio-volume-low-symbolic");
        assert_eq!(vol(0, false), "audio-volume-muted-symbolic");
        assert_eq!(vol(80, true), "audio-volume-muted-symbolic");
        assert_eq!(
            icon_name(&v(HudKind::Brightness, 50, false)),
            "display-brightness-symbolic"
        );
    }

    #[test]
    fn labels() {
        crate::i18n::apply_for_test("es");
        assert_eq!(label(&v(HudKind::Volume, 62, false)), "62 %");
        assert_eq!(label(&v(HudKind::Volume, 62, true)), "mudo");
        assert_eq!(label(&v(HudKind::Brightness, 30, false)), "30 %");
    }

    #[test]
    fn labels_and_titles_in_english() {
        crate::i18n::apply_for_test("en");
        assert_eq!(label(&v(HudKind::Volume, 62, true)), "muted");
        assert_eq!(title(&v(HudKind::Volume, 1, false)), "Volume");
        assert_eq!(title(&v(HudKind::Brightness, 1, false)), "Brightness");
    }

    #[test]
    fn show_rules() {
        let cfg = MorphConfig::default();
        let vol = v(HudKind::Volume, 50, false);
        assert!(should_show(&vol, true, false, &cfg));
        assert!(!should_show(&vol, false, false, &cfg));
        assert!(!should_show(&vol, true, true, &cfg));
        let off = MorphConfig {
            hud_volume: false,
            ..cfg.clone()
        };
        assert!(!should_show(&vol, true, false, &off));
        let bri = v(HudKind::Brightness, 50, false);
        assert!(should_show(&bri, true, false, &off));
        let off = MorphConfig {
            hud_brightness: false,
            ..cfg
        };
        assert!(!should_show(&bri, true, false, &off));
    }
}
