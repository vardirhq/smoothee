//! Interactive confirmation prompts.
//!
//! "Explain before acting" ends in a yes/no gate before any mutation. This
//! keeps that gate in one place with two safety properties:
//!
//! * When stdin is not a terminal (CI, pipes) and the user did not pass
//!   `--yes`, a prompt cannot be answered, so we decline rather than guess —
//!   never mutate a repository on an assumed "yes".
//! * `--yes` is an explicit, auditable opt-out for automation.

use std::io::{self, IsTerminal, Write};

/// Ask `question` and return whether the user approved.
///
/// `default_yes` selects the answer for a bare Enter and the `[Y/n]`/`[y/N]`
/// hint. `assume_yes` (from `--yes`) approves without prompting. A
/// non-interactive stdin with no `--yes` returns `false`.
pub fn confirm(question: &str, default_yes: bool, assume_yes: bool) -> bool {
    if assume_yes {
        return true;
    }
    if !io::stdin().is_terminal() {
        return false;
    }

    let hint = if default_yes { "[Y/n]" } else { "[y/N]" };
    print!("{question} {hint} ");
    let _ = io::stdout().flush();

    let mut line = String::new();
    if io::stdin().read_line(&mut line).is_err() {
        return false;
    }

    match line.trim().to_ascii_lowercase().as_str() {
        "" => default_yes,
        "y" | "yes" => true,
        _ => false,
    }
}
