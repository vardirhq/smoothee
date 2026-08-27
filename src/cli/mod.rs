//! Command-line surface.

pub mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// Smoothee — make Git smooth.
#[derive(Debug, Parser)]
#[command(name = "smoothee", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Explain repository state in plain language.
    Status,
    /// Safely update the current branch (fetch, restore point, merge/rebase).
    Sync {
        #[arg(long, conflicts_with = "merge")]
        rebase: bool,
        #[arg(long)]
        merge: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        no_verify: bool,
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Guided merge-conflict resolution.
    Resolve,
    /// Create an intentional commit from staged changes or one logical change group.
    Commit {
        #[arg(short = 'm', long)]
        message: Option<String>,
        #[arg(short = 'a', long, conflicts_with = "group")]
        all: bool,
        #[arg(long, value_name = "N", conflicts_with = "all")]
        group: Option<usize>,
        #[arg(long)]
        dry_run: bool,
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Reverse the last Smoothee-managed operation.
    Undo {
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Check Git, GitHub CLI, repository, and Smoothee configuration.
    Doctor,
    /// Inspect and create a GitHub pull request from the current branch.
    Pr {
        /// Override the proposed pull-request title.
        #[arg(long)]
        title: Option<String>,
        /// Override the generated pull-request body.
        #[arg(long)]
        body: Option<String>,
        /// Create the pull request as a draft.
        #[arg(long)]
        draft: bool,
        /// Deliberately publish/update the current branch on origin first.
        #[arg(long)]
        push: bool,
        /// Show the plan and commands without pushing or creating anything.
        #[arg(long)]
        dry_run: bool,
        /// Proceed without the confirmation prompt (for automation).
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

impl Cli {
    pub fn run(self) -> Result<()> {
        match self.command {
            Command::Status => commands::status::run(),
            Command::Sync {
                rebase,
                merge,
                dry_run,
                no_verify,
                yes,
            } => commands::sync::run(commands::sync::SyncArgs {
                rebase,
                merge,
                dry_run,
                no_verify,
                yes,
            }),
            Command::Resolve => commands::resolve::run(),
            Command::Commit {
                message,
                all,
                group,
                dry_run,
                yes,
            } => commands::commit::run(commands::commit::CommitArgs {
                message,
                all,
                group,
                dry_run,
                yes,
            }),
            Command::Undo { yes } => commands::undo::run(commands::undo::UndoArgs { yes }),
            Command::Doctor => commands::doctor::run(),
            Command::Pr {
                title,
                body,
                draft,
                push,
                dry_run,
                yes,
            } => commands::pr::run(commands::pr::PrArgs {
                title,
                body,
                draft,
                push,
                dry_run,
                yes,
            }),
        }
    }
}
