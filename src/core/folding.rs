/// Represents a collapsable/foldable code block in a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldRegion {
    /// 0-indexed starting line where opening bracket or tag is located
    pub start_line: usize,
    /// 0-indexed ending line where closing bracket or tag is located
    pub end_line: usize,
    /// Opening delimiter ('{', '[', or '<')
    pub open_char: char,
}

/// Scans text for matching balanced brackets ('{'-'}' and '['-']') and XML tags ('<tag>'-'</tag>')
/// while ignoring delimiters inside strings or comments.
/// Returns all regions spanning 2 or more lines.
pub fn find_fold_regions(text: &str) -> Vec<FoldRegion> {
    let mut regions = Vec::new();
    let mut bracket_stack: Vec<(usize, char)> = Vec::new(); // (line_index, open_char)
    let mut xml_stack: Vec<(usize, String)> = Vec::new();   // (line_index, tag_name)

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

        // Check for line comments (// or #)
        if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
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

        // Check for XML comments <!-- ... -->
        if ch == '<' && chars.peek() == Some(&'!') {
            chars.next(); // consume '!'
            if chars.peek() == Some(&'-') {
                chars.next(); // consume first '-'
                if chars.peek() == Some(&'-') {
                    chars.next(); // consume second '-'
                    while let Some(c) = chars.next() {
                        if c == '\n' { current_line += 1; }
                        if c == '-' && chars.peek() == Some(&'-') {
                            chars.next();
                            if chars.peek() == Some(&'>') {
                                chars.next();
                                break;
                            }
                        }
                    }
                    continue;
                }
            } else if chars.peek() == Some(&'[') {
                // CDATA <![CDATA[ ... ]]>
                while let Some(c) = chars.next() {
                    if c == '\n' { current_line += 1; }
                    if c == ']' && chars.peek() == Some(&']') {
                        chars.next();
                        if chars.peek() == Some(&'>') {
                            chars.next();
                            break;
                        }
                    }
                }
                continue;
            }
        }

        // Check for XML Processing Instruction <? ... ?>
        if ch == '<' && chars.peek() == Some(&'?') {
            chars.next();
            while let Some(c) = chars.next() {
                if c == '\n' { current_line += 1; }
                if c == '?' && chars.peek() == Some(&'>') {
                    chars.next();
                    break;
                }
            }
            continue;
        }

        // Check for XML closing tag </tag>
        if ch == '<' && chars.peek() == Some(&'/') {
            chars.next(); // consume '/'
            let mut tag_name = String::new();
            for c in chars.by_ref() {
                if c == '\n' { current_line += 1; }
                if c == '>' { break; }
                if !c.is_whitespace() { tag_name.push(c); }
            }
            if let Some((start_l, open_tag)) = xml_stack.pop() {
                if open_tag == tag_name && current_line > start_l {
                    regions.push(FoldRegion {
                        start_line: start_l,
                        end_line: current_line,
                        open_char: '<',
                    });
                }
            }
            continue;
        }

        // Check for XML opening tag <tag ... > or <tag ... />
        if ch == '<' && chars.peek().is_some_and(|c| c.is_alphabetic() || *c == '_' || *c == ':') {
            let mut tag_name = String::new();
            let mut is_self_closing = false;
            let mut in_attr_quote: Option<char> = None;

            // Extract tag name
            while let Some(&c) = chars.peek() {
                if c.is_alphanumeric() || c == '_' || c == ':' || c == '-' {
                    tag_name.push(c);
                    chars.next();
                } else {
                    break;
                }
            }

            // Read the rest of the tag until '>'
            let mut last_char = ' ';
            for c in chars.by_ref() {
                if c == '\n' { current_line += 1; }
                if let Some(q) = in_attr_quote {
                    if c == q { in_attr_quote = None; }
                } else if c == '"' || c == '\'' {
                    in_attr_quote = Some(c);
                } else if c == '>' {
                    if last_char == '/' {
                        is_self_closing = true;
                    }
                    break;
                }
                if !c.is_whitespace() {
                    last_char = c;
                }
            }

            if !is_self_closing && !tag_name.is_empty() {
                xml_stack.push((current_line, tag_name));
            }
            continue;
        }

        match ch {
            '{' | '[' => {
                bracket_stack.push((current_line, ch));
            }
            '}' => {
                if let Some((start_l, open_c)) = bracket_stack.pop() {
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
                if let Some((start_l, open_c)) = bracket_stack.pop() {
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

    #[test]
    fn test_find_fold_regions_xml() {
        let xml = r#"<bookstore>
  <book id="1">
    <title>Great</title>
  </book>
  <single id="2" />
</bookstore>"#;
        let regions = find_fold_regions(xml);
        assert_eq!(regions.len(), 2);
        // Outer bookstore
        assert_eq!(regions[0].start_line, 0);
        assert_eq!(regions[0].end_line, 5);
        assert_eq!(regions[0].open_char, '<');
        // Inner book
        assert_eq!(regions[1].start_line, 1);
        assert_eq!(regions[1].end_line, 3);
        assert_eq!(regions[1].open_char, '<');
    }
}
