//! Avisos de la pastilla: cola de alertas (alarmas, recordatorios, perdidas e info).
use std::collections::VecDeque;

use cosmic::iced::{Alignment, Color, Length};
use cosmic::{Element, widget};

use crate::agenda::schedule::{Due, DueKind, Missed};
use crate::agenda::{Item, ItemId};
use crate::dev::runs::CiEvent;
use crate::island::Page;
use crate::sys::rules::SysNotice;

/// Segundos que dura un aviso no fijo (alarmas, tareas, CI…).
const NOTICE_SECS: u64 = 15;
/// Segundos que dura un aviso del sistema no fijo.
const SYS_NOTICE_SECS: u64 = 4;

#[derive(Clone, Debug, PartialEq)]
pub enum NoticeKind {
    Alarm(ItemId),
    TaskStart(ItemId),
    TaskEnd(ItemId),
    Missed {
        alarms: usize,
        tasks: usize,
    },
    Info,
    /// Ejecución de Actions terminada (`failed`) con la URL de la ejecución.
    Ci {
        url: String,
        failed: bool,
    },
    /// Aviso del sistema (batería, Wi-Fi, Bluetooth) con su ícono simbólico.
    Sys {
        icon: &'static str,
        critical: bool,
    },
}

impl NoticeKind {
    pub fn item_id(&self) -> Option<ItemId> {
        match self {
            NoticeKind::Alarm(id) | NoticeKind::TaskStart(id) | NoticeKind::TaskEnd(id) => {
                Some(*id)
            }
            NoticeKind::Missed { .. }
            | NoticeKind::Info
            | NoticeKind::Ci { .. }
            | NoticeKind::Sys { .. } => None,
        }
    }

    /// Alarmas, tareas y resumen de perdidas: nunca los tapa un aviso del sistema.
    fn urgent(&self) -> bool {
        matches!(
            self,
            NoticeKind::Alarm(_)
                | NoticeKind::TaskStart(_)
                | NoticeKind::TaskEnd(_)
                | NoticeKind::Missed { .. }
        )
    }

    /// ¿Es el dueño del sonido en curso? (alarmas, tareas y batería crítica).
    pub fn owns_sound(&self) -> bool {
        matches!(
            self,
            NoticeKind::Alarm(_)
                | NoticeKind::TaskStart(_)
                | NoticeKind::TaskEnd(_)
                | NoticeKind::Sys { critical: true, .. }
        )
    }

    /// Segundos que dura el aviso si no es fijo.
    pub fn secs(&self) -> u64 {
        match self {
            NoticeKind::Sys { .. } => SYS_NOTICE_SECS,
            _ => NOTICE_SECS,
        }
    }

    /// Página que abre un clic sobre el aviso.
    pub fn target_page(&self) -> Page {
        match self {
            NoticeKind::TaskStart(_) | NoticeKind::TaskEnd(_) => Page::Todo,
            NoticeKind::Ci { .. } => Page::Dev,
            _ => Page::Calendario,
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            NoticeKind::Alarm(_) | NoticeKind::Missed { .. } => "⏰",
            NoticeKind::TaskStart(_) | NoticeKind::TaskEnd(_) => "✓",
            NoticeKind::Info => "ⓘ",
            NoticeKind::Ci { failed: true, .. } => "✗",
            NoticeKind::Ci { .. } => "✓",
            NoticeKind::Sys { .. } => "",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub title: String,
    pub subtitle: String,
    pub sticky: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoticeAction {
    Dismiss,
    Snooze(u32),
    /// Abre la URL del aviso de CI (la ejecución) y lo descarta.
    View,
    /// Abre Calendario o To-do según el tipo de aviso.
    Open,
}

#[derive(Default)]
pub struct NoticeQueue {
    q: VecDeque<Notice>,
}

impl NoticeQueue {
    /// Encola un aviso. Los urgentes pasan delante de los del sistema: reemplazan al actual
    /// si es un aviso del sistema no fijo (se descarta) o lo dejan detrás (batería crítica).
    /// Devuelve `true` si cambió el aviso mostrado.
    pub fn push(&mut self, n: Notice) -> bool {
        let was_empty = self.q.is_empty();
        if !n.kind.urgent() {
            self.q.push_back(n);
            return was_empty;
        }
        match self.q.front() {
            None => {
                self.q.push_back(n);
                true
            }
            Some(f) if matches!(f.kind, NoticeKind::Sys { .. }) => {
                if !f.sticky {
                    self.q.pop_front();
                }
                self.q.push_front(n);
                true
            }
            Some(_) => {
                let at = (1..self.q.len())
                    .find(|&i| matches!(self.q[i].kind, NoticeKind::Sys { .. }))
                    .unwrap_or(self.q.len());
                self.q.insert(at, n);
                false
            }
        }
    }

    /// ¿Hay una alarma mostrada o en cola?
    pub fn has_alarm(&self) -> bool {
        self.q
            .iter()
            .any(|n| matches!(n.kind, NoticeKind::Alarm(_)))
    }

    pub fn current(&self) -> Option<&Notice> {
        self.q.front()
    }

    pub fn pop(&mut self) -> Option<Notice> {
        self.q.pop_front()
    }

    /// Quita los avisos que cumplan el predicado.
    pub fn remove_where(&mut self, f: impl Fn(&Notice) -> bool) {
        self.q.retain(|n| !f(n));
    }

    /// Quita los avisos de ese elemento (p. ej. al borrarlo; lo usará la UI de borrado).
    pub fn remove_item(&mut self, id: ItemId) {
        self.q.retain(|n| n.kind.item_id() != Some(id));
    }
}

pub fn notice_for(item: &Item, due: &Due, h12: bool) -> Notice {
    let hm = crate::clock::fmt_hm(due.at.time(), h12);
    match due.kind {
        DueKind::Alarm => Notice {
            kind: NoticeKind::Alarm(item.id),
            title: format!("{hm} · {}", item.title),
            subtitle: crate::fl!("notice-alarm"),
            sticky: true,
        },
        DueKind::TaskStart => Notice {
            kind: NoticeKind::TaskStart(item.id),
            title: item.title.clone(),
            subtitle: crate::fl!("notice-task-start", time = hm),
            sticky: false,
        },
        DueKind::TaskEnd => Notice {
            kind: NoticeKind::TaskEnd(item.id),
            title: item.title.clone(),
            subtitle: crate::fl!("notice-task-end"),
            sticky: false,
        },
    }
}

/// Aviso de CI: el fallo es fijo y con «Ver»; el éxito se cierra solo.
pub fn ci_notice(ev: &CiEvent) -> Notice {
    let (run, failed) = match ev {
        CiEvent::Failed(r) => (r, true),
        CiEvent::Finished(r) => (r, false),
    };
    let what = if run.name.is_empty() { "CI" } else { &run.name };
    Notice {
        kind: NoticeKind::Ci {
            url: run.url.clone(),
            failed,
        },
        title: if failed {
            crate::fl!(
                "notice-ci-failed",
                what = what,
                project = run.project.as_str()
            )
        } else {
            crate::fl!("notice-ci-ok", what = what, project = run.project.as_str())
        },
        subtitle: run.branch.clone(),
        sticky: failed,
    }
}

/// Aviso de batería, Wi-Fi o Bluetooth (textos del boceto aprobado).
pub fn notice_from(n: &SysNotice) -> Notice {
    let plug = crate::fl!("notice-plug-in");
    let (icon, title, subtitle, critical) = match n {
        SysNotice::BatteryLow { p, mins } => (
            "battery-caution-symbolic",
            crate::fl!("notice-battery-low", p = u32::from(*p)),
            mins.map_or(plug, |m| crate::fl!("notice-battery-remaining", m = m)),
            false,
        ),
        SysNotice::BatteryCritical { p } => (
            "battery-caution-symbolic",
            crate::fl!("notice-battery-critical", p = u32::from(*p)),
            plug,
            true,
        ),
        SysNotice::BatteryFull => (
            "battery-full-charged-symbolic",
            crate::fl!("notice-battery-full"),
            crate::fl!("notice-battery-full-sub"),
            false,
        ),
        SysNotice::WifiConnected(ssid) => (
            "network-wireless-symbolic",
            crate::fl!("notice-wifi-connected"),
            ssid.clone(),
            false,
        ),
        SysNotice::WifiDisconnected => (
            "network-wireless-offline-symbolic",
            crate::fl!("notice-wifi-disconnected"),
            String::new(),
            false,
        ),
        SysNotice::NoInternet { ssid, portal } => (
            "network-wireless-offline-symbolic",
            crate::fl!("notice-no-internet"),
            if *portal {
                crate::fl!("notice-portal")
            } else {
                crate::fl!("notice-no-access", ssid = ssid.as_str())
            },
            false,
        ),
        SysNotice::BtConnected { name, battery } => (
            "audio-headphones-symbolic",
            crate::fl!("notice-bt-connected", name = name.as_str()),
            battery.map_or_else(String::new, |b| {
                crate::fl!("notice-bt-battery", p = u32::from(b))
            }),
            false,
        ),
        SysNotice::BtDisconnected { name } => (
            "audio-headphones-symbolic",
            crate::fl!("notice-bt-disconnected", name = name.as_str()),
            String::new(),
            false,
        ),
    };
    Notice {
        kind: NoticeKind::Sys { icon, critical },
        title,
        subtitle,
        sticky: critical,
    }
}

pub fn missed_notice(m: &Missed) -> Option<Notice> {
    let (a, t) = (m.alarms.len(), m.tasks.len());
    if a == 0 && t == 0 {
        return None;
    }
    let mut parts = Vec::new();
    if a > 0 {
        parts.push(crate::fl!("notice-missed-alarms", n = a));
    }
    if t > 0 {
        parts.push(crate::fl!("notice-missed-tasks", n = t));
    }
    Some(Notice {
        kind: NoticeKind::Missed {
            alarms: a,
            tasks: t,
        },
        title: if a > 0 {
            crate::fl!("notice-missed-alarms-title")
        } else {
            crate::fl!("notice-missed-title")
        },
        subtitle: crate::fl!(
            "notice-missed-sub",
            parts = match parts.as_slice() {
                [a, b] => crate::fl!("notice-missed-and", a = a.as_str(), b = b.as_str()),
                _ => parts.join(""),
            }
        ),
        sticky: true,
    })
}

/// Vista del aviso para la pastilla cerrada (ícono, textos y botones).
pub fn view<'a, M: Clone + 'static>(
    n: &Notice,
    on: impl Fn(NoticeAction) -> M + 'a,
) -> Element<'a, M> {
    let text = |s: String, size: u16, c: Color| {
        widget::text(s)
            .size(size)
            .wrapping(cosmic::iced::widget::text::Wrapping::None)
            .class(cosmic::theme::Text::Color(c))
    };
    let btn = |label: String, a: NoticeAction| {
        widget::button::text(label).on_press(on(a)).padding([2, 8])
    };
    let mut buttons = widget::Row::new().spacing(4).align_y(Alignment::Center);
    match n.kind {
        NoticeKind::Ci { failed: true, .. } => {
            buttons = buttons
                .push(btn(crate::fl!("notice-view"), NoticeAction::View))
                .push(btn(crate::fl!("notice-ok"), NoticeAction::Dismiss));
        }
        NoticeKind::Alarm(_) => {
            buttons = buttons
                .push(btn("5".into(), NoticeAction::Snooze(5)))
                .push(btn("10".into(), NoticeAction::Snooze(10)))
                .push(btn(
                    crate::fl!("notice-snooze-15"),
                    NoticeAction::Snooze(15),
                ))
                .push(btn(crate::fl!("notice-done"), NoticeAction::Dismiss));
        }
        NoticeKind::TaskStart(_) | NoticeKind::TaskEnd(_) => {
            buttons = buttons
                .push(btn(crate::fl!("notice-later"), NoticeAction::Snooze(5)))
                .push(btn(crate::fl!("notice-ok"), NoticeAction::Dismiss));
        }
        NoticeKind::Missed { .. }
        | NoticeKind::Info
        | NoticeKind::Ci { .. }
        | NoticeKind::Sys { .. } => {
            buttons = buttons.push(btn(crate::fl!("notice-ok"), NoticeAction::Dismiss));
        }
    }
    let body = widget::Column::new()
        .push(text(n.title.clone(), 14, Color::WHITE))
        .push(text(n.subtitle.clone(), 11, Color::from_rgb(0.6, 0.6, 0.6)));
    let icon: Element<'a, M> = match n.kind {
        NoticeKind::Sys { icon, .. } => widget::icon::from_name(icon)
            .size(18)
            .symbolic(true)
            .icon()
            .class(cosmic::theme::Svg::custom(|_| {
                cosmic::iced::widget::svg::Style {
                    color: Some(Color::WHITE),
                }
            }))
            .into(),
        _ => text(n.kind.icon().into(), 18, Color::WHITE).into(),
    };
    let open = widget::mouse_area(
        widget::Row::new()
            .spacing(8)
            .align_y(Alignment::Center)
            .push(icon)
            .push(widget::container(body).width(Length::Fill).clip(true)),
    )
    .on_press(on(NoticeAction::Open));
    widget::Row::new()
        .spacing(8)
        .align_y(Alignment::Center)
        .padding([0, 12])
        .push(widget::container(open).width(Length::Fill))
        .push(buttons)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agenda::schedule::{Due, DueKind};
    use crate::agenda::{Item, Kind};
    use chrono::{NaiveDate, NaiveTime};

    fn item(id: ItemId, title: &str) -> Item {
        Item {
            id,
            title: title.into(),
            kind: Kind::Alarm {
                time: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
                days: [false; 7],
                once_on: None,
                enabled: true,
            },
            snooze_until: None,
        }
    }

    fn due(id: ItemId, kind: DueKind, h: u32, m: u32) -> Due {
        Due {
            id,
            kind,
            at: NaiveDate::from_ymd_opt(2026, 10, 30)
                .unwrap()
                .and_hms_opt(h, m, 0)
                .unwrap(),
        }
    }

    #[test]
    fn notice_for_12h() {
        crate::i18n::apply_for_test("es");
        let a = notice_for(&item(1, "Salir"), &due(1, DueKind::Alarm, 18, 0), true);
        assert_eq!(a.title, "6:00 p. m. · Salir");
        let s = notice_for(&item(2, "E"), &due(2, DueKind::TaskStart, 0, 5), true);
        assert_eq!(s.subtitle, "Empieza ahora · 12:05 a. m.");
    }

    #[test]
    fn notice_for_texts_and_sticky() {
        crate::i18n::apply_for_test("es");
        let a = notice_for(&item(1, "Salir"), &due(1, DueKind::Alarm, 18, 0), false);
        assert_eq!(a.kind, NoticeKind::Alarm(1));
        assert_eq!(a.title, "18:00 · Salir");
        assert_eq!(a.subtitle, "Alarma");
        assert!(a.sticky);
        let s = notice_for(
            &item(2, "Estudiar"),
            &due(2, DueKind::TaskStart, 15, 0),
            false,
        );
        assert_eq!(s.kind, NoticeKind::TaskStart(2));
        assert_eq!(s.title, "Estudiar");
        assert_eq!(s.subtitle, "Empieza ahora · 15:00");
        assert!(!s.sticky);
        let e = notice_for(
            &item(2, "Estudiar"),
            &due(2, DueKind::TaskEnd, 16, 0),
            false,
        );
        assert_eq!(e.kind, NoticeKind::TaskEnd(2));
        assert_eq!(e.subtitle, "Se acabó el tiempo");
        assert!(!e.sticky);
    }

    #[test]
    fn missed_notice_cases() {
        crate::i18n::apply_for_test("es");
        let mk = |a: usize, t: usize| Missed {
            alarms: (0..a)
                .map(|i| due(i as u64, DueKind::Alarm, 1, 0))
                .collect(),
            tasks: (0..t)
                .map(|i| due(i as u64, DueKind::TaskStart, 1, 0))
                .collect(),
        };
        assert_eq!(missed_notice(&mk(0, 0)), None);
        let n = missed_notice(&mk(2, 1)).unwrap();
        assert_eq!(n.title, "Alarmas perdidas");
        assert_eq!(n.subtitle, "2 alarmas y 1 tarea sin avisar");
        assert!(n.sticky);
        let one = missed_notice(&mk(1, 0)).unwrap();
        assert_eq!(one.title, "Alarmas perdidas");
        assert_eq!(one.subtitle, "1 alarma sin avisar");
        let tasks = missed_notice(&mk(0, 3)).unwrap();
        assert_eq!(tasks.title, "Avisos perdidos");
        assert_eq!(tasks.subtitle, "3 tareas sin avisar");
    }

    #[test]
    fn queue_remove_item() {
        crate::i18n::apply_for_test("es");
        let mut q = NoticeQueue::default();
        let n1 = notice_for(&item(1, "A"), &due(1, DueKind::Alarm, 8, 0), false);
        let n2 = notice_for(&item(2, "B"), &due(2, DueKind::Alarm, 8, 0), false);
        let info = Notice {
            kind: NoticeKind::Info,
            title: "x".into(),
            subtitle: "y".into(),
            sticky: true,
        };
        q.push(n1);
        q.push(info.clone());
        q.push(n2.clone());
        q.push(notice_for(
            &item(1, "A"),
            &due(1, DueKind::TaskEnd, 9, 0),
            false,
        ));
        q.remove_item(1);
        assert_eq!(q.pop(), Some(info));
        assert_eq!(q.current(), Some(&n2));
        assert_eq!(q.pop(), Some(n2));
        assert_eq!(q.pop(), None);
    }

    #[test]
    fn ci_notice_cases() {
        crate::i18n::apply_for_test("es");
        use crate::dev::forge::{Forge, Pipeline};
        use crate::dev::github::RunStatus;
        let run = Pipeline {
            forge: Forge::GitHub,
            project: "o/r".into(),
            id: 1,
            name: "Build".into(),
            branch: "main".into(),
            status: RunStatus::Completed,
            conclusion: Some("failure".into()),
            url: "https://github.com/o/r/actions/runs/1".into(),
            updated: String::new(),
        };
        let f = ci_notice(&CiEvent::Failed(run.clone()));
        assert!(f.sticky);
        assert_eq!(f.title, "Build falló · o/r");
        assert_eq!(f.subtitle, "main");
        assert_eq!(
            f.kind,
            NoticeKind::Ci {
                url: run.url.clone(),
                failed: true
            }
        );
        assert_eq!(f.kind.target_page(), Page::Dev);
        assert_eq!(f.kind.item_id(), None);
        let ok = ci_notice(&CiEvent::Finished(run));
        assert!(!ok.sticky);
        assert_eq!(ok.title, "Build terminó bien · o/r");
    }

    #[test]
    fn notice_from_texts() {
        crate::i18n::apply_for_test("es");
        use crate::sys::rules::SysNotice as S;
        let t = |n: S| {
            let n = notice_from(&n);
            (n.title, n.subtitle, n.sticky)
        };
        assert_eq!(
            t(S::BatteryLow {
                p: 20,
                mins: Some(35)
            }),
            ("Batería baja · 20 %".into(), "Quedan ~35 min".into(), false)
        );
        assert_eq!(
            t(S::BatteryLow { p: 19, mins: None }),
            (
                "Batería baja · 19 %".into(),
                "Conecta el cargador".into(),
                false
            )
        );
        assert_eq!(
            t(S::BatteryCritical { p: 10 }),
            (
                "Batería crítica · 10 %".into(),
                "Conecta el cargador".into(),
                true
            )
        );
        assert_eq!(
            t(S::BatteryFull),
            (
                "Batería cargada".into(),
                "100 % · puedes desconectar".into(),
                false
            )
        );
        assert_eq!(
            t(S::WifiConnected("Casa".into())),
            ("Wi-Fi conectado".into(), "Casa".into(), false)
        );
        assert_eq!(
            t(S::WifiDisconnected),
            ("Wi-Fi desconectado".into(), String::new(), false)
        );
        assert_eq!(
            t(S::NoInternet {
                ssid: "Cafe".into(),
                portal: true
            }),
            (
                "Sin internet".into(),
                "La red pide iniciar sesión".into(),
                false
            )
        );
        assert_eq!(
            t(S::NoInternet {
                ssid: "Cafe".into(),
                portal: false
            }),
            (
                "Sin internet".into(),
                "Conectado a Cafe sin acceso".into(),
                false
            )
        );
        assert_eq!(
            t(S::BtConnected {
                name: "AirPods".into(),
                battery: Some(80)
            }),
            ("AirPods conectado".into(), "Batería 80 %".into(), false)
        );
        assert_eq!(
            t(S::BtConnected {
                name: "AirPods".into(),
                battery: None
            }),
            ("AirPods conectado".into(), String::new(), false)
        );
        assert_eq!(
            t(S::BtDisconnected {
                name: "AirPods".into()
            }),
            ("AirPods desconectado".into(), String::new(), false)
        );
    }

    #[test]
    fn sys_notice_kind_and_duration() {
        crate::i18n::apply_for_test("es");
        use crate::sys::rules::SysNotice as S;
        let n = notice_from(&S::WifiDisconnected);
        assert_eq!(
            n.kind,
            NoticeKind::Sys {
                icon: "network-wireless-offline-symbolic",
                critical: false
            }
        );
        assert_eq!(n.kind.secs(), 4);
        assert_eq!(n.kind.item_id(), None);
        assert!(matches!(
            notice_from(&S::BatteryCritical { p: 9 }).kind,
            NoticeKind::Sys { critical: true, .. }
        ));
        assert_eq!(NoticeKind::Alarm(1).secs(), 15);
        assert_eq!(NoticeKind::Info.secs(), 15);
    }

    fn sys(n: SysNotice) -> Notice {
        notice_from(&n)
    }
    fn alarm(id: ItemId) -> Notice {
        notice_for(&item(id, "A"), &due(id, DueKind::Alarm, 8, 0), false)
    }
    fn titles(q: &NoticeQueue) -> Vec<String> {
        q.q.iter().map(|n| n.title.clone()).collect()
    }

    #[test]
    fn alarm_replaces_current_non_sticky_sys() {
        crate::i18n::apply_for_test("es");
        let mut q = NoticeQueue::default();
        q.push(sys(SysNotice::WifiDisconnected));
        assert!(q.push(alarm(1)));
        assert_eq!(titles(&q), vec!["08:00 · A"]);
    }

    #[test]
    fn alarm_goes_ahead_of_critical_battery_which_is_requeued() {
        crate::i18n::apply_for_test("es");
        let mut q = NoticeQueue::default();
        q.push(sys(SysNotice::BatteryCritical { p: 9 }));
        assert!(q.push(alarm(1)));
        assert_eq!(titles(&q), vec!["08:00 · A", "Batería crítica · 9 %"]);
    }

    #[test]
    fn alarm_goes_ahead_of_queued_sys_but_behind_current_alarm() {
        crate::i18n::apply_for_test("es");
        let mut q = NoticeQueue::default();
        q.push(alarm(1));
        q.push(sys(SysNotice::WifiDisconnected));
        q.push(sys(SysNotice::BatteryCritical { p: 9 }));
        assert!(!q.push(alarm(2)));
        assert_eq!(
            titles(&q),
            vec![
                "08:00 · A",
                "08:00 · A",
                "Wi-Fi desconectado",
                "Batería crítica · 9 %"
            ]
        );
        assert_eq!(q.current().and_then(|n| n.kind.item_id()), Some(1));
    }

    #[test]
    fn non_urgent_notices_append() {
        crate::i18n::apply_for_test("es");
        let mut q = NoticeQueue::default();
        q.push(sys(SysNotice::BatteryFull));
        let info = Notice {
            kind: NoticeKind::Info,
            title: "i".into(),
            subtitle: String::new(),
            sticky: false,
        };
        assert!(!q.push(info));
        assert!(!q.push(sys(SysNotice::WifiDisconnected)));
        assert_eq!(
            titles(&q),
            vec!["Batería cargada", "i", "Wi-Fi desconectado"]
        );
    }

    #[test]
    fn has_alarm_and_sound_ownership() {
        crate::i18n::apply_for_test("es");
        let mut q = NoticeQueue::default();
        assert!(!q.has_alarm());
        q.push(sys(SysNotice::BatteryFull));
        assert!(!q.has_alarm());
        q.push(alarm(3));
        assert!(q.has_alarm());
        assert!(NoticeKind::Alarm(1).owns_sound());
        assert!(NoticeKind::TaskStart(1).owns_sound());
        assert!(
            notice_from(&SysNotice::BatteryCritical { p: 5 })
                .kind
                .owns_sound()
        );
        assert!(!notice_from(&SysNotice::BatteryFull).kind.owns_sound());
        assert!(!NoticeKind::Info.owns_sound());
        assert!(
            !NoticeKind::Ci {
                url: String::new(),
                failed: true
            }
            .owns_sound()
        );
    }

    #[test]
    fn target_pages() {
        crate::i18n::apply_for_test("es");
        assert_eq!(NoticeKind::Alarm(1).target_page(), Page::Calendario);
        assert_eq!(NoticeKind::Info.target_page(), Page::Calendario);
        assert_eq!(NoticeKind::TaskEnd(1).target_page(), Page::Todo);
    }

    #[test]
    fn remove_where_quita_coincidentes() {
        crate::i18n::apply_for_test("es");
        let mk = |t: &str| Notice {
            kind: NoticeKind::Info,
            title: t.into(),
            subtitle: String::new(),
            sticky: false,
        };
        let mut q = NoticeQueue::default();
        q.push(mk("a"));
        q.push(mk("b"));
        q.remove_where(|n| n.title == "a");
        assert_eq!(q.current().map(|n| n.title.as_str()), Some("b"));
    }

    #[test]
    fn english_notices() {
        crate::i18n::apply_for_test("en");
        let a = notice_for(&item(1, "Leave"), &due(1, DueKind::Alarm, 18, 0), true);
        assert_eq!(a.title, "6:00 PM · Leave");
        assert_eq!(a.subtitle, "Alarm");
        let s = notice_for(&item(2, "E"), &due(2, DueKind::TaskStart, 0, 5), false);
        assert_eq!(s.subtitle, "Starting now · 00:05");
        let m = |al: usize, ta: usize| {
            missed_notice(&Missed {
                alarms: (0..al as u64)
                    .map(|i| due(i, DueKind::Alarm, 8, 0))
                    .collect(),
                tasks: (0..ta as u64)
                    .map(|i| due(i, DueKind::TaskStart, 8, 0))
                    .collect(),
            })
            .unwrap()
        };
        assert_eq!(m(2, 1).subtitle, "2 alarms and 1 task not notified");
        assert_eq!(m(1, 0).subtitle, "1 alarm not notified");
        assert_eq!(m(0, 3).title, "Missed notices");
        let t = |n: SysNotice| {
            let n = notice_from(&n);
            (n.title, n.subtitle)
        };
        assert_eq!(
            t(SysNotice::BatteryLow {
                p: 20,
                mins: Some(35)
            }),
            ("Battery low · 20%".into(), "About 35 min left".into())
        );
        assert_eq!(
            t(SysNotice::NoInternet {
                ssid: "Cafe".into(),
                portal: false
            }),
            (
                "No internet".into(),
                "Connected to Cafe without access".into()
            )
        );
    }
}
