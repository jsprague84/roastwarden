//! Python literals, as Artisan stores an `.alog` (the `repr()` of a dict): a parser for what
//! `ast.literal_eval` accepts, and a writer whose output it accepts back.

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Map, Value};
use std::fmt::Write as _;

/// Parse an `.alog` body: JSON first (Artisan can export JSON, and older
/// Roastwarden exports were JSON), then a Python literal.
pub fn parse_alog(content: &str) -> Result<Value> {
    let trimmed = content.trim_start_matches('\u{feff}').trim();
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return Ok(v);
    }
    parse_python_literal(trimmed).context("file is neither JSON nor a Python literal (.alog)")
}

/// Parse a Python literal (dict/list/tuple/str/int/float/bool/None) into JSON.
/// Tuples become arrays, `inf`/`nan` become `null`, dict keys are stringified.
pub fn parse_python_literal(src: &str) -> Result<Value> {
    let mut p = PyParser {
        s: src.as_bytes(),
        i: 0,
        depth: 0,
    };
    p.skip_ws();
    let v = p.value()?;
    p.skip_ws();
    if p.i != p.s.len() {
        bail!("unexpected trailing data at byte {}", p.i);
    }
    Ok(v)
}

/// How deep lists, tuples and dicts may nest. An `.alog` nests three or four deep; the limit
/// keeps a hostile file from overflowing the stack (`serde_json` stops at 128).
pub(super) const MAX_DEPTH: usize = 64;

struct PyParser<'a> {
    s: &'a [u8],
    i: usize,
    depth: usize,
}

impl PyParser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn peek_at(&self, off: usize) -> Option<u8> {
        self.s.get(self.i + off).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() {
                self.i += 1;
            } else if c == b'#' {
                while let Some(c) = self.peek() {
                    self.i += 1;
                    if c == b'\n' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    fn expect(&mut self, c: u8) -> Result<()> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            bail!(
                "expected '{}' at byte {} but found {:?}",
                c as char,
                self.i,
                self.peek().map(|b| b as char)
            )
        }
    }

    fn starts_with(&self, word: &str) -> bool {
        self.s[self.i..].starts_with(word.as_bytes())
    }

    fn value(&mut self) -> Result<Value> {
        if matches!(self.peek(), Some(b'{' | b'[' | b'(')) {
            if self.depth >= MAX_DEPTH {
                bail!("nested more than {MAX_DEPTH} deep at byte {}", self.i);
            }
            self.depth += 1;
            let v = match self.peek() {
                Some(b'{') => self.dict(),
                Some(b'[') => self.seq(b']'),
                _ => self.seq(b')'),
            };
            self.depth -= 1;
            return v;
        }
        match self.peek() {
            None => bail!("unexpected end of input"),
            Some(b'\'' | b'"') => self.string().map(Value::String),
            Some(b'u' | b'U' | b'b' | b'B' | b'r' | b'R')
                if matches!(self.peek_at(1), Some(b'\'' | b'"')) =>
            {
                self.i += 1;
                self.string().map(Value::String)
            }
            Some(b'T') if self.starts_with("True") => {
                self.i += 4;
                Ok(Value::Bool(true))
            }
            Some(b'F') if self.starts_with("False") => {
                self.i += 5;
                Ok(Value::Bool(false))
            }
            Some(b'N') if self.starts_with("None") => {
                self.i += 4;
                Ok(Value::Null)
            }
            Some(b'i') if self.starts_with("inf") => {
                self.i += 3;
                Ok(Value::Null)
            }
            Some(b'n') if self.starts_with("nan") => {
                self.i += 3;
                Ok(Value::Null)
            }
            Some(c) if c == b'-' || c == b'+' || c == b'.' || c.is_ascii_digit() => self.number(),
            Some(c) => bail!("unexpected character '{}' at byte {}", c as char, self.i),
        }
    }

    fn dict(&mut self) -> Result<Value> {
        self.expect(b'{')?;
        let mut map = Map::new();
        loop {
            self.skip_ws();
            if self.peek() == Some(b'}') {
                self.i += 1;
                return Ok(Value::Object(map));
            }
            let key = self.value()?;
            let key = match key {
                Value::String(s) => s,
                other => other.to_string(),
            };
            self.skip_ws();
            self.expect(b':')?;
            self.skip_ws();
            let val = self.value()?;
            map.insert(key, val);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {}
                _ => bail!("expected ',' or '}}' at byte {}", self.i),
            }
        }
    }

    fn seq(&mut self, close: u8) -> Result<Value> {
        self.i += 1; // opening bracket
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() == Some(close) {
                self.i += 1;
                return Ok(Value::Array(items));
            }
            items.push(self.value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(c) if c == close => {}
                _ => bail!("expected ',' or '{}' at byte {}", close as char, self.i),
            }
        }
    }

    fn string(&mut self) -> Result<String> {
        let quote = self.peek().ok_or_else(|| anyhow!("expected string"))?;
        let triple = self.peek_at(1) == Some(quote) && self.peek_at(2) == Some(quote);
        self.i += if triple { 3 } else { 1 };
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = self
                .peek()
                .ok_or_else(|| anyhow!("unterminated string literal"))?;
            if c == quote {
                if triple {
                    if self.peek_at(1) == Some(quote) && self.peek_at(2) == Some(quote) {
                        self.i += 3;
                        break;
                    }
                    out.push(c);
                    self.i += 1;
                    continue;
                }
                self.i += 1;
                break;
            }
            if c == b'\\' {
                self.i += 1;
                let e = self
                    .peek()
                    .ok_or_else(|| anyhow!("unterminated escape sequence"))?;
                self.i += 1;
                match e {
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'b' => out.push(0x08),
                    b'f' => out.push(0x0c),
                    b'v' => out.push(0x0b),
                    b'a' => out.push(0x07),
                    b'0' => out.push(0),
                    b'\\' => out.push(b'\\'),
                    b'\'' => out.push(b'\''),
                    b'"' => out.push(b'"'),
                    b'\n' => {}
                    b'x' => {
                        let code = self.hex_digits(2)?;
                        push_char(&mut out, code);
                    }
                    b'u' => {
                        let code = self.hex_digits(4)?;
                        push_char(&mut out, code);
                    }
                    b'U' => {
                        let code = self.hex_digits(8)?;
                        push_char(&mut out, code);
                    }
                    other => {
                        out.push(b'\\');
                        out.push(other);
                    }
                }
                continue;
            }
            out.push(c);
            self.i += 1;
        }
        String::from_utf8(out).map_err(|e| anyhow!("invalid UTF-8 in string literal: {e}"))
    }

    fn hex_digits(&mut self, n: usize) -> Result<u32> {
        let end = self.i + n;
        let slice = self
            .s
            .get(self.i..end)
            .ok_or_else(|| anyhow!("truncated escape sequence"))?;
        let text = std::str::from_utf8(slice)?;
        let code = u32::from_str_radix(text, 16)?;
        self.i = end;
        Ok(code)
    }

    fn number(&mut self) -> Result<Value> {
        let start = self.i;
        if matches!(self.peek(), Some(b'-' | b'+')) {
            self.i += 1;
        }
        if self.starts_with("inf") {
            self.i += 3;
            return Ok(Value::Null);
        }
        if self.starts_with("nan") {
            self.i += 3;
            return Ok(Value::Null);
        }
        while let Some(c) = self.peek() {
            let exponent_sign =
                (c == b'-' || c == b'+') && matches!(self.s.get(self.i - 1), Some(b'e' | b'E'));
            if c.is_ascii_alphanumeric() || c == b'.' || c == b'_' || exponent_sign {
                self.i += 1;
            } else {
                break;
            }
        }
        let text: String = std::str::from_utf8(&self.s[start..self.i])?
            .chars()
            .filter(|c| *c != '_')
            .collect();
        let text = text.trim_start_matches('+');
        if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            return Ok(json!(i64::from_str_radix(hex, 16)?));
        }
        if !text.contains(['.', 'e', 'E'])
            && let Ok(i) = text.parse::<i64>() {
                return Ok(json!(i));
            }
        let f: f64 = text
            .parse()
            .map_err(|_| anyhow!("invalid number '{text}' at byte {start}"))?;
        Ok(serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number))
    }
}

fn push_char(out: &mut Vec<u8>, code: u32) {
    if let Some(ch) = char::from_u32(code) {
        let mut buf = [0u8; 4];
        out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
    }
}

/// Render JSON as a Python literal that `ast.literal_eval` accepts.
pub fn to_python_literal(v: &Value) -> String {
    let mut out = String::new();
    write_py(v, &mut out);
    out
}

fn write_py(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("None"),
        Value::Bool(true) => out.push_str("True"),
        Value::Bool(false) => out.push_str("False"),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                out.push_str(&i.to_string());
            } else if let Some(u) = n.as_u64() {
                out.push_str(&u.to_string());
            } else if let Some(f) = n.as_f64() {
                if f.is_finite() {
                    let s = format!("{f}");
                    out.push_str(&s);
                    if !s.contains(['.', 'e', 'E']) {
                        out.push_str(".0");
                    }
                } else {
                    out.push_str("None");
                }
            } else {
                out.push_str("None");
            }
        }
        Value::String(s) => write_py_string(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_py(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (i, (k, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_py_string(k, out);
                out.push_str(": ");
                write_py(item, out);
            }
            out.push('}');
        }
    }
}

fn write_py_string(s: &str, out: &mut String) {
    out.push('\'');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('\'');
}
