//! The local operation journal.
//!
//! Every mutating Smoothee operation is recorded here so it can be undone,
//! recovered after a crash, and inspected for diagnostics. The journal is a
//! human-readable JSON-lines file stored inside the repository's `.git`
//! directory (so it travels with the repo but never pollutes the working tree
//! or gets committed).
//!
//! Records reference *restore refs* — real Git refs created before a risky
//! operation — rather than storing any private snapshot format. Recovery always
//! flows back through standard Git.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Lifecycle state of a journaled operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    /// Recorded before the mutation; not yet known to have finished.
    Started,
    /// Finished successfully.
    Completed,
    /// Attempted but failed; the restore ref (if any) is the way back.
    Failed,
    /// Reverted via `undo`.
    UndoneByUser,
}

/// The repository state captured before an operation runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeforeState {
    /// HEAD commit prior to the operation.
    pub head: String,
    /// A Git ref pinning the pre-operation state, if one was created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore_ref: Option<String>,
}

/// The repository state captured after an operation completes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AfterState {
    /// HEAD commit after the operation.
    pub head: String,
}

/// A single journal record. Shape mirrors the spec's example.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationRecord {
    /// Unique, sortable operation id (`op_<millis>_<counter>`).
    pub id: String,
    /// Operation kind, e.g. `sync_rebase`, `resolve`, `undo`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Absolute path of the repository the operation ran in.
    pub repository: String,
    /// Branch the operation acted on.
    pub branch: String,
    /// When the operation began.
    pub started_at: DateTime<Utc>,
    pub before: BeforeState,
    /// Present once the operation has an outcome.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<AfterState>,
    pub status: OperationStatus,
}

impl OperationRecord {
    /// Begin a new record in the [`OperationStatus::Started`] state.
    pub fn begin(
        kind: impl Into<String>,
        repository: impl Into<String>,
        branch: impl Into<String>,
        before: BeforeState,
    ) -> Self {
        Self {
            id: next_id(),
            kind: kind.into(),
            repository: repository.into(),
            branch: branch.into(),
            started_at: Utc::now(),
            before,
            after: None,
            status: OperationStatus::Started,
        }
    }

    /// Mark the record completed, recording the resulting HEAD.
    pub fn complete(mut self, after_head: impl Into<String>) -> Self {
        self.after = Some(AfterState {
            head: after_head.into(),
        });
        self.status = OperationStatus::Completed;
        self
    }

    /// Mark the record failed.
    pub fn fail(mut self) -> Self {
        self.status = OperationStatus::Failed;
        self
    }
}

/// Errors while reading or writing the journal.
#[derive(Debug, Error)]
pub enum JournalError {
    #[error("journal I/O error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to serialize journal record: {0}")]
    Serialize(#[source] serde_json::Error),
}

/// Append-only, JSON-lines operation journal for one repository.
#[derive(Debug, Clone)]
pub struct Journal {
    path: PathBuf,
}

impl Journal {
    /// The journal file lives at `<git_dir>/smoothee/journal.jsonl`.
    pub fn for_git_dir(git_dir: &Path) -> Self {
        Self {
            path: git_dir.join("smoothee").join("journal.jsonl"),
        }
    }

    /// The on-disk path of the journal file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append a record as one JSON line, creating parent dirs as needed.
    pub fn append(&self, record: &OperationRecord) -> Result<(), JournalError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| JournalError::Io {
                path: parent.display().to_string(),
                source,
            })?;
        }

        let line = serde_json::to_string(record).map_err(JournalError::Serialize)?;

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|source| JournalError::Io {
                path: self.path.display().to_string(),
                source,
            })?;

        writeln!(file, "{line}").map_err(|source| JournalError::Io {
            path: self.path.display().to_string(),
            source,
        })
    }

    /// Read every record in chronological (append) order. A missing journal
    /// yields an empty list rather than an error.
    pub fn read_all(&self) -> Result<Vec<OperationRecord>, JournalError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let text = std::fs::read_to_string(&self.path).map_err(|source| JournalError::Io {
            path: self.path.display().to_string(),
            source,
        })?;

        let mut records = Vec::new();
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let record: OperationRecord =
                serde_json::from_str(line).map_err(JournalError::Serialize)?;
            records.push(record);
        }
        Ok(records)
    }

    /// The most recently appended record, if any.
    pub fn latest(&self) -> Result<Option<OperationRecord>, JournalError> {
        Ok(self.read_all()?.pop())
    }
}

/// Generate a sortable, unique-enough operation id.
///
/// `op_<unix_millis>_<process-local counter>`. Millisecond time keeps ids
/// chronologically sortable; the counter disambiguates operations recorded
/// within the same millisecond by a single process.
fn next_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("op_{millis:013}_{n:04}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(kind: &str) -> OperationRecord {
        OperationRecord::begin(
            kind,
            "/repo",
            "feature/login",
            BeforeState {
                head: "abc123".into(),
                restore_ref: Some("refs/smoothee/restore/op_x".into()),
            },
        )
    }

    #[test]
    fn append_and_read_round_trip() {
        let dir = tempfile::TempDir::new().unwrap();
        let journal = Journal::for_git_dir(dir.path());

        let first = sample("sync_rebase").complete("def456");
        let second = sample("resolve");
        journal.append(&first).unwrap();
        journal.append(&second).unwrap();

        let all = journal.read_all().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].kind, "sync_rebase");
        assert_eq!(all[0].status, OperationStatus::Completed);
        assert_eq!(all[0].after.as_ref().unwrap().head, "def456");
        assert_eq!(all[1].status, OperationStatus::Started);
    }

    #[test]
    fn latest_returns_most_recent() {
        let dir = tempfile::TempDir::new().unwrap();
        let journal = Journal::for_git_dir(dir.path());
        journal.append(&sample("a")).unwrap();
        journal.append(&sample("b")).unwrap();
        assert_eq!(journal.latest().unwrap().unwrap().kind, "b");
    }

    #[test]
    fn missing_journal_reads_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let journal = Journal::for_git_dir(dir.path());
        assert!(journal.read_all().unwrap().is_empty());
        assert!(journal.latest().unwrap().is_none());
    }

    #[test]
    fn ids_are_unique_and_sortable() {
        let a = next_id();
        let b = next_id();
        assert_ne!(a, b);
        assert!(a < b, "ids should sort chronologically: {a} < {b}");
    }

    #[test]
    fn record_serializes_with_spec_field_names() {
        let record = sample("sync_rebase").complete("def456");
        let json = serde_json::to_string(&record).unwrap();
        // The spec names the field "type"; ensure serde rename holds.
        assert!(json.contains("\"type\":\"sync_rebase\""));
        assert!(json.contains("\"restore_ref\""));
    }
}
