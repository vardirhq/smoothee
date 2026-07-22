//! `smoothee doctor` — check the local Git/GitHub environment.
//!
//! Doctor is deliberately read-only. It gathers the same deterministic context
//! later commands depend on, then reports which pieces are ready and which
//! ones deserve attention before a risky workflow starts.

use anyhow::Result;

use crate::config::global::GlobalPaths;
use crate::config::RepoConfig;
use crate::git::branches::{BaseBranch, BaseBranchSource};
use crate::git::command::GitCommand;
use crate::git::Repository;
use crate::ui::output;

/// Entry point for the `doctor` subcommand.
pub fn run() -> Result<()> {
    println!("{}", output::heading("Smoothee doctor"));
    println!();

    print_environment();
    println!();
    print_repository();
    println!();
    print_configuration();

    Ok(())
}

fn print_environment() {
    println!("{}", output::label("Environment:"));

    match GitCommand::new("--version").output() {
        Ok(version) => println!("{}", output::bullet(&output::ok(&version))),
        Err(err) => println!(
            "{}",
            output::bullet(&output::warn(&format!("Git unavailable: {err}")))
        ),
    }

    match command_version("gh", &["--version"]) {
        Some(version) => println!("{}", output::bullet(&output::ok(&version))),
        None => println!(
            "{}",
            output::bullet(&output::warn(
                "GitHub CLI not found; `smoothee pr` will need `gh` later"
            ))
        ),
    }

    match GlobalPaths::resolve() {
        Some(paths) => {
            println!(
                "{}",
                output::bullet(&output::ok(&format!(
                    "config dir: {}",
                    paths.config_dir().display()
                )))
            );
            println!(
                "{}",
                output::bullet(&output::ok(&format!(
                    "data dir: {}",
                    paths.data_dir().display()
                )))
            );
        }
        None => println!(
            "{}",
            output::bullet(&output::warn(
                "could not resolve user config/data directories"
            ))
        ),
    }
}

fn print_repository() {
    println!("{}", output::label("Repository:"));

    let repo = match Repository::discover_from_cwd() {
        Ok(repo) => repo,
        Err(_) => {
            println!(
                "{}",
                output::bullet(&output::warn("not inside a Git repository"))
            );
            return;
        }
    };

    println!(
        "{}",
        output::bullet(&output::ok(&format!(
            "workdir: {}",
            repo.workdir().display()
        )))
    );
    println!(
        "{}",
        output::bullet(&output::ok(&format!(
            "git dir: {}",
            repo.git_dir().display()
        )))
    );

    match repo.current_branch() {
        Ok(Some(branch)) => println!(
            "{}",
            output::bullet(&output::ok(&format!("branch: {branch}")))
        ),
        Ok(None) => println!(
            "{}",
            output::bullet(&output::warn("detached HEAD; sync/pr workflows need a branch"))
        ),
        Err(err) => println!(
            "{}",
            output::bullet(&output::warn(&format!("could not read branch: {err}")))
        ),
    }

    match RepoConfig::load(repo.workdir()) {
        Ok(config) => match BaseBranch::detect(&repo, config.base_branch.as_deref()) {
            Ok(Some(base)) => println!(
                "{}",
                output::bullet(&output::ok(&format!(
                    "base branch: {}{}",
                    base.name,
                    base_source_note(base.source)
                )))
            ),
            Ok(None) => println!(
                "{}",
                output::bullet(&output::warn(
                    "base branch not detected; set base_branch in .smoothee.toml"
                ))
            ),
            Err(err) => println!(
                "{}",
                output::bullet(&output::warn(&format!("could not detect base branch: {err}")))
            ),
        },
        Err(err) => println!(
            "{}",
            output::bullet(&output::warn(&format!("could not load repo config: {err}")))
        ),
    }
}

fn print_configuration() {
    println!("{}", output::label("Configuration:"));

    let repo = match Repository::discover_from_cwd() {
        Ok(repo) => repo,
        Err(_) => {
            println!(
                "{}",
                output::bullet(&output::label(
                    "repo config skipped because no repository was found"
                ))
            );
            return;
        }
    };

    let config_path = repo.workdir().join(RepoConfig::FILE_NAME);
    let config = match RepoConfig::load(repo.workdir()) {
        Ok(config) => config,
        Err(err) => {
            println!(
                "{}",
                output::bullet(&output::warn(&format!("invalid .smoothee.toml: {err}")))
            );
            return;
        }
    };

    if config_path.exists() {
        println!(
            "{}",
            output::bullet(&output::ok(&format!(
                "repo config: {}",
                config_path.display()
            )))
        );
    } else {
        println!(
            "{}",
            output::bullet(&output::label(
                "repo config: not present (defaults are in use)"
            ))
        );
    }

    println!(
        "{}",
        output::bullet(&output::ok(&format!(
            "sync strategy: {:?}",
            config.sync_strategy
        )))
    );

    let checks = [
        ("format", config.verification.format.as_deref()),
        ("lint", config.verification.lint.as_deref()),
        ("types", config.verification.types.as_deref()),
        ("test", config.verification.test.as_deref()),
    ];
    let configured: Vec<_> = checks
        .into_iter()
        .filter_map(|(name, command)| command.map(|command| (name, command)))
        .collect();

    if configured.is_empty() {
        println!(
            "{}",
            output::bullet(&output::warn(
                "no verification commands configured; sync cannot validate changes yet"
            ))
        );
    } else {
        for (name, command) in configured {
            println!(
                "{}",
                output::bullet(&output::ok(&format!("{name}: {command}")))
            );
        }
    }
}

fn command_version(command: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(command).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().next().map(str::to_string)
}

fn base_source_note(source: BaseBranchSource) -> &'static str {
    match source {
        BaseBranchSource::Configured => " (configured)",
        BaseBranchSource::RemoteHead => " (from origin)",
        BaseBranchSource::Conventional => " (guessed)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_source_notes_are_plain_language() {
        assert_eq!(
            base_source_note(BaseBranchSource::Configured),
            " (configured)"
        );
        assert_eq!(
            base_source_note(BaseBranchSource::RemoteHead),
            " (from origin)"
        );
        assert_eq!(
            base_source_note(BaseBranchSource::Conventional),
            " (guessed)"
        );
    }
}
