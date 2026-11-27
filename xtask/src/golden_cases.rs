//! The golden decision-log cases (docs/testing.md §3.3), written by xtask, never by the code
//! under test: the expected results are written by hand here, or computed by this file's own
//! naive model for the generated cases, so a bug in `culla_core::decisions::fold` cannot hide in
//! the fixtures. The Rust golden test (`crates/core/tests/golden.rs`) folds every log with the
//! core and compares the text; the TS reducer does the same in M3 (US-27).
//!
//! Format: design/file-engine.md §5 and the `culla_core::decisions::codec` module docs. One
//! event per line, keys in the order `seq, ts, cause, type, group_id, verdict, target_seq`; the
//! expected fold with `state` (decided groups only), `done`, `undone` and, for a log that breaks
//! a precondition, `corrupt`.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// One case: its name, the `.jsonl` text and the `.expected.json` text.
pub(crate) struct Case {
    pub(crate) name: String,
    pub(crate) log: String,
    pub(crate) expected: String,
}

/// A time for each event: 27 Nov 2026 00:00 UTC plus one second per seq.
const TS_BASE: i64 = 1_764_201_600_000;

#[derive(Clone, Copy)]
enum Ev {
    /// `Decided` by the user: seq, group, `keep` or `reject`.
    Decided(i64, i64, &'static str),
    /// `Cleared` by the user (U): seq, group.
    Cleared(i64, i64),
    /// `Cleared` by reconciliation (ADR 0024): seq, group.
    Reconcile(i64, i64),
    /// `Undo`: seq, target seq.
    Undo(i64, i64),
    /// `Redo`: seq, target seq.
    Redo(i64, i64),
    /// `Decided` with the reconcile cause, which no valid log has: seq, group.
    ReconcileDecided(i64, i64),
    /// `Undo` with the reconcile cause, which no valid log has: seq, target seq.
    ReconcileUndo(i64, i64),
    /// `Redo` with the reconcile cause, which no valid log has: seq, target seq.
    ReconcileRedo(i64, i64),
}

fn line(e: Ev) -> String {
    let head = |seq: i64, cause: &str, ty: &str| {
        format!(
            "{{\"seq\":{seq},\"ts\":{},\"cause\":\"{cause}\",\"type\":\"{ty}\"",
            TS_BASE + seq * 1000
        )
    };
    match e {
        Ev::Decided(s, g, v) => format!(
            "{},\"group_id\":{g},\"verdict\":\"{v}\"}}",
            head(s, "user", "decided")
        ),
        Ev::ReconcileDecided(s, g) => format!(
            "{},\"group_id\":{g},\"verdict\":\"keep\"}}",
            head(s, "reconcile", "decided")
        ),
        Ev::Cleared(s, g) => format!("{},\"group_id\":{g}}}", head(s, "user", "cleared")),
        Ev::Reconcile(s, g) => format!("{},\"group_id\":{g}}}", head(s, "reconcile", "cleared")),
        Ev::Undo(s, t) => format!("{},\"target_seq\":{t}}}", head(s, "user", "undo")),
        Ev::Redo(s, t) => format!("{},\"target_seq\":{t}}}", head(s, "user", "redo")),
        Ev::ReconcileUndo(s, t) => {
            format!("{},\"target_seq\":{t}}}", head(s, "reconcile", "undo"))
        }
        Ev::ReconcileRedo(s, t) => {
            format!("{},\"target_seq\":{t}}}", head(s, "reconcile", "redo"))
        }
    }
}

/// The expected fold, written by hand.
struct Exp {
    state: &'static [(i64, &'static str)],
    done: &'static [i64],
    undone: &'static [i64],
    corrupt: Option<(i64, &'static str)>,
}

fn render_expected(
    state: &BTreeMap<i64, &str>,
    done: &[i64],
    undone: &[i64],
    corrupt: Option<(i64, &str)>,
) -> String {
    let list = |xs: &[i64]| xs.iter().map(i64::to_string).collect::<Vec<_>>().join(", ");
    let mut s = String::from("{\n  \"state\": {");
    if state.is_empty() {
        s.push_str("},\n");
    } else {
        let entries: Vec<String> = state
            .iter()
            .map(|(g, v)| format!("    \"{g}\": \"{v}\""))
            .collect();
        let _ = write!(s, "\n{}\n  }},\n", entries.join(",\n"));
    }
    let _ = write!(
        s,
        "  \"done\": [{}],\n  \"undone\": [{}]",
        list(done),
        list(undone)
    );
    if let Some((seq, reason)) = corrupt {
        let _ = write!(
            s,
            ",\n  \"corrupt\": {{\"seq\": {seq}, \"reason\": \"{reason}\"}}"
        );
    }
    s.push_str("\n}\n");
    s
}

fn case(name: &str, events: &[Ev], exp: &Exp) -> Case {
    let log: String = events.iter().map(|e| line(*e) + "\n").collect();
    let state = exp.state.iter().copied().collect();
    Case {
        name: name.to_owned(),
        log,
        expected: render_expected(&state, exp.done, exp.undone, exp.corrupt),
    }
}

const fn ok(
    state: &'static [(i64, &'static str)],
    done: &'static [i64],
    undone: &'static [i64],
) -> Exp {
    Exp {
        state,
        done,
        undone,
        corrupt: None,
    }
}

const fn bad(
    state: &'static [(i64, &'static str)],
    done: &'static [i64],
    undone: &'static [i64],
    seq: i64,
    reason: &'static str,
) -> Exp {
    Exp {
        state,
        done,
        undone,
        corrupt: Some((seq, reason)),
    }
}

/// Every case, in name order.
pub(crate) fn all() -> Vec<Case> {
    use Ev::*;
    const K: &str = "keep";
    const R: &str = "reject";
    let mut cases = vec![
        case("empty", &[], &ok(&[], &[], &[])),
        case(
            "decide-keep-reject",
            &[Decided(1, 1, K), Decided(2, 2, R)],
            &ok(&[(1, "kept"), (2, "rejected")], &[1, 2], &[]),
        ),
        case(
            "undo-redo",
            &[Decided(1, 1, K), Undo(2, 1), Redo(3, 1)],
            &ok(&[(1, "kept")], &[1], &[]),
        ),
        case(
            "undo-walks-back",
            &[Decided(1, 1, K), Decided(2, 2, R), Undo(3, 2), Undo(4, 1)],
            &ok(&[], &[], &[2, 1]),
        ),
        case(
            "new-decision-empties-redo",
            &[Decided(1, 1, K), Undo(2, 1), Decided(3, 2, R)],
            &ok(&[(2, "rejected")], &[3], &[]),
        ),
        // U, then undo of U restores the verdict; redo walks forward again.
        case(
            "clear-undo-redo",
            &[
                Decided(1, 1, K),
                Cleared(2, 1),
                Undo(3, 2),
                Undo(4, 1),
                Redo(5, 1),
                Redo(6, 2),
            ],
            &ok(&[], &[1, 2], &[]),
        ),
        case(
            "clear-undo-restores-reject",
            &[Decided(1, 4, R), Cleared(2, 4), Undo(3, 2)],
            &ok(&[(4, "rejected")], &[1], &[2]),
        ),
        case(
            "clear-then-decide-again",
            &[Decided(1, 1, K), Cleared(2, 1), Decided(3, 1, R)],
            &ok(&[(1, "rejected")], &[1, 2, 3], &[]),
        ),
        // ADR 0024: the reconcile clear drops the group's events from both stacks.
        case(
            "reconcile-drops-history",
            &[Decided(1, 1, K), Decided(2, 2, K), Reconcile(3, 1)],
            &ok(&[(2, "kept")], &[2], &[]),
        ),
        case(
            "reconcile-drops-undone",
            &[Decided(1, 1, K), Cleared(2, 1), Undo(3, 2), Reconcile(4, 1)],
            &ok(&[], &[], &[]),
        ),
        case(
            "reconcile-keeps-other-redo",
            &[
                Decided(1, 1, K),
                Decided(2, 2, R),
                Undo(3, 2),
                Reconcile(4, 1),
                Redo(5, 2),
            ],
            &ok(&[(2, "rejected")], &[2], &[]),
        ),
        case(
            "seq-gaps",
            &[Decided(10, 3, K), Decided(20, 12, R), Undo(35, 20)],
            &ok(&[(3, "kept")], &[10], &[20]),
        ),
        case(
            "group-ids-order",
            &[Decided(1, 10, K), Decided(2, 9, R), Decided(3, 100, K)],
            &ok(
                &[(9, "rejected"), (10, "kept"), (100, "kept")],
                &[1, 2, 3],
                &[],
            ),
        ),
        // Logs that break a precondition: the fold stops at the bad event (§5.1).
        case(
            "corrupt-decided-on-decided",
            &[Decided(1, 1, K), Decided(2, 1, R), Decided(3, 2, K)],
            &bad(&[(1, "kept")], &[1], &[], 2, "decided_on_decided"),
        ),
        case(
            "corrupt-cleared-on-undecided",
            &[Decided(1, 1, K), Cleared(2, 2)],
            &bad(&[(1, "kept")], &[1], &[], 2, "cleared_on_undecided"),
        ),
        case(
            "corrupt-reconcile-on-undecided",
            &[Reconcile(1, 1)],
            &bad(&[], &[], &[], 1, "cleared_on_undecided"),
        ),
        case(
            "corrupt-reconcile-decided",
            &[Decided(1, 1, K), ReconcileDecided(2, 2)],
            &bad(&[(1, "kept")], &[1], &[], 2, "reconcile_not_cleared"),
        ),
        case(
            "corrupt-undo-not-top",
            &[Decided(1, 1, K), Decided(2, 2, K), Undo(3, 1)],
            &bad(&[(1, "kept"), (2, "kept")], &[1, 2], &[], 3, "undo_not_top"),
        ),
        case(
            "corrupt-undo-after-reconcile",
            &[Decided(1, 1, K), Reconcile(2, 1), Undo(3, 1)],
            &bad(&[], &[], &[], 3, "undo_not_top"),
        ),
        case(
            "corrupt-redo-not-top",
            &[Decided(1, 1, R), Undo(2, 1), Decided(3, 2, K), Redo(4, 1)],
            &bad(&[(2, "kept")], &[3], &[], 4, "redo_not_top"),
        ),
        case(
            "corrupt-seq-not-increasing",
            &[Decided(2, 1, K), Decided(2, 2, K)],
            &bad(&[(1, "kept")], &[2], &[], 2, "seq_not_increasing"),
        ),
        case(
            "corrupt-seq-decreasing",
            &[Decided(5, 1, K), Decided(3, 2, K)],
            &bad(&[(1, "kept")], &[5], &[], 3, "seq_not_increasing"),
        ),
        // The order of checks: the seq first, then the cause, then the body's precondition.
        case(
            "corrupt-order-seq-before-body",
            &[Decided(1, 1, K), Decided(1, 1, R)],
            &bad(&[(1, "kept")], &[1], &[], 1, "seq_not_increasing"),
        ),
        case(
            "corrupt-order-cause-before-body",
            &[Decided(1, 1, K), ReconcileDecided(2, 1)],
            &bad(&[(1, "kept")], &[1], &[], 2, "reconcile_not_cleared"),
        ),
        // The reconcile cause belongs to `cleared` only, undo and redo included.
        case(
            "corrupt-reconcile-undo",
            &[Decided(1, 1, K), ReconcileUndo(2, 1)],
            &bad(&[(1, "kept")], &[1], &[], 2, "reconcile_not_cleared"),
        ),
        case(
            "corrupt-reconcile-redo",
            &[Decided(1, 1, K), Undo(2, 1), ReconcileRedo(3, 1)],
            &bad(&[], &[], &[1], 3, "reconcile_not_cleared"),
        ),
        // A redo must target the top of a non-empty undone stack, not an event below it.
        case(
            "corrupt-redo-below-top",
            &[
                Decided(1, 1, K),
                Decided(2, 2, R),
                Undo(3, 2),
                Undo(4, 1),
                Redo(5, 2),
            ],
            &bad(&[], &[], &[2, 1], 5, "redo_not_top"),
        ),
        // U is a new user step: it empties the redo stack like a new decision.
        case(
            "clear-empties-redo",
            &[
                Decided(1, 1, K),
                Decided(2, 2, R),
                Undo(3, 2),
                Cleared(4, 1),
            ],
            &ok(&[], &[1, 4], &[]),
        ),
    ];
    for seed in 1..=4 {
        cases.push(generated(seed));
    }
    cases.sort_by(|a, b| a.name.cmp(&b.name));
    cases
}

// --- Generated cases -----------------------------------------------------------------------

/// xorshift64*, seeded.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) % n
    }
}

/// One entry of a stack: (seq, group, state before, state after).
type Step = (i64, i64, &'static str, &'static str);

/// A random session of 60 commands over 5 groups, folded by this file's own model: refused
/// commands append nothing, as in the core.
fn generated(seed: u64) -> Case {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut state: BTreeMap<i64, &'static str> = BTreeMap::new();
    let mut done: Vec<Step> = Vec::new();
    let mut undone: Vec<Step> = Vec::new();
    let mut events = Vec::new();
    let mut seq = 0;
    for _ in 0..60 {
        let g = rng.below(5) as i64 + 1;
        let now = state.get(&g).copied().unwrap_or("undecided");
        let next = seq + 1;
        let event = match rng.below(100) {
            0..40 if now == "undecided" => {
                let (v, after) = if rng.below(2) == 0 {
                    ("keep", "kept")
                } else {
                    ("reject", "rejected")
                };
                undone.clear();
                done.push((next, g, now, after));
                state.insert(g, after);
                Some(Ev::Decided(next, g, v))
            }
            40..55 if now != "undecided" => {
                undone.clear();
                done.push((next, g, now, "undecided"));
                state.insert(g, "undecided");
                Some(Ev::Cleared(next, g))
            }
            55..60 if now != "undecided" => {
                done.retain(|s| s.1 != g);
                undone.retain(|s| s.1 != g);
                state.insert(g, "undecided");
                Some(Ev::Reconcile(next, g))
            }
            60..82 => done.pop().map(|s| {
                state.insert(s.1, s.2);
                undone.push(s);
                Ev::Undo(next, s.0)
            }),
            82..100 => undone.pop().map(|s| {
                state.insert(s.1, s.3);
                done.push(s);
                Ev::Redo(next, s.0)
            }),
            _ => None,
        };
        if let Some(e) = event {
            events.push(e);
            seq = next;
        }
    }
    state.retain(|_, s| *s != "undecided");
    let seqs = |xs: &[Step]| xs.iter().map(|s| s.0).collect::<Vec<_>>();
    Case {
        name: format!("generated-{seed:02}"),
        log: events.iter().map(|e| line(*e) + "\n").collect(),
        expected: render_expected(&state, &seqs(&done), &seqs(&undone), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_sorted() {
        let names: Vec<String> = all().into_iter().map(|c| c.name).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names, sorted);
    }

    #[test]
    fn expected_text() {
        let state = BTreeMap::from([(2, "kept"), (10, "rejected")]);
        assert_eq!(
            render_expected(&state, &[1, 2], &[], Some((3, "undo_not_top"))),
            "{\n  \"state\": {\n    \"2\": \"kept\",\n    \"10\": \"rejected\"\n  },\n  \"done\": [1, 2],\n  \"undone\": [],\n  \"corrupt\": {\"seq\": 3, \"reason\": \"undo_not_top\"}\n}\n"
        );
    }

    #[test]
    fn generated_cases_are_not_trivial() {
        for seed in 1..=4 {
            let c = generated(seed);
            assert!(c.log.lines().count() >= 30, "{}", c.name);
            for ty in ["decided", "cleared", "undo", "redo"] {
                assert!(
                    c.log.contains(&format!("\"type\":\"{ty}\"")),
                    "{} lacks {ty}",
                    c.name
                );
            }
        }
        // The reconcile clear (ADR 0024) is in the generated set, so the TS reducer meets it
        // after a long random history, not only in the hand-written cases.
        let reconciles = (1..=4)
            .map(|seed| {
                generated(seed)
                    .log
                    .matches("\"cause\":\"reconcile\"")
                    .count()
            })
            .sum::<usize>();
        assert!(reconciles >= 1, "no reconcile event in the generated cases");
    }
}
