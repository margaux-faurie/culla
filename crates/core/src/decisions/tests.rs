use std::collections::{BTreeMap, BTreeSet};

use super::codec::{
    LogError, ParseError, parse_event, parse_log, render_event, render_fold, render_log,
};
use super::*;

const NO_LOCKS: &dyn Fn(GroupId) -> bool = &|_| false;

/// Appends what a command returned, with the next seq, and folds it.
struct Log {
    events: Vec<Event>,
    d: Decisions,
}

impl Log {
    fn new() -> Self {
        Self {
            events: Vec::new(),
            d: Decisions::new(),
        }
    }

    fn push(&mut self, made: Result<NewEvent, Refused>) -> Result<GroupId, Refused> {
        let e = made?.at(self.events.len() as Seq + 1, 0);
        let g = self.d.apply(&e).expect("a command's event always folds");
        self.events.push(e);
        Ok(g)
    }

    fn decide(&mut self, g: GroupId, v: Verdict) -> Result<GroupId, Refused> {
        let made = self.d.decide(g, v, NO_LOCKS);
        self.push(made)
    }

    fn clear(&mut self, g: GroupId) -> Result<GroupId, Refused> {
        let made = self.d.clear(g, NO_LOCKS);
        self.push(made)
    }

    fn reconcile(&mut self, g: GroupId) -> Result<GroupId, Refused> {
        let made = self.d.reconcile_clear(g, NO_LOCKS);
        self.push(made)
    }

    fn undo(&mut self) -> Result<GroupId, Refused> {
        let made = self.d.undo(NO_LOCKS);
        self.push(made)
    }

    fn redo(&mut self) -> Result<GroupId, Refused> {
        let made = self.d.redo(NO_LOCKS);
        self.push(made)
    }
}

fn ev(seq: Seq, cause: Cause, body: EventBody) -> Event {
    Event {
        seq,
        ts: 0,
        cause,
        body,
    }
}

fn decided(seq: Seq, group: GroupId, verdict: Verdict) -> Event {
    ev(seq, Cause::User, EventBody::Decided { group, verdict })
}

fn states(d: &Decisions) -> Vec<(GroupId, GroupState)> {
    d.decided().collect()
}

// --- Commands and fold (US-10, design/file-engine.md §5) ------------------------------------

#[test]
fn empty_log_folds_to_nothing() {
    let d = fold(&[]).unwrap();
    assert_eq!(d, Decisions::new());
    assert_eq!(d.state(1), GroupState::Undecided);
    assert!(d.done().is_empty() && d.undone().is_empty());
    assert_eq!(d.last_seq(), None);
    assert_eq!(d.undo_target(), None);
    assert_eq!(d.redo_target(), None);
}

#[test]
fn decide_keep_and_reject() {
    let mut log = Log::new();
    assert_eq!(log.decide(1, Verdict::Keep), Ok(1));
    assert_eq!(log.decide(2, Verdict::Reject), Ok(2));
    assert_eq!(
        states(&log.d),
        vec![(1, GroupState::Kept), (2, GroupState::Rejected)]
    );
    assert_eq!(log.d.done(), &[1, 2]);
    assert_eq!(log.d.undo_target(), Some((2, 2)));
}

/// INV-17, ADR 0006: ←/→ on a decided group append nothing.
#[test]
fn inv17_decide_on_decided_group_is_refused() {
    let mut log = Log::new();
    log.decide(1, Verdict::Keep).unwrap();
    let before = log.d.clone();
    for v in [Verdict::Keep, Verdict::Reject] {
        assert_eq!(log.decide(1, v), Err(Refused::AlreadyDecided { group: 1 }));
    }
    assert_eq!(log.d, before);
    assert_eq!(log.events.len(), 1);
}

/// INV-17: U on an undecided group, undo and redo with an empty stack append nothing.
#[test]
fn inv17_empty_commands_append_nothing() {
    let mut log = Log::new();
    assert_eq!(log.clear(1), Err(Refused::NotDecided { group: 1 }));
    assert_eq!(log.reconcile(1), Err(Refused::NotDecided { group: 1 }));
    assert_eq!(log.undo(), Err(Refused::NothingToUndo));
    assert_eq!(log.redo(), Err(Refused::NothingToRedo));
    assert!(log.events.is_empty());
}

#[test]
fn undo_walks_back_and_redo_walks_forward() {
    let mut log = Log::new();
    log.decide(1, Verdict::Keep).unwrap();
    log.decide(2, Verdict::Reject).unwrap();
    assert_eq!(log.undo(), Ok(2));
    assert_eq!(log.undo(), Ok(1));
    assert_eq!(log.undo(), Err(Refused::NothingToUndo));
    assert!(states(&log.d).is_empty());
    assert_eq!(log.d.undone(), &[2, 1]);
    assert_eq!(log.redo(), Ok(1));
    assert_eq!(states(&log.d), vec![(1, GroupState::Kept)]);
    assert_eq!(log.redo(), Ok(2));
    assert_eq!(log.redo(), Err(Refused::NothingToRedo));
    assert_eq!(log.d.done(), &[1, 2]);
}

#[test]
fn a_new_decision_empties_the_redo_stack() {
    let mut log = Log::new();
    log.decide(1, Verdict::Keep).unwrap();
    log.undo().unwrap();
    log.decide(2, Verdict::Reject).unwrap();
    assert!(log.d.undone().is_empty());
    assert_eq!(log.d.done(), &[3]);
    assert_eq!(log.redo(), Err(Refused::NothingToRedo));
}

#[test]
fn a_user_clear_empties_the_redo_stack() {
    let mut log = Log::new();
    log.decide(1, Verdict::Keep).unwrap();
    log.decide(2, Verdict::Keep).unwrap();
    log.undo().unwrap();
    log.clear(1).unwrap();
    assert!(log.d.undone().is_empty());
}

/// U is undoable: undoing it restores the state before it (§5.1).
#[test]
fn undo_of_clear_restores_the_previous_verdict() {
    for v in [Verdict::Keep, Verdict::Reject] {
        let mut log = Log::new();
        log.decide(1, v).unwrap();
        log.clear(1).unwrap();
        assert_eq!(log.d.state(1), GroupState::Undecided);
        assert_eq!(log.undo(), Ok(1));
        assert_eq!(log.d.state(1), v.state());
        assert_eq!(log.undo(), Ok(1));
        assert_eq!(log.d.state(1), GroupState::Undecided);
        log.redo().unwrap();
        log.redo().unwrap();
        assert_eq!(log.d.state(1), GroupState::Undecided);
        assert_eq!(log.d.done(), &[1, 2]);
    }
}

#[test]
fn clear_then_decide_again() {
    let mut log = Log::new();
    log.decide(1, Verdict::Keep).unwrap();
    log.clear(1).unwrap();
    log.decide(1, Verdict::Reject).unwrap();
    assert_eq!(log.d.state(1), GroupState::Rejected);
    assert_eq!(log.d.done(), &[1, 2, 3]);
}

/// ADR 0024: a reconcile clear makes the group undecided, drops its events from both stacks
/// and pushes nothing, so the old decision can be neither undone nor redone.
#[test]
fn inv17_reconcile_clear_leaves_the_stacks() {
    let mut log = Log::new();
    log.decide(1, Verdict::Keep).unwrap(); // 1
    log.decide(2, Verdict::Keep).unwrap(); // 2
    log.clear(2).unwrap(); // 3
    log.undo().unwrap(); // 4: undoes 3, group 2 kept again
    log.decide(3, Verdict::Reject).unwrap(); // 5: empties undone
    log.decide(4, Verdict::Reject).unwrap(); // 6
    log.undo().unwrap(); // 7: undoes 6
    assert_eq!(log.d.done(), &[1, 2, 5]);
    assert_eq!(log.d.undone(), &[6]);
    assert_eq!(log.reconcile(2), Ok(2)); // 8
    assert_eq!(log.d.state(2), GroupState::Undecided);
    assert_eq!(log.d.done(), &[1, 5]);
    assert_eq!(log.d.undone(), &[6], "the other group's redo survives");
    assert_eq!(log.events[7].cause, Cause::Reconcile);
    // Ctrl+Z goes to the decision before, never back to group 2's.
    assert_eq!(log.d.undo_target(), Some((5, 3)));
    assert_eq!(log.redo(), Ok(4));
}

#[test]
fn reconcile_clear_drops_an_undone_decision() {
    let mut log = Log::new();
    log.decide(1, Verdict::Keep).unwrap();
    log.clear(1).unwrap();
    log.undo().unwrap(); // kept again, undone = [2]
    log.reconcile(1).unwrap();
    assert!(log.d.done().is_empty());
    assert!(log.d.undone().is_empty());
    assert_eq!(log.redo(), Err(Refused::NothingToRedo));
    assert_eq!(log.undo(), Err(Refused::NothingToUndo));
}

/// §5.4: a locked or applied group's decision cannot change, and undo stops behind it.
#[test]
fn inv17_locked_groups_refuse_every_change() {
    let mut log = Log::new();
    log.decide(1, Verdict::Keep).unwrap();
    log.decide(2, Verdict::Keep).unwrap();
    log.decide(3, Verdict::Reject).unwrap();
    log.undo().unwrap(); // group 3 undone
    let locked: &dyn Fn(GroupId) -> bool = &|g| g == 2 || g == 3 || g == 5;
    let d = &log.d;
    assert_eq!(
        d.decide(5, Verdict::Keep, locked),
        Err(Refused::Locked { group: 5 })
    );
    assert_eq!(d.clear(2, locked), Err(Refused::Locked { group: 2 }));
    assert_eq!(
        d.reconcile_clear(2, locked),
        Err(Refused::Locked { group: 2 })
    );
    assert_eq!(d.undo(locked), Err(Refused::Locked { group: 2 }));
    assert_eq!(d.redo(locked), Err(Refused::Locked { group: 3 }));
    // U still works on the older, unlocked decision behind it.
    assert!(d.clear(1, locked).is_ok());
    // The lock is reported first: U would not help on a locked group.
    assert_eq!(
        d.decide(2, Verdict::Reject, locked),
        Err(Refused::Locked { group: 2 })
    );
}

#[test]
fn apply_returns_the_group_to_jump_to() {
    let mut log = Log::new();
    log.decide(7, Verdict::Keep).unwrap();
    log.decide(9, Verdict::Keep).unwrap();
    assert_eq!(log.undo(), Ok(9));
    assert_eq!(log.undo(), Ok(7));
    assert_eq!(log.redo(), Ok(7));
}

#[test]
fn seqs_may_have_gaps_but_must_increase() {
    let log = [
        decided(10, 1, Verdict::Keep),
        decided(20, 2, Verdict::Keep),
        ev(35, Cause::User, EventBody::Undo { target: 20 }),
    ];
    let d = fold(&log).unwrap();
    assert_eq!(d.done(), &[10]);
    assert_eq!(d.undone(), &[20]);
    assert_eq!(d.last_seq(), Some(35));
}

/// A log that breaks a precondition stops the fold at that event (§5.1).
#[test]
fn corrupt_logs_stop_at_the_bad_event() {
    use CorruptReason::*;
    let k = |seq, g| decided(seq, g, Verdict::Keep);
    let clear = |seq, g, cause| ev(seq, cause, EventBody::Cleared { group: g });
    let undo = |seq, t| ev(seq, Cause::User, EventBody::Undo { target: t });
    let redo = |seq, t| ev(seq, Cause::User, EventBody::Redo { target: t });
    let cases: Vec<(Vec<Event>, Seq, CorruptReason)> = vec![
        (vec![k(1, 1), k(1, 2)], 1, SeqNotIncreasing),
        (vec![k(2, 1), k(1, 2)], 1, SeqNotIncreasing),
        (
            vec![k(1, 1), decided(2, 1, Verdict::Reject)],
            2,
            DecidedOnDecided,
        ),
        (vec![clear(1, 1, Cause::User)], 1, ClearedOnUndecided),
        (vec![clear(1, 1, Cause::Reconcile)], 1, ClearedOnUndecided),
        (
            vec![ev(
                1,
                Cause::Reconcile,
                EventBody::Decided {
                    group: 1,
                    verdict: Verdict::Keep,
                },
            )],
            1,
            ReconcileNotCleared,
        ),
        (
            vec![
                k(1, 1),
                ev(2, Cause::Reconcile, EventBody::Undo { target: 1 }),
            ],
            2,
            ReconcileNotCleared,
        ),
        (vec![undo(1, 1)], 1, UndoNotTop),
        (vec![k(1, 1), k(2, 2), undo(3, 1)], 3, UndoNotTop),
        (vec![k(1, 1), redo(2, 1)], 2, RedoNotTop),
        (vec![k(1, 1), undo(2, 1), undo(3, 1)], 3, UndoNotTop),
        (
            vec![k(1, 1), clear(2, 1, Cause::Reconcile), undo(3, 1)],
            3,
            UndoNotTop,
        ),
        (
            vec![k(1, 1), undo(2, 1), k(3, 2), redo(4, 1)],
            4,
            RedoNotTop,
        ),
    ];
    for (log, seq, reason) in cases {
        let err = fold(&log).unwrap_err();
        assert_eq!(err.event, BadEvent { seq, reason }, "{log:?}");
        let at = log.iter().rposition(|e| e.seq == seq).unwrap();
        assert_eq!(*err.before, fold(&log[..at]).unwrap(), "{log:?}");
        // Events after the bad one are not folded.
        let mut longer = log.clone();
        longer.push(k(100, 50));
        assert_eq!(fold(&longer).unwrap_err(), err);
    }
}

#[test]
fn apply_leaves_the_fold_unchanged_on_error() {
    let mut d = fold(&[decided(1, 1, Verdict::Keep)]).unwrap();
    let before = d.clone();
    assert!(d.apply(&decided(2, 1, Verdict::Reject)).is_err());
    assert!(d.apply(&decided(1, 2, Verdict::Reject)).is_err());
    assert_eq!(d, before);
}

#[test]
fn messages() {
    assert_eq!(
        Refused::AlreadyDecided { group: 3 }.to_string(),
        "group 3 already has a decision"
    );
    let err = fold(&[decided(1, 1, Verdict::Keep), decided(2, 1, Verdict::Keep)]).unwrap_err();
    assert_eq!(
        err.to_string(),
        "corrupt decision log at event 2: decided on a group that already has a decision"
    );
    for r in [
        CorruptReason::SeqNotIncreasing,
        CorruptReason::DecidedOnDecided,
        CorruptReason::ClearedOnUndecided,
        CorruptReason::ReconcileNotCleared,
        CorruptReason::UndoNotTop,
        CorruptReason::RedoNotTop,
    ] {
        assert_eq!(CorruptReason::from_name(r.as_str()), Some(r));
    }
    assert_eq!(CorruptReason::from_name("other"), None);
}

// --- Codec ---------------------------------------------------------------------------------

#[test]
fn event_lines() {
    let log = [
        decided(1, 7, Verdict::Keep),
        ev(2, Cause::User, EventBody::Cleared { group: 7 }),
        ev(3, Cause::User, EventBody::Undo { target: 2 }),
        ev(4, Cause::User, EventBody::Redo { target: 2 }),
        ev(5, Cause::Reconcile, EventBody::Cleared { group: 8 }),
        Event {
            ts: 1_764_201_600_000,
            ..decided(6, -3, Verdict::Reject)
        },
    ];
    let text = render_log(&log);
    assert_eq!(
        text,
        "{\"seq\":1,\"ts\":0,\"cause\":\"user\",\"type\":\"decided\",\"group_id\":7,\"verdict\":\"keep\"}\n\
         {\"seq\":2,\"ts\":0,\"cause\":\"user\",\"type\":\"cleared\",\"group_id\":7}\n\
         {\"seq\":3,\"ts\":0,\"cause\":\"user\",\"type\":\"undo\",\"target_seq\":2}\n\
         {\"seq\":4,\"ts\":0,\"cause\":\"user\",\"type\":\"redo\",\"target_seq\":2}\n\
         {\"seq\":5,\"ts\":0,\"cause\":\"reconcile\",\"type\":\"cleared\",\"group_id\":8}\n\
         {\"seq\":6,\"ts\":1764201600000,\"cause\":\"user\",\"type\":\"decided\",\"group_id\":-3,\"verdict\":\"reject\"}\n"
    );
    assert_eq!(parse_log(&text).unwrap(), log);
    assert_eq!(parse_log(""), Ok(vec![]));
}

#[test]
fn event_lines_accept_any_key_order_and_whitespace() {
    let e = parse_event(
        " { \"verdict\" : \"keep\" ,\"group_id\":7,\t\"type\":\"decided\",\"cause\":\"user\",\"ts\":-5,\"seq\":1 } ",
    )
    .unwrap();
    assert_eq!(
        e,
        Event {
            ts: -5,
            ..decided(1, 7, Verdict::Keep)
        }
    );
}

#[test]
fn event_lines_are_strict() {
    let base = "\"seq\":1,\"ts\":0,\"cause\":\"user\"";
    let bad: &[(&str, ParseError)] = &[
        (
            "",
            ParseError::Syntax {
                at: 0,
                expected: "'{'",
            },
        ),
        (
            "[]",
            ParseError::Syntax {
                at: 0,
                expected: "'{'",
            },
        ),
        ("{}", ParseError::MissingKey("seq")),
        (
            "{\"seq\":1,\"ts\":0,\"type\":\"cleared\",\"group_id\":1}",
            ParseError::MissingKey("cause"),
        ),
        (
            &format!("{{{base},\"type\":\"cleared\"}}"),
            ParseError::MissingKey("group_id"),
        ),
        (
            &format!("{{{base},\"type\":\"decided\",\"group_id\":1}}"),
            ParseError::MissingKey("verdict"),
        ),
        (
            &format!("{{{base},\"type\":\"undo\"}}"),
            ParseError::MissingKey("target_seq"),
        ),
        (
            &format!("{{{base},\"type\":\"cleared\",\"group_id\":1,\"verdict\":\"keep\"}}"),
            ParseError::UnknownKey("verdict".into()),
        ),
        (
            &format!("{{{base},\"type\":\"undo\",\"target_seq\":1,\"group_id\":1}}"),
            ParseError::UnknownKey("group_id".into()),
        ),
        (
            &format!("{{{base},\"type\":\"cleared\",\"group_id\":1,\"note\":\"x\"}}"),
            ParseError::UnknownKey("note".into()),
        ),
        (
            &format!("{{{base},\"type\":\"cleared\",\"group_id\":1,\"group_id\":2}}"),
            ParseError::DuplicateKey("group_id".into()),
        ),
        (
            &format!("{{{base},\"type\":\"cleared\",\"group_id\":\"1\"}}"),
            ParseError::BadValue("group_id"),
        ),
        (
            &format!("{{{base},\"type\":\"shrug\",\"group_id\":1}}"),
            ParseError::BadValue("type"),
        ),
        (
            &format!("{{{base},\"type\":\"decided\",\"group_id\":1,\"verdict\":\"maybe\"}}"),
            ParseError::BadValue("verdict"),
        ),
        (
            "{\"seq\":1,\"ts\":0,\"cause\":\"robot\",\"type\":\"cleared\",\"group_id\":1}",
            ParseError::BadValue("cause"),
        ),
        (
            "{\"seq\":9007199254740992,\"ts\":0,\"cause\":\"user\",\"type\":\"cleared\",\"group_id\":1}",
            ParseError::BadValue("seq"),
        ),
        (
            "{\"seq\":-9223372036854775808,\"ts\":0,\"cause\":\"user\",\"type\":\"cleared\",\"group_id\":1}",
            ParseError::BadValue("seq"),
        ),
        (
            "{\"seq\":99999999999999999999,\"ts\":0}",
            ParseError::Syntax {
                at: 7,
                expected: "an integer in range",
            },
        ),
        (
            "{\"seq\":01}",
            ParseError::Syntax {
                at: 7,
                expected: "an integer",
            },
        ),
        (
            "{\"seq\":1.0}",
            ParseError::Syntax {
                at: 8,
                expected: "an integer",
            },
        ),
        (
            "{\"seq\":1e3}",
            ParseError::Syntax {
                at: 8,
                expected: "an integer",
            },
        ),
        (
            "{\"seq\":+1}",
            ParseError::Syntax {
                at: 7,
                expected: "an integer",
            },
        ),
        (
            "{\"seq\":true}",
            ParseError::Syntax {
                at: 7,
                expected: "an integer",
            },
        ),
        (
            "{\"ca\\u0075se\":1}",
            ParseError::Syntax {
                at: 4,
                expected: "a string without escapes",
            },
        ),
        (
            "{\"seq\":1,}",
            ParseError::Syntax {
                at: 9,
                expected: "a string",
            },
        ),
        (
            "{\"seq\" 1}",
            ParseError::Syntax {
                at: 7,
                expected: "':'",
            },
        ),
        (
            "{\"seq\":1",
            ParseError::Syntax {
                at: 8,
                expected: "',' or '}'",
            },
        ),
        (
            "{\"seq\":1}x",
            ParseError::Syntax {
                at: 9,
                expected: "the end of the line",
            },
        ),
        (
            "{\"se",
            ParseError::Syntax {
                at: 4,
                expected: "the end of the string",
            },
        ),
    ];
    for (line, want) in bad {
        assert_eq!(parse_event(line).as_ref(), Err(want), "{line}");
    }
}

#[test]
fn logs_need_a_final_newline_and_no_blank_lines() {
    let line = render_event(&decided(1, 1, Verdict::Keep));
    assert_eq!(
        parse_log(&line),
        Err(LogError {
            line: 1,
            error: ParseError::NoFinalNewline
        })
    );
    assert_eq!(parse_log(&format!("{line}\n{line}")).unwrap_err().line, 2);
    assert_eq!(parse_log("\n").unwrap_err().line, 1);
    assert_eq!(parse_log(&format!("{line}\n\n")).unwrap_err().line, 2);
    let err = parse_log(&format!("{line}\n{{}}\n")).unwrap_err();
    assert_eq!(err.to_string(), "line 2: missing key \"seq\"");
}

#[test]
fn fold_text() {
    assert_eq!(
        render_fold(&fold(&[])),
        "{\n  \"state\": {},\n  \"done\": [],\n  \"undone\": []\n}\n"
    );
    let mut log = Log::new();
    log.decide(9, Verdict::Reject).unwrap();
    log.decide(10, Verdict::Keep).unwrap();
    log.decide(2, Verdict::Keep).unwrap();
    log.undo().unwrap();
    assert_eq!(
        render_fold(&Ok(log.d.clone())),
        "{\n  \"state\": {\n    \"9\": \"rejected\",\n    \"10\": \"kept\"\n  },\n  \"done\": [1, 2],\n  \"undone\": [3]\n}\n"
    );
    let mut events = log.events.clone();
    events.push(decided(5, 9, Verdict::Keep));
    assert_eq!(
        render_fold(&fold(&events)),
        "{\n  \"state\": {\n    \"9\": \"rejected\",\n    \"10\": \"kept\"\n  },\n  \"done\": [1, 2],\n  \"undone\": [3],\n  \"corrupt\": {\"seq\": 5, \"reason\": \"decided_on_decided\"}\n}\n"
    );
}

// --- Property-style tests (docs/testing.md §3.2) -------------------------------------------

/// xorshift64*: small, seeded, deterministic.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

/// Cases per property: `CULLA_PROP_CASES` if set (10,000 for a long run), else 300.
fn cases() -> u64 {
    std::env::var("CULLA_PROP_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300)
}

#[derive(Clone, Copy, Debug)]
enum Cmd {
    Decide(GroupId, Verdict),
    Clear(GroupId),
    Reconcile(GroupId),
    Undo,
    Redo,
}

fn random_cmd(rng: &mut Rng, groups: u64) -> Cmd {
    let g = rng.below(groups) as GroupId + 1;
    let v = if rng.chance(50) {
        Verdict::Keep
    } else {
        Verdict::Reject
    };
    match rng.below(100) {
        0..40 => Cmd::Decide(g, v),
        40..55 => Cmd::Clear(g),
        55..60 => Cmd::Reconcile(g),
        60..82 => Cmd::Undo,
        _ => Cmd::Redo,
    }
}

/// The naive model of docs/testing.md §3.2: a map updated step by step, with stacks of
/// (seq, group, state before, state after). Written separately from `Decisions`.
#[derive(Clone, Debug, Default)]
struct Model {
    state: BTreeMap<GroupId, GroupState>,
    done: Vec<(Seq, GroupId, GroupState, GroupState)>,
    undone: Vec<(Seq, GroupId, GroupState, GroupState)>,
}

impl Model {
    fn get(&self, g: GroupId) -> GroupState {
        self.state.get(&g).copied().unwrap_or(GroupState::Undecided)
    }

    fn put(&mut self, g: GroupId, s: GroupState) {
        self.state.insert(g, s);
    }

    /// What the command should give, and the model after it.
    fn run(
        &mut self,
        cmd: Cmd,
        seq: Seq,
        locked: &BTreeSet<GroupId>,
    ) -> Result<EventBody, Refused> {
        match cmd {
            Cmd::Decide(g, v) => {
                if locked.contains(&g) {
                    return Err(Refused::Locked { group: g });
                }
                if self.get(g) != GroupState::Undecided {
                    return Err(Refused::AlreadyDecided { group: g });
                }
                let after = match v {
                    Verdict::Keep => GroupState::Kept,
                    Verdict::Reject => GroupState::Rejected,
                };
                self.done.push((seq, g, GroupState::Undecided, after));
                self.undone.clear();
                self.put(g, after);
                Ok(EventBody::Decided {
                    group: g,
                    verdict: v,
                })
            }
            Cmd::Clear(g) | Cmd::Reconcile(g) => {
                if locked.contains(&g) {
                    return Err(Refused::Locked { group: g });
                }
                let before = self.get(g);
                if before == GroupState::Undecided {
                    return Err(Refused::NotDecided { group: g });
                }
                if matches!(cmd, Cmd::Clear(_)) {
                    self.done.push((seq, g, before, GroupState::Undecided));
                    self.undone.clear();
                } else {
                    self.done.retain(|x| x.1 != g);
                    self.undone.retain(|x| x.1 != g);
                }
                self.put(g, GroupState::Undecided);
                Ok(EventBody::Cleared { group: g })
            }
            Cmd::Undo => {
                let Some(&(t, g, before, _)) = self.done.last() else {
                    return Err(Refused::NothingToUndo);
                };
                if locked.contains(&g) {
                    return Err(Refused::Locked { group: g });
                }
                let x = self.done.pop().unwrap();
                self.undone.push(x);
                self.put(g, before);
                Ok(EventBody::Undo { target: t })
            }
            Cmd::Redo => {
                let Some(&(t, g, _, after)) = self.undone.last() else {
                    return Err(Refused::NothingToRedo);
                };
                if locked.contains(&g) {
                    return Err(Refused::Locked { group: g });
                }
                let x = self.undone.pop().unwrap();
                self.done.push(x);
                self.put(g, after);
                Ok(EventBody::Redo { target: t })
            }
        }
    }

    fn matches(&self, d: &Decisions) -> bool {
        let model: Vec<_> = self
            .state
            .iter()
            .filter(|(_, s)| **s != GroupState::Undecided)
            .map(|(g, s)| (*g, *s))
            .collect();
        let done: Vec<Seq> = self.done.iter().map(|x| x.0).collect();
        let undone: Vec<Seq> = self.undone.iter().map(|x| x.0).collect();
        model == states(d) && done == d.done() && undone == d.undone()
    }
}

fn command(d: &Decisions, cmd: Cmd, locked: &BTreeSet<GroupId>) -> Result<NewEvent, Refused> {
    let locked = |g| locked.contains(&g);
    match cmd {
        Cmd::Decide(g, v) => d.decide(g, v, &locked),
        Cmd::Clear(g) => d.clear(g, &locked),
        Cmd::Reconcile(g) => d.reconcile_clear(g, &locked),
        Cmd::Undo => d.undo(&locked),
        Cmd::Redo => d.redo(&locked),
    }
}

/// Internal consistency: every stacked seq has its step, and nothing else does; the stacks are
/// disjoint; the top of each stack agrees with the group's state.
fn check_internal(d: &Decisions) {
    let stacked: BTreeSet<Seq> = d.done.iter().chain(&d.undone).copied().collect();
    assert_eq!(stacked.len(), d.done.len() + d.undone.len());
    assert_eq!(stacked, d.steps.keys().copied().collect());
    if let Some(t) = d.done.last() {
        assert_eq!(d.state(d.steps[t].group), d.steps[t].after);
    }
    if let Some(t) = d.undone.last() {
        assert_eq!(d.state(d.steps[t].group), d.steps[t].before);
    }
    assert!(d.state.values().all(|s| s.is_decided()));
}

/// One random session: commands checked against the model at every step. Returns the log and
/// the fold after each event.
fn random_session(rng: &mut Rng) -> (Vec<Event>, Vec<Decisions>) {
    let groups = 1 + rng.below(6);
    let len = rng.below(80);
    let mut d = Decisions::new();
    let mut model = Model::default();
    let mut locked = BTreeSet::new();
    let mut events = Vec::new();
    let mut after_each = Vec::new();
    let mut seq: Seq = rng.below(5) as Seq;
    for _ in 0..len {
        // Locks come and go as plans open and close.
        if rng.chance(5) {
            let g = rng.below(groups) as GroupId + 1;
            if !locked.remove(&g) {
                locked.insert(g);
            }
        }
        let cmd = random_cmd(rng, groups);
        let next = seq + 1 + rng.below(3) as Seq;
        let want = model.clone().run(cmd, next, &locked);
        let got = command(&d, cmd, &locked);
        assert_eq!(got.map(|n| n.body), want, "{cmd:?} on {d:?}");
        let before = d.clone();
        match got {
            Err(_) => assert_eq!(d, before),
            Ok(made) => {
                if matches!(cmd, Cmd::Reconcile(_)) {
                    assert_eq!(made.cause, Cause::Reconcile);
                } else {
                    assert_eq!(made.cause, Cause::User);
                }
                model.run(cmd, next, &locked).unwrap();
                let e = made.at(next, next * 1000);
                let jumped = d.apply(&e).unwrap();
                seq = next;
                events.push(e);
                after_each.push(d.clone());
                match cmd {
                    Cmd::Decide(g, _) | Cmd::Clear(g) | Cmd::Reconcile(g) => assert_eq!(jumped, g),
                    Cmd::Undo => assert_eq!(Some(jumped), before.undo_target().map(|t| t.1)),
                    Cmd::Redo => assert_eq!(Some(jumped), before.redo_target().map(|t| t.1)),
                }
            }
        }
        assert!(model.matches(&d), "{cmd:?}: model {model:?} vs {d:?}");
        check_internal(&d);
    }
    (events, after_each)
}

fn run_fold_matches_model(n: u64) {
    for case in 0..n {
        let mut rng = Rng::new(case);
        let (events, after_each) = random_session(&mut rng);
        // Every prefix folds, to the state the session had then (resume after a crash).
        for (i, want) in after_each.iter().enumerate() {
            assert_eq!(&fold(&events[..=i]).unwrap(), want, "case {case}");
        }
        assert_eq!(
            fold(&events).unwrap(),
            after_each.last().cloned().unwrap_or_default()
        );
        // The text form round-trips.
        assert_eq!(
            parse_log(&render_log(&events)).unwrap(),
            events,
            "case {case}"
        );
    }
}

/// US-10, INV-15: the fold equals the naive model, every refusal appends nothing, and every
/// prefix of a log folds to a valid state.
#[test]
fn prop_fold_matches_the_model() {
    run_fold_matches_model(cases());
}

/// The long run: `cargo test -p culla-core -- --ignored prop_fold_10k`.
#[test]
#[ignore = "long run (10,000 cases); CI runs the default count"]
fn prop_fold_10k() {
    run_fold_matches_model(10_000);
}

/// Undo then redo returns the same state and stacks; a new decision after an undo empties the
/// redo stack; after a reconcile clear no stacked event targets the group.
#[test]
fn prop_undo_redo_and_reconcile() {
    for case in 0..cases() {
        let mut rng = Rng::new(case ^ 0x5EED);
        let (events, after_each) = random_session(&mut rng);
        let mut seq = events.last().map_or(0, |e| e.seq);
        for d in &after_each {
            if let Ok(u) = d.undo(NO_LOCKS) {
                let mut x = d.clone();
                seq += 1;
                x.apply(&u.at(seq, 0)).unwrap();
                let r = x.redo(NO_LOCKS).unwrap();
                seq += 1;
                x.apply(&r.at(seq, 0)).unwrap();
                assert_eq!(states(&x), states(d), "case {case}");
                assert_eq!(x.done(), d.done());
                assert_eq!(x.undone(), d.undone());
                // A new decision after the undo empties the redo stack.
                let mut y = d.clone();
                seq += 1;
                y.apply(&u.at(seq, 0)).unwrap();
                if let Ok(n) = y.decide(1000, Verdict::Keep, NO_LOCKS) {
                    seq += 1;
                    y.apply(&n.at(seq, 0)).unwrap();
                    assert!(y.undone().is_empty());
                }
            }
            for (g, _) in d.decided() {
                let mut x = d.clone();
                seq += 1;
                x.apply(&d.reconcile_clear(g, NO_LOCKS).unwrap().at(seq, 0))
                    .unwrap();
                assert_eq!(x.state(g), GroupState::Undecided);
                let targets: Vec<GroupId> = x
                    .done
                    .iter()
                    .chain(&x.undone)
                    .map(|s| x.steps[s].group)
                    .collect();
                assert!(!targets.contains(&g), "case {case}");
                // Other groups keep their state.
                for (h, s) in d.decided().filter(|(h, _)| *h != g) {
                    assert_eq!(x.state(h), s);
                }
            }
        }
    }
}

/// "The state before a `Cleared`" (file-engine §5.1) is the fold of the log up to that event:
/// the step a user `Cleared` stacks remembers exactly the state that refolding the prefix
/// gives, and undoing it restores that state.
#[test]
fn prop_cleared_remembers_the_refolded_state() {
    for case in 0..cases() {
        let mut rng = Rng::new(case ^ 0xC1EA);
        let (events, after_each) = random_session(&mut rng);
        for (i, e) in events.iter().enumerate() {
            let EventBody::Cleared { group } = e.body else {
                continue;
            };
            if e.cause != Cause::User {
                continue;
            }
            let refolded = fold(&events[..i]).unwrap().state(group);
            assert!(refolded.is_decided(), "case {case}");
            let d = &after_each[i];
            assert_eq!(d.steps[&e.seq].before, refolded, "case {case}");
            let mut x = d.clone();
            x.apply(&d.undo(NO_LOCKS).unwrap().at(e.seq + 1, 0))
                .unwrap();
            assert_eq!(x.state(group), refolded, "case {case}");
        }
    }
}

/// A valid log with one bad event inserted stops at it, with the fold of what came before.
#[test]
fn prop_corrupt_logs_stop_at_the_bad_event() {
    for case in 0..cases() {
        let mut rng = Rng::new(case ^ 0xBAD);
        let (events, after_each) = random_session(&mut rng);
        let k = rng.below(events.len() as u64 + 1) as usize;
        let prefix = if k == 0 {
            Decisions::new()
        } else {
            after_each[k - 1].clone()
        };
        let last = prefix.last_seq().unwrap_or(0);
        let top = prefix.done().last().copied();
        let (bad, reason) = match rng.below(6) {
            0 if prefix.last_seq().is_some() => (
                decided(last, 1, Verdict::Keep),
                CorruptReason::SeqNotIncreasing,
            ),
            0 => (
                ev(-1, Cause::User, EventBody::Undo { target: 1 }),
                CorruptReason::UndoNotTop,
            ),
            1 => match prefix.decided().next() {
                Some((g, _)) => (
                    decided(last + 1, g, Verdict::Keep),
                    CorruptReason::DecidedOnDecided,
                ),
                None => (
                    ev(last + 1, Cause::User, EventBody::Cleared { group: 1 }),
                    CorruptReason::ClearedOnUndecided,
                ),
            },
            2 => (
                ev(
                    last + 1,
                    Cause::Reconcile,
                    EventBody::Cleared { group: 999 },
                ),
                CorruptReason::ClearedOnUndecided,
            ),
            3 => (
                ev(
                    last + 1,
                    Cause::User,
                    EventBody::Undo {
                        target: top.map_or(1, |t| t - 1),
                    },
                ),
                CorruptReason::UndoNotTop,
            ),
            4 => (
                ev(last + 1, Cause::User, EventBody::Redo { target: -1 }),
                CorruptReason::RedoNotTop,
            ),
            _ => (
                ev(
                    last + 1,
                    Cause::Reconcile,
                    EventBody::Decided {
                        group: 999,
                        verdict: Verdict::Reject,
                    },
                ),
                CorruptReason::ReconcileNotCleared,
            ),
        };
        let mut log = events[..k].to_vec();
        log.push(bad);
        log.extend_from_slice(&events[k..]);
        let err = fold(&log).unwrap_err();
        assert_eq!(
            err.event,
            BadEvent {
                seq: bad.seq,
                reason
            },
            "case {case}"
        );
        assert_eq!(*err.before, prefix, "case {case}");
    }
}
