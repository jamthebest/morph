use crate::lyrics::lrc::LyricLine;

/// Tiempo como m:ss (o h:mm:ss a partir de una hora).
pub fn fmt_time(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Líneas (anterior, actual, siguiente) alrededor del índice actual.
/// Con `idx = None` (antes de la primera línea) solo hay siguiente: la primera.
pub fn lyric_window(
    lines: &[LyricLine],
    idx: Option<usize>,
) -> (Option<&str>, Option<&str>, Option<&str>) {
    let text = |i: usize| lines.get(i).map(|l| l.text.as_str());
    match idx {
        None => (None, None, text(0)),
        Some(i) if i >= lines.len() => (None, None, None),
        Some(i) => (i.checked_sub(1).and_then(text), text(i), text(i + 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(t: &str) -> LyricLine {
        LyricLine {
            time_ms: 0,
            text: t.into(),
        }
    }

    #[test]
    fn fmt_time_cases() {
        assert_eq!(fmt_time(0), "0:00");
        assert_eq!(fmt_time(61_000), "1:01");
        assert_eq!(fmt_time(204_999), "3:24");
        assert_eq!(fmt_time(3_600_000), "1:00:00");
        assert_eq!(fmt_time(3_725_000), "1:02:05");
    }

    #[test]
    fn window_middle() {
        let v = [l("a"), l("b"), l("c")];
        assert_eq!(lyric_window(&v, Some(1)), (Some("a"), Some("b"), Some("c")));
    }

    #[test]
    fn window_edges() {
        let v = [l("a"), l("b")];
        assert_eq!(lyric_window(&v, Some(0)), (None, Some("a"), Some("b")));
        assert_eq!(lyric_window(&v, Some(1)), (Some("a"), Some("b"), None));
    }

    #[test]
    fn window_before_first_line() {
        let v = [l("a"), l("b")];
        assert_eq!(lyric_window(&v, None), (None, None, Some("a")));
    }

    #[test]
    fn window_empty_or_out_of_range() {
        assert_eq!(lyric_window(&[], None), (None, None, None));
        let v = [l("a")];
        assert_eq!(lyric_window(&v, Some(5)), (None, None, None));
    }
}
