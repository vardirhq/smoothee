//! `smoothee undo` — reverse the last Smoothee-managed operation.
//!
//! Undo is the payoff for journaling every mutation and creating a restore
//! point before it. It relies only on standard Git mechanisms: it aborts any
//! in-progress rebase/merge, then `reset --hard`s the branch back to the HEAD
//! recorded before the operation ran. The journal stays append-only — undoing
//! appends an `undo` record rather than rewriting history — so `undo` is itself
//! a journaled, inspectable event.

use crate::git::Repository;

use super::journal::{Journal, OperationRecord};

/// What an undo did, for reporting back to the user.
#[derive(Debug, Clone)]
pub struct UndoReport {
    /// The kind of operation that was reversed (e.g. `sync_rebase`).
    pub kind: String,
    /// The branch it acted on.
    pub branch: String,
    /// If an in-progress operation was aborted first, which one.
    pub aborted: Option<&'static str>,
    /// The HEAD the branch was at before undoing.
    pub from: String,
    /// The HEAD the branch was restored to.
    pub to: String,
}

/// Errors that can arise while undoing.
#[derive(Debug, thiserror::Error)]
pub enum UndoError {
    #[error(transparent)]
    Git(#[from] crate::git::command::GitError),
    #[error(transparent)]
    Journal(#[from] super::journal::JournalError),
}

/// The most recent operation that can still be undone, or `None`.
///
/// Pure over a folded operation list so it is exhaustively testable. Skips
/// `undo` records themselves (no redo) and any operation that a later `undo`
/// already reversed, then returns the newest remaining operation.
pub fn find_undo_target(operations: &[OperationRecord]) -> Option<OperationRecord> {
    let undone: std::collections::HashSet<&str> = operations
        .iter()
        .filter(|r| r.kind == "undo")
        .filter_map(|r| r.undoes.as_deref())
        .collect();

    operations
        .iter()
        .rev()
        .find(|r| r.kind != "undo" && !undone.contains(r.id.as_str()))
        .cloned()
}

/// Reverse `target`, restoring `target.branch` to its pre-operation HEAD.
///
/// Aborts an in-progress rebase/merge first, switches to the recorded branch if
/// needed, then resets it back. Appends an `undo` record and returns a report.
pub fn perform(
    repo: &Repository,
    journal: &Journal,
    target: &OperationRecord,
) -> Result<UndoReport, UndoError> {
    let from = repo.head()?;

    // Abort any operation Git still considers in progress, so the branch pointer
    // is free to move and no half-applied state lingers.
    let aborted = if repo.rebase_in_progress() {
        repo.git("rebase").arg("--abort").output()?;
        Some("rebase")
    } else if repo.merge_in_progress() {
        repo.git("merge").arg("--abort").output()?;
        Some("merge")
    } else {
        None
    };

    // Make sure the reset lands on the branch the operation touched.
    if repo.current_branch()?.as_deref() != Some(target.branch.as_str()) {
        repo.git("checkout").arg(&target.branch).output()?;
    }

    repo.git("reset")
        .arg("--hard")
        .arg(&target.before.head)
        .output()?;

    journal.append(&OperationRecord::undo(target, &from))?;

    Ok(UndoReport {
        kind: target.kind.clone(),
        branch: target.branch.clone(),
        aborted,
        from,
        to: target.before.head.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RepoConfig;
    use crate::git::repository::tests::{init_repo, run};
    use crate::git::restore::RestorePoint;
    use crate::operations::journal::BeforeState;
    use crate::operations::sync::{ResolvedStrategy, SyncEngine, SyncPlan, SyncResult};

    fn commit(path: &std::path::Path, name: &str, contents: &str) {
        std::fs::write(path.join(name), contents).unwrap();
        run(path, &["add", "."]);
        run(path, &["commit", "-q", "-m", &format!("add {name}")]);
    }

    fn record(kind: &str, id_marker: &str) -> OperationRecord {
        // Build a record with a controllable id by reusing begin then overriding.
        let mut r = OperationRecord::begin(
            kind,
            "/repo",
            "feature",
            BeforeState {
                head: "before".into(),
                restore_ref: None,
            },
        );
        r.id = id_marker.to_string();
        r
    }

    #[test]
    fn find_target_returns_newest_operation() {
        let ops = vec![record("sync_rebase", "op1"), record("sync_merge", "op2")];
        assert_eq!(find_undo_target(&ops).unwrap().id, "op2");
    }

    #[test]
    fn find_target_skips_already_undone() {
        let mut undo = record("undo", "op3");
        undo.kind = "undo".into();
        undo.undoes = Some("op2".into());
        let ops = vec![
            record("sync_rebase", "op1"),
            record("sync_merge", "op2"),
            undo,
        ];
        // op2 was undone, op3 is an undo record → op1 is the target.
        assert_eq!(find_undo_target(&ops).unwrap().id, "op1");
    }

    #[test]
    fn find_target_none_when_all_undone() {
        let mut undo = record("undo", "u1");
        undo.undoes = Some("op1".into());
        let ops = vec![record("sync_rebase", "op1"), undo];
        assert!(find_undo_target(&ops).is_none());
    }

    #[test]
    fn find_target_none_on_empty() {
        assert!(find_undo_target(&[]).is_none());
    }

    /// Build the two-branch layout: `main` advances past where `feature` forked.
    /// Returns (guard, path, feature_head_before_sync).
    fn diverged_repo() -> (tempfile::TempDir, std::path::PathBuf, String) {
        let (g, path) = init_repo();
        commit(&path, "shared.txt", "base");
        run(&path, &["checkout", "-q", "-b", "feature"]);
        commit(&path, "feature.txt", "feature work");
        let feature_head = Repository::discover(&path).unwrap().head().unwrap();
        run(&path, &["checkout", "-q", "main"]);
        commit(&path, "main.txt", "main work");
        run(&path, &["checkout", "-q", "feature"]);
        (g, path, feature_head)
    }

    fn plan_onto_main(branch: &str, strategy: ResolvedStrategy) -> SyncPlan {
        SyncPlan {
            branch: branch.to_string(),
            base_name: "main".to_string(),
            remote_ref: "main".to_string(), // execute() rebases/merges onto this ref directly
            ahead: 1,
            behind: 1,
            strategy,
            reason: "test".to_string(),
        }
    }

    #[test]
    fn sync_then_undo_restores_exactly() {
        let (_g, path, feature_head) = diverged_repo();
        let repo = Repository::discover(&path).unwrap();
        let config = RepoConfig::default();
        let journal = Journal::for_git_dir(repo.git_dir());
        let engine = SyncEngine::new(&repo, &config, &journal);

        // Clean rebase (no conflicts, different files).
        let plan = plan_onto_main("feature", ResolvedStrategy::Rebase);
        let restore = match engine.execute(&plan).unwrap() {
            SyncResult::Completed { restore, .. } => restore,
            other => panic!("expected clean completion, got {other:?}"),
        };
        assert_ne!(repo.head().unwrap(), feature_head, "rebase moved HEAD");
        assert!(RestorePoint::exists(&repo, &restore.ref_name).unwrap());

        // Undo returns the branch to exactly where it was.
        let target = find_undo_target(&journal.operations().unwrap()).unwrap();
        let report = perform(&repo, &journal, &target).unwrap();
        assert_eq!(report.kind, "sync_rebase");
        assert_eq!(report.to, feature_head);
        assert_eq!(repo.head().unwrap(), feature_head);

        // A second undo finds nothing (the only op was undone).
        assert!(find_undo_target(&journal.operations().unwrap()).is_none());
    }

    #[test]
    fn conflicting_sync_then_undo_aborts_and_restores() {
        let (g, path) = init_repo();
        commit(&path, "f.txt", "base");
        run(&path, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(path.join("f.txt"), "feature side").unwrap();
        run(&path, &["commit", "-q", "-am", "feature edit"]);
        let feature_head = Repository::discover(&path).unwrap().head().unwrap();
        run(&path, &["checkout", "-q", "main"]);
        std::fs::write(path.join("f.txt"), "main side").unwrap();
        run(&path, &["commit", "-q", "-am", "main edit"]);
        run(&path, &["checkout", "-q", "feature"]);
        let _ = g; // keep the tempdir alive

        let repo = Repository::discover(&path).unwrap();
        let config = RepoConfig::default();
        let journal = Journal::for_git_dir(repo.git_dir());
        let engine = SyncEngine::new(&repo, &config, &journal);

        let plan = plan_onto_main("feature", ResolvedStrategy::Merge);
        match engine.execute(&plan).unwrap() {
            SyncResult::Conflicted { files, .. } => {
                assert_eq!(files, vec!["f.txt".to_string()]);
            }
            other => panic!("expected conflict, got {other:?}"),
        }
        assert!(repo.merge_in_progress(), "merge should be in progress");

        // Undo aborts the merge and restores the pre-sync HEAD.
        let target = find_undo_target(&journal.operations().unwrap()).unwrap();
        let report = perform(&repo, &journal, &target).unwrap();
        assert_eq!(report.aborted, Some("merge"));
        assert!(!repo.merge_in_progress());
        assert_eq!(repo.head().unwrap(), feature_head);
        assert!(repo.conflicted_files().unwrap().is_empty());
    }
}
