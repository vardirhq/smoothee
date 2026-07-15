//! Structured runner for the installed `git` binary.
//!
//! Smoothee never reimplements Git. It shells out to the real `git` executable
//! and parses machine-readable output. This module is the single choke point
//! through which every Git invocation flows, which keeps two design invariants
//! enforceable in one place:
//!
//! * **Preserve access to Git.** Every command is rendered as a human-readable
//!   string (`GitCommand::display`) so the UI can show users exactly what ran.
//! * **Deterministic boundary.** The AI layer never touches this module; only
//!   deterministic code paths construct and execute [`GitCommand`]s.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

/// Errors that can arise while invoking `git`.
#[derive(Debug, Error)]
pub enum GitError {
    /// The `git` binary could not be launched (e.g. not installed).
    #[error("failed to run `{command}`: {source}")]
    Spawn {
        command: String,
        #[source]
        source: std::io::Error,
    },

    /// `git` ran but exited with a non-zero status.
    #[error("`{command}` exited with status {code}\n{stderr}")]
    Failed {
        command: String,
        code: i32,
        stderr: String,
    },

    /// Output that was expected to be UTF-8 was not.
    #[error("`{command}` produced non-UTF-8 output")]
    NotUtf8 { command: String },
}

/// The captured outcome of a [`GitCommand::run`] invocation, including a
/// non-zero exit that the caller wants to inspect rather than treat as an error.
#[derive(Debug, Clone)]
pub struct GitOutput {
    /// Whether the process exited with status zero.
    pub success: bool,
    /// The exit code (`-1` if the process was terminated by a signal).
    #[allow(dead_code)] // Surfaced for callers that need to branch on exit codes.
    pub code: i32,
    /// Captured stdout, trailing newlines trimmed.
    pub stdout: String,
    /// Captured stderr, trailing whitespace trimmed.
    pub stderr: String,
}

/// A fully-formed `git` invocation, ready to run and cheap to describe.
///
/// Construct with [`GitCommand::new`] and chain [`GitCommand::arg`] /
/// [`GitCommand::args`]. The command remembers the working directory so callers
/// can build it once and both run and display it.
#[derive(Debug, Clone)]
pub struct GitCommand {
    args: Vec<OsString>,
    cwd: Option<PathBuf>,
}

impl GitCommand {
    /// Start a new `git <subcommand>` invocation.
    pub fn new(subcommand: impl AsRef<OsStr>) -> Self {
        Self {
            args: vec![subcommand.as_ref().to_os_string()],
            cwd: None,
        }
    }

    /// Append a single argument.
    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    /// Append multiple arguments.
    #[allow(dead_code)] // Builder convenience used by Phase 2's multi-arg commands.
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for arg in args {
            self.args.push(arg.as_ref().to_os_string());
        }
        self
    }

    /// Run the command inside `dir` rather than the current directory.
    pub fn in_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.cwd = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Render the command the way a user would type it, for display and for
    /// the "Running:" transparency output Smoothee shows before mutating a repo.
    pub fn display(&self) -> String {
        let mut parts = Vec::with_capacity(self.args.len() + 1);
        parts.push("git".to_string());
        for arg in &self.args {
            let rendered = arg.to_string_lossy();
            if rendered.is_empty() || rendered.contains(char::is_whitespace) {
                parts.push(format!("\"{rendered}\""));
            } else {
                parts.push(rendered.into_owned());
            }
        }
        parts.join(" ")
    }

    fn build(&self) -> Command {
        let mut cmd = Command::new("git");
        cmd.args(&self.args);
        if let Some(cwd) = &self.cwd {
            cmd.current_dir(cwd);
        }
        cmd
    }

    /// Run the command, requiring a zero exit status, and return captured
    /// stdout as a UTF-8 string with trailing newline trimmed.
    pub fn output(&self) -> Result<String, GitError> {
        let display = self.display();
        let output = self.build().output().map_err(|source| GitError::Spawn {
            command: display.clone(),
            source,
        })?;

        if !output.status.success() {
            return Err(GitError::Failed {
                command: display,
                code: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr)
                    .trim_end()
                    .to_string(),
            });
        }

        let stdout =
            String::from_utf8(output.stdout).map_err(|_| GitError::NotUtf8 { command: display })?;
        Ok(stdout.trim_end_matches('\n').to_string())
    }

    /// Run the command, capturing its outcome *without* treating a non-zero
    /// exit as an error.
    ///
    /// Mutating commands such as `rebase` and `merge` signal "stopped for
    /// conflicts" through a non-zero status that is a normal, expected outcome
    /// rather than a failure to surface. Callers inspect [`GitOutput::success`]
    /// and the captured streams to decide what happened next.
    pub fn run(&self) -> Result<GitOutput, GitError> {
        let display = self.display();
        let output = self.build().output().map_err(|source| GitError::Spawn {
            command: display.clone(),
            source,
        })?;

        Ok(GitOutput {
            success: output.status.success(),
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout)
                .trim_end_matches('\n')
                .to_string(),
            stderr: String::from_utf8_lossy(&output.stderr)
                .trim_end()
                .to_string(),
        })
    }

    /// Run the command and report whether it exited successfully, without
    /// treating a non-zero status as an error. Useful for predicate-style
    /// checks such as "do these two refs share history?".
    ///
    /// stdout and stderr are discarded: callers want only the exit code, and
    /// letting the child inherit our stdio would leak stray output (e.g.
    /// `git merge-base` printing a commit hash) into Smoothee's own output.
    pub fn succeeds(&self) -> Result<bool, GitError> {
        use std::process::Stdio;
        let display = self.display();
        let status = self
            .build()
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|source| GitError::Spawn {
                command: display,
                source,
            })?;
        Ok(status.success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_renders_like_a_shell_command() {
        let cmd = GitCommand::new("status")
            .arg("--porcelain=v2")
            .arg("--branch");
        assert_eq!(cmd.display(), "git status --porcelain=v2 --branch");
    }

    #[test]
    fn display_quotes_arguments_with_spaces() {
        let cmd = GitCommand::new("commit").arg("-m").arg("a message");
        assert_eq!(cmd.display(), "git commit -m \"a message\"");
    }

    #[test]
    fn output_captures_stdout() {
        // `git --version` is stable and available wherever these tests run.
        let out = GitCommand::new("--version")
            .output()
            .expect("git --version");
        assert!(out.starts_with("git version"));
    }

    #[test]
    fn succeeds_reports_exit_status() {
        assert!(GitCommand::new("--version").succeeds().unwrap());
    }

    #[test]
    fn run_captures_success_and_stdout() {
        let out = GitCommand::new("--version").run().unwrap();
        assert!(out.success);
        assert_eq!(out.code, 0);
        assert!(out.stdout.starts_with("git version"));
    }

    #[test]
    fn run_reports_failure_without_erroring() {
        // A bogus subcommand exits non-zero; `run` must surface that as data,
        // not as a `GitError`.
        let out = GitCommand::new("not-a-real-subcommand").run().unwrap();
        assert!(!out.success);
        assert_ne!(out.code, 0);
    }
}
