//! `smoothee resolve` — guided merge-conflict resolution.
//!
//! Presentation and approval over [`operations::resolve`]. It explains each
//! conflict in the user's own terms (your changes vs the incoming ones), lets
//! them keep a side, edit by hand, or skip, validates edits, and — once nothing
//! is left conflicted — offers to finish the merge/rebase. Every choice is the
//! user's; the engine never invents a resolution.

use anyhow::{Context, Result};

use crate::git::conflicts::{ConflictContext, ConflictFile, Side};
use crate::git::Repository;
use crate::operations::journal::{Journal, OperationRecord};
use crate::operations::resolve::{FinishOutcome, ResolveEngine, ResolveState};
use crate::ui::{output, prompt};

/// Entry point for the `resolve` subcommand.
pub fn run() -> Result<()> {
    let repo = Repository::discover_from_cwd()
        .context("this does not look like a Git repository (or git is not installed)")?;
    let journal = Journal::for_git_dir(repo.git_dir());
    let engine = ResolveEngine::new(&repo, &journal);

    let (context, files) = match engine.inspect().context("inspecting for conflicts")? {
        ResolveState::Clean => {
            print_clean();
            return Ok(());
        }
        ResolveState::InProgress { context, files } => (context, files),
    };

    print_overview(context, &files);

    // Resolution is a sequence of human choices; without an interactive terminal
    // we cannot gather them. Explain the state and stop rather than guess.
    if !prompt::is_interactive() {
        print_non_interactive_guidance(context);
        return Ok(());
    }

    let branch = repo
        .current_branch()
        .context("reading the current branch")?
        .unwrap_or_else(|| "HEAD".to_string());

    // From here on we mutate: create the restore point and journal the operation
    // before staging anything, so `smoothee undo` can reverse all of it.
    let (record, restore) = engine
        .start(&branch, context)
        .context("starting a reversible resolution")?;
    println!();
    println!(
        "  {}",
        output::label(&format!("Restore point: {}", restore.display_name()))
    );

    let conflicts = engine.conflict_files().context("reading conflicts")?;
    let mut skipped = 0u32;
    for file in &conflicts {
        if resolve_file(&engine, context, file)? {
            // resolved
        } else {
            skipped += 1;
        }
    }

    let remaining = engine.remaining().context("re-checking conflicts")?;
    if !remaining.is_empty() {
        print_still_pending(&remaining, skipped);
        return Ok(());
    }

    offer_finish(&engine, &repo, context, &record)
}

/// Work through one file. Returns `true` if it ended resolved, `false` if skipped.
fn resolve_file(
    engine: &ResolveEngine,
    context: ConflictContext,
    file: &ConflictFile,
) -> Result<bool> {
    loop {
        print_conflict_header(context, file);

        let choice = if file.readable {
            prompt::choose(&menu(context, true), &['y', 'i', 'e', 'd', 's'])
        } else {
            // A binary or delete/modify conflict cannot be edited or diffed.
            prompt::choose(&menu(context, false), &['y', 'i', 's'])
        };

        match choice {
            Some('y') => {
                engine
                    .take_side(&file.path, context.mine())
                    .with_context(|| format!("keeping your changes to {}", file.path))?;
                print_kept(&file.path, "your changes");
                return Ok(true);
            }
            Some('i') => {
                engine
                    .take_side(&file.path, context.incoming())
                    .with_context(|| format!("keeping incoming changes to {}", file.path))?;
                print_kept(&file.path, context.incoming_label());
                return Ok(true);
            }
            Some('e') => match edit_file(engine, file) {
                EditResult::Resolved => {
                    print_kept(&file.path, "your edit");
                    return Ok(true);
                }
                EditResult::MarkersRemain => {
                    println!(
                        "{}",
                        output::warn(&format!(
                            "{} still has conflict markers — not staged. Try again.",
                            file.path
                        ))
                    );
                    // loop and re-present the menu
                }
                EditResult::Failed(msg) => {
                    println!("{}", output::warn(&msg));
                }
            },
            Some('d') => print_full_conflict(context, file),
            Some('s') | None => {
                println!(
                    "  {}",
                    output::label(&format!("Skipped {} for now.", file.path))
                );
                return Ok(false);
            }
            _ => {}
        }
    }
}

/// Outcome of a hand-edit attempt.
enum EditResult {
    Resolved,
    MarkersRemain,
    Failed(String),
}

/// Open the user's editor on a conflicted file, then validate and stage it.
fn edit_file(engine: &ResolveEngine, file: &ConflictFile) -> EditResult {
    if let Err(err) = open_in_editor(engine.workdir().join(&file.path).as_path()) {
        return EditResult::Failed(format!("could not open an editor: {err}"));
    }
    match engine.stage_edited(&file.path) {
        Ok(()) => EditResult::Resolved,
        Err(crate::operations::resolve::ResolveError::MarkersRemain { .. }) => {
            EditResult::MarkersRemain
        }
        Err(err) => EditResult::Failed(err.to_string()),
    }
}

/// Launch `$EDITOR`/`$VISUAL` (falling back to `vi`) on `path`, via the shell so
/// values like `code -w` work, and wait for it to exit.
fn open_in_editor(path: &std::path::Path) -> std::io::Result<()> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| default_editor().to_string());

    let quoted = format!("'{}'", path.display().to_string().replace('\'', "'\\''"));
    let command = format!("{editor} {quoted}");

    let status = shell(&command).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("editor exited with an error"))
    }
}

#[cfg(windows)]
fn default_editor() -> &'static str {
    "notepad"
}

#[cfg(not(windows))]
fn default_editor() -> &'static str {
    "vi"
}

#[cfg(windows)]
fn shell(command: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new("cmd");
    cmd.arg("/C").arg(command);
    cmd
}

#[cfg(not(windows))]
fn shell(command: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c").arg(command);
    cmd
}

/// Once nothing is left conflicted, show what finishing runs and, on approval,
/// conclude the merge/rebase.
fn offer_finish(
    engine: &ResolveEngine,
    repo: &Repository,
    context: ConflictContext,
    record: &OperationRecord,
) -> Result<()> {
    println!();
    println!("{}", output::ok("All conflicts resolved."));

    let finish_cmd = match context {
        ConflictContext::Rebase => repo.git("rebase").arg("--continue"),
        _ => repo.git("commit").arg("--no-edit"),
    };
    println!();
    println!("{}", output::running(&finish_cmd.display()));

    println!();
    if !prompt::confirm(&format!("Finish the {} now?", context.verb()), true, false) {
        print_finish_deferred(context);
        return Ok(());
    }

    match engine
        .finish(record, context)
        .context("finishing the operation")?
    {
        FinishOutcome::Completed { .. } => {
            println!(
                "{}",
                output::ok(&format!(
                    "The {} is complete. No changes have been lost.",
                    context.verb()
                ))
            );
            println!(
                "  {}",
                output::label("Changed your mind? `smoothee undo` reverses the resolution.")
            );
        }
        FinishOutcome::MoreConflicts { files } => {
            println!(
                "{}",
                output::warn("The rebase advanced, then stopped on the next commit's conflicts.")
            );
            print_file_list(&files);
            println!();
            println!(
                "  {}",
                output::label(
                    "Run `smoothee resolve` again to continue, or `smoothee undo` to stop."
                )
            );
        }
        FinishOutcome::Blocked { files } => {
            println!(
                "{}",
                output::warn("Some files are still conflicted, so the operation can't finish yet.")
            );
            print_file_list(&files);
        }
    }
    Ok(())
}

fn print_clean() {
    println!("{}", output::ok("Nothing to resolve."));
    println!(
        "  {}",
        output::label("No merge or rebase is in progress, and no files are conflicted.")
    );
}

fn print_overview(context: ConflictContext, files: &[String]) {
    println!(
        "{}",
        output::warn(&format!(
            "A {} is in progress with {} conflicting file{}. No changes have been lost.",
            context.verb(),
            files.len(),
            plural(files.len()),
        ))
    );
    print_file_list(files);
}

fn print_non_interactive_guidance(context: ConflictContext) {
    println!();
    println!(
        "  {}",
        output::label("Guided resolution needs an interactive terminal.")
    );
    println!(
        "  {}",
        output::label(&format!(
            "Resolve the files, then finish with: {}",
            finish_hint(context)
        ))
    );
    println!(
        "  {}",
        output::label("Or `smoothee undo` to return to before the operation.")
    );
}

fn print_conflict_header(context: ConflictContext, file: &ConflictFile) {
    println!();
    println!("{}", output::heading(&format!("File: {}", file.path)));
    if !file.readable {
        println!(
            "  {}",
            output::label("Binary or non-text conflict — choose a whole side.")
        );
        return;
    }
    let n = file.hunks.len();
    println!(
        "  {}",
        output::label(&format!(
            "{n} conflicting section{} — your changes vs {}.",
            plural(n),
            context.incoming_label()
        ))
    );
}

/// Render the per-file action menu.
fn menu(context: ConflictContext, editable: bool) -> String {
    let incoming = context.incoming_label();
    if editable {
        format!("[y] keep your changes  [i] keep {incoming}  [e] edit  [d] show diff  [s] skip")
    } else {
        format!("[y] keep your changes  [i] keep {incoming}  [s] skip")
    }
}

fn print_full_conflict(context: ConflictContext, file: &ConflictFile) {
    for (i, hunk) in file.hunks.iter().enumerate() {
        println!();
        println!("{}", output::label(&format!("Section {}:", i + 1)));
        println!("  {}", output::label("your changes:"));
        print_side(&side_lines(context, hunk, context.mine()));
        if let Some(base) = &hunk.base {
            println!("  {}", output::label("original (common ancestor):"));
            print_side(base);
        }
        println!(
            "  {}",
            output::label(&format!("{}:", context.incoming_label()))
        );
        print_side(&side_lines(context, hunk, context.incoming()));
    }
}

/// The lines for a given [`Side`] of a hunk.
fn side_lines(
    _context: ConflictContext,
    hunk: &crate::git::conflicts::ConflictHunk,
    side: Side,
) -> Vec<String> {
    match side {
        Side::Ours => hunk.ours.clone(),
        Side::Theirs => hunk.theirs.clone(),
    }
}

fn print_side(lines: &[String]) {
    if lines.is_empty() {
        println!("    {}", output::label("(nothing)"));
        return;
    }
    for line in lines {
        println!("    {line}");
    }
}

fn print_kept(path: &str, what: &str) {
    println!("{}", output::ok(&format!("Kept {what} for {path}.")));
}

fn print_still_pending(remaining: &[String], skipped: u32) {
    println!();
    println!(
        "{}",
        output::warn(&format!(
            "Still {} file{} to resolve. No changes have been lost.",
            remaining.len(),
            plural(remaining.len()),
        ))
    );
    print_file_list(remaining);
    if skipped > 0 {
        println!(
            "  {}",
            output::label(&format!(
                "You skipped {skipped}. Re-run `smoothee resolve` to finish."
            ))
        );
    }
    println!(
        "  {}",
        output::label("Or `smoothee undo` to return to before the operation.")
    );
}

fn print_finish_deferred(context: ConflictContext) {
    println!(
        "  {}",
        output::label("Everything is staged, but the operation isn't finished yet.")
    );
    println!(
        "  {}",
        output::label(&format!("Finish when ready with: {}", finish_hint(context)))
    );
}

fn print_file_list(files: &[String]) {
    for file in files {
        println!("{}", output::bullet(file));
    }
}

fn finish_hint(context: ConflictContext) -> &'static str {
    match context {
        ConflictContext::Rebase => "git rebase --continue",
        _ => "git commit --no-edit",
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_names_the_incoming_side_by_context() {
        assert!(menu(ConflictContext::Merge, true).contains("the incoming branch"));
        assert!(menu(ConflictContext::Rebase, true).contains("the base branch"));
        // Non-editable menu drops edit/diff.
        let m = menu(ConflictContext::Merge, false);
        assert!(!m.contains("edit"));
        assert!(m.contains("skip"));
    }

    #[test]
    fn finish_hint_matches_context() {
        assert_eq!(
            finish_hint(ConflictContext::Rebase),
            "git rebase --continue"
        );
        assert_eq!(finish_hint(ConflictContext::Merge), "git commit --no-edit");
    }

    #[test]
    fn side_lines_pick_the_requested_side() {
        let hunk = crate::git::conflicts::ConflictHunk {
            ours: vec!["o".into()],
            base: None,
            theirs: vec!["t".into()],
        };
        assert_eq!(
            side_lines(ConflictContext::Merge, &hunk, Side::Ours),
            vec!["o".to_string()]
        );
        assert_eq!(
            side_lines(ConflictContext::Merge, &hunk, Side::Theirs),
            vec!["t".to_string()]
        );
    }
}
