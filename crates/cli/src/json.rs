//! A minimal JSON writer for `--json` (no serde in this crate): values built in memory and
//! printed with two-space indents, keys in the order given.

use std::fmt::Write;

use crate::text;

/// A JSON value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// An integer (every number the CLI prints is one).
    Int(i128),
    /// A string, escaped when written.
    Str(String),
    /// An array.
    Arr(Vec<Value>),
    /// An object, keys in this order.
    Obj(Vec<(&'static str, Value)>),
}

impl Value {
    /// A string value.
    pub(crate) fn str(s: impl Into<String>) -> Self {
        Self::Str(s.into())
    }

    /// An integer value.
    pub(crate) fn int(n: impl Into<i128>) -> Self {
        Self::Int(n.into())
    }

    /// `null` for `None`, else the value `f` makes.
    pub(crate) fn opt<T>(v: Option<T>, f: impl FnOnce(T) -> Value) -> Self {
        v.map_or(Self::Null, f)
    }

    /// The value as pretty-printed JSON text, without a trailing newline.
    #[must_use]
    pub(crate) fn to_pretty(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out
    }

    fn write(&self, out: &mut String, depth: usize) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Self::Int(n) => {
                let _ = write!(out, "{n}");
            }
            Self::Str(s) => escape_into(s, out),
            Self::Arr(items) if items.is_empty() => out.push_str("[]"),
            Self::Obj(fields) if fields.is_empty() => out.push_str("{}"),
            Self::Arr(items) => {
                out.push('[');
                for (i, v) in items.iter().enumerate() {
                    out.push_str(if i == 0 { "\n" } else { ",\n" });
                    indent(out, depth + 1);
                    v.write(out, depth + 1);
                }
                out.push('\n');
                indent(out, depth);
                out.push(']');
            }
            Self::Obj(fields) => {
                out.push('{');
                for (i, (k, v)) in fields.iter().enumerate() {
                    out.push_str(if i == 0 { "\n" } else { ",\n" });
                    indent(out, depth + 1);
                    escape_into(k, out);
                    out.push_str(": ");
                    v.write(out, depth + 1);
                }
                out.push('\n');
                indent(out, depth);
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

/// Writes `s` as a JSON string: quotes, backslashes and every character that
/// [`text::needs_escape`] lists escaped as `\uXXXX` (UTF-16 surrogate pairs above U+FFFF):
/// control characters, bidi controls, zero-width and other invisible characters, the BOM and
/// U+2028 and U+2029 (line ends for JavaScript before ES2019). File names and EXIF text come
/// from the card, so a raw `--json` printed to a terminal shows no hidden or reordered text
/// (threat model T6b). Everything else is written as UTF-8.
fn escape_into(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if text::needs_escape(c) => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{u:04x}");
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_are_escaped() {
        let v = Value::str("a\"b\\c\nd\re\tf\u{8}\u{c}\u{0}\u{1f}\u{7f}\u{9b}\u{2028}\u{2029}é📷");
        assert_eq!(
            v.to_pretty(),
            "\"a\\\"b\\\\c\\nd\\re\\tf\\b\\f\\u0000\\u001f\\u007f\\u009b\\u2028\\u2029é📷\""
        );
        assert_eq!(Value::str("").to_pretty(), "\"\"");
    }

    #[test]
    fn invisible_and_bidi_characters_are_escaped() {
        let v = Value::str("a\u{202E}b\u{200B}\u{FEFF}\u{2066}\u{AD}\u{61C}\u{180E}\u{E0041}c");
        assert_eq!(
            v.to_pretty(),
            "\"a\\u202eb\\u200b\\ufeff\\u2066\\u00ad\\u061c\\u180e\\udb40\\udc41c\""
        );
    }

    #[test]
    fn nested_values_are_indented() {
        let v = Value::Obj(vec![
            ("a", Value::int(-1)),
            ("b", Value::Arr(vec![Value::Bool(true), Value::Null])),
            ("c", Value::Arr(vec![])),
            ("d", Value::Obj(vec![])),
            ("e", Value::opt(Some(u64::MAX), Value::int)),
            ("f", Value::opt(None::<u8>, Value::int)),
        ]);
        assert_eq!(
            v.to_pretty(),
            "{\n  \"a\": -1,\n  \"b\": [\n    true,\n    null\n  ],\n  \"c\": [],\n  \"d\": {},\n  \
             \"e\": 18446744073709551615,\n  \"f\": null\n}"
        );
    }
}
