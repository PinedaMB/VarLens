use serde_json::{Map, Value};

/// Returns `true` if `input` looks like a PHP `serialize()` payload or PHP session string.
pub fn is_php_serialized(input: &str) -> bool {
    let s = input.trim();
    if s.is_empty() {
        return false;
    }
    if s == "N;" {
        return true;
    }
    let bytes = s.as_bytes();
    if bytes.len() >= 4 && matches!(bytes[0], b'b' | b'i' | b'd' | b's' | b'a' | b'O' | b'C' | b'r' | b'R') && bytes[1] == b':' {
        return true;
    }
    // PHP session format: `key|...`
    if let Some(pipe_idx) = s.find('|') {
        let key = &s[..pipe_idx];
        let rest = &s[pipe_idx + 1..];
        if !key.is_empty()
            && !key.contains(|c: char| c.is_whitespace() || c == '{' || c == '[')
            && is_php_serialized(rest)
        {
            return true;
        }
    }
    false
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            bytes: input.as_bytes(),
            pos: 0,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn expect(&mut self, expected: u8) -> Result<(), String> {
        match self.bytes.get(self.pos).copied() {
            Some(b) if b == expected => {
                self.pos += 1;
                Ok(())
            }
            Some(b) => Err(format!(
                "Se esperaba '{}' pero se encontró '{}' en byte {}",
                expected as char, b as char, self.pos
            )),
            None => Err(format!(
                "Fin inesperado de entrada; se esperaba '{}'",
                expected as char
            )),
        }
    }

    fn read_until(&mut self, delim: u8) -> Result<&'a str, String> {
        let start = self.pos;
        while self.pos < self.bytes.len() && self.bytes[self.pos] != delim {
            self.pos += 1;
        }
        if self.pos >= self.bytes.len() {
            return Err(format!("No se encontró el delimitador '{}'", delim as char));
        }
        let slice = &self.bytes[start..self.pos];
        self.pos += 1; // consume delim
        std::str::from_utf8(slice).map_err(|e| format!("UTF-8 inválido: {}", e))
    }

    fn read_usize(&mut self, delim: u8) -> Result<usize, String> {
        let s = self.read_until(delim)?;
        s.trim()
            .parse::<usize>()
            .map_err(|_| format!("Longitud inválida '{}' en byte {}", s, self.pos))
    }

    fn read_exact_str(&mut self, len: usize) -> Result<String, String> {
        if self.pos + len > self.bytes.len() {
            return Err(format!(
                "Longitud de cadena ({}) excede los bytes disponibles en posición {}",
                len, self.pos
            ));
        }
        let slice = &self.bytes[self.pos..self.pos + len];
        self.pos += len;
        Ok(String::from_utf8_lossy(slice).into_owned())
    }
}

/// Cleans PHP serialized property visibility prefixes (`\0ClassName\0prop` and `\0*\0prop`).
fn clean_php_property_name(raw: &str) -> String {
    if let Some(rest) = raw.strip_prefix('\0') {
        if let Some(idx) = rest.find('\0') {
            let scope = &rest[..idx];
            let prop = &rest[idx + 1..];
            if scope == "*" {
                return format!("{} (protected)", prop);
            }
            return format!("{} (private)", prop);
        }
    }
    raw.to_string()
}

fn parse_value(cur: &mut Cursor<'_>) -> Result<Value, String> {
    cur.skip_ws();
    let tag = cur
        .peek()
        .ok_or_else(|| "Entrada de serialize() vacía o incompleta".to_string())?;
    cur.pos += 1;

    match tag {
        b'N' => {
            cur.expect(b';')?;
            Ok(Value::Null)
        }
        b'b' => {
            cur.expect(b':')?;
            let val_str = cur.read_until(b';')?;
            match val_str.trim() {
                "0" => Ok(Value::Bool(false)),
                "1" => Ok(Value::Bool(true)),
                other => Err(format!("Booleano inválido en serialize(): '{}'", other)),
            }
        }
        b'i' => {
            cur.expect(b':')?;
            let val_str = cur.read_until(b';')?;
            let num: i64 = val_str
                .trim()
                .parse()
                .map_err(|_| format!("Entero inválido en serialize(): '{}'", val_str))?;
            Ok(Value::Number(num.into()))
        }
        b'd' => {
            cur.expect(b':')?;
            let val_str = cur.read_until(b';')?;
            let t = val_str.trim();
            if t == "INF" || t == "-INF" || t == "NAN" {
                return Ok(Value::String(t.to_string()));
            }
            let f: f64 = t
                .parse()
                .map_err(|_| format!("Decimal inválido en serialize(): '{}'", t))?;
            serde_json::Number::from_f64(f)
                .map(Value::Number)
                .ok_or_else(|| format!("Número flotante no representable en JSON: {}", t))
        }
        b's' => {
            cur.expect(b':')?;
            let len = cur.read_usize(b':')?;
            cur.expect(b'"')?;
            let s = cur.read_exact_str(len)?;
            cur.expect(b'"')?;
            cur.expect(b';')?;
            Ok(Value::String(s))
        }
        b'r' | b'R' => {
            cur.expect(b':')?;
            let ref_id = cur.read_until(b';')?;
            Ok(Value::String(format!("*REFERENCE(#{})*", ref_id.trim())))
        }
        b'a' => {
            cur.expect(b':')?;
            let count = cur.read_usize(b':')?;
            cur.expect(b'{')?;
            let mut entries = Vec::with_capacity(count);
            for _ in 0..count {
                let k = parse_key(cur)?;
                let v = parse_value(cur)?;
                entries.push((k, v));
            }
            cur.skip_ws();
            cur.expect(b'}')?;

            let is_sequential = entries
                .iter()
                .enumerate()
                .all(|(idx, (k, _))| k == &idx.to_string());

            if is_sequential {
                Ok(Value::Array(entries.into_iter().map(|(_, v)| v).collect()))
            } else {
                let mut map = Map::new();
                for (k, v) in entries {
                    map.insert(k, v);
                }
                Ok(Value::Object(map))
            }
        }
        b'O' => {
            cur.expect(b':')?;
            let cname_len = cur.read_usize(b':')?;
            cur.expect(b'"')?;
            let class_name = cur.read_exact_str(cname_len)?;
            cur.expect(b'"')?;
            cur.expect(b':')?;
            let prop_count = cur.read_usize(b':')?;
            cur.expect(b'{')?;

            let mut map = Map::new();
            map.insert("__class".to_string(), Value::String(class_name));
            for _ in 0..prop_count {
                let raw_key = parse_key(cur)?;
                let clean_key = clean_php_property_name(&raw_key);
                let val = parse_value(cur)?;
                map.insert(clean_key, val);
            }
            cur.skip_ws();
            cur.expect(b'}')?;
            Ok(Value::Object(map))
        }
        b'C' => {
            cur.expect(b':')?;
            let cname_len = cur.read_usize(b':')?;
            cur.expect(b'"')?;
            let class_name = cur.read_exact_str(cname_len)?;
            cur.expect(b'"')?;
            cur.expect(b':')?;
            let data_len = cur.read_usize(b':')?;
            cur.expect(b'{')?;
            let data = cur.read_exact_str(data_len)?;
            cur.expect(b'}')?;

            let mut map = Map::new();
            map.insert("__class".to_string(), Value::String(class_name));
            map.insert("__serialized".to_string(), Value::String(data));
            Ok(Value::Object(map))
        }
        other => Err(format!(
            "Tipo desconocido '{}' en serialize() en byte {}",
            other as char,
            cur.pos.saturating_sub(1)
        )),
    }
}

fn parse_key(cur: &mut Cursor<'_>) -> Result<String, String> {
    match parse_value(cur)? {
        Value::String(s) => Ok(s),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(if b { "1" } else { "0" }.to_string()),
        Value::Null => Ok(String::new()),
        other => Ok(other.to_string()),
    }
}

/// Parses a PHP `serialize()` payload or PHP session string into a `serde_json::Value`.
pub fn parse_php_serialized(input: &str) -> Result<Value, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("El contenido de serialize() está vacío.".to_string());
    }

    let mut cur = Cursor::new(trimmed);
    // Try single serialized value first
    if let Ok(val) = parse_value(&mut cur) {
        cur.skip_ws();
        if cur.pos == cur.bytes.len() {
            return Ok(val);
        }
    }

    // Try PHP session format: `key1|val1;key2|val2;`
    let mut sess_cur = Cursor::new(trimmed);
    let mut sess_map = Map::new();
    while sess_cur.pos < sess_cur.bytes.len() {
        sess_cur.skip_ws();
        if sess_cur.pos >= sess_cur.bytes.len() {
            break;
        }
        let key = sess_cur.read_until(b'|')?;
        let val = parse_value(&mut sess_cur)?;
        sess_map.insert(key.to_string(), val);
    }

    if sess_map.is_empty() {
        Err("No se pudo parsear como PHP serialize()".to_string())
    } else {
        Ok(Value::Object(sess_map))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitives_and_utf8_bytes() {
        assert_eq!(parse_php_serialized("N;").unwrap(), Value::Null);
        assert_eq!(parse_php_serialized("b:1;").unwrap(), Value::Bool(true));
        assert_eq!(parse_php_serialized("i:-42;").unwrap(), serde_json::json!(-42));
        assert_eq!(parse_php_serialized("d:12.5;").unwrap(), serde_json::json!(12.5));
        // "café" is 5 bytes in UTF-8
        assert_eq!(
            parse_php_serialized(r#"s:5:"café";"#).unwrap(),
            Value::String("café".to_string())
        );
    }

    #[test]
    fn test_arrays_and_objects_with_visibility() {
        let payload = "a:2:{s:4:\"user\";O:4:\"User\":3:{s:2:\"id\";i:7;s:7:\"\0*\0role\";s:5:\"admin\";s:12:\"\0User\0secret\";s:3:\"xyz\";}s:4:\"tags\";a:2:{i:0;s:3:\"php\";i:1;s:4:\"rust\";}}";
        let val = parse_php_serialized(payload).unwrap();
        assert_eq!(val["user"]["__class"], "User");
        assert_eq!(val["user"]["id"], 7);
        assert_eq!(val["user"]["role (protected)"], "admin");
        assert_eq!(val["user"]["secret (private)"], "xyz");
        assert_eq!(val["tags"][0], "php");
        assert_eq!(val["tags"][1], "rust");
    }

    #[test]
    fn test_php_session_format() {
        let session = r#"user_id|i:42;username|s:6:"brayan";active|b:1;"#;
        assert!(is_php_serialized(session));
        let val = parse_php_serialized(session).unwrap();
        assert_eq!(val["user_id"], 42);
        assert_eq!(val["username"], "brayan");
        assert_eq!(val["active"], true);
    }
}
