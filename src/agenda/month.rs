//! Utilidades puras del calendario mensual (semana desde el lunes).

use chrono::{Datelike, Duration, NaiveDate};

/// Nombre del mes ("Octubre" / "October"), `i` desde 0.
fn month_name(i: usize) -> String {
    match i % 12 {
        0 => crate::fl!("cal-month-1"),
        1 => crate::fl!("cal-month-2"),
        2 => crate::fl!("cal-month-3"),
        3 => crate::fl!("cal-month-4"),
        4 => crate::fl!("cal-month-5"),
        5 => crate::fl!("cal-month-6"),
        6 => crate::fl!("cal-month-7"),
        7 => crate::fl!("cal-month-8"),
        8 => crate::fl!("cal-month-9"),
        9 => crate::fl!("cal-month-10"),
        10 => crate::fl!("cal-month-11"),
        _ => crate::fl!("cal-month-12"),
    }
}

/// Nombre del mes dentro de una fecha larga ("octubre" / "October"), `i` desde 0.
fn month_in_date(i: usize) -> String {
    match i % 12 {
        0 => crate::fl!("cal-month-of-1"),
        1 => crate::fl!("cal-month-of-2"),
        2 => crate::fl!("cal-month-of-3"),
        3 => crate::fl!("cal-month-of-4"),
        4 => crate::fl!("cal-month-of-5"),
        5 => crate::fl!("cal-month-of-6"),
        6 => crate::fl!("cal-month-of-7"),
        7 => crate::fl!("cal-month-of-8"),
        8 => crate::fl!("cal-month-of-9"),
        9 => crate::fl!("cal-month-of-10"),
        10 => crate::fl!("cal-month-of-11"),
        _ => crate::fl!("cal-month-of-12"),
    }
}

/// Nombre completo del día ("viernes" / "Friday"), `i` desde 0 = lunes.
fn weekday_name(i: usize) -> String {
    match i % 7 {
        0 => crate::fl!("cal-weekday-1"),
        1 => crate::fl!("cal-weekday-2"),
        2 => crate::fl!("cal-weekday-3"),
        3 => crate::fl!("cal-weekday-4"),
        4 => crate::fl!("cal-weekday-5"),
        5 => crate::fl!("cal-weekday-6"),
        _ => crate::fl!("cal-weekday-7"),
    }
}

/// Inicial del día para cabeceras y selector ("L" / "M"), `i` desde 0 = lunes.
pub fn week_letter(i: usize) -> String {
    match i % 7 {
        0 => crate::fl!("cal-week-letter-1"),
        1 => crate::fl!("cal-week-letter-2"),
        2 => crate::fl!("cal-week-letter-3"),
        3 => crate::fl!("cal-week-letter-4"),
        4 => crate::fl!("cal-week-letter-5"),
        5 => crate::fl!("cal-week-letter-6"),
        _ => crate::fl!("cal-week-letter-7"),
    }
}

/// Rejilla 6×7 que empieza en el lunes de la semana del día 1.
pub fn month_grid(year: i32, month: u32) -> [[NaiveDate; 7]; 6] {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("mes válido");
    let start = first - Duration::days(first.weekday().num_days_from_monday() as i64);
    std::array::from_fn(|w| std::array::from_fn(|d| start + Duration::days((w * 7 + d) as i64)))
}

pub fn shift_month(year: i32, month: u32, delta: i32) -> (i32, u32) {
    let idx = year * 12 + month as i32 - 1 + delta;
    (idx.div_euclid(12), idx.rem_euclid(12) as u32 + 1)
}

pub fn month_title(year: i32, month: u32) -> String {
    crate::fl!(
        "cal-month-title",
        month = month_name(month as usize - 1),
        year = year
    )
}

pub fn long_date(d: NaiveDate) -> String {
    crate::fl!(
        "cal-long-date",
        weekday = weekday_name(d.weekday().num_days_from_monday() as usize),
        day = d.day(),
        month = month_in_date(d.month0() as usize)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn grid_octubre_2026() {
        let g = month_grid(2026, 10);
        assert_eq!(g[0][0], d(2026, 9, 28));
        assert_eq!(g[4][4], d(2026, 10, 30));
        assert_eq!(g[5][6], d(2026, 11, 8));
    }

    #[test]
    fn grid_febrero_2027_empieza_lunes() {
        assert_eq!(month_grid(2027, 2)[0][0], d(2027, 2, 1));
    }

    #[test]
    fn shift_cruza_anio() {
        assert_eq!(shift_month(2026, 12, 1), (2027, 1));
        assert_eq!(shift_month(2026, 1, -1), (2025, 12));
        assert_eq!(shift_month(2026, 5, 0), (2026, 5));
        assert_eq!(shift_month(2026, 10, 14), (2027, 12));
    }

    #[test]
    fn titulo_y_fecha_larga() {
        crate::i18n::apply_for_test("es");
        assert_eq!(month_title(2026, 10), "Octubre 2026");
        assert_eq!(long_date(d(2026, 10, 30)), "viernes 30 de octubre");
        crate::i18n::apply_for_test("en");
        assert_eq!(month_title(2026, 10), "October 2026");
        assert_eq!(long_date(d(2026, 10, 30)), "Friday, October 30");
    }
}
