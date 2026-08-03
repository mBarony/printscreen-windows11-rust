//! JSON mínimo para o `config.json` (substitui `serde`/`serde_json`).
//!
//! Cobre o subconjunto que o arquivo de configuração usa: objetos, arrays,
//! strings (com escapes, incluindo `\uXXXX` e pares substitutos), números,
//! booleanos e null. Leitura tolerante por construção: campos desconhecidos
//! são ignorados e campos ausentes assumem padrão no chamador.

use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    /// Campo de objeto (None quando não é objeto ou não existe).
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(map) => map.get(key),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

pub fn parse(text: &str) -> Result<Value, String> {
    let mut p = Parser { bytes: text.as_bytes(), pos: 0 };
    p.skip_ws();
    let value = p.value()?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(format!("conteúdo inesperado no byte {}", p.pos));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn bump(&mut self) -> Result<u8, String> {
        let b = self.peek().ok_or_else(|| "fim inesperado".to_string())?;
        self.pos += 1;
        Ok(b)
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, b: u8) -> Result<(), String> {
        let got = self.bump()?;
        if got != b {
            return Err(format!(
                "esperava {:?} no byte {}, obtido {:?}",
                b as char,
                self.pos - 1,
                got as char
            ));
        }
        Ok(())
    }

    fn value(&mut self) -> Result<Value, String> {
        self.skip_ws();
        match self.peek().ok_or("fim inesperado")? {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => Ok(Value::String(self.string()?)),
            b't' => self.literal("true", Value::Bool(true)),
            b'f' => self.literal("false", Value::Bool(false)),
            b'n' => self.literal("null", Value::Null),
            b'-' | b'0'..=b'9' => self.number(),
            other => Err(format!("caractere inesperado {:?} no byte {}", other as char, self.pos)),
        }
    }

    fn literal(&mut self, text: &str, value: Value) -> Result<Value, String> {
        if self.bytes[self.pos..].starts_with(text.as_bytes()) {
            self.pos += text.len();
            Ok(value)
        } else {
            Err(format!("literal inválido no byte {}", self.pos))
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.expect(b'{')?;
        let mut map = BTreeMap::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Value::Object(map));
        }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.skip_ws();
            self.expect(b':')?;
            let value = self.value()?;
            map.insert(key, value);
            self.skip_ws();
            match self.bump()? {
                b',' => continue,
                b'}' => return Ok(Value::Object(map)),
                other => {
                    return Err(format!(
                        "esperava ',' ou '}}', obtido {:?} no byte {}",
                        other as char,
                        self.pos - 1
                    ))
                }
            }
        }
    }

    fn array(&mut self) -> Result<Value, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Value::Array(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_ws();
            match self.bump()? {
                b',' => continue,
                b']' => return Ok(Value::Array(items)),
                other => {
                    return Err(format!(
                        "esperava ',' ou ']', obtido {:?} no byte {}",
                        other as char,
                        self.pos - 1
                    ))
                }
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            match self.bump()? {
                b'"' => return Ok(out),
                b'\\' => match self.bump()? {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{0008}'),
                    b'f' => out.push('\u{000C}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let first = self.hex4()?;
                        let code = if (0xD800..=0xDBFF).contains(&first) {
                            // Par substituto UTF-16.
                            self.expect(b'\\')?;
                            self.expect(b'u')?;
                            let second = self.hex4()?;
                            if !(0xDC00..=0xDFFF).contains(&second) {
                                return Err("par substituto inválido".into());
                            }
                            0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)
                        } else {
                            first
                        };
                        out.push(
                            char::from_u32(code).ok_or("código Unicode inválido")?,
                        );
                    }
                    other => return Err(format!("escape inválido: \\{}", other as char)),
                },
                // UTF-8 multibyte: copia os bytes crus da string original.
                b if b >= 0x80 => {
                    let start = self.pos - 1;
                    let len = match b {
                        0xC0..=0xDF => 2,
                        0xE0..=0xEF => 3,
                        0xF0..=0xF7 => 4,
                        _ => return Err("UTF-8 inválido".into()),
                    };
                    if start + len > self.bytes.len() {
                        return Err("UTF-8 truncado".into());
                    }
                    let chunk = std::str::from_utf8(&self.bytes[start..start + len])
                        .map_err(|_| "UTF-8 inválido".to_string())?;
                    out.push_str(chunk);
                    self.pos = start + len;
                }
                b if b < 0x20 => return Err("caractere de controle em string".into()),
                b => out.push(b as char),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let mut code = 0u32;
        for _ in 0..4 {
            let b = self.bump()?;
            let digit = (b as char).to_digit(16).ok_or("dígito hex inválido")?;
            code = code * 16 + digit;
        }
        Ok(code)
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).expect("ASCII");
        text.parse::<f64>()
            .map(Value::Number)
            .map_err(|_| format!("número inválido: {text}"))
    }
}

// ---------------------------------------------------------------------------
// Escrita (pretty, 2 espaços — mesmo formato do serde_json pretty)
// ---------------------------------------------------------------------------

pub fn to_string_pretty(value: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, value, 0);
    out
}

fn write_value(out: &mut String, value: &Value, indent: usize) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                let _ = write!(out, "{}", *n as i64);
            } else {
                let _ = write!(out, "{n}");
            }
        }
        Value::String(s) => write_string(out, s),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('\n');
                push_indent(out, indent + 1);
                write_value(out, item, indent + 1);
            }
            out.push('\n');
            push_indent(out, indent);
            out.push(']');
        }
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (key, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('\n');
                push_indent(out, indent + 1);
                write_string(out, key);
                out.push_str(": ");
                write_value(out, item, indent + 1);
            }
            out.push('\n');
            push_indent(out, indent);
            out.push('}');
        }
    }
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn push_indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

// Construtores convenientes para a serialização do config.
pub fn obj(entries: Vec<(&str, Value)>) -> Value {
    Value::Object(entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

pub fn s(text: &str) -> Value {
    Value::String(text.to_string())
}

pub fn n(number: f64) -> Value {
    Value::Number(number)
}

pub fn b(flag: bool) -> Value {
    Value::Bool(flag)
}

pub fn arr(items: Vec<Value>) -> Value {
    Value::Array(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_config_like() {
        let text = r#"{
  "hotkeys": { "fullscreen": { "modifiers": ["CTRL"], "code": "PrintScreen" } },
  "stroke": 3.5, "count": 24, "on": true, "nada": null,
  "path": "C:\\Users\\voc\u00ea\\Imagens", "acentuação": "çãé✓"
}"#;
        let v = parse(text).unwrap();
        assert_eq!(
            v.get("hotkeys").unwrap().get("fullscreen").unwrap().get("code").unwrap().as_str(),
            Some("PrintScreen")
        );
        assert_eq!(v.get("stroke").unwrap().as_f64(), Some(3.5));
        assert_eq!(v.get("count").unwrap().as_f64(), Some(24.0));
        assert_eq!(v.get("on").unwrap().as_bool(), Some(true));
        assert_eq!(v.get("path").unwrap().as_str(), Some("C:\\Users\\você\\Imagens"));
        assert_eq!(v.get("acentuação").unwrap().as_str(), Some("çãé✓"));

        // Reescreve e re-parseia: estável.
        let text2 = to_string_pretty(&v);
        assert_eq!(parse(&text2).unwrap(), v);
    }

    #[test]
    fn escapes_and_surrogates() {
        let v = parse(r#"{"x": "a\n\t\"\\ \u0041 \ud83d\ude00"}"#).unwrap();
        assert_eq!(v.get("x").unwrap().as_str(), Some("a\n\t\"\\ A 😀"));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("{ banana }").is_err());
        assert!(parse("{}extra").is_err());
        assert!(parse("").is_err());
    }

    #[test]
    fn integers_written_without_decimal() {
        let v = obj(vec![("a", n(3.0)), ("b", n(3.5))]);
        let text = to_string_pretty(&v);
        assert!(text.contains("\"a\": 3"), "{text}");
        assert!(text.contains("\"b\": 3.5"), "{text}");
    }
}
