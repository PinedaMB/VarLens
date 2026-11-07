/// Represents a collapsable/foldable code block in a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldRegion {
    /// 0-indexed starting line where opening bracket '{' or '[' is located
    pub start_line: usize,
    /// 0-indexed ending line where closing bracket '}' or ']' is located
    pub end_line: usize,
    /// Opening delimiter ('{' or '[')
    pub open_char: char,
}

/// Scans text for matching balanced brackets ('{'-'}' and '['-']') while ignoring
/// bracket characters inside strings or line comments.
/// Returns all regions spanning 2 or more lines.
pub fn find_fold_regions(text: &str) -> Vec<FoldRegion> {
    let mut regions = Vec::new();
    let mut stack: Vec<(usize, char)> = Vec::new(); // (line_index, open_char)

    let mut in_string = false;
    let mut string_char = '"';
    let mut escape = false;

    let mut current_line = 0;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if escape {
            escape = false;
            continue;
        }

        if in_string {
            if ch == '\\' {
                escape = true;
            } else if ch == string_char {
                in_string = false;
            }
            if ch == '\n' {
                current_line += 1;
            }
            continue;
        }

        // Check for start of string
        if ch == '"' || ch == '\'' {
            in_string = true;
            string_char = ch;
            continue;
        }

        // Check for line comments (e.g. // or #)
        if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            // skip until newline
            while let Some(&c) = chars.peek() {
                if c == '\n' {
                    break;
                }
                chars.next();
            }
            continue;
        }

        if ch == '#' {
            while let Some(&c) = chars.peek() {
                if c == '\n' {
                    break;
                }
                chars.next();
            }
            continue;
        }

        match ch {
            '{' | '[' => {
                stack.push((current_line, ch));
            }
            '}' => {
                if let Some((start_l, open_c)) = stack.pop() {
                    if open_c == '{' && current_line > start_l {
                        regions.push(FoldRegion {
                            start_line: start_l,
                            end_line: current_line,
                            open_char: open_c,
                        });
                    }
                }
            }
            ']' => {
                if let Some((start_l, open_c)) = stack.pop() {
                    if open_c == '[' && current_line > start_l {
                        regions.push(FoldRegion {
                            start_line: start_l,
                            end_line: current_line,
                            open_char: open_c,
                        });
                    }
                }
            }
            '\n' => {
                current_line += 1;
            }
            _ => {}
        }
    }

    // Sort regions by start line
    regions.sort_by_key(|r| r.start_line);
    regions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_fold_regions_json() {
        let json = r#"{
  "name": "VarLens",
  "items": [
    1,
    2
  ]
}"#;
        let regions = find_fold_regions(json);
        assert_eq!(regions.len(), 2);
        // Outer object
        assert_eq!(regions[0].start_line, 0);
        assert_eq!(regions[0].end_line, 6);
        // Inner array
        assert_eq!(regions[1].start_line, 2);
        assert_eq!(regions[1].end_line, 5);
    }

    #[test]
    fn test_brackets_in_string_ignored() {
        let json = r#"{
  "text": "{ not a block }"
}"#;
        let regions = find_fold_regions(json);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].start_line, 0);
        assert_eq!(regions[0].end_line, 2);
    }
}
