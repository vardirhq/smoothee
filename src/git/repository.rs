//! Repository discovery.
//!
//! Locates the Git repository that contains the current working directory and
//! exposes the handful of paths and identifiers the rest of Smoothee needs.

use std::path::{Path, PathBuf};

use super::command::{GitCommand, GitError};

/// A discovered Git repository rooted at [`Repository::workdir`].
#[derive(Debug, Clone)]
pub struct Repository {
    /// The top-level working directory of the repository.
    workdir: PathBuf,
    /// The `.git` directory (or file, for worktrees/submodules).
    /// Read via [`Repository::git_dir`]; consumed by Phase 2's journal.
    #[allow(dead_code)]
    git_dir: PathBuf,
}

impl Repository {
    /// Discover the repository that contains `start`.
    ///
    /// Uses `git rev-parse` so the answer honours worktrees, submodules, and
    /// `GIT_DIR`/`GIT_WORK_TREE` exactly as Git itself would.
    pub fn discover(start: impl AsRef<Path>) -> Result<Self, GitError> {
        let start = start.as_ref();

        let workdir = GitCommand::new("rev-parse")
            .arg("--show-toplevel")
            .in_dir(start)
            .output()?;

        let git_dir = GitCommand::new("rev-parse")
            .arg("--absolute-git-dir")
            .in_dir(start)
            .output()?;

        Ok(Self {
            workdir: PathBuf::from(workdir),
            git_dir: PathBuf::from(git_dir),
        })
    }

    /// Discover the repository containing the process's current directory.
    pub fn discover_from_cwd() -> Result<Self, GitError> {
        let cwd = std::env::current_dir().map_err(|source| GitError::Spawn {
            command: "git rev-parse --show-toplevel".to_string(),
            source,
        })?;
        Self::discover(cwd)
    }

    /// The repository's top-level working directory.
    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    /// The repository's `.git` directory.
    ///
    /// Used by Phase 2 to locate the operation journal (`<git_dir>/smoothee/`);
    /// retained now so discovery captures it in one place.
    #[allow(dead_code)]
    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }

    /// A short, human-friendly name for the repository (its directory name).
    pub fn name(&self) -> String {
        self.workdir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.workdir.to_string_lossy().into_owned())
    }

    /// Build a [`GitCommand`] already scoped to this repository's working dir.
    pub fn git(&self, subcommand: impl AsRef<std::ffi::OsStr>) -> GitCommand {
        GitCommand::new(subcommand).in_dir(&self.workdir)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::process::Command;

    /// Create a throwaway repository and return its temp dir handle plus path.
    pub(crate) fn init_repo() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().to_path_buf();
        run(&path, &["init", "-q", "-b", "main"]);
        run(&path, &["config", "user.email", "test@example.com"]);
        run(&path, &["config", "user.name", "Test"]);
        (dir, path)
    }

    pub(crate) fn run(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    }

    #[test]
    fn discovers_repository_root() {
        let (_guard, path) = init_repo();
        let repo = Repository::discover(&path).expect("discover");
        // Compare canonicalized paths: macOS /var vs /private/var etc.
        assert_eq!(
            repo.workdir().canonicalize().unwrap(),
            path.canonicalize().unwrap()
        );
        assert!(repo.git_dir().ends_with(".git"));
    }

    #[test]
    fn discovers_from_subdirectory() {
        let (_guard, path) = init_repo();
        let nested = path.join("a/b/c");
        std::fs::create_dir_all(&nested).unwrap();
        let repo = Repository::discover(&nested).expect("discover from nested");
        assert_eq!(
            repo.workdir().canonicalize().unwrap(),
            path.canonicalize().unwrap()
        );
    }

    #[test]
    fn discovery_fails_outside_a_repository() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(Repository::discover(dir.path()).is_err());
    }
}
