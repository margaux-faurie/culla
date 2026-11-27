//! Text form of the decision log and of its fold, as plain data: the format of the golden
//! fixtures in `testdata/event-log/` shared by the Rust [`fold`](super::fold) and the TS reducer
//! (docs/testing.md §3.3, INV-15). Hand-written: the core has no dependencies.
//!
//! **A log** is JSON Lines: one event per line, every line ending with `\n`, an empty log being
//! the empty text. An event is a flat JSON object whose keys are the columns of the
//! `decision_event` table (design/file-engine.md §2.3):
//!
//! ```text
//! {"seq":1,"ts":1764201600000,"cause":"user","type":"decided","group_id":7,"verdict":"keep"}
//! {"seq":2,"ts":1764201601000,"cause":"user","type":"cleared","group_id":7}
//! {"seq":3,"ts":1764201602000,"cause":"user","type":"undo","target_seq":2}
//! {"seq":4,"ts":1764201603000,"cause":"reconcile","type":"cleared","group_id":7}
//! ```
//!
//! `seq`, `ts`, `cause` and `type` are always present; `group_id` with `decided` and `cleared`,
//! `verdict` (`keep`, `reject`) with `decided`, `target_seq` with `undo` and `redo`, and no other
//! key. Numbers are integers within ±2^53 (so a JS number holds them exactly). [`parse_log`]
//! accepts keys in any order and JSON whitespace; strings in this schema never need escapes, so
//! a string with a backslash is refused. [`render_log`] writes the keys in the order above with
//! no spaces.
//!
//! **A fold** (the `.expected.json` of a case) is a JSON object, written by [`render_fold`]
//! exactly as below (two-space indent, a final newline):
//!
//! ```text
//! {
//!   "state": {
//!     "7": "kept",
//!     "9": "rejected"
//!   },
//!   "done": [1, 4],
//!   "undone": [3],
//!   "corrupt": {"seq": 5, "reason": "decided_on_decided"}
//! }
//! ```
//!
//! `state` maps each decided group id (as a string, in numeric order) to `kept` or `rejected`;
//! undecided groups are absent, and an empty map is written `{}`. `done` and `undone` are the
//! stacks, bottom first. `corrupt` is present only when the fold stopped at a bad event
//! ([`CorruptReason::as_str`] names the reason); `state`, `done` and `undone` are then the fold
//! of the events before it.

use std::fmt;
use std::fmt::Write as _;

use super::{Cause, CorruptLog, Decisions, Event, EventBody, Seq, Verdict};

/// Largest magnitude of a number in the text form: a JS number holds every integer up to it.
pub const MAX_SAFE_INTEGER: i64 = (1 << 53) - 1;

/// One event as a line, without the newline.
pub fn render_event(e: &Event) -> String {
    let mut s = format!(
        "{{\"seq\":{},\"ts\":{},\"cause\":\"{}\",\"type\":\"{}\"",
        e.seq,
        e.ts,
        e.cause.as_str(),
        e.body.type_str()
    );
    match e.body {
        EventBody::Decided { group, verdict } => {
            let _ = write!(
                s,
                ",\"group_id\":{group},\"verdict\":\"{}\"",
                verdict.as_str()
            );
        }
        EventBody::Cleared { group } => {
            let _ = write!(s, ",\"group_id\":{group}");
        }
        EventBody::Undo { target } | EventBody::Redo { target } => {
            let _ = write!(s, ",\"target_seq\":{target}");
        }
    }
    s.push('}');
    s
}

/// A whole log, one line per event, each ending with `\n`.
pub fn render_log<'a>(events: impl IntoIterator<Item = &'a Event>) -> String {
    let mut s = String::new();
    for e in events {
        s.push_str(&render_event(e));
        s.push('\n');
    }
    s
}

/// The fold result in the `.expected.json` form.
pub fn render_fold(result: &Result<Decisions, CorruptLog>) -> String {
    let (d, corrupt) = match result {
        Ok(d) => (d, None),
        Err(c) => (c.before.as_ref(), Some(c.event)),
    };
    let mut s = String::from("{\n  \"state\": {");
    let mut first = true;
    for (g, st) in d.decided() {
        s.push_str(if first { "\n" } else { ",\n" });
        first = false;
        let _ = write!(s, "    \"{g}\": \"{}\"", st.as_str());
    }
    s.push_str(if first { "},\n" } else { "\n  },\n" });
    let list = |xs: &[Seq]| {
        xs.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let _ = writeln!(s, "  \"done\": [{}],", list(d.done()));
    let _ = write!(s, "  \"undone\": [{}]", list(d.undone()));
    if let Some(e) = corrupt {
        let _ = write!(
            s,
            ",\n  \"corrupt\": {{\"seq\": {}, \"reason\": \"{}\"}}",
            e.seq,
            e.reason.as_str()
        );
    }
    s.push_str("\n}\n");
    s
}

/// Why a line is not an event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// Not a flat JSON object of strings and integers; the byte offset in the line.
    Syntax {
        /// Byte offset in the line.
        at: usize,
        /// What was expected.
        expected: &'static str,
    },
    /// A key that is not in the schema, or not allowed with this event type.
    UnknownKey(String),
    /// A key given twice.
    DuplicateKey(String),
    /// A required key is missing.
    MissingKey(&'static str),
    /// A key has a value of the wrong kind or out of range.
    BadValue(&'static str),
    /// The log does not end with a newline.
    NoFinalNewline,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax { at, expected } => write!(f, "at byte {at}: expected {expected}"),
            Self::UnknownKey(k) => write!(f, "unexpected key {k:?}"),
            Self::DuplicateKey(k) => write!(f, "key {k:?} given twice"),
            Self::MissingKey(k) => write!(f, "missing key {k:?}"),
            Self::BadValue(k) => write!(f, "bad value for {k:?}"),
            Self::NoFinalNewline => f.write_str("does not end with a newline"),
        }
    }
}

impl std::error::Error for ParseError {}

/// A line of a log that is not an event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogError {
    /// 1-based line number.
    pub line: usize,
    /// What is wrong with it.
    pub error: ParseError,
}

impl fmt::Display for LogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.error)
    }
}

impl std::error::Error for LogError {}

/// Parses a whole log. Every line, the last included, must end with `\n`; an empty line is an
/// error. It does not fold: a parsed log may still be corrupt.
pub fn parse_log(text: &str) -> Result<Vec<Event>, LogError> {
    let mut events = Vec::new();
    if text.is_empty() {
        return Ok(events);
    }
    let Some(body) = text.strip_suffix('\n') else {
        let line = text.split('\n').count();
        return Err(LogError {
            line,
            error: ParseError::NoFinalNewline,
        });
    };
    for (i, line) in body.split('\n').enumerate() {
        let e = parse_event(line).map_err(|error| LogError { line: i + 1, error })?;
        events.push(e);
    }
    Ok(events)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Scalar {
    Int(i64),
    Str(String),
}

const KEYS: [&str; 7] = [
    "seq",
    "ts",
    "cause",
    "type",
    "group_id",
    "verdict",
    "target_seq",
];

/// Parses one event line (no newline).
pub fn parse_event(line: &str) -> Result<Event, ParseError> {
    let fields = flat_object(line)?;
    let mut values: [Option<Scalar>; 7] = Default::default();
    for (k, v) in fields {
        let Some(i) = KEYS.iter().position(|known| *known == k) else {
            return Err(ParseError::UnknownKey(k));
        };
        if values[i].is_some() {
            return Err(ParseError::DuplicateKey(k));
        }
        values[i] = Some(v);
    }
    let [seq, ts, cause, ty, group, verdict, target] = values;
    let int = |v: Option<Scalar>, key: &'static str| match v {
        None => Err(ParseError::MissingKey(key)),
        Some(Scalar::Int(n)) if n.unsigned_abs() <= MAX_SAFE_INTEGER.unsigned_abs() => Ok(n),
        Some(_) => Err(ParseError::BadValue(key)),
    };
    let string = |v: Option<Scalar>, key: &'static str| match v {
        None => Err(ParseError::MissingKey(key)),
        Some(Scalar::Str(s)) => Ok(s),
        Some(_) => Err(ParseError::BadValue(key)),
    };
    let unexpected = |v: &Option<Scalar>, key: &str| match v {
        Some(_) => Err(ParseError::UnknownKey(key.to_owned())),
        None => Ok(()),
    };
    let seq = int(seq, "seq")?;
    let ts = int(ts, "ts")?;
    let cause = match string(cause, "cause")?.as_str() {
        "user" => Cause::User,
        "reconcile" => Cause::Reconcile,
        _ => return Err(ParseError::BadValue("cause")),
    };
    let body = match string(ty, "type")?.as_str() {
        "decided" => {
            unexpected(&target, "target_seq")?;
            let verdict = match string(verdict, "verdict")?.as_str() {
                "keep" => Verdict::Keep,
                "reject" => Verdict::Reject,
                _ => return Err(ParseError::BadValue("verdict")),
            };
            EventBody::Decided {
                group: int(group, "group_id")?,
                verdict,
            }
        }
        "cleared" => {
            unexpected(&verdict, "verdict")?;
            unexpected(&target, "target_seq")?;
            EventBody::Cleared {
                group: int(group, "group_id")?,
            }
        }
        t @ ("undo" | "redo") => {
            unexpected(&group, "group_id")?;
            unexpected(&verdict, "verdict")?;
            let target = int(target, "target_seq")?;
            if t == "undo" {
                EventBody::Undo { target }
            } else {
                EventBody::Redo { target }
            }
        }
        _ => return Err(ParseError::BadValue("type")),
    };
    Ok(Event {
        seq,
        ts,
        cause,
        body,
    })
}

/// A JSON object whose values are all strings without escapes or integers.
fn flat_object(text: &str) -> Result<Vec<(String, Scalar)>, ParseError> {
    let b = text.as_bytes();
    let mut i = 0;
    let syntax = |at, expected| ParseError::Syntax { at, expected };
    let ws = |i: &mut usize| {
        while *i < b.len() && matches!(b[*i], b' ' | b'\t' | b'\r' | b'\n') {
            *i += 1;
        }
    };
    let string = |i: &mut usize| -> Result<String, ParseError> {
        if b.get(*i) != Some(&b'"') {
            return Err(syntax(*i, "a string"));
        }
        let start = *i + 1;
        let mut j = start;
        loop {
            match b.get(j) {
                None => return Err(syntax(j, "the end of the string")),
                Some(b'"') => break,
                Some(b'\\') => return Err(syntax(j, "a string without escapes")),
                Some(c) if *c < 0x20 => return Err(syntax(j, "no control character")),
                Some(_) => j += 1,
            }
        }
        *i = j + 1;
        // The slice starts and ends next to an ASCII quote, so it is whole UTF-8.
        Ok(text[start..j].to_owned())
    };
    let number = |i: &mut usize| -> Result<i64, ParseError> {
        let start = *i;
        let mut j = *i;
        if b.get(j) == Some(&b'-') {
            j += 1;
        }
        let digits = j;
        while b.get(j).is_some_and(u8::is_ascii_digit) {
            j += 1;
        }
        if j == digits || (b[digits] == b'0' && j > digits + 1) {
            return Err(syntax(start, "an integer"));
        }
        if matches!(b.get(j), Some(b'.' | b'e' | b'E')) {
            return Err(syntax(j, "an integer"));
        }
        *i = j;
        text[start..j]
            .parse()
            .map_err(|_| syntax(start, "an integer in range"))
    };

    ws(&mut i);
    if b.get(i) != Some(&b'{') {
        return Err(syntax(i, "'{'"));
    }
    i += 1;
    let mut fields = Vec::new();
    ws(&mut i);
    if b.get(i) == Some(&b'}') {
        i += 1;
    } else {
        loop {
            ws(&mut i);
            let key = string(&mut i)?;
            ws(&mut i);
            if b.get(i) != Some(&b':') {
                return Err(syntax(i, "':'"));
            }
            i += 1;
            ws(&mut i);
            let value = if b.get(i) == Some(&b'"') {
                Scalar::Str(string(&mut i)?)
            } else {
                Scalar::Int(number(&mut i)?)
            };
            fields.push((key, value));
            ws(&mut i);
            match b.get(i) {
                Some(b',') => i += 1,
                Some(b'}') => {
                    i += 1;
                    break;
                }
                _ => return Err(syntax(i, "',' or '}'")),
            }
        }
    }
    ws(&mut i);
    if i != b.len() {
        return Err(syntax(i, "the end of the line"));
    }
    Ok(fields)
}
