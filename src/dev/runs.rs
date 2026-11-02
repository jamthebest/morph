//! Transiciones de ejecuciones de Actions entre dos lecturas (para avisos de CI).

use super::forge::Pipeline;
use super::github::RunStatus;

#[derive(Clone, Debug, PartialEq)]
pub enum CiEvent {
    Failed(Pipeline),
    Finished(Pipeline),
}

/// Eventos de las ejecuciones que estaban en cola/en curso en `prev` y ahora están
/// completadas con éxito (`Finished`) o fallo/timeout (`Failed`); otras conclusiones no
/// avisan. Sin `prev` (primera carga) o ejecuciones nuevas ya completadas: nada.
pub fn transitions(prev: &[Pipeline], now: &[Pipeline]) -> Vec<CiEvent> {
    let pending = |r: &Pipeline| matches!(r.status, RunStatus::Queued | RunStatus::InProgress);
    now.iter()
        .filter(|r| r.status == RunStatus::Completed)
        .filter(|r| {
            prev.iter()
                .any(|p| p.forge == r.forge && p.project == r.project && p.id == r.id && pending(p))
        })
        .filter_map(|r| match r.conclusion.as_deref() {
            Some("success") => Some(CiEvent::Finished(r.clone())),
            Some("failure" | "timed_out") => Some(CiEvent::Failed(r.clone())),
            // cancelled, skipped, neutral, action_required, stale...: sin aviso.
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::dev::forge::Forge;

    fn run(id: u64, status: RunStatus, conclusion: Option<&str>) -> Pipeline {
        Pipeline {
            forge: Forge::GitHub,
            project: "o/r".into(),
            id,
            name: "CI".into(),
            branch: "main".into(),
            status,
            conclusion: conclusion.map(Into::into),
            url: format!("https://github.com/o/r/actions/runs/{id}"),
            updated: "2026-10-30T10:00:00Z".into(),
        }
    }

    #[test]
    fn first_load_has_no_events() {
        let now = [run(1, RunStatus::Completed, Some("failure"))];
        assert!(transitions(&[], &now).is_empty());
    }

    #[test]
    fn in_progress_to_success_is_finished() {
        let prev = [run(1, RunStatus::InProgress, None)];
        let now = [run(1, RunStatus::Completed, Some("success"))];
        assert_eq!(
            transitions(&prev, &now),
            vec![CiEvent::Finished(now[0].clone())]
        );
    }

    #[test]
    fn queued_to_failure_is_failed() {
        let prev = [run(1, RunStatus::Queued, None)];
        let now = [run(1, RunStatus::Completed, Some("failure"))];
        assert_eq!(
            transitions(&prev, &now),
            vec![CiEvent::Failed(now[0].clone())]
        );
        let now = [run(1, RunStatus::Completed, Some("timed_out"))];
        assert!(matches!(transitions(&prev, &now)[..], [CiEvent::Failed(_)]));
    }

    #[test]
    fn completed_to_completed_and_new_completed_do_nothing() {
        let prev = [run(1, RunStatus::Completed, Some("failure"))];
        let now = [
            run(1, RunStatus::Completed, Some("failure")),
            run(2, RunStatus::Completed, Some("failure")),
        ];
        assert!(transitions(&prev, &now).is_empty());
    }

    #[test]
    fn other_conclusions_emit_nothing() {
        let prev = [run(1, RunStatus::InProgress, None)];
        for c in [
            "cancelled",
            "skipped",
            "neutral",
            "action_required",
            "stale",
            "startup_failure",
        ] {
            let now = [run(1, RunStatus::Completed, Some(c))];
            assert!(transitions(&prev, &now).is_empty(), "{c}");
        }
        let now = [run(1, RunStatus::Completed, None)];
        assert!(transitions(&prev, &now).is_empty());
    }

    #[test]
    fn still_running_does_nothing() {
        let prev = [run(1, RunStatus::Queued, None)];
        let now = [run(1, RunStatus::InProgress, None)];
        assert!(transitions(&prev, &now).is_empty());
    }

    #[test]
    fn keyed_by_forge_and_project_not_just_id() {
        let mut gl_prev = run(1, RunStatus::InProgress, None);
        gl_prev.forge = Forge::GitLab;
        gl_prev.project = "g/p".into();
        // Un run de GitHub con el mismo id ya completado no confunde a GitLab, ni al revés.
        let gh_now = run(1, RunStatus::Completed, Some("failure"));
        assert!(transitions(std::slice::from_ref(&gl_prev), &[gh_now]).is_empty());
        let mut gl_now = gl_prev.clone();
        gl_now.status = RunStatus::Completed;
        gl_now.conclusion = Some("failure".into());
        assert_eq!(
            transitions(
                &[
                    gl_prev.clone(),
                    run(1, RunStatus::Completed, Some("success"))
                ],
                std::slice::from_ref(&gl_now)
            ),
            vec![CiEvent::Failed(gl_now.clone())]
        );
        // Mismo id y forge en otro proyecto: no es la misma ejecución.
        let mut other = gl_now.clone();
        other.project = "g/otro".into();
        assert!(transitions(&[gl_prev], &[other]).is_empty());
    }
}
