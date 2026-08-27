//! `smoothee pr` — inspect and create a GitHub pull request deliberately.

use anyhow::{Context, Result};

use crate::config::RepoConfig;
use crate::git::branches::{divergence_from_base, BaseBranch};
use crate::git::Repository;
use crate::github::GhCommand;
use crate::ui::{output, prompt};

#[derive(Debug, Clone, Default)]
pub struct PrArgs {
    pub title: Option<String>,
    pub body: Option<String>,
    pub draft: bool,
    pub push: bool,
    pub dry_run: bool,
    pub yes: bool,
}

pub fn run(args: PrArgs) -> Result<()> {
    let repo = Repository::discover_from_cwd()
        .context("this does not look like a Git repository (or git is not installed)")?;
    let branch = repo
        .current_branch()?
        .context("cannot create a pull request from detached HEAD")?;
    let config = RepoConfig::load(repo.workdir()).context("loading .smoothee.toml")?;
    let base = BaseBranch::detect(&repo, config.base_branch.as_deref())?
        .context("could not detect a base branch; configure base_branch in .smoothee.toml")?;

    if branch == base.name {
        anyhow::bail!("current branch `{branch}` is the base branch; create a feature branch first");
    }

    let (ahead, behind) = divergence_from_base(&repo, &branch, &base.name)?
        .context("current branch and base do not share history")?;
    if ahead == 0 {
        anyhow::bail!("`{branch}` has no commits that are not already in `{}`", base.name);
    }

    let remote_ref = format!("refs/remotes/origin/{branch}");
    let remote_head = repo.git("rev-parse").arg("--verify").arg(&remote_ref).run()?;
    let local_head = repo.head()?;
    let published = remote_head.success;
    let remote_matches = published && remote_head.stdout == local_head;

    print_summary(&repo, &branch, &base.name, ahead, behind)?;

    if !remote_matches && !args.push {
        println!();
        if published {
            println!("{}", output::warn("The remote branch is behind your local HEAD."));
        } else {
            println!("{}", output::warn("This branch has not been published to origin yet."));
        }
        println!(
            "  {}",
            output::label("Re-run with `--push` to publish the current branch deliberately.")
        );
        return Ok(());
    }

    let title = args
        .title
        .clone()
        .unwrap_or_else(|| suggested_title(&repo, &base.name).unwrap_or_else(|_| branch.clone()));
    let body = args
        .body
        .clone()
        .unwrap_or_else(|| suggested_body(&repo, &base.name).unwrap_or_default());

    let push_command = (!remote_matches).then(|| {
        repo.git("push")
            .arg("-u")
            .arg("origin")
            .arg(&branch)
    });
    let mut pr_command = GhCommand::new("pr")
        .arg("create")
        .arg("--base")
        .arg(&base.name)
        .arg("--head")
        .arg(&branch)
        .arg("--title")
        .arg(&title)
        .arg("--body")
        .arg(&body)
        .in_dir(repo.workdir());
    if args.draft {
        pr_command = pr_command.arg("--draft");
    }

    println!();
    println!("{}", output::label("Pull request:"));
    println!("{}", output::bullet(&format!("{} → {}", branch, base.name)));
    println!("{}", output::bullet(&format!("Title: {title}")));
    if args.draft {
        println!("{}", output::bullet("Mode: draft"));
    }

    if let Some(command) = &push_command {
        println!();
        println!("{}", output::running(&command.display()));
    }
    println!("{}", output::running(&pr_command.display()));

    if args.dry_run {
        println!();
        println!("  {}", output::label("Dry run: nothing pushed and no pull request created."));
        return Ok(());
    }

    println!();
    if !prompt::confirm("Create this pull request?", false, args.yes) {
        println!("  {}", output::label("No changes made."));
        return Ok(());
    }

    if let Some(command) = push_command {
        let result = command.run().context("publishing branch")?;
        if !result.success {
            anyhow::bail!("git push failed: {}", result.stderr);
        }
    }

    let auth = GhCommand::new("auth")
        .arg("status")
        .in_dir(repo.workdir())
        .run()
        .context("checking GitHub CLI authentication")?;
    if !auth.success {
        anyhow::bail!("GitHub CLI is not authenticated: {}", auth.stderr);
    }

    let result = pr_command.run().context("creating pull request")?;
    if !result.success {
        anyhow::bail!("gh pr create failed: {}", result.stderr);
    }

    println!();
    println!("{}", output::ok("Pull request created."));
    if !result.stdout.is_empty() {
        println!("  {}", output::label(&result.stdout));
    }
    Ok(())
}

fn print_summary(repo: &Repository, branch: &str, base: &str, ahead: u32, behind: u32) -> Result<()> {
    println!("{}", output::label("Pull request analysis:"));
    println!("{}", output::bullet(&format!("Branch: {branch}")));
    println!("{}", output::bullet(&format!("Base: {base}")));
    println!("{}", output::bullet(&format!("{ahead} branch-only commit(s)")));
    if behind > 0 {
        println!("{}", output::bullet(&output::warn(&format!("{behind} commit(s) behind {base}"))));
    }

    let range = format!("{base}..{branch}");
    let commits = repo
        .git("log")
        .arg("--oneline")
        .arg("--no-decorate")
        .arg(&range)
        .output()?;
    if !commits.is_empty() {
        println!();
        println!("{}", output::label("Commits:"));
        for line in commits.lines().take(12) {
            println!("{}", output::bullet(line));
        }
    }

    let stat = repo.git("diff").arg("--stat").arg(&range).output()?;
    if !stat.is_empty() {
        println!();
        println!("{}", output::label("Diff summary:"));
        for line in stat.lines().take(12) {
            println!("  {}", output::label(line));
        }
    }
    Ok(())
}

fn suggested_title(repo: &Repository, base: &str) -> Result<String> {
    let range = format!("{base}..HEAD");
    let subjects = repo
        .git("log")
        .arg("--format=%s")
        .arg(&range)
        .output()?;
    let mut lines = subjects.lines();
    let first = lines.next().unwrap_or("Update project");
    if lines.next().is_none() {
        Ok(first.to_string())
    } else {
        Ok(first.to_string())
    }
}

fn suggested_body(repo: &Repository, base: &str) -> Result<String> {
    let range = format!("{base}..HEAD");
    let subjects = repo
        .git("log")
        .arg("--reverse")
        .arg("--format=%s")
        .arg(&range)
        .output()?;
    let mut body = String::from("## Summary\n\n");
    for subject in subjects.lines() {
        body.push_str("- ");
        body.push_str(subject);
        body.push('\n');
    }
    Ok(body.trim_end().to_string())
}
