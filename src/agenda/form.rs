//! Estado y validación del formulario de tarea/alarma.

use super::{Item, ItemId, Kind};
use chrono::{NaiveDate, NaiveTime, Timelike};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormKind {
    Task,
    Alarm,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Form {
    pub editing: Option<ItemId>,
    pub kind: FormKind,
    pub date: Option<NaiveDate>,
    pub title: String,
    pub time: Option<NaiveTime>,
    /// Texto de los campos de hora y minuto (lo escrito, tal cual).
    pub hour_text: String,
    pub min_text: String,
    pub duration: String,
    pub remind_start: bool,
    pub remind_end: bool,
    pub days: [bool; 7],
    /// Hora puesta sola al pasar a Alarma (se quita al volver a Tarea si no se tocó).
    pub auto_time: Option<NaiveTime>,
    pub error: Option<String>,
    /// Selector de hora en formato de 12 h (lo fija `App` según la config).
    pub h12: bool,
}

/// Cuál de los dos selectores de hora recibe la rueda.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spinner {
    Hour,
    Minute,
}

/// Hora de 24 h → (1–12, es p. m.).
pub fn hour12(h24: u32) -> (u32, bool) {
    (
        if h24.is_multiple_of(12) { 12 } else { h24 % 12 },
        h24 >= 12,
    )
}

/// (1–12, p. m.) → hora de 24 h.
pub fn hour24(h12: u32, pm: bool) -> u32 {
    h12 % 12 + if pm { 12 } else { 0 }
}

/// Próxima hora en punto (23:xx → 00:00).
pub fn next_full_hour(now: NaiveTime) -> NaiveTime {
    NaiveTime::from_hms_opt((now.hour() + 1) % 24, 0, 0).unwrap()
}

/// Suma `delta` horas con vuelta (23 → 00); los minutos no cambian.
pub fn step_hour(t: NaiveTime, delta: i32) -> NaiveTime {
    let h = (t.hour() as i32 + delta).rem_euclid(24) as u32;
    NaiveTime::from_hms_opt(h, t.minute(), 0).unwrap()
}

/// Suma `delta` minutos con vuelta (59 → 00) sin acarreo a la hora.
pub fn step_minute1(t: NaiveTime, delta: i32) -> NaiveTime {
    let m = (t.minute() as i32 + delta).rem_euclid(60) as u32;
    NaiveTime::from_hms_opt(t.hour(), m, 0).unwrap()
}

/// `None` si el texto se rechaza (no son 0-2 dígitos); `Some(None)` si es válido pero
/// no es un valor en rango (vacío o mayor que `max`).
fn accept(s: &str, max: u32) -> Option<Option<u32>> {
    if s.len() > 2 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(s.parse::<u32>().ok().filter(|v| *v <= max))
}

impl Form {
    fn base(kind: FormKind, date: Option<NaiveDate>) -> Self {
        Form {
            editing: None,
            kind,
            date,
            title: String::new(),
            time: None,
            hour_text: String::new(),
            min_text: String::new(),
            duration: String::new(),
            remind_start: false,
            remind_end: false,
            days: [false; 7],
            auto_time: None,
            error: None,
            h12: false,
        }
    }

    pub fn new_task(date: Option<NaiveDate>) -> Self {
        Form {
            remind_start: true,
            ..Self::base(FormKind::Task, date)
        }
    }

    /// `now` fija la hora inicial (la próxima hora en punto).
    #[allow(dead_code)] // por ahora solo se usa en pruebas
    pub fn new_alarm(date: NaiveDate, now: NaiveTime) -> Self {
        let mut f = Self::base(FormKind::Alarm, Some(date));
        f.set_time(Some(next_full_hour(now)));
        f
    }

    /// Fija la hora y reescribe ambos textos con ceros a la izquierda.
    pub fn set_time(&mut self, t: Option<NaiveTime>) {
        self.time = t;
        let (h, m) = t.map_or((String::new(), String::new()), |t| {
            let h = if self.h12 {
                hour12(t.hour()).0
            } else {
                t.hour()
            };
            (format!("{h:02}"), format!("{:02}", t.minute()))
        });
        self.hour_text = h;
        self.min_text = m;
    }

    /// Cambia entre 12 h y 24 h y reescribe los textos.
    pub fn set_h12(&mut self, on: bool) {
        self.h12 = on;
        if self.time.is_some() {
            self.set_time(self.time);
        }
    }

    /// Pasa la hora a a. m. o p. m. (los minutos no cambian).
    pub fn set_pm(&mut self, pm: bool) {
        if let Some(t) = self.time {
            let h = hour24(hour12(t.hour()).0, pm);
            self.set_time(NaiveTime::from_hms_opt(h, t.minute(), 0));
        }
    }

    /// Texto de hora válido y en rango (1–12 en 12 h, 0–23 en 24 h).
    fn hour_value(&self, s: &str) -> Option<Option<u32>> {
        if self.h12 {
            accept(s, 12).map(|v| v.filter(|h| *h >= 1))
        } else {
            accept(s, 23)
        }
    }

    /// Suma `delta` a la hora o al minuto (con vuelta); reescribe los textos.
    pub fn step(&mut self, which: Spinner, delta: i32) {
        if let Some(t) = self.time {
            let n = match which {
                Spinner::Hour => step_hour(t, delta),
                Spinner::Minute => step_minute1(t, delta),
            };
            self.set_time(Some(n));
        }
    }

    pub fn type_hour(&mut self, s: &str) {
        if let Some(v) = self.hour_value(s) {
            self.hour_text = s.to_string();
            if let (Some(t), Some(h)) = (self.time, v) {
                let h = if self.h12 {
                    hour24(h, t.hour() >= 12)
                } else {
                    h
                };
                self.time = NaiveTime::from_hms_opt(h, t.minute(), 0);
            }
        }
    }

    pub fn type_minute(&mut self, s: &str) {
        if let Some(v) = accept(s, 59) {
            self.min_text = s.to_string();
            if let (Some(t), Some(m)) = (self.time, v) {
                self.time = NaiveTime::from_hms_opt(t.hour(), m, 0);
            }
        }
    }

    /// Cambia de tarea a alarma o viceversa. `today`/`now` rellenan lo que la alarma necesita.
    pub fn set_kind(&mut self, k: FormKind, today: NaiveDate, now: NaiveTime) {
        self.kind = k;
        match k {
            FormKind::Alarm => {
                if self.time.is_none() {
                    let t = next_full_hour(now);
                    self.set_time(Some(t));
                    self.auto_time = Some(t);
                }
                if self.date.is_none() {
                    self.date = Some(today);
                }
            }
            FormKind::Task => {
                if self.auto_time.is_some() && self.auto_time == self.time {
                    self.set_time(None);
                }
                self.auto_time = None;
            }
        }
    }

    pub fn edit(item: &Item) -> Self {
        let mut f = match &item.kind {
            Kind::Task {
                date,
                duration_min,
                remind_start,
                remind_end,
                ..
            } => Form {
                duration: duration_min.map(|d| d.to_string()).unwrap_or_default(),
                remind_start: *remind_start,
                remind_end: *remind_end,
                ..Self::base(FormKind::Task, *date)
            },
            Kind::Alarm { days, once_on, .. } => Form {
                days: *days,
                ..Self::base(FormKind::Alarm, *once_on)
            },
        };
        f.set_time(match &item.kind {
            Kind::Task { time, .. } => *time,
            Kind::Alarm { time, .. } => Some(*time),
        });
        f.editing = Some(item.id);
        f.title = item.title.clone();
        f
    }

    /// `today` fecha las tareas con hora pero sin fecha (si no, nunca avisarían).
    /// Título recortado y `Kind` listo para guardar (`done`/`enabled` los fija quien guarda).
    pub fn validate(&self, today: NaiveDate) -> Result<(String, Kind), String> {
        let title = self.title.trim().to_string();
        if title.is_empty() {
            return Err(crate::fl!("form-err-title"));
        }
        if self.time.is_some()
            && (self.hour_value(&self.hour_text).flatten().is_none()
                || accept(&self.min_text, 59).flatten().is_none())
        {
            return Err(crate::fl!("form-err-time"));
        }
        let dur_s = self.duration.trim();
        match self.kind {
            FormKind::Task => {
                let time = self.time;
                if time.is_none() && !dur_s.is_empty() {
                    return Err(crate::fl!("form-err-duration-needs-time"));
                }
                let duration_min = if dur_s.is_empty() {
                    None
                } else {
                    match dur_s.parse::<u32>() {
                        Ok(d) if (1..=1440).contains(&d) => Some(d),
                        _ => return Err(crate::fl!("form-err-duration-range")),
                    }
                };
                Ok((
                    title,
                    Kind::Task {
                        date: self.date.or(time.map(|_| today)),
                        time,
                        duration_min,
                        remind_start: self.remind_start,
                        remind_end: self.remind_end,
                        done: false,
                    },
                ))
            }
            FormKind::Alarm => {
                let time = self
                    .time
                    .ok_or_else(|| crate::fl!("form-err-alarm-needs-time"))?;
                let once_on = if self.days.iter().any(|&d| d) {
                    None
                } else {
                    Some(self.date.ok_or_else(|| crate::fl!("form-err-alarm-days"))?)
                };
                Ok((
                    title,
                    Kind::Alarm {
                        time,
                        days: self.days,
                        once_on,
                        enabled: true,
                    },
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn conversion_12_24() {
        assert_eq!(hour12(0), (12, false));
        assert_eq!(hour12(12), (12, true));
        assert_eq!(hour12(13), (1, true));
        assert_eq!(hour24(12, false), 0);
        assert_eq!(hour24(12, true), 12);
        assert_eq!(hour24(1, true), 13);
    }

    #[test]
    fn selector_12h() {
        crate::i18n::apply_for_test("es");
        let mut f = Form::new_alarm(d(30), t(22, 0));
        f.set_h12(true);
        assert_eq!(f.hour_text, "11");
        f.step(Spinner::Hour, 1);
        assert_eq!(f.time, Some(t(0, 0)));
        assert_eq!(f.hour_text, "12");
        f.type_hour("12");
        assert_eq!(f.time, Some(t(0, 0)));
        f.type_hour("5");
        assert_eq!(f.time, Some(t(5, 0)));
        f.type_hour("13");
        assert_eq!(f.time, Some(t(5, 0)));
        f.type_hour("0");
        assert_eq!(f.time, Some(t(5, 0)));
        f.title = "x".into();
        assert_eq!(f.validate(d(30)), Err("Hora no válida".into()));
        f.type_hour("5");
        assert!(f.validate(d(30)).is_ok());
        f.set_time(Some(t(9, 30)));
        f.set_pm(true);
        assert_eq!(f.time, Some(t(21, 30)));
        assert_eq!(f.hour_text, "09");
        f.type_hour("12");
        assert_eq!(f.time, Some(t(12, 30)));
        f.set_pm(false);
        assert_eq!(f.time, Some(t(0, 30)));
        f.set_h12(false);
        assert_eq!(f.hour_text, "00");
    }

    #[test]
    fn horas_con_vuelta() {
        assert_eq!(step_hour(t(23, 15), 1), t(0, 15));
        assert_eq!(step_hour(t(0, 15), -1), t(23, 15));
        assert_eq!(step_hour(t(7, 0), 1), t(8, 0));
    }

    #[test]
    fn escribir_hora_y_minuto() {
        let mut f = Form::new_alarm(d(30), t(6, 0));
        f.title = "x".into();
        assert_eq!((f.hour_text.as_str(), f.min_text.as_str()), ("07", "00"));
        f.type_hour("5");
        assert_eq!(f.time, Some(t(5, 0)));
        f.type_hour("7");
        assert_eq!(f.time, Some(t(7, 0)));
        f.type_minute("9");
        assert_eq!(f.time, Some(t(7, 9)));
        assert_eq!(f.min_text, "9");
        assert!(f.validate(d(30)).is_ok());
    }

    #[test]
    fn hora_fuera_de_rango_no_se_guarda() {
        crate::i18n::apply_for_test("es");
        let mut f = Form::new_alarm(d(30), t(6, 0));
        f.title = "x".into();
        f.type_hour("24");
        assert_eq!(f.validate(d(30)), Err("Hora no válida".into()));
        f.type_hour("7");
        f.type_minute("60");
        assert_eq!(f.validate(d(30)), Err("Hora no válida".into()));
        f.type_minute("");
        assert_eq!(f.validate(d(30)), Err("Hora no válida".into()));
        f.type_minute("59");
        assert!(f.validate(d(30)).is_ok());
    }

    #[test]
    fn texto_invalido_se_rechaza() {
        let mut f = Form::new_alarm(d(30), t(6, 0));
        f.type_hour("5a");
        assert_eq!(f.hour_text, "07");
        f.type_hour("123");
        assert_eq!(f.hour_text, "07");
        f.type_minute("-1");
        assert_eq!(f.min_text, "00");
        assert_eq!(f.time, Some(t(7, 0)));
    }

    #[test]
    fn flechas_reescriben_texto_con_ceros() {
        let mut f = Form::new_alarm(d(30), t(6, 0));
        f.type_minute("9");
        f.step(Spinner::Minute, 1);
        assert_eq!(f.time, Some(t(7, 10)));
        assert_eq!(f.min_text, "10");
        f.type_hour("5");
        f.step(Spinner::Hour, 1);
        assert_eq!(f.hour_text, "06");
        f.type_minute("");
        f.step(Spinner::Minute, -1);
        assert_eq!(f.min_text, "09");
        f.type_minute("59");
        f.step(Spinner::Minute, 1);
        assert_eq!(f.time, Some(t(6, 0)));
        assert_eq!(f.min_text, "00");
    }

    #[test]
    fn editar_rellena_textos() {
        let it = Item {
            id: 1,
            title: "a".into(),
            kind: Kind::Alarm {
                time: t(6, 5),
                days: [false; 7],
                once_on: Some(d(2)),
                enabled: true,
            },
            snooze_until: None,
        };
        let f = Form::edit(&it);
        assert_eq!((f.hour_text.as_str(), f.min_text.as_str()), ("06", "05"));
    }

    #[test]
    fn minuto_con_vuelta() {
        assert_eq!(step_minute1(t(7, 59), 1), t(7, 0));
        assert_eq!(step_minute1(t(7, 0), -1), t(7, 59));
        assert_eq!(step_minute1(t(7, 30), 1), t(7, 31));
    }

    #[test]
    fn tarea_alarma_tarea_quita_hora_automatica() {
        let mut f = task("a", None, "");
        f.set_kind(FormKind::Alarm, d(30), t(9, 41));
        assert_eq!(f.time, Some(t(10, 0)));
        f.set_kind(FormKind::Task, d(30), t(9, 41));
        assert_eq!(f.time, None);
        // Si el usuario cambió la hora, se conserva.
        f.set_kind(FormKind::Alarm, d(30), t(9, 41));
        f.time = Some(t(10, 5));
        f.set_kind(FormKind::Task, d(30), t(9, 41));
        assert_eq!(f.time, Some(t(10, 5)));
        // Una hora propia de la tarea nunca se quita.
        let mut g = task("a", Some(t(8, 0)), "");
        g.set_kind(FormKind::Alarm, d(30), t(9, 41));
        g.set_kind(FormKind::Task, d(30), t(9, 41));
        assert_eq!(g.time, Some(t(8, 0)));
    }

    #[test]
    fn proxima_hora_en_punto() {
        assert_eq!(next_full_hour(t(9, 41)), t(10, 0));
        assert_eq!(next_full_hour(t(23, 59)), t(0, 0));
        let a = Form::new_alarm(d(30), t(14, 20));
        assert_eq!(a.time, Some(t(15, 0)));
    }

    fn task(title: &str, time: Option<NaiveTime>, dur: &str) -> Form {
        let mut f = Form {
            title: title.into(),
            duration: dur.into(),
            ..Form::new_task(Some(d(30)))
        };
        f.set_time(time);
        f
    }

    #[test]
    fn errores() {
        crate::i18n::apply_for_test("es");
        assert_eq!(
            task("  ", None, "").validate(d(30)),
            Err("Escribe un título".into())
        );
        assert_eq!(
            task("a", None, "30").validate(d(30)),
            Err("La duración necesita una hora".into())
        );
        for bad in ["0", "2000", "x"] {
            assert_eq!(
                task("a", Some(t(9, 0)), bad).validate(d(30)),
                Err("Duración entre 1 y 1440 minutos".into())
            );
        }
        let mut a = Form::new_alarm(d(30), t(6, 0));
        a.title = "x".into();
        a.time = None;
        assert_eq!(a.validate(d(30)), Err("La alarma necesita una hora".into()));
    }

    #[test]
    fn tarea_valida() {
        let (t, k) = task(" Leer ", Some(t(9, 30)), "45")
            .validate(d(30))
            .unwrap();
        assert_eq!(t, "Leer");
        assert_eq!(
            k,
            Kind::Task {
                date: Some(d(30)),
                time: NaiveTime::from_hms_opt(9, 30, 0),
                duration_min: Some(45),
                remind_start: true,
                remind_end: false,
                done: false
            }
        );
    }

    #[test]
    fn alarma_una_vez_o_semanal() {
        let mut a = Form::new_alarm(d(30), t(6, 0));
        a.title = "x".into();
        a.time = Some(t(7, 0));
        let Ok((_, Kind::Alarm { once_on, .. })) = a.validate(d(30)) else {
            panic!()
        };
        assert_eq!(once_on, Some(d(30)));
        a.days[2] = true;
        let Ok((_, Kind::Alarm { once_on, days, .. })) = a.validate(d(30)) else {
            panic!()
        };
        assert_eq!(once_on, None);
        assert!(days[2]);
    }

    #[test]
    fn tarea_con_hora_sin_fecha_usa_hoy() {
        let mut f = task("a", Some(t(9, 0)), "");
        f.date = None;
        let Ok((_, Kind::Task { date, .. })) = f.validate(d(15)) else {
            panic!()
        };
        assert_eq!(date, Some(d(15)));
        // Sin hora y sin fecha sigue sin fecha.
        let mut g = task("a", None, "");
        g.date = None;
        let Ok((_, Kind::Task { date, .. })) = g.validate(d(15)) else {
            panic!()
        };
        assert_eq!(date, None);
    }

    #[test]
    fn alarma_sin_dias_ni_fecha_es_error() {
        crate::i18n::apply_for_test("es");
        let mut a = Form::new_alarm(d(30), t(6, 0));
        a.title = "x".into();
        a.time = Some(t(7, 0));
        a.date = None;
        assert_eq!(a.validate(d(30)), Err("Marca al menos un día".into()));
    }

    #[test]
    fn ida_y_vuelta() {
        let items = [
            Item {
                id: 3,
                title: "t".into(),
                kind: Kind::Task {
                    date: Some(d(1)),
                    time: NaiveTime::from_hms_opt(8, 5, 0),
                    duration_min: Some(20),
                    remind_start: false,
                    remind_end: true,
                    done: false,
                },
                snooze_until: None,
            },
            Item {
                id: 4,
                title: "a".into(),
                kind: Kind::Alarm {
                    time: NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
                    days: [true, false, false, false, true, false, false],
                    once_on: None,
                    enabled: true,
                },
                snooze_until: None,
            },
            Item {
                id: 5,
                title: "b".into(),
                kind: Kind::Alarm {
                    time: NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
                    days: [false; 7],
                    once_on: Some(d(2)),
                    enabled: true,
                },
                snooze_until: None,
            },
        ];
        for it in &items {
            let f = Form::edit(it);
            assert_eq!(f.editing, Some(it.id));
            let (t, k) = f.validate(d(30)).unwrap();
            assert_eq!(t, it.title);
            assert_eq!(k, it.kind);
        }
    }

    #[test]
    fn errores_en_ingles() {
        crate::i18n::apply_for_test("en");
        assert_eq!(
            task("  ", None, "").validate(d(30)),
            Err("Enter a title".into())
        );
    }
}
