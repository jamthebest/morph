//! Batería: `DisplayDevice` de UPower (señales de propiedades, sin sondeo).

use cosmic::iced::Subscription;
use cosmic::iced::futures::channel::mpsc;
use cosmic::iced::futures::stream::select_all;
use cosmic::iced::futures::{SinkExt, Stream, StreamExt};
use cosmic::iced::stream;
use std::pin::Pin;

use super::rules::{BatteryState, backoff, has_battery};

#[zbus::proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower/devices/DisplayDevice"
)]
trait Device {
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn time_to_empty(&self) -> zbus::Result<i64>;
    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;
    #[zbus(property, name = "Type")]
    fn kind(&self) -> zbus::Result<u32>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct BatteryReading {
    pub percent: f64,
    pub state: BatteryState,
    pub minutes_left: Option<u32>,
}

fn debug() -> bool {
    static D: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *D.get_or_init(|| crate::envcompat::var_os("MORPH_DEBUG").is_some())
}

async fn read(p: &DeviceProxy<'_>) -> zbus::Result<BatteryReading> {
    let state = BatteryState::from_upower(p.state().await?);
    let secs = p.time_to_empty().await.unwrap_or(0);
    let minutes_left = (state == BatteryState::Discharging && secs > 0)
        .then(|| u32::try_from(secs / 60).unwrap_or(u32::MAX));
    Ok(BatteryReading {
        percent: p.percentage().await?,
        state,
        minutes_left,
    })
}

enum Outcome {
    NoBattery,
    Lost(zbus::Error),
}

/// Emite la lectura inicial y cada cambio. `connected` pasa a true al hablar con UPower.
async fn run(out: &mut mpsc::Sender<BatteryReading>, connected: &mut bool) -> Outcome {
    let inner = async {
        let conn = zbus::Connection::system().await?;
        let p = DeviceProxy::new(&conn).await?;
        let present = p.is_present().await?;
        let kind = p.kind().await?;
        *connected = true;
        if !has_battery(present, kind) {
            return Ok(false);
        }
        let streams: Vec<Pin<Box<dyn Stream<Item = ()> + Send>>> = vec![
            Box::pin(p.receive_percentage_changed().await.map(|_| ())),
            Box::pin(p.receive_state_changed().await.map(|_| ())),
        ];
        let mut changes = select_all(streams);
        let first = read(&p).await?;
        if debug() {
            eprintln!(
                "morph: batería: lectura inicial {:.0} % ({:?})",
                first.percent, first.state
            );
        }
        let mut last = first.clone();
        let _ = out.send(first).await;
        while changes.next().await.is_some() {
            let Ok(r) = read(&p).await else { continue };
            // Percentage y State cambian a la vez: no repite lecturas iguales.
            if r != last {
                last = r.clone();
                let _ = out.send(r).await;
            }
        }
        Err::<bool, _>(zbus::Error::Failure("UPower cerró las señales".into()))
    };
    match inner.await {
        Ok(_) => Outcome::NoBattery,
        Err(e) => Outcome::Lost(e),
    }
}

pub fn subscription() -> Subscription<BatteryReading> {
    Subscription::run(|| {
        stream::channel(8, |mut out: mpsc::Sender<BatteryReading>| async move {
            let mut attempt = 0u32;
            loop {
                let mut connected = false;
                match run(&mut out, &mut connected).await {
                    Outcome::NoBattery => {
                        if debug() {
                            eprintln!("morph: batería: sin batería; vigilante detenido");
                        }
                        std::future::pending::<()>().await;
                    }
                    Outcome::Lost(e) => {
                        if connected {
                            attempt = 0;
                        }
                        if debug() {
                            eprintln!("morph: batería: vigilante terminó ({e}); reintento");
                        }
                    }
                }
                tokio::time::sleep(backoff(attempt)).await;
                attempt = attempt.saturating_add(1);
            }
        })
    })
}
