//! Golden decision-log fixtures shared by Rust and TS (docs/testing.md §3.3).
//!
//! A case is `testdata/event-log/<case>.jsonl` (one event object per line) plus
//! `<case>.expected.json`, the result of `fold` (design/file-engine.md §5.1): an object with
//! `state` (group id to state), `done` and `undone` (seq stacks).
//!
//! `golden --check` checks the files are well formed and paired. It does not fold the log: the
//! Rust `fold` and its golden test land in M2 (US-10), the TS reducer in M3 (US-27). The event
//! schema is owned by design/file-engine.md and is not checked here yet.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::json::{self, Value};

pub(crate) const DIR: &str = "testdata/event-log";
const LOG: &str = ".jsonl";
const EXPECTED: &str = ".expected.json";

pub(crate) fn check(root: &Path) -> Result<(), String> {
    let dir = root.join(DIR);
    let entries = fs::read_dir(&dir).map_err(|e| format!("cannot read {DIR}/: {e}"))?;
    let mut logs = BTreeSet::new();
    let mut expected = BTreeSet::new();
    let mut problems = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(case) = name.strip_suffix(EXPECTED) {
            expected.insert(case.to_owned());
        } else if let Some(case) = name.strip_suffix(LOG) {
            logs.insert(case.to_owned());
        } else {
            problems.push(format!("{DIR}/{name}: not a {LOG} or {EXPECTED} fixture"));
        }
    }
    if logs.is_empty() {
        problems.push(format!("{DIR}/: no {LOG} cases"));
    }
    for case in logs.symmetric_difference(&expected) {
        let missing = if logs.contains(case) { EXPECTED } else { LOG };
        problems.push(format!("{DIR}/{case}: {case}{missing} is missing"));
    }
    for case in logs.intersection(&expected) {
        problems.extend(check_log(&dir, case));
        problems.extend(check_expected(&dir, case));
    }
    if problems.is_empty() {
        println!("{DIR}: {} golden case(s) well formed", logs.len());
        Ok(())
    } else {
        Err(format!(
            "golden fixtures have problems:\n  {}",
            problems.join("\n  ")
        ))
    }
}

fn read(dir: &Path, file: &str) -> Result<String, String> {
    fs::read_to_string(dir.join(file)).map_err(|e| format!("{DIR}/{file}: {e}"))
}

/// JSON Lines: every line is one JSON object; the file is empty or ends with a newline.
pub(crate) fn check_log_text(text: &str) -> Result<usize, String> {
    if !text.is_empty() && !text.ends_with('\n') {
        return Err("does not end with a newline".into());
    }
    let mut events = 0;
    for (n, line) in text.lines().enumerate() {
        let line_no = n + 1;
        match json::parse(line) {
            Ok(Value::Object(_)) => events += 1,
            Ok(_) => return Err(format!("line {line_no}: not a JSON object")),
            Err(e) => return Err(format!("line {line_no}: {e}")),
        }
    }
    Ok(events)
}

pub(crate) fn check_expected_text(text: &str) -> Result<(), String> {
    let v = json::parse(text)?;
    match (v.get("state"), v.get("done"), v.get("undone")) {
        (Some(Value::Object(_)), Some(Value::Array(_)), Some(Value::Array(_))) => Ok(()),
        _ => Err("needs \"state\" (object), \"done\" and \"undone\" (arrays)".into()),
    }
}

fn check_log(dir: &Path, case: &str) -> Option<String> {
    let file = format!("{case}{LOG}");
    read(dir, &file)
        .and_then(|t| check_log_text(&t))
        .err()
        .map(|e| format!("{DIR}/{file}: {e}"))
}

fn check_expected(dir: &Path, case: &str) -> Option<String> {
    let file = format!("{case}{EXPECTED}");
    read(dir, &file)
        .and_then(|t| check_expected_text(&t))
        .err()
        .map(|e| format!("{DIR}/{file}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs() {
        assert_eq!(check_log_text(""), Ok(0));
        assert_eq!(check_log_text("{\"seq\":1}\n{\"seq\":2}\n"), Ok(2));
        assert!(check_log_text("{\"seq\":1}").is_err());
        assert!(check_log_text("[1]\n").is_err());
        assert!(check_log_text("\n").is_err());
    }

    #[test]
    fn expected() {
        assert!(check_expected_text("{\"state\":{},\"done\":[],\"undone\":[]}\n").is_ok());
        assert!(check_expected_text("{\"state\":[],\"done\":[],\"undone\":[]}").is_err());
        assert!(check_expected_text("{}").is_err());
    }

    #[test]
    fn committed_fixtures_are_well_formed() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        check(&root).unwrap();
    }
}
