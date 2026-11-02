//! Cuentas de `gh`, repos en clones locales y entrada manual de repos (funciones puras y el
//! escaneo del disco). Nunca se lee ni se guarda ningún token.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Inicio de sesión válido para pasarlo como argumento (`--user`) y usarlo como clave.
pub fn valid_login(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Cuentas de `github.com` en `hosts.yml` (la activa primero). Solo lee nombres: las líneas
/// `oauth_token` (y cualquier otra clave anidada) se ignoran por completo.
#[cfg(test)]
pub fn parse_hosts_accounts(text: &str) -> Vec<String> {
    parse_hosts_lines(text.lines())
}

fn parse_hosts_lines<S: AsRef<str>>(lines: impl Iterator<Item = S>) -> Vec<String> {
    let mut in_host = false;
    let mut users_indent: Option<usize> = None;
    let mut child_indent: Option<usize> = None;
    let mut users: Vec<String> = Vec::new();
    let mut active: Option<String> = None;
    for raw in lines {
        let line = raw.as_ref().trim_end();
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let ind = indent(line);
        if ind == 0 {
            in_host = t.trim_end_matches(':').trim_matches(['"', '\'']) == "github.com"
                && t.ends_with(':');
            users_indent = None;
            child_indent = None;
            continue;
        }
        if !in_host {
            continue;
        }
        if let Some(u) = users_indent
            && ind > u
        {
            let ci = *child_indent.get_or_insert(ind);
            if ind == ci {
                let name = t.split(':').next().unwrap_or("").trim_matches(['"', '\'']);
                if valid_login(name) && !users.iter().any(|x| x == name) {
                    users.push(name.to_string());
                }
            }
            continue;
        }
        users_indent = None;
        if t == "users:" {
            users_indent = Some(ind);
            child_indent = None;
        } else if let Some(v) = t.strip_prefix("user:") {
            let v = v.trim().trim_matches(['"', '\'']);
            if valid_login(v) {
                active = Some(v.to_string());
            }
        }
    }
    if let Some(a) = active {
        if let Some(i) = users.iter().position(|u| *u == a) {
            let u = users.remove(i);
            users.insert(0, u);
        } else {
            users.insert(0, a);
        }
    }
    users
}

/// Cuentas de `hosts.yml` leyendo línea a línea: las líneas `oauth_token` se descartan al leer.
pub fn parse_hosts_reader<R: std::io::BufRead>(r: R) -> Vec<String> {
    let kept: Vec<String> = r
        .lines()
        .map_while(Result::ok)
        .filter(|l| !l.trim_start().starts_with("oauth_token"))
        .collect();
    parse_hosts_lines(kept.iter())
}

/// Texto de `~/.ssh/config` más los archivos de sus `Include` (un nivel, sin comodines).
pub fn expand_ssh_config(main: &str, read: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = main.to_string();
    for line in main.lines() {
        let line = line.trim();
        let Some((k, v)) = line.split_once(|c: char| c.is_whitespace() || c == '=') else {
            continue;
        };
        if !k.eq_ignore_ascii_case("include") {
            continue;
        }
        for path in v.split_whitespace() {
            let path = path.trim_matches('"');
            if path.contains(['*', '?', '[']) {
                continue;
            }
            if let Some(t) = read(path) {
                // `Match all` aísla el texto incluido del último Host del principal.
                out.push_str("\nMatch all\n");
                out.push_str(&t);
            }
        }
    }
    out
}

/// Quita del mapa los repos no elegidos y las cuentas que ya no son utilizables.
pub fn prune_access(access: &mut HashMap<String, Access>, repos: &[String], usable: &[String]) {
    access.retain(|r, a| {
        repos.contains(r)
            && match a {
                Access::Account(l) => usable.contains(l),
                Access::NoAccess => true,
            }
    });
}

/// Olvida los «sin acceso» (se vuelven a resolver).
pub fn drop_no_access(access: &mut HashMap<String, Access>) {
    access.retain(|_, a| *a != Access::NoAccess);
}

/// Cuántos de los repos elegidos están marcados «sin acceso».
pub fn count_no_access(repos: &[String], access: &HashMap<String, Access>) -> usize {
    repos
        .iter()
        .filter(|r| access.get(*r) == Some(&Access::NoAccess))
        .count()
}

/// Alias de `~/.ssh/config` cuyo `HostName` es github.com.
#[cfg(test)]
pub fn parse_ssh_aliases(text: &str) -> Vec<String> {
    parse_ssh_aliases_for(text, "github.com")
}

/// Alias de `~/.ssh/config` cuyo `HostName` es `host`.
pub fn parse_ssh_aliases_for(text: &str, host: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut hosts: Vec<String> = Vec::new();
    let flush = |hosts: &mut Vec<String>, github: bool, out: &mut Vec<String>| {
        if github {
            for h in hosts.drain(..) {
                if !out.contains(&h) {
                    out.push(h);
                }
            }
        }
        hosts.clear();
    };
    let mut github = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, val) = match line.split_once(|c: char| c.is_whitespace() || c == '=') {
            Some((k, v)) => (
                k,
                v.trim_start_matches(|c: char| c.is_whitespace() || c == '='),
            ),
            None => continue,
        };
        match key.to_ascii_lowercase().as_str() {
            "host" => {
                flush(&mut hosts, github, &mut out);
                github = false;
                hosts = val
                    .split_whitespace()
                    .filter(|h| !h.contains(['*', '?', '!']))
                    .map(str::to_string)
                    .collect();
            }
            "match" => {
                flush(&mut hosts, github, &mut out);
                github = false;
            }
            "hostname" if val.trim().eq_ignore_ascii_case(host) => github = true,
            _ => {}
        }
    }
    flush(&mut hosts, github, &mut out);
    out
}

/// `dueño/repo` a partir de un remoto de git (https, scp, ssh) de github.com o de un alias.
pub fn remote_to_repo(url: &str, aliases: &[String]) -> Option<String> {
    let url = url.trim();
    let (host, path) = if let Some((_, rest)) = url.split_once("://") {
        let (auth, path) = rest.split_once('/')?;
        let host = auth.rsplit('@').next()?;
        (host.split(':').next()?, path)
    } else {
        let rest = url.rsplit_once('@').map_or(url, |(_, r)| r);
        rest.split_once(':')?
    };
    let ok = host.eq_ignore_ascii_case("github.com")
        || host.eq_ignore_ascii_case("www.github.com")
        || host.eq_ignore_ascii_case("ssh.github.com")
        || aliases.iter().any(|a| a.eq_ignore_ascii_case(host));
    if !ok {
        return None;
    }
    normalize_repo(path)
}

/// `grupo/sub/proyecto` a partir de un remoto de git (https, scp, ssh) del servidor GitLab
/// `host` o de un alias de ssh que apunta a él.
pub fn remote_to_gitlab_project(url: &str, aliases: &[String], host: &str) -> Option<String> {
    let url = url.trim();
    let (h, path) = if let Some((_, rest)) = url.split_once("://") {
        let (auth, path) = rest.split_once('/')?;
        let h = auth.rsplit('@').next()?;
        (h.split(':').next()?, path)
    } else {
        let rest = url.rsplit_once('@').map_or(url, |(_, r)| r);
        rest.split_once(':')?
    };
    if !(h.eq_ignore_ascii_case(host) || aliases.iter().any(|a| a.eq_ignore_ascii_case(h))) {
        return None;
    }
    normalize_gitlab_path(path)
}

/// Path `grupo/sub/proyecto` válido (al menos grupo y proyecto, sin `.git` ni `/` final).
fn normalize_gitlab_path(s: &str) -> Option<String> {
    let s = s.trim().trim_end_matches('/');
    let s = s.strip_suffix(".git").unwrap_or(s);
    let parts: Vec<&str> = s.split('/').collect();
    let ok = parts.len() >= 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && *p != "."
                && *p != ".."
                && *p != "-"
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        });
    ok.then(|| parts.join("/"))
}

fn valid_repo(owner: &str, name: &str) -> bool {
    !owner.is_empty()
        && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// `dueño/repo` válido (sin `.git` ni `/` final); None si no tiene esa forma.
fn normalize_repo(s: &str) -> Option<String> {
    let s = s.trim().trim_end_matches('/');
    let s = s.strip_suffix(".git").unwrap_or(s);
    let (o, n) = s.split_once('/')?;
    valid_repo(o, n).then(|| format!("{o}/{n}"))
}

/// Entrada manual: acepta `dueño/repo`, `dueño/repo.git` o una URL de github.com pegada.
pub fn normalize_manual(s: &str) -> Option<String> {
    let s = s.trim();
    let s = ["https://", "http://"]
        .iter()
        .find_map(|p| s.strip_prefix(p))
        .unwrap_or(s);
    let s = s
        .strip_prefix("www.")
        .unwrap_or(s)
        .strip_prefix("github.com/")
        .unwrap_or(s);
    normalize_repo(s)
}

/// Remotos de GitHub en el texto de un `.git/config`.
pub fn parse_git_config_repos(text: &str, aliases: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_remote = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_remote = line.starts_with("[remote ");
            continue;
        }
        if in_remote
            && let Some((k, v)) = line.split_once('=')
            && k.trim().eq_ignore_ascii_case("url")
            && let Some(r) = remote_to_repo(v.trim(), aliases)
            && !out.contains(&r)
        {
            out.push(r);
        }
    }
    out
}

/// Remotos de GitLab (`host` o alias) en el texto de un `.git/config`.
pub fn parse_git_config_gitlab(text: &str, aliases: &[String], host: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_remote = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_remote = line.starts_with("[remote ");
            continue;
        }
        if in_remote
            && let Some((k, v)) = line.split_once('=')
            && k.trim().eq_ignore_ascii_case("url")
            && let Some(r) = remote_to_gitlab_project(v.trim(), aliases, host)
            && !out.contains(&r)
        {
            out.push(r);
        }
    }
    out
}

/// Entrada manual de GitLab: `grupo/sub/proyecto`, con `.git` o una URL de `host` pegada.
pub fn normalize_gitlab_manual(s: &str, host: &str) -> Option<String> {
    let s = s.trim();
    match s.split_once("://") {
        Some((_, rest)) => {
            let (h, path) = rest.split_once('/')?;
            h.eq_ignore_ascii_case(host)
                .then(|| normalize_gitlab_path(path))?
        }
        None => normalize_gitlab_path(s),
    }
}

/// Repo de GitHub hallado en un clon local.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalRepo {
    pub repo: String,
    /// Nombre de la carpeta del clon.
    pub dir: String,
}

const SKIP_DIRS: [&str; 3] = ["node_modules", "target", "snap"];
const MAX_DEPTH: usize = 5;
const MAX_VISITED: usize = 2000;
const GLOBAL_CAP: usize = 20_000;

fn same_dev(e: &std::fs::DirEntry, dev: Option<u64>) -> bool {
    use std::os::unix::fs::MetadataExt;
    dev.is_none_or(|d| e.metadata().is_ok_and(|m| m.dev() == d))
}

/// `global` cuenta las carpetas visitadas en todo el escaneo (tope `cap`); `dev` = solo ese
/// sistema de archivos.
fn scan_tree(
    top: &Path,
    extract: &dyn Fn(&str) -> Vec<String>,
    (max_depth, budget): (usize, usize),
    (global, cap): (&mut usize, usize),
    dev: Option<u64>,
    out: &mut Vec<LocalRepo>,
) {
    let mut stack: Vec<(PathBuf, usize)> = vec![(top.to_path_buf(), 1)];
    let mut visited = 0usize;
    while let Some((dir, depth)) = stack.pop() {
        if visited >= budget || *global >= cap {
            return;
        }
        visited += 1;
        *global += 1;
        let git = dir.join(".git");
        if let Ok(md) = std::fs::symlink_metadata(&git) {
            if md.is_dir()
                && let Ok(cfg) = std::fs::read_to_string(git.join("config"))
            {
                let name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                for repo in extract(&cfg) {
                    out.push(LocalRepo {
                        repo,
                        dir: name.clone(),
                    });
                }
            }
            // `.git` como archivo (worktree o submódulo): se omite; un clon no se recorre.
            continue;
        }
        if depth >= max_depth {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut subs: Vec<PathBuf> = rd
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter(|e| {
                let n = e.file_name();
                let n = n.to_string_lossy();
                !n.starts_with('.') && !SKIP_DIRS.contains(&&*n)
            })
            .filter(|e| same_dev(e, dev))
            .map(|e| e.path())
            .collect();
        subs.sort();
        // La pila saca el último: se invierte para visitar en orden alfabético.
        for s in subs.into_iter().rev() {
            stack.push((s, depth + 1));
        }
    }
}

/// Repos de GitHub en clones bajo `root`, hasta profundidad 5, con tope por carpeta de primer nivel.
pub fn scan_local_repos_in(
    root: &Path,
    aliases: &[String],
    max_depth: usize,
    budget: usize,
    global_cap: usize,
) -> Vec<LocalRepo> {
    let extract = |cfg: &str| parse_git_config_repos(cfg, aliases);
    scan_local_with(root, &extract, max_depth, budget, global_cap)
}

fn scan_local_with(
    root: &Path,
    extract: &dyn Fn(&str) -> Vec<String>,
    max_depth: usize,
    budget: usize,
    global_cap: usize,
) -> Vec<LocalRepo> {
    use std::os::unix::fs::MetadataExt;
    let mut out = Vec::new();
    let dev = std::fs::metadata(root).ok().map(|m| m.dev());
    let mut global = 0usize;
    // El propio `root` (p. ej. `$HOME` versionado) también cuenta.
    scan_tree(root, extract, (1, 1), (&mut 0, 1), dev, &mut out);
    let Ok(rd) = std::fs::read_dir(root) else {
        return out;
    };
    let mut tops: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            !n.starts_with('.') && !SKIP_DIRS.contains(&&*n)
        })
        .filter(|e| same_dev(e, dev))
        .map(|e| e.path())
        .collect();
    tops.sort();
    for t in tops {
        scan_tree(
            &t,
            extract,
            (max_depth, budget),
            (&mut global, global_cap),
            dev,
            &mut out,
        );
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|l| seen.insert(l.repo.clone()));
    out
}

/// Alias de `~/.ssh/config` (con sus `Include`) cuyo `HostName` es `host`.
fn home_aliases(home: &Path, host: &str) -> Vec<String> {
    let ssh_dir = home.join(".ssh");
    let ssh = std::fs::read_to_string(ssh_dir.join("config")).unwrap_or_default();
    let read = |p: &str| {
        let path = Path::new(p);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            ssh_dir.join(path)
        };
        std::fs::read_to_string(path).ok()
    };
    parse_ssh_aliases_for(&expand_ssh_config(&ssh, &read), host)
}

/// Escanea `$HOME` (para ejecutar en un hilo aparte).
pub fn scan_local_repos(home: &Path) -> Vec<LocalRepo> {
    let aliases = home_aliases(home, "github.com");
    scan_local_repos_in(home, &aliases, MAX_DEPTH, MAX_VISITED, GLOBAL_CAP)
}

/// Proyectos de GitLab (`host`) en clones bajo `$HOME` (hilo aparte).
pub fn scan_local_gitlab(home: &Path, host: &str) -> Vec<LocalRepo> {
    let aliases = home_aliases(home, host);
    let extract = |cfg: &str| parse_git_config_gitlab(cfg, &aliases, host);
    scan_local_with(home, &extract, MAX_DEPTH, MAX_VISITED, GLOBAL_CAP)
}

// ---- Cuenta de cada repo ----

/// Quién ve un repo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Access {
    Account(String),
    NoAccess,
}

/// Repos agrupados por la cuenta que los ve (orden de aparición); los `NoAccess` y los aún
/// sin resolver quedan fuera.
pub fn group_by_account(
    repos: &[String],
    access: &HashMap<String, Access>,
) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for r in repos {
        let Some(Access::Account(a)) = access.get(r) else {
            continue;
        };
        match out.iter_mut().find(|(l, _)| l == a) {
            Some((_, v)) => v.push(r.clone()),
            None => out.push((a.clone(), vec![r.clone()])),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOSTS: &str = "github.com:
    users:
        bob:
            oauth_token: gho_SECRETO1
        alice-work:
            oauth_token: gho_SECRETO2
    git_protocol: ssh
    user: alice-work
    oauth_token: gho_SECRETO3
gitlab.example.com:
    users:
        otro:
    user: otro
";

    #[test]
    fn hosts_multiple_users_active_first() {
        let a = parse_hosts_accounts(HOSTS);
        assert_eq!(a, vec!["alice-work", "bob"]);
        assert!(!format!("{a:?}").contains("SECRETO"));
    }

    #[test]
    fn hosts_old_format_and_empty() {
        let old = "github.com:\n    oauth_token: gho_X\n    user: solo\n    git_protocol: https\n";
        assert_eq!(parse_hosts_accounts(old), vec!["solo"]);
        assert!(parse_hosts_accounts("").is_empty());
        assert!(parse_hosts_accounts("gitlab.com:\n    user: x\n").is_empty());
    }

    #[test]
    fn hosts_rejects_odd_logins() {
        let t = "github.com:\n    users:\n        -rf:\n        ok_1:\n    user: -rf\n";
        assert_eq!(parse_hosts_accounts(t), vec!["ok_1"]);
    }

    #[test]
    fn ssh_aliases() {
        let t = "Host github-personal gh2\n  HostName github.com\n  User git\n\n\
                 host work *.corp\n  hostname GitHub.com\n\nHost gl\n  HostName gitlab.com\n\n\
                 Host *\n  ServerAliveInterval 5\nHost last\n  HostName=github.com\n";
        assert_eq!(
            parse_ssh_aliases(t),
            vec!["github-personal", "gh2", "work", "last"]
        );
        assert!(parse_ssh_aliases("").is_empty());
    }

    #[test]
    fn remote_forms() {
        let al = vec!["github-personal".to_string()];
        let r = |u| remote_to_repo(u, &al);
        assert_eq!(r("https://github.com/o/r.git").as_deref(), Some("o/r"));
        assert_eq!(r("https://github.com/o/r").as_deref(), Some("o/r"));
        assert_eq!(r("https://tok@github.com/o/r.git/").as_deref(), Some("o/r"));
        assert_eq!(r("git@github.com:o/r.git").as_deref(), Some("o/r"));
        assert_eq!(r("git@github.com:o/r").as_deref(), Some("o/r"));
        assert_eq!(r("ssh://git@github.com/o/r.git").as_deref(), Some("o/r"));
        assert_eq!(r("ssh://git@github.com:22/o/r").as_deref(), Some("o/r"));
        assert_eq!(
            r("git@github-personal:me/rep.git").as_deref(),
            Some("me/rep")
        );
        assert_eq!(
            r("ssh://git@github-personal/me/rep").as_deref(),
            Some("me/rep")
        );
        assert_eq!(r("git@gitlab.com:o/r.git"), None);
        assert_eq!(r("https://gitlab.com/o/r.git"), None);
        assert_eq!(r("git@github.com:o/r/extra"), None);
        assert_eq!(r("/local/path"), None);
        assert_eq!(r(""), None);
    }

    #[test]
    fn git_config_remotes() {
        let al = vec!["gp".to_string()];
        let cfg = "[core]\n\turl = x\n[remote \"origin\"]\n\turl = git@gp:a/b.git\n\tfetch = +refs/*\n\
                   [remote \"up\"]\n\turl = https://github.com/c/d\n[remote \"gl\"]\n\turl = git@gitlab.com:e/f.git\n\
                   [branch \"main\"]\n\turl = https://github.com/z/z\n";
        assert_eq!(parse_git_config_repos(cfg, &al), vec!["a/b", "c/d"]);
    }

    #[test]
    fn manual_entry() {
        let n = normalize_manual;
        assert_eq!(n("o/r").as_deref(), Some("o/r"));
        assert_eq!(n("  o/r.git ").as_deref(), Some("o/r"));
        assert_eq!(n("https://github.com/o/r.git").as_deref(), Some("o/r"));
        assert_eq!(n("github.com/o/r/").as_deref(), Some("o/r"));
        assert_eq!(n("My-Org/a.b_c-d").as_deref(), Some("My-Org/a.b_c-d"));
        assert_eq!(n(""), None);
        assert_eq!(n("solo"), None);
        assert_eq!(n("a/b/c"), None);
        assert_eq!(n("a b/c"), None);
        assert_eq!(n("o_x/r"), None);
        assert_eq!(n("o/.."), None);
        assert_eq!(n("o/"), None);
        assert_eq!(n("/r"), None);
    }

    #[test]
    fn grouping_excludes_no_access_and_unresolved() {
        let repos: Vec<String> = ["a/1", "b/2", "a/3", "c/4", "d/5"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mut acc = HashMap::new();
        acc.insert("a/1".into(), Access::Account("yo".into()));
        acc.insert("b/2".into(), Access::Account("otro".into()));
        acc.insert("a/3".into(), Access::Account("yo".into()));
        acc.insert("c/4".into(), Access::NoAccess);
        let g = group_by_account(&repos, &acc);
        assert_eq!(
            g,
            vec![
                ("yo".to_string(), vec!["a/1".to_string(), "a/3".to_string()]),
                ("otro".to_string(), vec!["b/2".to_string()]),
            ]
        );
    }

    #[test]
    fn scans_local_clones() {
        let root = std::env::temp_dir().join(format!("morph-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mk = |p: &str, cfg: Option<&str>| {
            let d = root.join(p);
            std::fs::create_dir_all(d.join(".git")).unwrap();
            if let Some(c) = cfg {
                std::fs::write(d.join(".git/config"), c).unwrap();
            }
        };
        mk(
            "work/api",
            Some("[remote \"origin\"]\n url = git@gp:me/api.git\n"),
        );
        mk(
            "work/api-dup",
            Some("[remote \"origin\"]\n url = https://github.com/me/api\n"),
        );
        mk(
            "a/b/c/web",
            Some("[remote \"origin\"]\n url = https://github.com/o/web.git\n"),
        );
        mk(
            "gl/proj",
            Some("[remote \"origin\"]\n url = git@gitlab.com:o/p.git\n"),
        );
        mk(
            "node_modules/x",
            Some("[remote \"origin\"]\n url = https://github.com/n/m\n"),
        );
        mk(
            ".hidden/h",
            Some("[remote \"origin\"]\n url = https://github.com/h/h\n"),
        );
        mk(
            "deep/1/2/3/4/5/too",
            Some("[remote \"origin\"]\n url = https://github.com/t/t\n"),
        );
        std::fs::create_dir_all(root.join("wt")).unwrap();
        std::fs::write(root.join("wt/.git"), "gitdir: /x").unwrap();
        let al = vec!["gp".to_string()];
        let found = scan_local_repos_in(&root, &al, 5, 2000, 20_000);
        let names: Vec<&str> = found.iter().map(|l| l.repo.as_str()).collect();
        assert_eq!(names, vec!["o/web", "me/api"]);
        assert_eq!(found[1].dir, "api");
        // El tope de carpetas visitadas corta el recorrido.
        assert!(scan_local_repos_in(&root, &al, 5, 1, 20_000).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn hosts_reader_discards_token_lines() {
        let r = std::io::Cursor::new(HOSTS.as_bytes());
        assert_eq!(parse_hosts_reader(r), vec!["alice-work", "bob"]);
        // Una clave que empieza como un token pero está anidada como usuario no cuenta.
        let t = "github.com:\n    users:\n        a:\n            oauth_token: x\n    user: a\n";
        assert_eq!(parse_hosts_reader(std::io::Cursor::new(t)), vec!["a"]);
    }

    #[test]
    fn ssh_host_case_and_ssh_github_com() {
        let al = vec!["GitHub-Personal".to_string()];
        assert_eq!(
            remote_to_repo("git@github-personal:me/r.git", &al).as_deref(),
            Some("me/r")
        );
        assert_eq!(
            remote_to_repo("ssh://git@ssh.github.com:443/o/r.git", &[]).as_deref(),
            Some("o/r")
        );
        assert_eq!(
            remote_to_repo("git@SSH.GitHub.com:o/r", &[]).as_deref(),
            Some("o/r")
        );
    }

    #[test]
    fn ssh_include_one_level() {
        let main = "Include extra.conf\nInclude globby/*\nHost a\n  HostName github.com\n";
        let read = |p: &str| match p {
            "extra.conf" => {
                Some("Host b\n  HostName github.com\nInclude nested.conf\n".to_string())
            }
            "nested.conf" => Some("Host nested\n  HostName github.com\n".to_string()),
            _ => None,
        };
        let text = expand_ssh_config(main, &read);
        assert_eq!(parse_ssh_aliases(&text), vec!["a", "b"]);
        // El texto incluido no contagia al último Host del principal.
        let main2 = "Host x\n  HostName gitlab.com\nInclude i.conf\n";
        let read2 = |_: &str| Some("  HostName github.com\n".to_string());
        assert!(parse_ssh_aliases(&expand_ssh_config(main2, &read2)).is_empty());
    }

    #[test]
    fn prune_access_drops_unusable_accounts() {
        let mut acc = HashMap::new();
        acc.insert("a/1".to_string(), Access::Account("vivo".into()));
        acc.insert("a/2".to_string(), Access::Account("muerto".into()));
        acc.insert("a/3".to_string(), Access::NoAccess);
        acc.insert("a/4".to_string(), Access::Account("vivo".into()));
        let repos: Vec<String> = ["a/1", "a/2", "a/3"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        prune_access(&mut acc, &repos, &["vivo".to_string()]);
        let mut keys: Vec<&String> = acc.keys().collect();
        keys.sort();
        assert_eq!(keys, ["a/1", "a/3"]);
    }

    #[test]
    fn no_access_helpers() {
        let mut acc = HashMap::new();
        acc.insert("a/1".to_string(), Access::NoAccess);
        acc.insert("a/2".to_string(), Access::Account("x".into()));
        acc.insert("a/9".to_string(), Access::NoAccess);
        let repos: Vec<String> = ["a/1", "a/2", "a/3"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(count_no_access(&repos, &acc), 1);
        drop_no_access(&mut acc);
        assert_eq!(acc.len(), 1);
    }

    #[test]
    fn scan_has_a_global_cap() {
        let root = std::env::temp_dir().join(format!("morph-cap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for t in ["a", "b", "c"] {
            let d = root.join(t).join("repo");
            std::fs::create_dir_all(d.join(".git")).unwrap();
            std::fs::write(
                d.join(".git/config"),
                format!("[remote \"o\"]\n url = https://github.com/o/{t}\n"),
            )
            .unwrap();
        }
        // a visita 2 dirs (a y a/repo); con tope global 3 solo cabe un repo más de b.
        let n = scan_local_repos_in(&root, &[], 5, 2000, 20_000).len();
        assert_eq!(n, 3);
        let n = scan_local_repos_in(&root, &[], 5, 2000, 3).len();
        assert!(n < 3, "{n}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn gitlab_remotes() {
        let al = vec!["gl-work".to_string()];
        let r = |u: &str| remote_to_gitlab_project(u, &al, "gitlab.com");
        assert_eq!(
            r("git@gitlab.com:grupo/sub/proyecto.git").as_deref(),
            Some("grupo/sub/proyecto")
        );
        assert_eq!(
            r("https://gitlab.com/grupo/proyecto.git").as_deref(),
            Some("grupo/proyecto")
        );
        assert_eq!(
            r("https://user@GitLab.com/a/b/c/d/").as_deref(),
            Some("a/b/c/d")
        );
        assert_eq!(
            r("ssh://git@gitlab.com:2222/g/s/p").as_deref(),
            Some("g/s/p")
        );
        assert_eq!(r("git@gl-work:g/s/p.git").as_deref(), Some("g/s/p"));
        assert_eq!(r("ssh://git@gl-work/g/p").as_deref(), Some("g/p"));
        // Otros servidores, sin grupo o rutas raras: nada.
        assert_eq!(r("git@github.com:o/r.git"), None);
        assert_eq!(r("git@gitlab.com:solo.git"), None);
        assert_eq!(r("https://gitlab.com/a/../b"), None);
        assert_eq!(r("https://gitlab.com/a//b"), None);
        assert_eq!(r(""), None);
        assert_eq!(
            remote_to_gitlab_project("git@git.empresa.io:eq/app.git", &[], "git.empresa.io")
                .as_deref(),
            Some("eq/app")
        );
        assert_eq!(
            remote_to_gitlab_project("git@gitlab.com:a/b", &[], "git.empresa.io"),
            None
        );
    }

    #[test]
    fn ssh_aliases_for_gitlab_host() {
        let t = "Host gl-work\n  HostName gitlab.com\nHost gh\n  HostName github.com\n";
        assert_eq!(parse_ssh_aliases_for(t, "gitlab.com"), vec!["gl-work"]);
        assert_eq!(parse_ssh_aliases(t), vec!["gh"]);
    }

    #[test]
    fn git_config_gitlab_remotes() {
        let cfg = "[remote \"origin\"]\n\turl = git@gitlab.com:g/s/p.git\n[remote \"gh\"]\n\turl = git@github.com:o/r.git\n[remote \"x\"]\n\turl = git@gl-work:g/s/p.git\n[branch \"m\"]\n\turl = git@gitlab.com:no/no\n[remote \"y\"]\n\turl = https://gitlab.com/a/b\n";
        let al = vec!["gl-work".to_string()];
        assert_eq!(
            parse_git_config_gitlab(cfg, &al, "gitlab.com"),
            vec!["g/s/p", "a/b"]
        );
    }

    #[test]
    fn gitlab_manual_entry() {
        let n = |s: &str| normalize_gitlab_manual(s, "gitlab.com");
        assert_eq!(n("g/s/p").as_deref(), Some("g/s/p"));
        assert_eq!(n(" g/p.git ").as_deref(), Some("g/p"));
        assert_eq!(n("https://gitlab.com/g/s/p").as_deref(), Some("g/s/p"));
        assert_eq!(n("https://gitlab.com/g/s/p/-/merge_requests/3"), None);
        assert_eq!(n("solo"), None);
        assert_eq!(n("a/../b"), None);
        assert_eq!(n("https://github.com/o/r"), None);
        assert_eq!(
            normalize_gitlab_manual("https://git.x.io/a/b", "git.x.io").as_deref(),
            Some("a/b")
        );
    }
}
