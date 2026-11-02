//! Sonido de avisos con `pw-play` (sin bloquear; los errores solo se registran).
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SoundKind {
    Alarm,
    Reminder,
}

impl SoundKind {
    fn path(self) -> &'static str {
        match self {
            SoundKind::Alarm => "/usr/share/sounds/freedesktop/stereo/alarm-clock-elapsed.oga",
            SoundKind::Reminder => "/usr/share/sounds/freedesktop/stereo/complete.oga",
        }
    }
}

/// `MORPH_MUTE=1` fuerza silencio (pruebas).
fn muted() -> bool {
    crate::envcompat::var_os("MORPH_MUTE").is_some_and(|v| !v.is_empty() && v != "0")
}

type Shared = Arc<Mutex<Child>>;

/// Sonido en curso (solo uno a la vez).
static CURRENT: Mutex<Option<Shared>> = Mutex::new(None);

/// Registra el proceso como el actual y lanza un hilo que lo recoge al terminar
/// (sin zombis, sin bloquear la interfaz y sin retener el candado de forma prolongada).
fn track(child: Child) -> Shared {
    let shared: Shared = Arc::new(Mutex::new(child));
    if let Ok(mut cur) = CURRENT.lock() {
        *cur = Some(shared.clone());
    }
    let reaper = shared.clone();
    std::thread::spawn(move || {
        loop {
            match reaper.lock() {
                Ok(mut c) => {
                    if !matches!(c.try_wait(), Ok(None)) {
                        break;
                    }
                }
                Err(_) => break,
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    shared
}

/// Detiene el sonido en curso, si lo hay (los errores se ignoran).
pub fn stop() {
    let taken = CURRENT.lock().ok().and_then(|mut c| c.take());
    if let Some(shared) = taken
        && let Ok(mut c) = shared.lock()
    {
        let _ = c.kill();
    }
}

pub fn play(kind: SoundKind) {
    if muted() {
        eprintln!("morph: sonido silenciado ({kind:?})");
        return;
    }
    stop();
    match Command::new("pw-play")
        .arg(kind.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => {
            track(child);
        }
        Err(e) => eprintln!("morph: no se pudo reproducir el sonido: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_sin_sonido_no_hace_nada() {
        stop();
        stop();
    }

    #[test]
    fn stop_mata_el_proceso_actual() {
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        let shared = track(child);
        stop();
        let mut c = shared.lock().unwrap();
        assert!(c.wait().is_ok());
        assert!(c.try_wait().unwrap().is_some());
    }

    #[test]
    fn paths_per_kind() {
        assert!(SoundKind::Alarm.path().ends_with("alarm-clock-elapsed.oga"));
        assert!(SoundKind::Reminder.path().ends_with("complete.oga"));
    }
}
