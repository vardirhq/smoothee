//! Explicit recovery to an earlier Smoothee restore point.
//!
//! Recovery is deliberately stricter than `undo`: the caller selects a journal
//! operation, Smoothee proves its restore ref still exists, refuses to discard
//! working-tree changes or interrupt an active Git operation, then creates a new
//! restore point before moving HEAD. Recovery is therefore itself reversible.

use crate::git::restore::RestorePoint;
use crate::git::Repository;

use super::journal::{BeforeState, Journal, OperationRecord};

#[derive(Debug, Clone)]
pub struct RecoveryPlan {
    pub target: OperationRecord,
    pub current_head: String,
    pub restore_ref: String,
}

#[derive(Debug, Clone)]
pub struct RecoveryReport {
    pub target_id: String,
    pub from: String,
    pub to: String,
    pub safety_restore_ref: String,
}

#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    #[error(transparent)]
    Git(#[from] crate::git::command::GitError),
    #[error(transparent)]
    Journal(#[from] super::journal::JournalError),
    #[error("no Smoothee operation matches '{0}'")]
    NotFound(String),
    #[error("'{0}' matches more than one operation; use a longer id from `smoothee history`")]
    Ambiguous(String),
    #[error("operation {0} has no restore point")]
    NoRestorePoint(String),
    #[error("the restore point for operation {0} no longer exists")]
    MissingRestorePoint(String),
    #[error(
        "recovery only works on the operation's branch ({expected}); current branch is {actual}"
    )]
    WrongBranch { expected: String, actual: String },
    #[error("recovery requires a clean working tree; commit or stash your changes first")]
    DirtyWorkingTree,
    #[error("a merge or rebase is in progress; finish it or use `smoothee undo` first")]
    GitOperationInProgress,
}

pub fn plan(
    repo: &Repository,
    journal: &Journal,
    operation_id: &str,
) -> Result<RecoveryPlan, RecoveryError> {
    if repo.merge_in_progress() || repo.rebase_in_progress() {
        return Err(RecoveryError::GitOperationInProgress);
    }

    if !repo.git("status").arg("--porcelain").output()?.is_empty() {
        return Err(RecoveryError::DirtyWorkingTree);
    }

    let operations = journal.operations()?;
    let matches: Vec<_> = operations
        .into_iter()
        .filter(|record| record.id.starts_with(operation_id))
        .collect();
    let target = match matches.as_slice() {
        [] => return Err(RecoveryError::NotFound(operation_id.to_string())),
        [target] => target.clone(),
        _ => return Err(RecoveryError::Ambiguous(operation_id.to_string())),
    };

    let restore_ref = target
        .before
        .restore_ref
        .clone()
        .ok_or_else(|| RecoveryError::NoRestorePoint(target.id.clone()))?;
    if !RestorePoint::exists(repo, &restore_ref)? {
        return Err(RecoveryError::MissingRestorePoint(target.id.clone()));
    }

    let current_branch = repo
        .current_branch()?
        .unwrap_or_else(|| "detached HEAD".to_string());
    if current_branch != target.branch {
        return Err(RecoveryError::WrongBranch {
            expected: target.branch.clone(),
            actual: current_branch,
        });
    }

    Ok(RecoveryPlan {
        target,
        current_head: repo.head()?,
        restore_ref,
    })
}

pub fn perform(
    repo: &Repository,
    journal: &Journal,
    plan: &RecoveryPlan,
) -> Result<RecoveryReport, RecoveryError> {
    let safety = RestorePoint::create(repo, &plan.target.branch, &plan.current_head)?;
    let record = OperationRecord::begin(
        "recover",
        repo.workdir().display().to_string(),
        &plan.target.branch,
        BeforeState {
            head: plan.current_head.clone(),
            restore_ref: Some(safety.ref_name.clone()),
        },
    );
    journal.append(&record)?;

    let reset = repo
        .git("reset")
        .arg("--hard")
        .arg(&plan.restore_ref)
        .output();
    if let Err(err) = reset {
        journal.append(&record.fail())?;
        return Err(err.into());
    }

    let to = repo.head()?;
    journal.append(&record.complete(&to))?;

    Ok(RecoveryReport {
        target_id: plan.target.id.clone(),
        from: plan.current_head.clone(),
        to,
        safety_restore_ref: safety.ref_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repository::tests::{init_repo, run};

    fn commit(path: &std::path::Path, text: &str) {
        std::fs::write(path.join("f.txt"), text).unwrap();
        run(path, &["add", "."]);
        run(path, &["commit", "-q", "-m", text]);
    }

    #[test]
    fn recovery_is_reversible_and_restores_selected_state() {
        let (_g, path) = init_repo();
        commit(&path, "one");
        let repo = Repository::discover(&path).unwrap();
        let journal = Journal::for_git_dir(repo.git_dir());
        let first = repo.head().unwrap();
        let restore = RestorePoint::create(&repo, "main", &first).unwrap();
        let target = OperationRecord::begin(
            "sync_rebase",
            path.display().to_string(),
            "main",
            BeforeState {
                head: first.clone(),
                restore_ref: Some(restore.ref_name),
            },
        )
        .complete(&first);
        journal.append(&target).unwrap();

        commit(&path, "two");
        let second = repo.head().unwrap();
        let plan = plan(&repo, &journal, &target.id).unwrap();
        let report = perform(&repo, &journal, &plan).unwrap();

        assert_eq!(report.from, second);
        assert_eq!(report.to, first);
        assert_eq!(repo.head().unwrap(), first);
        assert!(RestorePoint::exists(&repo, &report.safety_restore_ref).unwrap());

        let ops = journal.operations().unwrap();
        let recovery = ops.last().unwrap();
        assert_eq!(recovery.kind, "recover");
        assert_eq!(recovery.before.head, second);
    }

    #[test]
    fn refuses_dirty_working_tree() {
        let (_g, path) = init_repo();
        commit(&path, "one");
        let repo = Repository::discover(&path).unwrap();
        let journal = Journal::for_git_dir(repo.git_dir());
        std::fs::write(path.join("f.txt"), "dirty").unwrap();
        assert!(matches!(
            plan(&repo, &journal, "anything"),
            Err(RecoveryError::DirtyWorkingTree)
        ));
    }
}
