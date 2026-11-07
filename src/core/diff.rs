use similar::{Algorithm, ChangeTag, TextDiff};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DiffTag {
    Equal,
    Insert,
    Delete,
}

/// Represents an individual line diff for highlighting in a GTK buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineHighlight {
    /// 0-indexed line number in the corresponding buffer
    pub line_index: usize,
    pub tag: DiffTag,
}

/// Statistics and diff metadata
#[derive(Debug, Clone)]
pub struct DiffSummary {
    pub insertions: usize,
    pub deletions: usize,
    #[allow(dead_code)]
    pub unchanged: usize,
    pub similarity_ratio: f32,
    pub unified_diff: String,
    pub left_highlights: Vec<LineHighlight>,
    pub right_highlights: Vec<LineHighlight>,
}

/// Computes the diff between two text inputs using the Myers or Patience algorithm.
pub fn compute_diff(old_text: &str, new_text: &str, patience: bool) -> DiffSummary {
    let algorithm = if patience {
        Algorithm::Patience
    } else {
        Algorithm::Myers
    };

    let diff = TextDiff::configure()
        .algorithm(algorithm)
        .diff_lines(old_text, new_text);

    let mut insertions = 0;
    let mut deletions = 0;
    let mut unchanged = 0;

    let mut left_highlights = Vec::new();
    let mut right_highlights = Vec::new();

    let mut old_line_idx = 0;
    let mut new_line_idx = 0;

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                unchanged += 1;
                old_line_idx += 1;
                new_line_idx += 1;
            }
            ChangeTag::Delete => {
                deletions += 1;
                left_highlights.push(LineHighlight {
                    line_index: old_line_idx,
                    tag: DiffTag::Delete,
                });
                old_line_idx += 1;
            }
            ChangeTag::Insert => {
                insertions += 1;
                right_highlights.push(LineHighlight {
                    line_index: new_line_idx,
                    tag: DiffTag::Insert,
                });
                new_line_idx += 1;
            }
        }
    }

    let total = insertions + deletions + (unchanged * 2);
    let similarity_ratio = if total == 0 {
        1.0
    } else {
        (unchanged * 2) as f32 / total as f32
    };

    let unified_diff = diff
        .unified_diff()
        .context_radius(3)
        .header("Original", "Modificado")
        .to_string();

    DiffSummary {
        insertions,
        deletions,
        unchanged,
        similarity_ratio,
        unified_diff,
        left_highlights,
        right_highlights,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_computation() {
        let old = "line 1\nline 2\nline 3\n";
        let new = "line 1\nline 2 modified\nline 3\nline 4\n";

        let summary = compute_diff(old, new, false);
        assert_eq!(summary.deletions, 1);
        assert_eq!(summary.insertions, 2);
        assert_eq!(summary.unchanged, 2);
        assert_eq!(summary.left_highlights.len(), 1);
        assert_eq!(summary.left_highlights[0].line_index, 1);
        assert_eq!(summary.right_highlights.len(), 2);
    }
}
