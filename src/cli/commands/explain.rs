//! `smoothee explain` — explain repository state and risks in plain language.

use anyhow::{Context, Result};

use crate::cli::commands::status::{Recommendation, StatusAssessment};
use crate::config::RepoConfig;
use crate::git::Repository;
use crate::operations::journal::{Journal, OperationStatus};
use crate::operations::undo::find_undo_target;
use crate::ui::output;

pub fn run() -> Result<()> {
    let repo = Repository::discover_from_cwd()
        .context("this does not look like a Git repository (or git is not installed)")?;
    let config = RepoConfig::load(repo.workdir()).context("loading .smoothee.toml")?;
    let assessment = StatusAssessment::gather(&repo, &config)?;
    let journal = Journal::for_git_dir(repo.git_dir());
    let operations = journal.operations().context("reading Smoothee history")?;

    println!("{}", output::heading("What is happening"));
    println!();
    for paragraph in explain(&assessment) {
        println!("{}", output::bullet(&paragraph));
    }

    let interrupted = operations
        .iter()
        .rev()
        .find(|record| record.status == OperationStatus::Started);
    let undo_target = find_undo_target(&operations);

    println!();
    println!("{}", output::heading("Safety net"));
    if let Some(record) = interrupted {
        println!(
            "{}",
            output::bullet(&format!(
                "Smoothee has an incomplete `{}` operation in its journal. Check `smoothee history` before making unrelated changes.",
                record.kind.replace('_', " ")
            ))
        );
    }
    if let Some(target) = undo_target {
        println!(
            "{}",
            output::bullet(&format!(
                "The latest recoverable Smoothee operation is `{}`. `smoothee undo` can reverse it.",
                target.kind.replace('_', " ")
            ))
        );
    } else {
        println!(
            "{}",
            output::bullet("There is no Smoothee-managed operation available to undo right now.")
        );
    }

    println!();
    println!("{}", output::heading("Safest next step"));
    println!("{}", output::bullet(assessment.recommendation.summary()));
    if let Some(command) = assessment.recommendation.command() {
        println!("{}", output::recommend(command));
    }

    Ok(())
}

fn explain(assessment: &StatusAssessment) -> Vec<String> {
    let mut lines = Vec::new();

    match assessment.recommendation {
        Recommendation::InitialCommit => lines.push(
            "This repository has no commit history yet, so there is no stable Git snapshot to compare against or recover to. Your first commit establishes that baseline."
                .to_string(),
        ),
        Recommendation::DetachedHead => lines.push(
            "HEAD points directly at a commit instead of a branch. New commits made here can become hard to find unless you create or check out a branch first."
                .to_string(),
        ),
        Recommendation::Resolve => lines.push(
            "Git has unresolved conflict entries. Until every conflict is resolved, operations such as commit, sync, and branch switching may be blocked or unsafe."
                .to_string(),
        ),
        Recommendation::Sync => lines.push(
            "Your branch is behind another relevant ref. Continuing to build on it increases the distance between histories and can make the eventual integration more complicated."
                .to_string(),
        ),
        Recommendation::Commit => lines.push(
            "Your working tree differs from the last commit. Those edits are not protected by Git history yet, so destructive Git operations could discard them."
                .to_string(),
        ),
        Recommendation::Push => lines.push(
            "Your local branch contains commits that are not fully represented upstream. They are safe in local Git history, but they are not yet shared through the normal remote workflow."
                .to_string(),
        ),
        Recommendation::UpToDate => lines.push(
            "The working tree is clean and the branch is not currently behind or waiting on an obvious local action. Smoothee does not see a repository problem that needs intervention."
                .to_string(),
        ),
    }

    if let (Some((ahead, behind)), Some(base)) = (assessment.base_divergence, &assessment.base) {
        if ahead > 0 && behind > 0 {
            lines.push(format!(
                "Your branch and `{}` have diverged: each side has commits the other does not. A sync must integrate both histories rather than simply fast-forwarding.",
                base.name
            ));
        } else if behind > 0 {
            lines.push(format!(
                "`{}` has {behind} commit{} that your branch does not contain yet.",
                base.name,
                plural(behind)
            ));
        } else if ahead > 0 {
            lines.push(format!(
                "Your branch has {ahead} commit{} that `{}` does not contain.",
                plural(ahead),
                base.name
            ));
        }
    }

    if assessment.tree.conflicted > 0 {
        lines.push(format!(
            "There {} {} conflicted file{}. Smoothee will not treat the repository as healthy until those index conflicts are gone.",
            if assessment.tree.conflicted == 1 { "is" } else { "are" },
            assessment.tree.conflicted,
            plural(assessment.tree.conflicted)
        ));
    }

    if assessment.tree.staged > 0 {
        lines.push(format!(
            "{} staged file{} are already selected for the next commit; `smoothee commit` will preserve that selection rather than regrouping it.",
            assessment.tree.staged,
            plural(assessment.tree.staged)
        ));
    }

    if assessment.tree.upstream.is_none() && assessment.branch.is_some() {
        lines.push(
            "This branch has no configured upstream, so Git cannot directly report whether its remote counterpart is ahead or behind. Smoothee can still reason about the detected base branch."
                .to_string(),
        );
    }

    lines
}

fn plural(value: u32) -> &'static str {
    if value == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::status::{AheadBehind, WorkingTreeStatus};

    fn assessment(recommendation: Recommendation) -> StatusAssessment {
        StatusAssessment {
            repo_name: "repo".into(),
            branch: Some("feature".into()),
            base: None,
            tree: WorkingTreeStatus {
                branch: Some("feature".into()),
                ..Default::default()
            },
            base_divergence: None,
            recommendation,
        }
    }

    #[test]
    fn explains_detached_head_risk() {
        let mut value = assessment(Recommendation::DetachedHead);
        value.branch = None;
        value.tree.branch = None;
        assert!(explain(&value)[0].contains("directly at a commit"));
    }

    #[test]
    fn explains_divergence() {
        let mut value = assessment(Recommendation::Sync);
        value.base = Some(crate::git::branches::BaseBranch {
            name: "main".into(),
            source: crate::git::branches::BaseBranchSource::Conventional,
        });
        value.base_divergence = Some((2, 3));
        let lines = explain(&value);
        assert!(lines.iter().any(|line| line.contains("diverged")));
    }

    #[test]
    fn explains_upstream_ahead_state() {
        let mut value = assessment(Recommendation::Push);
        value.tree.upstream = Some("origin/feature".into());
        value.tree.ahead_behind = Some(AheadBehind {
            ahead: 2,
            behind: 0,
        });
        assert!(explain(&value)[0].contains("not yet shared"));
    }
}
