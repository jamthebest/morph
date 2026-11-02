pub mod lrc;
pub mod lrclib;

use lrc::LyricLine;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TrackKey {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_s: Option<u32>,
}

impl TrackKey {
    /// Hay título y artista (sin contar espacios) para consultar LRCLIB.
    pub fn is_searchable(&self) -> bool {
        !self.title.trim().is_empty() && !self.artist.trim().is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Lyrics {
    Synced(Vec<LyricLine>),
    Plain(String),
    Instrumental,
    NotFound,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LyricsState {
    Idle,
    Loading(TrackKey),
    Ready(TrackKey, Lyrics),
    Failed(TrackKey),
}
