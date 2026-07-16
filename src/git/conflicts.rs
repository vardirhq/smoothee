//! Parsing conflicted files into structured, labelled hunks.
//!
//! When a merge or rebase stops, Git rewrites the conflicting files with marker
//! blocks (`<<<<<<<`, optional `|||||||` base, `=======`, `>>>>>>>`). This module
//! turns those blocks into structured [`ConflictHunk`]s so the `resolve` command
//! can *explain* a conflict rather than just dumping raw markers.
//!
//! It also resolves Smoothee's clarity promise around the most confusing part of
//! conflict resolution: "ours" and "theirs". Git's `<<<<<<<` side is always
//! "ours" and `>>>>>>>` is "theirs", but which one holds *your* work flips
//! between merge and rebase. [`ConflictContext`] captures that once, so every
//! label and every `git checkout --ours/--theirs` maps to the user's intent
//! instead of to Git's mechanical vocabulary.

use super::command::GitError;
use super::repository::Repository;

/// The two sides Git can pick from for a conflicted path, named the way Git
/// names them (`--ours` / `--theirs`) so the mapping to a flag is unambiguous.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Git's `<<<<<<<` side (`git checkout --ours`).
    Ours,
    /// Git's `>>>>>>>` side (`git checkout --theirs`).
    Theirs,
}

impl Side {
    /// The `git checkout` flag that selects this side.
    pub fn flag(self) -> &'static str {
        match self {
            Side::Ours => "--ours",
            Side::Theirs => "--theirs",
        }
    }
}

/// Which operation produced the conflicts. This decides how the mechanical
/// "ours"/"theirs" sides map to "your work" vs "the incoming work".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictContext {
    /// A `git merge` is in progress: `HEAD` (ours) is your branch.
    Merge,
    /// A `git rebase` is in progress: `HEAD` (ours) is the base being replayed
    /// onto, and *your* commit is the incoming (theirs) side.
    Rebase,
    /// Conflicts exist without a recognised merge/rebase marker (e.g. a
    /// cherry-pick). Labelled like a merge, which is the common intuition.
    Other,
}

impl ConflictContext {
    /// Detect the context from the repository's in-progress state.
    pub fn detect(repo: &Repository) -> Self {
        if repo.rebase_in_progress() {
            ConflictContext::Rebase
        } else if repo.merge_in_progress() {
            ConflictContext::Merge
        } else {
            ConflictContext::Other
        }
    }

    /// The side that holds *your* changes.
    pub fn mine(self) -> Side {
        match self {
            // A rebase replays your commits on top of the base, so your work is
            // the "theirs" side being applied.
            ConflictContext::Rebase => Side::Theirs,
            _ => Side::Ours,
        }
    }

    /// The side that holds the *incoming* changes you are syncing with.
    pub fn incoming(self) -> Side {
        match self {
            ConflictContext::Rebase => Side::Ours,
            _ => Side::Theirs,
        }
    }

    /// A short human label for [`ConflictContext::incoming`], for menus and prose.
    pub fn incoming_label(self) -> &'static str {
        match self {
            ConflictContext::Rebase => "the base branch",
            _ => "the incoming branch",
        }
    }

    /// The verb for the operation being finished ("merge"/"rebase").
    pub fn verb(self) -> &'static str {
        match self {
            ConflictContext::Rebase => "rebase",
            _ => "merge",
        }
    }

    /// The journal `type` recorded when resolving in this context.
    pub fn journal_kind(self) -> &'static str {
        match self {
            ConflictContext::Rebase => "resolve_rebase",
            _ => "resolve_merge",
        }
    }
}

/// One `<<<<<<< / ======= / >>>>>>>` block from a conflicted file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictHunk {
    /// Lines on Git's "ours" (`<<<<<<<`) side.
    pub ours: Vec<String>,
    /// Lines from the common ancestor, present only for diff3-style conflicts.
    pub base: Option<Vec<String>>,
    /// Lines on Git's "theirs" (`>>>>>>>`) side.
    pub theirs: Vec<String>,
}

/// A conflicted file and its parsed hunks.
#[derive(Debug, Clone)]
pub struct ConflictFile {
    /// Repository-relative path.
    pub path: String,
    /// The parsed conflict blocks, in order.
    pub hunks: Vec<ConflictHunk>,
    /// Whether the file could be read as UTF-8 text. A binary or unreadable
    /// conflict (e.g. delete/modify) yields no hunks and must be resolved by
    /// picking a whole side rather than editing.
    pub readable: bool,
}

impl ConflictFile {
    /// Read `path` (relative to the repository root) and parse its conflicts.
    ///
    /// A read failure (binary content, delete/modify conflict) is not an error:
    /// it returns a file with `readable = false` and no hunks, which the caller
    /// still resolves by choosing a side.
    pub fn read(repo: &Repository, path: &str) -> Self {
        match std::fs::read_to_string(repo.workdir().join(path)) {
            Ok(content) => Self {
                path: path.to_string(),
                hunks: parse(&content),
                readable: true,
            },
            Err(_) => Self {
                path: path.to_string(),
                hunks: Vec::new(),
                readable: false,
            },
        }
    }
}

/// Whether `line` begins with a run of at least seven `ch` characters — the
/// shape of a Git conflict marker.
fn is_marker(line: &str, ch: char) -> bool {
    line.chars().take_while(|&c| c == ch).count() >= 7
}

/// Parse the conflict blocks out of a file's contents. Context lines outside any
/// conflict are ignored — only the hunks matter for explanation and resolution.
pub fn parse(content: &str) -> Vec<ConflictHunk> {
    enum State {
        Outside,
        Ours,
        Base,
        Theirs,
    }

    let mut state = State::Outside;
    let mut hunks = Vec::new();
    let mut ours: Vec<String> = Vec::new();
    let mut base: Vec<String> = Vec::new();
    let mut theirs: Vec<String> = Vec::new();
    let mut have_base = false;

    for line in content.lines() {
        match state {
            State::Outside => {
                if is_marker(line, '<') {
                    state = State::Ours;
                    ours.clear();
                    base.clear();
                    theirs.clear();
                    have_base = false;
                }
            }
            State::Ours => {
                if is_marker(line, '|') {
                    state = State::Base;
                    have_base = true;
                } else if is_marker(line, '=') {
                    state = State::Theirs;
                } else {
                    ours.push(line.to_string());
                }
            }
            State::Base => {
                if is_marker(line, '=') {
                    state = State::Theirs;
                } else {
                    base.push(line.to_string());
                }
            }
            State::Theirs => {
                if is_marker(line, '>') {
                    hunks.push(ConflictHunk {
                        ours: std::mem::take(&mut ours),
                        base: if have_base {
                            Some(std::mem::take(&mut base))
                        } else {
                            None
                        },
                        theirs: std::mem::take(&mut theirs),
                    });
                    base.clear();
                    state = State::Outside;
                } else {
                    theirs.push(line.to_string());
                }
            }
        }
    }

    hunks
}

/// Resolve a conflicted `path` by keeping one whole side, then stage it.
///
/// `git checkout --ours/--theirs -- <path>` replaces the working-tree file with
/// the chosen side; `git add` marks the path resolved. Both flow through the
/// deterministic command layer so the operation stays inspectable.
pub fn take_side(repo: &Repository, path: &str, side: Side) -> Result<(), GitError> {
    repo.git("checkout")
        .arg(side.flag())
        .arg("--")
        .arg(path)
        .output()?;
    stage(repo, path)
}

/// Stage a (now-resolved) path with `git add`.
pub fn stage(repo: &Repository, path: &str) -> Result<(), GitError> {
    repo.git("add").arg("--").arg(path).output()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repository::tests::{init_repo, run};

    #[test]
    fn parses_two_way_conflict() {
        let content = "\
context before
<<<<<<< HEAD
our line one
our line two
=======
their line
>>>>>>> feature
context after
";
        let hunks = parse(content);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].ours, vec!["our line one", "our line two"]);
        assert_eq!(hunks[0].theirs, vec!["their line"]);
        assert!(hunks[0].base.is_none());
    }

    #[test]
    fn parses_diff3_conflict_with_base() {
        let content = "\
<<<<<<< HEAD
ours
||||||| merged common ancestors
base
=======
theirs
>>>>>>> other
";
        let hunks = parse(content);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].ours, vec!["ours"]);
        assert_eq!(hunks[0].base.as_deref(), Some(&["base".to_string()][..]));
        assert_eq!(hunks[0].theirs, vec!["theirs"]);
    }

    #[test]
    fn parses_multiple_hunks() {
        let content = "\
<<<<<<< HEAD
a
=======
b
>>>>>>> x
middle
<<<<<<< HEAD
c
=======
d
>>>>>>> x
";
        let hunks = parse(content);
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].ours, vec!["a"]);
        assert_eq!(hunks[1].theirs, vec!["d"]);
    }

    #[test]
    fn no_markers_yields_no_hunks() {
        assert!(parse("just\nregular\nlines\n").is_empty());
    }

    #[test]
    fn context_maps_sides_to_intent() {
        // Merge: your work is the "ours" side.
        assert_eq!(ConflictContext::Merge.mine(), Side::Ours);
        assert_eq!(ConflictContext::Merge.incoming(), Side::Theirs);
        // Rebase: your work is replayed as the "theirs" side.
        assert_eq!(ConflictContext::Rebase.mine(), Side::Theirs);
        assert_eq!(ConflictContext::Rebase.incoming(), Side::Ours);
        assert_eq!(ConflictContext::Merge.verb(), "merge");
        assert_eq!(ConflictContext::Rebase.journal_kind(), "resolve_rebase");
    }

    #[test]
    fn side_flags_are_git_flags() {
        assert_eq!(Side::Ours.flag(), "--ours");
        assert_eq!(Side::Theirs.flag(), "--theirs");
    }

    #[test]
    fn take_side_keeps_chosen_content_and_stages() {
        // Build a real merge conflict on f.txt, then keep our side.
        let (_g, path) = init_repo();
        std::fs::write(path.join("f.txt"), "base\n").unwrap();
        run(&path, &["add", "."]);
        run(&path, &["commit", "-q", "-m", "base"]);
        run(&path, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(path.join("f.txt"), "theirs\n").unwrap();
        run(&path, &["commit", "-q", "-am", "feature"]);
        run(&path, &["checkout", "-q", "main"]);
        std::fs::write(path.join("f.txt"), "ours\n").unwrap();
        run(&path, &["commit", "-q", "-am", "main"]);
        let out = std::process::Command::new("git")
            .args(["merge", "--no-edit", "feature"])
            .current_dir(&path)
            .output()
            .unwrap();
        assert!(!out.status.success());

        let repo = Repository::discover(&path).unwrap();
        assert_eq!(repo.conflicted_files().unwrap(), vec!["f.txt".to_string()]);

        take_side(&repo, "f.txt", Side::Ours).unwrap();
        assert_eq!(
            std::fs::read_to_string(path.join("f.txt")).unwrap(),
            "ours\n"
        );
        assert!(
            repo.conflicted_files().unwrap().is_empty(),
            "taking a side stages the file, clearing the conflict"
        );
    }

    #[test]
    fn read_reports_unreadable_for_missing_file() {
        let (_g, path) = init_repo();
        let repo = Repository::discover(&path).unwrap();
        let file = ConflictFile::read(&repo, "does-not-exist");
        assert!(!file.readable);
        assert!(file.hunks.is_empty());
    }
}
