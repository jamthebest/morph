//! Planificador de recordatorios y alarmas: calcula qué vence en una ventana.
use chrono::{Duration, NaiveDateTime, Timelike};

use super::{Agenda, ItemId, Kind, weekday_index};

#[derive(Clone, Debug, PartialEq)]
pub enum DueKind {
    TaskStart,
    TaskEnd,
    Alarm,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Due {
    pub id: ItemId,
    pub kind: DueKind,
    pub at: NaiveDateTime,
}

/// Avisos con `at` en la ventana `(from, to]`, ordenados por `at`.
pub fn due(a: &Agenda, from: NaiveDateTime, to: NaiveDateTime) -> Vec<Due> {
    let mut out = Vec::new();
    if to <= from {
        return out;
    }
    let inside = |at: NaiveDateTime| at > from && at <= to;
    for it in &a.items {
        match &it.kind {
            Kind::Task {
                date: Some(date),
                time: Some(time),
                duration_min,
                remind_start,
                remind_end,
                done: false,
            } => {
                let start = date.and_time(*time);
                if *remind_start && inside(start) {
                    out.push(Due {
                        id: it.id,
                        kind: DueKind::TaskStart,
                        at: start,
                    });
                }
                if *remind_end && let Some(m) = duration_min {
                    let end = start + Duration::minutes(*m as i64);
                    if inside(end) {
                        out.push(Due {
                            id: it.id,
                            kind: DueKind::TaskEnd,
                            at: end,
                        });
                    }
                }
            }
            Kind::Alarm {
                time,
                days,
                once_on,
                enabled: true,
            } => {
                if days.iter().any(|&x| x) {
                    let mut day = from.date();
                    while day <= to.date() {
                        let at = day.and_time(*time);
                        if days[weekday_index(day)] && inside(at) {
                            out.push(Due {
                                id: it.id,
                                kind: DueKind::Alarm,
                                at,
                            });
                        }
                        day = match day.succ_opt() {
                            Some(d) => d,
                            None => break,
                        };
                    }
                } else if let Some(d) = once_on {
                    let at = d.and_time(*time);
                    if inside(at) {
                        out.push(Due {
                            id: it.id,
                            kind: DueKind::Alarm,
                            at,
                        });
                    }
                }
            }
            _ => {}
        }
        // Posposición: aplica a cualquier elemento, esté activo o no.
        if let Some(s) = it.snooze_until
            && inside(s)
        {
            let kind = match it.kind {
                Kind::Alarm { .. } => DueKind::Alarm,
                Kind::Task { .. } => DueKind::TaskStart,
            };
            out.push(Due {
                id: it.id,
                kind,
                at: s,
            });
        }
    }
    out.sort_by_key(|d| d.at);
    out
}

/// Tras sonar: limpia la posposición; una alarma de una sola vez se desactiva.
pub fn on_fired(a: &mut Agenda, d: &Due) {
    if let Some(it) = a.items.iter_mut().find(|i| i.id == d.id) {
        it.snooze_until = None;
        if let Kind::Alarm {
            days,
            enabled,
            once_on: _,
            ..
        } = &mut it.kind
            && d.kind == DueKind::Alarm
            && !days.iter().any(|&x| x)
        {
            *enabled = false;
        }
    }
}

/// Trunca al minuto (segundos y nanosegundos a cero).
pub fn floor_to_minute(dt: NaiveDateTime) -> NaiveDateTime {
    dt.with_second(0)
        .and_then(|d| d.with_nanosecond(0))
        .unwrap_or(dt)
}

/// Pospone un elemento (tarea o alarma) hasta el minuto entero `minutes` minutos
/// después de `now` (sin segundos, para no perder el límite del minuto).
pub fn snooze(a: &mut Agenda, id: ItemId, minutes: u32, now: NaiveDateTime) {
    if let Some(it) = a.items.iter_mut().find(|i| i.id == id) {
        it.snooze_until = Some(floor_to_minute(now) + Duration::minutes(minutes as i64));
    }
}

pub struct Missed {
    pub alarms: Vec<Due>,
    pub tasks: Vec<Due>,
}

/// Separa los avisos que aún deben sonar (`at > now - 2 min`) de los perdidos.
pub fn classify_missed(dues: Vec<Due>, now: NaiveDateTime) -> (Vec<Due>, Missed) {
    let limit = now - Duration::minutes(2);
    let mut ring = Vec::new();
    let mut missed = Missed {
        alarms: Vec::new(),
        tasks: Vec::new(),
    };
    for d in dues {
        if d.at > limit {
            ring.push(d);
        } else if d.kind == DueKind::Alarm {
            missed.alarms.push(d);
        } else {
            missed.tasks.push(d);
        }
    }
    (ring, missed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveTime};

    fn dt(d: u32, h: u32, m: u32) -> NaiveDateTime {
        // octubre/noviembre 2026: d>=32 => noviembre
        let (mo, day) = if d > 31 { (11, d - 31) } else { (10, d) };
        NaiveDate::from_ymd_opt(2026, mo, day)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
    }
    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }
    fn task(rs: bool, re: bool, done: bool, time: Option<NaiveTime>) -> Kind {
        Kind::Task {
            date: NaiveDate::from_ymd_opt(2026, 10, 30),
            time,
            duration_min: Some(60),
            remind_start: rs,
            remind_end: re,
            done,
        }
    }
    fn weekdays(time: NaiveTime) -> Kind {
        Kind::Alarm {
            time,
            days: [true, true, true, true, true, false, false],
            once_on: None,
            enabled: true,
        }
    }
    fn once(time: NaiveTime, on: NaiveDate) -> Kind {
        Kind::Alarm {
            time,
            days: [false; 7],
            once_on: Some(on),
            enabled: true,
        }
    }

    #[test]
    fn tarea_inicio_y_fin() {
        let mut a = Agenda::default();
        let id = a.add("t".into(), task(true, true, false, Some(t(15, 0))));
        let v = due(&a, dt(30, 14, 59), dt(30, 16, 0));
        assert_eq!(
            v,
            vec![
                Due {
                    id,
                    kind: DueKind::TaskStart,
                    at: dt(30, 15, 0)
                },
                Due {
                    id,
                    kind: DueKind::TaskEnd,
                    at: dt(30, 16, 0)
                },
            ]
        );
        // límite inferior abierto
        let v = due(&a, dt(30, 15, 0), dt(30, 16, 0));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].kind, DueKind::TaskEnd);
    }

    #[test]
    fn tarea_solo_inicio_hecha_o_sin_hora() {
        let mut a = Agenda::default();
        a.add("t".into(), task(true, false, false, Some(t(15, 0))));
        let v = due(&a, dt(30, 14, 0), dt(30, 17, 0));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].kind, DueKind::TaskStart);

        let mut b = Agenda::default();
        b.add("t".into(), task(true, true, true, Some(t(15, 0))));
        b.add("s".into(), task(true, true, false, None));
        assert!(due(&b, dt(30, 0, 0), dt(31, 23, 0)).is_empty());
    }

    #[test]
    fn alarma_laborables() {
        let mut a = Agenda::default();
        a.add("al".into(), weekdays(t(18, 0)));
        assert_eq!(due(&a, dt(30, 17, 59), dt(30, 18, 0)).len(), 1);
        assert_eq!(due(&a, dt(31, 17, 0), dt(31, 19, 0)).len(), 0);
        assert_eq!(due(&a, dt(29, 17, 0), dt(30, 19, 0)).len(), 2);
    }

    #[test]
    fn alarma_cruza_medianoche() {
        let mut a = Agenda::default();
        a.add("al".into(), weekdays(t(23, 59)));
        let v = due(&a, dt(30, 23, 58), dt(31, 0, 1));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].at, dt(30, 23, 59));
    }

    #[test]
    fn alarma_de_una_vez() {
        let mut a = Agenda::default();
        let id = a.add(
            "al".into(),
            once(t(7, 0), NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()),
        );
        assert_eq!(due(&a, dt(30, 0, 0), dt(30, 23, 59)).len(), 0);
        let v = due(&a, dt(31, 6, 0), dt(31, 8, 0));
        assert_eq!(v.len(), 1);
        on_fired(&mut a, &v[0]);
        match &a.items[0].kind {
            Kind::Alarm { enabled, .. } => assert!(!*enabled),
            _ => panic!(),
        }
        assert_eq!(a.items[0].id, id);
        assert!(due(&a, dt(31, 6, 0), dt(31, 8, 0)).is_empty());
    }

    fn at(h: u32, m: u32, s: u32, ms: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 30)
            .unwrap()
            .and_hms_milli_opt(h, m, s, ms)
            .unwrap()
    }

    #[test]
    fn floor_to_minute_trunca() {
        assert_eq!(floor_to_minute(at(11, 45, 37, 500)), dt(30, 11, 45));
        assert_eq!(floor_to_minute(at(11, 45, 0, 0)), dt(30, 11, 45));
    }

    #[test]
    fn posponer_guarda_minuto_entero() {
        let mut a = Agenda::default();
        let id = a.add("al".into(), weekdays(t(18, 0)));
        snooze(&mut a, id, 5, at(11, 45, 37, 500));
        assert_eq!(a.items[0].snooze_until, Some(dt(30, 11, 50)));
    }

    #[test]
    fn ventana_con_despertar_temprano_cubre_el_limite() {
        let mut a = Agenda::default();
        let id = a.add("al".into(), weekdays(t(18, 0)));
        snooze(&mut a, id, 5, at(11, 45, 37, 500));
        let to = floor_to_minute(at(11, 49, 59, 200) + Duration::seconds(2));
        assert_eq!(to, dt(30, 11, 50));
        let v = due(&a, dt(30, 11, 49), to);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].at, dt(30, 11, 50));
    }

    #[test]
    fn alarma_suena_una_sola_vez_con_ventanas_contiguas() {
        let mut a = Agenda::default();
        a.add("al".into(), weekdays(t(18, 0)));
        let ticks = [at(17, 58, 59, 990), at(17, 59, 59, 990), at(18, 0, 59, 990)];
        let mut last = dt(30, 17, 58);
        let mut n = 0;
        for now in ticks {
            let to = floor_to_minute(now + Duration::seconds(2));
            n += due(&a, last, to).len();
            last = to;
        }
        assert_eq!(n, 1);
    }

    #[test]
    fn posponer_alarma() {
        let mut a = Agenda::default();
        let id = a.add("al".into(), weekdays(t(18, 0)));
        snooze(&mut a, id, 10, dt(30, 18, 0));
        let v = due(&a, dt(30, 18, 0), dt(30, 18, 10));
        assert_eq!(
            v,
            vec![Due {
                id,
                kind: DueKind::Alarm,
                at: dt(30, 18, 10)
            }]
        );
        on_fired(&mut a, &v[0]);
        assert_eq!(a.items[0].snooze_until, None);
        // alarma repetida sigue activa
        match &a.items[0].kind {
            Kind::Alarm { enabled, .. } => assert!(*enabled),
            _ => panic!(),
        }
        assert!(due(&a, dt(30, 18, 0), dt(30, 18, 10)).is_empty());
    }

    #[test]
    fn posponer_tarea() {
        let mut a = Agenda::default();
        let id = a.add("t".into(), task(true, true, false, Some(t(15, 0))));
        snooze(&mut a, id, 5, dt(30, 15, 0));
        let v = due(&a, dt(30, 15, 0), dt(30, 15, 5));
        assert_eq!(
            v,
            vec![Due {
                id,
                kind: DueKind::TaskStart,
                at: dt(30, 15, 5)
            }]
        );
        on_fired(&mut a, &v[0]);
        assert_eq!(a.items[0].snooze_until, None);
    }

    #[test]
    fn clasifica_perdidos() {
        let d18 = Due {
            id: 1,
            kind: DueKind::Alarm,
            at: dt(30, 18, 0),
        };
        let d08 = Due {
            id: 2,
            kind: DueKind::Alarm,
            at: dt(30, 8, 0),
        };
        let (ring, missed) = classify_missed(vec![d18.clone(), d08.clone()], dt(30, 18, 1));
        assert_eq!(ring, vec![d18]);
        assert_eq!(missed.alarms, vec![d08]);
        assert!(missed.tasks.is_empty());
    }
}
