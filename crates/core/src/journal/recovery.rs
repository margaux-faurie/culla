//! The recovery table of design/file-engine.md §10.2, written once as data.
//!
//! For an operation that is not final, [`cell`] gives what to do from its kind, its state and
//! what is on disk: S (the source is there; R1 has already failed a changed one), P (the current
//! attempt's partial is there) and F (something is at the target). The executor
//! (`journal::run`) carries the cell out and then continues normal execution from the state it
//! leaves. The tests walk every kind, state and (S, P, F) combination through the fake disk.
//!
//! Pending operations go through the table too, so a run and a resume are the same code: from
//! `pending` with nothing in the way, the cell is [`Cell::Restart`], which is normal execution.
//!
//! `finish` operations (redo plans, §9.6) have no partial: from `verifying` on they use the
//! `xmove` rows with P = 0, and from `pending` their own row ([`FINISH_PENDING`]).

use super::{FailReason, Kind, OpState};

/// One action of the table. The branches name the hash or metadata check they need; "= H1"
/// is an unbuffered BLAKE3 read equal to the journal's H1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    /// Back to `pending` (when not there already), then execute from the start.
    Restart,
    /// Remove P (`remove_own_partial`, S is present), back to `pending`, execute.
    RemovePRestart,
    /// Fail with this reason, touching nothing (a partial that is there is kept and reported).
    Fail(FailReason),
    /// Remove P (S is present), then fail.
    RemovePFail(FailReason),
    /// Re-run step 3 (H2 from P, H3 from S): match: rename P into place, flush 2, →
    /// `committed`, continue; mismatch: remove P, `failed (verify_mismatch)`.
    Reverify,
    /// The step had happened but the journal had not said so: → `committed`, then the cell.
    /// From `verifying` the target is flushed first (flush 2 again): the crash may have come
    /// between the partial's rename and its flush.
    ToCommitted(&'static Cell),
    /// Rename P into place and flush 2 (the rename was reverted or never journaled); from
    /// `verifying` → `committed`, otherwise the state stays; then the cell.
    CommitP(&'static Cell),
    /// Continue normal execution from `committed`: a copy → `done`; a cross-volume move waits
    /// for its group, then is re-verified (H2 from F, H3 from S) and its source removed; a
    /// rename stays `committed` until the plan closes.
    Continue,
    /// From `committed`, the source being gone already: a copy → `done`; a cross-volume move →
    /// `source_removed` → `done`.
    Finish,
    /// From `source_removed` with S back (the removal was reverted): re-verify (H2 from F, H3
    /// from S): match: remove S, → `done`; mismatch: `failed (verify_mismatch)`, keep both.
    RemoveAgain,
    /// → `done`.
    ToDone,
    /// P = H1: the first cell, else the second.
    IfP(&'static Cell, &'static Cell),
    /// F = H1: the first cell, else the second.
    IfF(&'static Cell, &'static Cell),
    /// F has the planned size and mtime (a rename keeps both): the first cell, else the second.
    IfFPlanned(&'static Cell, &'static Cell),
}

use Cell::{
    CommitP, Continue, Fail, Finish, IfF, IfFPlanned, IfP, RemoveAgain, RemovePFail,
    RemovePRestart, Restart, Reverify, ToCommitted, ToDone,
};
use FailReason::{Conflict, FsInconsistent, Lost, SourceMissing};

/// `rename`: rows S1F0, S1F1, S0F1, S0F0; columns `pending`, `renaming`, `committed`.
pub const RENAME: [[Cell; 3]; 4] = [
    // S1 F0: not done, or reverted.
    [Restart, Restart, Restart],
    // S1 F1: not done or torn; torn after the rename.
    [Fail(Conflict), Fail(Conflict), Fail(FsInconsistent)],
    // S0 F1.
    [
        Fail(SourceMissing),
        IfFPlanned(&ToCommitted(&Continue), &Fail(SourceMissing)),
        Continue,
    ],
    // S0 F0.
    [Fail(SourceMissing), Fail(Lost), Fail(Lost)],
];

/// `copy` and `xmove`: rows S1P0F0, S1P1F0, S1P0F1, S1P1F1, S0P0F0, S0P1F0, S0P0F1, S0P1F1;
/// columns `pending` or `copying`, `verifying`, `committed`, `source_removed` (`xmove` only).
pub const COPY: [[Cell; 4]; 8] = [
    // S1 P0 F0: start again.
    [Restart, Restart, Restart, Restart],
    // S1 P1 F0.
    [
        RemovePRestart,
        Reverify,
        IfP(&CommitP(&Continue), &RemovePRestart),
        IfP(&CommitP(&RemoveAgain), &RemovePRestart),
    ],
    // S1 P0 F1.
    [
        Fail(Conflict),
        IfF(&ToCommitted(&Continue), &Fail(Conflict)),
        Continue,
        RemoveAgain,
    ],
    // S1 P1 F1.
    [
        RemovePFail(Conflict),
        IfF(&Fail(FsInconsistent), &RemovePFail(Conflict)),
        Fail(FsInconsistent),
        Fail(FsInconsistent),
    ],
    // S0 P0 F0.
    [Fail(SourceMissing), Fail(Lost), Fail(Lost), Fail(Lost)],
    // S0 P1 F0: P may be the only copy.
    [
        Fail(SourceMissing),
        Fail(SourceMissing),
        IfP(&CommitP(&Finish), &Fail(Lost)),
        IfP(&CommitP(&ToDone), &Fail(Lost)),
    ],
    // S0 P0 F1.
    [
        Fail(SourceMissing),
        IfF(&ToCommitted(&Finish), &Fail(SourceMissing)),
        IfF(&Finish, &Fail(Lost)),
        ToDone,
    ],
    // S0 P1 F1.
    [
        Fail(SourceMissing),
        Fail(FsInconsistent),
        Fail(FsInconsistent),
        Fail(FsInconsistent),
    ],
];

/// `finish` from `pending`: rows S1F0, S1F1, S0F1, S0F0 (proposed with the `finish` kind,
/// SPEC §10). Both files there: execute (re-verify, then remove S). The target gone: alarm.
/// The source gone already: nothing left to finish, reported.
pub const FINISH_PENDING: [Cell; 4] = [Fail(Lost), Restart, Fail(SourceMissing), Fail(Lost)];

/// The cell for an operation of `kind` in `state` with S, P and F as seen. `None` for a final
/// state, or a state the kind never takes. P is ignored for kinds without a partial.
pub fn cell(kind: Kind, state: OpState, s: bool, p: bool, f: bool) -> Option<&'static Cell> {
    let sf = match (s, f) {
        (true, false) => 0,
        (true, true) => 1,
        (false, true) => 2,
        (false, false) => 3,
    };
    match kind {
        Kind::Rename => {
            let col = match state {
                OpState::Pending => 0,
                OpState::Renaming => 1,
                OpState::Committed => 2,
                _ => return None,
            };
            Some(&RENAME[sf][col])
        }
        Kind::Copy | Kind::XMove | Kind::Finish => {
            let p = p && kind.has_partial();
            let col = match (kind, state) {
                (Kind::Finish, OpState::Pending) => return Some(&FINISH_PENDING[sf]),
                (Kind::Finish, OpState::Copying) => return None,
                (_, OpState::Pending | OpState::Copying) => 0,
                (_, OpState::Verifying) => 1,
                (_, OpState::Committed) => 2,
                (Kind::XMove | Kind::Finish, OpState::SourceRemoved) => 3,
                _ => return None,
            };
            let row = match (s, p, f) {
                (true, false, false) => 0,
                (true, true, false) => 1,
                (true, false, true) => 2,
                (true, true, true) => 3,
                (false, false, false) => 4,
                (false, true, false) => 5,
                (false, false, true) => 6,
                (false, true, true) => 7,
            };
            Some(&COPY[row][col])
        }
    }
}
