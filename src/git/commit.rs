//! Commit planning over Git's machine-readable status.

use std::collections::BTreeMap;

use super::command::{GitCommand, GitError};
use super::Repository;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub path: String,
    pub staged: bool,
    pub unstaged: bool,
    pub untracked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeGroup {
    pub scope: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CommitPlan {
    pub changes: Vec<Change>,
    pub groups: Vec<ChangeGroup>,
}

impl CommitPlan {
    pub fn inspect(repo: &Repository) -> Result<Self, GitError> {
        let raw = repo
            .git("status")
            .args(["--porcelain=v1", "-z", "--untracked-files=all"])
            .output()?;
        let changes = parse_porcelain(&raw);
        let groups = group_paths(
            changes
                .iter()
                .filter(|change| change.unstaged || change.untracked)
                .map(|change| change.path.as_str()),
        );
        Ok(Self { changes, groups })
    }

    pub fn staged_paths(&self) -> Vec<String> {
        self.changes
            .iter()
            .filter(|change| change.staged)
            .map(|change| change.path.clone())
            .collect()
    }

    pub fn has_staged(&self) -> bool {
        self.changes.iter().any(|change| change.staged)
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

pub fn stage_command(repo: &Repository, paths: &[String]) -> GitCommand {
    let mut command = repo.git("add").arg("--");
    for path in paths {
        command = command.arg(path);
    }
    command
}

pub fn commit_command(repo: &Repository, message: &str) -> GitCommand {
    repo.git("commit").arg("-m").arg(message)
}

pub fn suggested_message(group: Option<&ChangeGroup>, staged: &[String]) -> String {
    if let Some(group) = group {
        return if group.scope == "root" {
            "Update project files".to_string()
        } else {
            format!("Update {}", group.scope)
        };
    }

    let scopes = group_paths(staged.iter().map(String::as_str));
    match scopes.as_slice() {
        [group] if group.scope != "root" => format!("Update {}", group.scope),
        _ => "Update project changes".to_string(),
    }
}

fn parse_porcelain(raw: &str) -> Vec<Change> {
    let mut entries = raw.split('\0').filter(|entry| !entry.is_empty());
    let mut changes = Vec::new();

    while let Some(entry) = entries.next() {
        if entry.len() < 3 {
            continue;
        }
        let bytes = entry.as_bytes();
        let x = bytes[0] as char;
        let y = bytes[1] as char;
        let path = entry[3..].to_string();
        if x == 'R' || x == 'C' {
            let _ = entries.next();
        }
        changes.push(Change {
            path,
            staged: x != ' ' && x != '?',
            unstaged: y != ' ',
            untracked: x == '?' && y == '?',
        });
    }
    changes
}

fn group_paths<'a>(paths: impl Iterator<Item = &'a str>) -> Vec<ChangeGroup> {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in paths {
        let scope = path
            .split('/')
            .next()
            .filter(|_| path.contains('/'))
            .unwrap_or("root")
            .to_string();
        groups.entry(scope).or_default().push(path.to_string());
    }
    groups
        .into_iter()
        .map(|(scope, paths)| ChangeGroup { scope, paths })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_staged_unstaged_and_untracked() {
        let changes = parse_porcelain("M  src/a.rs\0 M README.md\0?? notes.txt\0");
        assert_eq!(changes.len(), 3);
        assert!(changes[0].staged);
        assert!(changes[1].unstaged);
        assert!(changes[2].untracked);
    }

    #[test]
    fn groups_unstaged_paths_by_top_level_scope() {
        let groups =
            group_paths(["src/a.rs", "src/b.rs", "docs/guide.md", "README.md"].into_iter());
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].scope, "docs");
        assert_eq!(groups[1].scope, "root");
        assert_eq!(groups[2].scope, "src");
    }

    #[test]
    fn suggestion_uses_selected_scope() {
        let group = ChangeGroup {
            scope: "src".into(),
            paths: vec!["src/a.rs".into()],
        };
        assert_eq!(suggested_message(Some(&group), &[]), "Update src");
    }
}
