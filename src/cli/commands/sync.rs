//! `smoothee sync` — safely update the current branch against its base.
//!
//! This command is the presentation and approval layer over
//! [`operations::sync`]. It renders the plan, gets explicit approval, shows the
//! exact Git commands it runs, and reports the outcome — while every decision
//! and mutation lives in the deterministic engine.

use anyhow::{Context, Result};

use crate::config::RepoConfig;
use crate::git::Repository;
use crate::operations::journal::Journal;
use crate::operations::sync::{
    ResolvedStrategy, SyncEngine, SyncPlan, SyncPlanOutcome, SyncResult,
};
use crate::ui::{output, prompt};
use crate::verification;

/// Flags parsed for `smoothee sync`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SyncArgs {
    pub rebase: bool,
    pub merge: bool,
    pub dry_run: bool,
    pub no_verify: bool,
    pub yes: bool,
}

/// Entry point for the `sync` subcommand.
pub fn run(args: SyncArgs) -> Result<()> {
    let repo = Repository::discover_from_cwd()
        .context("this does not look like a Git repository (or git is not installed)")?;
    let config = RepoConfig::load(repo.workdir()).context("loading .smoothee.toml")?;
    let journal = Journal::for_git_dir(repo.git_dir());

    let engine = SyncEngine::new(&repo, &config, &journal);

    let cli_override = match (args.rebase, args.merge) {
        (true, false) => Some(ResolvedStrategy::Rebase),
        (false, true) => Some(ResolvedStrategy::Merge),
        _ => None,
    };

    let plan = match engine
        .plan(cli_override)
        .context("planning synchronization")?
    {
        SyncPlanOutcome::AlreadyUpToDate { ahead, base_name } => {
            print_up_to_date(ahead, &base_name);
            return Ok(());
        }
        SyncPlanOutcome::Planned(plan) => plan,
    };

    print_plan(&repo, &plan);

    if args.dry_run {
        println!();
        println!(
            "  {}",
            output::label("Dry run: no changes made. Re-run without --dry-run to sync.")
        );
        return Ok(());
    }

    println!();
    if !prompt::confirm("Continue?", true, args.yes) {
        println!("  {}", output::label("No changes made."));
        return Ok(());
    }

    // The plan already showed the exact command (the "Running:" block above);
    // the user approved it, so execute without repeating it.
    println!();
    match engine
        .execute(&plan)
        .context("performing synchronization")?
    {
        SyncResult::Completed { restore, strategy } => {
            print_completed(strategy, restore.display_name());
            if !args.no_verify {
                run_verification(&repo, &config);
            }
        }
        SyncResult::Conflicted {
            restore,
            files,
            strategy,
        } => {
            print_conflicted(strategy, restore.display_name(), &files);
        }
    }

    Ok(())
}

fn print_up_to_date(ahead: u32, base_name: &str) {
    println!(
        "{}",
        output::ok(&format!("Already up to date with {base_name}."))
    );
    if ahead > 0 {
        println!(
            "  {}",
            output::label(&format!(
                "You are {ahead} commit{} ahead — `smoothee pr` when you're ready to share.",
                plural(ahead)
            ))
        );
    }
}

fn print_plan(repo: &Repository, plan: &SyncPlan) {
    println!(
        "{}",
        output::ok(&format!("Fetched origin/{}.", plan.base_name))
    );
    println!();
    println!("{}", output::label("Your branch:"));
    println!(
        "{}",
        output::bullet(&format!(
            "{} commit{} ahead",
            plan.ahead,
            plural(plan.ahead)
        ))
    );
    println!(
        "{}",
        output::bullet(&format!(
            "{} commit{} behind {}",
            plan.behind,
            plural(plan.behind),
            plan.base_name
        ))
    );

    println!();
    println!("{}", output::label("Recommended action:"));
    println!(
        "{}",
        output::bullet(&format!(
            "{} onto {}",
            plan.strategy.label(),
            plan.remote_ref
        ))
    );

    println!();
    println!("{}", output::label("Why:"));
    println!("  {}", plan.reason);

    println!();
    println!("{}", output::label("Plan:"));
    println!(
        "{}",
        output::bullet("Create a restore point (so this is reversible)")
    );
    println!(
        "{}",
        output::bullet(&format!(
            "{} onto {}",
            plan.strategy.label(),
            plan.remote_ref
        ))
    );
    println!(
        "{}",
        output::bullet("Report the result (and run verification checks)")
    );

    // Preview the command without running it yet.
    println!();
    println!(
        "{}",
        output::running(&plan.mutation_command(repo).display())
    );
}

fn print_completed(strategy: crate::operations::sync::ResolvedStrategy, restore: &str) {
    println!(
        "{}",
        output::ok(&format!(
            "Synced using {}. No changes have been lost.",
            strategy.verb()
        ))
    );
    println!("  {}", output::label(&format!("Restore point: {restore}")));
    println!(
        "  {}",
        output::label("Changed your mind? `smoothee undo` restores the branch.")
    );
}

fn print_conflicted(
    strategy: crate::operations::sync::ResolvedStrategy,
    restore: &str,
    files: &[String],
) {
    println!(
        "{}",
        output::warn(&format!(
            "The {} stopped on conflicts. No changes have been lost.",
            strategy.verb()
        ))
    );
    if !files.is_empty() {
        println!();
        println!("{}", output::label("Conflicts in:"));
        for file in files {
            println!("{}", output::bullet(file));
        }
    }
    println!();
    println!("{}", output::label("Your options:"));
    println!(
        "{}",
        output::bullet("`smoothee resolve` — guided, reversible conflict resolution")
    );
    println!(
        "{}",
        output::bullet("`smoothee undo` — return to the restore point, as if nothing happened")
    );
    println!();
    println!("  {}", output::label(&format!("Restore point: {restore}")));
}

fn run_verification(repo: &Repository, config: &RepoConfig) {
    let results = verification::run_checks(repo, &config.verification);
    if results.is_empty() {
        return;
    }

    println!();
    println!("{}", output::label("Verification:"));
    for result in &results {
        let line = format!("{} ({})", result.name, result.command);
        if result.passed {
            println!("{}", output::bullet(&output::ok(&line)));
        } else {
            println!("{}", output::bullet(&output::warn(&line)));
        }
    }

    if !verification::all_passed(&results) {
        println!();
        println!(
            "  {}",
            output::label(
                "Some checks failed. Review the changes, or `smoothee undo` to revert the sync."
            )
        );
    }
}

fn plural(n: u32) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}
