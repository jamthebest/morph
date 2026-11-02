//! Bluetooth: BlueZ por `ObjectManager` y `PropertiesChanged` (sin sondeo, solo lectura).

use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

use cosmic::iced::Subscription;
use cosmic::iced::futures::channel::mpsc;
use cosmic::iced::futures::stream::select_all;
use cosmic::iced::futures::{SinkExt, Stream, StreamExt};
use cosmic::iced::stream;
use zbus::zvariant::OwnedValue;

use super::rules::{BtDevice, backoff, bt_is_audio};

const DEVICE: &str = "org.bluez.Device1";

#[zbus::proxy(interface = "org.bluez.Device1", default_service = "org.bluez")]
trait Device1 {
    #[zbus(property)]
    fn connected(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn alias(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn icon(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn class(&self) -> zbus::Result<u32>;
}

#[zbus::proxy(interface = "org.bluez.Battery1", default_service = "org.bluez")]
trait Battery1 {
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<u8>;
}

fn debug() -> bool {
    static D: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *D.get_or_init(|| crate::envcompat::var_os("MORPH_DEBUG").is_some())
}

enum Ev {
    /// Dispositivo nuevo en el árbol de objetos.
    Added(String),
    /// Cambió `Connected` en ese dispositivo.
    Changed(String),
    /// BlueZ cambió de dueño en el bus (se reinicia el vigilante).
    Owner,
}

async fn battery(conn: &zbus::Connection, path: &str) -> Option<u8> {
    let b = Battery1Proxy::builder(conn)
        .path(path.to_owned())
        .ok()?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .ok()?;
    b.percentage().await.ok()
}

async fn read_device(conn: &zbus::Connection, path: &str) -> Option<BtDevice> {
    let d = Device1Proxy::builder(conn)
        .path(path.to_owned())
        .ok()?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .ok()?;
    let connected = d.connected().await.ok()?;
    let name = match d.alias().await {
        Ok(a) if !a.is_empty() => a,
        _ => d.name().await.unwrap_or_default(),
    };
    let icon = d.icon().await.unwrap_or_default();
    let class = d.class().await.unwrap_or(0);
    Some(BtDevice {
        path: path.to_owned(),
        name,
        audio: bt_is_audio(&icon, class),
        connected,
        battery: if connected {
            battery(conn, path).await
        } else {
            None
        },
    })
}

/// `connected` pasa a true al hablar con BlueZ.
async fn run(out: &mut mpsc::Sender<BtDevice>, connected: &mut bool) -> zbus::Result<()> {
    let conn = zbus::Connection::system().await?;
    let om = zbus::fdo::ObjectManagerProxy::builder(&conn)
        .destination("org.bluez")?
        .path("/")?
        .build()
        .await?;
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.bluez")?
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .path_namespace("/org/bluez")?
        .arg(0, "org.bluez.Device1")?
        .build();
    let props = zbus::MessageStream::for_match_rule(rule, &conn, None).await?;
    let added = om.receive_interfaces_added().await?;
    let dbus = zbus::fdo::DBusProxy::new(&conn).await?;
    let owner = dbus
        .receive_name_owner_changed_with_args(&[(0, "org.bluez")])
        .await?;
    let existing = om.get_managed_objects().await?;
    *connected = true;

    let streams: Vec<Pin<Box<dyn Stream<Item = Ev> + Send>>> = vec![
        Box::pin(added.filter_map(|s| async move {
            let args = s.args().ok()?;
            args.interfaces_and_properties()
                .contains_key(DEVICE)
                .then(|| Ev::Added(args.object_path().to_string()))
        })),
        Box::pin(props.filter_map(|m| async move {
            let m = m.ok()?;
            let path = m.header().path()?.to_string();
            let (iface, changed, _): (String, HashMap<String, OwnedValue>, Vec<String>) =
                m.body().deserialize().ok()?;
            (iface == DEVICE && changed.contains_key("Connected")).then_some(Ev::Changed(path))
        })),
        Box::pin(owner.filter_map(|s| async move {
            s.args().ok()?;
            Some(Ev::Owner)
        })),
    ];
    let mut events = select_all(streams);

    let mut paths: Vec<String> = existing
        .iter()
        .filter(|(_, ifs)| ifs.contains_key(DEVICE))
        .map(|(p, _)| p.to_string())
        .collect();
    paths.sort();
    let mut n_connected = 0;
    for p in &paths {
        if let Some(d) = read_device(&conn, p).await {
            n_connected += usize::from(d.connected);
            let _ = out.send(d).await;
        }
    }
    if debug() {
        eprintln!(
            "morph: bluetooth: {} dispositivos ({n_connected} conectados)",
            paths.len()
        );
    }
    while let Some(ev) = events.next().await {
        match ev {
            Ev::Owner => return Err(zbus::Error::Failure("BlueZ cambió de dueño".into())),
            Ev::Added(p) => {
                if let Some(d) = read_device(&conn, &p).await {
                    let _ = out.send(d).await;
                }
            }
            Ev::Changed(p) => {
                if let Some(mut d) = read_device(&conn, &p).await {
                    if d.connected && d.battery.is_none() {
                        // `Battery1` suele aparecer un instante después de conectar.
                        tokio::time::sleep(Duration::from_millis(1200)).await;
                        d.battery = battery(&conn, &p).await;
                    }
                    let _ = out.send(d).await;
                }
            }
        }
    }
    Err(zbus::Error::Failure("BlueZ cerró las señales".into()))
}

pub fn subscription() -> Subscription<BtDevice> {
    Subscription::run(|| {
        stream::channel(8, |mut out: mpsc::Sender<BtDevice>| async move {
            let mut attempt = 0u32;
            loop {
                let mut connected = false;
                let err = run(&mut out, &mut connected).await.err();
                if connected {
                    attempt = 0;
                }
                if debug() {
                    eprintln!(
                        "morph: bluetooth: bluez no disponible o terminó ({err:?}); reintento"
                    );
                }
                tokio::time::sleep(backoff(attempt)).await;
                attempt = attempt.saturating_add(1);
            }
        })
    })
}
