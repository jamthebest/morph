//! Servidores locales: sockets TCP en escucha (`/proc/net/tcp{,6}`) unidos a su proceso.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listener {
    pub port: u16,
    pub pid: u32,
    /// Instante de inicio del proceso (campo 22 de `/proc/<pid>/stat`): distingue un pid reutilizado.
    pub start: u64,
    /// Nombre del proceso (`comm`).
    pub name: String,
    pub cwd: Option<PathBuf>,
}

/// Sockets en estado LISTEN (0A) de `/proc/net/tcp` (`v6 = false`) o `tcp6`:
/// (puerto, inodo, uid del dueño). Ignora la cabecera y las líneas mal formadas.
pub fn parse_proc_net_tcp(text: &str, v6: bool) -> Vec<(u16, u64, u32)> {
    let addr_len = if v6 { 32 } else { 8 };
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 10 || f[3] != "0A" {
                return None;
            }
            let (addr, port) = f[1].split_once(':')?;
            if addr.len() != addr_len {
                return None;
            }
            let port = u16::from_str_radix(port, 16).ok()?;
            let uid = f[7].parse().ok()?;
            let inode = f[9].parse().ok()?;
            Some((port, inode, uid))
        })
        .collect()
}

/// Quita duplicados por (puerto, pid) (v4 y v6 del mismo proceso) y ordena por puerto.
fn merge(mut v: Vec<Listener>) -> Vec<Listener> {
    v.sort_by_key(|a| (a.port, a.pid));
    v.dedup_by(|a, b| a.port == b.port && a.pid == b.pid);
    v
}

/// El administrador de sesión (`systemd`) tiene sockets activados para otros: no es su dueño.
fn is_session_manager(comm: &str) -> bool {
    comm == "systemd"
}

/// Con un socket compartido (varios procesos) se elige siempre el pid más bajo.
fn record_owner(map: &mut HashMap<u64, u32>, inode: u64, pid: u32) {
    map.entry(inode)
        .and_modify(|p| *p = (*p).min(pid))
        .or_insert(pid);
}

/// Campo 22 (`starttime`) de `/proc/<pid>/stat`; `comm` puede traer espacios y paréntesis,
/// así que se cuenta desde el último `)`.
fn parse_starttime(stat: &str) -> Option<u64> {
    let rest = &stat[stat.rfind(')')? + 1..];
    // Tras `)` el primer campo es el 3 (estado): el 22 es el índice 19.
    rest.split_whitespace().nth(19)?.parse().ok()
}

fn start_time(pid: u32) -> Option<u64> {
    parse_starttime(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

/// El pid sigue siendo el proceso que se listó (mismo instante de inicio).
fn same_process(pid: u32, start: u64) -> bool {
    start_time(pid) == Some(start)
}

/// inodo de socket → pid, recorriendo `/proc/*/fd`. Lo ilegible se omite sin avisar.
fn socket_owners() -> HashMap<u64, u32> {
    let mut map = HashMap::new();
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return map;
    };
    for e in dir.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        if std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .is_ok_and(|c| is_session_manager(c.trim()))
        {
            continue;
        }
        let Ok(fds) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
            continue;
        };
        for fd in fds.flatten() {
            let Ok(link) = std::fs::read_link(fd.path()) else {
                continue;
            };
            let inode = link
                .to_str()
                .and_then(|s| s.strip_prefix("socket:["))
                .and_then(|s| s.strip_suffix(']'))
                .and_then(|s| s.parse::<u64>().ok());
            if let Some(i) = inode {
                record_owner(&mut map, i, pid);
            }
        }
    }
    map
}

/// Servidores TCP en escucha de `uid` (excluye el propio proceso), por puerto.
pub fn scan(uid: u32) -> Vec<Listener> {
    let mut socks = Vec::new();
    for (path, v6) in [("/proc/net/tcp", false), ("/proc/net/tcp6", true)] {
        if let Ok(text) = std::fs::read_to_string(path) {
            socks.extend(parse_proc_net_tcp(&text, v6));
        }
    }
    socks.retain(|s| s.2 == uid);
    if socks.is_empty() {
        return Vec::new();
    }
    let owners = socket_owners();
    let me = std::process::id();
    let out = socks
        .into_iter()
        .filter_map(|(port, inode, _)| {
            let pid = *owners.get(&inode)?;
            if pid == me {
                return None;
            }
            if uid_of(pid) != Some(uid) {
                return None;
            }
            let start = start_time(pid)?;
            let name = std::fs::read_to_string(format!("/proc/{pid}/comm"))
                .ok()?
                .trim()
                .to_string();
            let cwd = std::fs::read_link(format!("/proc/{pid}/cwd")).ok();
            Some(Listener {
                port,
                pid,
                start,
                name,
                cwd,
            })
        })
        .collect();
    merge(out)
}

/// Ruta de trabajo con `~` por el home ("~/Code/web-app").
pub fn cwd_label(p: &Path, home: &Path) -> String {
    match p.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

/// uid real de la isla.
pub fn my_uid() -> u32 {
    // SAFETY: `getuid` no tiene precondiciones ni puede fallar.
    unsafe { libc::getuid() }
}

fn uid_of(pid: u32) -> Option<u32> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let line = s.lines().find(|l| l.starts_with("Uid:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

/// El proceso existe y es de `uid`.
pub fn can_stop(pid: u32, uid: u32) -> bool {
    uid_of(pid) == Some(uid)
}

/// Pide al proceso que termine (SIGTERM, nunca SIGKILL).
/// `start` es el instante de inicio listado: si cambió, el pid lo reutiliza otro proceso.
pub fn stop(pid: u32, start: u64) -> Result<(), String> {
    let pid = i32::try_from(pid).map_err(|_| crate::fl!("dev-ports-bad-pid"))?;
    if pid <= 1 {
        return Err(crate::fl!("dev-ports-bad-pid"));
    }
    if !same_process(pid as u32, start) {
        return Err(crate::fl!("dev-ports-changed"));
    }
    // SAFETY: `kill` con un pid válido y una señal estándar.
    if unsafe { libc::kill(pid, libc::SIGTERM) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V4: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:0BB8 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 12345 1 0000000000000000 100 0 0 10 0\n   1: 0100007F:1F90 0100007F:D2A0 01 00000000:00000000 00:00000000 00000000  1000        0 999 1 0000000000000000 100 0 0 10 0\n";
    const V6: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 00000000000000000000000001000000:1435 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 67890 1 0000000000000000 100 0 0 10 0\n   1: 00000000000000000000000001000000:0050 00000000000000000000000000000000:0000 06 00000000:00000000 00:00000000 00000000     0        0 1 1 0000000000000000 100 0 0 10 0\n";

    #[test]
    fn parses_v4_listen_only() {
        assert_eq!(parse_proc_net_tcp(V4, false), vec![(3000, 12345, 1000)]);
    }

    #[test]
    fn parses_v6_listen_only() {
        assert_eq!(parse_proc_net_tcp(V6, true), vec![(5173, 67890, 1000)]);
    }

    #[test]
    fn wrong_family_is_ignored() {
        assert!(parse_proc_net_tcp(V4, true).is_empty());
        assert!(parse_proc_net_tcp("", false).is_empty());
    }

    #[test]
    fn cwd_label_uses_tilde() {
        let home = Path::new("/home/jam");
        assert_eq!(
            cwd_label(Path::new("/home/jam/Code/web-app"), home),
            "~/Code/web-app"
        );
        assert_eq!(cwd_label(home, home), "~");
        assert_eq!(cwd_label(Path::new("/srv/x"), home), "/srv/x");
        assert_eq!(cwd_label(Path::new("/home/jamie/x"), home), "/home/jamie/x");
    }

    #[test]
    fn starttime_is_field_22_even_with_odd_comm() {
        let tail = "S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 987654 20 21";
        let plain = format!("42 (node) {tail}");
        assert_eq!(parse_starttime(&plain), Some(987654));
        let odd = format!("42 (a b) c) {tail}");
        assert_eq!(parse_starttime(&odd), Some(987654));
        assert_eq!(parse_starttime("42 (x) S 1 2"), None);
        assert_eq!(parse_starttime(""), None);
    }

    #[test]
    fn stop_refuses_when_start_time_changed() {
        let me = std::process::id();
        let real = start_time(me).unwrap();
        assert!(!same_process(me, real + 1));
        assert!(same_process(me, real));
    }

    #[test]
    fn session_manager_is_not_an_owner() {
        assert!(is_session_manager("systemd"));
        assert!(!is_session_manager("systemd-resolve"));
        assert!(!is_session_manager("node"));
    }

    #[test]
    fn shared_inode_picks_lowest_pid() {
        let mut m = HashMap::new();
        record_owner(&mut m, 7, 300);
        record_owner(&mut m, 7, 100);
        record_owner(&mut m, 7, 200);
        assert_eq!(m[&7], 100);
    }

    fn l(port: u16, pid: u32) -> Listener {
        Listener {
            port,
            pid,
            start: 0,
            name: "n".into(),
            cwd: None,
        }
    }

    #[test]
    fn merge_dedupes_and_sorts() {
        let v = merge(vec![l(8080, 2), l(3000, 1), l(8080, 2), l(8080, 3)]);
        assert_eq!(v, vec![l(3000, 1), l(8080, 2), l(8080, 3)]);
    }

    #[test]
    fn can_stop_checks_existence_and_owner() {
        let me = std::process::id();
        assert!(can_stop(me, my_uid()));
        assert!(!can_stop(me, my_uid() + 12345));
        assert!(!can_stop(u32::MAX - 1, my_uid()));
    }

    #[test]
    fn stop_rejects_reserved_pids() {
        assert!(stop(0, 0).is_err());
        assert!(stop(1, 0).is_err());
    }
}
