//! Fijaciones de apps a escritorios y filas por escritorio (lógica pura).

use crate::config::Pin;
use crate::desktops::{Window, Workspace};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub ws: Workspace,
    pub windows: Vec<Window>,
    /// app_ids fijados a este escritorio que no están abiertos en ninguna parte.
    pub pinned_missing: Vec<String>,
}

/// Filas para los escritorios (ya filtrados a un monitor) y sus ventanas.
pub fn rows(ws: &[Workspace], wins: &[Window], pins: &[Pin]) -> Vec<Row> {
    ws.iter()
        .map(|w| Row {
            ws: w.clone(),
            windows: wins
                .iter()
                .filter(|win| win.workspace == Some(w.id))
                .cloned()
                .collect(),
            pinned_missing: pins
                .iter()
                .filter(|p| p.workspace == w.index)
                .filter(|p| {
                    !wins
                        .iter()
                        .any(|win| win.app_id.eq_ignore_ascii_case(&p.app_id))
                })
                .map(|p| p.app_id.clone())
                .collect(),
        })
        .collect()
}

/// Fija la app a `workspace`; si ya estaba ahí, la quita; si estaba en otro, la mueve.
pub fn toggle_pin(pins: &mut Vec<Pin>, app_id: &str, workspace: u32) {
    if app_id.is_empty() {
        return;
    }
    match pins
        .iter()
        .position(|p| p.app_id.eq_ignore_ascii_case(app_id))
    {
        Some(i) if pins[i].workspace == workspace => {
            pins.remove(i);
        }
        Some(i) => pins[i].workspace = workspace,
        None => pins.push(Pin {
            app_id: app_id.into(),
            workspace,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(id: u32, index: u32) -> Workspace {
        Workspace {
            id,
            name: index.to_string(),
            active: false,
            output: None,
            index,
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

    fn pin(app: &str, workspace: u32) -> Pin {
        Pin {
            app_id: app.into(),
            workspace,
        }
    }

    #[test]
    fn fijada_no_abierta_aparece_en_su_escritorio() {
        let r = rows(&[ws(10, 1), ws(11, 2)], &[], &[pin("kitty", 2)]);
        assert!(r[0].pinned_missing.is_empty());
        assert_eq!(r[1].pinned_missing, ["kitty"]);
    }

    #[test]
    fn fijada_abierta_en_otro_escritorio_no_es_faltante() {
        let r = rows(
            &[ws(10, 1), ws(11, 2)],
            &[win(1, "Kitty", Some(10))],
            &[pin("kitty", 2)],
        );
        assert!(r[1].pinned_missing.is_empty());
        assert_eq!(r[0].windows.len(), 1);
        assert!(r[0].pinned_missing.is_empty());
    }

    #[test]
    fn fijada_a_escritorio_inexistente_se_ignora() {
        let r = rows(
            &[ws(1, 1), ws(2, 2), ws(3, 3), ws(4, 4)],
            &[],
            &[pin("x", 5)],
        );
        assert!(r.iter().all(|row| row.pinned_missing.is_empty()));
    }

    #[test]
    fn ventanas_van_a_su_fila_y_huerfanas_no_aparecen() {
        let r = rows(
            &[ws(10, 1), ws(11, 2)],
            &[win(1, "a", Some(11)), win(2, "b", None)],
            &[],
        );
        assert!(r[0].windows.is_empty());
        assert_eq!(r[1].windows[0].id, 1);
    }

    #[test]
    fn toggle_fija_mueve_y_quita() {
        let mut p = Vec::new();
        toggle_pin(&mut p, "kitty", 1);
        assert_eq!(p, [pin("kitty", 1)]);
        toggle_pin(&mut p, "code", 1);
        toggle_pin(&mut p, "kitty", 3);
        assert_eq!(p, [pin("kitty", 3), pin("code", 1)]);
        toggle_pin(&mut p, "kitty", 3);
        assert_eq!(p, [pin("code", 1)]);
    }

    #[test]
    fn toggle_ignora_mayusculas() {
        let mut p = vec![pin("Code", 1)];
        toggle_pin(&mut p, "code", 1);
        assert!(p.is_empty());
        let mut p = vec![pin("Code", 1)];
        toggle_pin(&mut p, "code", 2);
        assert_eq!(p, [pin("Code", 2)]);
    }

    #[test]
    fn toggle_ignora_app_id_vacio() {
        let mut p = Vec::new();
        toggle_pin(&mut p, "", 1);
        assert!(p.is_empty());
    }
}
