use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct GhOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone)]
pub struct GhCommand {
    args: Vec<OsString>,
    cwd: Option<PathBuf>,
}

impl GhCommand {
    pub fn new(subcommand: impl AsRef<OsStr>) -> Self {
        Self {
            args: vec![subcommand.as_ref().to_os_string()],
            cwd: None,
        }
    }

    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    pub fn in_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.cwd = Some(dir.as_ref().to_path_buf());
        self
    }

    pub fn display(&self) -> String {
        let mut parts = vec!["gh".to_string()];
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

    pub fn run(&self) -> Result<GhOutput> {
        let mut command = Command::new("gh");
        command.args(&self.args);
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        let output = command
            .output()
            .with_context(|| format!("failed to run `{}`", self.display()))?;
        Ok(GhOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout)
                .trim_end_matches('\n')
                .to_string(),
            stderr: String::from_utf8_lossy(&output.stderr)
                .trim_end()
                .to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_quotes_arguments_with_spaces() {
        let command = GhCommand::new("pr")
            .arg("create")
            .arg("--title")
            .arg("Add safer PR workflow");
        assert_eq!(
            command.display(),
            "gh pr create --title \"Add safer PR workflow\""
        );
    }
}
