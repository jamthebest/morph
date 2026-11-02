#[derive(Clone, Debug, PartialEq)]
pub struct LyricLine {
    pub time_ms: u64,
    pub text: String,
}

/// Parsea un texto LRC. Devuelve las líneas ordenadas por tiempo.
pub fn parse_lrc(src: &str) -> Vec<LyricLine> {
    let mut raw: Vec<(u64, String)> = Vec::new();
    let mut offset: i64 = 0;

    for line in src.lines() {
        let mut rest = line.trim_start();
        let mut stamps = Vec::new();
        while let Some(body) = rest.strip_prefix('[') {
            let Some(end) = body.find(']') else { break };
            let tag = &body[..end];
            if let Some(ms) = parse_timestamp(tag) {
                stamps.push(ms);
            } else if !stamps.is_empty() {
                // Tras una marca de tiempo, los corchetes forman parte del texto.
                break;
            } else if let Some(value) = tag.strip_prefix("offset:")
                && let Ok(v) = value.trim().parse::<i64>()
            {
                offset = v;
            }
            rest = &body[end + 1..];
        }
        if stamps.is_empty() {
            continue;
        }
        let text = rest.trim();
        for ms in stamps {
            raw.push((ms, text.to_string()));
        }
    }

    let mut lines: Vec<LyricLine> = raw
        .into_iter()
        .map(|(t, text)| LyricLine {
            // i128: evita que `t as i64` envuelva para t > i64::MAX.
            time_ms: (t as i128 - offset as i128).clamp(0, u64::MAX as i128) as u64,
            text,
        })
        .collect();
    lines.sort_by_key(|l| l.time_ms);
    lines
}

/// Parsea el contenido de una marca `mm:ss`, `mm:ss.x`, `mm:ss.xx` o `mm:ss.xxx`.
fn parse_timestamp(tag: &str) -> Option<u64> {
    fn all_digits(s: &str) -> bool {
        !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
    }

    let (mm, rest) = tag.split_once(':')?;
    let (ss, frac) = match rest.split_once('.') {
        Some((ss, frac)) => (ss, Some(frac)),
        None => (rest, None),
    };
    if !all_digits(mm) || !all_digits(ss) || ss.len() > 2 {
        return None;
    }
    let minutes: u64 = mm.parse().ok()?;
    let seconds: u64 = ss.parse().ok()?;
    let millis: u64 = match frac {
        None => 0,
        Some(f) => {
            if !all_digits(f) {
                return None;
            }
            let mut digits: String = f.chars().take(3).collect();
            while digits.len() < 3 {
                digits.push('0');
            }
            digits.parse().ok()?
        }
    };
    minutes
        .checked_mul(60_000)?
        .checked_add(seconds.checked_mul(1000)?)?
        .checked_add(millis)
}

/// Índice de la última línea con `time_ms <= pos_ms`; `None` antes de la primera.
pub fn current_line(lines: &[LyricLine], pos_ms: u64) -> Option<usize> {
    lines
        .partition_point(|l| l.time_ms <= pos_ms)
        .checked_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(src: &str) -> Vec<(u64, String)> {
        parse_lrc(src)
            .into_iter()
            .map(|l| (l.time_ms, l.text))
            .collect()
    }

    fn s(t: &str) -> String {
        t.to_string()
    }

    fn line(time_ms: u64, text: &str) -> LyricLine {
        LyricLine {
            time_ms,
            text: text.to_string(),
        }
    }

    #[test]
    fn parses_basic_lines_and_sorts_by_time() {
        assert_eq!(
            pairs("[00:07.82] First things first\n[00:10.5] Hola\n"),
            vec![(7820, s("First things first")), (10500, s("Hola"))]
        );
        assert_eq!(
            pairs("[00:20.00] b\n[00:10.00] a\n"),
            vec![(10000, s("a")), (20000, s("b"))]
        );
    }

    #[test]
    fn accepts_all_fraction_precisions() {
        assert_eq!(
            pairs("[01:02]a\n[01:03.3]b\n[01:04.34]c\n[01:05.345]d\n"),
            vec![
                (62000, s("a")),
                (63300, s("b")),
                (64340, s("c")),
                (65345, s("d")),
            ]
        );
        assert_eq!(pairs("[01:02.003] x\n"), vec![(62003, s("x"))]);
    }

    #[test]
    fn expands_multiple_timestamps_for_same_text() {
        assert_eq!(
            pairs("[00:10.00][01:30.50] coro\n"),
            vec![(10000, s("coro")), (90500, s("coro"))]
        );
    }

    #[test]
    fn keeps_empty_text_lines() {
        assert_eq!(parse_lrc("[00:20.00]\n"), vec![line(20000, "")]);
    }

    #[test]
    fn ignores_metadata_unmarked_text_and_handles_crlf() {
        let src = "[ar:X]\n[ti:Y]\n[00:01.00] uno\nsin marca\n[00:02.00] dos\r\n";
        assert_eq!(pairs(src), vec![(1000, s("uno")), (2000, s("dos"))]);
    }

    #[test]
    fn offset_tag_shifts_all_lines_and_clamps_at_zero() {
        let src = "[offset:+500]\n[00:00.20] a\n[00:01.00] b\n";
        assert_eq!(pairs(src), vec![(0, s("a")), (500, s("b"))]);
    }

    #[test]
    fn negative_offset_shifts_later() {
        assert_eq!(pairs("[offset:-300]\n[00:01.00] a\n"), vec![(1300, s("a"))]);
    }

    #[test]
    fn current_line_before_first_is_none() {
        let lines = parse_lrc("[00:07.82] a\n[00:10.5] b\n");
        assert_eq!(current_line(&lines, 0), None);
    }

    #[test]
    fn current_line_at_and_after_timestamps() {
        let lines = parse_lrc("[00:07.82] a\n[00:10.5] b\n");
        assert_eq!(current_line(&lines, 7820), Some(0));
        assert_eq!(current_line(&lines, 10499), Some(0));
        assert_eq!(current_line(&lines, 10500), Some(1));
        assert_eq!(current_line(&lines, 99999), Some(1));
    }

    #[test]
    fn current_line_on_empty_input_is_none() {
        assert_eq!(current_line(&[], 5000), None);
    }

    #[test]
    fn overflowing_timestamps_are_skipped_without_panic() {
        assert_eq!(
            pairs("[999999999999999:59.999] x\n[99999999999999999999999:00] y\n[00:01.00] ok\n"),
            vec![(1000, s("ok"))]
        );
    }

    #[test]
    fn offset_math_does_not_wrap_for_huge_timestamps() {
        // 200000000000000 min = 1.2e19 ms, por encima de i64::MAX.
        assert_eq!(
            pairs("[offset:+500]\n[200000000000000:00.000] a\n"),
            vec![(11_999_999_999_999_999_500, s("a"))]
        );
        assert_eq!(
            pairs("[offset:-9223372036854775807]\n[00:01.00] a\n"),
            vec![(9_223_372_036_854_776_807, s("a"))]
        );
    }

    #[test]
    fn tags_after_a_timestamp_are_text() {
        assert_eq!(
            pairs("[00:01.00] [Chorus] hola\n"),
            vec![(1000, s("[Chorus] hola"))]
        );
        assert_eq!(
            pairs("[00:01.00][Chorus] hola\n"),
            vec![(1000, s("[Chorus] hola"))]
        );
    }

    #[test]
    fn odd_input_does_not_panic_and_text_is_verbatim() {
        assert_eq!(
            pairs("[xx:yy] a\n[00:01.00] canción ñandú\n[00:02.00] a]b\n"),
            vec![(1000, s("canción ñandú")), (2000, s("a]b"))]
        );
    }
}
