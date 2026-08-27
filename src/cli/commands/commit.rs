//! `smoothee commit` — build one intentional, reviewable commit.

use anyhow::{Context, Result};

use crate::git::commit::{
    commit_command, stage_command, suggested_message, ChangeGroup, CommitPlan,
};
use crate::git::Repository;
use crate::ui::{output, prompt};

#[derive(Debug, Clone, Default)]
pub struct CommitArgs {
    pub message: Option<String>,
    pub all: bool,
    pub group: Option<usize>,
    pub dry_run: bool,
    pub yes: bool,
}

pub fn run(args: CommitArgs) -> Result<()> {
    let repo = Repository::discover_from_cwd()
        .context("this does not look like a Git repository (or git is not installed)")?;
    let plan = CommitPlan::inspect(&repo).context("inspecting changes")?;

    if plan.is_empty() {
        println!(
            "{}",
            output::ok("Nothing to commit. The working tree is clean.")
        );
        return Ok(());
    }

    print_changes(&plan);

    if args.group.is_some() && plan.has_staged() {
        println!();
        println!(
            "{}",
            output::warn("A logical group cannot be isolated while other changes are staged.")
        );
        println!(
            "  {}",
            output::label(
                "Commit the staged selection first, or unstage it before using `--group N`."
            )
        );
        return Ok(());
    }

    let selected_group = select_group(&plan, &args)?;
    let mut stage_paths = Vec::new();

    if plan.has_staged() && !args.all {
        println!();
        println!("{}", output::label("Plan:"));
        println!(
            "{}",
            output::bullet("Keep the current staged selection exactly as-is")
        );
    } else if args.all {
        stage_paths = plan
            .changes
            .iter()
            .map(|change| change.path.clone())
            .collect();
        println!();
        println!("{}", output::label("Plan:"));
        println!(
            "{}",
            output::bullet("Stage every current working-tree change")
        );
    } else if let Some(group) = selected_group {
        stage_paths = group.paths.clone();
        println!();
        println!("{}", output::label("Plan:"));
        println!(
            "{}",
            output::bullet(&format!("Stage only the `{}` change group", group.scope))
        );
        for path in &group.paths {
            println!("  {}", output::label(path));
        }
    } else {
        println!();
        println!(
            "{}",
            output::warn("There are multiple unrelated-looking change groups.")
        );
        println!(
            "  {}",
            output::label("Choose one with `--group N`, or use `--all` deliberately.")
        );
        return Ok(());
    }

    let staged = plan.staged_paths();
    let message = args
        .message
        .clone()
        .unwrap_or_else(|| suggested_message(selected_group, &staged));

    println!();
    println!("{}", output::label("Commit message:"));
    println!("  {message}");

    if !stage_paths.is_empty() {
        println!();
        println!(
            "{}",
            output::running(&stage_command(&repo, &stage_paths).display())
        );
    }
    println!(
        "{}",
        output::running(&commit_command(&repo, &message).display())
    );

    if args.dry_run {
        println!();
        println!(
            "  {}",
            output::label("Dry run: no files staged and no commit created.")
        );
        return Ok(());
    }

    println!();
    if !prompt::confirm("Create this commit?", false, args.yes) {
        println!("  {}", output::label("No changes made."));
        return Ok(());
    }

    if !stage_paths.is_empty() {
        let result = stage_command(&repo, &stage_paths)
            .run()
            .context("staging selected changes")?;
        if !result.success {
            anyhow::bail!("git add failed: {}", result.stderr);
        }
    }

    let result = commit_command(&repo, &message)
        .run()
        .context("creating commit")?;
    if !result.success {
        anyhow::bail!("git commit failed: {}", result.stderr);
    }

    println!();
    println!("{}", output::ok("Commit created."));
    if !result.stdout.is_empty() {
        println!("  {}", output::label(&result.stdout));
    }
    Ok(())
}

fn select_group<'a>(plan: &'a CommitPlan, args: &CommitArgs) -> Result<Option<&'a ChangeGroup>> {
    if args.all || (plan.has_staged() && args.group.is_none()) {
        return Ok(None);
    }
    if let Some(index) = args.group {
        let group = plan.groups.get(index.saturating_sub(1));
        if group.is_none() {
            anyhow::bail!(
                "--group {index} does not exist; run `smoothee commit --dry-run` to inspect groups"
            );
        }
        return Ok(group);
    }
    if plan.groups.len() == 1 {
        return Ok(plan.groups.first());
    }
    Ok(None)
}

fn print_changes(plan: &CommitPlan) {
    println!("{}", output::label("Changes:"));
    for change in &plan.changes {
        let state = if change.untracked {
            "untracked"
        } else if change.staged && change.unstaged {
            "staged + modified again"
        } else if change.staged {
            "staged"
        } else {
            "unstaged"
        };
        println!(
            "{}",
            output::bullet(&format!("{} ({state})", change.path))
        );
    }

    if !plan.has_staged() && plan.groups.len() > 1 {
        println!();
        println!("{}", output::label("Logical groups:"));
        for (index, group) in plan.groups.iter().enumerate() {
            println!(
                "{}",
                output::bullet(&format!(
                    "{}. {} ({} files)",
                    index + 1,
                    group.scope,
                    group.paths.len()
                ))
            );
        }
    }
}
