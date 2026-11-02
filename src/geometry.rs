use cosmic::iced::Rectangle;

use crate::config::{Edge, HudStyle};
use crate::island::{IslandState, Page};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

const COLLAPSED: (u32, u32) = (220, 36);
const MINIMAL: (u32, u32) = (120, 28);
const EXPANDED: (u32, u32) = (520, 300);
const SETTINGS: (u32, u32) = (560, 330);
const NOTICE: (u32, u32) = (420, 56);
const HUD_WIDE: (u32, u32) = (340, 40);
const HUD_TALL: (u32, u32) = (240, 56);
const LINE: (u32, u32) = (120, 3);
const LINE_ZONE: u32 = 6;

fn scaled(base: u32, pct: u16) -> u32 {
    (base as u64 * pct as u64 + 50) as u32 / 100
}

pub fn size_for(state: &IslandState, scale_percent: u16) -> Size {
    let (w, h) = match state {
        IslandState::Collapsed => COLLAPSED,
        IslandState::Expanded { .. } => EXPANDED,
    };
    Size {
        w: scaled(w, scale_percent),
        h: scaled(h, scale_percent),
    }
}

/// Tamaño de la isla con el panel de Ajustes abierto.
pub fn settings_size(scale_percent: u16) -> Size {
    Size {
        w: scaled(SETTINGS.0, scale_percent),
        h: scaled(SETTINGS.1, scale_percent),
    }
}

/// Tamaño al que se maqueta el contenido expandido: el de la caja visible
/// (Ajustes si el panel está abierto o cerrándose; si no, la isla expandida).
pub fn expanded_content_size(settings: bool, scale_percent: u16) -> Size {
    if settings {
        settings_size(scale_percent)
    } else {
        size_for(&IslandState::Expanded { page: Page::Musica }, scale_percent)
    }
}

/// Tamaño de la pastilla cerrada: con música siempre el normal; en modo mínimo y sin música, el reducido.
pub fn collapsed_size(minimal: bool, playing: bool, scale_percent: u16) -> Size {
    let (w, h) = if minimal && !playing {
        MINIMAL
    } else {
        COLLAPSED
    };
    Size {
        w: scaled(w, scale_percent),
        h: scaled(h, scale_percent),
    }
}

/// Ancho extra de la pastilla cerrada para la cápsula del contador del Shelf.
const BADGE_EXTRA: u32 = 64;

/// Pastilla cerrada con la cápsula del Shelf: más ancha, misma altura.
pub fn with_badge(size: Size, scale_percent: u16) -> Size {
    Size {
        w: size.w + scaled(BADGE_EXTRA, scale_percent),
        h: size.h,
    }
}

/// Margen interior de cada dato extra de la pastilla frente a la esquina redondeada, a 100 %.
pub const EXTRA_MARGIN: u32 = 6;

/// Pastilla cerrada con `n_items` datos extra (batería, volumen): `PEEK_ITEM` más el margen
/// interior (`EXTRA_MARGIN`), escalados, por dato.
pub fn with_extras(base: Size, n_items: u32, scale_percent: u16) -> Size {
    Size {
        w: base.w + scaled(n_items * (PEEK_ITEM + EXTRA_MARGIN), scale_percent),
        h: base.h,
    }
}

/// Datos que muestra la pastilla cerrada: (batería, volumen). Sin batería no hay batería;
/// la pastilla mínima no lleva ninguno.
pub fn pill_extras(
    battery_on: bool,
    has_battery: bool,
    volume_on: bool,
    minimal: bool,
) -> (bool, bool) {
    (battery_on && has_battery && !minimal, volume_on && !minimal)
}

/// Ancho de un dato de la vista asomada (ícono + «100 %») y margen exterior, a 100 %.
pub const PEEK_ITEM: u32 = 54;
pub const PEEK_EDGE: u32 = 12;
const PEEK_EXTRA_H: u32 = 4;

/// Datos a la izquierda (batería y RAM; sin batería solo RAM). A la derecha siempre 2.
pub fn peek_left_items(has_battery: bool) -> u32 {
    if has_battery { 2 } else { 1 }
}

/// Pastilla «asomada»: el centro actual más los datos de cada lado; un poco más alta.
pub fn peek_size(center: Size, has_battery: bool, scale_percent: u16) -> Size {
    let items = peek_left_items(has_battery) + 2;
    Size {
        w: center.w + scaled(items * PEEK_ITEM + 2 * PEEK_EDGE, scale_percent),
        h: center.h + scaled(PEEK_EXTRA_H, scale_percent),
    }
}

/// Tamaño de la pastilla ampliada que muestra un aviso.
pub fn notice_size(scale_percent: u16) -> Size {
    Size {
        w: scaled(NOTICE.0, scale_percent),
        h: scaled(NOTICE.1, scale_percent),
    }
}

/// Tamaño de la pastilla ampliada que muestra el HUD de volumen o brillo.
pub fn hud_size(style: HudStyle, scale_percent: u16) -> Size {
    let (w, h) = match style {
        HudStyle::Wide => HUD_WIDE,
        HudStyle::Tall => HUD_TALL,
    };
    Size {
        w: scaled(w, scale_percent),
        h: scaled(h, scale_percent),
    }
}

/// Línea del modo «Ocultar en una línea» (lo que se dibuja).
pub fn line_size(scale_percent: u16) -> Size {
    Size {
        w: scaled(LINE.0, scale_percent),
        h: scaled(LINE.1, scale_percent),
    }
}

/// Alto de la zona de entrada de la línea: el doble de lo dibujado para poder alcanzarla
/// en el borde.
pub fn line_zone_h(scale_percent: u16) -> u32 {
    scaled(LINE_ZONE, scale_percent)
}

/// Alto de la zona de entrada: en modo oculto con la isla cerrada nunca baja del mínimo
/// de la línea (tampoco mientras crece hacia la pastilla).
pub fn zone_height(hide_closed: bool, visible_h: u32, scale_percent: u16) -> u32 {
    if hide_closed {
        visible_h.max(line_zone_h(scale_percent))
    } else {
        visible_h
    }
}

/// Centro (x) de una caja de ancho `box_w` colocada en `pos` de un monitor de `output_w`.
pub fn box_center(output_w: u32, box_w: u32, pos: f32) -> f32 {
    box_x(output_w, box_w, pos) as f32 + box_w as f32 / 2.0
}

/// Tamaño fijo de la superficie: el máximo por eje entre la isla expandida y el
/// panel de Ajustes para la escala dada.
pub fn surface_size(scale_percent: u16) -> Size {
    let e = size_for(&IslandState::Expanded { page: Page::Musica }, scale_percent);
    let s = settings_size(scale_percent);
    Size {
        w: e.w.max(s.w),
        h: e.h.max(s.h),
    }
}

/// x de la caja dentro de la superficie (que ocupa todo el ancho del monitor) para la
/// posición guardada: el centro deseado es `pos * output_w`, sin salirse de la superficie.
pub fn box_x(output_w: u32, box_w: u32, pos: f32) -> u32 {
    // `f32::clamp` entra en pánico con min > max: el máximo se satura a 0.
    let c = pos.clamp(0.0, 1.0) * output_w as f32;
    let max = output_w.saturating_sub(box_w) as f32;
    (c - box_w as f32 / 2.0).max(0.0).min(max).round() as u32
}

/// Distancia (px lógicos) a la que el arrastre se pega al centro o a un borde.
const SNAP_PX: f32 = 24.0;

/// Imán del arrastre: al centro del monitor y a los bordes (24 px lógicos).
pub fn snap(output_w: u32, box_w: u32, pos: f32) -> f32 {
    if output_w == 0 {
        return pos;
    }
    let ow = output_w as f32;
    let half = box_w as f32 / 2.0;
    let c = pos * ow;
    if (c - ow / 2.0).abs() <= SNAP_PX {
        0.5
    } else if c - half <= SNAP_PX {
        half / ow
    } else if ow - (c + half) <= SNAP_PX {
        (ow - half) / ow
    } else {
        pos
    }
}

/// y de la caja dentro de la superficie: arriba pegada al borde de arriba; abajo, al de abajo.
pub fn box_y(edge: Edge, surface_h: u32, box_h: u32) -> u32 {
    match edge {
        Edge::Top => 0,
        Edge::Bottom => surface_h.saturating_sub(box_h),
    }
}

/// Borde de destino de la pastilla según la y cruda del puntero (marco de la superficie de
/// reposo, de alto `rest_h`). La y de pantalla es `raw_y` arriba y `(screen_h - rest_h) +
/// raw_y` abajo; se cambia de borde al pasar la mitad. Sin alto de monitor, cuando el puntero
/// se aleja más de 300 px de la superficie hacia el otro borde.
pub fn target_edge(edge: Edge, raw_y: f32, rest_h: u32, screen_h: Option<u32>) -> Edge {
    const FALLBACK: f32 = 300.0;
    let other = match edge {
        Edge::Top => Edge::Bottom,
        Edge::Bottom => Edge::Top,
    };
    let cross = match (screen_h, edge) {
        (Some(sh), Edge::Top) => raw_y * 2.0 >= sh as f32,
        (Some(sh), Edge::Bottom) => ((sh as f32 - rest_h as f32) + raw_y) * 2.0 < sh as f32,
        (None, Edge::Top) => raw_y > rest_h as f32 + FALLBACK,
        (None, Edge::Bottom) => raw_y < -FALLBACK,
    };
    if cross { other } else { edge }
}

/// Radios de las esquinas de la caja: redondeadas las que dan al interior de la pantalla.
pub fn radii(edge: Edge, r: f32) -> [f32; 4] {
    match edge {
        Edge::Top => [0.0, 0.0, r, r],
        Edge::Bottom => [r, r, 0.0, 0.0],
    }
}

/// Rectángulo de la caja visible en (`box_x`, `box_y`); es también la zona de entrada.
pub fn box_rect_at(box_x: u32, box_y: u32, visible: Size) -> Rectangle {
    Rectangle {
        x: box_x as f32,
        y: box_y as f32,
        width: visible.w as f32,
        height: visible.h as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::island::Page;

    #[test]
    fn expanded_content_size_follows_visible_box() {
        assert_eq!(expanded_content_size(false, 100), Size { w: 520, h: 300 });
        assert_eq!(expanded_content_size(true, 100), Size { w: 560, h: 330 });
        assert_eq!(expanded_content_size(false, 150), Size { w: 780, h: 450 });
    }

    #[test]
    fn settings_size_scales() {
        assert_eq!(settings_size(100), Size { w: 560, h: 330 });
        assert_eq!(settings_size(150), Size { w: 840, h: 495 });
    }

    #[test]
    fn badge_ensancha_sin_cambiar_alto() {
        let base = collapsed_size(false, false, 100);
        assert_eq!(with_badge(base, 100), Size { w: 284, h: 36 });
        let big = collapsed_size(false, false, 150);
        assert_eq!(with_badge(big, 150), Size { w: 426, h: 54 });
    }

    #[test]
    fn extras_ensanchan_la_pastilla() {
        let base = Size { w: 220, h: 36 };
        assert_eq!(with_extras(base, 2, 100), Size { w: 340, h: 36 });
        assert_eq!(with_extras(base, 1, 150), Size { w: 220 + 90, h: 36 });
        assert_eq!(with_extras(base, 0, 150), base);
    }

    #[test]
    fn extras_segun_config() {
        assert_eq!(pill_extras(true, false, true, false), (false, true));
        assert_eq!(pill_extras(true, true, true, true), (false, false));
        assert_eq!(pill_extras(false, true, false, false), (false, false));
        assert_eq!(pill_extras(true, true, true, false), (true, true));
    }

    #[test]
    fn sizes() {
        assert_eq!(
            size_for(&IslandState::Collapsed, 100),
            Size { w: 220, h: 36 }
        );
        assert_eq!(
            size_for(&IslandState::Collapsed, 150),
            Size { w: 330, h: 54 }
        );
        assert_eq!(
            size_for(&IslandState::Expanded { page: Page::Dev }, 100),
            Size { w: 520, h: 300 }
        );
    }

    #[test]
    fn collapsed_sizes() {
        assert_eq!(collapsed_size(true, false, 100), Size { w: 120, h: 28 });
        assert_eq!(collapsed_size(true, true, 100), Size { w: 220, h: 36 });
        assert_eq!(collapsed_size(false, false, 150), Size { w: 330, h: 54 });
    }

    #[test]
    fn peek_sizes() {
        let c = collapsed_size(false, false, 100);
        assert_eq!(peek_size(c, true, 100), Size { w: 460, h: 40 });
        assert_eq!(peek_size(c, false, 100), Size { w: 406, h: 40 });
        let c = collapsed_size(false, false, 150);
        assert_eq!(peek_size(c, true, 150), Size { w: 690, h: 60 });
        // Con la cápsula del Shelf el centro es más ancho y el total también.
        let b = with_badge(collapsed_size(false, false, 100), 100);
        assert_eq!(peek_size(b, true, 100).w, 284 + 240);
        assert_eq!(peek_left_items(true), 2);
        assert_eq!(peek_left_items(false), 1);
    }

    #[test]
    fn notice_sizes() {
        assert_eq!(notice_size(100), Size { w: 420, h: 56 });
        assert_eq!(notice_size(150), Size { w: 630, h: 84 });
    }

    #[test]
    fn hud_sizes() {
        assert_eq!(hud_size(HudStyle::Wide, 100), Size { w: 340, h: 40 });
        assert_eq!(hud_size(HudStyle::Tall, 100), Size { w: 240, h: 56 });
        assert_eq!(hud_size(HudStyle::Wide, 150), Size { w: 510, h: 60 });
        assert_eq!(hud_size(HudStyle::Tall, 150), Size { w: 360, h: 84 });
    }

    #[test]
    fn line_sizes() {
        assert_eq!(line_size(100), Size { w: 120, h: 3 });
        assert_eq!(line_size(150), Size { w: 180, h: 5 });
        assert_eq!(line_zone_h(100), 6);
        assert_eq!(line_zone_h(150), 9);
    }

    #[test]
    fn zone_height_keeps_line_minimum_when_hidden() {
        assert_eq!(zone_height(true, 3, 100), 6);
        assert_eq!(zone_height(true, 30, 100), 30);
        assert_eq!(zone_height(false, 3, 100), 3);
    }

    #[test]
    fn box_center_is_own_center_even_at_edge() {
        assert_eq!(box_center(1920, 220, 0.0), 110.0);
        assert_eq!(box_center(1920, 220, 0.5), 960.0);
        assert_eq!(box_center(1920, 220, 1.0), 1810.0);
    }

    #[test]
    fn surface_fits_expanded_and_settings() {
        assert_eq!(surface_size(100), Size { w: 560, h: 330 });
        assert_eq!(surface_size(150), Size { w: 840, h: 495 });
    }

    #[test]
    fn box_x_cases() {
        assert_eq!(box_x(1920, 220, 0.5), 850);
        assert_eq!(box_x(1920, 220, 0.0), 0);
        assert_eq!(box_x(1920, 520, 0.0), 0);
        assert_eq!(box_x(1920, 220, 1.0), 1700);
        assert_eq!(box_x(1920, 520, 0.98), 1400);
    }

    #[test]
    fn box_x_box_wider_than_output() {
        assert_eq!(box_x(400, 560, 0.5), 0);
        assert_eq!(box_x(0, 220, 0.5), 0);
    }

    #[test]
    fn box_rect_at_places_box() {
        assert_eq!(
            box_rect_at(170, 0, Size { w: 220, h: 36 }),
            Rectangle {
                x: 170.0,
                y: 0.0,
                width: 220.0,
                height: 36.0
            }
        );
    }

    #[test]
    fn snap_cases() {
        assert_eq!(snap(1920, 220, 0.51), 0.5);
        assert_eq!(snap(1920, 220, 0.53), 0.53);
        let left = snap(1920, 220, 0.065);
        assert!((left * 1920.0 - 110.0).abs() < 0.01);
        let right = snap(1920, 220, 0.94);
        assert!((right * 1920.0 + 110.0 - 1920.0).abs() < 0.01);
    }

    #[test]
    fn box_rect_at_places_box_at_bottom() {
        let r = box_rect_at(170, 294, Size { w: 220, h: 36 });
        assert_eq!((r.y, r.height), (294.0, 36.0));
    }

    #[test]
    fn box_y_cases() {
        assert_eq!(box_y(Edge::Top, 330, 36), 0);
        assert_eq!(box_y(Edge::Bottom, 330, 36), 294);
        assert_eq!(box_y(Edge::Bottom, 20, 36), 0);
    }

    #[test]
    fn target_edge_top() {
        let (t, b) = (Edge::Top, Edge::Bottom);
        assert_eq!(target_edge(t, 10.0, 330, Some(1080)), t);
        assert_eq!(target_edge(t, 539.0, 330, Some(1080)), t);
        assert_eq!(target_edge(t, 540.0, 330, Some(1080)), b);
        assert_eq!(target_edge(t, 5000.0, 330, Some(1080)), b);
    }

    #[test]
    fn target_edge_bottom() {
        let (t, b) = (Edge::Top, Edge::Bottom);
        // Superficie de 330 en una pantalla de 1080: su y 0 es la 750 de pantalla.
        assert_eq!(target_edge(b, 10.0, 330, Some(1080)), b);
        assert_eq!(target_edge(b, -210.0, 330, Some(1080)), b);
        assert_eq!(target_edge(b, -211.0, 330, Some(1080)), t);
        assert_eq!(target_edge(b, -900.0, 330, Some(1080)), t);
    }

    #[test]
    fn target_edge_without_screen_height() {
        let (t, b) = (Edge::Top, Edge::Bottom);
        assert_eq!(target_edge(t, 630.0, 330, None), t);
        assert_eq!(target_edge(t, 631.0, 330, None), b);
        assert_eq!(target_edge(b, -300.0, 330, None), b);
        assert_eq!(target_edge(b, -301.0, 330, None), t);
    }

    #[test]
    fn radii_follow_edge() {
        assert_eq!(radii(Edge::Top, 18.0), [0.0, 0.0, 18.0, 18.0]);
        assert_eq!(radii(Edge::Bottom, 18.0), [18.0, 18.0, 0.0, 0.0]);
    }
}
