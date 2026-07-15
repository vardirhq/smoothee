//! Restore points — real Git refs that pin pre-operation state.
//!
//! "Safe by default" means every risky operation is preceded by a restore
//! point: a commit-ish Smoothee can return to if anything goes wrong. Rather
//! than invent a private snapshot format, a restore point is an ordinary Git
//! ref under `refs/smoothee/restore/…` pointing at the pre-operation `HEAD`.
//! Recovery is then a plain `git reset --hard`, and the ref is visible to
//! `git for-each-ref`, `git log`, and the reflog like any other.

use chrono::Local;

use super::command::GitError;
use super::repository::Repository;

/// The ref namespace all restore points live under.
const RESTORE_NAMESPACE: &str = "refs/smoothee/restore";

/// A created restore point: a Git ref and the commit it pins.
#[derive(Debug, Clone)]
pub struct RestorePoint {
    /// The full ref name, e.g. `refs/smoothee/restore/feature-login/2026-07-15-143205`.
    pub ref_name: String,
    /// The commit the ref points at.
    #[allow(dead_code)] // Same commit the journal records; kept for diagnostics.
    pub head: String,
}

impl RestorePoint {
    /// Create a restore point for `branch` pinning `head`.
    ///
    /// The ref name embeds a sanitised branch name and a timestamp so restore
    /// points are self-describing and never collide across operations.
    pub fn create(repo: &Repository, branch: &str, head: &str) -> Result<Self, GitError> {
        let timestamp = Local::now().format("%Y-%m-%d-%H%M%S").to_string();
        let ref_name = format!("{RESTORE_NAMESPACE}/{}/{timestamp}", sanitize(branch));
        repo.git("update-ref").arg(&ref_name).arg(head).output()?;
        Ok(Self {
            ref_name,
            head: head.to_string(),
        })
    }

    /// The human-facing short name (drops the leading `refs/`), matching how the
    /// spec presents restore points to users.
    pub fn display_name(&self) -> &str {
        self.ref_name
            .strip_prefix("refs/")
            .unwrap_or(&self.ref_name)
    }

    /// Whether a restore ref of the given name still exists.
    #[allow(dead_code)] // Used by tests today; by `doctor`/recovery checks later.
    pub fn exists(repo: &Repository, ref_name: &str) -> Result<bool, GitError> {
        repo.git("show-ref")
            .arg("--verify")
            .arg("--quiet")
            .arg(ref_name)
            .succeeds()
    }

    /// Delete a restore ref (used when an aborted operation makes it redundant).
    #[allow(dead_code)] // Used by tests today; by restore-point pruning later.
    pub fn delete(repo: &Repository, ref_name: &str) -> Result<(), GitError> {
        repo.git("update-ref").arg("-d").arg(ref_name).output()?;
        Ok(())
    }
}

/// Make a branch name safe to embed as a single ref path component: slashes
/// (which would create unwanted ref hierarchy) become dashes, matching the
/// `feature/login` → `feature-login` presentation in the spec.
fn sanitize(branch: &str) -> String {
    branch.replace('/', "-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repository::tests::{init_repo, run};

    fn commit(path: &std::path::Path, msg: &str) {
        std::fs::write(path.join("f.txt"), msg).unwrap();
        run(path, &["add", "."]);
        run(path, &["commit", "-q", "-m", msg]);
    }

    #[test]
    fn sanitizes_branch_names() {
        assert_eq!(sanitize("feature/login"), "feature-login");
        assert_eq!(sanitize("main"), "main");
        assert_eq!(sanitize("a/b/c"), "a-b-c");
    }

    #[test]
    fn create_pins_head_and_is_recoverable() {
        let (_g, path) = init_repo();
        commit(&path, "one");
        let repo = Repository::discover(&path).unwrap();
        let head = repo.head().unwrap();

        let rp = RestorePoint::create(&repo, "feature/login", &head).unwrap();
        assert!(rp
            .ref_name
            .starts_with("refs/smoothee/restore/feature-login/"));
        assert_eq!(rp.display_name(), &rp.ref_name["refs/".len()..]);
        assert_eq!(rp.head, head);
        assert!(RestorePoint::exists(&repo, &rp.ref_name).unwrap());

        // The ref resolves to exactly the pinned commit.
        let resolved = repo.git("rev-parse").arg(&rp.ref_name).output().unwrap();
        assert_eq!(resolved, head);
    }

    #[test]
    fn delete_removes_the_ref() {
        let (_g, path) = init_repo();
        commit(&path, "one");
        let repo = Repository::discover(&path).unwrap();
        let head = repo.head().unwrap();
        let rp = RestorePoint::create(&repo, "main", &head).unwrap();

        RestorePoint::delete(&repo, &rp.ref_name).unwrap();
        assert!(!RestorePoint::exists(&repo, &rp.ref_name).unwrap());
    }
}
