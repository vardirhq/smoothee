//! `smoothee resolve` — guided merge-conflict resolution.
//!
//! Presentation and approval over [`operations::resolve`]. It explains each
//! conflict in the user's own terms, gathers explicit choices, validates edits,
//! and offers to finish the merge/rebase once conflicts are gone.

mod view;

use anyhow::{Context, Result};

use crate::git::conflicts::{ConflictContext, ConflictFile};
use crate::git::Repository;
use crate::operations::journal::{Journal, OperationRecord};
use crate::operations::resolve::{FinishOutcome, ResolveEngine, ResolveState};
use crate::ui::{output, prompt};
use view::{
    menu, print_clean, print_conflict_header, print_file_list, print_finish_deferred,
    print_full_conflict, print_kept, print_non_interactive_guidance, print_overview,
    print_still_pending,
};

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
    if !prompt::is_interactive() {
        print_non_interactive_guidance(context);
        return Ok(());
    }

    let branch = repo
        .current_branch()
        .context("reading the current branch")?
        .unwrap_or_else(|| "HEAD".to_string());
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
        if !resolve_file(&engine, context, file)? {
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
                EditResult::MarkersRemain => println!(
                    "{}",
                    output::warn(&format!(
                        "{} still has conflict markers — not staged. Try again.",
                        file.path
                    ))
                ),
                EditResult::Failed(msg) => println!("{}", output::warn(&msg)),
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

enum EditResult {
    Resolved,
    MarkersRemain,
    Failed(String),
}

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

fn open_in_editor(path: &std::path::Path) -> std::io::Result<()> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| default_editor().to_string());
    let quoted = format!("'{}'", path.display().to_string().replace('\'', "'\\''"));
    let status = shell(&format!("{editor} {quoted}")).status()?;
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
