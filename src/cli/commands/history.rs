//! `smoothee history` — inspect Smoothee-managed operations.

use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};

use crate::git::restore::RestorePoint;
use crate::git::Repository;
use crate::operations::journal::{Journal, OperationRecord, OperationStatus};
use crate::operations::undo::find_undo_target;
use crate::ui::output;

#[derive(Debug, Clone, Copy)]
pub struct HistoryArgs {
    pub limit: usize,
}

pub fn run(args: HistoryArgs) -> Result<()> {
    if args.limit == 0 {
        anyhow::bail!("--limit must be at least 1");
    }

    let repo = Repository::discover_from_cwd()
        .context("this does not look like a Git repository (or git is not installed)")?;
    let journal = Journal::for_git_dir(repo.git_dir());
    let operations = journal.operations().context("reading Smoothee history")?;

    if operations.is_empty() {
        println!(
            "{}",
            output::label("No Smoothee-managed operations recorded for this repository.")
        );
        return Ok(());
    }

    let undo_target = find_undo_target(&operations).map(|record| record.id);
    println!("{}", output::heading("Smoothee history"));
    println!();

    for record in operations.iter().rev().take(args.limit) {
        render_record(&repo, record, undo_target.as_deref())?;
    }

    Ok(())
}

fn render_record(
    repo: &Repository,
    record: &OperationRecord,
    undo_target: Option<&str>,
) -> Result<()> {
    let time = local_time(record.started_at);
    println!(
        "{}",
        output::heading(&format!("{time}  {}", kind_label(&record.kind)))
    );
    println!(
        "{}",
        output::bullet(&format!("Status: {}", status_label(record.status)))
    );
    println!("{}", output::bullet(&format!("Branch: {}", record.branch)));
    println!(
        "{}",
        output::bullet(&format!("Before: {}", short_sha(&record.before.head)))
    );

    if let Some(after) = &record.after {
        println!(
            "{}",
            output::bullet(&format!("After: {}", short_sha(&after.head)))
        );
    }

    if let Some(target) = &record.undoes {
        println!(
            "{}",
            output::bullet(&format!("Undid: {}", short_id(target)))
        );
    }

    if let Some(restore_ref) = &record.before.restore_ref {
        let exists = RestorePoint::exists(repo, restore_ref).context("checking restore point")?;
        let state = if exists { "available" } else { "missing" };
        println!("{}", output::bullet(&format!("Restore point: {state}")));
    }

    if undo_target == Some(record.id.as_str()) {
        println!("{}", output::bullet("Undo: available (`smoothee undo`)"));
    }

    if record.status == OperationStatus::Started {
        println!(
            "{}",
            output::bullet(&output::warn(
                "Operation has no recorded outcome; this may indicate an interrupted run."
            ))
        );
    }

    println!();
    Ok(())
}

fn local_time(time: DateTime<Utc>) -> String {
    time.with_timezone(&Local)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

fn kind_label(kind: &str) -> String {
    match kind {
        "sync_rebase" => "sync (rebase)".to_string(),
        "sync_merge" => "sync (merge)".to_string(),
        "resolve_rebase" => "resolve (rebase)".to_string(),
        "resolve_merge" => "resolve (merge)".to_string(),
        "undo" => "undo".to_string(),
        other => other.replace('_', " "),
    }
}

fn status_label(status: OperationStatus) -> &'static str {
    match status {
        OperationStatus::Started => "started / incomplete",
        OperationStatus::Completed => "completed",
        OperationStatus::Failed => "failed",
        OperationStatus::UndoneByUser => "undone",
    }
}

fn short_sha(value: &str) -> &str {
    value.get(..value.len().min(8)).unwrap_or(value)
}

fn short_id(value: &str) -> &str {
    value.get(..value.len().min(24)).unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_known_operation_kinds() {
        assert_eq!(kind_label("sync_rebase"), "sync (rebase)");
        assert_eq!(kind_label("resolve_merge"), "resolve (merge)");
        assert_eq!(kind_label("future_kind"), "future kind");
    }

    #[test]
    fn short_sha_handles_short_values() {
        assert_eq!(short_sha("abcdef123456"), "abcdef12");
        assert_eq!(short_sha("abc"), "abc");
    }
}
