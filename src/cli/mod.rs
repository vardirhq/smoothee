//! Command-line surface.
//!
//! Declares the full MVP command set with [`clap`] so `--help` reflects the
//! product's real shape, and dispatches each to its implementation.

pub mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// Smoothee — make Git smooth.
///
/// A safer, clearer Git and GitHub workflow. Smoothee sits above Git: it
/// explains repository state, synchronises branches safely, guides conflict
/// resolution, and makes risky operations reversible.
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
        /// Force a rebase, overriding configuration and auto-detection.
        #[arg(long, conflicts_with = "merge")]
        rebase: bool,
        /// Force a merge, overriding configuration and auto-detection.
        #[arg(long)]
        merge: bool,
        /// Show the plan without making any changes.
        #[arg(long)]
        dry_run: bool,
        /// Skip the configured verification checks after syncing.
        #[arg(long)]
        no_verify: bool,
        /// Proceed without the confirmation prompt (for automation).
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Guided merge-conflict resolution.
    Resolve,
    /// Reverse the last Smoothee-managed operation.
    Undo {
        /// Proceed without the confirmation prompt (for automation).
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Create a GitHub pull request from the current branch.
    Pr,
}

impl Cli {
    /// Run the parsed command.
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
            Command::Undo { yes } => commands::undo::run(commands::undo::UndoArgs { yes }),
            Command::Pr => commands::not_yet_implemented("pr", "Phase 4: GitHub workflow"),
        }
    }
}
