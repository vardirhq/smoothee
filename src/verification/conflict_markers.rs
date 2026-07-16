//! Detecting leftover conflict markers in a resolved file.
//!
//! Part of the "validate before offering to the user" safety layer: after a
//! manual edit, Smoothee refuses to stage a file that still contains Git's
//! conflict markers, so a half-resolved conflict can never be committed by
//! accident.
//!
//! Detection keys on the bracket markers (`<<<<<<<`, `|||||||`, `>>>>>>>`),
//! which never occur in legitimate source. The bare `=======` separator is
//! deliberately *not* treated as a marker on its own: it collides with common
//! content (Markdown headings, ASCII rules), and a resolution that removes the
//! bracket markers but leaves a lone separator is not a realistic outcome.

/// Whether `content` still contains Git conflict markers.
pub fn has_conflict_markers(content: &str) -> bool {
    content.lines().any(is_marker_line)
}

/// Whether a single line is one of Git's bracket conflict markers.
fn is_marker_line(line: &str) -> bool {
    ['<', '|', '>']
        .into_iter()
        .any(|ch| line.chars().take_while(|&c| c == ch).count() >= 7)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_start_and_end_markers() {
        assert!(has_conflict_markers("a\n<<<<<<< HEAD\nb\n"));
        assert!(has_conflict_markers("a\n>>>>>>> feature\nb\n"));
        assert!(has_conflict_markers("a\n||||||| base\nb\n"));
    }

    #[test]
    fn clean_content_has_no_markers() {
        assert!(!has_conflict_markers("just some\nresolved code\n"));
    }

    #[test]
    fn bare_separator_is_not_flagged() {
        // A Markdown heading underline must not be mistaken for a conflict.
        assert!(!has_conflict_markers("Title\n=======\nbody\n"));
    }

    #[test]
    fn shorter_runs_are_not_markers() {
        // Fewer than seven bracket characters is not a Git marker.
        assert!(!has_conflict_markers("<<<< not a marker\n"));
    }
}
