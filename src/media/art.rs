//! Carga de carátulas: archivo local o descarga HTTP con caché en memoria.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use cosmic::widget::image::Handle;

use super::ArtSource;

/// Tamaño máximo aceptado para una carátula remota.
const MAX_BYTES: usize = 2 * 1024 * 1024;
/// Entradas máximas de la caché (la más antigua sale primero).
const CACHE_MAX: usize = 8;

type Cache = Mutex<VecDeque<(String, Handle)>>;

fn cache() -> &'static Cache {
    static C: OnceLock<Cache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn cached(url: &str) -> Option<Handle> {
    let c = cache().lock().ok()?;
    c.iter().find(|(u, _)| u == url).map(|(_, h)| h.clone())
}

fn remember(url: String, h: Handle) {
    if let Ok(mut c) = cache().lock() {
        c.retain(|(u, _)| *u != url);
        if c.len() >= CACHE_MAX {
            c.pop_front();
        }
        c.push_back((url, h));
    }
}

/// Añade `chunk` si no supera `max` en total; false = excede (no se añade).
fn push_capped(buf: &mut Vec<u8>, chunk: &[u8], max: usize) -> bool {
    if buf.len() + chunk.len() > max {
        return false;
    }
    buf.extend_from_slice(chunk);
    true
}

async fn download(url: &url::Url) -> Option<Vec<u8>> {
    let mut resp = crate::http::client().get(url.clone()).send().await.ok()?;
    if !resp.status().is_success() || resp.content_length().is_some_and(|n| n > MAX_BYTES as u64) {
        return None;
    }
    // Sin confiar en Content-Length: se corta al superar el tope.
    let mut buf = Vec::new();
    while let Some(chunk) = resp.chunk().await.ok()? {
        if !push_capped(&mut buf, &chunk, MAX_BYTES) {
            return None;
        }
    }
    Some(buf)
}

pub async fn load(src: ArtSource) -> Option<Handle> {
    match src {
        ArtSource::File(path) => path.is_file().then(|| Handle::from_path(path)),
        ArtSource::Remote(url) => {
            if let Some(h) = cached(url.as_str()) {
                return Some(h);
            }
            let h = Handle::from_bytes(download(&url).await?);
            remember(url.to_string(), h.clone());
            Some(h)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_capped_stops_at_limit() {
        let mut b = Vec::new();
        assert!(push_capped(&mut b, &[1, 2, 3], 5));
        assert!(push_capped(&mut b, &[4, 5], 5));
        assert!(!push_capped(&mut b, &[6], 5));
        assert_eq!(b, [1, 2, 3, 4, 5]);
    }
}
