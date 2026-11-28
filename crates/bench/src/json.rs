//! A minimal JSON writer and reader, std only (no serde in this crate): the writer prints the
//! `--json` result of a run, the reader reads those results back for `culla-bench report`.
//!
//! The writer escapes every character outside printable ASCII as `\uXXXX`, so the output is
//! plain ASCII: a file names or volume labels from the card cannot hide or reorder text in a
//! terminal (threat model T6b), and the bytes are the same whatever encoding the shell's
//! redirection uses for ASCII.

use std::fmt::Write;

/// A JSON value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
    Null,
    Bool(bool),
    /// An integer (counts, bytes).
    Int(i128),
    /// A measured value (milliseconds, MB/s). Not finite is written as `null`.
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    /// An object, keys in this order.
    Obj(Vec<(String, Value)>),
}

impl Value {
    pub(crate) fn str(s: impl Into<String>) -> Self {
        Self::Str(s.into())
    }

    pub(crate) fn int(n: impl Into<i128>) -> Self {
        Self::Int(n.into())
    }

    /// An object from `(key, value)` pairs.
    pub(crate) fn obj<K: Into<String>>(fields: impl IntoIterator<Item = (K, Value)>) -> Self {
        Self::Obj(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    /// `null` for `None`.
    pub(crate) fn opt<T>(v: Option<T>, f: impl FnOnce(T) -> Value) -> Self {
        v.map_or(Self::Null, f)
    }

    /// The field `key` of an object.
    pub(crate) fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Self::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// A number as `f64` (an integer too).
    #[allow(
        clippy::cast_precision_loss,
        reason = "measured values, far below 2^53"
    )]
    pub(crate) fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Num(n) => Some(*n),
            Self::Int(n) => Some(*n as f64),
            _ => None,
        }
    }

    pub(crate) fn as_arr(&self) -> Option<&[Value]> {
        match self {
            Self::Arr(items) => Some(items),
            _ => None,
        }
    }

    /// Pretty-printed with two-space indents, without a trailing newline.
    #[must_use]
    pub(crate) fn to_pretty(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out
    }

    /// On one line, without spaces (the `report` grouping key).
    #[must_use]
    pub(crate) fn to_compact(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, usize::MAX);
        out
    }

    fn write(&self, out: &mut String, depth: usize) {
        let compact = depth == usize::MAX;
        let next = if compact { depth } else { depth + 1 };
        let open = |out: &mut String, first: bool, d: usize| {
            if !first {
                out.push(',');
            }
            if !compact {
                out.push('\n');
                indent(out, d);
            }
        };
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Self::Int(n) => {
                let _ = write!(out, "{n}");
            }
            Self::Num(n) if n.is_finite() => {
                // Three decimals: microseconds for milliseconds; enough for MB/s.
                let s = format!("{n:.3}");
                let s = s.trim_end_matches('0').trim_end_matches('.');
                out.push_str(if s == "-0" { "0" } else { s });
            }
            Self::Num(_) => out.push_str("null"),
            Self::Str(s) => escape_into(s, out),
            Self::Arr(items) if items.is_empty() => out.push_str("[]"),
            Self::Obj(fields) if fields.is_empty() => out.push_str("{}"),
            Self::Arr(items) => {
                out.push('[');
                for (i, v) in items.iter().enumerate() {
                    open(out, i == 0, next);
                    v.write(out, next);
                }
                if !compact {
                    out.push('\n');
                    indent(out, depth);
                }
                out.push(']');
            }
            Self::Obj(fields) => {
                out.push('{');
                for (i, (k, v)) in fields.iter().enumerate() {
                    open(out, i == 0, next);
                    escape_into(k, out);
                    out.push_str(if compact { ":" } else { ": " });
                    v.write(out, next);
                }
                if !compact {
                    out.push('\n');
                    indent(out, depth);
                }
                out.push('}');
            }
        }
    }
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

/// `s` as a JSON string of printable ASCII: everything else as `\uXXXX` (UTF-16 surrogate
/// pairs above U+FFFF).
fn escape_into(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ' '..='~' => out.push(c),
            c => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{u:04x}");
                }
            }
        }
    }
    out.push('"');
}

/// Nesting deeper than this is refused (the results are 4 levels deep).
const MAX_DEPTH: usize = 64;

/// Parses one JSON document.
///
/// # Errors
///
/// A message with the byte offset when `text` is not valid JSON or nests too deep.
pub(crate) fn parse(text: &str) -> Result<Value, String> {
    let mut p = Parser {
        s: text.as_bytes(),
        i: 0,
    };
    // A UTF-8 byte order mark, as some editors and shells write.
    if p.s.starts_with(&[0xEF, 0xBB, 0xBF]) {
        p.i = 3;
    }
    let v = p.value(0)?;
    p.ws();
    if p.i != p.s.len() {
        return Err(p.err("text after the value"));
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn err(&self, what: &str) -> String {
        format!("{what} at byte {}", self.i)
    }

    fn ws(&mut self) {
        while self
            .s
            .get(self.i)
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
        {
            self.i += 1;
        }
    }

    fn eat(&mut self, lit: &[u8]) -> bool {
        if self.s[self.i..].starts_with(lit) {
            self.i += lit.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err(self.err("nested too deep"));
        }
        self.ws();
        match self.s.get(self.i) {
            None => Err(self.err("unexpected end")),
            Some(b'n') if self.eat(b"null") => Ok(Value::Null),
            Some(b't') if self.eat(b"true") => Ok(Value::Bool(true)),
            Some(b'f') if self.eat(b"false") => Ok(Value::Bool(false)),
            Some(b'"') => self.string().map(Value::Str),
            Some(b'[') => {
                self.i += 1;
                let mut items = Vec::new();
                self.ws();
                if self.eat(b"]") {
                    return Ok(Value::Arr(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    self.ws();
                    if self.eat(b"]") {
                        return Ok(Value::Arr(items));
                    }
                    if !self.eat(b",") {
                        return Err(self.err("expected , or ]"));
                    }
                }
            }
            Some(b'{') => {
                self.i += 1;
                let mut fields = Vec::new();
                self.ws();
                if self.eat(b"}") {
                    return Ok(Value::Obj(fields));
                }
                loop {
                    self.ws();
                    if self.s.get(self.i) != Some(&b'"') {
                        return Err(self.err("expected a key"));
                    }
                    let k = self.string()?;
                    self.ws();
                    if !self.eat(b":") {
                        return Err(self.err("expected :"));
                    }
                    fields.push((k, self.value(depth + 1)?));
                    self.ws();
                    if self.eat(b"}") {
                        return Ok(Value::Obj(fields));
                    }
                    if !self.eat(b",") {
                        return Err(self.err("expected , or }"));
                    }
                }
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.err("unexpected character")),
        }
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.i;
        while self
            .s
            .get(self.i)
            .is_some_and(|b| matches!(b, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'))
        {
            self.i += 1;
        }
        let text = std::str::from_utf8(&self.s[start..self.i]).map_err(|_| self.err("number"))?;
        if let Ok(n) = text.parse::<i128>() {
            return Ok(Value::Int(n));
        }
        text.parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map(Value::Num)
            .ok_or_else(|| self.err("invalid number"))
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let h = self
            .s
            .get(self.i..self.i + 4)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .ok_or_else(|| self.err("invalid \\u escape"))?;
        self.i += 4;
        Ok(h)
    }

    fn string(&mut self) -> Result<String, String> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let start = self.i;
            while self
                .s
                .get(self.i)
                .is_some_and(|b| *b != b'"' && *b != b'\\' && *b >= 0x20)
            {
                self.i += 1;
            }
            let run =
                std::str::from_utf8(&self.s[start..self.i]).map_err(|_| self.err("not UTF-8"))?;
            out.push_str(run);
            match self.s.get(self.i) {
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.i += 1;
                    let Some(&e) = self.s.get(self.i) else {
                        return Err(self.err("unexpected end"));
                    };
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            let c = if (0xD800..0xDC00).contains(&hi) && self.eat(b"\\u") {
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return Err(self.err("invalid surrogate pair"));
                                }
                                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                            } else {
                                hi
                            };
                            out.push(char::from_u32(c).ok_or_else(|| self.err("lone surrogate"))?);
                        }
                        _ => return Err(self.err("invalid escape")),
                    }
                }
                _ => return Err(self.err("unterminated string")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_are_ascii_only() {
        let v = Value::str("a\"b\\c\nd\u{0}\u{7f}é\u{202E}📷");
        assert_eq!(
            v.to_pretty(),
            "\"a\\\"b\\\\c\\nd\\u0000\\u007f\\u00e9\\u202e\\ud83d\\udcf7\""
        );
    }

    #[test]
    fn numbers_are_trimmed() {
        assert_eq!(Value::Num(1.5).to_pretty(), "1.5");
        assert_eq!(Value::Num(2.0).to_pretty(), "2");
        assert_eq!(Value::Num(0.000_4).to_pretty(), "0");
        assert_eq!(Value::Num(-0.000_4).to_pretty(), "0");
        assert_eq!(Value::Num(12.345_67).to_pretty(), "12.346");
        assert_eq!(Value::Num(f64::NAN).to_pretty(), "null");
        assert_eq!(Value::int(u64::MAX).to_pretty(), "18446744073709551615");
    }

    #[test]
    fn pretty_and_compact() {
        let v = Value::obj([
            ("a", Value::int(-1)),
            ("b", Value::Arr(vec![Value::Bool(true), Value::Null])),
            ("c", Value::Arr(vec![])),
            ("d", Value::obj(Vec::<(String, Value)>::new())),
        ]);
        assert_eq!(
            v.to_pretty(),
            "{\n  \"a\": -1,\n  \"b\": [\n    true,\n    null\n  ],\n  \"c\": [],\n  \"d\": {}\n}"
        );
        assert_eq!(
            v.to_compact(),
            "{\"a\":-1,\"b\":[true,null],\"c\":[],\"d\":{}}"
        );
    }

    #[test]
    fn what_is_written_reads_back() {
        let v = Value::obj([
            ("schema", Value::str("unstable")),
            ("s", Value::str("é📷\"\\\n\u{1}")),
            ("n", Value::Num(12.5)),
            ("i", Value::int(42)),
            (
                "a",
                Value::Arr(vec![Value::Num(0.25), Value::Null, Value::Bool(false)]),
            ),
        ]);
        assert_eq!(parse(&v.to_pretty()).unwrap(), v);
        assert_eq!(parse(&v.to_compact()).unwrap(), v);
    }

    #[test]
    fn reads_other_json() {
        assert!(parse("").unwrap_err().contains("unexpected end"));
        let v = parse(" { \"x\" : [ 1e3 , -2.5E-1, \"\\/\\u00e9\" ] } ").unwrap();
        let x = v.get("x").unwrap().as_arr().unwrap();
        assert_eq!(x[0].as_f64(), Some(1000.0));
        assert_eq!(x[1].as_f64(), Some(-0.25));
        assert_eq!(x[2].as_str(), Some("/é"));
        // A UTF-8 byte order mark is skipped.
        assert_eq!(parse("\u{feff}null").unwrap(), Value::Null);
    }

    #[test]
    fn refuses_invalid_json() {
        for bad in [
            "",
            "{",
            "[1,]",
            "{\"a\" 1}",
            "\"abc",
            "nul",
            "1 2",
            "\"\\x\"",
            "\"\\ud800\"",
            "--1",
            "{1:2}",
            "\"a\u{1}\"",
        ] {
            assert!(parse(bad).is_err(), "{bad:?} parsed");
        }
        let deep = "[".repeat(MAX_DEPTH + 2) + &"]".repeat(MAX_DEPTH + 2);
        assert!(parse(&deep).unwrap_err().contains("deep"));
    }
}
