use serde::Serialize;
use serde_json::ser::PrettyFormatter;
use serde_json::{Serializer, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentStyle {
    Spaces2,
    Spaces4,
    Tabs,
}

impl IndentStyle {
    pub fn as_bytes(&self) -> &'static [u8] {
        match self {
            IndentStyle::Spaces2 => b"  ",
            IndentStyle::Spaces4 => b"    ",
            IndentStyle::Tabs => b"\t",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            IndentStyle::Spaces2 => crate::tr!("2 Espacios", "2 Spaces"),
            IndentStyle::Spaces4 => crate::tr!("4 Espacios", "4 Spaces"),
            IndentStyle::Tabs => crate::tr!("Tabuladores", "Tabs"),
        }
    }
}

/// Formats a JSON string sorting all object keys alphabetically (recursively).
pub fn sort_json_keys(input: &str, indent: IndentStyle) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(crate::tr!("El contenido está vacío.", "Content is empty.").to_string());
    }

    let parsed: Value = serde_json::from_str(trimmed).map_err(|err| {
        format!(
            "{} {}, {} {}: {}",
            crate::tr!("Error de sintaxis JSON en línea", "JSON syntax error at line"),
            err.line(),
            crate::tr!("columna", "column"),
            err.column(),
            err
        )
    })?;

    fn sort_value(val: Value) -> Value {
        match val {
            Value::Object(map) => {
                let mut entries: Vec<(String, Value)> =
                    map.into_iter().map(|(k, v)| (k, sort_value(v))).collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                let mut sorted_map = serde_json::Map::new();
                for (k, v) in entries {
                    sorted_map.insert(k, v);
                }
                Value::Object(sorted_map)
            }
            Value::Array(arr) => Value::Array(arr.into_iter().map(sort_value).collect()),
            other => other,
        }
    }

    let sorted = sort_value(parsed);
    let mut buf = Vec::new();
    let formatter = PrettyFormatter::with_indent(indent.as_bytes());
    let mut ser = Serializer::with_formatter(&mut buf, formatter);
    sorted
        .serialize(&mut ser)
        .map_err(|err| format!("{}: {}", crate::tr!("Error al serializar JSON", "Error serializing JSON"), err))?;
    String::from_utf8(buf).map_err(|err| format!("UTF-8 error: {}", err))
}

#[derive(Debug, Clone, Copy, Default)]
pub struct JsonMetrics {
    pub max_depth: usize,
    pub total_keys: usize,
    pub total_nodes: usize,
    pub size_bytes: usize,
}

impl JsonMetrics {
    pub fn formatted_size(&self) -> String {
        if self.size_bytes < 1024 {
            format!("{} B", self.size_bytes)
        } else {
            format!("{:.1} KB", self.size_bytes as f64 / 1024.0)
        }
    }
}

/// Formats a `serde_json::Value` either as pretty-printed (with custom indent) or minified.
pub fn format_value_with_indent(
    val: &Value,
    pretty: bool,
    indent: IndentStyle,
) -> Result<String, String> {
    if pretty {
        let mut buf = Vec::new();
        let formatter = PrettyFormatter::with_indent(indent.as_bytes());
        let mut ser = Serializer::with_formatter(&mut buf, formatter);
        val.serialize(&mut ser)
            .map_err(|err| format!("Error al serializar JSON con formato: {}", err))?;
        String::from_utf8(buf).map_err(|err| format!("Error UTF-8 al formatear JSON: {}", err))
    } else {
        serde_json::to_string(val).map_err(|err| format!("Error al minificar JSON: {}", err))
    }
}

/// Formats a JSON string either as pretty-printed (with custom indent) or minified (compact).
pub fn format_json_with_indent(
    input: &str,
    pretty: bool,
    indent: IndentStyle,
) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("El contenido está vacío.".to_string());
    }

    let parsed: Value = serde_json::from_str(trimmed).map_err(|err| {
        format!(
            "Error de sintaxis JSON en línea {}, columna {}: {}",
            err.line(),
            err.column(),
            err
        )
    })?;

    format_value_with_indent(&parsed, pretty, indent)
}

/// Formats a JSON string using default 2-space indentation when `pretty` is true.
#[allow(dead_code)]
pub fn format_json(input: &str, pretty: bool) -> Result<String, String> {
    format_json_with_indent(input, pretty, IndentStyle::Spaces2)
}

/// Validates whether the given string is valid JSON syntax.
#[allow(dead_code)]
pub fn validate_json(input: &str) -> Result<(), String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("El contenido está vacío.".to_string());
    }

    let _: Value = serde_json::from_str(trimmed).map_err(|err| {
        format!(
            "JSON inválido en línea {}, columna {}: {}",
            err.line(),
            err.column(),
            err
        )
    })?;

    Ok(())
}

/// Computes AST metrics (depth, keys, total nodes, byte size) if input is valid JSON.
pub fn analyze_json(input: &str) -> Result<JsonMetrics, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Vacío".to_string());
    }

    let parsed: Value = serde_json::from_str(trimmed)
        .map_err(|err| format!("Línea {}, Col {}", err.line(), err.column()))?;

    let mut metrics = JsonMetrics {
        max_depth: 0,
        total_keys: 0,
        total_nodes: 0,
        size_bytes: input.len(),
    };

    fn walk(val: &Value, current_depth: usize, m: &mut JsonMetrics) {
        m.total_nodes += 1;
        if current_depth > m.max_depth {
            m.max_depth = current_depth;
        }
        match val {
            Value::Object(map) => {
                m.total_keys += map.len();
                for v in map.values() {
                    walk(v, current_depth + 1, m);
                }
            }
            Value::Array(arr) => {
                for v in arr {
                    walk(v, current_depth + 1, m);
                }
            }
            _ => {}
        }
    }

    walk(&parsed, 0, &mut metrics);
    Ok(metrics)
}

/// Helper to count the number of lines in a text slice.
pub fn count_lines(text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        text.lines().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_pretty() {
        let raw = r#"{"b":2,"a":1}"#;
        let formatted = format_json(raw, true).unwrap();
        assert!(formatted.contains('\n'));
        assert_eq!(formatted, "{\n  \"b\": 2,\n  \"a\": 1\n}");
    }

    #[test]
    fn test_format_indent_4() {
        let raw = r#"{"a":1}"#;
        let formatted = format_json_with_indent(raw, true, IndentStyle::Spaces4).unwrap();
        assert_eq!(formatted, "{\n    \"a\": 1\n}");
    }

    #[test]
    fn test_format_minify() {
        let raw = "{\n  \"hello\": \"world\"\n}";
        let minified = format_json(raw, false).unwrap();
        assert_eq!(minified, r#"{"hello":"world"}"#);
    }

    #[test]
    fn test_validate_invalid() {
        let raw = r#"{"name": "test",}"#;
        assert!(validate_json(raw).is_err());
    }

    #[test]
    fn test_analyze_metrics() {
        let raw = r#"{"id": "1", "actor": {"name": "miguel", "active": true}}"#;
        let m = analyze_json(raw).unwrap();
        assert_eq!(m.total_keys, 4);
        assert_eq!(m.max_depth, 2);
    }

    #[test]
    fn test_sort_json_keys() {
        let raw = r#"{"z": 1, "a": {"d": 4, "c": 3}, "b": [{"y": 2, "x": 1}]}"#;
        let sorted = sort_json_keys(raw, IndentStyle::Spaces2).unwrap();
        let expected = "{\n  \"a\": {\n    \"c\": 3,\n    \"d\": 4\n  },\n  \"b\": [\n    {\n      \"x\": 1,\n      \"y\": 2\n    }\n  ],\n  \"z\": 1\n}";
        assert_eq!(sorted, expected);
    }
}
