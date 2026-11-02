use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

use crate::island::Page;

#[derive(CosmicConfigEntry, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[version = 1]
pub struct MorphConfig {
    /// Nombre del monitor (p. ej. "eDP-1"); None = monitor activo.
    pub output: Option<String>,
    /// 0 = Auto, o 100/115/130/150.
    pub size_percent: u16,
    pub minimal: bool,
    pub open_on_hover: bool,
    /// Horas en formato de 12 h ("5:07 p. m."); false = 24 h.
    pub clock_12h: bool,
    pub pages: Vec<Page>,
    /// Animar la apertura y el cierre; false = salto directo.
    pub animate: bool,
    /// Sonido en alarmas y recordatorios.
    pub sounds: bool,
    /// Disposición de la página Shelf.
    pub shelf_layout: ShelfLayout,
    /// Mostrar el contador de archivos del Shelf en la isla.
    pub shelf_badge: bool,
    /// Vaciar el Shelf al iniciar.
    pub shelf_start_empty: bool,
    /// Arrastres del Shelf: true = copiar (el original se queda), false = mover.
    pub shelf_drag_copy: bool,
    /// Disposición de la página Escritorios.
    pub desktops_layout: DesktopsLayout,
    /// Apps fijadas a un escritorio.
    pub pins: Vec<Pin>,
    /// Disposición de la página Dev.
    pub dev_layout: DevLayout,
    /// Repos de GitHub vigilados (`owner/name`).
    pub github_repos: Vec<String>,
    /// Servidor de GitLab (sin esquema).
    pub gitlab_host: String,
    /// Proyectos de GitLab vigilados (path completo `grupo/sub/proyecto`).
    pub gitlab_projects: Vec<String>,
    /// Tamaño de la terminal desplegable.
    pub term_size: TermSize,
    /// Alto elegido arrastrando el borde (% del monitor); None = el del tamaño.
    pub term_height_pct: Option<u8>,
    /// Tamaño de la fuente de la terminal.
    pub term_font_size: u16,
    /// Forma del HUD de volumen y brillo.
    pub hud_style: HudStyle,
    /// Mostrar el HUD al cambiar el volumen.
    pub hud_volume: bool,
    /// Mostrar el HUD al cambiar el brillo.
    pub hud_brightness: bool,
    /// Avisos de batería baja, crítica y cargada.
    pub notify_battery: bool,
    /// Avisos de Wi-Fi (conectado, desconectado, sin internet).
    pub notify_wifi: bool,
    /// Avisos de dispositivos Bluetooth de audio.
    pub notify_bluetooth: bool,
    /// Avisos de pipelines y Actions que cambian de estado.
    pub notify_ci: bool,
    /// Centro de la pastilla como fracción (0–1) del ancho del monitor; 0.5 = centrada.
    pub pill_pos: f32,
    /// Cerrada y sin hover, la isla se reduce a una línea negra.
    pub hide_line: bool,
    /// Borde del monitor en el que vive la isla.
    pub pill_edge: Edge,
    /// Se pone al nivel del panel de COSMIC (encima) en vez de debajo.
    pub panel_level: bool,
    /// Batería (ícono y porcentaje) a la izquierda de la pastilla cerrada.
    pub pill_battery: bool,
    /// Volumen (ícono y porcentaje) a la derecha de la pastilla cerrada.
    pub pill_volume: bool,
    /// Idioma de la interfaz.
    pub language: Language,
}

/// Idioma de la interfaz: `System` sigue al escritorio (con respaldo en inglés).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    System,
    En,
    Es,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Edge {
    #[default]
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HudStyle {
    #[default]
    Wide,
    Tall,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TermSize {
    #[default]
    Large,
    Medium,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DevLayout {
    #[default]
    Columns,
    Tabs,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DesktopsLayout {
    #[default]
    Rows,
    Columns,
}

/// App fijada a un escritorio de su monitor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pin {
    pub app_id: String,
    /// 1-based: `Workspace.index`, posición del escritorio en su monitor.
    pub workspace: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShelfLayout {
    #[default]
    Grid,
    List,
}

impl Default for MorphConfig {
    fn default() -> Self {
        Self {
            output: None,
            size_percent: 0,
            minimal: false,
            open_on_hover: false,
            clock_12h: false,
            pages: Page::DEFAULT.to_vec(),
            animate: true,
            sounds: true,
            shelf_layout: ShelfLayout::Grid,
            shelf_badge: true,
            shelf_start_empty: false,
            shelf_drag_copy: false,
            desktops_layout: DesktopsLayout::Rows,
            pins: Vec::new(),
            dev_layout: DevLayout::Columns,
            github_repos: Vec::new(),
            gitlab_host: "gitlab.com".into(),
            gitlab_projects: Vec::new(),
            term_size: TermSize::Large,
            term_height_pct: None,
            term_font_size: 13,
            hud_style: HudStyle::Wide,
            hud_volume: true,
            hud_brightness: true,
            notify_battery: true,
            notify_wifi: true,
            notify_bluetooth: true,
            notify_ci: true,
            pill_pos: 0.5,
            hide_line: false,
            pill_edge: Edge::Top,
            panel_level: false,
            pill_battery: false,
            pill_volume: false,
            language: Language::System,
        }
    }
}

/// Porcentaje de escala efectivo: 0 (Auto) o valores no soportados dan 100.
pub fn effective_scale(size_percent: u16) -> u16 {
    match size_percent {
        100 | 115 | 130 | 150 => size_percent,
        _ => 100,
    }
}

/// Monitor pedido si existe entre los disponibles; None ⇒ usar el monitor activo.
pub fn resolve_output(wanted: Option<&str>, available: &[String]) -> Option<String> {
    let wanted = wanted?;
    available.iter().find(|n| n.as_str() == wanted).cloned()
}

/// Config recibida del observador: `pill_pos` y `pill_edge` los escribe la propia app en
/// pareja al soltar la pastilla, y cada eco trae solo una clave nueva con la otra vieja.
/// Estos dos campos solo se toman del eco si su clave está en `keys` (vacío = carga inicial,
/// todo); el resto se toma siempre.
pub fn merge_update(current: &MorphConfig, mut new: MorphConfig, keys: &[&str]) -> MorphConfig {
    if !keys.is_empty() {
        if !keys.contains(&"pill_pos") {
            new.pill_pos = current.pill_pos;
        }
        if !keys.contains(&"pill_edge") {
            new.pill_edge = current.pill_edge;
        }
    }
    new
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_keeps_own_pair_on_stale_echo() {
        let cur = MorphConfig {
            pill_pos: 0.2,
            pill_edge: Edge::Bottom,
            ..MorphConfig::default()
        };
        let echo = MorphConfig {
            pill_pos: 0.2,
            pill_edge: Edge::Top,
            minimal: true,
            ..MorphConfig::default()
        };
        let m = merge_update(&cur, echo, &["pill_pos"]);
        assert_eq!(m.pill_edge, Edge::Bottom);
        assert_eq!(m.pill_pos, 0.2);
        assert!(m.minimal);
    }

    #[test]
    fn merge_takes_listed_keys_and_initial_load() {
        let cur = MorphConfig::default();
        let echo = MorphConfig {
            pill_pos: 0.9,
            pill_edge: Edge::Bottom,
            ..MorphConfig::default()
        };
        let m = merge_update(&cur, echo.clone(), &["pill_edge"]);
        assert_eq!((m.pill_pos, m.pill_edge), (0.5, Edge::Bottom));
        assert_eq!(merge_update(&cur, echo.clone(), &[]), echo);
    }

    #[test]
    fn resolve_output_cases() {
        let edp = vec!["eDP-1".to_string()];
        assert_eq!(resolve_output(Some("HDMI-A-1"), &edp), None);
        let both = vec!["eDP-1".to_string(), "HDMI-A-1".to_string()];
        assert_eq!(
            resolve_output(Some("eDP-1"), &both),
            Some("eDP-1".to_string())
        );
        assert_eq!(resolve_output(None, &both), None);
    }

    #[test]
    fn default_desktops_config() {
        let c = MorphConfig::default();
        assert_eq!(c.desktops_layout, DesktopsLayout::Rows);
        assert!(c.pins.is_empty());
    }

    #[test]
    fn default_dev_config() {
        let c = MorphConfig::default();
        assert_eq!(c.dev_layout, DevLayout::Columns);
        assert!(c.github_repos.is_empty());
        assert_eq!(c.gitlab_host, "gitlab.com");
        assert!(c.gitlab_projects.is_empty());
    }

    #[test]
    fn default_term_config() {
        let c = MorphConfig::default();
        assert_eq!(c.term_size, TermSize::Large);
        assert_eq!(c.term_height_pct, None);
        assert_eq!(c.term_font_size, 13);
    }

    #[test]
    fn default_hud_config() {
        let c = MorphConfig::default();
        assert_eq!(c.hud_style, HudStyle::Wide);
        assert!(c.hud_volume && c.hud_brightness);
    }

    #[test]
    fn default_notify_config() {
        let c = MorphConfig::default();
        assert!(c.notify_battery && c.notify_wifi && c.notify_bluetooth && c.notify_ci);
    }

    #[test]
    fn default_pill_pos() {
        assert_eq!(MorphConfig::default().pill_pos, 0.5);
    }

    #[test]
    fn default_hide_line() {
        assert!(!MorphConfig::default().hide_line);
    }

    #[test]
    fn default_edge_config() {
        let c = MorphConfig::default();
        assert_eq!(c.pill_edge, Edge::Top);
        assert!(!c.panel_level);
        assert!(!c.pill_battery);
        assert!(!c.pill_volume);
    }

    #[test]
    fn default_language_is_system() {
        assert_eq!(MorphConfig::default().language, Language::System);
        assert_eq!(Language::default(), Language::System);
    }

    #[test]
    fn scale_cases() {
        assert_eq!(effective_scale(0), 100);
        assert_eq!(effective_scale(130), 130);
        assert_eq!(effective_scale(42), 100);
    }

    #[test]
    fn default_pages_without_dev() {
        let c = MorphConfig::default();
        assert_eq!(c.pages.len(), 5);
        assert_eq!(c.pages, Page::DEFAULT.to_vec());
        assert!(!c.pages.contains(&Page::Dev));
        assert_eq!(c.output, None);
    }
}
