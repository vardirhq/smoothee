//! `smoothee undo` — reverse the last Smoothee-managed operation.
//!
//! Presentation and approval over [`operations::undo`]. It shows what will be
//! reversed and the restore point involved, requires an explicit confirmation
//! (undo *discards* the current post-operation state, so the default is "no"),
//! and reports what it did.

use anyhow::{Context, Result};

use crate::git::Repository;
use crate::operations::journal::Journal;
use crate::operations::undo::{self, UndoReport};
use crate::ui::{output, prompt};

/// Flags parsed for `smoothee undo`.
#[derive(Debug, Clone, Copy, Default)]
pub struct UndoArgs {
    pub yes: bool,
}

/// Entry point for the `undo` subcommand.
pub fn run(args: UndoArgs) -> Result<()> {
    let repo = Repository::discover_from_cwd()
        .context("this does not look like a Git repository (or git is not installed)")?;
    let journal = Journal::for_git_dir(repo.git_dir());

    let operations = journal
        .operations()
        .context("reading the operation journal")?;
    let Some(target) = undo::find_undo_target(&operations) else {
        println!("{}", output::ok("Nothing to undo."));
        println!(
            "  {}",
            output::label("Smoothee has no reversible operations recorded for this repository.")
        );
        return Ok(());
    };

    print_target(&target);

    println!();
    if !prompt::confirm("Restore previous state?", false, args.yes) {
        println!("  {}", output::label("Left as-is. Nothing was changed."));
        return Ok(());
    }

    let report = undo::perform(&repo, &journal, &target).context("undoing the last operation")?;
    print_report(&report);
    Ok(())
}

fn print_target(target: &crate::operations::journal::OperationRecord) {
    println!("{}", output::label("Last operation:"));
    println!(
        "{}",
        output::bullet(&describe(&target.kind, &target.branch))
    );

    if let Some(restore_ref) = &target.before.restore_ref {
        let display = restore_ref.strip_prefix("refs/").unwrap_or(restore_ref);
        println!();
        println!("  {}", output::label(&format!("Restore point: {display}")));
    }
}

fn print_report(report: &UndoReport) {
    if let Some(op) = report.aborted {
        println!("{}", output::ok(&format!("Aborted the in-progress {op}.")));
    }
    println!(
        "{}",
        output::ok(&format!(
            "Reversed: {}. No changes have been lost.",
            describe(&report.kind, &report.branch)
        ))
    );
    println!(
        "  {}",
        output::label(&format!("{} → {}", short(&report.from), short(&report.to)))
    );
}

/// Plain-language description of an operation kind for the "Last operation" line.
fn describe(kind: &str, branch: &str) -> String {
    match kind {
        "sync_rebase" => format!("Synced {branch} using rebase"),
        "sync_merge" => format!("Synced {branch} using merge"),
        "resolve_merge" => format!("Resolved conflicts on {branch} (merge)"),
        "resolve_rebase" => format!("Resolved conflicts on {branch} (rebase)"),
        other => format!("{other} on {branch}"),
    }
}

/// Abbreviate a commit SHA for display, mirroring Git's short hashes.
fn short(sha: &str) -> &str {
    if sha.len() >= 8 {
        &sha[..8]
    } else {
        sha
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_known_and_unknown_kinds() {
        assert_eq!(
            describe("sync_rebase", "feature"),
            "Synced feature using rebase"
        );
        assert_eq!(
            describe("sync_merge", "feature"),
            "Synced feature using merge"
        );
        assert_eq!(
            describe("resolve_merge", "feature"),
            "Resolved conflicts on feature (merge)"
        );
        assert_eq!(describe("other_op", "feature"), "other_op on feature");
    }

    #[test]
    fn short_abbreviates_long_shas() {
        assert_eq!(short("0123456789abcdef"), "01234567");
        assert_eq!(short("abc"), "abc");
    }
}
