//! `smoothee recover` — explicitly restore an earlier Smoothee state.

use anyhow::{Context, Result};

use crate::git::Repository;
use crate::operations::journal::Journal;
use crate::operations::recovery;
use crate::ui::{output, prompt};

#[derive(Debug, Clone)]
pub struct RecoverArgs {
    pub operation: String,
    pub dry_run: bool,
    pub yes: bool,
}

pub fn run(args: RecoverArgs) -> Result<()> {
    let repo = Repository::discover_from_cwd()
        .context("this does not look like a Git repository (or git is not installed)")?;
    let journal = Journal::for_git_dir(repo.git_dir());
    let plan = recovery::plan(&repo, &journal, &args.operation).context("planning recovery")?;

    println!("{}", output::heading("Recovery plan"));
    println!("{}", output::bullet(&format!("Operation: {}", plan.target.id)));
    println!("{}", output::bullet(&format!("Branch: {}", plan.target.branch)));
    println!(
        "{}",
        output::bullet(&format!("Current HEAD: {}", short_sha(&plan.current_head)))
    );
    println!(
        "{}",
        output::bullet(&format!("Restore to: {}", short_sha(&plan.target.before.head)))
    );
    println!("{}", output::bullet("Working tree: clean"));
    println!();
    println!(
        "{}",
        output::label("Smoothee will create a new restore point for the current HEAD first.")
    );

    let command = repo.git("reset").arg("--hard").arg(&plan.restore_ref);
    println!();
    println!("{}", output::running(&command.display()));

    if args.dry_run {
        println!();
        println!("{}", output::label("Dry run only; nothing changed."));
        return Ok(());
    }

    println!();
    if !prompt::confirm(
        "Recover this branch to the selected Smoothee restore point?",
        false,
        args.yes,
    ) {
        println!("{}", output::label("Recovery cancelled; nothing changed."));
        return Ok(());
    }

    let report = recovery::perform(&repo, &journal, &plan).context("performing recovery")?;
    println!();
    println!(
        "{}",
        output::ok(&format!(
            "Recovered {} from {} to {}.",
            plan.target.branch,
            short_sha(&report.from),
            short_sha(&report.to)
        ))
    );
    println!(
        "{}",
        output::label("The state you just left is also protected by a new restore point.")
    );
    println!(
        "{}",
        output::label("Changed your mind? `smoothee undo` reverses this recovery.")
    );
    Ok(())
}

fn short_sha(value: &str) -> &str {
    value.get(..value.len().min(8)).unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_sha_is_human_sized() {
        assert_eq!(short_sha("abcdef123456"), "abcdef12");
        assert_eq!(short_sha("abc"), "abc");
    }
}
