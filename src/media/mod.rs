//! Modelo de reproducción (puro) y selección del reproductor activo.

pub mod art;
pub mod mpris;

use std::path::PathBuf;
use std::time::Instant;

use crate::lyrics::TrackKey;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Playing,
    Paused,
    Stopped,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArtSource {
    File(PathBuf),
    Remote(url::Url),
}

#[derive(Clone, Debug, PartialEq)]
pub struct NowPlaying {
    /// Nombre del bus.
    pub player: String,
    /// "" si falta.
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub length_ms: Option<u64>,
    pub art: Option<ArtSource>,
    pub status: Status,
    /// Leída del bus.
    pub position_ms: u64,
    /// Cuándo se leyó `position_ms`.
    pub position_at: Instant,
    /// 1.0 por defecto.
    pub rate: f64,
}

impl NowPlaying {
    pub fn display_title(&self) -> String {
        if self.title.trim().is_empty() {
            crate::fl!("music-untitled")
        } else {
            self.title.clone()
        }
    }

    pub fn track_key(&self) -> TrackKey {
        TrackKey {
            title: self.title.clone(),
            artist: self.artists.join(", "),
            album: self.album.clone(),
            duration_s: self.length_ms.map(|ms| ((ms + 500) / 1000) as u32),
        }
    }
}

/// Posición estimada en `now`: si suena, avanza según `rate`; tope `length_ms`.
pub fn position_at(np: &NowPlaying, now: Instant) -> u64 {
    if np.status != Status::Playing {
        return np.position_ms;
    }
    let elapsed = now.saturating_duration_since(np.position_at).as_secs_f64();
    let pos = np.position_ms + (elapsed * np.rate * 1000.0).max(0.0) as u64;
    match np.length_ms {
        Some(len) => pos.min(len),
        None => pos,
    }
}

pub struct PlayerSnapshot {
    pub name: String,
    pub status: Status,
    pub has_metadata: bool,
}

/// Playing 100, Paused 10, otro 0, +1 con metadatos; empate → primero.
pub fn pick_active(players: &[PlayerSnapshot]) -> Option<usize> {
    let score = |p: &PlayerSnapshot| {
        let base = match p.status {
            Status::Playing => 100,
            Status::Paused => 10,
            Status::Stopped => 0,
        };
        base + p.has_metadata as u32
    };
    let mut best: Option<(usize, u32)> = None;
    for (i, p) in players.iter().enumerate() {
        let s = score(p);
        if best.is_none_or(|(_, b)| s > b) {
            best = Some((i, s));
        }
    }
    best.map(|(i, _)| i)
}

/// `file://` → ruta decodificada; `http(s)` → remota; otro → `None`.
pub fn art_source(url: &str) -> Option<ArtSource> {
    let u = url::Url::parse(url).ok()?;
    match u.scheme() {
        "file" => u.to_file_path().ok().map(ArtSource::File),
        "http" | "https" => Some(ArtSource::Remote(u)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn np() -> NowPlaying {
        NowPlaying {
            player: "org.mpris.MediaPlayer2.x".into(),
            title: "T".into(),
            artists: vec!["A".into(), "B".into()],
            album: "Al".into(),
            length_ms: Some(204_400),
            art: None,
            status: Status::Playing,
            position_ms: 10_000,
            position_at: Instant::now(),
            rate: 1.0,
        }
    }

    fn snap(name: &str, status: Status, meta: bool) -> PlayerSnapshot {
        PlayerSnapshot {
            name: name.into(),
            status,
            has_metadata: meta,
        }
    }

    #[test]
    fn pick_prefers_playing() {
        let p = [
            snap("Brave", Status::Paused, true),
            snap("Spotify", Status::Playing, true),
        ];
        assert_eq!(pick_active(&p), Some(1));
    }

    #[test]
    fn pick_metadata_breaks_stopped_tie() {
        let p = [
            snap("A", Status::Stopped, false),
            snap("B", Status::Stopped, true),
        ];
        assert_eq!(pick_active(&p), Some(1));
    }

    #[test]
    fn pick_empty_is_none() {
        assert_eq!(pick_active(&[]), None);
    }

    #[test]
    fn pick_tie_keeps_first() {
        let p = [
            snap("A", Status::Playing, true),
            snap("B", Status::Playing, true),
        ];
        assert_eq!(pick_active(&p), Some(0));
    }

    #[test]
    fn position_advances_while_playing() {
        let n = np();
        let t = n.position_at + Duration::from_millis(2500);
        assert_eq!(position_at(&n, t), 12_500);
    }

    #[test]
    fn position_respects_rate() {
        let mut n = np();
        n.rate = 2.0;
        let t = n.position_at + Duration::from_millis(2500);
        assert_eq!(position_at(&n, t), 15_000);
    }

    #[test]
    fn position_frozen_when_paused() {
        let mut n = np();
        n.status = Status::Paused;
        let t = n.position_at + Duration::from_secs(30);
        assert_eq!(position_at(&n, t), 10_000);
    }

    #[test]
    fn position_capped_at_length() {
        let n = np();
        let t = n.position_at + Duration::from_secs(1000);
        assert_eq!(position_at(&n, t), 204_400);
    }

    #[test]
    fn display_title_fallback() {
        crate::i18n::apply_for_test("es");
        let mut n = np();
        n.title = "  ".into();
        assert_eq!(n.display_title(), "Reproduciendo");
        n.title = String::new();
        assert_eq!(n.display_title(), "Reproduciendo");
        n.title = "Hola".into();
        assert_eq!(n.display_title(), "Hola");
        crate::i18n::apply_for_test("en");
        n.title = String::new();
        assert_eq!(n.display_title(), "Playing");
    }

    #[test]
    fn track_key_joins_artists_and_rounds() {
        let k = np().track_key();
        assert_eq!(k.title, "T");
        assert_eq!(k.artist, "A, B");
        assert_eq!(k.album, "Al");
        assert_eq!(k.duration_s, Some(204));
        let mut n = np();
        n.length_ms = None;
        assert_eq!(n.track_key().duration_s, None);
    }

    #[test]
    fn art_sources() {
        assert_eq!(
            art_source("file:///tmp/a%20b.jpg"),
            Some(ArtSource::File(PathBuf::from("/tmp/a b.jpg")))
        );
        assert!(matches!(
            art_source("https://i.scdn.co/image/x"),
            Some(ArtSource::Remote(_))
        ));
        assert_eq!(art_source("data:image/png;base64,AAAA"), None);
        assert_eq!(art_source("no es url"), None);
    }
}
