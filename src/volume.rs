use std::time::Duration;
use tokio::process::Command;

const SINK: &str = "@DEFAULT_AUDIO_SINK@";
const TIMEOUT: Duration = Duration::from_secs(2);

/// "Volume: 0.45" o "Volume: 0.45 [MUTED]" -> (volumen, silenciado).
pub fn parse_get_volume(stdout: &str) -> Option<(f32, bool)> {
    let rest = stdout.trim().strip_prefix("Volume:")?;
    let mut parts = rest.split_whitespace();
    let v: f32 = parts.next()?.parse().ok()?;
    if !v.is_finite() || v < 0.0 {
        return None;
    }
    Some((v.min(1.5), parts.any(|p| p == "[MUTED]")))
}

fn debug() -> bool {
    static D: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *D.get_or_init(|| crate::envcompat::var_os("MORPH_DEBUG").is_some())
}

async fn wpctl(args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("wpctl");
    cmd.args(args).kill_on_drop(true);
    let out = match tokio::time::timeout(TIMEOUT, cmd.output()).await {
        Ok(Ok(o)) if o.status.success() => o,
        other => {
            if debug() {
                eprintln!("morph: wpctl {args:?} falló: {other:?}");
            }
            return None;
        }
    };
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub async fn get() -> Option<(f32, bool)> {
    parse_get_volume(&wpctl(&["get-volume", SINK]).await?)
}

pub async fn set(v: f32) {
    let v = if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    };
    wpctl(&["set-volume", SINK, &format!("{v:.2}")]).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_normal() {
        assert_eq!(parse_get_volume("Volume: 0.45\n"), Some((0.45, false)));
    }

    #[test]
    fn parses_muted() {
        assert_eq!(
            parse_get_volume("Volume: 0.45 [MUTED]\n"),
            Some((0.45, true))
        );
    }

    #[test]
    fn parses_full_and_boost() {
        assert_eq!(parse_get_volume("Volume: 1.00"), Some((1.0, false)));
        assert_eq!(parse_get_volume("Volume: 1.50"), Some((1.5, false)));
    }

    #[test]
    fn garbage_is_none() {
        assert_eq!(parse_get_volume(""), None);
        assert_eq!(parse_get_volume("hola"), None);
        assert_eq!(parse_get_volume("Volume: abc"), None);
        assert_eq!(parse_get_volume("Volume: -1"), None);
    }
}
