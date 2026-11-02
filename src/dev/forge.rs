//! Modelo común de las forjas (GitHub y GitLab): solicitudes de cambio (PR/MR) y pipelines.

use super::github::{ChecksState, Pr, ReviewState, Run, RunStatus};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Forge {
    GitHub,
    GitLab,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChangeRequest {
    pub forge: Forge,
    pub project: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub draft: bool,
    pub review: ReviewState,
    pub checks: ChecksState,
    pub conflicts: bool,
    /// false = me piden revisión.
    pub mine: bool,
    pub updated: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pipeline {
    pub forge: Forge,
    pub project: String,
    pub id: u64,
    pub name: String,
    pub branch: String,
    pub status: RunStatus,
    pub conclusion: Option<String>,
    pub url: String,
    pub updated: String,
}

impl From<Pr> for ChangeRequest {
    fn from(p: Pr) -> Self {
        ChangeRequest {
            forge: Forge::GitHub,
            project: p.repo,
            number: p.number,
            title: p.title,
            url: p.url,
            draft: p.draft,
            review: p.review,
            checks: p.checks,
            conflicts: p.conflicts,
            mine: p.mine,
            updated: p.updated,
        }
    }
}

impl From<Run> for Pipeline {
    fn from(r: Run) -> Self {
        Pipeline {
            forge: Forge::GitHub,
            project: r.repo,
            id: r.id,
            name: r.name,
            branch: r.branch,
            status: r.status,
            conclusion: r.conclusion,
            url: r.url,
            updated: r.updated,
        }
    }
}

/// PRs/MRs de ambas forjas: los míos primero, luego por actualización (más reciente antes).
pub fn merge_change_requests(mut all: Vec<ChangeRequest>) -> Vec<ChangeRequest> {
    all.sort_by(|a, b| b.mine.cmp(&a.mine).then_with(|| b.updated.cmp(&a.updated)));
    all
}

/// Pipelines de ambas forjas por actualización (más reciente antes).
pub fn merge_pipelines(mut all: Vec<Pipeline>) -> Vec<Pipeline> {
    all.sort_by(|a, b| b.updated.cmp(&a.updated));
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cr(forge: Forge, n: u64, mine: bool, updated: &str) -> ChangeRequest {
        ChangeRequest {
            forge,
            project: "p".into(),
            number: n,
            title: String::new(),
            url: String::new(),
            draft: false,
            review: ReviewState::None,
            checks: ChecksState::None,
            conflicts: false,
            mine,
            updated: updated.into(),
        }
    }

    #[test]
    fn from_github_types() {
        let pr = Pr {
            repo: "o/r".into(),
            number: 3,
            title: "t".into(),
            url: "u".into(),
            draft: true,
            review: ReviewState::Approved,
            checks: ChecksState::Failure,
            conflicts: true,
            mine: false,
            updated: "2026-10-30".into(),
        };
        let c = ChangeRequest::from(pr);
        assert_eq!(
            (c.forge, c.project.as_str(), c.number),
            (Forge::GitHub, "o/r", 3)
        );
        assert!(c.draft && c.conflicts && !c.mine);
        assert_eq!(c.updated, "2026-10-30");
        let run = Run {
            repo: "o/r".into(),
            id: 9,
            name: "CI".into(),
            branch: "main".into(),
            title: "x".into(),
            status: RunStatus::Queued,
            conclusion: None,
            url: "u".into(),
            updated: "z".into(),
        };
        let p = Pipeline::from(run);
        assert_eq!(
            (p.forge, p.project.as_str(), p.id),
            (Forge::GitHub, "o/r", 9)
        );
        assert_eq!(p.status, RunStatus::Queued);
    }

    #[test]
    fn merged_order_mine_first_then_updated() {
        let out = merge_change_requests(vec![
            cr(Forge::GitHub, 1, false, "2026-10-31"),
            cr(Forge::GitLab, 2, true, "2026-10-01"),
            cr(Forge::GitLab, 3, true, "2026-10-20"),
        ]);
        assert_eq!(out.iter().map(|c| c.number).collect::<Vec<_>>(), [3, 2, 1]);
    }
}
