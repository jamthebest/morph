//! Wi-Fi y conectividad: NetworkManager por señales de propiedades (sin sondeo).

use std::pin::Pin;

use cosmic::iced::Subscription;
use cosmic::iced::futures::channel::mpsc;
use cosmic::iced::futures::stream::select_all;
use cosmic::iced::futures::{SinkExt, Stream, StreamExt};
use cosmic::iced::stream;
use zbus::zvariant::OwnedObjectPath;

use super::rules::{Connectivity, NetState, backoff};

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait NetworkManager {
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn connectivity(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn primary_connection(&self) -> zbus::Result<OwnedObjectPath>;
    #[zbus(property)]
    fn active_connections(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Connection.Active",
    default_service = "org.freedesktop.NetworkManager"
)]
trait ActiveConnection {
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;
    #[zbus(property, name = "Type")]
    fn kind(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Device.Wireless",
    default_service = "org.freedesktop.NetworkManager"
)]
trait WirelessDevice {
    #[zbus(property)]
    fn mode(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn active_access_point(&self) -> zbus::Result<OwnedObjectPath>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.AccessPoint",
    default_service = "org.freedesktop.NetworkManager"
)]
trait AccessPoint {
    #[zbus(property)]
    fn ssid(&self) -> zbus::Result<Vec<u8>>;
}

/// `Mode` 3 de `Device.Wireless` = el equipo es un punto de acceso (hotspot).
const MODE_AP: u32 = 3;

fn debug() -> bool {
    static D: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *D.get_or_init(|| crate::envcompat::var_os("MORPH_DEBUG").is_some())
}

/// Nombre de la conexión Wi-Fi activada (SSID), si hay una.
async fn wifi_ssid(
    conn: &zbus::Connection,
    nm: &NetworkManagerProxy<'_>,
) -> zbus::Result<Option<String>> {
    for path in nm.active_connections().await? {
        let Ok(builder) = ActiveConnectionProxy::builder(conn).path(path) else {
            continue;
        };
        let Ok(ac) = builder
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
        else {
            continue;
        };
        // Estado 2 = activada.
        if ac.kind().await.ok().as_deref() != Some("802-11-wireless")
            || ac.state().await.ok() != Some(2)
        {
            continue;
        }
        let mut ssid = None;
        let mut hotspot = false;
        for dev in ac.devices().await.unwrap_or_default() {
            let Ok(b) = WirelessDeviceProxy::builder(conn).path(dev) else {
                continue;
            };
            let Ok(w) = b
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
            else {
                continue;
            };
            if w.mode().await.ok() == Some(MODE_AP) {
                hotspot = true;
                break;
            }
            if let Ok(ap) = w.active_access_point().await
                && ap.as_str() != "/"
                && let Ok(b) = AccessPointProxy::builder(conn).path(ap)
                && let Ok(ap) = b
                    .cache_properties(zbus::proxy::CacheProperties::No)
                    .build()
                    .await
                && let Ok(raw) = ap.ssid().await
                && !raw.is_empty()
            {
                ssid = Some(String::from_utf8_lossy(&raw).into_owned());
            }
        }
        if hotspot {
            continue;
        }
        if let Some(ssid) = ssid {
            return Ok(Some(ssid));
        }
        if let Ok(id) = ac.id().await {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

async fn read(conn: &zbus::Connection, nm: &NetworkManagerProxy<'_>) -> zbus::Result<NetState> {
    Ok(NetState {
        wifi: wifi_ssid(conn, nm).await?,
        connectivity: Connectivity::from_nm(nm.connectivity().await?),
    })
}

/// Emite el estado inicial y cada cambio. `connected` pasa a true al hablar con NM.
async fn run(out: &mut mpsc::Sender<NetState>, connected: &mut bool) -> zbus::Result<()> {
    let conn = zbus::Connection::system().await?;
    let nm = NetworkManagerProxy::new(&conn).await?;
    let streams: Vec<Pin<Box<dyn Stream<Item = ()> + Send>>> = vec![
        Box::pin(nm.receive_state_changed().await.map(|_| ())),
        Box::pin(nm.receive_connectivity_changed().await.map(|_| ())),
        Box::pin(nm.receive_primary_connection_changed().await.map(|_| ())),
        Box::pin(nm.receive_active_connections_changed().await.map(|_| ())),
    ];
    let mut changes = select_all(streams);
    let first = read(&conn, &nm).await?;
    *connected = true;
    if debug() {
        eprintln!("morph: red: estado inicial {first:?}");
    }
    let mut last = first.clone();
    let _ = out.send(first).await;
    while changes.next().await.is_some() {
        let Ok(s) = read(&conn, &nm).await else {
            continue;
        };
        if s != last {
            last = s.clone();
            let _ = out.send(s).await;
        }
    }
    Err(zbus::Error::Failure(
        "NetworkManager cerró las señales".into(),
    ))
}

pub fn subscription() -> Subscription<NetState> {
    Subscription::run(|| {
        stream::channel(8, |mut out: mpsc::Sender<NetState>| async move {
            let mut attempt = 0u32;
            loop {
                let mut connected = false;
                let err = run(&mut out, &mut connected).await.err();
                if connected {
                    attempt = 0;
                }
                if debug() {
                    eprintln!("morph: red: vigilante terminó ({err:?}); reintento");
                }
                tokio::time::sleep(backoff(attempt)).await;
                attempt = attempt.saturating_add(1);
            }
        })
    })
}
