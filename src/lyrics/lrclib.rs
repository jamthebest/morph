use serde::Deserialize;
use url::Url;

use super::lrc::parse_lrc;
use super::{Lyrics, TrackKey};

pub const BASE: &str = "https://lrclib.net";

/// Diferencia máxima de duración (s) para aceptar un resultado de búsqueda.
/// Tope del cuerpo de respuesta (1 MiB).
const MAX_BYTES: usize = 1024 * 1024;

const MAX_DELTA_S: f64 = 3.0;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    instrumental: bool,
    #[serde(default)]
    plain_lyrics: Option<String>,
    #[serde(default)]
    synced_lyrics: Option<String>,
}

pub fn get_url(k: &TrackKey) -> Url {
    let mut u = Url::parse(BASE).expect("base válida");
    u.set_path("/api/get");
    {
        let mut q = u.query_pairs_mut();
        q.append_pair("track_name", &k.title);
        q.append_pair("artist_name", &k.artist);
        if !k.album.trim().is_empty() {
            q.append_pair("album_name", &k.album);
        }
        if let Some(d) = k.duration_s {
            q.append_pair("duration", &d.to_string());
        }
    }
    u
}

pub fn search_url(k: &TrackKey) -> Url {
    let mut u = Url::parse(BASE).expect("base válida");
    u.set_path("/api/search");
    u.query_pairs_mut()
        .append_pair("track_name", &k.title)
        .append_pair("artist_name", &k.artist);
    u
}

fn record_to_lyrics(r: &Record) -> Lyrics {
    if r.instrumental {
        return Lyrics::Instrumental;
    }
    if let Some(s) = r.synced_lyrics.as_deref().filter(|s| !s.trim().is_empty()) {
        let lines = parse_lrc(s);
        if !lines.is_empty() {
            return Lyrics::Synced(lines);
        }
    }
    match r.plain_lyrics.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(p) => Lyrics::Plain(p.to_string()),
        None => Lyrics::NotFound,
    }
}

pub fn parse_record(json: &str) -> Result<Lyrics, String> {
    let r: Record = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(record_to_lyrics(&r))
}

fn has_text(s: &Option<String>) -> bool {
    s.as_deref().is_some_and(|s| !s.trim().is_empty())
}

pub fn pick_best(results_json: &str, duration_s: Option<u32>) -> Result<Lyrics, String> {
    let all: Vec<Record> = serde_json::from_str(results_json).map_err(|e| e.to_string())?;
    // Solo registros útiles: con letra (sincronizada o plana) o instrumentales.
    let records: Vec<&Record> = all
        .iter()
        .filter(|r| r.instrumental || has_text(&r.synced_lyrics) || has_text(&r.plain_lyrics))
        .collect();
    let chosen = match duration_s {
        Some(d) => {
            let d = f64::from(d);
            records
                .iter()
                .copied()
                .filter_map(|r| {
                    let delta = (r.duration? - d).abs();
                    (delta <= MAX_DELTA_S).then_some((delta, r))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, r)| r)
        }
        None => records
            .iter()
            .copied()
            .find(|r| has_text(&r.synced_lyrics))
            .or_else(|| records.iter().copied().find(|r| has_text(&r.plain_lyrics))),
    };
    Ok(chosen.map_or(Lyrics::NotFound, record_to_lyrics))
}

enum Reply {
    Body(String),
    NotFound,
}

async fn request(url: Url) -> Result<Reply, String> {
    let resp = crate::http::client()
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(Reply::NotFound);
    }
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    if resp.content_length().is_some_and(|n| n > MAX_BYTES as u64) {
        return Err("respuesta demasiado grande".into());
    }
    // Sin confiar en Content-Length: se corta al superar el tope.
    let mut buf: Vec<u8> = Vec::new();
    let mut resp = resp;
    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        if buf.len() + chunk.len() > MAX_BYTES {
            return Err("respuesta demasiado grande".into());
        }
        buf.extend_from_slice(&chunk);
    }
    String::from_utf8(buf)
        .map(Reply::Body)
        .map_err(|e| e.to_string())
}

async fn fetch_inner(k: &TrackKey) -> Result<Lyrics, String> {
    // /api/get exige los cuatro parámetros: sin álbum o duración, directo a búsqueda.
    if !k.album.trim().is_empty() && k.duration_s.is_some() {
        match request(get_url(k)).await? {
            Reply::Body(b) => match parse_record(&b)? {
                Lyrics::NotFound => {}
                found => return Ok(found),
            },
            Reply::NotFound => {}
        }
    }
    match request(search_url(k)).await? {
        Reply::Body(b) => pick_best(&b, k.duration_s),
        Reply::NotFound => Ok(Lyrics::NotFound),
    }
}

pub async fn fetch(k: TrackKey) -> (TrackKey, Result<Lyrics, String>) {
    let r = fetch_inner(&k).await;
    (k, r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(album: &str, d: Option<u32>) -> TrackKey {
        TrackKey {
            title: "Mañana".into(),
            artist: "Café Tacvba".into(),
            album: album.into(),
            duration_s: d,
        }
    }

    #[test]
    fn get_url_codifica_y_omite() {
        let s = get_url(&key("Re", Some(204))).to_string();
        assert!(s.starts_with("https://lrclib.net/api/get?"));
        assert!(s.contains("track_name=Ma%C3%B1ana"), "{s}");
        assert!(
            s.contains("artist_name=Caf%C3%A9+Tacvba")
                || s.contains("artist_name=Caf%C3%A9%20Tacvba"),
            "{s}"
        );
        assert!(s.contains("album_name=Re"));
        assert!(s.contains("duration=204"));
        let s = get_url(&key("", None)).to_string();
        assert!(!s.contains("duration"));
        assert!(!s.contains("album_name"));
    }

    #[test]
    fn search_url_basica() {
        let s = search_url(&key("Re", Some(204))).to_string();
        assert!(s.starts_with("https://lrclib.net/api/search?"));
        assert!(s.contains("track_name=Ma%C3%B1ana"));
        assert!(!s.contains("duration"));
    }

    #[test]
    fn parse_record_variantes() {
        let synced = r#"{"instrumental":false,"plainLyrics":"a\nb","syncedLyrics":"[00:01.00] a\n[00:02.50] b"}"#;
        match parse_record(synced).unwrap() {
            Lyrics::Synced(l) => {
                assert_eq!(l.len(), 2);
                assert_eq!(l[0].time_ms, 1000);
                assert_eq!(l[1].text, "b");
            }
            o => panic!("{o:?}"),
        }
        let plain = r#"{"instrumental":false,"plainLyrics":"hola","syncedLyrics":null}"#;
        assert_eq!(parse_record(plain).unwrap(), Lyrics::Plain("hola".into()));
        let inst = r#"{"instrumental":true,"plainLyrics":null,"syncedLyrics":null}"#;
        assert_eq!(parse_record(inst).unwrap(), Lyrics::Instrumental);
        assert!(parse_record("no es json").is_err());
    }

    const RESULTS: &str = r#"[
        {"duration":180.0,"instrumental":false,"plainLyrics":"ciento ochenta","syncedLyrics":null},
        {"duration":203.0,"instrumental":false,"plainLyrics":"doscientos tres","syncedLyrics":null},
        {"duration":240.0,"instrumental":false,"plainLyrics":"doscientos cuarenta","syncedLyrics":null}
    ]"#;

    #[test]
    fn pick_best_por_duracion() {
        assert_eq!(
            pick_best(RESULTS, Some(204)).unwrap(),
            Lyrics::Plain("doscientos tres".into())
        );
        assert_eq!(pick_best(RESULTS, Some(300)).unwrap(), Lyrics::NotFound);
        assert!(pick_best("{", Some(1)).is_err());
    }

    #[test]
    fn pick_best_sin_duracion_prefiere_sincronizada() {
        let r = r#"[
            {"duration":1.0,"instrumental":false,"plainLyrics":"p","syncedLyrics":null},
            {"duration":2.0,"instrumental":false,"plainLyrics":"p2","syncedLyrics":"[00:01.00] x"}
        ]"#;
        assert!(matches!(pick_best(r, None).unwrap(), Lyrics::Synced(_)));
        assert_eq!(
            pick_best(RESULTS, None).unwrap(),
            Lyrics::Plain("ciento ochenta".into())
        );
        assert_eq!(pick_best("[]", None).unwrap(), Lyrics::NotFound);
    }

    #[test]
    fn pick_best_salta_registros_sin_letra() {
        let r = r#"[
            {"duration":204.0,"instrumental":false,"plainLyrics":null,"syncedLyrics":null},
            {"duration":206.0,"instrumental":false,"plainLyrics":"con letra","syncedLyrics":null}
        ]"#;
        assert_eq!(
            pick_best(r, Some(204)).unwrap(),
            Lyrics::Plain("con letra".into())
        );
    }

    #[test]
    fn is_searchable() {
        let mut k = key("", None);
        assert!(k.is_searchable());
        k.title = "  ".into();
        assert!(!k.is_searchable());
        k.title = "x".into();
        k.artist = "".into();
        assert!(!k.is_searchable());
    }

    #[tokio::test]
    #[ignore = "usa la red real (lrclib.net)"]
    async fn fetch_real_track() {
        let k = TrackKey {
            title: "Believer".into(),
            artist: "Imagine Dragons".into(),
            album: "Evolve".into(),
            duration_s: Some(204),
        };
        let (_, r) = fetch(k).await;
        assert!(matches!(r, Ok(Lyrics::Synced(_))), "{r:?}");
    }
}
