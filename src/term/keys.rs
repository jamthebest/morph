//! Codificación de teclas a bytes de terminal (xterm). Pura y sin estado.
use cosmic::iced::keyboard::key::Named;
use cosmic::iced::keyboard::{Key, Modifiers};

/// Parámetro de modificadores xterm (1 + shift + 2·alt + 4·ctrl).
fn mod_param(mods: Modifiers) -> u8 {
    1 + u8::from(mods.shift()) + 2 * u8::from(mods.alt()) + 4 * u8::from(mods.control())
}

fn csi(final_byte: char, mods: Modifiers, app_cursor: bool) -> Vec<u8> {
    let m = mod_param(mods);
    if m > 1 {
        format!("\x1b[1;{m}{final_byte}").into_bytes()
    } else if app_cursor {
        format!("\x1bO{final_byte}").into_bytes()
    } else {
        format!("\x1b[{final_byte}").into_bytes()
    }
}

fn tilde(n: u8, mods: Modifiers) -> Vec<u8> {
    let m = mod_param(mods);
    if m > 1 {
        format!("\x1b[{n};{m}~").into_bytes()
    } else {
        format!("\x1b[{n}~").into_bytes()
    }
}

fn ss3_or_csi(final_byte: char, mods: Modifiers) -> Vec<u8> {
    let m = mod_param(mods);
    if m > 1 {
        format!("\x1b[1;{m}{final_byte}").into_bytes()
    } else {
        format!("\x1bO{final_byte}").into_bytes()
    }
}

fn with_alt(mods: Modifiers, mut bytes: Vec<u8>) -> Vec<u8> {
    if mods.alt() {
        bytes.insert(0, 0x1b);
    }
    bytes
}

fn named(n: Named, mods: Modifiers, app_cursor: bool) -> Option<Vec<u8>> {
    let out = match n {
        Named::Enter => with_alt(mods, vec![b'\r']),
        Named::Backspace => with_alt(mods, vec![if mods.control() { 0x08 } else { 0x7f }]),
        Named::Escape => with_alt(mods, vec![0x1b]),
        Named::Tab => {
            if mods.shift() {
                b"\x1b[Z".to_vec()
            } else {
                with_alt(mods, vec![b'\t'])
            }
        }
        Named::ArrowUp => csi('A', mods, app_cursor),
        Named::ArrowDown => csi('B', mods, app_cursor),
        Named::ArrowRight => csi('C', mods, app_cursor),
        Named::ArrowLeft => csi('D', mods, app_cursor),
        Named::Home => csi('H', mods, app_cursor),
        Named::End => csi('F', mods, app_cursor),
        Named::Insert => tilde(2, mods),
        Named::Delete => tilde(3, mods),
        Named::PageUp => tilde(5, mods),
        Named::PageDown => tilde(6, mods),
        Named::F1 => ss3_or_csi('P', mods),
        Named::F2 => ss3_or_csi('Q', mods),
        Named::F3 => ss3_or_csi('R', mods),
        Named::F4 => ss3_or_csi('S', mods),
        Named::F5 => tilde(15, mods),
        Named::F6 => tilde(17, mods),
        Named::F7 => tilde(18, mods),
        Named::F8 => tilde(19, mods),
        Named::F9 => tilde(20, mods),
        Named::F10 => tilde(21, mods),
        Named::F11 => tilde(23, mods),
        Named::F12 => tilde(24, mods),
        _ => return None,
    };
    Some(out)
}

fn ctrl_byte(c: char) -> Option<u8> {
    match c {
        'a'..='z' => Some(c as u8 - b'a' + 1),
        'A'..='Z' => Some(c as u8 - b'A' + 1),
        '@' | ' ' | '2' => Some(0),
        '[' | '3' => Some(0x1b),
        '\\' | '4' => Some(0x1c),
        ']' | '5' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '/' | '7' => Some(0x1f),
        '?' | '8' => Some(0x7f),
        _ => None,
    }
}

/// Bytes a enviar al shell para una pulsación, o `None` si la tecla no se
/// envía (atajos propios de Morph, modificadores solos, teclas sin codificación).
pub fn encode(key: &Key, mods: Modifiers, text: Option<&str>, app_cursor: bool) -> Option<Vec<u8>> {
    // Super/Logo se deja para el compositor.
    if mods.logo() {
        return None;
    }
    match key {
        Key::Named(n) => {
            // Ctrl+Tab / Ctrl+Shift+Tab: cambio de pestaña.
            if *n == Named::Tab && mods.control() {
                return None;
            }
            named(*n, mods, app_cursor)
        }
        Key::Character(s) => {
            let mut chars = s.chars();
            let c = chars.next()?;
            let single = chars.next().is_none();
            // Atajos propios: Ctrl+Shift+C/V/T/W.
            if mods.control()
                && mods.shift()
                && single
                && matches!(c.to_ascii_lowercase(), 'c' | 'v' | 't' | 'w')
            {
                return None;
            }
            if mods.control() {
                if !single {
                    return None;
                }
                return ctrl_byte(c).map(|b| with_alt(mods, vec![b]));
            }
            let body = match text {
                Some(t) if !t.is_empty() && !mods.alt() => t,
                _ => s.as_str(),
            };
            if body.is_empty() {
                return None;
            }
            Some(with_alt(mods, body.as_bytes().to_vec()))
        }
        Key::Unidentified => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(s: &str) -> Key {
        Key::Character(s.into())
    }
    fn nm(n: Named) -> Key {
        Key::Named(n)
    }
    const NONE: Modifiers = Modifiers::empty();

    fn enc(k: Key, m: Modifiers) -> Option<Vec<u8>> {
        encode(&k, m, None, false)
    }

    #[test]
    fn letras_y_texto() {
        assert_eq!(enc(ch("a"), NONE), Some(b"a".to_vec()));
        assert_eq!(enc(ch("A"), Modifiers::SHIFT), Some(b"A".to_vec()));
        assert_eq!(
            encode(&ch("ñ"), NONE, Some("ñ"), false),
            Some("ñ".as_bytes().to_vec())
        );
    }

    #[test]
    fn basicas() {
        assert_eq!(enc(nm(Named::Enter), NONE), Some(b"\r".to_vec()));
        assert_eq!(enc(nm(Named::Backspace), NONE), Some(vec![0x7f]));
        assert_eq!(enc(nm(Named::Tab), NONE), Some(b"\t".to_vec()));
        assert_eq!(
            enc(nm(Named::Tab), Modifiers::SHIFT),
            Some(b"\x1b[Z".to_vec())
        );
        assert_eq!(enc(nm(Named::Escape), NONE), Some(vec![0x1b]));
        assert_eq!(
            encode(&ch(" "), NONE, Some(" "), false),
            Some(b" ".to_vec())
        );
        assert_eq!(enc(ch(" "), Modifiers::CTRL), Some(vec![0]));
    }

    #[test]
    fn flechas_y_navegacion() {
        assert_eq!(enc(nm(Named::ArrowUp), NONE), Some(b"\x1b[A".to_vec()));
        assert_eq!(
            encode(&nm(Named::ArrowUp), NONE, None, true),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(enc(nm(Named::ArrowLeft), NONE), Some(b"\x1b[D".to_vec()));
        assert_eq!(enc(nm(Named::Home), NONE), Some(b"\x1b[H".to_vec()));
        assert_eq!(enc(nm(Named::End), NONE), Some(b"\x1b[F".to_vec()));
        assert_eq!(enc(nm(Named::PageUp), NONE), Some(b"\x1b[5~".to_vec()));
        assert_eq!(enc(nm(Named::PageDown), NONE), Some(b"\x1b[6~".to_vec()));
        assert_eq!(enc(nm(Named::Delete), NONE), Some(b"\x1b[3~".to_vec()));
        assert_eq!(
            enc(nm(Named::ArrowRight), Modifiers::CTRL),
            Some(b"\x1b[1;5C".to_vec())
        );
    }

    #[test]
    fn funcion() {
        assert_eq!(enc(nm(Named::F1), NONE), Some(b"\x1bOP".to_vec()));
        assert_eq!(enc(nm(Named::F5), NONE), Some(b"\x1b[15~".to_vec()));
        assert_eq!(enc(nm(Named::F12), NONE), Some(b"\x1b[24~".to_vec()));
    }

    #[test]
    fn control_y_alt() {
        assert_eq!(enc(ch("c"), Modifiers::CTRL), Some(vec![0x03]));
        assert_eq!(enc(ch("a"), Modifiers::CTRL), Some(vec![0x01]));
        assert_eq!(enc(ch("x"), Modifiers::ALT), Some(b"\x1bx".to_vec()));
        assert_eq!(
            encode(&ch("x"), Modifiers::ALT, Some("x"), false),
            Some(b"\x1bx".to_vec())
        );
    }

    #[test]
    fn atajos_propios_son_none() {
        let cs = Modifiers::CTRL | Modifiers::SHIFT;
        for c in ["c", "C", "v", "V", "t", "T", "w", "W"] {
            assert_eq!(enc(ch(c), cs), None, "{c}");
        }
        assert_eq!(enc(nm(Named::Tab), Modifiers::CTRL), None);
        assert_eq!(enc(nm(Named::Tab), cs), None);
    }

    #[test]
    fn retroceso_y_modificadores_extra() {
        assert_eq!(enc(nm(Named::Backspace), Modifiers::CTRL), Some(vec![0x08]));
        assert_eq!(
            enc(nm(Named::Backspace), Modifiers::ALT),
            Some(vec![0x1b, 0x7f])
        );
        assert_eq!(
            enc(nm(Named::F5), Modifiers::SHIFT),
            Some(b"\x1b[15;2~".to_vec())
        );
        assert_eq!(
            encode(&nm(Named::Home), NONE, None, true),
            Some(b"\x1bOH".to_vec())
        );
        assert_eq!(
            encode(&nm(Named::End), NONE, None, true),
            Some(b"\x1bOF".to_vec())
        );
        assert_eq!(
            enc(nm(Named::ArrowUp), Modifiers::CTRL | Modifiers::SHIFT),
            Some(b"\x1b[1;6A".to_vec())
        );
    }

    #[test]
    fn sin_codificacion() {
        assert_eq!(enc(nm(Named::Shift), NONE), None);
        assert_eq!(enc(Key::Unidentified, NONE), None);
        assert_eq!(enc(ch("a"), Modifiers::LOGO), None);
    }
}
