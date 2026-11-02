//! Volumen del sistema: eventos de `pactl subscribe`, valor leído con `wpctl`.

use std::process::Stdio;
use std::time::{Duration, Instant};

use cosmic::iced::Subscription;
use cosmic::iced::futures::SinkExt;
use cosmic::iced::futures::channel::mpsc;
use cosmic::iced::stream;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::rules::{HudKind, HudValue, backoff, is_sink_event, next_attempt, volume_percent};

fn debug() -> bool {
    static D: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *D.get_or_init(|| crate::envcompat::var_os("MORPH_DEBUG").is_some())
}

async fn read() -> Option<HudValue> {
    // `wpctl` ya lleva su propio tope; este es el cinturón (kill_on_drop al soltar).
    let (v, muted) = tokio::time::timeout(Duration::from_secs(2), crate::volume::get())
        .await
        .ok()??;
    Some(HudValue {
        kind: HudKind::Volume,
        percent: volume_percent(v),
        muted,
    })
}

/// Una sesión de `pactl subscribe`. `got_line` pasa a true al llegar la primera línea.
async fn run(out: &mut mpsc::Sender<HudValue>, got_line: &mut bool) -> std::io::Result<()> {
    let mut child = Command::new("pactl")
        .arg("subscribe")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("sin stdout"))?;
    // Si la lectura inicial falla, la primera que funcione será la línea base.
    let mut have_base = false;
    if let Some(v) = read().await {
        have_base = true;
        if debug() {
            eprintln!(
                "morph: HUD volumen: lectura inicial {} %{}",
                v.percent,
                if v.muted { " (mudo)" } else { "" }
            );
        }
        let _ = out.send(v).await;
    } else if debug() {
        eprintln!("morph: HUD volumen: lectura inicial fallida; la próxima será la línea base");
    }
    let mut lines = BufReader::new(stdout).lines();
    while let Some(line) = lines.next_line().await? {
        *got_line = true;
        if !is_sink_event(&line) {
            continue;
        }
        // Agrupa ráfagas (tecla mantenida): lee una sola vez tras ~40 ms de calma.
        loop {
            match tokio::time::timeout(Duration::from_millis(40), lines.next_line()).await {
                Ok(Ok(Some(_))) => {}
                Ok(Ok(None)) => return Err(std::io::Error::other("pactl terminó")),
                Ok(Err(e)) => return Err(e),
                Err(_) => break,
            }
        }
        if let Some(v) = read().await {
            if !have_base {
                have_base = true;
                if debug() {
                    eprintln!("morph: HUD volumen: línea base tardía {} %", v.percent);
                }
            }
            let _ = out.send(v).await;
        }
    }
    Err(std::io::Error::other("pactl terminó"))
}

pub fn subscription() -> Subscription<HudValue> {
    Subscription::run(|| {
        stream::channel(8, |mut out: mpsc::Sender<HudValue>| async move {
            let mut attempt = 0u32;
            loop {
                let mut got_line = false;
                let started = Instant::now();
                let err = run(&mut out, &mut got_line).await.err();
                // `attempt` = fallos seguidos; 0 tras una sesión con señales de vida.
                attempt = next_attempt(attempt, got_line, started.elapsed());
                if debug() {
                    eprintln!("morph: HUD volumen: vigilante terminó ({err:?}); reintento");
                }
                tokio::time::sleep(backoff(attempt.saturating_sub(1))).await;
            }
        })
    })
}
