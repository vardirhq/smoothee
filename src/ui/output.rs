//! Terminal output helpers.
//!
//! A thin, dependency-light styling layer over [`console`]. Centralising it
//! keeps Smoothee's tone consistent — calm, clear, reassuring in failure — and
//! makes the "always show the git commands" principle a single call away.
//!
//! Colour is disabled automatically when stdout is not a terminal or when
//! `NO_COLOR` is set, so piped output stays clean.
//!
//! Some helpers (`ok`, `running`, `is_interactive`) exist for the mutating
//! commands landing in Phase 2 — notably the "Running: git …" transparency
//! block — and are not yet called from `status`.
#![allow(dead_code)]

use console::{style, Term};

/// A styled section heading (bold).
pub fn heading(text: &str) -> String {
    style(text).bold().to_string()
}

/// A dimmed label, e.g. field names in a status block.
pub fn label(text: &str) -> String {
    style(text).dim().to_string()
}

/// A success marker: green check plus message.
pub fn ok(text: &str) -> String {
    format!("{} {}", style("✓").green(), text)
}

/// A warning marker: yellow sign plus message.
pub fn warn(text: &str) -> String {
    format!("{} {}", style("⚠").yellow(), text)
}

/// An informational bullet.
pub fn bullet(text: &str) -> String {
    format!("  {} {}", style("•").dim(), text)
}

/// Render the "Running:" transparency block for a git command, honouring the
/// "preserve access to Git" principle — users always see the real command.
pub fn running(command: &str) -> String {
    format!("{}\n  {}", label("Running:"), style(command).cyan())
}

/// Emphasise a recommended next step.
pub fn recommend(text: &str) -> String {
    style(text).cyan().bold().to_string()
}

/// Whether the current stdout is an interactive terminal. Callers can use this
/// to decide between rich and plain rendering.
pub fn is_interactive() -> bool {
    Term::stdout().is_term()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_include_their_message() {
        // Styling may be stripped when not a tty, but the text must survive.
        assert!(ok("done").contains("done"));
        assert!(warn("careful").contains("careful"));
        assert!(bullet("item").contains("item"));
        assert!(running("git status").contains("git status"));
    }
}
