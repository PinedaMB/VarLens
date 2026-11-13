use crate::core::json::IndentStyle;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct XmlMetrics {
    pub max_depth: usize,
    pub total_elements: usize,
    pub total_attributes: usize,
    pub size_bytes: usize,
}

impl XmlMetrics {
    pub fn formatted_size(&self) -> String {
        if self.size_bytes < 1024 {
            format!("{} B", self.size_bytes)
        } else {
            format!("{:.1} KB", self.size_bytes as f64 / 1024.0)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmlToken {
    Prolog(String),
    Doctype(String),
    Comment(String),
    CData(String),
    StartTag {
        name: String,
        attributes: Vec<(String, String)>,
    },
    EndTag(String),
    SelfClosingTag {
        name: String,
        attributes: Vec<(String, String)>,
    },
    Text(String),
}

/// Tokenizes an XML string into tokens with line and column tracking for precise error reporting.
pub fn tokenize_xml(input: &str) -> Result<Vec<(XmlToken, usize, usize)>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.char_indices().peekable();
    let mut line = 1;
    let mut col = 1;

    let advance_pos = |ch: char, l: &mut usize, c: &mut usize| {
        if ch == '\n' {
            *l += 1;
            *c = 1;
        } else {
            *c += 1;
        }
    };

    while let Some(&(start_idx, ch)) = chars.peek() {
        let token_line = line;
        let token_col = col;

        if ch == '<' {
            chars.next();
            advance_pos('<', &mut line, &mut col);

            // Peek next character
            match chars.peek().map(|&(_, c)| c) {
                Some('?') => {
                    // Processing instruction / Prolog: <? ... ?>
                    chars.next();
                    advance_pos('?', &mut line, &mut col);
                    let mut content = String::new();
                    let mut closed = false;

                    while let Some((_, c)) = chars.next() {
                        advance_pos(c, &mut line, &mut col);
                        if c == '?' && chars.peek().map(|&(_, n)| n) == Some('>') {
                            chars.next();
                            advance_pos('>', &mut line, &mut col);
                            closed = true;
                            break;
                        }
                        content.push(c);
                    }

                    if !closed {
                        return Err(format!(
                            "Instrucción de procesamiento sin cerrar '<?' en línea {}, columna {}",
                            token_line, token_col
                        ));
                    }
                    tokens.push((XmlToken::Prolog(content.trim().to_string()), token_line, token_col));
                }
                Some('!') => {
                    chars.next();
                    advance_pos('!', &mut line, &mut col);

                    if input[start_idx..].starts_with("<!--") {
                        if let Some((_, c1)) = chars.next() { advance_pos(c1, &mut line, &mut col); }
                        if let Some((_, c2)) = chars.next() { advance_pos(c2, &mut line, &mut col); }

                        let mut comment = String::new();
                        let mut closed = false;
                        while let Some((_, c)) = chars.next() {
                            advance_pos(c, &mut line, &mut col);
                            if c == '-' && chars.peek().map(|&(_, n)| n) == Some('-') {
                                chars.next();
                                advance_pos('-', &mut line, &mut col);
                                if chars.peek().map(|&(_, n)| n) == Some('>') {
                                    chars.next();
                                    advance_pos('>', &mut line, &mut col);
                                    closed = true;
                                    break;
                                }
                                comment.push('-');
                            }
                            comment.push(c);
                        }
                        if !closed {
                            return Err(format!("Comentario sin cerrar '<!--' en línea {}, columna {}", token_line, token_col));
                        }
                        tokens.push((XmlToken::Comment(comment), token_line, token_col));
                    } else if input[start_idx..].starts_with("<![CDATA[") {
                        for _ in 0..7 {
                            if let Some((_, c)) = chars.next() { advance_pos(c, &mut line, &mut col); }
                        }
                        let mut cdata = String::new();
                        let mut closed = false;
                        while let Some((_, c)) = chars.next() {
                            advance_pos(c, &mut line, &mut col);
                            if c == ']' && chars.peek().map(|&(_, n)| n) == Some(']') {
                                chars.next();
                                advance_pos(']', &mut line, &mut col);
                                if chars.peek().map(|&(_, n)| n) == Some('>') {
                                    chars.next();
                                    advance_pos('>', &mut line, &mut col);
                                    closed = true;
                                    break;
                                }
                                cdata.push(']');
                            }
                            cdata.push(c);
                        }
                        if !closed {
                            return Err(format!("Bloque CDATA sin cerrar en línea {}, columna {}", token_line, token_col));
                        }
                        tokens.push((XmlToken::CData(cdata), token_line, token_col));
                    } else {
                        let mut decl = String::new();
                        let mut depth = 1;
                        for (_, c) in chars.by_ref() {
                            advance_pos(c, &mut line, &mut col);
                            if c == '<' {
                                depth += 1;
                            } else if c == '>' {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            decl.push(c);
                        }
                        tokens.push((XmlToken::Doctype(decl.trim().to_string()), token_line, token_col));
                    }
                }
                Some('/') => {
                    chars.next();
                    advance_pos('/', &mut line, &mut col);

                    let mut name = String::new();
                    for (_, c) in chars.by_ref() {
                        advance_pos(c, &mut line, &mut col);
                        if c == '>' {
                            break;
                        }
                        if !c.is_whitespace() {
                            name.push(c);
                        }
                    }

                    if name.is_empty() {
                        return Err(format!("Etiqueta de cierre vacía '</>' en línea {}, columna {}", token_line, token_col));
                    }
                    tokens.push((XmlToken::EndTag(name), token_line, token_col));
                }
                _ => {
                    let mut tag_content = String::new();
                    let mut in_quote: Option<char> = None;
                    let mut closed = false;

                    for (_, c) in chars.by_ref() {
                        advance_pos(c, &mut line, &mut col);
                        if let Some(q) = in_quote {
                            if c == q {
                                in_quote = None;
                            }
                            tag_content.push(c);
                        } else if c == '"' || c == '\'' {
                            in_quote = Some(c);
                            tag_content.push(c);
                        } else if c == '>' {
                            closed = true;
                            break;
                        } else {
                            tag_content.push(c);
                        }
                    }

                    if !closed {
                        return Err(format!("Etiqueta '<{}' sin cerrar en línea {}, columna {}", tag_content.split_whitespace().next().unwrap_or(""), token_line, token_col));
                    }

                    let trimmed = tag_content.trim();
                    let is_self_closing = trimmed.ends_with('/');
                    let body = if is_self_closing {
                        trimmed[..trimmed.len() - 1].trim()
                    } else {
                        trimmed
                    };

                    let (tag_name, attrs) = parse_tag_attributes(body, token_line, token_col)?;
                    if is_self_closing {
                        tokens.push((XmlToken::SelfClosingTag { name: tag_name, attributes: attrs }, token_line, token_col));
                    } else {
                        tokens.push((XmlToken::StartTag { name: tag_name, attributes: attrs }, token_line, token_col));
                    }
                }
            }
        } else {
            let mut text = String::new();
            while let Some(&(_, c)) = chars.peek() {
                if c == '<' {
                    break;
                }
                text.push(c);
                chars.next();
                advance_pos(c, &mut line, &mut col);
            }
            if !text.is_empty() {
                tokens.push((XmlToken::Text(text), token_line, token_col));
            }
        }
    }

    Ok(tokens)
}

fn parse_tag_attributes(body: &str, line: usize, col: usize) -> Result<(String, Vec<(String, String)>), String> {
    let mut parts = body.split_whitespace();
    let name = match parts.next() {
        Some(n) => n.to_string(),
        None => return Err(format!("Etiqueta sin nombre en línea {}, columna {}", line, col)),
    };

    let rest = &body[name.len()..].trim_start();
    let mut attributes = Vec::new();
    let mut chars = rest.chars().peekable();

    while chars.peek().is_some() {
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() { chars.next(); } else { break; }
        }
        if chars.peek().is_none() { break; }

        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c.is_whitespace() { break; }
            key.push(c);
            chars.next();
        }

        if key.is_empty() { break; }

        while let Some(&c) = chars.peek() {
            if c.is_whitespace() { chars.next(); } else { break; }
        }

        if chars.peek() == Some(&'=') {
            chars.next();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() { chars.next(); } else { break; }
            }

            let quote = chars.peek().copied();
            if quote == Some('"') || quote == Some('\'') {
                chars.next();
                let q = quote.unwrap();
                let mut val = String::new();
                for c in chars.by_ref() {
                    if c == q { break; }
                    val.push(c);
                }
                attributes.push((key, val));
            } else {
                let mut val = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() { break; }
                    val.push(c);
                    chars.next();
                }
                attributes.push((key, val));
            }
        } else {
            attributes.push((key, String::new()));
        }
    }

    Ok((name, attributes))
}

pub fn analyze_xml(input: &str) -> Result<XmlMetrics, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("El contenido está vacío.".to_string());
    }

    let tokens = tokenize_xml(trimmed)?;
    let mut stack: Vec<(String, usize, usize)> = Vec::new();
    let mut max_depth = 0;
    let mut total_elements = 0;
    let mut total_attributes = 0;
    let mut has_root = false;

    for (token, line, col) in tokens {
        match token {
            XmlToken::StartTag { name, attributes } => {
                has_root = true;
                total_elements += 1;
                total_attributes += attributes.len();
                stack.push((name, line, col));
                if stack.len() > max_depth {
                    max_depth = stack.len();
                }
            }
            XmlToken::SelfClosingTag { attributes, .. } => {
                has_root = true;
                total_elements += 1;
                total_attributes += attributes.len();
                if stack.len() + 1 > max_depth {
                    max_depth = stack.len() + 1;
                }
            }
            XmlToken::EndTag(name) => {
                match stack.pop() {
                    Some((open_name, open_line, open_col)) => {
                        if open_name != name {
                            return Err(format!(
                                "Etiqueta de cierre '</{}>' en línea {}, columna {} no coincide con '<{}>' abierta en línea {}, columna {}",
                                name, line, col, open_name, open_line, open_col
                            ));
                        }
                    }
                    None => {
                        return Err(format!(
                            "Etiqueta de cierre inesperada '</{}>' en línea {}, columna {} sin etiqueta de apertura",
                            name, line, col
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    if let Some((unclosed_name, line, col)) = stack.pop() {
        return Err(format!(
            "Etiqueta '<{}>' en línea {}, columna {} no fue cerrada (fin de archivo)",
            unclosed_name, line, col
        ));
    }

    if !has_root {
        return Err("El documento no contiene ningún elemento raíz XML.".to_string());
    }

    Ok(XmlMetrics {
        max_depth,
        total_elements,
        total_attributes,
        size_bytes: input.len(),
    })
}

pub fn format_xml_with_indent(
    input: &str,
    pretty: bool,
    indent: IndentStyle,
) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("El contenido está vacío.".to_string());
    }

    let tokens = tokenize_xml(trimmed)?;
    let _ = analyze_xml(trimmed)?;

    let indent_str = match indent {
        IndentStyle::Spaces2 => "  ",
        IndentStyle::Spaces4 => "    ",
        IndentStyle::Tabs => "\t",
    };

    if !pretty {
        let mut out = String::with_capacity(input.len());
        for (token, _, _) in tokens {
            match token {
                XmlToken::Prolog(p) => {
                    out.push_str("<?");
                    out.push_str(&p);
                    out.push_str("?>");
                }
                XmlToken::Doctype(d) => {
                    out.push_str("<!DOCTYPE ");
                    out.push_str(&d);
                    out.push('>');
                }
                XmlToken::Comment(c) => {
                    out.push_str("<!--");
                    out.push_str(&c);
                    out.push_str("-->");
                }
                XmlToken::CData(cd) => {
                    out.push_str("<![CDATA[");
                    out.push_str(&cd);
                    out.push_str("]]>");
                }
                XmlToken::StartTag { name, attributes } => {
                    out.push('<');
                    out.push_str(&name);
                    for (k, v) in attributes {
                        out.push(' ');
                        out.push_str(&k);
                        out.push_str("=\"");
                        out.push_str(&v);
                        out.push('"');
                    }
                    out.push('>');
                }
                XmlToken::SelfClosingTag { name, attributes } => {
                    out.push('<');
                    out.push_str(&name);
                    for (k, v) in attributes {
                        out.push(' ');
                        out.push_str(&k);
                        out.push_str("=\"");
                        out.push_str(&v);
                        out.push('"');
                    }
                    out.push_str("/>");
                }
                XmlToken::EndTag(name) => {
                    out.push_str("</");
                    out.push_str(&name);
                    out.push('>');
                }
                XmlToken::Text(t) => {
                    let trimmed_t = t.trim();
                    if !trimmed_t.is_empty() {
                        out.push_str(trimmed_t);
                    }
                }
            }
        }
        return Ok(out);
    }

    let mut out = String::with_capacity(input.len() + 1024);
    let mut depth: usize = 0;
    let mut prev_was_start_tag = false;
    let mut current_tag_has_children = false;

    let len = tokens.len();
    for i in 0..len {
        let (token, _, _) = &tokens[i];
        match token {
            XmlToken::Prolog(p) => {
                out.push_str("<?");
                out.push_str(p);
                out.push_str("?>\n");
            }
            XmlToken::Doctype(d) => {
                out.push_str("<!DOCTYPE ");
                out.push_str(d);
                out.push_str(">\n");
            }
            XmlToken::Comment(c) => {
                for _ in 0..depth { out.push_str(indent_str); }
                out.push_str("<!--");
                out.push_str(c);
                out.push_str("-->\n");
            }
            XmlToken::CData(cd) => {
                for _ in 0..depth { out.push_str(indent_str); }
                out.push_str("<![CDATA[");
                out.push_str(cd);
                out.push_str("]]>\n");
            }
            XmlToken::StartTag { name, attributes } => {
                let has_child_elements = has_nested_elements(&tokens, i + 1);

                for _ in 0..depth { out.push_str(indent_str); }
                out.push('<');
                out.push_str(name);
                for (k, v) in attributes {
                    out.push(' ');
                    out.push_str(k);
                    out.push_str("=\"");
                    out.push_str(v);
                    out.push('"');
                }
                out.push('>');

                if has_child_elements {
                    out.push('\n');
                    depth += 1;
                    current_tag_has_children = true;
                } else {
                    current_tag_has_children = false;
                }
                prev_was_start_tag = true;
            }
            XmlToken::SelfClosingTag { name, attributes } => {
                for _ in 0..depth { out.push_str(indent_str); }
                out.push('<');
                out.push_str(name);
                for (k, v) in attributes {
                    out.push(' ');
                    out.push_str(k);
                    out.push_str("=\"");
                    out.push_str(v);
                    out.push('"');
                }
                out.push_str(" />\n");
                prev_was_start_tag = false;
            }
            XmlToken::EndTag(name) => {
                if current_tag_has_children || !prev_was_start_tag {
                    if depth > 0 && current_tag_has_children {
                        depth -= 1;
                    }
                    for _ in 0..depth { out.push_str(indent_str); }
                }
                out.push_str("</");
                out.push_str(name);
                out.push_str(">\n");
                prev_was_start_tag = false;
                current_tag_has_children = true;
            }
            XmlToken::Text(t) => {
                let trimmed_t = t.trim();
                if !trimmed_t.is_empty() {
                    out.push_str(trimmed_t);
                }
            }
        }
    }

    Ok(out.trim_end().to_string())
}

fn has_nested_elements(tokens: &[(XmlToken, usize, usize)], start_idx: usize) -> bool {
    let mut balance = 1;
    for (tok, _, _) in &tokens[start_idx..] {
        match tok {
            XmlToken::StartTag { .. } | XmlToken::SelfClosingTag { .. } => {
                return true;
            }
            XmlToken::EndTag(_) => {
                balance -= 1;
                if balance == 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    false
}

#[derive(Debug)]
struct ElementNode {
    name: String,
    attributes: Vec<(String, String)>,
    children: Vec<ElementNode>,
    text: String,
}

impl ElementNode {
    fn to_json_value(&self) -> serde_json::Value {
        let has_attrs = !self.attributes.is_empty();
        let has_children = !self.children.is_empty();
        let trimmed_text = self.text.trim();

        if !has_attrs && !has_children {
            if trimmed_text.is_empty() {
                return serde_json::Value::Null;
            }
            if let Ok(b) = trimmed_text.parse::<bool>() {
                return serde_json::Value::Bool(b);
            }
            if let Ok(i) = trimmed_text.parse::<i64>() {
                return serde_json::json!(i);
            }
            if let Ok(f) = trimmed_text.parse::<f64>() {
                if f.is_finite() {
                    return serde_json::json!(f);
                }
            }
            return serde_json::Value::String(trimmed_text.to_string());
        }

        let mut map = serde_json::Map::new();

        for (k, v) in &self.attributes {
            map.insert(format!("@{}", k), serde_json::Value::String(v.clone()));
        }

        for child in &self.children {
            let child_val = child.to_json_value();
            match map.entry(&child.name) {
                serde_json::map::Entry::Vacant(e) => {
                    e.insert(child_val);
                }
                serde_json::map::Entry::Occupied(mut e) => {
                    if let serde_json::Value::Array(ref mut arr) = e.get_mut() {
                        arr.push(child_val);
                    } else {
                        let prev = e.get().clone();
                        e.insert(serde_json::Value::Array(vec![prev, child_val]));
                    }
                }
            }
        }

        if !trimmed_text.is_empty() {
            map.insert("#text".to_string(), serde_json::Value::String(trimmed_text.to_string()));
        }

        serde_json::Value::Object(map)
    }
}

/// Converts an XML string into a structured JSON string.
pub fn xml_to_json_string(input: &str, pretty: bool) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("El contenido está vacío.".to_string());
    }

    let tokens = tokenize_xml(trimmed)?;
    let mut stack: Vec<ElementNode> = Vec::new();
    let mut root_elements: Vec<ElementNode> = Vec::new();

    for (token, line, col) in tokens {
        match token {
            XmlToken::StartTag { name, attributes } => {
                stack.push(ElementNode {
                    name,
                    attributes,
                    children: Vec::new(),
                    text: String::new(),
                });
            }
            XmlToken::SelfClosingTag { name, attributes } => {
                let node = ElementNode {
                    name,
                    attributes,
                    children: Vec::new(),
                    text: String::new(),
                };
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    root_elements.push(node);
                }
            }
            XmlToken::EndTag(name) => {
                if let Some(node) = stack.pop() {
                    if node.name != name {
                        return Err(format!(
                            "Etiqueta de cierre '</{}>' en línea {}, columna {} no coincide con '<{}>'",
                            name, line, col, node.name
                        ));
                    }
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    } else {
                        root_elements.push(node);
                    }
                } else {
                    return Err(format!(
                        "Etiqueta de cierre inesperada '</{}>' en línea {}, columna {} sin etiqueta de apertura",
                        name, line, col
                    ));
                }
            }
            XmlToken::Text(t) => {
                if let Some(current) = stack.last_mut() {
                    current.text.push_str(&t);
                }
            }
            XmlToken::CData(cd) => {
                if let Some(current) = stack.last_mut() {
                    current.text.push_str(&cd);
                }
            }
            _ => {}
        }
    }

    if let Some(unclosed) = stack.pop() {
        return Err(format!(
            "Etiqueta '<{}>' no fue cerrada (fin de archivo)",
            unclosed.name
        ));
    }

    if root_elements.is_empty() {
        return Err("No se encontró ningún elemento raíz XML para convertir.".to_string());
    }

    let json_val = if root_elements.len() == 1 {
        let root = &root_elements[0];
        let mut map = serde_json::Map::new();
        map.insert(root.name.clone(), root.to_json_value());
        serde_json::Value::Object(map)
    } else {
        let mut map = serde_json::Map::new();
        for el in root_elements {
            let val = el.to_json_value();
            match map.entry(el.name) {
                serde_json::map::Entry::Vacant(e) => {
                    e.insert(val);
                }
                serde_json::map::Entry::Occupied(mut e) => {
                    if let serde_json::Value::Array(ref mut arr) = e.get_mut() {
                        arr.push(val);
                    } else {
                        let prev = e.get().clone();
                        e.insert(serde_json::Value::Array(vec![prev, val]));
                    }
                }
            }
        }
        serde_json::Value::Object(map)
    };

    if pretty {
        serde_json::to_string_pretty(&json_val).map_err(|e| e.to_string())
    } else {
        serde_json::to_string(&json_val).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_xml_analysis() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<bookstore>
    <book id="1" category="fiction">
        <title>The Great Gatsby</title>
        <price>12.99</price>
    </book>
    <book id="2" category="science">
        <title>A Brief History of Time</title>
        <price>15.50</price>
    </book>
</bookstore>"#;

        let metrics = analyze_xml(xml).expect("Should parse valid XML");
        assert_eq!(metrics.total_elements, 7);
        assert_eq!(metrics.max_depth, 3);
        assert_eq!(metrics.total_attributes, 4);
    }

    #[test]
    fn test_mismatched_tags() {
        let xml = "<root><item>Hello</other></root>";
        let err = analyze_xml(xml).expect_err("Should detect mismatched tag");
        assert!(err.contains("no coincide"));
    }

    #[test]
    fn test_unclosed_tag() {
        let xml = "<root><item>Hello</root>";
        let err = analyze_xml(xml).expect_err("Should detect unclosed tag");
        assert!(err.contains("no coincide") || err.contains("no fue cerrada"));
    }

    #[test]
    fn test_format_and_minify() {
        let xml = "<root><user id=\"42\"><name>Alice</name><active/></user></root>";
        let pretty = format_xml_with_indent(xml, true, IndentStyle::Spaces2).unwrap();
        assert!(pretty.contains("  <user id=\"42\">"));
        assert!(pretty.contains("    <name>Alice</name>"));

        let minified = format_xml_with_indent(&pretty, false, IndentStyle::Spaces2).unwrap();
        assert_eq!(minified, "<root><user id=\"42\"><name>Alice</name><active/></user></root>");
    }

    #[test]
    fn test_cdata_and_comments() {
        let xml = r#"<root>
    <!-- Important comment -->
    <data><![CDATA[Some <raw> & unescaped text]]></data>
</root>"#;
        let metrics = analyze_xml(xml).unwrap();
        assert_eq!(metrics.total_elements, 2);
    }

    #[test]
    fn test_xml_to_json() {
        let xml = r#"<user id="123"><name>John</name><age>30</age></user>"#;
        let json_str = xml_to_json_string(xml, false).unwrap();
        assert!(json_str.contains("\"@id\":\"123\""));
        assert!(json_str.contains("\"name\":\"John\""));
        assert!(json_str.contains("\"age\":30"));
    }

    #[test]
    fn test_xml_to_json_unclosed_fails() {
        let xml = r#"<user id="123"><name>John</user>"#;
        assert!(xml_to_json_string(xml, false).is_err());
    }
}
