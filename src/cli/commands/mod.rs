//! Subcommand implementations.

pub mod commit;
pub mod doctor;
pub mod resolve;
pub mod status;
pub mod sync;
pub mod undo;

use anyhow::Result;

use crate::ui::output;

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
        output::label(
            "Today, `smoothee status`, `sync`, `resolve`, `commit`, `undo`, and `doctor` are available."
        )
    );
    Ok(())
}
