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

/// Whether stdin is an interactive terminal, so a menu can actually be answered.
///
/// Commands that must gather a human choice (e.g. `resolve`) check this and
/// decline to mutate when it is false, rather than guessing.
pub fn is_interactive() -> bool {
    io::stdin().is_terminal()
}

/// Present a single-key menu and return the chosen key.
///
/// Reads a line, lower-cases it, and accepts its first character if it is in
/// `allowed`; otherwise it re-asks. Returns `None` on EOF or a non-interactive
/// stdin, so callers can fall back to non-mutating behaviour instead of looping
/// forever. `question` should already list the options.
pub fn choose(question: &str, allowed: &[char]) -> Option<char> {
    if !io::stdin().is_terminal() {
        return None;
    }

    loop {
        print!("{question} ");
        let _ = io::stdout().flush();

        let mut line = String::new();
        if io::stdin().read_line(&mut line).ok()? == 0 {
            return None; // EOF
        }

        if let Some(ch) = line.trim().to_ascii_lowercase().chars().next() {
            if allowed.contains(&ch) {
                return Some(ch);
            }
        }
        println!("  Please choose one of: {}", render_allowed(allowed));
    }
}

/// Render an allowed-key set like `y / i / e / d / s` for the re-ask hint.
fn render_allowed(allowed: &[char]) -> String {
    allowed
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(" / ")
}
