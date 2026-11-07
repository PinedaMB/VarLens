use serde_json::Value;

/// Formats a JSON string either as pretty-printed (indented) or minified (compact).
///
/// Uses `serde_json` with `preserve_order` to maintain the original ordering of keys.
pub fn format_json(input: &str, pretty: bool) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("El contenido está vacío.".to_string());
    }

    let parsed: Value = serde_json::from_str(trimmed)
        .map_err(|err| format!("Error de sintaxis JSON en línea {}, columna {}: {}", err.line(), err.column(), err))?;

    if pretty {
        serde_json::to_string_pretty(&parsed)
            .map_err(|err| format!("Error al serializar JSON con formato: {}", err))
    } else {
        serde_json::to_string(&parsed)
            .map_err(|err| format!("Error al minificar JSON: {}", err))
    }
}

/// Validates whether the given string is valid JSON syntax.
pub fn validate_json(input: &str) -> Result<(), String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("El contenido está vacío.".to_string());
    }

    let _: Value = serde_json::from_str(trimmed)
        .map_err(|err| format!("JSON inválido en línea {}, columna {}: {}", err.line(), err.column(), err))?;

    Ok(())
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
}
