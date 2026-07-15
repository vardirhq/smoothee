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
    Sync,
    /// Guided merge-conflict resolution.
    Resolve,
    /// Reverse the last Smoothee-managed operation.
    Undo,
    /// Create a GitHub pull request from the current branch.
    Pr,
}

impl Cli {
    /// Run the parsed command.
    pub fn run(self) -> Result<()> {
        match self.command {
            Command::Status => commands::status::run(),
            Command::Sync => commands::not_yet_implemented("sync", "Phase 2: Safe synchronization"),
            Command::Resolve => {
                commands::not_yet_implemented("resolve", "Phase 3: Conflict workflow")
            }
            Command::Undo => commands::not_yet_implemented("undo", "Phase 2: Safe synchronization"),
            Command::Pr => commands::not_yet_implemented("pr", "Phase 4: GitHub workflow"),
        }
    }
}
