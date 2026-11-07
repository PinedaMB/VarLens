use similar::{Algorithm, ChangeTag, DiffOp, TextDiff};

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

/// Represents a character/word-level inline highlight inside a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanHighlight {
    pub line_index: usize,
    /// Character offset from start of line
    pub start_char: usize,
    /// Character offset end in line
    pub end_char: usize,
    pub tag: DiffTag,
}

/// Statistics and diff metadata
#[derive(Debug, Clone)]
pub struct DiffSummary {
    pub insertions: usize,
    pub deletions: usize,
    #[allow(dead_code)]
    pub unchanged: usize,
    /// Number of contiguous change blocks/hunks
    pub hunk_count: usize,
    /// (left_line, right_line) for each hunk for Prev/Next navigation
    pub hunk_positions: Vec<(usize, usize)>,
    #[allow(dead_code)]
    pub similarity_ratio: f32,
    #[allow(dead_code)]
    pub unified_diff: String,
    pub left_highlights: Vec<LineHighlight>,
    pub right_highlights: Vec<LineHighlight>,
    pub left_spans: Vec<SpanHighlight>,
    pub right_spans: Vec<SpanHighlight>,
}

/// Computes the diff between two text inputs using Myers or Patience, with optional whitespace ignoring.
pub fn compute_diff_ext(
    old_text: &str,
    new_text: &str,
    patience: bool,
    ignore_whitespace: bool,
) -> DiffSummary {
    let algorithm = if patience {
        Algorithm::Patience
    } else {
        Algorithm::Myers
    };

    let old_lines_raw: Vec<&str> = old_text.lines().collect();
    let new_lines_raw: Vec<&str> = new_text.lines().collect();

    let old_norm: Vec<String>;
    let new_norm: Vec<String>;

    let old_keys: Vec<&str> = if ignore_whitespace {
        old_norm = old_lines_raw.iter().map(|l| l.trim().to_string()).collect();
        old_norm.iter().map(|s| s.as_str()).collect()
    } else {
        old_lines_raw.clone()
    };

    let new_keys: Vec<&str> = if ignore_whitespace {
        new_norm = new_lines_raw.iter().map(|l| l.trim().to_string()).collect();
        new_norm.iter().map(|s| s.as_str()).collect()
    } else {
        new_lines_raw.clone()
    };

    let ops = similar::capture_diff_slices(algorithm, &old_keys, &new_keys);

    let mut insertions = 0;
    let deletions_mut = &mut 0;
    let mut unchanged = 0;
    let mut hunk_count = 0;
    let mut hunk_positions = Vec::new();

    let mut left_highlights = Vec::new();
    let mut right_highlights = Vec::new();
    let mut left_spans = Vec::new();
    let mut right_spans = Vec::new();

    for op in &ops {
        match *op {
            DiffOp::Equal { len, .. } => {
                unchanged += len;
            }
            DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => {
                hunk_count += 1;
                hunk_positions.push((old_index, new_index));
                *deletions_mut += old_len;
                for i in old_index..(old_index + old_len) {
                    left_highlights.push(LineHighlight {
                        line_index: i,
                        tag: DiffTag::Delete,
                    });
                }
            }
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => {
                hunk_count += 1;
                hunk_positions.push((old_index, new_index));
                insertions += new_len;
                for j in new_index..(new_index + new_len) {
                    right_highlights.push(LineHighlight {
                        line_index: j,
                        tag: DiffTag::Insert,
                    });
                }
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                hunk_count += 1;
                hunk_positions.push((old_index, new_index));
                *deletions_mut += old_len;
                insertions += new_len;

                for i in old_index..(old_index + old_len) {
                    left_highlights.push(LineHighlight {
                        line_index: i,
                        tag: DiffTag::Delete,
                    });
                }
                for j in new_index..(new_index + new_len) {
                    right_highlights.push(LineHighlight {
                        line_index: j,
                        tag: DiffTag::Insert,
                    });
                }

                // Compute inline word-level diffs for paired lines in the Replace block
                let pair_count = old_len.min(new_len);
                for k in 0..pair_count {
                    let l_idx = old_index + k;
                    let r_idx = new_index + k;
                    if let (Some(&l_str), Some(&r_str)) =
                        (old_lines_raw.get(l_idx), new_lines_raw.get(r_idx))
                    {
                        compute_inline_spans(
                            l_idx,
                            r_idx,
                            l_str,
                            r_str,
                            &mut left_spans,
                            &mut right_spans,
                        );
                    }
                }
            }
        }
    }

    let deletions = *deletions_mut;
    let total = insertions + deletions + (unchanged * 2);
    let similarity_ratio = if total == 0 {
        1.0
    } else {
        (unchanged * 2) as f32 / total as f32
    };

    let text_diff = TextDiff::configure()
        .algorithm(algorithm)
        .diff_lines(old_text, new_text);
    let unified_diff = text_diff
        .unified_diff()
        .context_radius(3)
        .header("Original", "Modificado")
        .to_string();

    DiffSummary {
        insertions,
        deletions,
        unchanged,
        hunk_count,
        hunk_positions,
        similarity_ratio,
        unified_diff,
        left_highlights,
        right_highlights,
        left_spans,
        right_spans,
    }
}

fn compute_inline_spans(
    left_line: usize,
    right_line: usize,
    old_str: &str,
    new_str: &str,
    left_spans: &mut Vec<SpanHighlight>,
    right_spans: &mut Vec<SpanHighlight>,
) {
    let word_diff = TextDiff::from_words(old_str, new_str);
    let mut l_char = 0;
    let mut r_char = 0;

    for change in word_diff.iter_all_changes() {
        let len = change.value().chars().count();
        match change.tag() {
            ChangeTag::Equal => {
                l_char += len;
                r_char += len;
            }
            ChangeTag::Delete => {
                if !change.value().trim().is_empty() {
                    left_spans.push(SpanHighlight {
                        line_index: left_line,
                        start_char: l_char,
                        end_char: l_char + len,
                        tag: DiffTag::Delete,
                    });
                }
                l_char += len;
            }
            ChangeTag::Insert => {
                if !change.value().trim().is_empty() {
                    right_spans.push(SpanHighlight {
                        line_index: right_line,
                        start_char: r_char,
                        end_char: r_char + len,
                        tag: DiffTag::Insert,
                    });
                }
                r_char += len;
            }
        }
    }
}

/// Computes the diff between two text inputs using default whitespace sensitivity.
pub fn compute_diff(old_text: &str, new_text: &str, patience: bool) -> DiffSummary {
    compute_diff_ext(old_text, new_text, patience, false)
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
        assert_eq!(summary.hunk_count, 2);
        assert_eq!(summary.left_highlights.len(), 1);
        assert_eq!(summary.left_highlights[0].line_index, 1);
        assert_eq!(summary.right_highlights.len(), 2);
        assert!(!summary.right_spans.is_empty());
    }

    #[test]
    fn test_ignore_whitespace() {
        let old = "  {\"a\": 1}\n";
        let new = "{\"a\": 1}   \n";
        let summary = compute_diff_ext(old, new, false, true);
        assert_eq!(summary.deletions, 0);
        assert_eq!(summary.insertions, 0);
        assert_eq!(summary.hunk_count, 0);
    }
}
