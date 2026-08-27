//! Parsing of `git status --porcelain=v2 --branch` into structured state.
//!
//! Porcelain v2 is a stable, machine-readable format designed exactly for this
//! purpose, so Smoothee parses it rather than the human-facing `git status`.
//! See `git-status(1)` "Porcelain Format Version 2" for the grammar.

use super::command::GitError;
use super::repository::Repository;

/// The upstream tracking relationship for the current branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AheadBehind {
    /// Commits the local branch has that the upstream does not.
    pub ahead: u32,
    /// Commits the upstream has that the local branch does not.
    pub behind: u32,
}

/// A structured snapshot of `git status`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkingTreeStatus {
    /// Current branch name, or `None` when in detached HEAD.
    pub branch: Option<String>,
    /// Configured upstream branch (e.g. `origin/feature/login`), if any.
    pub upstream: Option<String>,
    /// Ahead/behind counts versus the upstream, if an upstream is set.
    pub ahead_behind: Option<AheadBehind>,
    /// Files with staged (index) changes.
    pub staged: u32,
    /// Tracked files with unstaged worktree changes.
    pub modified: u32,
    /// Files with unresolved merge conflicts.
    pub conflicted: u32,
    /// Untracked files.
    pub untracked: u32,
    /// Whether the repository has no commits yet (unborn HEAD).
    pub is_initial: bool,
}

impl WorkingTreeStatus {
    /// Query the repository for its current status.
    pub fn query(repo: &Repository) -> Result<Self, GitError> {
        let raw = repo
            .git("status")
            .arg("--porcelain=v2")
            .arg("--branch")
            .arg("--untracked-files=normal")
            .output()?;
        Ok(Self::parse(&raw))
    }

    /// Whether there are no changes of any kind in the working tree or index.
    pub fn is_clean(&self) -> bool {
        self.staged == 0 && self.modified == 0 && self.conflicted == 0 && self.untracked == 0
    }

    /// Parse porcelain v2 text. Kept separate from [`query`](Self::query) so it
    /// can be unit-tested against fixture strings without a live repository.
    pub fn parse(raw: &str) -> Self {
        let mut status = WorkingTreeStatus::default();

        for line in raw.lines() {
            if let Some(header) = line.strip_prefix("# ") {
                status.parse_header(header);
            } else if let Some(rest) = line.strip_prefix("1 ") {
                // Ordinary changed entry: "<XY> <sub> ... <path>".
                status.tally_xy(rest);
            } else if let Some(rest) = line.strip_prefix("2 ") {
                // Renamed/copied entry: same leading XY field.
                status.tally_xy(rest);
            } else if line.starts_with("u ") {
                // Unmerged entry — always a conflict.
                status.conflicted += 1;
            } else if line.starts_with("? ") {
                status.untracked += 1;
            }
            // "! " ignored entries are intentionally not surfaced.
        }

        status
    }

    fn parse_header(&mut self, header: &str) {
        let mut fields = header.split_whitespace();
        match fields.next() {
            Some("branch.head") => {
                if let Some(name) = fields.next() {
                    if name == "(detached)" {
                        self.branch = None;
                    } else {
                        self.branch = Some(name.to_string());
                    }
                }
            }
            Some("branch.upstream") => {
                self.upstream = fields.next().map(str::to_string);
            }
            Some("branch.ab") => {
                // Format: "+<ahead> -<behind>".
                let ahead = fields
                    .next()
                    .and_then(|f| f.strip_prefix('+'))
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                let behind = fields
                    .next()
                    .and_then(|f| f.strip_prefix('-'))
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                self.ahead_behind = Some(AheadBehind { ahead, behind });
            }
            Some("branch.oid") if fields.next() == Some("(initial)") => {
                self.is_initial = true;
            }
            _ => {}
        }
    }

    /// Given the remainder of a "1"/"2" line, whose first field is the two-
    /// character `<XY>` staged/worktree status code, tally staged vs modified.
    fn tally_xy(&mut self, rest: &str) {
        let Some(xy) = rest.split_whitespace().next() else {
            return;
        };
        let mut chars = xy.chars();
        let index = chars.next().unwrap_or('.');
        let worktree = chars.next().unwrap_or('.');

        if index != '.' {
            self.staged += 1;
        }
        if worktree != '.' {
            self.modified += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branch_and_ahead_behind() {
        let raw = "\
# branch.oid abc123
# branch.head feature/login
# branch.upstream origin/feature/login
# branch.ab +4 -6
";
        let s = WorkingTreeStatus::parse(raw);
        assert_eq!(s.branch.as_deref(), Some("feature/login"));
        assert_eq!(s.upstream.as_deref(), Some("origin/feature/login"));
        assert_eq!(
            s.ahead_behind,
            Some(AheadBehind {
                ahead: 4,
                behind: 6
            })
        );
        assert!(s.is_clean());
    }

    #[test]
    fn tallies_staged_modified_untracked() {
        let raw = "\
# branch.head main
1 M. N... 100644 100644 100644 aaa bbb staged_only.rs
1 .M N... 100644 100644 100644 ccc ddd worktree_only.rs
1 MM N... 100644 100644 100644 eee fff both.rs
? new_file.rs
";
        let s = WorkingTreeStatus::parse(raw);
        assert_eq!(s.staged, 2);
        assert_eq!(s.modified, 2);
        assert_eq!(s.untracked, 1);
        assert_eq!(s.conflicted, 0);
        assert!(!s.is_clean());
    }

    #[test]
    fn counts_unmerged_as_conflicts() {
        let raw = "\
# branch.head main
u UU N... 100644 100644 100644 100644 aaa bbb ccc conflicted.rs
";
        let s = WorkingTreeStatus::parse(raw);
        assert_eq!(s.conflicted, 1);
    }

    #[test]
    fn detects_detached_head_and_initial() {
        let detached = WorkingTreeStatus::parse("# branch.head (detached)\n");
        assert_eq!(detached.branch, None);

        let initial = WorkingTreeStatus::parse("# branch.oid (initial)\n# branch.head main\n");
        assert!(initial.is_initial);
    }

    #[test]
    fn handles_renamed_entries() {
        let raw = "\
# branch.head main
2 R. N... 100644 100644 100644 aaa bbb R100 new.rs\told.rs
";
        let s = WorkingTreeStatus::parse(raw);
        assert_eq!(s.staged, 1);
        assert_eq!(s.modified, 0);
    }
}
