use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudKind {
    Volume,
    Brightness,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudValue {
    pub kind: HudKind,
    pub percent: u8,
    pub muted: bool,
}

/// Recuerda la última lectura por tipo: la primera es la línea base y no se muestra.
#[derive(Default)]
pub struct HudGate {
    volume: Option<HudValue>,
    brightness: Option<HudValue>,
}

impl HudGate {
    /// `None` en el primer valor de cada tipo o si no cambió; `Some` si cambió.
    pub fn feed(&mut self, v: HudValue) -> Option<HudValue> {
        let slot = match v.kind {
            HudKind::Volume => &mut self.volume,
            HudKind::Brightness => &mut self.brightness,
        };
        let prev = slot.replace(v);
        prev.filter(|p| *p != v).map(|_| v)
    }
}

/// Espera antes de reintentar un vigilante: 2 s, 4 s, 8 s … tope 300 s.
pub fn backoff(attempt: u32) -> Duration {
    let secs = 2u64.checked_shl(attempt).unwrap_or(u64::MAX);
    Duration::from_secs(secs.min(300))
}

/// Intento siguiente de un vigilante: se reinicia solo si la sesión dio señales de vida
/// (llegó una línea) o duró al menos 10 s; si no, la espera sigue creciendo.
pub fn next_attempt(attempt: u32, got_line: bool, lived: Duration) -> u32 {
    if got_line || lived >= Duration::from_secs(10) {
        0
    } else {
        attempt.saturating_add(1)
    }
}

/// ¿La línea de `pactl subscribe` habla de un cambio en un sink (o en el servidor)?
pub fn is_sink_event(line: &str) -> bool {
    line.strip_prefix("Event 'change' on ")
        .is_some_and(|rest| rest.starts_with("sink #") || rest.starts_with("server"))
}

/// `DisplayBrightness` / `MaxDisplayBrightness` → porcentaje redondeado (0..=100).
pub fn brightness_percent(cur: i32, max: i32) -> Option<u8> {
    if max <= 0 {
        return None;
    }
    let p = (cur as f64 * 100.0 / max as f64).round();
    Some(p.clamp(0.0, 100.0) as u8)
}

/// Volumen de `wpctl` (1.0 = 100 %) → porcentaje redondeado.
pub fn volume_percent(v: f32) -> u8 {
    (v * 100.0).round().clamp(0.0, 150.0) as u8
}

/// Estado de la batería según UPower (`State`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryState {
    Unknown,
    Charging,
    Discharging,
    Empty,
    FullyCharged,
    PendingCharge,
    PendingDischarge,
}

impl BatteryState {
    pub fn from_upower(v: u32) -> Self {
        match v {
            1 => Self::Charging,
            2 => Self::Discharging,
            3 => Self::Empty,
            4 => Self::FullyCharged,
            5 => Self::PendingCharge,
            6 => Self::PendingDischarge,
            _ => Self::Unknown,
        }
    }

    /// ¿Está conectado a la corriente?
    pub fn plugged(self) -> bool {
        matches!(
            self,
            Self::Charging | Self::FullyCharged | Self::PendingCharge
        )
    }
}

/// ¿El `DisplayDevice` de UPower es una batería real? (`Type` 2 = Battery.)
pub fn has_battery(is_present: bool, kind: u32) -> bool {
    is_present && kind == 2
}

#[derive(Clone, Debug, PartialEq)]
pub enum SysNotice {
    BatteryLow { p: u8, mins: Option<u32> },
    BatteryCritical { p: u8 },
    BatteryFull,
    WifiConnected(String),
    WifiDisconnected,
    NoInternet { ssid: String, portal: bool },
    BtConnected { name: String, battery: Option<u8> },
    BtDisconnected { name: String },
}

/// Umbrales de batería armados: cada uno avisa una vez por descarga.
#[derive(Default)]
pub struct BatteryRules {
    started: bool,
    armed_low: bool,
    armed_crit: bool,
    armed_full: bool,
}

pub const BATTERY_LOW: f64 = 20.0;
pub const BATTERY_CRITICAL: f64 = 10.0;
/// Puntos por encima del umbral para rearmarlo sin enchufar.
pub const REARM_MARGIN: f64 = 5.0;

impl BatteryRules {
    pub fn feed(
        &mut self,
        percent: f64,
        state: BatteryState,
        minutes_left: Option<u32>,
    ) -> Option<SysNotice> {
        if state == BatteryState::Unknown {
            return None;
        }
        let plugged = state.plugged();
        let full = plugged && (percent >= 100.0 || state == BatteryState::FullyCharged);
        if !self.started {
            // La primera lectura es la línea base: no avisa de lo que ya pasó.
            self.started = true;
            self.armed_low = percent > BATTERY_LOW;
            self.armed_crit = percent > BATTERY_CRITICAL;
            self.armed_full = !full;
            return None;
        }
        if plugged || percent >= BATTERY_LOW + REARM_MARGIN {
            self.armed_low = true;
        }
        if plugged || percent >= BATTERY_CRITICAL + REARM_MARGIN {
            self.armed_crit = true;
        }
        if !full && (!plugged || percent <= 100.0 - REARM_MARGIN) {
            self.armed_full = true;
        }
        let p = percent.round().clamp(0.0, 100.0) as u8;
        if full {
            return std::mem::take(&mut self.armed_full).then_some(SysNotice::BatteryFull);
        }
        if plugged {
            return None;
        }
        if percent <= BATTERY_CRITICAL && self.armed_crit {
            self.armed_crit = false;
            self.armed_low = false;
            return Some(SysNotice::BatteryCritical { p });
        }
        if percent <= BATTERY_LOW && percent > BATTERY_CRITICAL && self.armed_low {
            self.armed_low = false;
            return Some(SysNotice::BatteryLow {
                p,
                mins: minutes_left,
            });
        }
        None
    }
}

/// `Connectivity` de NetworkManager.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connectivity {
    Unknown,
    None,
    Portal,
    Limited,
    Full,
}

impl Connectivity {
    pub fn from_nm(v: u32) -> Self {
        match v {
            1 => Self::None,
            2 => Self::Portal,
            3 => Self::Limited,
            4 => Self::Full,
            _ => Self::Unknown,
        }
    }
}

/// Lo que importa de NetworkManager: SSID de la conexión Wi-Fi activa (si hay) y conectividad.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetState {
    pub wifi: Option<String>,
    pub connectivity: Connectivity,
}

/// Segundos sin reconexión para confirmar una desconexión de Wi-Fi.
pub const WIFI_GRACE: std::time::Duration = std::time::Duration::from_secs(10);

/// Segundos de mala conectividad antes de avisar «Sin internet».
pub const NO_INTERNET_GRACE: std::time::Duration = std::time::Duration::from_secs(4);
/// Una primera conexión Wi-Fi tan pronto tras arrancar cuenta como línea base.
pub const WIFI_STARTUP_GRACE: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Default)]
pub struct WifiRules {
    prev: Option<Option<String>>,
    /// Red de la que nos desconectamos y desde cuándo (aún sin confirmar).
    pending: Option<(String, std::time::Instant)>,
    /// Mala conectividad en curso: desde cuándo, red y si pide portal.
    bad: Option<(std::time::Instant, String, bool)>,
    no_internet_sent: bool,
    first_seen: Option<std::time::Instant>,
    startup_grace: bool,
}

impl WifiRules {
    pub fn feed(&mut self, ev: NetState, now: std::time::Instant) -> Vec<SysNotice> {
        // Lo vencido se confirma antes de procesar el evento nuevo.
        let mut out = self.tick(now);
        let cur = ev.wifi;
        let is_bad = matches!(
            ev.connectivity,
            Connectivity::Portal | Connectivity::Limited
        );
        let Some(old) = self.prev.replace(cur.clone()) else {
            // Línea base: lo que ya estaba al arrancar no avisa.
            self.first_seen = Some(now);
            self.startup_grace = cur.is_none();
            self.no_internet_sent = cur.is_some() && is_bad;
            return out;
        };
        match (old, &cur) {
            (Some(a), None) => self.pending = Some((a, now)),
            (None, Some(b)) => {
                let same = self.pending.take().is_some_and(|(a, _)| a == *b);
                if !same {
                    let early = self.startup_grace
                        && self
                            .first_seen
                            .is_some_and(|f| now.saturating_duration_since(f) < WIFI_STARTUP_GRACE);
                    if early {
                        self.no_internet_sent = is_bad;
                    } else {
                        out.push(SysNotice::WifiConnected(b.clone()));
                        self.no_internet_sent = false;
                    }
                }
                self.startup_grace = false;
            }
            (Some(a), Some(b)) if a != *b => {
                self.pending = None;
                out.push(SysNotice::WifiConnected(b.clone()));
                self.no_internet_sent = false;
            }
            _ => {}
        }
        match cur {
            Some(ssid) if is_bad && !self.no_internet_sent => {
                let portal = ev.connectivity == Connectivity::Portal;
                let since = match &self.bad {
                    Some((t, s, _)) if *s == ssid => *t,
                    _ => now,
                };
                self.bad = Some((since, ssid, portal));
            }
            _ => self.bad = None,
        }
        out.extend(self.tick(now));
        out
    }

    /// Confirma lo pendiente que ya venció: desconexión (10 s) o falta de internet (4 s).
    pub fn tick(&mut self, now: std::time::Instant) -> Vec<SysNotice> {
        let mut out = Vec::new();
        if let Some((_, since)) = &self.pending
            && now.saturating_duration_since(*since) >= WIFI_GRACE
        {
            self.pending = None;
            out.push(SysNotice::WifiDisconnected);
        }
        if let Some((since, _, _)) = &self.bad
            && now.saturating_duration_since(*since) >= NO_INTERNET_GRACE
            && let Some((_, ssid, portal)) = self.bad.take()
        {
            self.no_internet_sent = true;
            out.push(SysNotice::NoInternet { ssid, portal });
        }
        out
    }

    /// Instante en que vence lo pendiente (el llamador arma un temporizador de un solo uso).
    pub fn deadline(&self) -> Option<std::time::Instant> {
        let a = self.pending.as_ref().map(|(_, t)| *t + WIFI_GRACE);
        let b = self.bad.as_ref().map(|(t, _, _)| *t + NO_INTERNET_GRACE);
        a.into_iter().chain(b).min()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BtDevice {
    pub path: String,
    pub name: String,
    pub audio: bool,
    pub connected: bool,
    pub battery: Option<u8>,
}

/// ¿Es de audio? Ícono `audio-*` o clase mayor 0x04 (Audio/Video).
pub fn bt_is_audio(icon: &str, class: u32) -> bool {
    icon.starts_with("audio-") || (class >> 8) & 0x1f == 0x04
}

/// Dispositivos vistos y si estaban conectados: lo conectado al arrancar es línea base.
#[derive(Default)]
pub struct BtRules {
    known: std::collections::HashMap<String, bool>,
}

impl BtRules {
    pub fn feed(&mut self, dev: BtDevice) -> Option<SysNotice> {
        let prev = self.known.insert(dev.path, dev.connected);
        if !dev.audio {
            return None;
        }
        match (prev, dev.connected) {
            (Some(false), true) => Some(SysNotice::BtConnected {
                name: dev.name,
                battery: dev.battery,
            }),
            (Some(true), false) => Some(SysNotice::BtDisconnected { name: dev.name }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn bat(r: &mut BatteryRules, p: f64, st: BatteryState) -> Option<SysNotice> {
        r.feed(p, st, None)
    }
    const D: BatteryState = BatteryState::Discharging;
    const C: BatteryState = BatteryState::Charging;

    #[test]
    fn battery_start_is_baseline() {
        let mut r = BatteryRules::default();
        assert_eq!(bat(&mut r, 50.0, D), None);
        assert_eq!(bat(&mut r, 49.0, D), None);
    }

    #[test]
    fn battery_start_below_threshold_is_silent() {
        let mut r = BatteryRules::default();
        assert_eq!(bat(&mut r, 15.0, D), None);
        assert_eq!(bat(&mut r, 14.0, D), None);
        assert_eq!(
            bat(&mut r, 10.0, D),
            Some(SysNotice::BatteryCritical { p: 10 })
        );
    }

    #[test]
    fn battery_low_once_and_no_oscillation() {
        let mut r = BatteryRules::default();
        bat(&mut r, 50.0, D);
        assert_eq!(bat(&mut r, 21.0, D), None);
        let low = r.feed(20.0, D, Some(42));
        assert_eq!(
            low,
            Some(SysNotice::BatteryLow {
                p: 20,
                mins: Some(42)
            })
        );
        assert_eq!(bat(&mut r, 19.0, D), None);
        assert_eq!(bat(&mut r, 20.0, D), None);
        assert_eq!(bat(&mut r, 19.0, D), None);
        assert_eq!(bat(&mut r, 24.0, D), None);
        assert_eq!(bat(&mut r, 20.0, D), None);
    }

    #[test]
    fn battery_rearms_five_points_above() {
        let mut r = BatteryRules::default();
        bat(&mut r, 50.0, D);
        assert!(bat(&mut r, 20.0, D).is_some());
        assert_eq!(bat(&mut r, 25.0, D), None);
        assert!(matches!(
            bat(&mut r, 20.0, D),
            Some(SysNotice::BatteryLow { .. })
        ));
    }

    #[test]
    fn battery_critical_after_low_and_once() {
        let mut r = BatteryRules::default();
        bat(&mut r, 50.0, D);
        bat(&mut r, 20.0, D);
        assert_eq!(
            bat(&mut r, 10.0, D),
            Some(SysNotice::BatteryCritical { p: 10 })
        );
        assert_eq!(bat(&mut r, 9.0, D), None);
        assert_eq!(bat(&mut r, 10.0, D), None);
    }

    #[test]
    fn battery_jump_to_critical_skips_low() {
        let mut r = BatteryRules::default();
        bat(&mut r, 50.0, D);
        assert_eq!(
            bat(&mut r, 9.0, D),
            Some(SysNotice::BatteryCritical { p: 9 })
        );
        assert_eq!(bat(&mut r, 8.0, D), None);
    }

    #[test]
    fn battery_plugging_rearms() {
        let mut r = BatteryRules::default();
        bat(&mut r, 50.0, D);
        bat(&mut r, 20.0, D);
        bat(&mut r, 10.0, D);
        assert_eq!(bat(&mut r, 11.0, C), None);
        // Desenchufar con poca carga avisa de nuevo (umbrales rearmados).
        assert!(matches!(
            bat(&mut r, 12.0, D),
            Some(SysNotice::BatteryLow { .. })
        ));
        assert_eq!(
            bat(&mut r, 10.0, D),
            Some(SysNotice::BatteryCritical { p: 10 })
        );
    }

    #[test]
    fn battery_no_low_while_plugged() {
        let mut r = BatteryRules::default();
        bat(&mut r, 50.0, C);
        assert_eq!(bat(&mut r, 18.0, C), None);
    }

    #[test]
    fn battery_full_once() {
        let mut r = BatteryRules::default();
        bat(&mut r, 90.0, C);
        assert_eq!(bat(&mut r, 99.0, C), None);
        assert_eq!(bat(&mut r, 100.0, C), Some(SysNotice::BatteryFull));
        assert_eq!(bat(&mut r, 100.0, BatteryState::FullyCharged), None);
        assert_eq!(bat(&mut r, 99.0, C), None);
        assert_eq!(bat(&mut r, 100.0, C), None);
    }

    #[test]
    fn battery_fully_charged_state_notifies() {
        let mut r = BatteryRules::default();
        bat(&mut r, 98.0, C);
        assert_eq!(
            bat(&mut r, 99.0, BatteryState::FullyCharged),
            Some(SysNotice::BatteryFull)
        );
    }

    #[test]
    fn battery_full_rearms_after_discharge() {
        let mut r = BatteryRules::default();
        bat(&mut r, 90.0, C);
        assert!(bat(&mut r, 100.0, C).is_some());
        bat(&mut r, 80.0, D);
        bat(&mut r, 95.0, C);
        assert_eq!(bat(&mut r, 100.0, C), Some(SysNotice::BatteryFull));
    }

    #[test]
    fn battery_fully_charged_below_limit_notifies_once() {
        let mut r = BatteryRules::default();
        bat(&mut r, 50.0, D);
        let fc = BatteryState::FullyCharged;
        assert_eq!(bat(&mut r, 80.0, fc), Some(SysNotice::BatteryFull));
        assert_eq!(bat(&mut r, 79.9, fc), None);
        assert_eq!(bat(&mut r, 80.0, fc), None);
    }

    #[test]
    fn battery_full_not_at_start() {
        let mut r = BatteryRules::default();
        assert_eq!(bat(&mut r, 100.0, BatteryState::FullyCharged), None);
        assert_eq!(bat(&mut r, 100.0, C), None);
    }

    #[test]
    fn battery_unknown_state_is_ignored() {
        let mut r = BatteryRules::default();
        assert_eq!(bat(&mut r, 0.0, BatteryState::Unknown), None);
        assert_eq!(bat(&mut r, 0.0, BatteryState::Unknown), None);
    }

    #[test]
    fn has_battery_cases() {
        assert!(has_battery(true, 2));
        assert!(!has_battery(false, 2));
        assert!(!has_battery(true, 0));
        assert!(!has_battery(false, 0));
    }

    #[test]
    fn battery_state_mapping() {
        assert_eq!(BatteryState::from_upower(1), C);
        assert_eq!(BatteryState::from_upower(2), D);
        assert_eq!(BatteryState::from_upower(4), BatteryState::FullyCharged);
        assert_eq!(BatteryState::from_upower(99), BatteryState::Unknown);
        assert!(C.plugged() && BatteryState::FullyCharged.plugged());
        assert!(BatteryState::PendingCharge.plugged());
        assert!(!D.plugged() && !BatteryState::Empty.plugged());
    }

    fn net(ssid: Option<&str>, c: Connectivity) -> NetState {
        NetState {
            wifi: ssid.map(String::from),
            connectivity: c,
        }
    }
    fn secs(t: Instant, s: u64) -> Instant {
        t + std::time::Duration::from_secs(s)
    }

    #[test]
    fn wifi_start_connected_is_silent() {
        let mut r = WifiRules::default();
        let t = Instant::now();
        assert!(r.feed(net(Some("Casa"), Connectivity::Full), t).is_empty());
        assert!(r.deadline().is_none());
    }

    #[test]
    fn wifi_quick_reconnect_same_network_is_silent() {
        let mut r = WifiRules::default();
        let t = Instant::now();
        r.feed(net(Some("Casa"), Connectivity::Full), t);
        assert!(r.feed(net(None, Connectivity::None), secs(t, 1)).is_empty());
        assert!(r.deadline().is_some());
        assert!(r.tick(secs(t, 5)).is_empty());
        assert!(
            r.feed(net(Some("Casa"), Connectivity::Full), secs(t, 5))
                .is_empty()
        );
        assert!(r.deadline().is_none());
        assert!(r.tick(secs(t, 30)).is_empty());
    }

    #[test]
    fn wifi_disconnect_confirmed_by_tick() {
        let mut r = WifiRules::default();
        let t = Instant::now();
        r.feed(net(Some("Casa"), Connectivity::Full), t);
        r.feed(net(None, Connectivity::None), secs(t, 1));
        assert!(r.tick(secs(t, 10)).is_empty());
        assert_eq!(r.tick(secs(t, 12)), vec![SysNotice::WifiDisconnected]);
        assert!(r.deadline().is_none());
        assert!(r.tick(secs(t, 40)).is_empty());
    }

    #[test]
    fn wifi_new_connection_notifies() {
        let mut r = WifiRules::default();
        let t = Instant::now();
        r.feed(net(None, Connectivity::None), t);
        assert_eq!(
            r.feed(net(Some("Oficina"), Connectivity::Full), secs(t, 20)),
            vec![SysNotice::WifiConnected("Oficina".into())]
        );
    }

    #[test]
    fn wifi_other_network_after_disconnect() {
        let mut r = WifiRules::default();
        let t = Instant::now();
        r.feed(net(Some("Casa"), Connectivity::Full), t);
        r.feed(net(None, Connectivity::None), secs(t, 1));
        assert_eq!(
            r.feed(net(Some("Oficina"), Connectivity::Full), secs(t, 4)),
            vec![SysNotice::WifiConnected("Oficina".into())]
        );
        assert!(r.deadline().is_none());
    }

    #[test]
    fn wifi_reconnect_after_grace_without_tick_reports_both() {
        let mut r = WifiRules::default();
        let t = Instant::now();
        r.feed(net(Some("Casa"), Connectivity::Full), t);
        r.feed(net(None, Connectivity::None), secs(t, 1));
        assert_eq!(
            r.feed(net(Some("Casa"), Connectivity::Full), secs(t, 20)),
            vec![
                SysNotice::WifiDisconnected,
                SysNotice::WifiConnected("Casa".into())
            ]
        );
    }

    #[test]
    fn wifi_roaming_between_networks_notifies() {
        let mut r = WifiRules::default();
        let t = Instant::now();
        r.feed(net(Some("A"), Connectivity::Full), t);
        assert_eq!(
            r.feed(net(Some("B"), Connectivity::Full), secs(t, 1)),
            vec![SysNotice::WifiConnected("B".into())]
        );
    }

    #[test]
    fn wifi_no_internet_after_four_seconds_once_per_connection() {
        let mut r = WifiRules::default();
        let t = Instant::now();
        r.feed(net(None, Connectivity::None), t);
        let v = r.feed(net(Some("Cafe"), Connectivity::Unknown), secs(t, 20));
        assert_eq!(v, vec![SysNotice::WifiConnected("Cafe".into())]);
        assert!(
            r.feed(net(Some("Cafe"), Connectivity::Portal), secs(t, 21))
                .is_empty()
        );
        assert_eq!(r.deadline(), Some(secs(t, 25)));
        assert!(r.tick(secs(t, 24)).is_empty());
        assert_eq!(
            r.tick(secs(t, 25)),
            vec![SysNotice::NoInternet {
                ssid: "Cafe".into(),
                portal: true
            }]
        );
        assert_eq!(r.deadline(), None);
        assert!(
            r.feed(net(Some("Cafe"), Connectivity::Limited), secs(t, 40))
                .is_empty()
        );
        assert!(r.tick(secs(t, 60)).is_empty());
        // Otra conexión: vuelve a avisar.
        r.feed(net(Some("Otra"), Connectivity::Full), secs(t, 70));
        assert!(
            r.feed(net(Some("Otra"), Connectivity::Limited), secs(t, 71))
                .is_empty()
        );
        assert_eq!(
            r.tick(secs(t, 76)),
            vec![SysNotice::NoInternet {
                ssid: "Otra".into(),
                portal: false
            }]
        );
    }

    #[test]
    fn wifi_no_internet_cancelled_when_it_recovers() {
        let mut r = WifiRules::default();
        let t = Instant::now();
        r.feed(net(Some("Casa"), Connectivity::Full), t);
        r.feed(net(Some("Casa"), Connectivity::Limited), secs(t, 1));
        assert!(r.deadline().is_some());
        r.feed(net(Some("Casa"), Connectivity::Full), secs(t, 2));
        assert_eq!(r.deadline(), None);
        assert!(r.tick(secs(t, 10)).is_empty());
    }

    #[test]
    fn wifi_no_internet_cancelled_on_disconnect() {
        let mut r = WifiRules::default();
        let t = Instant::now();
        r.feed(net(Some("Casa"), Connectivity::Full), t);
        r.feed(net(Some("Casa"), Connectivity::Portal), secs(t, 1));
        r.feed(net(None, Connectivity::None), secs(t, 2));
        assert!(r.tick(secs(t, 6)).is_empty());
        assert_eq!(r.deadline(), Some(secs(t, 12)));
    }

    #[test]
    fn wifi_deadline_tracks_pending_disconnect() {
        let mut r = WifiRules::default();
        let t = Instant::now();
        r.feed(net(Some("Casa"), Connectivity::Full), t);
        assert_eq!(r.deadline(), None);
        r.feed(net(None, Connectivity::None), secs(t, 3));
        assert_eq!(r.deadline(), Some(secs(t, 13)));
    }

    #[test]
    fn wifi_first_connection_within_startup_grace_is_baseline() {
        let mut r = WifiRules::default();
        let t = Instant::now();
        r.feed(net(None, Connectivity::None), t);
        assert!(
            r.feed(net(Some("Casa"), Connectivity::Full), secs(t, 5))
                .is_empty()
        );
        // Ya no hay gracia: la siguiente conexión avisa.
        r.feed(net(None, Connectivity::None), secs(t, 30));
        r.tick(secs(t, 45));
        assert_eq!(
            r.feed(net(Some("Otra"), Connectivity::Full), secs(t, 46)),
            vec![SysNotice::WifiConnected("Otra".into())]
        );
    }

    #[test]
    fn wifi_connection_after_startup_grace_notifies() {
        let mut r = WifiRules::default();
        let t = Instant::now();
        r.feed(net(None, Connectivity::None), t);
        assert_eq!(
            r.feed(net(Some("Casa"), Connectivity::Full), secs(t, 15)),
            vec![SysNotice::WifiConnected("Casa".into())]
        );
    }

    #[test]
    fn wifi_start_without_internet_is_silent() {
        let mut r = WifiRules::default();
        let t = Instant::now();
        assert!(
            r.feed(net(Some("Cafe"), Connectivity::Portal), t)
                .is_empty()
        );
        assert!(
            r.feed(net(Some("Cafe"), Connectivity::Portal), secs(t, 1))
                .is_empty()
        );
    }

    #[test]
    fn connectivity_mapping() {
        assert_eq!(Connectivity::from_nm(2), Connectivity::Portal);
        assert_eq!(Connectivity::from_nm(3), Connectivity::Limited);
        assert_eq!(Connectivity::from_nm(4), Connectivity::Full);
        assert_eq!(Connectivity::from_nm(1), Connectivity::None);
        assert_eq!(Connectivity::from_nm(0), Connectivity::Unknown);
        assert_eq!(Connectivity::from_nm(9), Connectivity::Unknown);
    }

    fn dev(path: &str, audio: bool, connected: bool, battery: Option<u8>) -> BtDevice {
        BtDevice {
            path: path.into(),
            name: format!("Dev {path}"),
            audio,
            connected,
            battery,
        }
    }

    #[test]
    fn bt_connected_at_start_is_silent() {
        let mut r = BtRules::default();
        assert_eq!(r.feed(dev("a", true, true, Some(80))), None);
        assert_eq!(r.feed(dev("a", true, true, Some(79))), None);
    }

    #[test]
    fn bt_audio_connect_and_disconnect() {
        let mut r = BtRules::default();
        assert_eq!(r.feed(dev("a", true, false, None)), None);
        assert_eq!(
            r.feed(dev("a", true, true, Some(80))),
            Some(SysNotice::BtConnected {
                name: "Dev a".into(),
                battery: Some(80)
            })
        );
        assert_eq!(r.feed(dev("a", true, true, Some(80))), None);
        assert_eq!(
            r.feed(dev("a", true, false, None)),
            Some(SysNotice::BtDisconnected {
                name: "Dev a".into()
            })
        );
    }

    #[test]
    fn bt_disconnect_of_baseline_device_notifies() {
        let mut r = BtRules::default();
        r.feed(dev("a", true, true, None));
        assert_eq!(
            r.feed(dev("a", true, false, None)),
            Some(SysNotice::BtDisconnected {
                name: "Dev a".into()
            })
        );
    }

    #[test]
    fn bt_non_audio_is_silent() {
        let mut r = BtRules::default();
        r.feed(dev("m", false, false, None));
        assert_eq!(r.feed(dev("m", false, true, None)), None);
        assert_eq!(r.feed(dev("m", false, false, None)), None);
    }

    #[test]
    fn bt_audio_detection() {
        assert!(bt_is_audio("audio-headset", 0));
        assert!(bt_is_audio("audio-headphones", 0));
        assert!(bt_is_audio("", 0x240404));
        assert!(!bt_is_audio("input-mouse", 0x002580));
        assert!(!bt_is_audio("", 0));
    }

    fn vol(p: u8, muted: bool) -> HudValue {
        HudValue {
            kind: HudKind::Volume,
            percent: p,
            muted,
        }
    }
    fn bri(p: u8) -> HudValue {
        HudValue {
            kind: HudKind::Brightness,
            percent: p,
            muted: false,
        }
    }

    #[test]
    fn gate_first_value_is_baseline() {
        let mut g = HudGate::default();
        assert_eq!(g.feed(vol(40, false)), None);
        assert_eq!(g.feed(bri(70)), None);
    }

    #[test]
    fn gate_same_value_is_none() {
        let mut g = HudGate::default();
        g.feed(vol(40, false));
        assert_eq!(g.feed(vol(40, false)), None);
    }

    #[test]
    fn gate_change_is_some() {
        let mut g = HudGate::default();
        g.feed(vol(40, false));
        assert_eq!(g.feed(vol(41, false)), Some(vol(41, false)));
        assert_eq!(g.feed(vol(41, false)), None);
        g.feed(bri(70));
        assert_eq!(g.feed(bri(60)), Some(bri(60)));
    }

    #[test]
    fn gate_mute_toggle_is_some() {
        let mut g = HudGate::default();
        g.feed(vol(40, false));
        assert_eq!(g.feed(vol(40, true)), Some(vol(40, true)));
        assert_eq!(g.feed(vol(40, false)), Some(vol(40, false)));
    }

    #[test]
    fn backoff_doubles_and_caps() {
        let s: Vec<u64> = (0..10).map(|a| backoff(a).as_secs()).collect();
        assert_eq!(s, vec![2, 4, 8, 16, 32, 64, 128, 256, 300, 300]);
        assert_eq!(backoff(u32::MAX).as_secs(), 300);
    }

    #[test]
    fn attempt_resets_only_with_life_signs() {
        let short = Duration::from_millis(50);
        assert_eq!(next_attempt(3, false, short), 4);
        assert_eq!(next_attempt(3, true, short), 0);
        assert_eq!(next_attempt(3, false, Duration::from_secs(10)), 0);
        assert_eq!(next_attempt(u32::MAX, false, short), u32::MAX);
    }

    #[test]
    fn pactl_lines() {
        assert!(is_sink_event("Event 'change' on sink #56"));
        assert!(is_sink_event("Event 'change' on server #0"));
        assert!(!is_sink_event("Event 'change' on client #56"));
        assert!(!is_sink_event("Event 'new' on sink #56"));
        assert!(!is_sink_event("Event 'change' on source #3"));
        assert!(!is_sink_event(""));
    }

    #[test]
    fn brightness_conversion() {
        assert_eq!(brightness_percent(0, 100), Some(0));
        assert_eq!(brightness_percent(50, 100), Some(50));
        assert_eq!(brightness_percent(1, 3), Some(33));
        assert_eq!(brightness_percent(2, 3), Some(67));
        assert_eq!(brightness_percent(300, 255), Some(100));
        assert_eq!(brightness_percent(-4, 255), Some(0));
        assert_eq!(brightness_percent(10, 0), None);
        assert_eq!(brightness_percent(10, -1), None);
    }

    #[test]
    fn volume_conversion() {
        assert_eq!(volume_percent(0.0), 0);
        assert_eq!(volume_percent(0.455), 46);
        assert_eq!(volume_percent(1.0), 100);
        assert_eq!(volume_percent(1.5), 150);
    }
}
