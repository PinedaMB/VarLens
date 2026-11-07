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
            IndentStyle::Spaces2 => "2 Espacios",
            IndentStyle::Spaces4 => "4 Espacios",
            IndentStyle::Tabs => "Tabuladores",
        }
    }
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

    if pretty {
        let mut buf = Vec::new();
        let formatter = PrettyFormatter::with_indent(indent.as_bytes());
        let mut ser = Serializer::with_formatter(&mut buf, formatter);
        parsed
            .serialize(&mut ser)
            .map_err(|err| format!("Error al serializar JSON con formato: {}", err))?;
        String::from_utf8(buf).map_err(|err| format!("Error UTF-8 al formatear JSON: {}", err))
    } else {
        serde_json::to_string(&parsed).map_err(|err| format!("Error al minificar JSON: {}", err))
    }
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
}
