//! Running project-defined verification checks.
//!
//! Smoothee never hardcodes toolchain commands. After a synchronization it runs
//! exactly the commands a repository configures under `[verification]` in its
//! `.smoothee.toml` (`format`, `lint`, `types`, `test`) and reports the result.
//!
//! Verification is *advisory* here: a failing check never rolls anything back on
//! its own. It tells the user their tree needs attention and that `smoothee
//! undo` is available — the human decides.

use std::process::Command;

use crate::config::repository::VerificationConfig;
use crate::git::Repository;

/// The outcome of one verification command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckResult {
    /// The check's role (e.g. `lint`, `test`).
    pub name: &'static str,
    /// The exact command line that ran.
    pub command: String,
    /// Whether the command exited zero.
    pub passed: bool,
}

/// Run every configured check in a stable order, in the repository's working
/// directory. Unconfigured checks are skipped. Returns one result per command
/// that ran.
pub fn run_checks(repo: &Repository, config: &VerificationConfig) -> Vec<CheckResult> {
    let steps = [
        ("format", &config.format),
        ("lint", &config.lint),
        ("types", &config.types),
        ("test", &config.test),
    ];

    steps
        .into_iter()
        .filter_map(|(name, command)| command.as_ref().map(|c| (name, c.clone())))
        .map(|(name, command)| {
            let passed = run_one(repo, &command);
            CheckResult {
                name,
                command,
                passed,
            }
        })
        .collect()
}

/// Whether every check passed (vacuously true when none are configured).
pub fn all_passed(results: &[CheckResult]) -> bool {
    results.iter().all(|r| r.passed)
}

/// Run a single shell command in the repository root, returning whether it
/// succeeded. The command string is handed to the platform shell so users can
/// write natural command lines (pipes, `&&`, etc.) in their config.
fn run_one(repo: &Repository, command: &str) -> bool {
    let mut cmd = shell_command(command);
    cmd.current_dir(repo.workdir());
    matches!(cmd.status(), Ok(status) if status.success())
}

#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new("cmd");
    cmd.arg("/C").arg(command);
    cmd
}

#[cfg(not(windows))]
fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repository::tests::init_repo;

    #[test]
    fn runs_only_configured_checks() {
        let (_g, path) = init_repo();
        let repo = Repository::discover(&path).unwrap();
        let config = VerificationConfig {
            lint: Some("true".to_string()),
            test: Some("false".to_string()),
            ..Default::default()
        };

        let results = run_checks(&repo, &config);
        assert_eq!(results.len(), 2, "only lint and test are configured");
        assert_eq!(results[0].name, "lint");
        assert!(results[0].passed);
        assert_eq!(results[1].name, "test");
        assert!(!results[1].passed);
        assert!(!all_passed(&results));
    }

    #[test]
    fn no_checks_is_vacuously_passing() {
        let (_g, path) = init_repo();
        let repo = Repository::discover(&path).unwrap();
        let results = run_checks(&repo, &VerificationConfig::default());
        assert!(results.is_empty());
        assert!(all_passed(&results));
    }
}
