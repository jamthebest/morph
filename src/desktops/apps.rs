//! Resolución de `app_id` a apps instaladas (nombre, ícono, Exec) leyendo archivos `.desktop`.
//!
//! Parser mínimo propio (sin dependencias nuevas): solo el grupo `[Desktop Entry]` y las
//! claves Name/Icon/Exec/StartupWMClass/NoDisplay/Hidden/Type.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppInfo {
    /// Id del .desktop: nombre del archivo sin `.desktop`.
    pub app_id: String,
    pub name: String,
    pub icon: Option<String>,
    pub exec: Option<String>,
    /// `StartupWMClass` del .desktop.
    pub wm_class: Option<String>,
    /// `Terminal=true`: necesita una terminal (no se lanza todavía).
    pub terminal: bool,
    /// `Path=`: directorio de trabajo.
    pub path: Option<String>,
}

/// Índice app_id → app (claves en minúsculas; el id del .desktop gana sobre `StartupWMClass`).
#[derive(Default)]
pub struct AppIndex {
    list: Vec<AppInfo>,
    map: std::collections::HashMap<String, usize>,
}

impl AppIndex {
    pub fn new(list: Vec<AppInfo>) -> Self {
        let mut map = std::collections::HashMap::new();
        for (i, a) in list.iter().enumerate() {
            map.entry(a.app_id.to_lowercase()).or_insert(i);
        }
        for (i, a) in list.iter().enumerate() {
            if let Some(c) = &a.wm_class {
                map.entry(c.to_lowercase()).or_insert(i);
            }
        }
        Self { list, map }
    }

    pub fn get(&self, app_id: &str) -> Option<&AppInfo> {
        if app_id.is_empty() {
            return None;
        }
        self.map.get(&app_id.to_lowercase()).map(|&i| &self.list[i])
    }
}

/// Lee el grupo `[Desktop Entry]`. `None` si falta Name, es `Hidden=true` o no es
/// `Type=Application`. `NoDisplay` no descarta: la app puede tener ventanas.
pub fn parse_desktop(id: &str, text: &str) -> Option<AppInfo> {
    parse_desktop_lang(id, text, std::env::var("LANG").ok().as_deref())
}

/// Como `parse_desktop`, con `lang` (valor de `LANG`) para elegir `Name[es_XX]`.
fn parse_desktop_lang(id: &str, text: &str, lang: Option<&str>) -> Option<AppInfo> {
    let (mut name, mut icon, mut exec, mut wm_class) = (None, None, None, None);
    let (mut name_es, mut name_es_var): (Option<String>, Vec<(String, String)>) =
        (None, Vec::new());
    let (mut hidden, mut kind_ok) = (false, true);
    let (mut terminal, mut path) = (false, None);
    let mut in_entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim();
        match k.trim() {
            "Name" => name = Some(v.to_string()),
            "Icon" => icon = Some(v.to_string()),
            "Exec" => exec = Some(v.to_string()),
            "Name[es]" => name_es = Some(v.to_string()),
            k if k.starts_with("Name[es_") && k.ends_with(']') => {
                name_es_var.push((k[5..k.len() - 1].to_string(), v.to_string()));
            }
            "StartupWMClass" => wm_class = Some(v.to_string()),
            "Terminal" => terminal = v == "true",
            "Path" => path = Some(v.to_string()),
            "Hidden" => hidden = v == "true",
            "Type" => kind_ok = v == "Application",
            _ => {}
        }
    }
    let non_empty = |o: Option<String>| o.filter(|s| !s.is_empty());
    // Español: `Name[es]`, luego `Name[es_XX]` que coincida con LANG, luego cualquier `es_XX`.
    let want = lang.and_then(|l| l.split(['.', '@']).next());
    let es_var = name_es_var
        .iter()
        .find(|(loc, _)| Some(loc.as_str()) == want)
        .or_else(|| name_es_var.first())
        .map(|(_, v)| v.clone());
    let name = non_empty(name_es).or(non_empty(es_var)).or(non_empty(name));
    if hidden || !kind_ok {
        return None;
    }
    Some(AppInfo {
        app_id: id.to_string(),
        name: name?,
        icon: non_empty(icon),
        exec: non_empty(exec),
        wm_class: non_empty(wm_class),
        terminal,
        path: non_empty(path),
    })
}

/// Escanea directorios `applications` en orden; el primero que define un id gana
/// (un `Hidden=true` también lo reserva). Subcarpetas dan ids `sub-nombre`.
pub fn load_from_dirs(dirs: &[PathBuf]) -> Vec<AppInfo> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for dir in dirs {
        scan(dir, "", 0, &mut seen, &mut out);
    }
    out
}

/// Subcarpetas máximas que se recorren dentro de cada `applications`.
const MAX_DEPTH: usize = 3;

fn scan(
    dir: &Path,
    prefix: &str,
    depth: usize,
    seen: &mut std::collections::HashSet<String>,
    out: &mut Vec<AppInfo>,
) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if path.is_dir() {
            if depth < MAX_DEPTH {
                scan(&path, &format!("{prefix}{fname}-"), depth + 1, seen, out);
            }
        } else if let Some(stem) = fname.strip_suffix(".desktop") {
            let id = format!("{prefix}{stem}");
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(info) = std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| parse_desktop(&id, &t))
            {
                out.push(info);
            }
        }
    }
}

/// Directorios `applications` en orden de prioridad (sin repetidos).
pub fn data_dirs(
    home: Option<&str>,
    xdg_home: Option<&str>,
    xdg_dirs: Option<&str>,
) -> Vec<PathBuf> {
    let mut bases: Vec<PathBuf> = Vec::new();
    match (xdg_home.filter(|s| !s.is_empty()), home) {
        (Some(x), _) => bases.push(x.into()),
        (None, Some(h)) => bases.push(Path::new(h).join(".local/share")),
        _ => {}
    }
    let dirs = xdg_dirs
        .filter(|s| !s.is_empty())
        .unwrap_or("/usr/local/share:/usr/share");
    bases.extend(dirs.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    // Exports de Flatpak (no siempre están en XDG_DATA_DIRS en una sesión COSMIC).
    if let Some(h) = home {
        bases.push(Path::new(h).join(".local/share/flatpak/exports/share"));
    }
    bases.push("/var/lib/flatpak/exports/share".into());
    let mut out: Vec<PathBuf> = Vec::new();
    for b in bases {
        let d = b.join("applications");
        if !out.contains(&d) {
            out.push(d);
        }
    }
    out
}

/// Apps instaladas según el entorno XDG (incluye exports de Flatpak si están en
/// `XDG_DATA_DIRS`). Bloqueante: llamar fuera del hilo de UI.
pub fn load_entries() -> Vec<AppInfo> {
    let var = |k| std::env::var(k).ok();
    let dirs = data_dirs(
        var("HOME").as_deref(),
        var("XDG_DATA_HOME").as_deref(),
        var("XDG_DATA_DIRS").as_deref(),
    );
    load_from_dirs(&dirs)
}

/// Escapes de cadena del estándar: `\s` espacio, `\n`, `\t`, `\r`, `\\`; otros se conservan.
fn unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut it = v.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(n) => {
                out.push('\\');
                out.push(n);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Separa `Exec` con las reglas de comillas del estándar (comillas dobles; dentro,
/// `\` escapa `"`, `` ` ``, `$` y `\`), quita los códigos de campo y convierte `%%` en `%`.
pub fn exec_argv(exec: &str) -> Vec<String> {
    let exec = &unescape(exec);
    let mut args: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' if quoted => match chars.next() {
                Some(n @ ('"' | '`' | '$' | '\\')) => cur.push(n),
                Some(n) => {
                    cur.push('\\');
                    cur.push(n);
                }
                None => cur.push('\\'),
            },
            c if c.is_whitespace() && !quoted => {
                if started {
                    args.extend(strip_codes(&cur));
                    cur.clear();
                    started = false;
                }
            }
            c => {
                cur.push(c);
                started = true;
            }
        }
    }
    if started {
        args.extend(strip_codes(&cur));
    }
    args
}

/// Quita códigos de campo de un argumento; `%%` → `%`. Un argumento que solo era
/// código desaparece (se devuelve `None`); uno vacío por comillas se conserva.
fn strip_codes(tok: &str) -> Option<String> {
    let mut out = String::new();
    let mut it = tok.chars();
    while let Some(c) = it.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('%') => out.push('%'),
            Some('f' | 'F' | 'u' | 'U' | 'i' | 'c' | 'k' | 'd' | 'D' | 'n' | 'N' | 'v' | 'm') => {}
            Some(n) => {
                out.push('%');
                out.push(n);
            }
            None => out.push('%'),
        }
    }
    (!out.is_empty() || tok.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn app(id: &str, wm: Option<&str>) -> AppInfo {
        AppInfo {
            app_id: id.into(),
            name: id.into(),
            icon: None,
            exec: None,
            wm_class: wm.map(Into::into),
            terminal: false,
            path: None,
        }
    }

    fn catalog() -> Vec<AppInfo> {
        vec![
            app("brave-browser", None),
            app("code", Some("Code")),
            app("github-desktop", Some("GitHub Desktop")),
            app("kitty", None),
            app("com.spotify.Client", None),
        ]
    }

    #[test]
    fn resuelve_casos_reales() {
        let c = catalog();
        let idx = AppIndex::new(c);
        let id = |a: &str| idx.get(a).map(|x| x.app_id.as_str());
        assert_eq!(id("brave-browser"), Some("brave-browser"));
        assert_eq!(id("Code"), Some("code"));
        assert_eq!(id("GitHub Desktop"), Some("github-desktop"));
        assert_eq!(id("kitty"), Some("kitty"));
        assert_eq!(id("com.spotify.Client"), Some("com.spotify.Client"));
        assert_eq!(id("COM.SPOTIFY.CLIENT"), Some("com.spotify.Client"));
        assert_eq!(id("desconocida"), None);
        assert_eq!(id(""), None);
    }

    #[test]
    fn id_gana_sobre_wm_class() {
        let c = vec![app("otra", Some("kitty")), app("kitty", None)];
        assert_eq!(AppIndex::new(c).get("kitty").unwrap().app_id, "kitty");
    }

    const CODE: &str = "# comentario\n[Desktop Entry]\nType=Application\nName=Visual Studio Code\nName[es]=VS Code es\nIcon=vscode\nExec=code --new-window %F\nStartupWMClass=Code\n\n[Desktop Action new-empty-window]\nName=Nueva ventana\nExec=code --new-empty-window\n";

    #[test]
    fn parsea_desktop_entry() {
        let a = parse_desktop("code", CODE).unwrap();
        assert_eq!(a.app_id, "code");
        assert_eq!(a.name, "VS Code es"); // Name[es] gana sobre Name
        assert_eq!(a.icon.as_deref(), Some("vscode"));
        assert_eq!(a.exec.as_deref(), Some("code --new-window %F"));
        assert_eq!(a.wm_class.as_deref(), Some("Code"));
    }

    #[test]
    fn parser_descarta_hidden_y_sin_nombre_y_no_app() {
        assert!(parse_desktop("a", "[Desktop Entry]\nName=A\nHidden=true\n").is_none());
        assert!(parse_desktop("a", "[Desktop Entry]\nIcon=x\n").is_none());
        assert!(parse_desktop("a", "[Desktop Entry]\nName=A\nType=Link\n").is_none());
        assert!(parse_desktop("a", "[Other]\nName=A\n").is_none());
        // NoDisplay se conserva: la app puede tener ventanas.
        assert!(parse_desktop("a", "[Desktop Entry]\nName=A\nNoDisplay=true\n").is_some());
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("morph-apps-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn escanea_dirs_primeros_ganan() {
        let root = tmp("scan");
        let (a, b) = (root.join("a/applications"), root.join("b/applications"));
        fs::create_dir_all(a.join("sub")).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("kitty.desktop"), "[Desktop Entry]\nName=Kitty A\n").unwrap();
        fs::write(b.join("kitty.desktop"), "[Desktop Entry]\nName=Kitty B\n").unwrap();
        fs::write(b.join("otra.desktop"), "[Desktop Entry]\nName=Otra\n").unwrap();
        fs::write(b.join("nota.txt"), "x").unwrap();
        fs::write(a.join("sub/inner.desktop"), "[Desktop Entry]\nName=Inner\n").unwrap();
        // Hidden en el dir previo oculta el id de los siguientes.
        fs::write(
            a.join("oculta.desktop"),
            "[Desktop Entry]\nName=O\nHidden=true\n",
        )
        .unwrap();
        fs::write(b.join("oculta.desktop"), "[Desktop Entry]\nName=O2\n").unwrap();
        let got = load_from_dirs(&[a, b, root.join("no-existe")]);
        let name = |id: &str| got.iter().find(|x| x.app_id == id).map(|x| x.name.clone());
        assert_eq!(name("kitty").as_deref(), Some("Kitty A"));
        assert_eq!(name("otra").as_deref(), Some("Otra"));
        assert_eq!(name("sub-inner").as_deref(), Some("Inner"));
        assert_eq!(name("oculta"), None);
        assert_eq!(got.len(), 3);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn data_dirs_orden_y_defaults() {
        let d = data_dirs(Some("/home/u"), Some("/xh"), Some("/a:/b::/a"));
        let s: Vec<_> = d.iter().map(|p| p.to_str().unwrap()).collect();
        assert_eq!(
            s,
            [
                "/xh/applications",
                "/a/applications",
                "/b/applications",
                "/home/u/.local/share/flatpak/exports/share/applications",
                "/var/lib/flatpak/exports/share/applications"
            ]
        );
        let d = data_dirs(Some("/home/u"), None, None);
        let s: Vec<_> = d.iter().map(|p| p.to_str().unwrap()).collect();
        assert_eq!(
            s,
            [
                "/home/u/.local/share/applications",
                "/usr/local/share/applications",
                "/usr/share/applications",
                "/home/u/.local/share/flatpak/exports/share/applications",
                "/var/lib/flatpak/exports/share/applications"
            ]
        );
    }

    fn argv(s: &str) -> Vec<String> {
        exec_argv(s)
    }

    #[test]
    fn exec_quita_codigos() {
        assert_eq!(argv("code --new-window %F"), ["code", "--new-window"]);
        assert_eq!(argv("kitty %u %U %f %i %c %k"), ["kitty"]);
        assert_eq!(
            argv("app --icon %i --name=%c x"),
            ["app", "--icon", "--name=", "x"]
        );
        assert_eq!(argv("app 100%% %n"), ["app", "100%"]);
        assert_eq!(argv("  a   b  "), ["a", "b"]);
        assert!(argv("").is_empty());
    }

    #[test]
    fn exec_comillas() {
        assert_eq!(argv("\"/opt/My App/app\" %U"), ["/opt/My App/app"]);
        assert_eq!(
            argv(r#"sh -c "echo \"hi\" \$HOME \\ x""#),
            ["sh", "-c", r#"echo "hi" $HOME \ x"#]
        );
        assert_eq!(argv("a \"\" b"), ["a", "", "b"]);
        assert_eq!(argv("env X=1 \"my app\"%f"), ["env", "X=1", "my app"]);
    }

    #[test]
    fn nombre_en_espanol_gana() {
        let t = "[Desktop Entry]\nName=Files\nName[es]=Archivos\nName[es_MX]=Archivos MX\nName[fr]=Fichiers\n";
        assert_eq!(parse_desktop("a", t).unwrap().name, "Archivos");
        // Sin Name[es]: Name[es_*] antes que Name.
        let t = "[Desktop Entry]\nName=Files\nName[fr]=Fichiers\nName[es_MX]=Archivos MX\n";
        assert_eq!(parse_desktop("a", t).unwrap().name, "Archivos MX");
        let t = "[Desktop Entry]\nName[es]=Solo es\n";
        assert_eq!(parse_desktop("a", t).unwrap().name, "Solo es");
        let t = "[Desktop Entry]\nName=Files\nName[fr]=Fichiers\n";
        assert_eq!(parse_desktop("a", t).unwrap().name, "Files");
    }

    #[test]
    fn exec_desescapa_antes_de_comillas() {
        let t = "[Desktop Entry]\nName=A\nExec=sh -c \"echo\\sx\"\n";
        let a = parse_desktop("a", t).unwrap();
        assert_eq!(
            exec_argv(a.exec.as_deref().unwrap()),
            ["sh", "-c", "echo x"]
        );
        assert_eq!(unescape(r"a\sb\nc\td\re\\f\q"), "a b\nc\td\re\\f\\q");
    }

    #[test]
    fn data_dirs_incluye_flatpak_sin_repetir() {
        let d = data_dirs(Some("/home/u"), None, Some("/usr/share"));
        let s: Vec<_> = d.iter().map(|p| p.to_str().unwrap()).collect();
        assert_eq!(
            s,
            [
                "/home/u/.local/share/applications",
                "/usr/share/applications",
                "/home/u/.local/share/flatpak/exports/share/applications",
                "/var/lib/flatpak/exports/share/applications",
            ]
        );
        let d = data_dirs(
            Some("/home/u"),
            None,
            Some("/var/lib/flatpak/exports/share"),
        );
        let n = d
            .iter()
            .filter(|p| p.to_str() == Some("/var/lib/flatpak/exports/share/applications"))
            .count();
        assert_eq!(n, 1);
    }

    #[test]
    fn escaneo_limita_profundidad() {
        let root = tmp("depth");
        let a = root.join("applications");
        let deep = a.join("a/b/c/d");
        fs::create_dir_all(&deep).unwrap();
        fs::write(a.join("a/b/c/ok.desktop"), "[Desktop Entry]\nName=Ok\n").unwrap();
        fs::write(deep.join("hondo.desktop"), "[Desktop Entry]\nName=H\n").unwrap();
        let got = load_from_dirs(&[a]);
        assert!(got.iter().any(|x| x.app_id == "a-b-c-ok"));
        assert!(!got.iter().any(|x| x.app_id.contains("hondo")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parsea_terminal_y_path() {
        let t = "[Desktop Entry]\nName=A\nTerminal=true\nPath=/opt/a\nExec=a\n";
        let a = parse_desktop("a", t).unwrap();
        assert!(a.terminal);
        assert_eq!(a.path.as_deref(), Some("/opt/a"));
        let a = parse_desktop("a", "[Desktop Entry]\nName=A\n").unwrap();
        assert!(!a.terminal);
        assert_eq!(a.path, None);
    }

    #[test]
    fn indice_en_minusculas_y_clase() {
        let idx = AppIndex::new(catalog());
        assert_eq!(idx.get("GITHUB DESKTOP").unwrap().app_id, "github-desktop");
        assert!(idx.get("").is_none());
        assert!(AppIndex::default().get("code").is_none());
    }
}
