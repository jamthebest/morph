//! Geometría pura de la superficie de la terminal.
use crate::config::TermSize;

pub const MIN_W: u32 = 480;
pub const MIN_H: u32 = 240;
/// Monitor supuesto si no se conoce su tamaño.
pub const FALLBACK_MONITOR: (u32, u32) = (1920, 1080);

/// Tamaño `(ancho, alto)` en px lógicos: `Large` 80 %×65 %, `Medium` 55 %×45 %;
/// `height_pct` (si existe) sustituye el alto. Mínimo 480×240, máximo el monitor.
pub fn surface_size(monitor: (u32, u32), size: TermSize, height_pct: Option<u8>) -> (u32, u32) {
    let (mw, mh) = monitor;
    let (wp, hp) = match size {
        TermSize::Large => (80, 65),
        TermSize::Medium => (55, 45),
    };
    let hp = height_pct.map_or(hp, u32::from);
    let w = (mw * wp / 100).max(MIN_W).min(mw);
    let h = (mh * hp / 100).max(MIN_H).min(mh);
    (w, h)
}

/// Monitor para una salida sin resolver (Active): el menor ancho y el menor alto
/// conocidos, para no desbordar ninguno. Sin datos, `FALLBACK_MONITOR`.
pub fn smallest_monitor(sizes: &[(u32, u32)]) -> (u32, u32) {
    let w = sizes.iter().map(|s| s.0).min();
    let h = sizes.iter().map(|s| s.1).min();
    match (w, h) {
        (Some(w), Some(h)) => (w, h),
        _ => FALLBACK_MONITOR,
    }
}

/// Porcentaje del monitor que representa un alto en px (para guardarlo), entre 10 y 100.
pub fn pct_from_height(h: u32, monitor_h: u32) -> u8 {
    if monitor_h == 0 {
        return 100;
    }
    ((u64::from(h) * 100 + u64::from(monitor_h) / 2) / u64::from(monitor_h)).clamp(10, 100) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tamanos_por_defecto() {
        assert_eq!(
            surface_size((1920, 1080), TermSize::Large, None),
            (1536, 702)
        );
        assert_eq!(
            surface_size((1920, 1080), TermSize::Medium, None),
            (1056, 486)
        );
    }

    #[test]
    fn alto_por_porcentaje() {
        assert_eq!(
            surface_size((1920, 1080), TermSize::Large, Some(50)),
            (1536, 540)
        );
    }

    #[test]
    fn minimos_y_maximos() {
        assert_eq!(surface_size((800, 400), TermSize::Medium, None), (480, 240));
        // Nunca más grande que el monitor.
        assert_eq!(surface_size((400, 200), TermSize::Large, None), (400, 200));
        assert_eq!(
            surface_size((1920, 1080), TermSize::Large, Some(250)),
            (1536, 1080)
        );
    }

    #[test]
    fn porcentaje_desde_alto() {
        assert_eq!(pct_from_height(540, 1080), 50);
        assert_eq!(pct_from_height(10, 1080), 10);
        assert_eq!(pct_from_height(5000, 1080), 100);
        assert_eq!(pct_from_height(100, 0), 100);
        // Redondea, no trunca.
        assert_eq!(pct_from_height(547, 1080), 51);
        assert_eq!(pct_from_height(545, 1080), 50);
    }

    #[test]
    fn monitor_sin_resolver() {
        assert_eq!(smallest_monitor(&[]), FALLBACK_MONITOR);
        assert_eq!(
            smallest_monitor(&[(2560, 1440), (1920, 1200), (3840, 1100)]),
            (1920, 1100)
        );
    }
}
