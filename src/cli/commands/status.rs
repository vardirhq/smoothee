//! `smoothee status` — explain repository state in plain language.
//!
//! This is Smoothee's first and most-run command. It performs deterministic
//! repository inspection and answers the questions developers actually have:
//! what branch am I on, is anything uncommitted, am I ahead or behind, is it
//! safe to push, and what should I do next.

use anyhow::{Context, Result};

use crate::config::RepoConfig;
use crate::git::branches::{self, BaseBranch, BaseBranchSource};
use crate::git::status::WorkingTreeStatus;
use crate::git::Repository;
use crate::ui::output;

/// A fully-assembled, render-ready assessment of repository state.
///
/// Kept as data (rather than printing inline) so the assembly is unit-testable
/// and later commands can reuse the same recommendation logic.
#[derive(Debug, Clone)]
pub struct StatusAssessment {
    pub repo_name: String,
    pub branch: Option<String>,
    pub base: Option<BaseBranch>,
    pub tree: WorkingTreeStatus,
    /// Ahead/behind versus the base branch, when it differs from the current
    /// branch and shares history.
    pub base_divergence: Option<(u32, u32)>,
    pub recommendation: Recommendation,
}

/// The single recommended next step, derived deterministically from state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recommendation {
    /// Repository has no commits yet.
    InitialCommit,
    /// Detached HEAD — not on a branch.
    DetachedHead,
    /// Unresolved merge conflicts are present.
    Resolve,
    /// Behind the base branch; syncing is advised.
    Sync,
    /// Local changes are present and could be committed.
    Commit,
    /// Ahead of upstream with a clean tree; pushing/PR is the next step.
    Push,
    /// Nothing to do.
    UpToDate,
}

impl Recommendation {
    /// The command a user would run to act on this recommendation, if any.
    pub fn command(&self) -> Option<&'static str> {
        match self {
            Recommendation::InitialCommit => Some("smoothee commit"),
            Recommendation::DetachedHead => None,
            Recommendation::Resolve => Some("smoothee resolve"),
            Recommendation::Sync => Some("smoothee sync"),
            Recommendation::Commit => Some("smoothee commit"),
            Recommendation::Push => Some("smoothee pr"),
            Recommendation::UpToDate => None,
        }
    }

    /// A one-line human explanation of the recommendation.
    pub fn summary(&self) -> &'static str {
        match self {
            Recommendation::InitialCommit => "This repository has no commits yet.",
            Recommendation::DetachedHead => {
                "You are not on a branch (detached HEAD). Check out a branch to continue."
            }
            Recommendation::Resolve => "You have merge conflicts that need attention.",
            Recommendation::Sync => "Your branch is behind its base and can be updated safely.",
            Recommendation::Commit => "You have changes that are not committed yet.",
            Recommendation::Push => "Your branch is ahead and ready to share.",
            Recommendation::UpToDate => "Everything is up to date. Nothing to do.",
        }
    }
}

impl StatusAssessment {
    /// Inspect `repo` and build an assessment. Pure aside from Git queries.
    pub fn gather(repo: &Repository, config: &RepoConfig) -> Result<Self> {
        let tree = WorkingTreeStatus::query(repo).context("reading repository status")?;

        let base = BaseBranch::detect(repo, config.base_branch.as_deref())
            .context("detecting base branch")?;

        // Divergence versus the base is only meaningful when we're on a branch
        // that isn't the base itself.
        let base_divergence = match (&tree.branch, &base) {
            (Some(branch), Some(base)) if *branch != base.name => {
                branches::divergence_from_base(repo, branch, &base.name)
                    .context("computing divergence from base")?
            }
            _ => None,
        };

        let recommendation = Self::recommend(&tree, &base_divergence);

        Ok(Self {
            repo_name: repo.name(),
            branch: tree.branch.clone(),
            base,
            tree,
            base_divergence,
            recommendation,
        })
    }

    /// Derive the recommended next step from state. Order encodes priority:
    /// the most urgent, safety-relevant action wins.
    fn recommend(tree: &WorkingTreeStatus, base_divergence: &Option<(u32, u32)>) -> Recommendation {
        if tree.is_initial {
            return Recommendation::InitialCommit;
        }
        if tree.branch.is_none() {
            return Recommendation::DetachedHead;
        }
        if tree.conflicted > 0 {
            return Recommendation::Resolve;
        }
        // Behind the base branch (or upstream) → sync is the safe next move.
        let behind_base = base_divergence.map(|(_, behind)| behind).unwrap_or(0);
        let behind_upstream = tree.ahead_behind.as_ref().map(|ab| ab.behind).unwrap_or(0);
        if behind_base > 0 || behind_upstream > 0 {
            return Recommendation::Sync;
        }
        if !tree.is_clean() {
            return Recommendation::Commit;
        }
        let ahead_upstream = tree.ahead_behind.as_ref().map(|ab| ab.ahead).unwrap_or(0);
        if ahead_upstream > 0 {
            return Recommendation::Push;
        }
        Recommendation::UpToDate
    }

    /// Render the assessment as the plain-language block users see.
    pub fn render(&self) -> String {
        let mut out = String::new();

        out.push_str(&output::heading(&format!("Repository: {}", self.repo_name)));
        out.push('\n');
        match &self.branch {
            Some(b) => out.push_str(&format!("{} {}\n", output::label("Branch:"), b)),
            None => out.push_str(&format!(
                "{} {}\n",
                output::label("Branch:"),
                "(detached HEAD)"
            )),
        }
        if let Some(base) = &self.base {
            let note = match base.source {
                BaseBranchSource::Configured => "",
                BaseBranchSource::RemoteHead => " (from origin)",
                BaseBranchSource::Conventional => " (guessed)",
            };
            out.push_str(&format!(
                "{} {}{}\n",
                output::label("Base branch:"),
                base.name,
                output::label(note)
            ));
        }

        // Working tree.
        out.push('\n');
        out.push_str(&output::label("Working tree:"));
        out.push('\n');
        if self.tree.is_clean() {
            out.push_str(&output::bullet("clean"));
            out.push('\n');
        } else {
            for (count, noun) in [
                (self.tree.staged, "staged"),
                (self.tree.modified, "modified"),
                (self.tree.conflicted, "conflicted"),
                (self.tree.untracked, "untracked"),
            ] {
                if count > 0 {
                    out.push_str(&output::bullet(&format!(
                        "{count} {noun} file{}",
                        plural(count)
                    )));
                    out.push('\n');
                }
            }
        }

        // Branch divergence.
        out.push('\n');
        out.push_str(&output::label("Branch state:"));
        out.push('\n');
        let mut printed_state = false;
        if let Some(ab) = &self.tree.ahead_behind {
            if let Some(upstream) = &self.tree.upstream {
                out.push_str(&output::bullet(&describe_divergence(
                    ab.ahead, ab.behind, upstream,
                )));
                out.push('\n');
                printed_state = true;
            }
        }
        if let (Some((ahead, behind)), Some(base)) = (self.base_divergence, &self.base) {
            out.push_str(&output::bullet(&describe_divergence(
                ahead, behind, &base.name,
            )));
            out.push('\n');
            printed_state = true;
        }
        if !printed_state {
            out.push_str(&output::bullet("no upstream configured"));
            out.push('\n');
        }

        // Recommendation.
        out.push('\n');
        out.push_str(&output::label("Recommended next step:"));
        out.push('\n');
        out.push_str(&format!("  {}\n", self.recommendation.summary()));
        if let Some(cmd) = self.recommendation.command() {
            out.push_str(&format!("  {}\n", output::recommend(cmd)));
        }

        out
    }
}

/// Describe ahead/behind counts relative to a named ref in plain language.
fn describe_divergence(ahead: u32, behind: u32, reference: &str) -> String {
    match (ahead, behind) {
        (0, 0) => format!("up to date with {reference}"),
        (a, 0) => format!("{a} commit{} ahead of {reference}", plural(a)),
        (0, b) => format!("{b} commit{} behind {reference}", plural(b)),
        (a, b) => format!(
            "{a} commit{} ahead, {b} commit{} behind {reference}",
            plural(a),
            plural(b)
        ),
    }
}

fn plural(n: u32) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Entry point for the `status` subcommand.
pub fn run() -> Result<()> {
    let repo = Repository::discover_from_cwd()
        .context("this does not look like a Git repository (or git is not installed)")?;
    let config = RepoConfig::load(repo.workdir()).context("loading .smoothee.toml")?;

    let assessment = StatusAssessment::gather(&repo, &config)?;
    print!("{}", assessment.render());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::status::AheadBehind;

    fn tree() -> WorkingTreeStatus {
        WorkingTreeStatus {
            branch: Some("feature".into()),
            ..Default::default()
        }
    }

    #[test]
    fn recommends_resolve_when_conflicts_present() {
        let mut t = tree();
        t.conflicted = 2;
        assert_eq!(
            StatusAssessment::recommend(&t, &None),
            Recommendation::Resolve
        );
    }

    #[test]
    fn recommends_sync_when_behind_base() {
        let t = tree();
        assert_eq!(
            StatusAssessment::recommend(&t, &Some((1, 3))),
            Recommendation::Sync
        );
    }

    #[test]
    fn recommends_sync_when_behind_upstream() {
        let mut t = tree();
        t.ahead_behind = Some(AheadBehind {
            ahead: 0,
            behind: 2,
        });
        assert_eq!(StatusAssessment::recommend(&t, &None), Recommendation::Sync);
    }

    #[test]
    fn recommends_commit_when_dirty_and_current() {
        let mut t = tree();
        t.modified = 1;
        assert_eq!(
            StatusAssessment::recommend(&t, &Some((0, 0))),
            Recommendation::Commit
        );
    }

    #[test]
    fn recommends_push_when_ahead_and_clean() {
        let mut t = tree();
        t.ahead_behind = Some(AheadBehind {
            ahead: 2,
            behind: 0,
        });
        assert_eq!(
            StatusAssessment::recommend(&t, &Some((2, 0))),
            Recommendation::Push
        );
    }

    #[test]
    fn recommends_nothing_when_up_to_date() {
        let mut t = tree();
        t.ahead_behind = Some(AheadBehind {
            ahead: 0,
            behind: 0,
        });
        assert_eq!(
            StatusAssessment::recommend(&t, &Some((0, 0))),
            Recommendation::UpToDate
        );
    }

    #[test]
    fn conflicts_outrank_being_behind() {
        // Safety-first ordering: a conflict must win over a sync suggestion.
        let mut t = tree();
        t.conflicted = 1;
        assert_eq!(
            StatusAssessment::recommend(&t, &Some((0, 5))),
            Recommendation::Resolve
        );
    }

    #[test]
    fn detached_head_is_flagged() {
        let mut t = tree();
        t.branch = None;
        assert_eq!(
            StatusAssessment::recommend(&t, &None),
            Recommendation::DetachedHead
        );
    }

    #[test]
    fn divergence_phrasing_is_singular_and_plural() {
        assert_eq!(describe_divergence(1, 0, "main"), "1 commit ahead of main");
        assert_eq!(describe_divergence(0, 2, "main"), "2 commits behind main");
        assert_eq!(
            describe_divergence(3, 4, "main"),
            "3 commits ahead, 4 commits behind main"
        );
        assert_eq!(describe_divergence(0, 0, "main"), "up to date with main");
    }
}
