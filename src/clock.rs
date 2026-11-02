use crate::agenda::form::hour12;
use chrono::{DateTime, Datelike, Local, NaiveTime, Timelike};
use std::time::Duration;

fn weekday(i: usize) -> String {
    match i {
        0 => crate::fl!("time-day-mon"),
        1 => crate::fl!("time-day-tue"),
        2 => crate::fl!("time-day-wed"),
        3 => crate::fl!("time-day-thu"),
        4 => crate::fl!("time-day-fri"),
        5 => crate::fl!("time-day-sat"),
        _ => crate::fl!("time-day-sun"),
    }
}

fn month(i: usize) -> String {
    match i {
        0 => crate::fl!("time-month-jan"),
        1 => crate::fl!("time-month-feb"),
        2 => crate::fl!("time-month-mar"),
        3 => crate::fl!("time-month-apr"),
        4 => crate::fl!("time-month-may"),
        5 => crate::fl!("time-month-jun"),
        6 => crate::fl!("time-month-jul"),
        7 => crate::fl!("time-month-aug"),
        8 => crate::fl!("time-month-sep"),
        9 => crate::fl!("time-month-oct"),
        10 => crate::fl!("time-month-nov"),
        _ => crate::fl!("time-month-dec"),
    }
}

/// Fecha corta en el idioma activo: "vie 6 nov" / "Fri 6 Nov".
pub fn short_date(d: chrono::NaiveDate) -> String {
    crate::fl!(
        "time-short-date",
        weekday = weekday(d.weekday().num_days_from_monday() as usize),
        day = d.day(),
        month = month(d.month0() as usize)
    )
}

/// Hora del día en el idioma activo: "17:07" en 24 h; en 12 h "5:07 PM" (en) o "5:07 p. m." (es).
pub fn fmt_hm(t: NaiveTime, h12: bool) -> String {
    if h12 {
        let (h, pm) = hour12(t.hour());
        let (h, m) = (h.to_string(), format!("{:02}", t.minute()));
        if pm {
            crate::fl!("time-12h-pm", h = h, m = m)
        } else {
            crate::fl!("time-12h-am", h = h, m = m)
        }
    } else {
        format!("{:02}:{:02}", t.hour(), t.minute())
    }
}

/// Texto de la pastilla en el idioma activo, p. ej. "17:45 · jue 29 oct" o "17:45 · Thu 29 Oct".
pub fn format_pill(dt: DateTime<Local>, h12: bool) -> String {
    crate::fl!(
        "time-pill",
        time = fmt_hm(dt.time(), h12),
        weekday = weekday(dt.weekday().num_days_from_monday() as usize),
        day = dt.day(),
        month = month(dt.month0() as usize)
    )
}

/// Solo la hora, p. ej. "17:45" (pastilla estrecha).
pub fn format_short(dt: DateTime<Local>, h12: bool) -> String {
    fmt_hm(dt.time(), h12)
}

/// Tiempo hasta el próximo minuto exacto.
pub fn next_minute_delay(now: DateTime<Local>) -> Duration {
    let elapsed = Duration::new(now.second() as u64, now.nanosecond() % 1_000_000_000);
    Duration::from_secs(60).saturating_sub(elapsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn formats_spanish_pill() {
        crate::i18n::apply_for_test("es");
        let dt = Local.with_ymd_and_hms(2026, 10, 29, 17, 45, 3).unwrap();
        assert_eq!(format_pill(dt, false), "17:45 · jue 29 oct");
    }

    #[test]
    fn pads_hour_and_handles_sunday_january() {
        crate::i18n::apply_for_test("es");
        let dt = Local.with_ymd_and_hms(2027, 1, 3, 7, 5, 0).unwrap();
        assert_eq!(format_pill(dt, false), "07:05 · dom 3 ene");
    }

    #[test]
    fn short_is_zero_padded_hh_mm() {
        let dt = Local.with_ymd_and_hms(2027, 1, 3, 7, 5, 9).unwrap();
        assert_eq!(format_short(dt, false), "07:05");
        let dt = Local.with_ymd_and_hms(2026, 10, 29, 17, 45, 3).unwrap();
        assert_eq!(format_short(dt, false), "17:45");
    }

    #[test]
    fn fmt_hm_12h_y_24h() {
        crate::i18n::apply_for_test("es");
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        assert_eq!(fmt_hm(t(0, 0), true), "12:00 a. m.");
        assert_eq!(fmt_hm(t(12, 0), true), "12:00 p. m.");
        assert_eq!(fmt_hm(t(17, 7), true), "5:07 p. m.");
        assert_eq!(fmt_hm(t(23, 59), true), "11:59 p. m.");
        assert_eq!(fmt_hm(t(9, 5), false), "09:05");
        assert_eq!(fmt_hm(t(17, 7), false), "17:07");
    }

    #[test]
    fn pastilla_12h() {
        crate::i18n::apply_for_test("es");
        let dt = Local.with_ymd_and_hms(2026, 10, 29, 17, 45, 3).unwrap();
        assert_eq!(format_pill(dt, true), "5:45 p. m. · jue 29 oct");
        assert_eq!(format_short(dt, true), "5:45 p. m.");
    }

    #[test]
    fn formats_english_pill() {
        crate::i18n::apply_for_test("en");
        let dt = Local.with_ymd_and_hms(2026, 10, 29, 17, 45, 3).unwrap();
        assert_eq!(format_pill(dt, false), "17:45 · Thu 29 Oct");
        let dt = Local.with_ymd_and_hms(2027, 1, 3, 7, 5, 0).unwrap();
        assert_eq!(format_pill(dt, false), "07:05 · Sun 3 Jan");
    }

    #[test]
    fn fmt_hm_english_12h() {
        crate::i18n::apply_for_test("en");
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        assert_eq!(fmt_hm(t(17, 7), true), "5:07 PM");
        assert_eq!(fmt_hm(t(0, 0), true), "12:00 AM");
        assert_eq!(fmt_hm(t(17, 7), false), "17:07");
        let dt = Local.with_ymd_and_hms(2026, 10, 29, 17, 45, 3).unwrap();
        assert_eq!(format_pill(dt, true), "5:45 PM · Thu 29 Oct");
    }

    #[test]
    fn delay_until_next_minute() {
        let dt = Local.with_ymd_and_hms(2026, 10, 29, 17, 45, 3).unwrap();
        assert_eq!(next_minute_delay(dt), std::time::Duration::from_secs(57));
    }
}
