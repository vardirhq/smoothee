//! Branch metadata: base-branch detection and divergence analysis.
//!
//! "Base branch" is the branch a feature branch is intended to merge back into
//! (typically `main`). It is distinct from the *upstream*, which is the remote
//! tracking counterpart of the current branch. Smoothee needs both: the
//! upstream answers "is it safe to push?", the base answers "am I behind what
//! I'll merge into, and should I sync?".

use super::command::GitError;
use super::repository::Repository;

/// How Smoothee arrived at the base branch, surfaced so `status` can be honest
/// about whether the base was configured or merely guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseBranchSource {
    /// Read from `.smoothee.toml`.
    Configured,
    /// Derived from the remote's default branch (`origin/HEAD`).
    RemoteHead,
    /// Fell back to a conventional name that exists locally.
    Conventional,
}

/// The resolved base branch and how it was determined.
#[derive(Debug, Clone)]
pub struct BaseBranch {
    /// Local branch name (e.g. `main`).
    pub name: String,
    pub source: BaseBranchSource,
}

/// Conventional default branch names, in order of preference.
const CONVENTIONAL_NAMES: &[&str] = &["main", "master", "trunk", "develop"];

impl BaseBranch {
    /// Detect the base branch for `repo`.
    ///
    /// Resolution order (safe, deterministic — no network):
    /// 1. an explicit `configured` name (from `.smoothee.toml`), if it exists;
    /// 2. the remote default branch via `origin/HEAD`;
    /// 3. the first conventional name (`main`, `master`, …) that exists.
    pub fn detect(repo: &Repository, configured: Option<&str>) -> Result<Option<Self>, GitError> {
        if let Some(name) = configured {
            if branch_exists(repo, name)? {
                return Ok(Some(Self {
                    name: name.to_string(),
                    source: BaseBranchSource::Configured,
                }));
            }
        }

        if let Some(name) = remote_default_branch(repo)? {
            return Ok(Some(Self {
                name,
                source: BaseBranchSource::RemoteHead,
            }));
        }

        for candidate in CONVENTIONAL_NAMES {
            if branch_exists(repo, candidate)? {
                return Ok(Some(Self {
                    name: (*candidate).to_string(),
                    source: BaseBranchSource::Conventional,
                }));
            }
        }

        Ok(None)
    }
}

/// Ahead/behind divergence of `branch` relative to `base`, computed with
/// `git rev-list --left-right --count base...branch`.
///
/// Returns `None` when the two share no history (no merge base), which happens
/// for unrelated or freshly-initialised branches.
pub fn divergence_from_base(
    repo: &Repository,
    branch: &str,
    base: &str,
) -> Result<Option<(u32, u32)>, GitError> {
    // Without a merge base the `...` range is meaningless; report None instead
    // of a misleading count.
    let has_merge_base = repo.git("merge-base").arg(base).arg(branch).succeeds()?;
    if !has_merge_base {
        return Ok(None);
    }

    let range = format!("{base}...{branch}");
    let out = repo
        .git("rev-list")
        .arg("--left-right")
        .arg("--count")
        .arg(range)
        .output()?;

    // Output is "<behind>\t<ahead>": left side is base-only commits (behind),
    // right side is branch-only commits (ahead).
    let mut parts = out.split_whitespace();
    let behind = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    let ahead = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    Ok(Some((ahead, behind)))
}

/// Whether a local branch of the given name exists.
pub fn branch_exists(repo: &Repository, name: &str) -> Result<bool, GitError> {
    repo.git("show-ref")
        .arg("--verify")
        .arg("--quiet")
        .arg(format!("refs/heads/{name}"))
        .succeeds()
}

/// The remote's default branch as a bare local name (e.g. `origin/HEAD`
/// pointing at `origin/main` yields `main`), or `None` if unset.
fn remote_default_branch(repo: &Repository) -> Result<Option<String>, GitError> {
    let exists = repo
        .git("symbolic-ref")
        .arg("--quiet")
        .arg("refs/remotes/origin/HEAD")
        .succeeds()?;
    if !exists {
        return Ok(None);
    }

    let full = repo
        .git("symbolic-ref")
        .arg("--short")
        .arg("refs/remotes/origin/HEAD")
        .output()?;
    // `full` looks like "origin/main"; strip the remote prefix.
    Ok(full.strip_prefix("origin/").map(str::to_string))
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
    fn detects_conventional_base_when_nothing_configured() {
        let (_g, path) = init_repo();
        commit(&path, "one");
        let repo = Repository::discover(&path).unwrap();
        let base = BaseBranch::detect(&repo, None).unwrap().unwrap();
        assert_eq!(base.name, "main");
        assert_eq!(base.source, BaseBranchSource::Conventional);
    }

    #[test]
    fn prefers_configured_base_when_it_exists() {
        let (_g, path) = init_repo();
        commit(&path, "one");
        run(&path, &["branch", "develop"]);
        let repo = Repository::discover(&path).unwrap();
        let base = BaseBranch::detect(&repo, Some("develop")).unwrap().unwrap();
        assert_eq!(base.name, "develop");
        assert_eq!(base.source, BaseBranchSource::Configured);
    }

    #[test]
    fn ignores_configured_base_that_does_not_exist() {
        let (_g, path) = init_repo();
        commit(&path, "one");
        let repo = Repository::discover(&path).unwrap();
        // "release" does not exist; should fall back to conventional "main".
        let base = BaseBranch::detect(&repo, Some("release")).unwrap().unwrap();
        assert_eq!(base.name, "main");
    }

    #[test]
    fn computes_divergence_between_branches() {
        let (_g, path) = init_repo();
        commit(&path, "base one");
        commit(&path, "base two");
        // Branch off, then advance both sides.
        run(&path, &["checkout", "-q", "-b", "feature"]);
        commit(&path, "feature one");
        run(&path, &["checkout", "-q", "main"]);
        commit(&path, "base three");

        let repo = Repository::discover(&path).unwrap();
        let (ahead, behind) = divergence_from_base(&repo, "feature", "main")
            .unwrap()
            .unwrap();
        assert_eq!(ahead, 1, "feature has 1 commit main lacks");
        assert_eq!(behind, 1, "main has 1 commit feature lacks");
    }

    #[test]
    fn divergence_is_none_without_shared_history() {
        let (_g, path) = init_repo();
        commit(&path, "one");
        // An orphan branch shares no history with main.
        run(&path, &["checkout", "-q", "--orphan", "orphan"]);
        commit(&path, "orphan one");
        let repo = Repository::discover(&path).unwrap();
        assert!(divergence_from_base(&repo, "orphan", "main")
            .unwrap()
            .is_none());
    }
}
