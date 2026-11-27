//! Golden decision-log fixtures (docs/testing.md §3.3; US-10; INV-15): every
//! `testdata/event-log/<case>.jsonl` is parsed and folded by the core, and the fold's text must
//! equal `<case>.expected.json` byte for byte. The fixtures are written by `cargo xtask golden`
//! from hand-written expectations and xtask's own model, never by this code. The TS reducer
//! runs the same files in M3 (US-27).
//!
//! The core does no file I/O, tests included (ADR 0002), so the cases are compiled in with
//! `include_str!`. `cargo xtask golden --check` fails if a case in `testdata/event-log/` is
//! missing from [`CASES`], so a new case cannot be skipped here.

use culla_core::decisions::codec::{parse_log, render_fold, render_log};
use culla_core::decisions::fold;

macro_rules! case {
    ($name:literal) => {
        (
            $name,
            include_str!(concat!("../../../testdata/event-log/", $name, ".jsonl")),
            include_str!(concat!(
                "../../../testdata/event-log/",
                $name,
                ".expected.json"
            )),
        )
    };
}

/// (name, log, expected fold), in name order.
const CASES: &[(&str, &str, &str)] = &[
    case!("clear-then-decide-again"),
    case!("clear-undo-redo"),
    case!("clear-undo-restores-reject"),
    case!("corrupt-cleared-on-undecided"),
    case!("corrupt-decided-on-decided"),
    case!("corrupt-reconcile-decided"),
    case!("corrupt-reconcile-on-undecided"),
    case!("corrupt-redo-not-top"),
    case!("corrupt-seq-not-increasing"),
    case!("corrupt-undo-after-reconcile"),
    case!("corrupt-undo-not-top"),
    case!("decide-keep-reject"),
    case!("empty"),
    case!("generated-01"),
    case!("generated-02"),
    case!("generated-03"),
    case!("generated-04"),
    case!("group-ids-order"),
    case!("new-decision-empties-redo"),
    case!("reconcile-drops-history"),
    case!("reconcile-drops-undone"),
    case!("reconcile-keeps-other-redo"),
    case!("seq-gaps"),
    case!("undo-redo"),
    case!("undo-walks-back"),
];

#[test]
fn inv15_every_golden_case_folds_to_its_expected_state() {
    let mut failures = Vec::new();
    for (name, log, expected) in CASES {
        let events = match parse_log(log) {
            Ok(events) => events,
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        // The text form is canonical: rendering the parsed log gives the file back.
        if render_log(&events) != *log {
            failures.push(format!("{name}: the log is not in canonical form"));
        }
        let got = render_fold(&fold(&events));
        if got != *expected {
            failures.push(format!(
                "{name}:\n--- expected\n{expected}--- folded\n{got}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Resume after a crash: every prefix of every valid case folds without error, and a corrupt
/// case folds cleanly up to the event before the bad one.
#[test]
fn every_prefix_of_a_golden_log_folds() {
    for (name, log, _) in CASES {
        let events = parse_log(log).unwrap();
        let end = match fold(&events) {
            Ok(_) => events.len(),
            Err(c) => events.iter().rposition(|e| e.seq == c.event.seq).unwrap(),
        };
        for i in 0..=end {
            assert!(fold(&events[..i]).is_ok(), "{name}: prefix of {i} events");
        }
    }
}

#[test]
fn cases_are_listed_in_name_order() {
    let names: Vec<&str> = CASES.iter().map(|c| c.0).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(names, sorted);
}
