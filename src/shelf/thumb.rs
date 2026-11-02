//! Miniaturas del Shelf: se buscan en la caché XDG (solo lectura) o se decodifican
//! y reducen fuera del hilo de la interfaz. Nunca se escribe en `~/.cache/thumbnails`.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Lado máximo de la miniatura en memoria (el doble de los 34 px mostrados).
pub const MAX_SIDE: u32 = 68;

/// Píxeles RGBA ya reducidos.
pub type Rgba = (u32, u32, Vec<u8>);

pub fn md5_hex(data: &[u8]) -> String {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let k: Vec<u32> = (0..64)
        .map(|i| ((i as f64 + 1.0).sin().abs() * 4294967296.0) as u32)
        .collect();
    let (mut a0, mut b0, mut c0, mut d0) =
        (0x67452301u32, 0xefcdab89u32, 0x98badcfeu32, 0x10325476u32);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64).wrapping_mul(8)).to_le_bytes());
    for chunk in msg.chunks(64) {
        let m: Vec<u32> = chunk
            .chunks(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f2 = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f2.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    [a0, b0, c0, d0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Ruta de la miniatura en la caché freedesktop: `<base>/thumbnails/<normal|large>/<md5 del URI>.png`.
pub fn cache_path(cache_base: &Path, uri: &str, size_dir: &str) -> PathBuf {
    cache_base
        .join("thumbnails")
        .join(size_dir)
        .join(format!("{}.png", md5_hex(uri.as_bytes())))
}

/// Dimensiones que caben en `max` conservando la proporción (nunca amplía).
pub fn fit(w: u32, h: u32, max: u32) -> (u32, u32) {
    if w == 0 || h == 0 || (w <= max && h <= max) {
        return (w.max(1), h.max(1));
    }
    if w >= h {
        (max, ((h as u64 * max as u64 / w as u64) as u32).max(1))
    } else {
        (((w as u64 * max as u64 / h as u64) as u32).max(1), max)
    }
}

fn cache_base() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
}

fn reduce(img: image::DynamicImage) -> Rgba {
    let (w, h) = fit(img.width(), img.height(), MAX_SIDE);
    let rgba = img.to_rgba8();
    let out = if (w, h) == rgba.dimensions() {
        rgba
    } else {
        image::imageops::thumbnail(&rgba, w, h)
    };
    (out.width(), out.height(), out.into_raw())
}

/// Miniatura de la caché XDG si existe y no es más vieja que el archivo.
fn from_cache(path: &Path, mtime: Option<SystemTime>) -> Option<Rgba> {
    let base = cache_base()?;
    let uri = url::Url::from_file_path(path).ok()?;
    for dir in ["normal", "large"] {
        let c = cache_path(&base, uri.as_str(), dir);
        let Ok(meta) = std::fs::metadata(&c) else {
            continue;
        };
        if let (Ok(cm), Some(fm)) = (meta.modified(), mtime)
            && cm < fm
        {
            continue;
        }
        if let Ok(img) = image::open(&c) {
            return Some(reduce(img));
        }
    }
    None
}

/// Bloqueante: llamar desde `spawn_blocking`.
pub fn load(path: &Path, mtime: Option<SystemTime>) -> Option<Rgba> {
    from_cache(path, mtime).or_else(|| image::open(path).ok().map(reduce))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_vectores() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            md5_hex(
                b"12345678901234567890123456789012345678901234567890123456789012345678901234567890"
            ),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
    }

    #[test]
    fn ruta_de_cache_segun_la_especificacion() {
        // Ejemplo de la especificación (hash verificado con md5sum).
        assert_eq!(
            cache_path(Path::new("/c"), "file:///home/jens/photo/me.png", "normal"),
            PathBuf::from("/c/thumbnails/normal/d40775e596682f2a16d1b834c221c0a2.png")
        );
    }

    #[test]
    fn ajuste_de_tamano() {
        assert_eq!(fit(1000, 500, 68), (68, 34));
        assert_eq!(fit(500, 1000, 68), (34, 68));
        assert_eq!(fit(20, 10, 68), (20, 10));
        assert_eq!(fit(10000, 1, 68), (68, 1));
    }

    #[test]
    fn decodifica_y_reduce() {
        let dir = std::env::temp_dir().join(format!("morph-thumb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.png");
        image::RgbaImage::from_pixel(200, 100, image::Rgba([255, 0, 0, 255]))
            .save(&f)
            .unwrap();
        let (w, h, px) = load(&f, None).unwrap();
        assert_eq!((w, h), (68, 34));
        assert_eq!(px.len(), (w * h * 4) as usize);
        assert!(load(&dir.join("nada.png"), None).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
