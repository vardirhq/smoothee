//! `smoothee resolve` — guided, reversible merge-conflict resolution.
//!
//! Resolution is where Smoothee's principles are load-bearing: the repository is
//! already mid-operation and the user is anxious. So the engine is deterministic
//! and reversible end to end:
//!
//! * **Safe by default.** [`ResolveEngine::start`] creates a restore point at the
//!   true pre-operation `HEAD` (the pre-rebase `ORIG_HEAD` for a rebase, the
//!   branch tip for a merge) and journals the operation *before* anything is
//!   staged, so `smoothee undo` reverses the whole thing.
//! * **AI suggests, humans approve.** This engine performs only what the user
//!   explicitly chose (keep a side, stage an edit, finish). It never invents a
//!   resolution; the CLI layer drives the choices.
//! * **Validate before staging.** An edited file is refused if it still contains
//!   conflict markers ([`ResolveEngine::stage_edited`]).

use crate::git::conflicts::{ConflictContext, ConflictFile, Side};
use crate::git::restore::RestorePoint;
use crate::git::Repository;
use crate::verification::conflict_markers;

use super::journal::{BeforeState, Journal, OperationRecord};

/// Whether there is anything to resolve, and if so what.
#[derive(Debug, Clone)]
pub enum ResolveState {
    /// No in-progress operation and no conflicted files.
    Clean,
    /// Conflicts are present and waiting to be worked through.
    InProgress {
        context: ConflictContext,
        files: Vec<String>,
    },
}

/// The result of trying to conclude the operation once conflicts are resolved.
#[derive(Debug, Clone)]
pub enum FinishOutcome {
    /// The merge/rebase concluded; `head` is the new branch tip.
    Completed {
        /// The new branch tip. Recorded in the journal; surfaced for diagnostics.
        #[allow(dead_code)]
        head: String,
    },
    /// A rebase advanced but stopped again on the next commit's conflicts.
    MoreConflicts { files: Vec<String> },
    /// Conflicts remain unresolved, so the operation cannot be finished yet.
    Blocked { files: Vec<String> },
}

/// Errors specific to resolution.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("{path} still contains conflict markers; finish editing it first")]
    MarkersRemain { path: String },
    #[error("could not read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("finishing the {verb} failed and your work is untouched.\n{stderr}")]
    FinishFailed { verb: &'static str, stderr: String },
    #[error(transparent)]
    Git(#[from] crate::git::command::GitError),
    #[error(transparent)]
    Journal(#[from] super::journal::JournalError),
}

/// Drives conflict resolution for one repository.
pub struct ResolveEngine<'a> {
    repo: &'a Repository,
    journal: &'a Journal,
}

impl<'a> ResolveEngine<'a> {
    /// Create an engine over a repository and its journal.
    pub fn new(repo: &'a Repository, journal: &'a Journal) -> Self {
        Self { repo, journal }
    }

    /// The repository's working directory, for resolving relative conflict paths
    /// to absolute ones (e.g. to hand to an editor).
    pub fn workdir(&self) -> &std::path::Path {
        self.repo.workdir()
    }

    /// Inspect the repository for conflicts without touching anything.
    pub fn inspect(&self) -> Result<ResolveState, ResolveError> {
        let files = self.repo.conflicted_files()?;
        let in_progress = self.repo.rebase_in_progress() || self.repo.merge_in_progress();
        if files.is_empty() && !in_progress {
            return Ok(ResolveState::Clean);
        }
        Ok(ResolveState::InProgress {
            context: ConflictContext::detect(self.repo),
            files,
        })
    }

    /// Parse every currently-conflicted file into structured hunks.
    pub fn conflict_files(&self) -> Result<Vec<ConflictFile>, ResolveError> {
        Ok(self
            .repo
            .conflicted_files()?
            .iter()
            .map(|path| ConflictFile::read(self.repo, path))
            .collect())
    }

    /// Begin a resolution: create the restore point and journal the operation
    /// *before* any file is staged, so it is reversible from this point on.
    /// Returns the started record; call [`finish`](Self::finish) to complete it.
    pub fn start(
        &self,
        branch: &str,
        context: ConflictContext,
    ) -> Result<(OperationRecord, RestorePoint), ResolveError> {
        let before = self.pre_operation_head(context)?;
        let restore = RestorePoint::create(self.repo, branch, &before)?;

        let record = OperationRecord::begin(
            context.journal_kind(),
            self.repo.workdir().display().to_string(),
            branch,
            BeforeState {
                head: before,
                restore_ref: Some(restore.ref_name.clone()),
            },
        );
        self.journal.append(&record)?;
        Ok((record, restore))
    }

    /// Keep one whole side of a conflicted file and stage it.
    pub fn take_side(&self, path: &str, side: Side) -> Result<(), ResolveError> {
        crate::git::conflicts::take_side(self.repo, path, side)?;
        Ok(())
    }

    /// Stage a file the user edited by hand, but only after confirming no
    /// conflict markers remain — Smoothee never stages a half-resolved file.
    pub fn stage_edited(&self, path: &str) -> Result<(), ResolveError> {
        let content =
            std::fs::read_to_string(self.repo.workdir().join(path)).map_err(|source| {
                ResolveError::Read {
                    path: path.to_string(),
                    source,
                }
            })?;
        if conflict_markers::has_conflict_markers(&content) {
            return Err(ResolveError::MarkersRemain {
                path: path.to_string(),
            });
        }
        crate::git::conflicts::stage(self.repo, path)?;
        Ok(())
    }

    /// The files that still have unresolved conflicts.
    pub fn remaining(&self) -> Result<Vec<String>, ResolveError> {
        Ok(self.repo.conflicted_files()?)
    }

    /// Conclude the operation once conflicts are resolved: commit the merge or
    /// continue the rebase. `GIT_EDITOR=true` accepts the prepared commit
    /// message so nothing blocks on an editor.
    ///
    /// A rebase that stops again on the next commit reports `MoreConflicts`; the
    /// journal record stays `Started` because the operation genuinely isn't done.
    pub fn finish(
        &self,
        record: &OperationRecord,
        context: ConflictContext,
    ) -> Result<FinishOutcome, ResolveError> {
        let remaining = self.repo.conflicted_files()?;
        if !remaining.is_empty() {
            return Ok(FinishOutcome::Blocked { files: remaining });
        }

        let outcome = match context {
            ConflictContext::Rebase => self
                .repo
                .git("rebase")
                .arg("--continue")
                .env("GIT_EDITOR", "true")
                .run()?,
            _ => self
                .repo
                .git("commit")
                .arg("--no-edit")
                .env("GIT_EDITOR", "true")
                .run()?,
        };

        // A rebase can stop again on a later commit; report that instead of
        // claiming success.
        if self.repo.rebase_in_progress() || self.repo.merge_in_progress() {
            return Ok(FinishOutcome::MoreConflicts {
                files: self.repo.conflicted_files()?,
            });
        }

        if !outcome.success {
            return Err(ResolveError::FinishFailed {
                verb: context.verb(),
                stderr: outcome.stderr,
            });
        }

        let head = self.repo.head()?;
        self.journal
            .append(&record.clone().complete(head.clone()))?;
        Ok(FinishOutcome::Completed { head })
    }

    /// The `HEAD` to which `undo` should return — i.e. the state before the
    /// operation that caused the conflicts. For a rebase that is `ORIG_HEAD`
    /// (the pre-rebase tip); for a merge the current `HEAD` (the merge commit
    /// has not been created yet).
    fn pre_operation_head(&self, context: ConflictContext) -> Result<String, ResolveError> {
        if let ConflictContext::Rebase = context {
            let out = self
                .repo
                .git("rev-parse")
                .arg("--verify")
                .arg("--quiet")
                .arg("ORIG_HEAD")
                .run()?;
            if out.success {
                return Ok(out.stdout);
            }
        }
        Ok(self.repo.head()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::conflicts::ConflictContext;
    use crate::git::repository::tests::{init_repo, run};
    use crate::operations::undo::{self};

    fn commit(path: &std::path::Path, name: &str, contents: &str) {
        std::fs::write(path.join(name), contents).unwrap();
        run(path, &["add", "."]);
        run(path, &["commit", "-q", "-m", &format!("edit {name}")]);
    }

    /// A repository stopped mid-merge with a single conflicted file, plus the
    /// feature-branch tip that existed before the merge started.
    fn merging_repo() -> (tempfile::TempDir, std::path::PathBuf, String) {
        let (g, path) = init_repo();
        commit(&path, "f.txt", "base\n");
        run(&path, &["checkout", "-q", "-b", "feature"]);
        commit(&path, "f.txt", "feature side\n");
        let feature_head = Repository::discover(&path).unwrap().head().unwrap();
        run(&path, &["checkout", "-q", "main"]);
        commit(&path, "f.txt", "main side\n");
        run(&path, &["checkout", "-q", "feature"]);
        // Merge main into feature; conflicts on f.txt.
        let out = std::process::Command::new("git")
            .args(["merge", "--no-edit", "main"])
            .current_dir(&path)
            .output()
            .unwrap();
        assert!(!out.status.success(), "merge should conflict");
        (g, path, feature_head)
    }

    #[test]
    fn inspect_reports_clean_when_nothing_in_progress() {
        let (_g, path) = init_repo();
        commit(&path, "f.txt", "one\n");
        let repo = Repository::discover(&path).unwrap();
        let journal = Journal::for_git_dir(repo.git_dir());
        let engine = ResolveEngine::new(&repo, &journal);
        assert!(matches!(engine.inspect().unwrap(), ResolveState::Clean));
    }

    #[test]
    fn inspect_reports_merge_conflicts() {
        let (_g, path, _) = merging_repo();
        let repo = Repository::discover(&path).unwrap();
        let journal = Journal::for_git_dir(repo.git_dir());
        let engine = ResolveEngine::new(&repo, &journal);

        match engine.inspect().unwrap() {
            ResolveState::InProgress { context, files } => {
                assert_eq!(context, ConflictContext::Merge);
                assert_eq!(files, vec!["f.txt".to_string()]);
            }
            ResolveState::Clean => panic!("expected conflicts"),
        }
    }

    #[test]
    fn resolve_merge_then_finish_completes_and_is_undoable() {
        let (_g, path, feature_head) = merging_repo();
        let repo = Repository::discover(&path).unwrap();
        let journal = Journal::for_git_dir(repo.git_dir());
        let engine = ResolveEngine::new(&repo, &journal);

        let (record, restore) = engine.start("feature", ConflictContext::Merge).unwrap();
        assert!(RestorePoint::exists(&repo, &restore.ref_name).unwrap());

        // Keep our (feature) side and finish the merge.
        engine
            .take_side("f.txt", ConflictContext::Merge.mine())
            .unwrap();
        assert!(engine.remaining().unwrap().is_empty());

        match engine.finish(&record, ConflictContext::Merge).unwrap() {
            FinishOutcome::Completed { head } => {
                assert_ne!(head, feature_head, "merge commit advanced HEAD");
            }
            other => panic!("expected completion, got {other:?}"),
        }
        assert!(!repo.merge_in_progress());
        // The kept side is the feature content.
        assert_eq!(
            std::fs::read_to_string(path.join("f.txt")).unwrap(),
            "feature side\n"
        );

        // Undo reverses the whole thing back to the pre-merge tip.
        let target = undo::find_undo_target(&journal.operations().unwrap()).unwrap();
        assert_eq!(target.kind, "resolve_merge");
        let report = undo::perform(&repo, &journal, &target).unwrap();
        assert_eq!(report.to, feature_head);
        assert_eq!(repo.head().unwrap(), feature_head);
    }

    #[test]
    fn finish_is_blocked_while_conflicts_remain() {
        let (_g, path, _) = merging_repo();
        let repo = Repository::discover(&path).unwrap();
        let journal = Journal::for_git_dir(repo.git_dir());
        let engine = ResolveEngine::new(&repo, &journal);

        let (record, _restore) = engine.start("feature", ConflictContext::Merge).unwrap();
        // Nothing resolved yet.
        match engine.finish(&record, ConflictContext::Merge).unwrap() {
            FinishOutcome::Blocked { files } => assert_eq!(files, vec!["f.txt".to_string()]),
            other => panic!("expected blocked, got {other:?}"),
        }
    }

    #[test]
    fn stage_edited_refuses_leftover_markers() {
        let (_g, path, _) = merging_repo();
        let repo = Repository::discover(&path).unwrap();
        let journal = Journal::for_git_dir(repo.git_dir());
        let engine = ResolveEngine::new(&repo, &journal);

        // The conflicted file still holds markers; staging it must be refused.
        let err = engine.stage_edited("f.txt").unwrap_err();
        assert!(matches!(err, ResolveError::MarkersRemain { .. }));

        // After a clean hand-edit, staging succeeds and clears the conflict.
        std::fs::write(path.join("f.txt"), "resolved by hand\n").unwrap();
        engine.stage_edited("f.txt").unwrap();
        assert!(engine.remaining().unwrap().is_empty());
    }

    #[test]
    fn rebase_uses_orig_head_as_restore_point() {
        // Diverge feature and main on the same file, then rebase feature onto
        // main so it stops on a conflict.
        let (_g, path) = init_repo();
        commit(&path, "f.txt", "base\n");
        let base_tip = Repository::discover(&path).unwrap().head().unwrap();
        run(&path, &["checkout", "-q", "-b", "feature"]);
        commit(&path, "f.txt", "feature side\n");
        let feature_tip = Repository::discover(&path).unwrap().head().unwrap();
        run(&path, &["checkout", "-q", "main"]);
        commit(&path, "f.txt", "main side\n");
        run(&path, &["checkout", "-q", "feature"]);
        let out = std::process::Command::new("git")
            .args(["rebase", "main"])
            .current_dir(&path)
            .output()
            .unwrap();
        assert!(!out.status.success(), "rebase should conflict");

        let repo = Repository::discover(&path).unwrap();
        let journal = Journal::for_git_dir(repo.git_dir());
        let engine = ResolveEngine::new(&repo, &journal);
        assert!(repo.rebase_in_progress());
        assert_ne!(base_tip, feature_tip);

        let (record, _restore) = engine.start("feature", ConflictContext::Rebase).unwrap();
        // The restore point pins the pre-rebase feature tip (ORIG_HEAD), not the
        // mid-rebase HEAD.
        assert_eq!(record.before.head, feature_tip);

        // Keep our (feature) side — during a rebase that is the "theirs" side —
        // and continue.
        engine
            .take_side("f.txt", ConflictContext::Rebase.mine())
            .unwrap();
        match engine.finish(&record, ConflictContext::Rebase).unwrap() {
            FinishOutcome::Completed { .. } => {}
            other => panic!("expected completion, got {other:?}"),
        }
        assert!(!repo.rebase_in_progress());
        assert_eq!(
            std::fs::read_to_string(path.join("f.txt")).unwrap(),
            "feature side\n"
        );

        // Undo returns feature to its pre-rebase tip.
        let target = undo::find_undo_target(&journal.operations().unwrap()).unwrap();
        let report = undo::perform(&repo, &journal, &target).unwrap();
        assert_eq!(report.to, feature_tip);
        assert_eq!(repo.head().unwrap(), feature_tip);
    }
}
