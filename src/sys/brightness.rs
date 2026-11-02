//! Brillo de pantalla: señal `DisplayBrightness` del daemon de ajustes de COSMIC.

use cosmic::iced::Subscription;
use cosmic::iced::futures::channel::mpsc;
use cosmic::iced::futures::{SinkExt, StreamExt};
use cosmic::iced::stream;

use super::rules::{HudKind, HudValue, backoff, brightness_percent};

#[zbus::proxy(
    interface = "com.system76.CosmicSettingsDaemon",
    default_service = "com.system76.CosmicSettingsDaemon",
    default_path = "/com/system76/CosmicSettingsDaemon"
)]
trait SettingsDaemon {
    #[zbus(property)]
    fn display_brightness(&self) -> zbus::Result<i32>;
    #[zbus(property)]
    fn max_display_brightness(&self) -> zbus::Result<i32>;
}

fn debug() -> bool {
    static D: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *D.get_or_init(|| crate::envcompat::var_os("MORPH_DEBUG").is_some())
}

fn value(percent: u8) -> HudValue {
    HudValue {
        kind: HudKind::Brightness,
        percent,
        muted: false,
    }
}

/// Lee el brillo y emite cada cambio. Termina (Err) si el servicio falla o desaparece.
/// `connected` pasa a true tras la primera lectura correcta.
async fn run(out: &mut mpsc::Sender<HudValue>, connected: &mut bool) -> zbus::Result<()> {
    let conn = zbus::Connection::session().await?;
    let proxy = SettingsDaemonProxy::new(&conn).await?;
    let mut changes = proxy.receive_display_brightness_changed().await;
    let max = proxy.max_display_brightness().await?;
    let cur = proxy.display_brightness().await?;
    let Some(p) = brightness_percent(cur, max) else {
        return Err(zbus::Error::Failure("sin pantalla con brillo".into()));
    };
    *connected = true;
    if debug() {
        eprintln!("morph: HUD brillo: lectura inicial {p} % ({cur}/{max})");
    }
    let _ = out.send(value(p)).await;
    while let Some(change) = changes.next().await {
        let Ok(cur) = change.get().await else {
            continue;
        };
        if let Some(p) = brightness_percent(cur, max) {
            let _ = out.send(value(p)).await;
        }
    }
    Err(zbus::Error::Failure("el daemon cerró la señal".into()))
}

pub fn subscription() -> Subscription<HudValue> {
    Subscription::run(|| {
        stream::channel(8, |mut out: mpsc::Sender<HudValue>| async move {
            let mut attempt = 0u32;
            loop {
                let mut connected = false;
                let err = run(&mut out, &mut connected).await.err();
                if connected {
                    attempt = 0;
                }
                if debug() {
                    eprintln!("morph: HUD brillo: vigilante terminó ({err:?}); reintento");
                }
                tokio::time::sleep(backoff(attempt)).await;
                attempt = attempt.saturating_add(1);
            }
        })
    })
}
