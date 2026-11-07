pub mod parser;

use parser::{Rule, VarDumpParser};
use pest::iterators::Pair;
use pest::Parser;
use serde_json::{Map, Number, Value};

/// Unescapes common PHP string escape sequences.
fn unescape_php_string(raw: &str) -> String {
    let mut result = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Extracts a clean key from an `object_key` AST pair.
fn parse_object_key(pair: Pair<Rule>) -> (Option<i64>, String) {
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::quoted_key => {
                if let Some(inner_key) = inner.into_inner().next() {
                    return (None, unescape_php_string(inner_key.as_str()));
                }
                return (None, String::new());
            }
            Rule::int_lit => {
                let s = inner.as_str();
                return (s.parse::<i64>().ok(), s.to_string());
            }
            Rule::identifier => {
                return (None, inner.as_str().to_string());
            }
            _ => {}
        }
    }
    (None, String::new())
}

/// Extracts a clean key from an array `key` AST pair.
fn parse_array_key(pair: Pair<Rule>) -> (Option<i64>, String) {
    let inner = pair.clone().into_inner().next().unwrap_or(pair);
    match inner.as_rule() {
        Rule::int_lit => {
            let s = inner.as_str();
            let num = s.parse::<i64>().ok();
            (num, s.to_string())
        }
        Rule::quoted_key => {
            let key_str = if let Some(ik) = inner.into_inner().next() {
                unescape_php_string(ik.as_str())
            } else {
                String::new()
            };
            (None, key_str)
        }
        Rule::identifier => (None, inner.as_str().to_string()),
        _ => (None, inner.as_str().to_string()),
    }
}

fn parse_printr_scalar(raw: &str) -> Value {
    let s = raw.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("null") {
        return Value::Null;
    }
    if s == "true" {
        return Value::Bool(true);
    }
    if s == "false" {
        return Value::Bool(false);
    }
    if let Ok(i) = s.parse::<i64>() {
        return Value::Number(i.into());
    }
    if let Ok(f) = s.parse::<f64>() {
        if let Some(num) = Number::from_f64(f) {
            return Value::Number(num);
        }
    }
    Value::String(s.to_string())
}

/// Recursively converts a Pest AST Pair into a `serde_json::Value`.
fn pair_to_value(pair: Pair<Rule>) -> Result<Value, String> {
    match pair.as_rule() {
        Rule::root => {
            let value_pair = pair
                .into_inner()
                .find(|p| p.as_rule() == Rule::value)
                .ok_or_else(|| "No se encontró un valor raíz en la salida de var_dump".to_string())?;
            pair_to_value(value_pair)
        }
        Rule::value => {
            let inner = pair
                .into_inner()
                .find(|p| p.as_rule() != Rule::reference)
                .ok_or_else(|| "Nodo de valor vacío en var_dump".to_string())?;
            pair_to_value(inner)
        }
        Rule::null_val => Ok(Value::Null),
        Rule::bool_val => {
            let lit = pair
                .into_inner()
                .next()
                .ok_or_else(|| "Nodo bool sin literal".to_string())?;
            match lit.as_str() {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                other => Err(format!("Valor booleano desconocido: {}", other)),
            }
        }
        Rule::int_val => {
            let lit = pair
                .into_inner()
                .next()
                .ok_or_else(|| "Nodo int sin literal".to_string())?;
            let num: i64 = lit
                .as_str()
                .parse()
                .map_err(|e| format!("Número entero inválido '{}': {}", lit.as_str(), e))?;
            Ok(Value::Number(num.into()))
        }
        Rule::float_val => {
            let lit = pair
                .into_inner()
                .next()
                .ok_or_else(|| "Nodo float sin literal".to_string())?;
            let s = lit.as_str();
            match s {
                "INF" | "+INF" => Ok(Value::String("INF".to_string())),
                "-INF" => Ok(Value::String("-INF".to_string())),
                "NAN" => Ok(Value::String("NAN".to_string())),
                _ => {
                    let f: f64 = s
                        .parse()
                        .map_err(|e| format!("Número flotante inválido '{}': {}", s, e))?;
                    if let Some(num) = Number::from_f64(f) {
                        Ok(Value::Number(num))
                    } else {
                        Ok(Value::Null)
                    }
                }
            }
        }
        Rule::string_val => {
            let quoted = pair
                .into_inner()
                .find(|p| p.as_rule() == Rule::quoted_string)
                .ok_or_else(|| "Cadena sin contenido entre comillas".to_string())?;
            let inner = quoted.into_inner().next();
            let raw_str = inner.map(|p| p.as_str()).unwrap_or("");
            Ok(Value::String(unescape_php_string(raw_str)))
        }
        Rule::array_val => {
            let mut entries: Vec<(Option<i64>, String, Value)> = Vec::new();
            for item in pair.into_inner() {
                if item.as_rule() == Rule::array_entry {
                    let mut entry_inner = item.into_inner();
                    let key_pair = entry_inner
                        .next()
                        .ok_or_else(|| "Entrada de array sin clave".to_string())?;
                    let val_pair = entry_inner
                        .next()
                        .ok_or_else(|| "Entrada de array sin valor".to_string())?;

                    let (num_key, str_key) = parse_array_key(key_pair);
                    let val = pair_to_value(val_pair)?;
                    entries.push((num_key, str_key, val));
                }
            }

            let is_sequential = !entries.is_empty()
                && entries
                    .iter()
                    .enumerate()
                    .all(|(idx, (num_opt, _, _))| *num_opt == Some(idx as i64));

            if entries.is_empty() || is_sequential {
                let vec: Vec<Value> = entries.into_iter().map(|(_, _, v)| v).collect();
                Ok(Value::Array(vec))
            } else {
                let mut map = Map::new();
                for (_, k, v) in entries {
                    map.insert(k, v);
                }
                Ok(Value::Object(map))
            }
        }
        Rule::object_val => {
            let mut map = Map::new();
            for item in pair.into_inner() {
                if item.as_rule() == Rule::object_entry {
                    let mut entry_inner = item.into_inner();
                    let key_pair = entry_inner
                        .next()
                        .ok_or_else(|| "Entrada de objeto sin propiedad".to_string())?;
                    let val_pair = entry_inner
                        .next()
                        .ok_or_else(|| "Entrada de objeto sin valor".to_string())?;

                    let (_, key_str) = parse_object_key(key_pair);
                    let val = pair_to_value(val_pair)?;
                    map.insert(key_str, val);
                }
            }
            Ok(Value::Object(map))
        }
        Rule::printr_array => {
            let mut entries: Vec<(Option<i64>, String, Value)> = Vec::new();
            for item in pair.into_inner() {
                if item.as_rule() == Rule::printr_entry {
                    let mut entry_inner = item.into_inner();
                    let key_pair = entry_inner
                        .next()
                        .ok_or_else(|| "Entrada print_r sin clave".to_string())?;
                    let val_pair = entry_inner
                        .next()
                        .ok_or_else(|| "Entrada print_r sin valor".to_string())?;

                    let (num_key, str_key) = parse_object_key(key_pair);
                    let val = pair_to_value(val_pair)?;
                    entries.push((num_key, str_key, val));
                }
            }

            let is_sequential = !entries.is_empty()
                && entries
                    .iter()
                    .enumerate()
                    .all(|(idx, (num_opt, _, _))| *num_opt == Some(idx as i64));

            if entries.is_empty() || is_sequential {
                let vec: Vec<Value> = entries.into_iter().map(|(_, _, v)| v).collect();
                Ok(Value::Array(vec))
            } else {
                let mut map = Map::new();
                for (_, k, v) in entries {
                    map.insert(k, v);
                }
                Ok(Value::Object(map))
            }
        }
        Rule::printr_object => {
            let mut map = Map::new();
            for item in pair.into_inner() {
                if item.as_rule() == Rule::printr_entry {
                    let mut entry_inner = item.into_inner();
                    let key_pair = entry_inner
                        .next()
                        .ok_or_else(|| "Entrada print_r objeto sin clave".to_string())?;
                    let val_pair = entry_inner
                        .next()
                        .ok_or_else(|| "Entrada print_r objeto sin valor".to_string())?;

                    let (_, key_str) = parse_object_key(key_pair);
                    let val = pair_to_value(val_pair)?;
                    map.insert(key_str, val);
                }
            }
            Ok(Value::Object(map))
        }
        Rule::printr_scalar => Ok(parse_printr_scalar(pair.as_str())),
        Rule::resource_val => Ok(Value::String(pair.as_str().to_string())),
        Rule::recursion_val => Ok(Value::String("*RECURSION*".to_string())),
        Rule::uninitialized_val => Ok(Value::Null),
        other => Err(format!("Regla inesperada de var_dump: {:?}", other)),
    }
}

/// Parses a PHP `var_dump()` or `print_r()` output string into a `serde_json::Value`.
pub fn parse_vardump(input: &str) -> Result<Value, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("El contenido de var_dump está vacío.".to_string());
    }

    let mut pairs = VarDumpParser::parse(Rule::root, trimmed)
        .map_err(|err| format!("Error sintáctico al parsear var_dump:\n{}", err))?;

    let root_pair = pairs
        .next()
        .ok_or_else(|| "Entrada de var_dump no produjo tokens.".to_string())?;

    pair_to_value(root_pair)
}

/// Parses a PHP `var_dump()` or `print_r()` output and formats it directly to a JSON string.
pub fn vardump_to_json_string(input: &str, pretty: bool) -> Result<String, String> {
    let val = parse_vardump(input)?;
    if pretty {
        serde_json::to_string_pretty(&val)
            .map_err(|e| format!("Error al serializar a JSON: {}", e))
    } else {
        serde_json::to_string(&val)
            .map_err(|e| format!("Error al serializar a JSON: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitives() {
        assert_eq!(parse_vardump("NULL").unwrap(), Value::Null);
        assert_eq!(parse_vardump("bool(true)").unwrap(), Value::Bool(true));
        assert_eq!(parse_vardump("bool(false)").unwrap(), Value::Bool(false));
        assert_eq!(parse_vardump("int(42)").unwrap(), Value::Number(42.into()));
        assert_eq!(parse_vardump("int(-15)").unwrap(), Value::Number((-15).into()));
        assert_eq!(parse_vardump("float(0)").unwrap(), serde_json::json!(0.0));
        assert_eq!(parse_vardump("double(128.45)").unwrap(), serde_json::json!(128.45));
        assert_eq!(
            parse_vardump(r#"string(5) "hello""#).unwrap(),
            Value::String("hello".to_string())
        );
    }

    #[test]
    fn test_sequential_array() {
        let input = r#"
            array(3) {
                [0]=>
                int(10)
                [1]=>
                int(20)
                [2]=>
                int(30)
            }
        "#;
        let val = parse_vardump(input).unwrap();
        assert!(val.is_array());
        assert_eq!(val.as_array().unwrap().len(), 3);
        assert_eq!(val[0], 10);
        assert_eq!(val[1], 20);
        assert_eq!(val[2], 30);
    }

    #[test]
    fn test_associative_array() {
        let input = r#"
            array(2) {
                ["name"]=>
                string(4) "John"
                ["age"]=>
                int(35)
            }
        "#;
        let val = parse_vardump(input).unwrap();
        assert!(val.is_object());
        assert_eq!(val["name"], "John");
        assert_eq!(val["age"], 35);
    }

    #[test]
    fn test_nested_object_and_array() {
        let input = r#"
            /var/www/html/app.php:24:
            object(App\User)#5 (2) {
                ["id"]=>
                int(1)
                ["roles"]=>
                array(2) {
                    [0]=>
                    string(5) "admin"
                    [1]=>
                    string(4) "user"
                }
            }
        "#;
        let val = parse_vardump(input).unwrap();
        assert!(val.is_object());
        assert_eq!(val["id"], 1);
        assert_eq!(val["roles"][0], "admin");
        assert_eq!(val["roles"][1], "user");
    }

    #[test]
    fn test_visibility_keys() {
        let input = r#"
            object(SecretClass)#1 (2) {
                ["secret":"SecretClass":private]=>
                string(7) "private"
                ["token":protected]=>
                string(3) "xyz"
            }
        "#;
        let val = parse_vardump(input).unwrap();
        assert_eq!(val["secret"], "private");
        assert_eq!(val["token"], "xyz");
    }

    #[test]
    fn test_printr_array_raw() {
        let input = r#"
Array
(
    [id] => usr_9f83a21
    [quota_limit] => 4096
    [active] => true
    [roles] => Array
        (
            [0] => admin
            [1] => systems-architect
        )
)
        "#;
        let val = parse_vardump(input).unwrap();
        assert!(val.is_object());
        assert_eq!(val["id"], "usr_9f83a21");
        assert_eq!(val["quota_limit"], 4096);
        assert_eq!(val["active"], true);
        assert_eq!(val["roles"][0], "admin");
        assert_eq!(val["roles"][1], "systems-architect");
    }
}
