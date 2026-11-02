//! Fuente monoespaciada de COSMIC y métrica de celda.
use std::path::PathBuf;
use std::sync::OnceLock;

use cosmic::iced::Font;
use cosmic::iced::core::text::{self, LineHeight, Paragraph, Shaping, Wrapping};
use cosmic::iced::core::{Pixels, Size};

/// Extrae el campo `family` de un RON como `(family: "Fira Mono", weight: Normal, …)`.
pub fn parse_family(ron: &str) -> Option<String> {
    let rest = &ron[ron.find("family")? + "family".len()..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let name = &rest[..rest.find('"')?];
    (!name.is_empty()).then(|| name.to_string())
}

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("cosmic/com.system76.CosmicTk/v1/monospace_font"))
}

/// Fuente monoespaciada del escritorio (o `Font::MONOSPACE` si no se puede leer).
pub fn mono_font() -> Font {
    static FONT: OnceLock<Font> = OnceLock::new();
    *FONT.get_or_init(|| {
        let family = config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| parse_family(&s));
        match family {
            // `Font::with_name` pide `&'static str`: se filtra una sola vez.
            Some(f) => Font::with_name(Box::leak(f.into_boxed_str())),
            None => Font::MONOSPACE,
        }
    })
}

/// Alto de línea (px) para un tamaño de fuente.
pub fn line_height(font_size: f32) -> f32 {
    (font_size * 1.25).round().max(1.0)
}

/// Mide la celda `(ancho, alto)` en px: ancho de 10 «M» entre 10.
pub fn cell_metrics<R>(font_size: f32) -> (f32, f32)
where
    R: text::Renderer<Font = Font>,
{
    let h = line_height(font_size);
    let p = R::Paragraph::with_text(text::Text {
        content: "MMMMMMMMMM",
        bounds: Size::new(f32::INFINITY, h),
        size: Pixels(font_size),
        line_height: LineHeight::Absolute(Pixels(h)),
        font: mono_font(),
        align_x: text::Alignment::Left,
        align_y: cosmic::iced::alignment::Vertical::Top,
        shaping: Shaping::Basic,
        wrapping: Wrapping::None,
        ellipsize: text::Ellipsize::None,
    });
    let w = p.min_bounds().width / 10.0;
    (if w > 0.0 { w } else { font_size * 0.6 }, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_familia() {
        let ron = r#"(family: "Fira Mono", weight: Normal, stretch: Normal, style: Normal)"#;
        assert_eq!(parse_family(ron).as_deref(), Some("Fira Mono"));
        assert_eq!(
            parse_family("(\n    family: \"JetBrains Mono\",\n    weight: Bold,\n)").as_deref(),
            Some("JetBrains Mono")
        );
    }

    #[test]
    fn familia_invalida() {
        assert_eq!(parse_family(""), None);
        assert_eq!(parse_family("(weight: Normal)"), None);
        assert_eq!(parse_family(r#"(family: "")"#), None);
        assert_eq!(parse_family(r#"(family: "sin cierre"#), None);
    }

    #[test]
    fn metrica_de_celda_con_el_renderer_real() {
        let (w, h) = cell_metrics::<cosmic::Renderer>(13.0);
        assert_eq!(h, 16.0);
        assert!((4.0..=13.0).contains(&w), "w={w}");
    }

    #[test]
    fn altura_de_linea() {
        assert_eq!(line_height(13.0), 16.0);
        assert!(line_height(0.0) >= 1.0);
    }
}
