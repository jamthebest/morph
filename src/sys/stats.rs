//! Uso de RAM y CPU para la vista asomada: se lee /proc solo mientras dura el hover.

use std::time::Duration;

use cosmic::iced::Subscription;
use cosmic::iced::futures::SinkExt;
use cosmic::iced::futures::channel::mpsc;
use cosmic::iced::stream;

/// Contadores agregados de la línea `cpu` de /proc/stat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuTimes {
    pub idle: u64,
    pub total: u64,
}

/// Una muestra de la vista asomada. `cpu` es None hasta tener dos lecturas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub ram: Option<u8>,
    pub cpu: Option<u8>,
}

/// % de RAM usada = (MemTotal − MemAvailable) / MemTotal.
pub fn parse_meminfo(s: &str) -> Option<u8> {
    let field = |name: &str| -> Option<u64> {
        s.lines()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix(':'))?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    };
    let total = field("MemTotal")?;
    let avail = field("MemAvailable")?.min(total);
    if total == 0 {
        return None;
    }
    let used = (total - avail) as f64 / total as f64 * 100.0;
    Some(used.round() as u8)
}

/// Línea agregada `cpu ` de /proc/stat (user nice system idle iowait irq softirq steal …).
pub fn parse_proc_stat(s: &str) -> Option<CpuTimes> {
    let line = s.lines().find(|l| l.starts_with("cpu "))?;
    let v: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(|x| x.parse().ok())
        .collect::<Option<_>>()?;
    if v.len() < 8 {
        return None;
    }
    Some(CpuTimes {
        idle: v[3].saturating_add(v[4]),
        total: v.iter().fold(0u64, |a, b| a.saturating_add(*b)),
    })
}

/// % de CPU ocupada entre dos lecturas; contadores iguales (o retrocedidos) → 0.
pub fn cpu_percent(prev: CpuTimes, now: CpuTimes) -> u8 {
    let total = now.total.saturating_sub(prev.total);
    if total == 0 {
        return 0;
    }
    let idle = now.idle.saturating_sub(prev.idle).min(total);
    ((total - idle) as f64 / total as f64 * 100.0).round() as u8
}

fn read_ram() -> Option<u8> {
    parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

fn read_cpu() -> Option<CpuTimes> {
    parse_proc_stat(&std::fs::read_to_string("/proc/stat").ok()?)
}

fn debug() -> bool {
    static D: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *D.get_or_init(|| crate::envcompat::var_os("MORPH_DEBUG").is_some())
}

/// Muestras mientras exista: una al arrancar, otra a los 200 ms (para tener delta de CPU)
/// y luego cada segundo. Se arma solo durante la vista asomada.
pub fn subscription() -> Subscription<Sample> {
    Subscription::run(|| {
        stream::channel(2, |mut out: mpsc::Sender<Sample>| async move {
            if debug() {
                eprintln!("morph: vista asomada: muestreo de CPU/RAM iniciado");
            }
            let mut prev = read_cpu();
            let _ = out
                .send(Sample {
                    ram: read_ram(),
                    cpu: None,
                })
                .await;
            let mut wait = Duration::from_millis(200);
            loop {
                tokio::time::sleep(wait).await;
                wait = Duration::from_secs(1);
                let now = read_cpu();
                let cpu = prev.zip(now).map(|(p, n)| cpu_percent(p, n));
                prev = now;
                let _ = out
                    .send(Sample {
                        ram: read_ram(),
                        cpu,
                    })
                    .await;
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meminfo_usado() {
        let s = "MemTotal:       16000000 kB\nMemFree:         1000000 kB\nMemAvailable:    4000000 kB\n";
        assert_eq!(parse_meminfo(s), Some(75));
    }

    #[test]
    fn meminfo_invalido() {
        assert_eq!(parse_meminfo(""), None);
        assert_eq!(parse_meminfo("MemTotal: 0 kB\nMemAvailable: 0 kB\n"), None);
        assert_eq!(parse_meminfo("MemTotal: 100 kB\n"), None);
        assert_eq!(parse_meminfo("MemTotal: x kB\nMemAvailable: 1 kB\n"), None);
        // Disponible > total (dato absurdo): 0 %, sin desbordar.
        assert_eq!(
            parse_meminfo("MemTotal: 100 kB\nMemAvailable: 200 kB\n"),
            Some(0)
        );
    }

    #[test]
    fn proc_stat_agregado() {
        let s = "cpu  100 10 50 800 20 5 5 10 0 0\ncpu0 50 5 25 400 10 2 3 5 0 0\n";
        let t = parse_proc_stat(s).unwrap();
        assert_eq!(t.idle, 820); // idle + iowait
        assert_eq!(t.total, 1000); // user..steal
    }

    #[test]
    fn proc_stat_invalido() {
        assert_eq!(parse_proc_stat(""), None);
        assert_eq!(parse_proc_stat("cpu0 1 2 3 4 5 6 7 8\n"), None);
        assert_eq!(parse_proc_stat("cpu  1 2 3\n"), None);
        assert_eq!(parse_proc_stat("cpu  a b c d e f g h\n"), None);
    }

    #[test]
    fn cpu_porcentaje() {
        let a = CpuTimes {
            idle: 800,
            total: 1000,
        };
        let b = CpuTimes {
            idle: 850,
            total: 1100,
        };
        assert_eq!(cpu_percent(a, b), 50);
        assert_eq!(cpu_percent(a, a), 0);
        let full = CpuTimes {
            idle: 800,
            total: 1100,
        };
        assert_eq!(cpu_percent(a, full), 100);
        // Contadores que retroceden: 0, sin pánico.
        assert_eq!(cpu_percent(b, a), 0);
    }
}
