//! Subcommand implementations.
//!
//! Phase 1 shipped `status`; Phase 2 adds the mutating `sync` and `undo`. The
//! remaining MVP commands are declared in the CLI surface so `--help` reflects
//! the real product shape, and each reports its roadmap phase rather than
//! pretending to work.

pub mod resolve;
pub mod status;
pub mod sync;
pub mod undo;

use anyhow::Result;

use crate::ui::output;

/// Placeholder for a command that is on the roadmap but not yet implemented.
///
/// Honesty over theatrics: tell the user plainly that the command is planned,
/// and which phase delivers it, rather than failing cryptically.
pub fn not_yet_implemented(command: &str, phase: &str) -> Result<()> {
    println!(
        "{}",
        output::warn(&format!(
            "`smoothee {command}` is planned but not implemented yet."
        ))
    );
    println!("  {}", output::label(&format!("Arriving in: {phase}")));
    println!(
        "  {}",
        output::label("Today, `smoothee status`, `sync`, `resolve`, and `undo` are available.")
    );
    Ok(())
}
