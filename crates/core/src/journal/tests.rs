//! Journal and executor tests: the transition table, the plan-to-journal mapping and its
//! refusals, every cell of the recovery table run on the fake disk, the executor's error and
//! group rules (US-17 to US-20), and a seeded crash loop: from any interleaving of crashes,
//! resume reaches a terminal state where every planned file is exactly where its end state
//! says, nothing else was touched, and a second resume does nothing (INV-01, INV-11, INV-12,
//! INV-14, INV-16).

use std::collections::{BTreeMap, BTreeSet};

use super::fake::{
    FakeDisk, FakeStore, FileRec, Mode, Rng, Shared, World, dst_key, dst_key_of, hash_of, src_key,
};
use super::recovery::{COPY, Cell, FINISH_PENDING, RENAME, cell};
use super::*;
use crate::plan::{OpKind, Root};
use crate::preflight::TimeRule;

const MTIME: i64 = 1_700_000_000_000_000_000;

fn mk(id: OpId, ord: u32, group: GroupId, kind: Kind, root: Root, name: &str) -> Op {
    Op {
        id,
        ord,
        group,
        member: id * 10,
        action: if root == Root::Rejected {
            Verdict::Reject
        } else {
            Verdict::Keep
        },
        kind,
        src: vec!["100NIKON".to_owned(), name.to_owned()],
        root,
        dst: vec!["100NIKON".to_owned(), name.to_owned()],
        expected_size: 1000 + u64::try_from(id).unwrap(),
        src_mtime_ns: Some(MTIME),
        finish_hash: None,
    }
}

/// Ops for `(group, kind, root)` triples, ids 1, 2, … in order.
fn ops(spec: &[(GroupId, Kind, Root)]) -> Vec<Op> {
    spec.iter()
        .enumerate()
        .map(|(i, &(g, k, r))| {
            let id = i as OpId + 1;
            let mut o = mk(id, i as u32, g, k, r, &format!("DSC_{id:04}.JPG"));
            if k == Kind::Finish {
                o.finish_hash = Some(hash_of(10_000 + id as u64));
            }
            o
        })
        .collect()
}

fn world(ops: Vec<Op>) -> Shared {
    World::new(Journal::new(ops).unwrap(), 1)
}

fn go(w: &Shared, j: &mut Journal) -> Result<End, RunError> {
    run(
        j,
        &mut FakeDisk(w.clone()),
        &mut FakeStore(w.clone()),
        &mut || None,
    )
}

fn state(j: &Journal, id: OpId) -> OpState {
    j.get(id).unwrap().1.state()
}

fn err(j: &Journal, id: OpId) -> Option<FailReason> {
    j.get(id).unwrap().1.last_error
}

fn file(w: &Shared, k: &str) -> Option<u64> {
    w.borrow().files.get(k).map(|r| r.content)
}

fn content(w: &Shared, id: OpId) -> u64 {
    w.borrow().content[&id]
}

fn partial_files(w: &Shared) -> Vec<String> {
    w.borrow()
        .files
        .keys()
        .filter(|k| k.ends_with(".culla-partial"))
        .cloned()
        .collect()
}

// --- Transitions -----------------------------------------------------------------------------

#[test]
fn edges_follow_file_engine_9_2() {
    assert_eq!(edges(Kind::Rename).len(), 9);
    assert_eq!(edges(Kind::Copy).len(), 12);
    assert_eq!(edges(Kind::XMove).len(), 17);
    assert_eq!(edges(Kind::Finish).len(), 12);
    use OpState::*;
    assert!(is_legal(Kind::Copy, Committed, Done));
    assert!(!is_legal(Kind::XMove, Committed, Done));
    assert!(is_legal(Kind::XMove, Committed, SourceRemoved));
    assert!(!is_legal(Kind::Copy, Committed, SourceRemoved));
    assert!(!is_legal(Kind::Rename, Pending, Copying));
    assert!(!is_legal(Kind::Copy, Pending, Renaming));
    assert!(is_legal(Kind::XMove, CopiedSourceKept, Committed));
    assert!(!is_legal(Kind::Finish, Pending, Copying));
    for kind in [Kind::Rename, Kind::Copy, Kind::XMove, Kind::Finish] {
        let e = edges(kind);
        let set: BTreeSet<_> = e.iter().collect();
        assert_eq!(set.len(), e.len(), "{kind:?} has a repeated edge");
        for (from, to) in e {
            assert_ne!(from, to);
            // Out of a final state only the user actions of §9.6.
            if from.is_final() {
                assert!(
                    matches!(
                        (from, to),
                        (Failed, Pending) | (CopiedSourceKept, Committed)
                    ),
                    "{kind:?}: {from} → {to}"
                );
            }
            // Never back to a state before the copy from done.
            assert_ne!(*from, Done, "{kind:?}: nothing leaves done");
        }
        // Every non-final state the kind uses can fail.
        for (from, _) in e {
            if !from.is_final() {
                assert!(is_legal(kind, *from, Failed), "{kind:?} {from} cannot fail");
            }
        }
    }
    for s in OpState::ALL {
        assert!(!s.as_str().is_empty());
    }
    assert_eq!(Kind::from(OpKind::XMove), Kind::XMove);
    assert_eq!(Kind::Finish.as_str(), "finish");
}

#[test]
fn journal_refuses_illegal_entries() {
    let mut j = Journal::new(ops(&[(1, Kind::Copy, Root::Dest)])).unwrap();
    let e = |from, to, detail| Entry {
        op: 1,
        from,
        to,
        detail,
    };
    use OpState::*;
    assert!(matches!(
        j.apply(&e(Pending, Renaming, Detail::None)),
        Err(JournalError::Illegal(_))
    ));
    assert!(matches!(
        j.apply(&e(Copying, Verifying, Detail::None)),
        Err(JournalError::NotCurrent { .. })
    ));
    assert!(matches!(
        j.apply(&e(Pending, Copying, Detail::None)),
        Err(JournalError::BadDetail(_))
    ));
    assert!(matches!(
        j.apply(&e(Pending, Copying, Detail::Attempt { attempt: 2 })),
        Err(JournalError::BadDetail(_))
    ));
    assert!(matches!(
        j.apply(&Entry {
            op: 9,
            ..e(Pending, Failed, Detail::Failed(FailReason::Locked))
        }),
        Err(JournalError::UnknownOp(9))
    ));
    j.apply(&e(Pending, Copying, Detail::Attempt { attempt: 1 }))
        .unwrap();
    // H1 must come with the plan's size.
    assert!(matches!(
        j.apply(&e(
            Copying,
            Verifying,
            Detail::Copied {
                hash: [0; 32],
                size: 1
            }
        )),
        Err(JournalError::BadDetail(_))
    ));
    assert!(matches!(
        j.apply(&e(Copying, Failed, Detail::None)),
        Err(JournalError::BadDetail(_))
    ));
    j.apply(&e(
        Copying,
        Verifying,
        Detail::Copied {
            hash: [7; 32],
            size: 1001,
        },
    ))
    .unwrap();
    let (_, p) = j.get(1).unwrap();
    assert_eq!((p.attempts, p.hash), (1, Some([7; 32])));
    assert_eq!(
        j.partials(0),
        vec![ops(&[(1, Kind::Copy, Root::Dest)])[0].partial(1)]
    );
    // Replay of the same entries gives the same journal.
    let mut k = Journal::new(ops(&[(1, Kind::Copy, Root::Dest)])).unwrap();
    k.replay(&[
        e(Pending, Copying, Detail::Attempt { attempt: 1 }),
        e(
            Copying,
            Verifying,
            Detail::Copied {
                hash: [7; 32],
                size: 1001,
            },
        ),
    ])
    .unwrap();
    assert_eq!(j, k);
    assert!(j.apply(&e(Verifying, Committed, Detail::None)).is_ok());
    assert!(j.get(1).unwrap().1.ever_committed);
    assert!(!JournalError::BadOrder.to_string().is_empty());
}

#[test]
fn journal_needs_plan_order_by_group() {
    let mut o = ops(&[
        (1, Kind::Rename, Root::Rejected),
        (2, Kind::Rename, Root::Rejected),
    ]);
    o[1].id = 1;
    assert_eq!(Journal::new(o).unwrap_err(), JournalError::DuplicateOp(1));
    let mut o = ops(&[
        (1, Kind::Rename, Root::Rejected),
        (2, Kind::Rename, Root::Rejected),
    ]);
    o[1].ord = 5;
    assert_eq!(Journal::new(o).unwrap_err(), JournalError::BadOrder);
    let o = ops(&[
        (1, Kind::Rename, Root::Rejected),
        (2, Kind::Rename, Root::Rejected),
        (1, Kind::Rename, Root::Rejected),
    ]);
    assert_eq!(Journal::new(o).unwrap_err(), JournalError::BadOrder);
    let j = Journal::new(ops(&[
        (1, Kind::Rename, Root::Rejected),
        (1, Kind::Rename, Root::Rejected),
        (4, Kind::Copy, Root::Dest),
    ]))
    .unwrap();
    assert_eq!(j.groups(), vec![0..2, 2..3]);
}

#[test]
fn from_plan_maps_every_operation_pending() {
    use crate::decisions::Decisions;
    use crate::grouping::{ScanEntry, ScanPolicy, group};
    use crate::plan::{Existing, KeepMode, KeepTo, PlanGroup, Settings, plan};
    let entries = ["A.JPG", "A.NEF", "B.JPG"].iter().map(|n| {
        let mut e = ScanEntry::file(&["100NIKON"], n);
        e.size = 5;
        e.mtime_ns = Some(MTIME);
        e
    });
    let groups: Vec<PlanGroup> = group(entries, &ScanPolicy::new(true))
        .groups
        .iter()
        .enumerate()
        .map(|(i, g)| PlanGroup::from_group(i as i64 + 1, g, 100 * (i as i64 + 1)))
        .collect();
    let mut d = Decisions::new();
    for (i, v) in [Verdict::Keep, Verdict::Reject].iter().enumerate() {
        let e = d
            .decide(i as i64 + 1, *v, &|_| false)
            .unwrap()
            .at(i as i64 + 1, 0);
        d.apply(&e).unwrap();
    }
    let p = plan(
        &groups,
        &d,
        &Settings {
            raw_follows: true,
            keep: Some(KeepTo {
                mode: KeepMode::Move,
                same_volume: false,
            }),
        },
        &Existing::new(),
    )
    .unwrap();
    let j = Journal::from_plan(&p, 40).unwrap();
    assert_eq!(j.ops().len(), 3);
    for (i, (o, po)) in j.ops().iter().zip(&p.ops).enumerate() {
        assert_eq!(o.id, 40 + i as i64);
        assert_eq!((o.group, o.member, o.root), (po.group, po.member, po.root));
        assert_eq!(o.kind, Kind::from(po.kind));
        assert_eq!(j.progress(i).state(), OpState::Pending);
    }
    // Rejects come first: one rename, then the kept pair as cross-volume moves.
    assert_eq!(j.ops()[0].kind, Kind::Rename);
    assert_eq!(j.ops()[1].kind, Kind::XMove);
    assert_eq!(j.groups().len(), 2);
    assert_eq!(
        j.ops()[1].partial(3).last().unwrap(),
        &format!("{}.41-3.culla-partial", j.ops()[1].dst.last().unwrap())
    );
}

// --- The recovery table ----------------------------------------------------------------------

/// The transitions a cell may make from `state`, all legal for `kind`.
fn legal_cell(kind: Kind, state: OpState, c: &Cell) -> bool {
    use OpState::*;
    let to = |s| state == s || is_legal(kind, state, s);
    match *c {
        Cell::Restart | Cell::RemovePRestart => to(Pending),
        Cell::Fail(_) | Cell::RemovePFail(_) => to(Failed),
        Cell::Reverify => is_legal(kind, state, Committed) && is_legal(kind, state, Failed),
        Cell::ToCommitted(n) => is_legal(kind, state, Committed) && legal_cell(kind, Committed, n),
        Cell::CommitP(n) => {
            let next = if state == Verifying { Committed } else { state };
            (state != Verifying || is_legal(kind, state, Committed)) && legal_cell(kind, next, n)
        }
        Cell::Continue => matches!(state, Committed),
        Cell::Finish => match kind {
            Kind::Copy => is_legal(kind, state, Done),
            Kind::XMove | Kind::Finish => {
                is_legal(kind, state, SourceRemoved) && is_legal(kind, SourceRemoved, Done)
            }
            Kind::Rename => false,
        },
        Cell::RemoveAgain => state == SourceRemoved && is_legal(kind, state, Done),
        Cell::ToDone => is_legal(kind, state, Done),
        Cell::IfP(a, b) | Cell::IfF(a, b) | Cell::IfFPlanned(a, b) => {
            legal_cell(kind, state, a) && legal_cell(kind, state, b)
        }
    }
}

fn states_of(kind: Kind) -> Vec<OpState> {
    use OpState::*;
    match kind {
        Kind::Rename => vec![Pending, Renaming, Committed],
        Kind::Copy => vec![Pending, Copying, Verifying, Committed],
        Kind::XMove => vec![Pending, Copying, Verifying, Committed, SourceRemoved],
        Kind::Finish => vec![Pending, Verifying, Committed, SourceRemoved],
    }
}

const KINDS: [Kind; 4] = [Kind::Rename, Kind::Copy, Kind::XMove, Kind::Finish];

#[test]
fn every_cell_exists_and_makes_only_legal_transitions() {
    for kind in KINDS {
        for state in OpState::ALL {
            for bits in 0..8u8 {
                let (s, p, f) = (bits & 4 != 0, bits & 2 != 0, bits & 1 != 0);
                let c = cell(kind, state, s, p, f);
                if states_of(kind).contains(&state) {
                    let c = c.unwrap_or_else(|| panic!("{kind:?} {state} {bits:03b}"));
                    assert!(
                        legal_cell(kind, state, c),
                        "{kind:?} {state} {bits:03b}: {c:?}"
                    );
                    // Without a partial, P makes no difference.
                    if !kind.has_partial() {
                        assert_eq!(Some(c), cell(kind, state, s, !p, f));
                    }
                } else {
                    assert!(c.is_none(), "{kind:?} {state}");
                }
            }
        }
    }
    // Spot checks against §10.2.
    assert_eq!(RENAME[1][2], Cell::Fail(FailReason::FsInconsistent));
    assert_eq!(COPY[1][1], Cell::Reverify);
    assert_eq!(COPY[6][3], Cell::ToDone);
    assert_eq!(FINISH_PENDING[1], Cell::Restart);
}

/// The steps that take an op of `kind` from `pending` to `state` (attempt 1 for copies).
fn path_to(op: &Op, state: OpState) -> Vec<Entry> {
    use OpState::*;
    let h = hash_of(10_000 + op.id as u64);
    let steps: Vec<(OpState, Detail)> = match op.kind {
        Kind::Rename => vec![(Renaming, Detail::None), (Committed, Detail::None)],
        Kind::Copy | Kind::XMove => vec![
            (Copying, Detail::Attempt { attempt: 1 }),
            (
                Verifying,
                Detail::Copied {
                    hash: h,
                    size: op.expected_size,
                },
            ),
            (Committed, Detail::None),
            (SourceRemoved, Detail::None),
        ],
        Kind::Finish => vec![
            (Verifying, Detail::None),
            (Committed, Detail::None),
            (SourceRemoved, Detail::None),
        ],
    };
    let mut out = Vec::new();
    let mut from = Pending;
    for (to, d) in steps {
        if from == state {
            break;
        }
        out.push(Entry {
            op: op.id,
            from,
            to,
            detail: d,
        });
        from = to;
    }
    out
}

/// Every set-up of the recovery table: kind, state, (S, P, F), and whether P and F match H1.
fn cell_setups() -> Vec<(Kind, OpState, bool, bool, bool, bool)> {
    let mut out = Vec::new();
    for kind in KINDS {
        for state in states_of(kind) {
            for bits in 0..8u8 {
                let (s, p, f) = (bits & 4 != 0, bits & 2 != 0, bits & 1 != 0);
                let attempts = path_to(&ops(&[(1, kind, Root::Dest)])[0], state)
                    .iter()
                    .any(|e| e.to == OpState::Copying);
                if p && !(kind.has_partial() && attempts) {
                    continue;
                }
                for matching in [true, false] {
                    out.push((kind, state, s, p, f, matching));
                }
            }
        }
    }
    out
}

/// A one-op world with the journal at `state` and the files as (S, P, F) say, durable.
fn cell_world(setup: (Kind, OpState, bool, bool, bool, bool), seed: u64) -> (Shared, Op, String) {
    cell_world_with(setup, seed, false)
}

/// As [`cell_world`]; with `unflushed`, F and the target's folder are left unflushed, as a
/// crashed earlier run that created them may leave them.
fn cell_world_with(
    (kind, state, s, p, f, matching): (Kind, OpState, bool, bool, bool, bool),
    seed: u64,
    unflushed: bool,
) -> (Shared, Op, String) {
    let root = if kind == Kind::Rename {
        Root::Rejected
    } else {
        Root::Dest
    };
    let o = ops(&[(1, kind, root)]);
    let op = o[0].clone();
    let w = World::new(Journal::new(o).unwrap(), seed);
    let c = content(&w, 1);
    {
        let mut wb = w.borrow_mut();
        for e in &path_to(&op, state) {
            wb.journal.apply(e).unwrap();
            wb.entries.push(e.clone());
        }
        if !s {
            wb.files.remove(&src_key(&op));
        }
        let rec = |content| FileRec {
            content,
            size: op.expected_size,
            mtime: op.src_mtime_ns,
        };
        if p {
            let pc = if matching { c } else { c ^ 1 };
            wb.files
                .insert(dst_key_of(op.root, &op.partial(1)), rec(pc));
        }
        if f {
            if matching && unflushed {
                wb.unflushed_change(dst_key(&op), rec(c));
            } else if matching {
                wb.files.insert(dst_key(&op), rec(c));
            } else {
                wb.put_foreign(dst_key(&op), 77);
            }
        }
        if unflushed {
            wb.unflushed_dir(dst_key_of(op.root, &op.dst[..op.dst.len() - 1]));
        }
        // INV-11 holds from here on only if the photo is still at S.
        wb.check_inv11 = s;
    }
    let what = format!(
        "{kind:?} {state} S{} P{} F{} {matching}{}",
        s as u8,
        p as u8,
        f as u8,
        if unflushed { " unflushed" } else { "" }
    );
    (w, op, what)
}

/// After a run of a cell set-up that ended: the operation is final (or a rename `committed`),
/// our content is still somewhere if it was anywhere, nothing of ours is left over while S is
/// there, and a second run does nothing.
fn check_cell_end(w: &Shared, j: &mut Journal, op: &Op, had_copy: bool, what: &str) {
    assert!(j.finished(), "{what}");
    let c = content(w, 1);
    if had_copy {
        let wb = w.borrow();
        assert!(
            wb.files.values().any(|r| r.content == c),
            "{what}: the photo is gone"
        );
    }
    if file(w, &src_key(op)) == Some(c) && err(j, 1) != Some(FailReason::FsInconsistent) {
        assert!(
            partial_files(w).is_empty(),
            "{what}: {:?}",
            partial_files(w)
        );
    }
    let (m, a) = (w.borrow().mutations, w.borrow().appends);
    assert_eq!(go(w, j), Ok(End::Finished), "{what}");
    assert_eq!((w.borrow().mutations, w.borrow().appends), (m, a), "{what}");
}

/// Runs every kind, state and (S, P, F) through the executor with the files set up as the row
/// says, P and F either matching H1 or not: no invariant of the fake breaks, the run ends with
/// the operation final (or a rename `committed`), a cell that fails touches nothing, and a
/// second run does nothing.
#[test]
fn every_cell_runs_on_the_fake_disk() {
    let mut outcomes: BTreeMap<String, u64> = BTreeMap::new();
    for setup in cell_setups() {
        let (kind, state, s, p, f, matching) = setup;
        let (w, op, what) = cell_world(setup, 1);
        let mut j = w.borrow().reload();
        let before = w.borrow().mutations;
        let end = go(&w, &mut j).unwrap_or_else(|e| panic!("{what}: {e}"));
        assert_eq!(end, End::Finished, "{what}");
        let st = state_of(&j);
        if let Some(Cell::Fail(r)) = cell(kind, state, s, p, f) {
            assert_eq!(
                w.borrow().mutations,
                before,
                "{what}: a failing cell touched"
            );
            assert_eq!((st, err(&j, 1)), (OpState::Failed, Some(*r)), "{what}");
        }
        check_cell_end(
            &w,
            &mut j,
            &op,
            s || (p && matching) || (f && matching),
            &what,
        );
        *outcomes
            .entry(format!("{st} {:?}", err(&j, 1)))
            .or_default() += 1;
    }
    for want in [
        "done None",
        "committed None",
        "failed Some(Conflict)",
        "failed Some(SourceMissing)",
        "failed Some(Lost)",
        "failed Some(FsInconsistent)",
        "failed Some(VerifyMismatch)",
    ] {
        assert!(
            outcomes.contains_key(want),
            "{want} never came up: {outcomes:?}"
        );
    }
}

/// Every cell again, with F and the target's folder left unflushed by a crashed earlier run:
/// the fake's checks hold (no source is removed before `flush_dirs` and the flush of F, even
/// for a folder this run did not create), and the run ends as with everything durable.
#[test]
fn every_cell_runs_with_an_unflushed_target_and_folder() {
    for setup in cell_setups() {
        let (kind, state, s, p, f, matching) = setup;
        let (w, op, what) = cell_world_with(setup, 1, true);
        let mut j = w.borrow().reload();
        let before = w.borrow().mutations;
        let end = go(&w, &mut j).unwrap_or_else(|e| panic!("{what}: {e}"));
        assert_eq!(end, End::Finished, "{what}");
        if let Some(Cell::Fail(r)) = cell(kind, state, s, p, f) {
            assert_eq!(
                w.borrow().mutations,
                before,
                "{what}: a failing cell touched"
            );
            assert_eq!(
                (state_of(&j), err(&j, 1)),
                (OpState::Failed, Some(*r)),
                "{what}"
            );
        }
        let log = w.borrow().log.clone();
        if let Some(r) = log.iter().position(|l| l == "remove_source 1") {
            assert!(log[..r].contains(&"flush_dirs 1".to_owned()), "{what}");
        }
        check_cell_end(
            &w,
            &mut j,
            &op,
            s || (p && matching) || (f && matching),
            &what,
        );
    }
}

/// Every cell, from every set-up, crashed at each of its disk calls and appends in turn (before
/// the call, half-way with its effect unflushed, after it), then resumed: the fake's checks
/// hold throughout (a source is removed only with F and its folders durable, the journal says
/// `done` or `source_removed` only with F durable, INV-01, INV-11, INV-12), and the resume ends
/// as an uninterrupted run would.
#[test]
fn every_cell_survives_a_crash_at_each_call() {
    let mut crashed_runs = 0u64;
    for setup in cell_setups() {
        let (_, _, s, p, f, matching) = setup;
        // Revert every unflushed change, none, or each at random (two seeds).
        for (seed, revert) in [(1, Some(true)), (1, Some(false)), (1, None), (2, None)] {
            for mode in [Mode::Before, Mode::Mid, Mode::After] {
                for k in 1.. {
                    let (w, op, what) = cell_world(setup, seed);
                    let what = format!("{what} {revert:?}/{seed} {mode:?} at call {k}");
                    {
                        let mut wb = w.borrow_mut();
                        wb.crash_mode = Some(mode);
                        wb.revert_all = revert;
                        wb.crash_at = Some(k);
                    }
                    let mut j = w.borrow().reload();
                    let _ = go(&w, &mut j);
                    if !w.borrow().crashed {
                        break;
                    }
                    crashed_runs += 1;
                    w.borrow_mut().crashed = false;
                    let mut j = w.borrow().reload();
                    let end = go(&w, &mut j).unwrap_or_else(|e| panic!("{what}: {e}"));
                    assert_eq!(end, End::Finished, "{what}");
                    check_cell_end(
                        &w,
                        &mut j,
                        &op,
                        s || (p && matching) || (f && matching),
                        &what,
                    );
                }
            }
        }
    }
    assert!(crashed_runs > 1000, "{crashed_runs}");
}

fn state_of(j: &Journal) -> OpState {
    j.progress(0).state()
}

#[test]
fn roll_forward_past_verifying_never_copies_again() {
    // testing.md §6.2: a matching partial at `verifying`, or `committed`, ends done without a
    // new copy.
    for state in [OpState::Verifying, OpState::Committed] {
        let o = ops(&[(1, Kind::XMove, Root::Dest)]);
        let op = o[0].clone();
        let w = world(o);
        {
            let mut wb = w.borrow_mut();
            for e in path_to(&op, state) {
                wb.journal.apply(&e).unwrap();
                wb.entries.push(e);
            }
            let s = wb.files[&src_key(&op)];
            wb.files.insert(dst_key_of(op.root, &op.partial(1)), s);
        }
        let mut j = w.borrow().reload();
        assert_eq!(go(&w, &mut j).unwrap(), End::Finished);
        assert_eq!(state_of(&j), OpState::Done);
        let log = w.borrow().log.clone();
        assert!(!log.iter().any(|l| l.starts_with("copy")), "{log:?}");
        assert_eq!(file(&w, &dst_key(&op)), Some(content(&w, 1)));
        assert_eq!(file(&w, &src_key(&op)), None);
    }
}

// --- Normal execution ------------------------------------------------------------------------

#[test]
fn us17_us18_each_kind_ends_where_it_should() {
    let o = ops(&[
        (1, Kind::Rename, Root::Rejected),
        (1, Kind::Rename, Root::Rejected),
        (2, Kind::Copy, Root::Dest),
        (3, Kind::XMove, Root::Dest),
        (3, Kind::XMove, Root::Dest),
    ]);
    let all = o.clone();
    let w = world(o);
    let mut j = w.borrow().reload();
    assert_eq!(go(&w, &mut j).unwrap(), End::Finished);
    assert!(j.finished());
    // Renames wait at committed until the plan closes.
    assert_eq!(state(&j, 1), OpState::Committed);
    close(&mut j, &mut FakeStore(w.clone())).unwrap();
    for op in &all {
        let c = content(&w, op.id);
        assert_eq!(state(&j, op.id), OpState::Done, "{}", op.id);
        assert_eq!(file(&w, &dst_key(op)), Some(c));
        let s = file(&w, &src_key(op));
        assert_eq!(s, (op.kind == Kind::Copy).then_some(c));
    }
    assert!(partial_files(&w).is_empty());
    // The verify step's proof is used: no second read, and no source removed before both
    // members are committed.
    let log = w.borrow().log.clone();
    assert!(!log.iter().any(|l| l.starts_with("prove")), "{log:?}");
    let first_removal = log
        .iter()
        .position(|l| l.starts_with("remove_source"))
        .unwrap();
    let last_commit = log.iter().rposition(|l| l == "commit 5").unwrap();
    assert!(last_commit < first_removal);
    // Closing again appends nothing.
    let a = w.borrow().appends;
    close(&mut j, &mut FakeStore(w.clone())).unwrap();
    assert_eq!(w.borrow().appends, a);
}

#[test]
fn us19_a_locked_member_fails_its_group_and_keeps_every_source() {
    let o = ops(&[(1, Kind::XMove, Root::Dest), (1, Kind::XMove, Root::Dest)]);
    let w = world(o.clone());
    w.borrow_mut().locked.insert(2);
    let mut j = w.borrow().reload();
    assert_eq!(go(&w, &mut j).unwrap(), End::Finished);
    assert_eq!(state(&j, 1), OpState::CopiedSourceKept);
    assert_eq!(
        (state(&j, 2), err(&j, 2)),
        (OpState::Failed, Some(FailReason::Locked))
    );
    assert_eq!(file(&w, &src_key(&o[0])), Some(content(&w, 1)));
    assert_eq!(file(&w, &dst_key(&o[0])), Some(content(&w, 1)));
    assert_eq!(file(&w, &src_key(&o[1])), Some(content(&w, 2)));
    assert!(partial_files(&w).is_empty());
    assert!(
        !w.borrow()
            .log
            .iter()
            .any(|l| l.starts_with("remove_source"))
    );
}

#[test]
fn a_failed_member_stops_the_rest_of_its_group_before_they_start() {
    // The second member's target is taken when the group is looked at: the first never starts.
    let o = ops(&[
        (1, Kind::Rename, Root::Rejected),
        (1, Kind::Rename, Root::Rejected),
    ]);
    let w = world(o.clone());
    w.borrow_mut().put_foreign(dst_key(&o[1]), 5);
    let mut j = w.borrow().reload();
    go(&w, &mut j).unwrap();
    assert_eq!(
        (state(&j, 1), err(&j, 1)),
        (OpState::Failed, Some(FailReason::GroupFailed))
    );
    assert_eq!(
        (state(&j, 2), err(&j, 2)),
        (OpState::Failed, Some(FailReason::Conflict))
    );
    assert_eq!(w.borrow().mutations, 0);
}

#[test]
fn us17_a_target_that_appears_at_the_rename_is_a_hard_error() {
    // A split rename group: the first member moved, the second met a new file at its target.
    let o = ops(&[
        (1, Kind::Rename, Root::Rejected),
        (1, Kind::Rename, Root::Rejected),
    ]);
    let w = world(o.clone());
    w.borrow_mut().foreign_late.insert(2);
    let mut j = w.borrow().reload();
    go(&w, &mut j).unwrap();
    close(&mut j, &mut FakeStore(w.clone())).unwrap();
    assert_eq!(state(&j, 1), OpState::Done);
    assert_eq!(
        (state(&j, 2), err(&j, 2)),
        (OpState::Failed, Some(FailReason::Conflict))
    );
    assert_eq!(file(&w, &src_key(&o[1])), Some(content(&w, 2)));
    assert_eq!(file(&w, &dst_key(&o[1])), Some(7));
}

#[test]
fn us18_a_target_taken_before_the_final_rename_removes_the_partial() {
    for kind in [Kind::Copy, Kind::XMove] {
        let o = ops(&[(1, kind, Root::Dest)]);
        let w = world(o.clone());
        w.borrow_mut().foreign_late.insert(1);
        let mut j = w.borrow().reload();
        go(&w, &mut j).unwrap();
        assert_eq!(err(&j, 1), Some(FailReason::Conflict));
        assert!(partial_files(&w).is_empty());
        assert_eq!(file(&w, &src_key(&o[0])), Some(content(&w, 1)));
        assert_eq!(file(&w, &dst_key(&o[0])), Some(7));
    }
}

#[test]
fn us18_a_hash_mismatch_keeps_the_source_and_removes_the_partial() {
    let o = ops(&[(1, Kind::XMove, Root::Dest), (1, Kind::XMove, Root::Dest)]);
    let w = world(o.clone());
    w.borrow_mut().flaky.insert(1);
    let mut j = w.borrow().reload();
    go(&w, &mut j).unwrap();
    assert_eq!(err(&j, 1), Some(FailReason::VerifyMismatch));
    assert_eq!(err(&j, 2), Some(FailReason::GroupFailed));
    assert!(partial_files(&w).is_empty());
    for op in &o {
        assert_eq!(file(&w, &src_key(op)), Some(content(&w, op.id)));
        assert_eq!(file(&w, &dst_key(op)), None);
    }
}

#[test]
fn a_full_disk_pauses_with_the_operation_back_at_pending() {
    let o = ops(&[(1, Kind::Copy, Root::Dest)]);
    let w = world(o.clone());
    w.borrow_mut().full.insert(1);
    let mut j = w.borrow().reload();
    assert_eq!(go(&w, &mut j).unwrap(), End::Paused(PauseReason::DiskFull));
    assert_eq!(state(&j, 1), OpState::Pending);
    assert!(partial_files(&w).is_empty());
    // Space freed: resume finishes with attempt 2.
    assert_eq!(go(&w, &mut j).unwrap(), End::Finished);
    assert_eq!(state(&j, 1), OpState::Done);
    assert_eq!(j.get(1).unwrap().1.attempts, 2);
    assert!(partial_files(&w).is_empty());
}

#[test]
fn us20_a_pulled_card_pauses_and_resume_finishes() {
    let o = ops(&[(1, Kind::XMove, Root::Dest), (1, Kind::XMove, Root::Dest)]);
    let w = world(o.clone());
    w.borrow_mut().pull.insert((2, "copy"));
    let mut j = w.borrow().reload();
    assert_eq!(
        go(&w, &mut j).unwrap(),
        End::Paused(PauseReason::VolumeLost)
    );
    assert_eq!(state(&j, 1), OpState::Committed);
    // The card is back: the first member's proof is gone, so it is proved again.
    assert_eq!(go(&w, &mut j).unwrap(), End::Finished);
    assert_eq!(state(&j, 1), OpState::Done);
    assert_eq!(state(&j, 2), OpState::Done);
    assert!(w.borrow().log.iter().any(|l| l == "prove 1"));
}

#[test]
fn cancel_cuts_a_cross_volume_group_and_keeps_both_copies() {
    let o = ops(&[
        (1, Kind::XMove, Root::Dest),
        (1, Kind::XMove, Root::Dest),
        (2, Kind::Copy, Root::Dest),
    ]);
    let w = world(o.clone());
    let mut j = w.borrow().reload();
    let mut asks = 0;
    let mut halt = || {
        asks += 1;
        // Before group 1, before member 1, before member 2: cancel there.
        (asks == 3).then_some(Halt::Cancel)
    };
    let end = run(
        &mut j,
        &mut FakeDisk(w.clone()),
        &mut FakeStore(w.clone()),
        &mut halt,
    )
    .unwrap();
    assert_eq!(end, End::Cancelled);
    assert_eq!(state(&j, 1), OpState::CopiedSourceKept);
    assert_eq!(
        (state(&j, 2), err(&j, 2)),
        (OpState::Failed, Some(FailReason::GroupFailed))
    );
    assert_eq!(state(&j, 3), OpState::Pending);
    close(&mut j, &mut FakeStore(w.clone())).unwrap();
}

#[test]
fn pause_stops_between_groups() {
    let o = ops(&[(1, Kind::Copy, Root::Dest), (2, Kind::Copy, Root::Dest)]);
    let w = world(o);
    let mut j = w.borrow().reload();
    let mut asks = 0;
    let mut halt = || {
        asks += 1;
        (asks == 3).then_some(Halt::Pause)
    };
    let end = run(
        &mut j,
        &mut FakeDisk(w.clone()),
        &mut FakeStore(w.clone()),
        &mut halt,
    )
    .unwrap();
    assert_eq!(end, End::Paused(PauseReason::User));
    assert_eq!(state(&j, 1), OpState::Done);
    assert_eq!(state(&j, 2), OpState::Pending);
    assert_eq!(go(&w, &mut j).unwrap(), End::Finished);
    assert_eq!(state(&j, 2), OpState::Done);
}

#[test]
fn a_changed_source_fails_without_touching_anything() {
    let o = ops(&[(1, Kind::Rename, Root::Rejected)]);
    let w = world(o.clone());
    w.borrow_mut().files.get_mut(&src_key(&o[0])).unwrap().size += 1;
    let mut j = w.borrow().reload();
    go(&w, &mut j).unwrap();
    assert_eq!(err(&j, 1), Some(FailReason::SourceChanged));
    assert_eq!(w.borrow().mutations, 0);
}

#[test]
fn finish_operations_remove_a_source_that_came_back() {
    let o = ops(&[(1, Kind::Finish, Root::Dest)]);
    let w = world(o.clone());
    {
        let mut wb = w.borrow_mut();
        let s = wb.files[&src_key(&o[0])];
        wb.files.insert(dst_key(&o[0]), s);
    }
    let mut j = w.borrow().reload();
    assert_eq!(go(&w, &mut j).unwrap(), End::Finished);
    assert_eq!(state(&j, 1), OpState::Done);
    assert_eq!(file(&w, &src_key(&o[0])), None);
    // A target that no longer matches keeps both.
    let w = world(o.clone());
    w.borrow_mut().put_foreign(dst_key(&o[0]), 3);
    let mut j = w.borrow().reload();
    go(&w, &mut j).unwrap();
    assert_eq!(err(&j, 1), Some(FailReason::VerifyMismatch));
    assert_eq!(file(&w, &src_key(&o[0])), Some(content(&w, 1)));
}

#[test]
fn store_failure_stops_the_run() {
    let o = ops(&[(1, Kind::Copy, Root::Dest)]);
    let w = world(o);
    w.borrow_mut().crash_at = Some(2);
    let mut j = w.borrow().reload();
    // Call 1 is the probe, call 2 the first append.
    match go(&w, &mut j) {
        Err(RunError::Store(_)) | Ok(End::Paused(PauseReason::VolumeLost)) => {}
        other => panic!("{other:?}"),
    }
    assert!(w.borrow().crashed);
}

#[test]
fn reverted_moves_after_the_plan_closed() {
    let o = ops(&[
        (1, Kind::Rename, Root::Rejected),
        (2, Kind::XMove, Root::Dest),
        (3, Kind::Copy, Root::Dest),
    ]);
    let done = Progress {
        state: Some(OpState::Done),
        ..Progress::default()
    };
    let f = |size| {
        Some(FileSeen {
            file: true,
            size,
            mtime_ns: Some(MTIME),
        })
    };
    let seen = |s: bool, t: bool, op: &Op| Seen {
        rule: TimeRule::Exact,
        source: if s { f(op.expected_size) } else { None },
        target: if t { f(op.expected_size) } else { None },
        partials: vec![],
    };
    assert_eq!(
        reverted(&o[0], &done, &seen(true, false, &o[0])),
        Some(Reverted::Rename)
    );
    assert_eq!(
        reverted(&o[0], &done, &seen(true, true, &o[0])),
        Some(Reverted::Torn)
    );
    assert_eq!(
        reverted(&o[0], &done, &seen(false, false, &o[0])),
        Some(Reverted::Torn)
    );
    assert_eq!(reverted(&o[0], &done, &seen(false, true, &o[0])), None);
    assert_eq!(
        reverted(&o[1], &done, &seen(true, true, &o[1])),
        Some(Reverted::SourceBack)
    );
    assert_eq!(reverted(&o[1], &done, &seen(false, true, &o[1])), None);
    assert_eq!(reverted(&o[2], &done, &seen(true, true, &o[2])), None);
    assert_eq!(
        reverted(&o[0], &Progress::default(), &seen(true, false, &o[0])),
        None
    );
}

// --- Seeded crash loop -----------------------------------------------------------------------

/// Cases per property: `CULLA_PROP_CASES` if set (10,000 for a long run), else 300.
fn cases() -> u64 {
    std::env::var("CULLA_PROP_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300)
}

fn random_ops(rng: &mut Rng) -> Vec<Op> {
    let mut spec = Vec::new();
    let groups = 1 + rng.below(4);
    for g in 0..groups {
        let (kind, root) = match rng.below(4) {
            0 => (Kind::Rename, Root::Rejected),
            1 => (Kind::Rename, Root::Dest),
            2 => (Kind::Copy, Root::Dest),
            _ => (Kind::XMove, Root::Dest),
        };
        for _ in 0..1 + rng.below(3) {
            spec.push((g as GroupId + 1, kind, root));
        }
    }
    ops(&spec)
}

fn crash_loop(case: u64, seen: &mut BTreeMap<String, u64>) {
    let mut rng = Rng::new(case + 1);
    let all = random_ops(&mut rng);
    let w = World::new(Journal::new(all.clone()).unwrap(), case + 7);
    {
        let mut wb = w.borrow_mut();
        for op in &all {
            match rng.below(25) {
                0 => wb.put_foreign(dst_key(op), 7),
                1 => {
                    wb.foreign_late.insert(op.id);
                }
                2 => {
                    wb.locked.insert(op.id);
                }
                3 if op.kind.has_partial() => {
                    wb.flaky.insert(op.id);
                }
                _ => {}
            }
        }
        // Files nobody decided on.
        for k in 0..3 {
            wb.put_foreign(format!("S:100NIKON\\UNDECIDED_{k}.JPG"), 500 + k);
        }
    }
    let mut crashes = rng.below(7);
    let mut j = w.borrow().reload();
    let mut runs = 0;
    loop {
        runs += 1;
        assert!(runs < 100, "case {case}: no progress");
        {
            let mut wb = w.borrow_mut();
            wb.crash_at = (crashes > 0).then(|| wb.calls + 1 + rng.below(25));
        }
        let res = go(&w, &mut j);
        if w.borrow().crashed {
            w.borrow_mut().crashed = false;
            crashes -= 1;
            // A new process: the journal as stored.
            j = w.borrow().reload();
            continue;
        }
        assert_eq!(res, Ok(End::Finished), "case {case}");
        break;
    }
    w.borrow_mut().crash_at = None;
    *seen.entry("crashes".to_owned()).or_default() += w.borrow().crashes;
    check_end_state(&format!("case {case}"), &w, j, &all, seen);
}

fn seen_files(wb: &World, op: &Op) -> Seen {
    let at = |k: &str| {
        wb.files.get(k).map(|r| FileSeen {
            file: true,
            size: r.size,
            mtime_ns: r.mtime,
        })
    };
    Seen {
        rule: TimeRule::Exact,
        source: at(&src_key(op)),
        target: at(&dst_key(op)),
        partials: vec![],
    }
}

/// testing.md §6.1 on a run that ended: the journal is the store's, closing and a second resume
/// change nothing, every op is in an end state with its files where that state says, the
/// launch check finds nothing but lost source removals, no stray files, undecided files as
/// they were.
fn check_end_state(
    case: &str,
    w: &Shared,
    mut j: Journal,
    all: &[Op],
    seen: &mut BTreeMap<String, u64>,
) {
    assert_eq!(j, w.borrow().reload(), "{case}: journal and store differ");
    close(&mut j, &mut FakeStore(w.clone())).unwrap();
    // Idempotent: a second resume does nothing.
    let (m, a) = (w.borrow().mutations, w.borrow().appends);
    assert_eq!(go(w, &mut j), Ok(End::Finished));
    assert_eq!(
        (w.borrow().mutations, w.borrow().appends),
        (m, a),
        "{case}: resume again changed something"
    );
    w.borrow_mut().flush_all();

    // End state per op (testing.md §6.1).
    let wb = w.borrow();
    let mut ours: BTreeSet<String> = BTreeSet::new();
    for (i, op) in all.iter().enumerate() {
        let p = j.progress(i);
        let c = wb.content[&op.id];
        let at = |k: &str| wb.files.get(k).map(|r| r.content);
        let s = at(&src_key(op));
        let f = at(&dst_key(op));
        let what = format!(
            "{case}: op {} {:?} {} {:?}\nLOG {:?}\nENTRIES {:?}",
            op.id,
            op.kind,
            p.state(),
            p.last_error,
            wb.log,
            wb.entries
                .iter()
                .filter(|e| e.op == op.id)
                .map(|e| (e.from, e.to))
                .collect::<Vec<_>>()
        );
        match (p.state(), op.kind) {
            (OpState::Done, Kind::Rename) => {
                assert_eq!((s, f), (None, Some(c)), "{what}");
            }
            (OpState::Done, Kind::Copy) => assert_eq!((s, f), (Some(c), Some(c)), "{what}"),
            (OpState::Done, Kind::XMove | Kind::Finish) => {
                assert_eq!(f, Some(c), "{what}");
                if s.is_some() {
                    // A source removal the file system lost after `done` (§9.6): found by the
                    // launch check and offered as "Finish moves".
                    assert_eq!(
                        reverted(op, p, &seen_files(&wb, op)),
                        Some(Reverted::SourceBack),
                        "{what}"
                    );
                    *seen.entry("source back after done".to_owned()).or_default() += 1;
                }
            }
            (OpState::Failed, _) => {
                let r = p.last_error.unwrap();
                assert!(
                    matches!(
                        r,
                        FailReason::Locked
                            | FailReason::Conflict
                            | FailReason::VerifyMismatch
                            | FailReason::GroupFailed
                    ),
                    "{what}"
                );
                assert_eq!(s, Some(c), "{what}: the source must stay");
                assert_ne!(f, Some(c), "{what}");
            }
            (OpState::CopiedSourceKept, Kind::XMove) => {
                assert_eq!((s, f), (Some(c), Some(c)), "{what}");
            }
            _ => panic!("{what}: not an end state"),
        }
        // The launch check on the plan just closed: nothing reverted, lost or torn, except a
        // lost source removal (above).
        if !(matches!(op.kind, Kind::XMove | Kind::Finish) && s.is_some()) {
            assert_eq!(reverted(op, p, &seen_files(&wb, op)), None, "{what}");
        }
        *seen
            .entry(format!("{:?} {} {:?}", op.kind, p.state(), p.last_error))
            .or_default() += 1;
        ours.insert(src_key(op));
        ours.insert(dst_key(op));
        if op.kind.has_partial() {
            for a in 1..=p.attempts {
                ours.insert(dst_key_of(op.root, &op.partial(a)));
            }
        }
    }
    // No stray files: only sources, targets, foreign files, and partials of ops whose source is
    // gone (kept and reported).
    for k in wb.files.keys() {
        if wb.foreign.contains_key(k) {
            continue;
        }
        assert!(ours.contains(k), "{case}: stray file {k}");
        if k.ends_with(".culla-partial") {
            let (i, op) = all
                .iter()
                .enumerate()
                .find(|(i, op)| {
                    (1..=j.progress(*i).attempts).any(|a| dst_key_of(op.root, &op.partial(a)) == *k)
                })
                .unwrap();
            let _ = i;
            assert!(
                !wb.files.contains_key(&src_key(op)),
                "{case}: partial {k} left with its source there"
            );
            *seen
                .entry("partial kept, source gone".to_owned())
                .or_default() += 1;
        }
    }
    // The undecided files are as they were (checked on every call too).
    for k in 0..3u64 {
        assert_eq!(
            wb.files
                .get(&format!("S:100NIKON\\UNDECIDED_{k}.JPG"))
                .map(|r| r.content),
            Some(500 + k)
        );
    }
}

fn run_crash_loop(n: u64) -> BTreeMap<String, u64> {
    let mut seen = BTreeMap::new();
    for case in 0..n {
        crash_loop(case, &mut seen);
    }
    seen
}

#[test]
fn prop_crash_loop_reaches_the_end_state() {
    let n = cases();
    let seen = run_crash_loop(n);
    if n >= 300 {
        assert!(seen["crashes"] > n, "{seen:?}");
        for want in [
            "Rename done None",
            "Copy done None",
            "XMove done None",
            "XMove copied_source_kept None",
            "Rename failed Some(GroupFailed)",
            "XMove failed Some(Locked)",
            "Copy failed Some(Conflict)",
            "XMove failed Some(VerifyMismatch)",
        ] {
            assert!(seen.contains_key(want), "{want} never came up: {seen:?}");
        }
    }
}

#[test]
#[ignore = "long run: 10,000 cases"]
fn prop_crash_loop_10k() {
    run_crash_loop(10_000);
}

// --- Exhaustive crash points over small plans ----------------------------------------------

/// Small plans, each kind alone and the groups that matter (a cross-volume pair, a split
/// rename group, a copy next to a move).
fn small_plans() -> Vec<Vec<Op>> {
    vec![
        ops(&[(1, Kind::Rename, Root::Rejected)]),
        ops(&[(1, Kind::Copy, Root::Dest)]),
        ops(&[(1, Kind::XMove, Root::Dest)]),
        ops(&[(1, Kind::Finish, Root::Dest)]),
        ops(&[(1, Kind::XMove, Root::Dest), (1, Kind::XMove, Root::Dest)]),
        ops(&[
            (1, Kind::Rename, Root::Rejected),
            (1, Kind::Rename, Root::Rejected),
            (2, Kind::Copy, Root::Dest),
        ]),
        ops(&[(1, Kind::Copy, Root::Dest), (2, Kind::XMove, Root::Dest)]),
    ]
}

/// A fresh world for a small plan; a `finish` op gets its target in place, as a lost source
/// removal leaves it.
fn small_world(all: &[Op], seed: u64) -> Shared {
    let w = World::new(Journal::new(all.to_vec()).unwrap(), seed);
    {
        let mut wb = w.borrow_mut();
        for op in all.iter().filter(|o| o.kind == Kind::Finish) {
            let s = wb.files[&src_key(op)];
            wb.files.insert(dst_key(op), s);
        }
        for k in 0..3 {
            wb.put_foreign(format!("S:100NIKON\\UNDECIDED_{k}.JPG"), 500 + k);
        }
    }
    w
}

/// Runs to the end, crashing at the calls in `at` (one per process run, counted from that run's
/// first call): true when every crash fired.
fn run_with_crashes(w: &Shared, at: &[(u64, Mode)]) -> (Journal, bool) {
    let mut j = w.borrow().reload();
    for &(k, mode) in at {
        {
            let mut wb = w.borrow_mut();
            wb.crash_mode = Some(mode);
            wb.crash_at = Some(wb.calls + k);
        }
        let _ = go(w, &mut j);
        if !w.borrow().crashed {
            return (j, false);
        }
        w.borrow_mut().crashed = false;
        j = w.borrow().reload();
    }
    assert_eq!(go(w, &mut j), Ok(End::Finished));
    (j, true)
}

/// Every crash point k (each of the three modes; all unflushed changes reverted, none, or each
/// at random), then every second crash point k2 in the recovery that follows: each ends in the
/// end state (file-engine §1, proposed: exhaustive rather than random crash points).
#[test]
fn exhaustive_crash_at_every_call_of_small_plans() {
    const MODES: [Mode; 3] = [Mode::Before, Mode::Mid, Mode::After];
    let mut seen = BTreeMap::new();
    let mut runs = 0u64;
    for (n, all) in small_plans().into_iter().enumerate() {
        for (seed, revert) in [(1, Some(true)), (1, Some(false)), (3, None)] {
            for m1 in MODES {
                for k1 in 1.. {
                    let w = small_world(&all, seed);
                    w.borrow_mut().revert_all = revert;
                    let (j, fired) = run_with_crashes(&w, &[(k1, m1)]);
                    if !fired {
                        break;
                    }
                    runs += 1;
                    check_end_state(
                        &format!("plan {n} {revert:?} {m1:?}@{k1}"),
                        &w,
                        j,
                        &all,
                        &mut seen,
                    );
                    // A second crash: `after` call k2 is `before` call k2 + 1, so two modes
                    // cover it; with nothing reverted it adds little, so it is skipped then.
                    if revert == Some(false) {
                        continue;
                    }
                    for m2 in [Mode::Before, Mode::Mid] {
                        for k2 in 1.. {
                            let w = small_world(&all, seed);
                            w.borrow_mut().revert_all = revert;
                            let (j, fired) = run_with_crashes(&w, &[(k1, m1), (k2, m2)]);
                            if !fired {
                                break;
                            }
                            runs += 1;
                            check_end_state(
                                &format!("plan {n} {revert:?} {m1:?}@{k1} {m2:?}@{k2}"),
                                &w,
                                j,
                                &all,
                                &mut seen,
                            );
                        }
                    }
                }
            }
        }
    }
    assert!(runs > 10_000, "{runs}");
}

// --- Review fixes ------------------------------------------------------------------------------

#[test]
fn a_partial_is_kept_when_the_source_vanishes_during_its_step() {
    // INV-01: after a failed copy or verify the partial may be the only copy; it is removed
    // only if a fresh probe finds S there as planned.
    for step in ["copy", "verify"] {
        let o = ops(&[(1, Kind::XMove, Root::Dest)]);
        let w = world(o.clone());
        {
            let mut wb = w.borrow_mut();
            wb.vanish.insert((1, step));
            // S goes away by another hand; INV-11 cannot hold for this op.
            wb.check_inv11 = false;
        }
        let mut j = w.borrow().reload();
        assert_eq!(go(&w, &mut j).unwrap(), End::Finished, "{step}");
        assert_eq!(
            (state(&j, 1), err(&j, 1)),
            (OpState::Failed, Some(FailReason::Io(2))),
            "{step}"
        );
        assert_eq!(
            partial_files(&w),
            vec![dst_key_of(o[0].root, &o[0].partial(1))],
            "{step}"
        );
        if step == "verify" {
            // The partial holds the photo: the only copy left, kept and reported.
            assert_eq!(
                file(&w, &dst_key_of(o[0].root, &o[0].partial(1))),
                Some(content(&w, 1))
            );
        }
        assert!(
            !w.borrow()
                .log
                .iter()
                .any(|l| l.starts_with("remove_partial"))
        );
    }
}

#[test]
fn a_full_disk_outside_an_attempt_pauses_without_a_journal_entry() {
    // committed, S1 P1 F0 (the rename reverted): the commit meets a full disk.
    let o = ops(&[(1, Kind::Copy, Root::Dest)]);
    let op = o[0].clone();
    let w = world(o);
    {
        let mut wb = w.borrow_mut();
        for e in path_to(&op, OpState::Committed) {
            wb.journal.apply(&e).unwrap();
            wb.entries.push(e);
        }
        let s = wb.files[&src_key(&op)];
        wb.files.insert(dst_key_of(op.root, &op.partial(1)), s);
        wb.fail_with.insert((1, "commit"), DiskError::DiskFull);
    }
    let mut j = w.borrow().reload();
    let appends = w.borrow().appends;
    assert_eq!(go(&w, &mut j).unwrap(), End::Paused(PauseReason::DiskFull));
    assert_eq!(state(&j, 1), OpState::Committed);
    assert_eq!(w.borrow().appends, appends);
    assert_eq!(partial_files(&w).len(), 1);
    assert_eq!(go(&w, &mut j).unwrap(), End::Finished);
    assert_eq!(state(&j, 1), OpState::Done);
    assert!(partial_files(&w).is_empty());
}

/// A two-member cross-volume group: member 1 at `source_removed` with its source back (the
/// removal reverted), member 2 not started.
fn source_back_world() -> (Shared, Vec<Op>) {
    let o = ops(&[(1, Kind::XMove, Root::Dest), (1, Kind::XMove, Root::Dest)]);
    let w = world(o.clone());
    {
        let mut wb = w.borrow_mut();
        for e in path_to(&o[0], OpState::SourceRemoved) {
            wb.journal.apply(&e).unwrap();
            wb.entries.push(e);
        }
        let s = wb.files[&src_key(&o[0])];
        wb.files.insert(dst_key(&o[0]), s);
    }
    (w, o)
}

#[test]
fn a_source_back_at_source_removed_waits_for_its_group() {
    let (w, o) = source_back_world();
    let mut j = w.borrow().reload();
    assert_eq!(go(&w, &mut j).unwrap(), End::Finished);
    assert_eq!((state(&j, 1), state(&j, 2)), (OpState::Done, OpState::Done));
    for op in &o {
        assert_eq!(file(&w, &src_key(op)), None);
    }
    let log = w.borrow().log.clone();
    let removal = log.iter().position(|l| l == "remove_source 1").unwrap();
    let commit = log.iter().position(|l| l == "commit 2").unwrap();
    assert!(commit < removal, "{log:?}");
    // Every member's folders are flushed before the first removal.
    let first = log
        .iter()
        .position(|l| l.starts_with("remove_source"))
        .unwrap();
    for id in [1, 2] {
        let f = log
            .iter()
            .position(|l| *l == format!("flush_dirs {id}"))
            .unwrap();
        assert!(f < first, "{log:?}");
    }
}

#[test]
fn a_failed_folder_flush_removes_no_source_of_its_group() {
    let o = ops(&[(1, Kind::XMove, Root::Dest), (1, Kind::XMove, Root::Dest)]);
    let w = world(o.clone());
    w.borrow_mut()
        .fail_with
        .insert((2, "flush_dirs"), DiskError::AccessDenied);
    let mut j = w.borrow().reload();
    assert_eq!(go(&w, &mut j).unwrap(), End::Finished);
    // Member 1's copy is committed: it is kept, and so is its source.
    assert_eq!(state(&j, 1), OpState::CopiedSourceKept);
    assert_eq!(
        (state(&j, 2), err(&j, 2)),
        (OpState::Failed, Some(FailReason::AccessDenied))
    );
    for (i, op) in o.iter().enumerate() {
        let id = i as OpId + 1;
        assert_eq!(file(&w, &src_key(op)), Some(content(&w, id)));
    }
    let log = w.borrow().log.clone();
    assert!(log.contains(&"flush_dirs 1".to_owned()), "{log:?}");
    assert!(
        !log.iter().any(|l| l.starts_with("remove_source")),
        "{log:?}"
    );
}

#[test]
fn a_source_back_at_source_removed_is_kept_when_its_group_fails() {
    let (w, o) = source_back_world();
    w.borrow_mut().locked.insert(2);
    let mut j = w.borrow().reload();
    assert_eq!(go(&w, &mut j).unwrap(), End::Finished);
    assert_eq!(
        (state(&j, 1), err(&j, 1)),
        (OpState::Failed, Some(FailReason::GroupFailed))
    );
    assert_eq!(err(&j, 2), Some(FailReason::Locked));
    assert_eq!(file(&w, &src_key(&o[0])), Some(content(&w, 1)));
    assert_eq!(file(&w, &dst_key(&o[0])), Some(content(&w, 1)));
    assert!(
        !w.borrow()
            .log
            .iter()
            .any(|l| l.starts_with("remove_source"))
    );
}

#[test]
fn a_stopped_run_releases_every_proof() {
    let o = ops(&[(1, Kind::XMove, Root::Dest), (1, Kind::XMove, Root::Dest)]);
    let w = world(o);
    w.borrow_mut().pull.insert((2, "commit"));
    let mut j = w.borrow().reload();
    assert_eq!(
        go(&w, &mut j).unwrap(),
        End::Paused(PauseReason::VolumeLost)
    );
    // Member 1 was verified (its proof held) and committed before the pull.
    assert_eq!(state(&j, 1), OpState::Committed);
    assert!(w.borrow().proofs.is_empty(), "{:?}", w.borrow().proofs);
}

#[test]
fn a_finish_whose_target_is_gone_is_lost_from_every_state() {
    for state in [
        OpState::Pending,
        OpState::Verifying,
        OpState::Committed,
        OpState::SourceRemoved,
    ] {
        assert_eq!(
            cell(Kind::Finish, state, true, false, false),
            Some(&Cell::Fail(FailReason::Lost)),
            "{state}"
        );
    }
    // An `xmove` in the same row starts again: S is there and nothing else.
    assert_eq!(
        cell(Kind::XMove, OpState::Verifying, true, false, false),
        Some(&Cell::Restart)
    );
}

#[test]
fn the_launch_check_reports_a_lost_target() {
    let o = ops(&[
        (1, Kind::Rename, Root::Rejected),
        (2, Kind::XMove, Root::Dest),
        (3, Kind::Finish, Root::Dest),
    ]);
    let done = Progress {
        state: Some(OpState::Done),
        ..Progress::default()
    };
    let at = |size, mtime| {
        Some(FileSeen {
            file: true,
            size,
            mtime_ns: Some(mtime),
        })
    };
    let seen = |s: Option<FileSeen>, t: Option<FileSeen>| Seen {
        rule: TimeRule::Exact,
        source: s,
        target: t,
        partials: vec![],
    };
    for op in &o[1..] {
        let planned = at(op.expected_size, MTIME);
        assert_eq!(
            reverted(op, &done, &seen(planned, None)),
            Some(Reverted::Lost)
        );
        assert_eq!(reverted(op, &done, &seen(None, None)), Some(Reverted::Lost));
        assert_eq!(reverted(op, &done, &seen(None, planned)), None);
    }
    // A rename's source back, but changed: not offered as a redo.
    let r = &o[0];
    assert_eq!(
        reverted(r, &done, &seen(at(r.expected_size + 1, MTIME), None)),
        Some(Reverted::Lost)
    );
    assert_eq!(
        reverted(r, &done, &seen(at(r.expected_size, MTIME), None)),
        Some(Reverted::Rename)
    );
}
