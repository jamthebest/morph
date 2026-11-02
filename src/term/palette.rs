//! Paleta de colores de la terminal (oscura, fija) y resolución de colores ANSI.
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

/// Colores base: texto, fondo y los 16 ANSI.
#[derive(Clone, Debug)]
pub struct Palette {
    pub fg: Rgb,
    pub bg: Rgb,
    pub ansi: [Rgb; 16],
}

const fn rgb(hex: u32) -> Rgb {
    Rgb {
        r: (hex >> 16) as u8,
        g: (hex >> 8) as u8,
        b: hex as u8,
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            fg: rgb(0xdddddd),
            bg: rgb(0x000000),
            ansi: [
                rgb(0x000000),
                rgb(0xcc3333),
                rgb(0x33aa33),
                rgb(0xccaa33),
                rgb(0x4477dd),
                rgb(0xbb55bb),
                rgb(0x33aaaa),
                rgb(0xcccccc),
                rgb(0x666666),
                rgb(0xff5555),
                rgb(0x55dd55),
                rgb(0xffdd55),
                rgb(0x6699ff),
                rgb(0xee77ee),
                rgb(0x55dddd),
                rgb(0xffffff),
            ],
        }
    }
}

/// Oscurece un color (para el atributo DIM).
pub fn dim(c: Rgb) -> Rgb {
    let f = |v: u8| (u16::from(v) * 2 / 3) as u8;
    Rgb {
        r: f(c.r),
        g: f(c.g),
        b: f(c.b),
    }
}

fn indexed(i: u8, palette: &Palette) -> Rgb {
    match i {
        0..=15 => palette.ansi[usize::from(i)],
        16..=231 => {
            let n = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
            Rgb {
                r: level(n / 36),
                g: level((n / 6) % 6),
                b: level(n % 6),
            }
        }
        232..=255 => {
            let v = 8 + 10 * (i - 232);
            Rgb { r: v, g: v, b: v }
        }
    }
}

/// Resuelve un color de la rejilla a RGB. `fg_default` indica qué valor por
/// defecto usar para colores sin equivalente directo (cursor, etc.).
pub fn resolve(color: Color, palette: &Palette, fg_default: bool) -> Rgb {
    match color {
        Color::Spec(c) => c,
        Color::Indexed(i) => indexed(i, palette),
        Color::Named(n) => match n {
            NamedColor::Foreground | NamedColor::BrightForeground => palette.fg,
            NamedColor::Background => palette.bg,
            NamedColor::DimForeground => dim(palette.fg),
            NamedColor::Cursor => {
                if fg_default {
                    palette.fg
                } else {
                    palette.bg
                }
            }
            other => {
                let i = other as usize;
                match i {
                    0..=15 => palette.ansi[i],
                    // Dim*: 261..=268 → ANSI 0..=7 oscurecidos.
                    _ => {
                        let base = i.saturating_sub(NamedColor::DimBlack as usize);
                        dim(palette.ansi[base.min(7)])
                    }
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> Palette {
        Palette::default()
    }

    #[test]
    fn nombrados() {
        let p = p();
        assert_eq!(resolve(Color::Named(NamedColor::Red), &p, true), p.ansi[1]);
        assert_eq!(
            resolve(Color::Named(NamedColor::BrightWhite), &p, true),
            p.ansi[15]
        );
        assert_eq!(
            resolve(Color::Named(NamedColor::Foreground), &p, true),
            p.fg
        );
        assert_eq!(
            resolve(Color::Named(NamedColor::Background), &p, false),
            p.bg
        );
        assert_eq!(
            resolve(Color::Named(NamedColor::DimRed), &p, true),
            dim(p.ansi[1])
        );
        assert_eq!(resolve(Color::Named(NamedColor::Cursor), &p, true), p.fg);
        assert_eq!(resolve(Color::Named(NamedColor::Cursor), &p, false), p.bg);
    }

    #[test]
    fn indices() {
        let p = p();
        assert_eq!(resolve(Color::Indexed(3), &p, true), p.ansi[3]);
        assert_eq!(resolve(Color::Indexed(15), &p, true), p.ansi[15]);
        // Cubo 6x6x6.
        assert_eq!(
            resolve(Color::Indexed(16), &p, true),
            Rgb { r: 0, g: 0, b: 0 }
        );
        assert_eq!(
            resolve(Color::Indexed(196), &p, true),
            Rgb { r: 255, g: 0, b: 0 }
        );
        assert_eq!(
            resolve(Color::Indexed(21), &p, true),
            Rgb { r: 0, g: 0, b: 255 }
        );
        assert_eq!(
            resolve(Color::Indexed(231), &p, true),
            Rgb {
                r: 255,
                g: 255,
                b: 255
            }
        );
        assert_eq!(
            resolve(Color::Indexed(67), &p, true),
            Rgb {
                r: 95,
                g: 135,
                b: 175
            }
        );
        // Grises.
        assert_eq!(
            resolve(Color::Indexed(232), &p, true),
            Rgb { r: 8, g: 8, b: 8 }
        );
        assert_eq!(
            resolve(Color::Indexed(255), &p, true),
            Rgb {
                r: 238,
                g: 238,
                b: 238
            }
        );
    }

    #[test]
    fn rgb_directo() {
        let c = Rgb { r: 1, g: 2, b: 3 };
        assert_eq!(resolve(Color::Spec(c), &p(), true), c);
    }
}
