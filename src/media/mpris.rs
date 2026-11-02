//! Suscripción MPRIS: elige el reproductor activo y emite `NowPlaying`.

use std::time::{Duration, Instant};

use cosmic::iced::futures::{SinkExt, Stream, StreamExt, channel::mpsc, stream::SelectAll};
use cosmic::iced::{Subscription, stream};
use mpris2_zbus::bindings::player::PlayerProxy;
use mpris2_zbus::enumerator::{Enumerator, Event};
use mpris2_zbus::player::{PlaybackStatus, Player};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use zbus::{Connection, names::OwnedBusName};

use super::{NowPlaying, PlayerSnapshot, Status, art_source, pick_active};

#[derive(Clone, Copy, Debug)]
pub enum MediaCmd {
    PlayPause,
    Next,
    Previous,
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)] // interfaz fijada por el plan; evento poco frecuente
pub enum MediaEvent {
    Ready(UnboundedSender<MediaCmd>),
    Changed(Option<NowPlaying>),
}

/// Tiempo máximo de una llamada D-Bus.
const DBUS_TIMEOUT: Duration = Duration::from_secs(2);
const DRIFT_POLL: Duration = Duration::from_secs(5);
/// Diferencia tolerada entre posición leída y estimada.
const DRIFT_TOLERANCE_MS: u64 = 500;

/// Misma pista (ignora estado y posición).
fn same_track(a: &NowPlaying, b: &NowPlaying) -> bool {
    a.player == b.player
        && a.title == b.title
        && a.artists == b.artists
        && a.album == b.album
        && a.length_ms == b.length_ms
}

/// Ajusta la posición de `n` frente a la última emitida: si la lectura falló
/// (`read_ok == false`) estima desde `last` (misma pista); si no, ignora
/// deriva <= tolerancia con jugador, estado y pista sin cambios.
fn reconcile(last: Option<&NowPlaying>, n: &mut NowPlaying, read_ok: bool) {
    let Some(l) = last else { return };
    if !same_track(l, n) {
        return;
    }
    let predicted = super::position_at(l, n.position_at);
    if !read_ok {
        n.position_ms = predicted;
    } else if l.status == n.status && n.position_ms.abs_diff(predicted) <= DRIFT_TOLERANCE_MS {
        n.position_ms = l.position_ms;
        n.position_at = l.position_at;
    }
}

fn debug() -> bool {
    static D: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *D.get_or_init(|| crate::envcompat::var_os("MORPH_DEBUG").is_some())
}

pub fn subscription() -> Subscription<MediaEvent> {
    Subscription::run(|| {
        stream::channel(16, |mut out: mpsc::Sender<MediaEvent>| async move {
            let (tx, rx) = unbounded_channel::<MediaCmd>();
            let _ = out.send(MediaEvent::Ready(tx.clone())).await;
            // `tx` se mantiene vivo mientras dure la suscripción.
            if let Err(e) = run(&mut out, rx).await {
                eprintln!("morph: MPRIS: {e}");
                let _ = out.send(MediaEvent::Changed(None)).await;
            }
            drop(tx);
            std::future::pending::<()>().await;
        })
    })
}

struct Known {
    name: OwnedBusName,
    player: Player,
}

type Ticks = SelectAll<Box<dyn Stream<Item = ()> + Unpin + Send>>;

async fn watch(players: &[Known]) -> Ticks {
    let mut all: Ticks = SelectAll::new();
    for k in players {
        let p = &k.player;
        all.push(Box::new(p.receive_metadata_changed().await.map(|_| ())));
        all.push(Box::new(
            p.receive_playback_status_changed().await.map(|_| ()),
        ));
        all.push(Box::new(p.receive_rate_changed().await.map(|_| ())));
        if let Ok(s) = p.receive_seeked().await {
            all.push(Box::new(s.map(|_| ())));
        }
    }
    all
}

async fn add(conn: &Connection, players: &mut Vec<Known>, name: OwnedBusName) {
    if players.iter().any(|k| k.name == name) {
        return;
    }
    // `Position` no emite PropertiesChanged: sin caché para leerla siempre del bus.
    let built = async {
        PlayerProxy::builder(conn)
            .destination(name.clone())?
            .uncached_properties(&["Position"])
            .build()
            .await
    }
    .await;
    match built.map(Player::from) {
        Ok(player) => players.push(Known { name, player }),
        Err(e) => eprintln!("morph: MPRIS: no se pudo añadir {name}: {e}"),
    }
}

async fn status_of(p: &Player) -> Status {
    match p.playback_status().await {
        Ok(PlaybackStatus::Playing) => Status::Playing,
        Ok(PlaybackStatus::Paused) => Status::Paused,
        _ => Status::Stopped,
    }
}

/// Devuelve también si la lectura de `Position` tuvo éxito.
async fn snapshot(players: &[Known]) -> Option<(NowPlaying, bool)> {
    let mut snaps = Vec::with_capacity(players.len());
    for k in players {
        snaps.push(PlayerSnapshot {
            name: k.name.to_string(),
            status: status_of(&k.player).await,
            has_metadata: k.player.metadata().await.is_ok(),
        });
    }
    let idx = pick_active(&snaps)?;
    let k = &players[idx];
    let md = k.player.metadata().await.ok();
    let status = snaps[idx].status;
    let (position_ms, position_at, read_ok) = match k.player.position().await {
        Ok(Some(d)) => (d.as_millis().max(0) as u64, Instant::now(), true),
        _ => (0, Instant::now(), false),
    };
    let rate = k.player.rate().await.ok().flatten().unwrap_or(1.0);
    let (title, artists, album, length_ms, art) = match &md {
        Some(m) => (
            m.title().unwrap_or_default(),
            m.artists().unwrap_or_default(),
            m.album().unwrap_or_default(),
            m.length().map(|d| d.as_millis().max(0) as u64),
            m.art_url().and_then(|u| art_source(&u)),
        ),
        None => Default::default(),
    };
    Some((
        NowPlaying {
            player: snaps[idx].name.clone(),
            title,
            artists,
            album,
            length_ms,
            art,
            status,
            position_ms,
            position_at,
            rate: if rate > 0.0 { rate } else { 1.0 },
        },
        read_ok,
    ))
}

/// Igualdad ignorando `position_at`.
fn same(a: &Option<NowPlaying>, b: &Option<NowPlaying>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let mut b = b.clone();
            b.position_at = a.position_at;
            *a == b
        }
        _ => false,
    }
}

async fn run(
    out: &mut mpsc::Sender<MediaEvent>,
    mut rx: UnboundedReceiver<MediaCmd>,
) -> zbus::Result<()> {
    let conn = zbus::connection::Builder::session()?
        .method_timeout(DBUS_TIMEOUT)
        .build()
        .await?;
    let enumerator = Enumerator::new(&conn).await?;
    let mut changes = enumerator.receive_changes().await?;
    let mut players: Vec<Known> = Vec::new();
    for name in enumerator.players().await? {
        add(&conn, &mut players, name).await;
    }
    players.sort_by(|a, b| a.name.cmp(&b.name));
    let mut ticks = watch(&players).await;
    let mut last: Option<NowPlaying> = None;
    let mut first = true;
    let mut poll = tokio::time::interval(DRIFT_POLL);
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        let mut now = None;
        if let Some((mut n, read_ok)) = snapshot(&players).await {
            reconcile(last.as_ref(), &mut n, read_ok);
            now = Some(n);
        }
        if first || !same(&now, &last) {
            first = false;
            if debug() {
                match &now {
                    Some(n) => eprintln!(
                        "morph: MPRIS: activo {} ({:?}) \"{}\" - {} @{}ms",
                        n.player,
                        n.status,
                        n.display_title(),
                        n.artists.join(", "),
                        n.position_ms
                    ),
                    None => eprintln!("morph: MPRIS: sin reproductor"),
                }
            }
            last = now.clone();
            if out.send(MediaEvent::Changed(now)).await.is_err() {
                return Ok(());
            }
        }

        let playing = last.as_ref().is_some_and(|n| n.status == Status::Playing);
        tokio::select! {
            ev = changes.next() => match ev {
                Some(Ok(Event::Add(name))) => {
                    add(&conn, &mut players, name).await;
                    players.sort_by(|a, b| a.name.cmp(&b.name));
                    ticks = watch(&players).await;
                }
                Some(Ok(Event::Remove(name))) => {
                    players.retain(|k| k.name != name);
                    ticks = watch(&players).await;
                }
                Some(Err(e)) => return Err(e),
                None => return Err(zbus::Error::Failure("el enumerador MPRIS terminó".into())),
            },
            Some(()) = ticks.next(), if !players.is_empty() => {
                if debug() {
                    eprintln!("morph: MPRIS: evento de reproductor (Seeked/metadata/estado)");
                }
            }
            _ = poll.tick(), if playing => {}
            cmd = rx.recv() => {
                if let Some(cmd) = cmd {
                    send_cmd(&players, last.as_ref(), cmd).await;
                }
            }
        }
    }
}

async fn send_cmd(players: &[Known], active: Option<&NowPlaying>, cmd: MediaCmd) {
    let Some(a) = active else { return };
    let Some(k) = players.iter().find(|k| k.name.as_str() == a.player) else {
        return;
    };
    let call = async {
        match cmd {
            MediaCmd::PlayPause => k.player.play_pause().await,
            MediaCmd::Next => k.player.next().await,
            MediaCmd::Previous => k.player.previous().await,
        }
    };
    match tokio::time::timeout(DBUS_TIMEOUT, call).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => eprintln!("morph: MPRIS: comando {cmd:?} falló: {e}"),
        Err(_) => eprintln!("morph: MPRIS: comando {cmd:?} agotó el tiempo"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn np(pos: u64) -> NowPlaying {
        NowPlaying {
            player: "p".into(),
            title: "T".into(),
            artists: vec![],
            album: String::new(),
            length_ms: Some(300_000),
            art: None,
            status: Status::Playing,
            position_ms: pos,
            position_at: Instant::now(),
            rate: 1.0,
        }
    }

    #[test]
    fn drift_within_tolerance_keeps_last() {
        let last = np(10_000);
        let mut n = np(10_200);
        n.position_at = last.position_at + Duration::from_secs(0);
        reconcile(Some(&last), &mut n, true);
        assert_eq!(n.position_ms, 10_000);
        assert_eq!(n.position_at, last.position_at);
    }

    #[test]
    fn real_seek_passes() {
        let last = np(10_000);
        let mut n = np(60_000);
        n.position_at = last.position_at;
        reconcile(Some(&last), &mut n, true);
        assert_eq!(n.position_ms, 60_000);
    }

    #[test]
    fn status_or_track_change_passes() {
        let last = np(10_000);
        let mut n = np(10_100);
        n.position_at = last.position_at;
        n.status = Status::Paused;
        reconcile(Some(&last), &mut n, true);
        assert_eq!(n.position_ms, 10_100);
        let mut n = np(10_100);
        n.position_at = last.position_at;
        n.title = "otra".into();
        reconcile(Some(&last), &mut n, true);
        assert_eq!(n.position_ms, 10_100);
    }

    #[test]
    fn read_error_estimates_from_last_same_track() {
        let last = np(10_000);
        let mut n = np(0);
        n.position_at = last.position_at + Duration::from_secs(3);
        reconcile(Some(&last), &mut n, false);
        assert_eq!(n.position_ms, 13_000);
    }

    #[test]
    fn read_error_other_track_stays_zero() {
        let last = np(10_000);
        let mut n = np(0);
        n.title = "otra".into();
        reconcile(Some(&last), &mut n, false);
        assert_eq!(n.position_ms, 0);
        let mut n = np(0);
        reconcile(None, &mut n, false);
        assert_eq!(n.position_ms, 0);
    }
}
