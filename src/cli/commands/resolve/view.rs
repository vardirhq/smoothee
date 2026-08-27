use crate::git::conflicts::{ConflictContext, ConflictFile, Side};
use crate::ui::output;

pub(super) fn print_clean() {
    println!("{}", output::ok("Nothing to resolve."));
    println!(
        "  {}",
        output::label("No merge or rebase is in progress, and no files are conflicted.")
    );
}

pub(super) fn print_overview(context: ConflictContext, files: &[String]) {
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

pub(super) fn print_non_interactive_guidance(context: ConflictContext) {
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

pub(super) fn print_conflict_header(context: ConflictContext, file: &ConflictFile) {
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

pub(super) fn menu(context: ConflictContext, editable: bool) -> String {
    let incoming = context.incoming_label();
    if editable {
        format!("[y] keep your changes  [i] keep {incoming}  [e] edit  [d] show diff  [s] skip")
    } else {
        format!("[y] keep your changes  [i] keep {incoming}  [s] skip")
    }
}

pub(super) fn print_full_conflict(context: ConflictContext, file: &ConflictFile) {
    for (i, hunk) in file.hunks.iter().enumerate() {
        println!();
        println!("{}", output::label(&format!("Section {}:", i + 1)));
        println!("  {}", output::label("your changes:"));
        print_side(&side_lines(hunk, context.mine()));
        if let Some(base) = &hunk.base {
            println!("  {}", output::label("original (common ancestor):"));
            print_side(base);
        }
        println!(
            "  {}",
            output::label(&format!("{}:", context.incoming_label()))
        );
        print_side(&side_lines(hunk, context.incoming()));
    }
}

fn side_lines(hunk: &crate::git::conflicts::ConflictHunk, side: Side) -> Vec<String> {
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

pub(super) fn print_kept(path: &str, what: &str) {
    println!("{}", output::ok(&format!("Kept {what} for {path}.")));
}

pub(super) fn print_still_pending(remaining: &[String], skipped: u32) {
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

pub(super) fn print_finish_deferred(context: ConflictContext) {
    println!(
        "  {}",
        output::label("Everything is staged, but the operation isn't finished yet.")
    );
    println!(
        "  {}",
        output::label(&format!("Finish when ready with: {}", finish_hint(context)))
    );
}

pub(super) fn print_file_list(files: &[String]) {
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
    if n == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_names_the_incoming_side_by_context() {
        assert!(menu(ConflictContext::Merge, true).contains("the incoming branch"));
        assert!(menu(ConflictContext::Rebase, true).contains("the base branch"));
        let m = menu(ConflictContext::Merge, false);
        assert!(!m.contains("edit"));
        assert!(m.contains("skip"));
    }

    #[test]
    fn finish_hint_matches_context() {
        assert_eq!(finish_hint(ConflictContext::Rebase), "git rebase --continue");
        assert_eq!(finish_hint(ConflictContext::Merge), "git commit --no-edit");
    }

    #[test]
    fn side_lines_pick_the_requested_side() {
        let hunk = crate::git::conflicts::ConflictHunk {
            ours: vec!["o".into()],
            base: None,
            theirs: vec!["t".into()],
        };
        assert_eq!(side_lines(&hunk, Side::Ours), vec!["o".to_string()]);
        assert_eq!(side_lines(&hunk, Side::Theirs), vec!["t".to_string()]);
    }
}
