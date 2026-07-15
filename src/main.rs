//! Smoothee — a safer, clearer Git and GitHub workflow.
//!
//! Binary entry point. Argument parsing and dispatch live in [`cli`]; this file
//! only wires them together and turns errors into a clean, non-panicking exit.

mod cli;
mod config;
mod git;
mod operations;
mod ui;
mod verification;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::Cli;
use crate::ui::output;

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // Clarity in failure states: lead with the problem, then the chain
            // of causes, each on its own line. No stack traces, no panics.
            eprintln!("{}", output::warn(&err.to_string()));
            for cause in err.chain().skip(1) {
                eprintln!("  {}", output::label(&format!("caused by: {cause}")));
            }
            ExitCode::FAILURE
        }
    }
}
