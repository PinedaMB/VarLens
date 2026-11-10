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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    let old_clean = old_str.trim_end_matches(&['\r', '\n'][..]);
    let new_clean = new_str.trim_end_matches(&['\r', '\n'][..]);
    let word_diff = TextDiff::from_words(old_clean, new_clean);

    let mut l_char = 0;
    let mut r_char = 0;
    let mut line_l_spans = Vec::new();
    let mut line_r_spans = Vec::new();

    for change in word_diff.iter_all_changes() {
        let val = change.value();
        let len = val.chars().count();
        match change.tag() {
            ChangeTag::Equal => {
                l_char += len;
                r_char += len;
            }
            ChangeTag::Delete => {
                let trimmed = val.trim();
                if !trimmed.is_empty() {
                    let leading = val.chars().take_while(|c| c.is_whitespace()).count();
                    let trailing = val.chars().rev().take_while(|c| c.is_whitespace()).count();
                    line_l_spans.push(SpanHighlight {
                        line_index: left_line,
                        start_char: l_char + leading,
                        end_char: (l_char + len).saturating_sub(trailing),
                        tag: DiffTag::Delete,
                    });
                }
                l_char += len;
            }
            ChangeTag::Insert => {
                let trimmed = val.trim();
                if !trimmed.is_empty() {
                    let leading = val.chars().take_while(|c| c.is_whitespace()).count();
                    let trailing = val.chars().rev().take_while(|c| c.is_whitespace()).count();
                    line_r_spans.push(SpanHighlight {
                        line_index: right_line,
                        start_char: r_char + leading,
                        end_char: (r_char + len).saturating_sub(trailing),
                        tag: DiffTag::Insert,
                    });
                }
                r_char += len;
            }
        }
    }

    merge_adjacent_spans(&mut line_l_spans, old_clean);
    merge_adjacent_spans(&mut line_r_spans, new_clean);

    left_spans.extend(line_l_spans);
    right_spans.extend(line_r_spans);
}

fn merge_adjacent_spans(spans: &mut Vec<SpanHighlight>, text: &str) {
    if spans.len() < 2 {
        return;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut merged = Vec::with_capacity(spans.len());
    let mut current = spans[0];

    for next in spans.iter().skip(1) {
        if next.line_index == current.line_index && next.tag == current.tag {
            let gap_start = current.end_char.min(chars.len());
            let gap_end = next.start_char.min(chars.len());
            let gap_is_ws = if gap_start <= gap_end {
                chars[gap_start..gap_end].iter().all(|c| c.is_whitespace())
            } else {
                false
            };

            // Merge tokens if separated only by whitespace (e.g. "in process" or {"a": 1})
            if gap_is_ws && (gap_end.saturating_sub(gap_start)) <= 4 {
                current.end_char = next.end_char;
                continue;
            }
        }
        merged.push(current);
        current = *next;
    }
    merged.push(current);
    *spans = merged;
}

/// Computes the diff between two text inputs using default whitespace sensitivity.
#[allow(dead_code)]
pub fn compute_diff(old_text: &str, new_text: &str, patience: bool) -> DiffSummary {
    compute_diff_ext(old_text, new_text, patience, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_debug_spans() {
        let left = r#"{
  "confirmation_code": "J9MYX3",
  "status": "confirmed"
}
"#;
        let right = r#"{
  "confirmation_code": "9R99XK",
  "status": "in process"
}
"#;
        let summary = compute_diff_ext(left, right, false, false);
        assert_eq!(summary.left_highlights.len(), 2);
        assert_eq!(summary.right_highlights.len(), 2);
        assert_eq!(summary.left_spans.len(), 2);
        // "in process" is merged into a single span
        assert_eq!(summary.right_spans.len(), 2);
    }

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
