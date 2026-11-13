use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodedKind {
    EscapedJson,
    Jwt,
    Base64,
}

impl DecodedKind {
    pub fn badge_label(&self) -> &'static str {
        match self {
            DecodedKind::EscapedJson => "JSON-STR",
            DecodedKind::Jwt => "JWT",
            DecodedKind::Base64 => "B64",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedPayload {
    pub kind: DecodedKind,
    pub value: Value,
}

fn decode_base64_bytes(input: &str, url_safe: bool) -> Option<Vec<u8>> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }

    let mut buf: Vec<u8> = Vec::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b == b'=' {
            continue;
        }
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' if !url_safe => 62,
            b'/' if !url_safe => 63,
            b'-' if url_safe => 62,
            b'_' if url_safe => 63,
            _ => return None,
        };
        buf.push(val);
    }

    if buf.len() < 2 {
        return None;
    }

    let mut out = Vec::with_capacity((buf.len() * 3) / 4);
    let mut i = 0;
    while i + 1 < buf.len() {
        let b0 = buf[i];
        let b1 = buf[i + 1];
        out.push((b0 << 2) | (b1 >> 4));
        if i + 2 < buf.len() {
            let b2 = buf[i + 2];
            out.push((b1 << 4) | (b2 >> 2));
            if i + 3 < buf.len() {
                let b3 = buf[i + 3];
                out.push((b2 << 6) | b3);
            }
        }
        i += 4;
    }
    Some(out)
}

/// Detects whether a string node contains escaped JSON, a JWT token, or readable Base64,
/// returning the decoded `serde_json::Value` when applicable.
pub fn detect_and_decode_str(input: &str) -> Option<DecodedPayload> {
    let trimmed = input.trim();
    if trimmed.len() < 2 {
        return None;
    }

    // 1. Escaped JSON string
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
            return Some(DecodedPayload {
                kind: DecodedKind::EscapedJson,
                value: val,
            });
        }
    }

    // 2. JWT (header.payload.signature)
    if trimmed.starts_with("eyJ") {
        let parts: Vec<&str> = trimmed.split('.').collect();
        if parts.len() == 3 && !parts[0].is_empty() && !parts[1].is_empty() {
            if let (Some(h_bytes), Some(p_bytes)) = (
                decode_base64_bytes(parts[0], true),
                decode_base64_bytes(parts[1], true),
            ) {
                if let (Ok(header), Ok(payload)) = (
                    serde_json::from_slice::<Value>(&h_bytes),
                    serde_json::from_slice::<Value>(&p_bytes),
                ) {
                    if header.is_object() && payload.is_object() {
                        return Some(DecodedPayload {
                            kind: DecodedKind::Jwt,
                            value: json!({
                                "header": header,
                                "payload": payload,
                                "signature": parts[2],
                            }),
                        });
                    }
                }
            }
        }
    }

    // 3. Base64 (standard or URL-safe, at least 8 chars and multiple of 4 when padded)
    if trimmed.len() >= 8 && trimmed.len().is_multiple_of(4) && !trimmed.contains(' ') {
        let bytes = decode_base64_bytes(trimmed, false)
            .or_else(|| decode_base64_bytes(trimmed, true));
        if let Some(raw) = bytes {
            if let Ok(text) = std::str::from_utf8(&raw) {
                let is_readable = !text.is_empty()
                    && text
                        .chars()
                        .all(|c| !c.is_control() || c == '\n' || c == '\r' || c == '\t');
                if is_readable {
                    if let Ok(json_val) = serde_json::from_str::<Value>(text.trim()) {
                        if json_val.is_object() || json_val.is_array() {
                            return Some(DecodedPayload {
                                kind: DecodedKind::Base64,
                                value: json_val,
                            });
                        }
                    }
                    return Some(DecodedPayload {
                        kind: DecodedKind::Base64,
                        value: Value::String(text.to_string()),
                    });
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_escaped_json() {
        let res = detect_and_decode_str(r#"{"id":123,"ok":true}"#).unwrap();
        assert_eq!(res.kind, DecodedKind::EscapedJson);
        assert_eq!(res.value["id"], 123);
    }

    #[test]
    fn test_decode_jwt() {
        // {"alg":"HS256","typ":"JWT"}.{"sub":"123","name":"VarLens"}.sig
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjMiLCJuYW1lIjoiVmFyTGVucyJ9.signature123";
        let res = detect_and_decode_str(jwt).unwrap();
        assert_eq!(res.kind, DecodedKind::Jwt);
        assert_eq!(res.value["header"]["alg"], "HS256");
        assert_eq!(res.value["payload"]["name"], "VarLens");
    }

    #[test]
    fn test_decode_base64_text_and_json() {
        // "hello world!" -> aGVsbG8gd29ybGQh
        let res = detect_and_decode_str("aGVsbG8gd29ybGQh").unwrap();
        assert_eq!(res.kind, DecodedKind::Base64);
        assert_eq!(res.value, Value::String("hello world!".to_string()));
    }
}
