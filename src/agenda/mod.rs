//! Agenda: tareas y alarmas con persistencia en JSON y planificador de avisos.

pub mod calendar_view;
pub mod form;
pub mod form_view;
pub mod month;
pub mod schedule;
pub mod store;
pub mod todo_view;

use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};

pub type ItemId = u64;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Task {
        date: Option<NaiveDate>,
        time: Option<NaiveTime>,
        duration_min: Option<u32>,
        remind_start: bool,
        remind_end: bool,
        done: bool,
    },
    Alarm {
        time: NaiveTime,
        /// Días de la semana, de lunes (0) a domingo (6).
        days: [bool; 7],
        /// Si todos los días son `false`: fecha de la única vez.
        once_on: Option<NaiveDate>,
        enabled: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: ItemId,
    pub title: String,
    pub kind: Kind,
    pub snooze_until: Option<NaiveDateTime>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Agenda {
    pub next_id: ItemId,
    pub items: Vec<Item>,
    pub last_check: Option<NaiveDateTime>,
}

/// Lunes = 0 … domingo = 6.
pub fn weekday_index(d: NaiveDate) -> usize {
    d.weekday().num_days_from_monday() as usize
}

impl Item {
    /// Hora del elemento en el día (si tiene).
    fn time_of_day(&self) -> Option<NaiveTime> {
        match &self.kind {
            Kind::Task { time, .. } => *time,
            Kind::Alarm { time, .. } => Some(*time),
        }
    }

    /// Hora de la posposición si cae en `date`.
    fn snooze_time_on(&self, date: NaiveDate) -> Option<NaiveTime> {
        self.snooze_until
            .filter(|s| s.date() == date)
            .map(|s| s.time())
    }

    fn touches(&self, date: NaiveDate) -> bool {
        self.snooze_time_on(date).is_some() || self.touches_by_schedule(date)
    }

    fn touches_by_schedule(&self, date: NaiveDate) -> bool {
        match &self.kind {
            Kind::Task { date: d, .. } => *d == Some(date),
            // Las desactivadas también se ven (para poder reactivarlas).
            Kind::Alarm { days, once_on, .. } => {
                if days.iter().any(|&x| x) {
                    days[weekday_index(date)]
                } else {
                    *once_on == Some(date)
                }
            }
        }
    }
}

impl Agenda {
    pub fn add(&mut self, title: String, kind: Kind) -> ItemId {
        // Garantiza ids únicos aunque `next_id` venga de un archivo editado.
        let floor = self.items.iter().map(|i| i.id + 1).max().unwrap_or(0);
        let id = self.next_id.max(floor).max(1);
        self.next_id = id + 1;
        self.items.push(Item {
            id,
            title,
            kind,
            snooze_until: None,
        });
        id
    }

    pub fn update(&mut self, id: ItemId, title: String, kind: Kind) -> bool {
        match self.items.iter_mut().find(|i| i.id == id) {
            Some(it) => {
                it.title = title;
                it.kind = kind;
                it.snooze_until = None;
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, id: ItemId) -> bool {
        let before = self.items.len();
        self.items.retain(|i| i.id != id);
        self.items.len() != before
    }

    /// Reinserta un elemento borrado con su id original. `false` (sin cambios) si ya hay uno con ese id.
    pub fn restore(&mut self, item: Item) -> bool {
        if self.items.iter().any(|i| i.id == item.id) {
            return false;
        }
        let mut item = item;
        // Una posposición ya vencida (el planificador pasó de largo) no debe quedar atascada.
        if let (Some(s), Some(last)) = (item.snooze_until, self.last_check)
            && s <= last
        {
            item.snooze_until = None;
        }
        self.items.push(item);
        true
    }

    /// Solo alarmas; desactivar también cancela una posposición pendiente.
    pub fn set_enabled(&mut self, id: ItemId, on: bool) -> bool {
        match self.items.iter_mut().find(|i| i.id == id) {
            Some(Item {
                kind: Kind::Alarm { enabled, .. },
                snooze_until,
                ..
            }) => {
                *enabled = on;
                if !on {
                    *snooze_until = None;
                }
                true
            }
            _ => false,
        }
    }

    /// Solo tareas; con una alarma o id inexistente devuelve `false`.
    pub fn toggle_done(&mut self, id: ItemId) -> bool {
        match self.items.iter_mut().find(|i| i.id == id) {
            Some(Item {
                kind: Kind::Task { done, .. },
                snooze_until,
                ..
            }) => {
                *done = !*done;
                // Una tarea hecha no debe sonar por una posposición pendiente.
                if *done {
                    *snooze_until = None;
                }
                true
            }
            _ => false,
        }
    }

    /// Elementos que tocan ese día, ordenados por hora (sin hora al final).
    pub fn items_on(&self, date: NaiveDate) -> Vec<&Item> {
        let mut v: Vec<&Item> = self.items.iter().filter(|i| i.touches(date)).collect();
        v.sort_by_key(|i| {
            let t = i.snooze_time_on(date).or_else(|| i.time_of_day());
            (t.is_none(), t)
        });
        v
    }

    /// Solo tareas: pendientes con fecha, pendientes sin fecha, completadas.
    pub fn todo_order(&self) -> Vec<&Item> {
        let mut dated = Vec::new();
        let mut undated = Vec::new();
        let mut done_v = Vec::new();
        for it in &self.items {
            if let Kind::Task { date, done, .. } = &it.kind {
                if *done {
                    done_v.push(it);
                } else if date.is_some() {
                    dated.push(it);
                } else {
                    undated.push(it);
                }
            }
        }
        dated.sort_by_key(|i| match &i.kind {
            Kind::Task { date, time, .. } => (*date, time.is_none(), *time),
            _ => unreachable!(),
        });
        dated.into_iter().chain(undated).chain(done_v).collect()
    }

    pub fn has_items(&self, date: NaiveDate) -> bool {
        self.items.iter().any(|i| i.touches(date))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }
    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }
    pub(crate) fn task(date: Option<NaiveDate>, time: Option<NaiveTime>) -> Kind {
        Kind::Task {
            date,
            time,
            duration_min: None,
            remind_start: true,
            remind_end: false,
            done: false,
        }
    }
    fn alarm(time: NaiveTime, days: [bool; 7]) -> Kind {
        Kind::Alarm {
            time,
            days,
            once_on: None,
            enabled: true,
        }
    }

    #[test]
    fn ids_crecientes_y_ops_inexistentes() {
        let mut a = Agenda::default();
        let i1 = a.add("a".into(), task(None, None));
        let i2 = a.add("b".into(), task(None, None));
        assert!(i2 > i1);
        assert!(!a.update(99, "x".into(), task(None, None)));
        assert!(!a.remove(99));
        assert!(!a.toggle_done(99));
        assert!(a.update(i1, "z".into(), task(None, None)));
        assert!(a.toggle_done(i1));
        assert!(a.remove(i1));
        assert_eq!(a.items.len(), 1);
    }

    #[test]
    fn toggle_done_en_alarma_falla() {
        let mut a = Agenda::default();
        let id = a.add("al".into(), alarm(t(7, 0), [true; 7]));
        assert!(!a.toggle_done(id));
    }

    #[test]
    fn items_on_ordena_por_hora() {
        let mut a = Agenda::default();
        let sin_hora = a.add("sin".into(), task(Some(d(2026, 10, 30)), None));
        let al = a.add(
            "al".into(),
            alarm(t(18, 0), [true, true, true, true, true, false, false]),
        );
        let t9 = a.add("t9".into(), task(Some(d(2026, 10, 30)), Some(t(9, 0))));
        let off = a.add(
            "off".into(),
            Kind::Alarm {
                time: t(8, 0),
                days: [true; 7],
                once_on: None,
                enabled: false,
            },
        );
        let ids: Vec<_> = a.items_on(d(2026, 10, 30)).iter().map(|i| i.id).collect();
        // La alarma desactivada también aparece (08:00).
        assert_eq!(ids, vec![off, t9, al, sin_hora]);
        assert!(a.has_items(d(2026, 10, 30)));
        // sábado: la alarma L-V no aparece (solo la desactivada diaria)
        let sab: Vec<_> = a.items_on(d(2026, 10, 31)).iter().map(|i| i.id).collect();
        assert_eq!(sab, vec![off]);
    }

    #[test]
    fn alarma_unica_pospuesta_sigue_visible() {
        let day = d(2026, 10, 30);
        let mut a = Agenda::default();
        let id = a.add(
            "al".into(),
            Kind::Alarm {
                time: t(11, 45),
                days: [false; 7],
                once_on: Some(day),
                enabled: true,
            },
        );
        let due = schedule::Due {
            id,
            kind: schedule::DueKind::Alarm,
            at: day.and_time(t(11, 45)),
        };
        schedule::on_fired(&mut a, &due);
        // Ya disparada queda desactivada pero visible.
        assert!(a.has_items(day));
        schedule::snooze(&mut a, id, 5, day.and_hms_opt(11, 45, 37).unwrap());
        assert!(a.has_items(day));
        assert_eq!(a.items_on(day).len(), 1);
        let due2 = schedule::Due {
            id,
            kind: schedule::DueKind::Alarm,
            at: day.and_time(t(11, 50)),
        };
        schedule::on_fired(&mut a, &due2);
        assert!(a.has_items(day));
        assert_eq!(a.items_on(day).len(), 1);
        assert_eq!(a.items[0].snooze_until, None);
    }

    #[test]
    fn items_on_ordena_pospuesta_por_su_hora() {
        let day = d(2026, 10, 30);
        let mut a = Agenda::default();
        let t10 = a.add("t10".into(), task(Some(day), Some(t(10, 0))));
        let t9 = a.add("t9".into(), task(Some(day), Some(t(9, 0))));
        a.items[1].snooze_until = Some(day.and_time(t(11, 0)));
        let ids: Vec<_> = a.items_on(day).iter().map(|i| i.id).collect();
        assert_eq!(ids, vec![t10, t9]);
    }

    #[test]
    fn todo_order_agrupa() {
        let mut a = Agenda::default();
        let manana = a.add("mañana".into(), task(Some(d(2026, 10, 31)), None));
        let ua = a.add("A".into(), task(None, None));
        let hoy = a.add("hoy".into(), task(Some(d(2026, 10, 30)), Some(t(15, 0))));
        let hecha = a.add("hecha".into(), task(None, None));
        a.toggle_done(hecha);
        let ub = a.add("B".into(), task(None, None));
        a.add("al".into(), alarm(t(7, 0), [true; 7]));
        let ids: Vec<_> = a.todo_order().iter().map(|i| i.id).collect();
        assert_eq!(ids, vec![hoy, manana, ua, ub, hecha]);
    }

    fn alarm_off(days: [bool; 7], once_on: Option<NaiveDate>) -> Kind {
        Kind::Alarm {
            time: t(7, 0),
            days,
            once_on,
            enabled: false,
        }
    }

    #[test]
    fn alarma_desactivada_se_ve_pero_no_suena() {
        let mut a = Agenda::default();
        // 2026-10-30 es viernes (índice 4).
        let rep = a.add(
            "rep".into(),
            alarm_off([false, false, false, false, true, false, false], None),
        );
        let once = a.add("once".into(), alarm_off([false; 7], Some(d(2026, 10, 31))));
        let ids = |a: &Agenda, day| a.items_on(day).iter().map(|i| i.id).collect::<Vec<_>>();
        assert_eq!(ids(&a, d(2026, 10, 30)), vec![rep]);
        assert_eq!(ids(&a, d(2026, 10, 31)), vec![once]);
        assert!(a.has_items(d(2026, 10, 31)));
        assert!(ids(&a, d(2026, 11, 1)).is_empty());
        let dues = schedule::due(
            &a,
            d(2026, 10, 29).and_time(t(0, 0)),
            d(2026, 11, 2).and_time(t(0, 0)),
        );
        assert!(dues.is_empty());
    }

    #[test]
    fn set_enabled_apaga_y_limpia_posposicion() {
        let mut a = Agenda::default();
        let id = a.add("al".into(), alarm(t(7, 0), [true; 7]));
        a.items[0].snooze_until = Some(d(2026, 10, 30).and_time(t(7, 5)));
        assert!(a.set_enabled(id, false));
        assert_eq!(a.items[0].snooze_until, None);
        assert!(matches!(
            a.items[0].kind,
            Kind::Alarm { enabled: false, .. }
        ));
        a.items[0].snooze_until = Some(d(2026, 10, 30).and_time(t(7, 5)));
        assert!(a.set_enabled(id, true));
        // Encender no toca la posposición.
        assert!(a.items[0].snooze_until.is_some());
        assert!(matches!(a.items[0].kind, Kind::Alarm { enabled: true, .. }));
        assert!(!a.set_enabled(99, true));
        let tk = a.add("t".into(), task(None, None));
        assert!(!a.set_enabled(tk, false));
    }

    #[test]
    fn restore_reinserta_con_mismo_id() {
        let mut a = Agenda::default();
        let id = a.add("a".into(), task(None, None));
        let next = a.next_id;
        let it = a.items[0].clone();
        assert!(a.remove(id));
        assert!(a.restore(it.clone()));
        assert_eq!(a.items, vec![it.clone()]);
        assert_eq!(a.next_id, next);
        // Ya existe: no hace nada.
        assert!(!a.restore(Item {
            title: "otro".into(),
            ..it.clone()
        }));
        assert_eq!(a.items, vec![it]);
    }

    #[test]
    fn restore_limpia_posposicion_vencida() {
        let day = d(2026, 10, 30);
        let mut a = Agenda::default();
        let id = a.add("a".into(), task(Some(day), Some(t(9, 0))));
        a.items[0].snooze_until = Some(day.and_time(t(9, 5)));
        let it = a.items[0].clone();
        a.remove(id);
        a.last_check = Some(day.and_time(t(9, 5)));
        assert!(a.restore(it.clone()));
        assert_eq!(a.items[0].snooze_until, None);
        // Posposición futura: se conserva.
        a.remove(id);
        a.last_check = Some(day.and_time(t(9, 4)));
        assert!(a.restore(it));
        assert!(a.items[0].snooze_until.is_some());
    }

    #[test]
    fn marcar_hecha_limpia_posposicion() {
        let day = d(2026, 10, 30);
        let mut a = Agenda::default();
        let id = a.add("a".into(), task(Some(day), Some(t(9, 0))));
        a.items[0].snooze_until = Some(day.and_time(t(9, 5)));
        assert!(a.toggle_done(id));
        assert_eq!(a.items[0].snooze_until, None);
    }

    #[test]
    fn weekday_index_lunes_domingo() {
        assert_eq!(weekday_index(d(2026, 11, 2)), 0);
        assert_eq!(weekday_index(d(2026, 11, 8)), 6);
    }
}
