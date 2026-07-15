//! `smoothee sync` — safely update the current branch against its base.
//!
//! This is the first *mutating* command, so it is where Smoothee's safety
//! principles become concrete:
//!
//! * **Explain before acting.** [`SyncEngine::plan`] performs only
//!   non-destructive inspection (fetch + divergence analysis) and returns a
//!   [`SyncPlan`] the caller renders for approval. Nothing in the working tree
//!   changes until [`SyncEngine::execute`] runs.
//! * **Safe by default.** `execute` creates a restore point *before* the
//!   rebase/merge and journals the operation before it runs, so a crash or a
//!   bad outcome is always recoverable through `smoothee undo`.
//! * **Refuse ambiguity gracefully.** When the operation stops on conflicts,
//!   Smoothee leaves the repository exactly as Git left it and hands the user
//!   clear, reversible options rather than guessing.

use crate::config::repository::SyncStrategy;
use crate::config::RepoConfig;
use crate::git::branches::{self, BaseBranch};
use crate::git::command::GitCommand;
use crate::git::restore::RestorePoint;
use crate::git::Repository;

use super::journal::{BeforeState, Journal, OperationRecord};

/// A concrete synchronization strategy, after `auto` has been resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedStrategy {
    /// Replay local commits on top of the base (`git rebase`).
    Rebase,
    /// Join the histories with a merge commit (`git merge`).
    Merge,
}

impl ResolvedStrategy {
    /// The lowercase verb, for prose ("Rebasing…", "…using merge").
    pub fn verb(self) -> &'static str {
        match self {
            ResolvedStrategy::Rebase => "rebase",
            ResolvedStrategy::Merge => "merge",
        }
    }

    /// The capitalised label, for headings ("Rebase onto…").
    pub fn label(self) -> &'static str {
        match self {
            ResolvedStrategy::Rebase => "Rebase",
            ResolvedStrategy::Merge => "Merge",
        }
    }

    /// The journal `type` recorded for this strategy.
    fn journal_kind(self) -> &'static str {
        match self {
            ResolvedStrategy::Rebase => "sync_rebase",
            ResolvedStrategy::Merge => "sync_merge",
        }
    }
}

/// Choose merge vs. rebase and explain why — the deterministic recommendation
/// at the heart of `sync`. Pure so it can be exhaustively unit-tested.
///
/// Precedence: an explicit CLI override wins; then a configured strategy; then
/// `auto`, which rebases private branches (clean history) and merges shared
/// ones (never rewrite published commits).
pub fn choose_strategy(
    configured: SyncStrategy,
    cli_override: Option<ResolvedStrategy>,
    shared: bool,
) -> (ResolvedStrategy, String) {
    if let Some(strategy) = cli_override {
        let reason = match strategy {
            ResolvedStrategy::Rebase => "You asked for a rebase with --rebase.",
            ResolvedStrategy::Merge => "You asked for a merge with --merge.",
        };
        return (strategy, reason.to_string());
    }

    match configured {
        SyncStrategy::Rebase => (
            ResolvedStrategy::Rebase,
            "Your .smoothee.toml sets sync_strategy = \"rebase\".".to_string(),
        ),
        SyncStrategy::Merge => (
            ResolvedStrategy::Merge,
            "Your .smoothee.toml sets sync_strategy = \"merge\".".to_string(),
        ),
        SyncStrategy::Auto if shared => (
            ResolvedStrategy::Merge,
            "Your branch has been pushed, so others may have it. Merging avoids \
             rewriting history they might already share."
                .to_string(),
        ),
        SyncStrategy::Auto => (
            ResolvedStrategy::Rebase,
            "Your branch has not been shared, so rebasing keeps its history clean \
             without an unnecessary merge commit."
                .to_string(),
        ),
    }
}

/// A ready-to-approve synchronization plan: what Smoothee found and what it
/// intends to do. Produced by inspection only; holds no side effects.
#[derive(Debug, Clone)]
pub struct SyncPlan {
    /// The branch being synchronized.
    pub branch: String,
    /// The base branch name (e.g. `main`).
    pub base_name: String,
    /// The remote-tracking ref the operation targets (e.g. `origin/main`).
    pub remote_ref: String,
    /// Commits the branch is ahead of the base.
    pub ahead: u32,
    /// Commits the branch is behind the base.
    pub behind: u32,
    /// The resolved strategy.
    pub strategy: ResolvedStrategy,
    /// Why that strategy was chosen.
    pub reason: String,
}

impl SyncPlan {
    /// The Git command `execute` will run to perform the synchronization,
    /// rendered for the "Running:" transparency block before it runs.
    pub fn mutation_command(&self, repo: &Repository) -> GitCommand {
        match self.strategy {
            ResolvedStrategy::Rebase => repo.git("rebase").arg(&self.remote_ref),
            ResolvedStrategy::Merge => repo.git("merge").arg("--no-edit").arg(&self.remote_ref),
        }
    }
}

/// The result of planning: either there is nothing to do, or a plan awaits
/// approval.
#[derive(Debug, Clone)]
pub enum SyncPlanOutcome {
    /// The branch is not behind its base; no synchronization is needed.
    AlreadyUpToDate { ahead: u32, base_name: String },
    /// A plan is ready for the user to approve.
    Planned(SyncPlan),
}

/// The result of executing an approved plan.
#[derive(Debug)]
pub enum SyncResult {
    /// The synchronization completed and the working tree is clean.
    Completed {
        restore: RestorePoint,
        strategy: ResolvedStrategy,
    },
    /// The operation stopped on conflicts. The repository is left mid-operation
    /// (as Git leaves it) and the restore point is the way back.
    Conflicted {
        restore: RestorePoint,
        files: Vec<String>,
        strategy: ResolvedStrategy,
    },
}

/// Errors specific to synchronization, kept distinct so the CLI can phrase them
/// helpfully rather than surfacing raw Git failures.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("you are not on a branch (detached HEAD); check out a branch before syncing")]
    DetachedHead,
    #[error("this repository has no commits yet; make a commit before syncing")]
    NoCommits,
    #[error(
        "could not determine a base branch to sync against (set base_branch in .smoothee.toml)"
    )]
    NoBaseBranch,
    #[error("no remote branch {0} was found after fetching (has the base been pushed?)")]
    MissingRemoteRef(String),
    #[error("your branch and {0} share no history, so they cannot be synchronized")]
    UnrelatedHistories(String),
    #[error(
        "the `{strategy}` failed and Smoothee restored your branch to where it was.\n{stderr}"
    )]
    OperationFailed {
        strategy: &'static str,
        stderr: String,
    },
    #[error(transparent)]
    Git(#[from] crate::git::command::GitError),
    #[error(transparent)]
    Journal(#[from] super::journal::JournalError),
}

/// Drives synchronization for one repository. Borrows the pieces it needs so
/// the CLI owns their lifetimes.
pub struct SyncEngine<'a> {
    repo: &'a Repository,
    config: &'a RepoConfig,
    journal: &'a Journal,
    remote: String,
}

impl<'a> SyncEngine<'a> {
    /// Create an engine targeting the given remote (conventionally `origin`).
    pub fn new(repo: &'a Repository, config: &'a RepoConfig, journal: &'a Journal) -> Self {
        Self {
            repo,
            config,
            journal,
            remote: "origin".to_string(),
        }
    }

    /// Inspect the repository and build a plan. Non-destructive: it fetches the
    /// base branch (updating remote-tracking refs only) and computes divergence,
    /// but touches neither the working tree nor local branch pointers.
    pub fn plan(
        &self,
        cli_override: Option<ResolvedStrategy>,
    ) -> Result<SyncPlanOutcome, SyncError> {
        let branch = self.repo.current_branch()?.ok_or(SyncError::DetachedHead)?;

        // An unborn branch (no commits yet) has nothing to synchronize and would
        // make every later Git query fail cryptically; guard it up front.
        let has_commits = self
            .repo
            .git("rev-parse")
            .arg("--verify")
            .arg("--quiet")
            .arg("HEAD")
            .succeeds()?;
        if !has_commits {
            return Err(SyncError::NoCommits);
        }

        let base = BaseBranch::detect(self.repo, self.config.base_branch.as_deref())?
            .ok_or(SyncError::NoBaseBranch)?;

        // Fetch the base so divergence is measured against the real remote state.
        self.repo
            .git("fetch")
            .arg(&self.remote)
            .arg(&base.name)
            .output()?;

        let remote_ref = format!("{}/{}", self.remote, base.name);
        let remote_exists = self
            .repo
            .git("rev-parse")
            .arg("--verify")
            .arg("--quiet")
            .arg(format!("{remote_ref}^{{commit}}"))
            .succeeds()?;
        if !remote_exists {
            return Err(SyncError::MissingRemoteRef(remote_ref));
        }

        let (ahead, behind) = branches::divergence_from_base(self.repo, "HEAD", &remote_ref)?
            .ok_or_else(|| SyncError::UnrelatedHistories(remote_ref.clone()))?;

        if behind == 0 {
            return Ok(SyncPlanOutcome::AlreadyUpToDate {
                ahead,
                base_name: base.name,
            });
        }

        let shared = self.branch_is_shared(&branch)?;
        let (strategy, reason) = choose_strategy(self.config.sync_strategy, cli_override, shared);

        Ok(SyncPlanOutcome::Planned(SyncPlan {
            branch,
            base_name: base.name,
            remote_ref,
            ahead,
            behind,
            strategy,
            reason,
        }))
    }

    /// Execute an approved plan.
    ///
    /// Ordering matters for safety: capture HEAD, create the restore point,
    /// journal the (started) operation, *then* mutate. On conflict the
    /// repository is left mid-operation for the user to resolve or undo. On an
    /// unexpected failure the branch is reset back to the restore point.
    pub fn execute(&self, plan: &SyncPlan) -> Result<SyncResult, SyncError> {
        let head_before = self.repo.head()?;
        let restore = RestorePoint::create(self.repo, &plan.branch, &head_before)?;

        let record = OperationRecord::begin(
            plan.strategy.journal_kind(),
            self.repo.workdir().display().to_string(),
            &plan.branch,
            BeforeState {
                head: head_before.clone(),
                restore_ref: Some(restore.ref_name.clone()),
            },
        );
        self.journal.append(&record)?;

        let outcome = plan.mutation_command(self.repo).run()?;

        let conflicts = self.repo.conflicted_files()?;
        let in_progress = self.repo.rebase_in_progress() || self.repo.merge_in_progress();

        if !conflicts.is_empty() || in_progress {
            // Leave the record in its `Started` state: the operation is genuinely
            // in progress and `undo` treats that as recoverable.
            return Ok(SyncResult::Conflicted {
                restore,
                files: conflicts,
                strategy: plan.strategy,
            });
        }

        if !outcome.success {
            // Failed without leaving a resolvable conflict (e.g. refused for a
            // reason Git surfaced on stderr). Restore to safety before reporting.
            self.repo
                .git("reset")
                .arg("--hard")
                .arg(&head_before)
                .output()?;
            self.journal.append(&record.clone().fail())?;
            return Err(SyncError::OperationFailed {
                strategy: plan.strategy.verb(),
                stderr: outcome.stderr,
            });
        }

        let head_after = self.repo.head()?;
        self.journal.append(&record.complete(head_after))?;
        Ok(SyncResult::Completed {
            restore,
            strategy: plan.strategy,
        })
    }

    /// A branch is "shared" once it has been published — i.e. a remote-tracking
    /// ref exists for it. Rebasing a shared branch rewrites history others may
    /// hold, so this drives the `auto` merge-vs-rebase choice.
    fn branch_is_shared(&self, branch: &str) -> Result<bool, SyncError> {
        Ok(self
            .repo
            .git("rev-parse")
            .arg("--verify")
            .arg("--quiet")
            .arg(format!("refs/remotes/{}/{}", self.remote, branch))
            .succeeds()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_override_beats_configuration() {
        let (s, why) = choose_strategy(
            SyncStrategy::Merge,
            Some(ResolvedStrategy::Rebase),
            true, // shared, which auto would merge
        );
        assert_eq!(s, ResolvedStrategy::Rebase);
        assert!(why.contains("--rebase"));
    }

    #[test]
    fn configured_strategy_is_honoured() {
        assert_eq!(
            choose_strategy(SyncStrategy::Merge, None, false).0,
            ResolvedStrategy::Merge
        );
        assert_eq!(
            choose_strategy(SyncStrategy::Rebase, None, true).0,
            ResolvedStrategy::Rebase
        );
    }

    #[test]
    fn auto_rebases_private_and_merges_shared() {
        let (private, _) = choose_strategy(SyncStrategy::Auto, None, false);
        assert_eq!(private, ResolvedStrategy::Rebase);

        let (shared, why) = choose_strategy(SyncStrategy::Auto, None, true);
        assert_eq!(shared, ResolvedStrategy::Merge);
        assert!(why.contains("pushed"));
    }

    #[test]
    fn strategy_labels_and_kinds() {
        assert_eq!(ResolvedStrategy::Rebase.verb(), "rebase");
        assert_eq!(ResolvedStrategy::Merge.label(), "Merge");
        assert_eq!(ResolvedStrategy::Rebase.journal_kind(), "sync_rebase");
        assert_eq!(ResolvedStrategy::Merge.journal_kind(), "sync_merge");
    }
}
