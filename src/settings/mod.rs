//! Panel de Ajustes: estado, mensajes y lógica pura (la vista vive en `view`).

pub mod view;

use std::path::{Path, PathBuf};

use crate::config::{DesktopsLayout, DevLayout, HudStyle, Language, Pin, ShelfLayout, TermSize};
use crate::island::Page;

/// Plantilla del archivo de autostart (el mismo del paquete); su `Exec=` se reemplaza.
const AUTOSTART_TEMPLATE: &str =
    include_str!("../../resources/io.github.jamthebest.Morph-autostart.desktop");
const AUTOSTART_FILE: &str = "io.github.jamthebest.Morph.desktop";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    General,
    Pages,
    Shelf,
    Desktops,
    Dev,
    GitHub,
    GitLab,
    Terminal,
    Notices,
}

impl Section {
    pub const ALL: [Section; 9] = [
        Section::General,
        Section::Pages,
        Section::Shelf,
        Section::Desktops,
        Section::Dev,
        Section::GitHub,
        Section::GitLab,
        Section::Terminal,
        Section::Notices,
    ];

    pub fn title(self) -> String {
        match self {
            Section::General => crate::fl!("settings-section-general"),
            Section::Pages => crate::fl!("settings-section-pages"),
            Section::Shelf => crate::fl!("settings-section-shelf"),
            Section::Desktops => crate::fl!("settings-section-desktops"),
            Section::Dev => crate::fl!("settings-section-dev"),
            Section::GitHub => crate::fl!("settings-section-github"),
            Section::GitLab => crate::fl!("settings-section-gitlab"),
            Section::Terminal => crate::fl!("settings-section-terminal"),
            Section::Notices => crate::fl!("settings-section-notices"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum SettingsMsg {
    Open,
    Close,
    Section(Section),
    Set(Change),
    PageMove(Page, i32),
    PageToggle(Page, bool),
    Autostart(bool),
    /// Quita esa app fijada.
    Unpin(Pin),
    /// Borrador del servidor de GitLab mientras se escribe.
    GitlabHostEdit(String),
    /// Guarda el borrador si es válido (Enter, cambio de sección o cierre).
    GitlabHostCommit,
    /// Pone la pastilla en el centro del monitor.
    CenterPill,
}

/// Un cambio de configuración hecho desde el panel (`App::apply_change` lo guarda).
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Clock12h(bool),
    Size(u16),
    Output(Option<String>),
    OpenOnHover(bool),
    Animate(bool),
    Sounds(bool),
    Minimal(bool),
    HudStyle(HudStyle),
    HudVolume(bool),
    HudBrightness(bool),
    NotifyBattery(bool),
    NotifyWifi(bool),
    NotifyBluetooth(bool),
    NotifyCi(bool),
    ShelfLayout(ShelfLayout),
    ShelfBadge(bool),
    ShelfStartEmpty(bool),
    ShelfDragCopy(bool),
    DesktopsLayout(DesktopsLayout),
    DevLayout(DevLayout),
    TermSize(TermSize),
    TermFont(u16),
    GitlabHost(String),
    GithubRepos(Vec<String>),
    GitlabProjects(Vec<String>),
    Pins(Vec<crate::config::Pin>),
    HideLine(bool),
    PanelLevel(bool),
    PillBattery(bool),
    PillVolume(bool),
    Language(Language),
}

/// Posición centrada de la pastilla.
pub const PILL_CENTER: f32 = 0.5;

/// «Centrar» solo sirve si la pastilla no está ya centrada.
pub fn can_center(pill_pos: f32) -> bool {
    pill_pos != PILL_CENTER
}

/// Páginas en el orden mostrado: las activas como están en `pages`, luego las
/// inactivas en el orden de `Page::ALL`.
pub fn page_rows(pages: &[Page]) -> Vec<(Page, bool)> {
    let mut rows: Vec<(Page, bool)> = pages.iter().map(|p| (*p, true)).collect();
    rows.extend(
        Page::ALL
            .iter()
            .filter(|p| !pages.contains(p))
            .map(|p| (*p, false)),
    );
    rows
}

/// Activa o desactiva una página; nunca deja la lista vacía. Activar la añade al final.
pub fn toggle_page(pages: &[Page], p: Page, on: bool) -> Vec<Page> {
    let mut v = pages.to_vec();
    if on {
        if !v.contains(&p) {
            v.push(p);
        }
    } else if v.len() > 1 {
        v.retain(|x| *x != p);
    }
    v
}

/// Mueve una página activa `delta` posiciones entre las activas; en los bordes no cambia.
pub fn move_page(pages: &[Page], p: Page, delta: i32) -> Vec<Page> {
    let mut v = pages.to_vec();
    let Some(i) = v.iter().position(|x| *x == p) else {
        return v;
    };
    let j = i as i64 + i64::from(delta);
    if j < 0 || j >= v.len() as i64 {
        return v;
    }
    v.swap(i, j as usize);
    v
}

/// Página que se muestra de verdad: la pedida si sigue activa, si no la primera activa.
pub fn effective_page(pages: &[Page], page: Page) -> Page {
    if pages.contains(&page) {
        page
    } else {
        pages.first().copied().unwrap_or(page)
    }
}

/// Carpeta de autostart: `$MORPH_AUTOSTART_DIR`, `$XDG_CONFIG_HOME/autostart` o `~/.config/autostart`.
pub fn autostart_dir() -> PathBuf {
    let var = |k: &str| crate::envcompat::var_os(k).filter(|v| !v.is_empty());
    if let Some(d) = var("MORPH_AUTOSTART_DIR") {
        return PathBuf::from(d);
    }
    if let Some(d) = var("XDG_CONFIG_HOME") {
        return PathBuf::from(d).join("autostart");
    }
    var("HOME")
        .map(|h| PathBuf::from(h).join(".config/autostart"))
        .unwrap_or_else(|| PathBuf::from(".config/autostart"))
}

/// Valor de `Exec=` para un ejecutable: sin comillas si es "simple"; si no, entre comillas
/// dobles con `\\`, `"`, `` ` `` y `$` escapados (Desktop Entry: dos niveles de escape, así
/// que la barra invertida se duplica) y `%` como `%%`. Sin ruta, `morph`.
fn exec_value(exe: Option<&Path>) -> String {
    let Some(exe) = exe else {
        return "morph".to_string();
    };
    let raw = exe.to_string_lossy();
    let reserved = " \t\n\"'\\><~|&;$*?#()`";
    let quote = raw.chars().any(|c| reserved.contains(c));
    let mut out = String::new();
    if quote {
        out.push('"');
    }
    for c in raw.chars() {
        match c {
            '%' => out.push_str("%%"),
            '"' | '`' | '$' if quote => {
                out.push('\\');
                out.push(c);
            }
            '\\' if quote => out.push_str("\\\\\\\\"),
            c => out.push(c),
        }
    }
    if quote {
        out.push('"');
    }
    out
}

/// Contenido del autostart con `Exec=` apuntando al ejecutable dado (función pura).
pub fn autostart_contents(exe: Option<&Path>) -> String {
    let exec = format!("Exec={}", exec_value(exe));
    let mut out = String::new();
    for line in AUTOSTART_TEMPLATE.lines() {
        out.push_str(if line.starts_with("Exec=") {
            &exec
        } else {
            line
        });
        out.push('\n');
    }
    out
}

pub fn autostart_enabled(dir: &Path) -> bool {
    dir.join(AUTOSTART_FILE).is_file()
}

/// Activa (crea el archivo, y la carpeta si falta) o desactiva (lo borra; si no está, Ok).
pub fn set_autostart(dir: &Path, on: bool) -> std::io::Result<()> {
    let file = dir.join(AUTOSTART_FILE);
    if on {
        std::fs::create_dir_all(dir)?;
        let exe = std::env::current_exe().ok();
        std::fs::write(file, autostart_contents(exe.as_deref()))
    } else {
        match std::fs::remove_file(file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

/// Normaliza el servidor de GitLab a `host[:puerto]`: sin esquema, sin `/` final y en
/// minúsculas. El host son etiquetas `[a-z0-9-]` separadas por puntos (sin vacías ni
/// con `-` en los extremos); el puerto, 1–65535.
pub fn normalize_host(s: &str) -> Option<String> {
    let lower = s.trim().to_lowercase();
    let h = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .unwrap_or(&lower);
    let h = h.strip_suffix('/').unwrap_or(h);
    let (name, port) = match h.split_once(':') {
        Some((n, p)) => (n, Some(p)),
        None => (h, None),
    };
    let label_ok = |l: &str| {
        !l.is_empty()
            && !l.starts_with('-')
            && !l.ends_with('-')
            && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    if !name.split('.').all(label_ok) {
        return None;
    }
    if let Some(p) = port
        && (p.is_empty()
            || !p.chars().all(|c| c.is_ascii_digit())
            || !p.parse::<u32>().is_ok_and(|n| (1..=65535).contains(&n)))
    {
        return None;
    }
    Some(h.to_string())
}

/// Qué hacer con el borrador del servidor de GitLab.
#[derive(Debug, PartialEq, Eq)]
pub enum HostDecision {
    /// Guardar este valor normalizado.
    Save(String),
    /// Nada que guardar: descartar el borrador.
    Drop,
    /// No válido y el usuario sigue editando: conservar el borrador y mostrar el error.
    KeepInvalid,
}

/// `last`: el panel se cierra o se cambia de sección (un borrador no válido se descarta);
/// si no (Enter), un borrador no válido se conserva.
pub fn decide_host(draft: &str, current: &str, last: bool) -> HostDecision {
    match normalize_host(draft) {
        Some(h) if h != current => HostDecision::Save(h),
        Some(_) => HostDecision::Drop,
        None if last => HostDecision::Drop,
        None => HostDecision::KeepInvalid,
    }
}

/// ¿Se ignora la salida del cursor? Sí con Ajustes, un formulario o un borrador abiertos.
pub fn keep_open_on_leave(settings_open: bool, form_open: bool, draft: bool) -> bool {
    settings_open || form_open || draft
}

/// Añade `name` a la lista si no está y lo quita si está; conserva el orden.
pub fn toggle_repo(list: &[String], name: &str) -> Vec<String> {
    if list.iter().any(|r| r == name) {
        list.iter().filter(|r| *r != name).cloned().collect()
    } else {
        let mut v = list.to_vec();
        v.push(name.to_string());
        v
    }
}

/// Como `toggle_repo` para proyectos de GitLab: al quitar se van todas las grafías
/// (sin distinguir mayúsculas); al añadir se guarda `name` tal cual.
pub fn toggle_project(list: &[String], name: &str) -> Vec<String> {
    if list.iter().any(|r| r.eq_ignore_ascii_case(name)) {
        list.iter()
            .filter(|r| !r.eq_ignore_ascii_case(name))
            .cloned()
            .collect()
    } else {
        let mut v = list.to_vec();
        v.push(name.to_string());
        v
    }
}

/// ¿El resultado asíncrono de GitLab es del servidor actual? Si no, se ignora.
pub fn host_is_current(result_host: &str, current: &str) -> bool {
    result_host == current
}

/// «N repos de GitHub · M proyectos de GitLab» con singular.
pub fn summary(repos: usize, projects: usize) -> String {
    crate::fl!("settings-summary", repos = repos, projects = projects)
}

/// Pulsar la sección ya abierta no hace nada.
pub fn section_switch(current: Section, next: Section) -> bool {
    current != next
}

/// ¿Toca lanzar la lectura aplazada? Solo el último temporizador y si algo sigue pendiente.
pub fn debounce_fire(scheduled: u64, current: u64, pending: bool) -> bool {
    pending && scheduled == current
}

/// Quita una app fijada (por valor, no por posición).
pub fn unpin(pins: &[Pin], p: &Pin) -> Vec<Pin> {
    pins.iter().filter(|x| *x != p).cloned().collect()
}

/// Cambia el tamaño de la letra de la terminal, acotado a 9..=24.
pub fn step_font(size: u16, delta: i32) -> u16 {
    (i32::from(size) + delta).clamp(9, 24) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use Page::*;

    #[test]
    fn host_tag_check() {
        assert!(host_is_current("gitlab.com", "gitlab.com"));
        assert!(!host_is_current("gitlab.com", "git.acme.io"));
    }

    #[test]
    fn untoggle_removes_every_spelling() {
        let l = vec![
            "Grupo/P".to_string(),
            "x/y".to_string(),
            "grupo/p".to_string(),
        ];
        assert_eq!(toggle_project(&l, "GRUPO/p"), ["x/y"]);
        assert_eq!(
            toggle_project(&l, "a/b"),
            ["Grupo/P", "x/y", "grupo/p", "a/b"]
        );
    }

    #[test]
    fn summary_singular() {
        crate::i18n::apply_for_test("es");
        assert_eq!(summary(1, 1), "1 repo de GitHub · 1 proyecto de GitLab");
        assert_eq!(summary(0, 2), "0 repos de GitHub · 2 proyectos de GitLab");
    }

    #[test]
    fn english_titles_and_plural_summary() {
        crate::i18n::apply_for_test("en");
        assert_eq!(Section::Notices.title(), "Notices & HUD");
        assert_eq!(Section::Pages.title(), "Pages");
        assert_eq!(summary(1, 1), "1 GitHub repo · 1 GitLab project");
        assert_eq!(summary(0, 2), "0 GitHub repos · 2 GitLab projects");
        assert_eq!(Page::Todo.title(), "Tasks");
    }

    #[test]
    fn same_section_is_noop() {
        assert!(!section_switch(Section::GitLab, Section::GitLab));
        assert!(section_switch(Section::GitLab, Section::GitHub));
    }

    #[test]
    fn debounce_decision() {
        assert!(debounce_fire(3, 3, true));
        assert!(!debounce_fire(2, 3, true)); // llegó otro cambio después
        assert!(!debounce_fire(3, 3, false)); // ya se lanzó
    }

    #[test]
    fn nav_has_nine_sections_in_order() {
        crate::i18n::apply_for_test("es");
        let t: Vec<String> = Section::ALL.iter().map(|s| s.title()).collect();
        assert_eq!(
            t,
            [
                "General",
                "Páginas",
                "Estante",
                "Escritorios",
                "Desarrollo",
                "GitHub",
                "GitLab",
                "Terminal",
                "Avisos y HUD"
            ]
        );
    }

    #[test]
    fn leave_is_ignored_when_any_is_open() {
        assert!(!keep_open_on_leave(false, false, false));
        assert!(keep_open_on_leave(true, false, false));
        assert!(keep_open_on_leave(false, true, false));
        assert!(keep_open_on_leave(false, false, true));
        assert!(keep_open_on_leave(true, true, true));
    }

    #[test]
    fn toggle_repo_adds_removes_keeps_order() {
        let l = vec!["a/x".to_string(), "b/y".to_string(), "c/z".to_string()];
        assert_eq!(toggle_repo(&l, "b/y"), ["a/x", "c/z"]);
        assert_eq!(toggle_repo(&l, "d/w"), ["a/x", "b/y", "c/z", "d/w"]);
        let dup = vec!["a/x".to_string(), "a/x".to_string()];
        assert_eq!(toggle_repo(&dup, "a/x"), Vec::<String>::new());
        assert_eq!(toggle_repo(&[], "a/x"), ["a/x"]);
    }

    #[test]
    fn center_pill_target_and_availability() {
        assert_eq!(PILL_CENTER, 0.5);
        assert!(can_center(0.3));
        assert!(!can_center(PILL_CENTER));
        assert!(!can_center(0.5));
    }

    fn tmp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("morph-set-{tag}-{}", std::process::id()))
    }

    #[test]
    fn rows_order_active_then_inactive() {
        assert_eq!(
            page_rows(&[Dev, Musica]),
            vec![
                (Dev, true),
                (Musica, true),
                (Calendario, false),
                (Todo, false),
                (Shelf, false),
                (Escritorios, false)
            ]
        );
    }

    #[test]
    fn toggle_never_empties() {
        assert_eq!(toggle_page(&[Musica], Musica, false), vec![Musica]);
        let no_shelf = toggle_page(&Page::ALL, Shelf, false);
        assert!(!no_shelf.contains(&Shelf) && no_shelf.len() == 5);
        assert_eq!(toggle_page(&[Musica], Dev, true), vec![Musica, Dev]);
        assert_eq!(toggle_page(&[Musica, Dev], Dev, true), vec![Musica, Dev]);
    }

    #[test]
    fn move_cases() {
        assert_eq!(
            move_page(&[Musica, Dev, Todo], Dev, -1),
            vec![Dev, Musica, Todo]
        );
        assert_eq!(
            move_page(&[Musica, Dev, Todo], Dev, 1),
            vec![Musica, Todo, Dev]
        );
        assert_eq!(
            move_page(&[Musica, Dev, Todo], Musica, -1),
            vec![Musica, Dev, Todo]
        );
        assert_eq!(
            move_page(&[Musica, Dev, Todo], Todo, 1),
            vec![Musica, Dev, Todo]
        );
        assert_eq!(move_page(&[Musica, Dev], Shelf, -1), vec![Musica, Dev]);
    }

    #[test]
    fn effective_page_falls_back_to_first() {
        assert_eq!(effective_page(&[Dev, Musica], Musica), Musica);
        assert_eq!(effective_page(&[Dev, Musica], Todo), Dev);
    }

    #[test]
    fn hiding_current_page_resolves_to_first_active() {
        let pages = toggle_page(&[Todo, Dev, Musica], Todo, false);
        assert_eq!(pages, vec![Dev, Musica]);
        assert_eq!(effective_page(&pages, Todo), Dev);
        // Ocultar hasta quedar una: la última no se puede quitar.
        let one = toggle_page(&toggle_page(&pages, Dev, false), Musica, false);
        assert_eq!(one, vec![Musica]);
    }

    #[test]
    fn section_titles() {
        crate::i18n::apply_for_test("es");
        assert_eq!(Section::ALL[0].title(), "General");
        assert_eq!(Section::Notices.title(), "Avisos y HUD");
        assert_eq!(Section::ALL.len(), 9);
    }

    #[test]
    fn autostart_creates_and_removes() {
        let dir = tmp("auto").join("no/existe");
        let _ = std::fs::remove_dir_all(tmp("auto"));
        assert!(!autostart_enabled(&dir));
        set_autostart(&dir, true).unwrap();
        assert!(autostart_enabled(&dir));
        let body = std::fs::read_to_string(dir.join("io.github.jamthebest.Morph.desktop")).unwrap();
        let exe = std::env::current_exe().unwrap();
        assert_eq!(body, autostart_contents(Some(&exe)));
        set_autostart(&dir, false).unwrap();
        assert!(!autostart_enabled(&dir));
        set_autostart(&dir, false).unwrap();
        let _ = std::fs::remove_dir_all(tmp("auto"));
    }

    fn exec_line(exe: Option<&str>) -> String {
        let c = autostart_contents(exe.map(Path::new));
        c.lines()
            .find(|l| l.starts_with("Exec="))
            .unwrap()
            .to_string()
    }

    #[test]
    fn autostart_contents_uses_absolute_path() {
        assert_eq!(
            exec_line(Some("/home/u/.local/bin/morph")),
            "Exec=/home/u/.local/bin/morph"
        );
        assert_eq!(exec_line(None), "Exec=morph");
        let c = autostart_contents(Some(Path::new("/usr/bin/morph")));
        assert!(c.contains("X-GNOME-Autostart-enabled=true"));
        assert!(c.ends_with('\n'));
        assert_eq!(c.matches("Exec=").count(), 1);
    }

    #[test]
    fn autostart_contents_quotes_special_paths() {
        assert_eq!(
            exec_line(Some("/home/a b/morph")),
            "Exec=\"/home/a b/morph\""
        );
        assert_eq!(exec_line(Some("/o/$x/morph")), "Exec=\"/o/\\$x/morph\"");
        assert_eq!(exec_line(Some("/o/50%/morph")), "Exec=/o/50%%/morph");
        assert_eq!(exec_line(Some("/a \"b\"/m")), "Exec=\"/a \\\"b\\\"/m\"");
    }

    #[test]
    fn autostart_error_is_reported() {
        // Un archivo donde debería ir la carpeta: no se puede crear.
        let root = tmp("err");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("f");
        std::fs::write(&file, "x").unwrap();
        assert!(set_autostart(&file.join("sub"), true).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn host_normalization() {
        assert_eq!(normalize_host("gitlab.com"), Some("gitlab.com".into()));
        assert_eq!(
            normalize_host(" https://GitLab.Example.com/ "),
            Some("gitlab.example.com".into())
        );
        assert_eq!(
            normalize_host("http://git.local:8443"),
            Some("git.local:8443".into())
        );
        assert_eq!(normalize_host(""), None);
        assert_eq!(normalize_host("gitlab.com/grupo"), None);
        assert_eq!(normalize_host("a b"), None);
        assert_eq!(normalize_host("https://"), None);
        assert_eq!(
            normalize_host("git.local:8443"),
            Some("git.local:8443".into())
        );
        for bad in [
            "h:abc", "u@h", "h?x", "h:0", "h:", "h:65536", "-a.com", "a..b", ".", "h#x",
        ] {
            assert_eq!(normalize_host(bad), None, "{bad}");
        }
    }

    #[test]
    fn host_decisions() {
        use HostDecision::*;
        assert_eq!(
            decide_host("GitLab.io", "gitlab.com", false),
            Save("gitlab.io".into())
        );
        assert_eq!(decide_host("gitlab.com", "gitlab.com", true), Drop);
        assert_eq!(decide_host("a b", "gitlab.com", false), KeepInvalid);
        assert_eq!(decide_host("a b", "gitlab.com", true), Drop);
    }

    #[test]
    fn unpin_by_value() {
        let p = |a: &str, w| Pin {
            app_id: a.into(),
            workspace: w,
        };
        let pins = vec![p("a", 1), p("b", 2), p("a", 2)];
        assert_eq!(unpin(&pins, &p("a", 2)), vec![p("a", 1), p("b", 2)]);
        assert_eq!(unpin(&pins, &p("z", 1)), pins);
    }

    #[test]
    fn font_step_clamps() {
        assert_eq!(step_font(13, 1), 14);
        assert_eq!(step_font(24, 1), 24);
        assert_eq!(step_font(9, -1), 9);
    }
}
