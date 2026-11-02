//! Escritorios: lista de escritorios/ventanas por monitor vía cctk y comandos de activación.

pub mod apps;
pub mod pins;
pub mod view;
mod wayland;

use std::sync::atomic::AtomicBool;

pub use wayland::{send, subscription};

/// La UI muestra títulos (isla abierta en Escritorios): solo entonces se publican
/// cambios de solo título.
pub static TITLES_WANTED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    /// Índice interno asignado por el hilo Wayland (estable mientras vive el objeto).
    pub id: u32,
    pub name: String,
    pub active: bool,
    /// Nombre del wl_output del grupo, p. ej. "eDP-1".
    pub output: Option<String>,
    /// Posición 1-based dentro de su grupo/monitor.
    pub index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    /// Índice interno asignado por el hilo Wayland (estable mientras vive el objeto).
    pub id: u32,
    pub app_id: String,
    pub title: String,
    pub workspace: Option<u32>,
    /// Tiene el estado `Activated` del toplevel.
    pub active: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub workspaces: Vec<Workspace>,
    pub windows: Vec<Window>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopsEvent {
    Snapshot(Snapshot),
}

#[derive(Clone, Debug)]
pub enum DesktopsCmd {
    ActivateWorkspace(u32),
    ActivateWindow(u32),
    /// Despierta el hilo para republicar la instantánea actual (no toca ventanas).
    Refresh,
}

/// Ordena los escritorios de un grupo como COSMIC: por `coordinates` (lexicográfico);
/// sin coordenadas, por nombre numérico; el resto conserva el orden del protocolo.
pub fn sort_group<T>(items: &mut [T], key: impl Fn(&T) -> (&[u32], &str)) {
    items.sort_by_key(|a| order_key(key(a)));
}

fn order_key((coords, name): (&[u32], &str)) -> (bool, Vec<u32>, bool, u64) {
    let num = name.trim().parse::<u64>().ok();
    (
        coords.is_empty(),
        coords.to_vec(),
        num.is_none(),
        num.unwrap_or(0),
    )
}

/// Asigna `index` 1-based dentro de cada monitor, en el orden dado.
pub fn number_by_output(ws: &mut [Workspace]) {
    for i in 0..ws.len() {
        let n = ws[..i].iter().filter(|w| w.output == ws[i].output).count();
        ws[i].index = n as u32 + 1;
    }
}

/// Escritorios del monitor dado; `None` o sin coincidencias → el primer grupo.
pub fn filter_by_output(ws: &[Workspace], output: Option<&str>) -> Vec<Workspace> {
    let Some(first) = ws.first() else {
        return Vec::new();
    };
    let target = output
        .filter(|o| ws.iter().any(|w| w.output.as_deref() == Some(o)))
        .map(|o| Some(o.to_string()))
        .unwrap_or_else(|| first.output.clone());
    ws.iter().filter(|w| w.output == target).cloned().collect()
}

/// Monitor cuyos escritorios se muestran. Con monitor explícito, ese. Sin él
/// (monitor "activo"): COSMIC coloca la superficie de capa de "salida activa" en el
/// monitor con foco, que se aproxima con el grupo del escritorio de la ventana activada;
/// si no hay ninguna, el primer grupo (`filter_by_output` con `None`).
pub fn pick_output(snap: &Snapshot, surface_output: Option<&str>) -> Option<String> {
    if let Some(o) = surface_output {
        return Some(o.to_string());
    }
    let ws_id = snap.windows.iter().find(|w| w.active)?.workspace?;
    snap.workspaces
        .iter()
        .find(|w| w.id == ws_id)?
        .output
        .clone()
}

/// Escritorios y ventanas visibles para el monitor de la isla.
pub fn visible(snap: &Snapshot, surface_output: Option<&str>) -> (Vec<Workspace>, Vec<Window>) {
    let out = pick_output(snap, surface_output);
    let ws = filter_by_output(&snap.workspaces, out.as_deref());
    let wins = snap
        .windows
        .iter()
        .filter(|w| w.workspace.is_none_or(|id| ws.iter().any(|x| x.id == id)))
        .cloned()
        .collect();
    (ws, wins)
}

/// ¿Cambió algo más que títulos? (ventanas/escritorios/activo/app_id)
pub fn significant_change(old: &Snapshot, new: &Snapshot) -> bool {
    fn strip(s: &Snapshot) -> Snapshot {
        let mut s = s.clone();
        for w in &mut s.windows {
            w.title.clear();
        }
        s
    }
    strip(old) != strip(new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(id: u32) -> Workspace {
        Workspace {
            id,
            name: format!("Escritorio {id}"),
            active: id == 1,
            output: None,
            index: id,
        }
    }

    fn wso(id: u32, output: &str) -> Workspace {
        Workspace {
            output: Some(output.into()),
            index: 0,
            ..ws(id)
        }
    }

    fn win(id: u32, app: &str, workspace: Option<u32>) -> Window {
        Window {
            id,
            app_id: app.into(),
            title: app.into(),
            workspace,
            active: false,
        }
    }

    fn two_monitors() -> Vec<Workspace> {
        let mut v = vec![
            wso(1, "eDP-1"),
            wso(2, "eDP-1"),
            wso(3, "HDMI-A-1"),
            wso(4, "HDMI-A-1"),
            wso(5, "HDMI-A-1"),
        ];
        number_by_output(&mut v);
        v
    }

    #[test]
    fn indices_son_1_based_por_grupo() {
        let v = two_monitors();
        let idx: Vec<u32> = v.iter().map(|w| w.index).collect();
        assert_eq!(idx, [1, 2, 1, 2, 3]);
    }

    fn names(v: &[(Vec<u32>, String)]) -> Vec<&str> {
        v.iter().map(|(_, n)| n.as_str()).collect()
    }

    fn sorted(mut v: Vec<(Vec<u32>, String)>) -> Vec<(Vec<u32>, String)> {
        sort_group(&mut v, |(c, n)| (c.as_slice(), n.as_str()));
        v
    }

    fn item(c: &[u32], n: &str) -> (Vec<u32>, String) {
        (c.to_vec(), n.to_string())
    }

    #[test]
    fn ordena_por_coordenadas() {
        let v = sorted(vec![
            item(&[2], "3"),
            item(&[1], "2"),
            item(&[4], "5"),
            item(&[0], "1"),
            item(&[3], "4"),
        ]);
        assert_eq!(names(&v), ["1", "2", "3", "4", "5"]);
    }

    #[test]
    fn sin_coordenadas_ordena_por_nombre_numerico() {
        let v = sorted(vec![
            item(&[], "10"),
            item(&[], "3"),
            item(&[], "2"),
            item(&[], "1"),
        ]);
        assert_eq!(names(&v), ["1", "2", "3", "10"]);
    }

    #[test]
    fn nombres_no_numericos_conservan_orden() {
        let v = sorted(vec![item(&[], "b"), item(&[], "a"), item(&[], "z")]);
        assert_eq!(names(&v), ["b", "a", "z"]);
    }

    fn two_mon_snap(active_on: Option<u32>) -> Snapshot {
        let mut w = win(10, "code", active_on);
        w.active = active_on.is_some();
        Snapshot {
            workspaces: two_monitors(),
            windows: vec![w],
        }
    }

    #[test]
    fn pick_output_explicito_gana() {
        let s = two_mon_snap(Some(3));
        assert_eq!(pick_output(&s, Some("eDP-1")).as_deref(), Some("eDP-1"));
    }

    #[test]
    fn pick_output_usa_monitor_de_la_ventana_activa() {
        let s = two_mon_snap(Some(3));
        assert_eq!(pick_output(&s, None).as_deref(), Some("HDMI-A-1"));
        let (ws, wins) = visible(&s, None);
        assert_eq!(ws.len(), 3);
        assert_eq!(wins.len(), 1);
    }

    #[test]
    fn pick_output_sin_ventana_activa_es_primer_grupo() {
        let mut s = two_mon_snap(Some(3));
        s.windows[0].active = false;
        assert_eq!(pick_output(&s, None), None);
        let (ws, wins) = visible(&s, None);
        assert_eq!(ws.iter().map(|w| w.id).collect::<Vec<_>>(), [1, 2]);
        assert!(wins.is_empty());
    }

    #[test]
    fn filtra_por_monitor() {
        let v = two_monitors();
        let f = filter_by_output(&v, Some("HDMI-A-1"));
        assert_eq!(f.iter().map(|w| w.id).collect::<Vec<_>>(), [3, 4, 5]);
    }

    #[test]
    fn none_o_desconocido_da_el_primer_grupo() {
        let v = two_monitors();
        for o in [None, Some("DP-9")] {
            let f = filter_by_output(&v, o);
            assert_eq!(f.iter().map(|w| w.id).collect::<Vec<_>>(), [1, 2]);
        }
        assert!(filter_by_output(&[], None).is_empty());
    }

    fn snap() -> Snapshot {
        Snapshot {
            workspaces: vec![ws(1), ws(2)],
            windows: vec![win(10, "code", Some(1))],
        }
    }

    #[test]
    fn solo_titulo_no_es_significativo() {
        let a = snap();
        let mut b = snap();
        b.windows[0].title = "otro".into();
        assert!(!significant_change(&a, &b));
        assert!(!significant_change(&a, &a));
    }

    #[test]
    fn otros_cambios_son_significativos() {
        let a = snap();
        let mut b = snap();
        b.windows[0].active = true;
        assert!(significant_change(&a, &b));
        let mut b = snap();
        b.windows[0].app_id = "x".into();
        assert!(significant_change(&a, &b));
        let mut b = snap();
        b.windows[0].workspace = Some(2);
        assert!(significant_change(&a, &b));
        let mut b = snap();
        b.windows.push(win(11, "y", None));
        assert!(significant_change(&a, &b));
        let mut b = snap();
        b.workspaces[1].active = true;
        assert!(significant_change(&a, &b));
    }
}
