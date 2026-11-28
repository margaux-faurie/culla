//! The apply journal: operation states, legal transitions, the plan-to-journal mapping, and
//! the executor that applies a plan and rolls it forward after a crash or a pulled card
//! (design/file-engine.md §2.1, §9, §10; ADR 0004, 0009, 0010, 0021, 0022; stories US-17 to
//! US-20).
//!
//! Everything here is pure. The file system steps are requests to a [`Disk`] port, which the
//! adapter (`culla-adapters::winfs`) executes, one primitive group per call; every journal
//! transition is written through a [`Store`] port before the next step starts (write-ahead,
//! §2.2). The core decides what to do and in which state; the adapter performs it and reports
//! the outcome (§1).
//!
//! - [`OpState`] and [`Kind`] are the states and kinds of §9.2; [`edges`] lists the legal
//!   transitions per kind, exactly the `op_transition` seed, and [`Journal::apply`] refuses any
//!   other (INV-16).
//! - [`Journal::from_plan`] maps a confirmed plan to operations, all `pending`;
//!   [`Journal::replay`] rebuilds the same state from stored entries after a restart.
//! - [`run`] executes the plan in order, group by group (§9.3, §9.5), and is also resume: every
//!   operation that is not final is first looked at on disk ([`Disk::probe`]) and handled by the
//!   recovery table of §10.2 ([`recovery`]), then normal execution continues from the resulting
//!   state. No handle or proof survives a stop: every check and every hash is done afresh.
//! - [`close`] journals `committed → done` for every rename still `committed` when the plan
//!   closes (§9.1, §9.3 step 4), in one append.
//! - [`reverted`] is the launch check of §9.6 on a closed plan's `done` operations.
//!
//! The two removals Culla ever makes are requests too: [`Disk::remove_partial`] for a partial
//! this operation's journal recorded, asked only while the source is present (INV-01), and
//! [`Disk::remove_source`] for a cross-volume source, asked only after [`Disk::verify`] or
//! [`Disk::prove`] in the same run built the `VerifiedMove` proof (§9.7) and every operation of
//! the group is `committed` with none failed (INV-12).

pub mod recovery;
mod run;

#[cfg(test)]
mod fake;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt;

use crate::decisions::{GroupId, Verdict};
use crate::plan::{MemberId, OpKind, Plan, Root};
use crate::preflight::TimeRule;

pub use run::{
    At, BACKOFF_MS, Copied, Disk, DiskError, End, Halt, PauseReason, RunError, Store, StoreError,
    close, run,
};

/// An operation's id: `operation.id` in the session database. It is part of every partial
/// name (§7.2).
pub type OpId = i64;

/// A BLAKE3 hash (H1, H2 or H3, §9.3).
pub type Hash = [u8; 32];

/// An operation's kind (§2.3 `operation.kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// A no-replace rename of a user file on one volume.
    Rename,
    /// A verified copy; the source stays.
    Copy,
    /// A verified copy across volumes, then removal of the source.
    XMove,
    /// Redo plans only (§9.6, proposed): finish a cross-volume move whose source removal was
    /// lost after its plan closed.
    Finish,
}

impl Kind {
    /// The `operation.kind` value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rename => "rename",
            Self::Copy => "copy",
            Self::XMove => "xmove",
            Self::Finish => "finish",
        }
    }

    /// Writes a partial (§7.2).
    pub fn has_partial(self) -> bool {
        matches!(self, Self::Copy | Self::XMove)
    }

    /// Removes the source once the group is committed.
    pub fn removes_source(self) -> bool {
        matches!(self, Self::XMove | Self::Finish)
    }
}

impl From<OpKind> for Kind {
    fn from(k: OpKind) -> Self {
        match k {
            OpKind::Rename => Self::Rename,
            OpKind::Copy => Self::Copy,
            OpKind::XMove => Self::XMove,
        }
    }
}

/// An operation's state (§2.1 `OpState`, §9.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OpState {
    /// Not started, or back to the start.
    Pending,
    /// The rename's intent is logged; the rename may or may not have happened.
    Renaming,
    /// The attempt and its partial are logged; the copy may be under way.
    Copying,
    /// The partial is written, its times set, flushed, and H1 logged.
    Verifying,
    /// The file is at its target (H1 = H2 = H3 for a copy).
    Committed,
    /// A cross-volume source was removed.
    SourceRemoved,
    /// Finished.
    Done,
    /// A cross-volume move whose group failed or was cancelled after this file was committed:
    /// the file is at both places.
    CopiedSourceKept,
    /// Failed; see the reason.
    Failed,
}

impl OpState {
    /// Every state, in the order of §2.1.
    pub const ALL: [Self; 9] = [
        Self::Pending,
        Self::Renaming,
        Self::Copying,
        Self::Verifying,
        Self::Committed,
        Self::SourceRemoved,
        Self::Done,
        Self::CopiedSourceKept,
        Self::Failed,
    ];

    /// The `operation.state` value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Renaming => "renaming",
            Self::Copying => "copying",
            Self::Verifying => "verifying",
            Self::Committed => "committed",
            Self::SourceRemoved => "source_removed",
            Self::Done => "done",
            Self::CopiedSourceKept => "copied_source_kept",
            Self::Failed => "failed",
        }
    }

    /// Final for execution and recovery: `done`, `copied_source_kept`, `failed` (§2.1). Only a
    /// user action moves the last two on.
    pub fn is_final(self) -> bool {
        matches!(self, Self::Done | Self::CopiedSourceKept | Self::Failed)
    }
}

impl fmt::Display for OpState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why an operation failed (`operation.last_error`, §9.4, §10.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FailReason {
    /// Another process held the file through the whole backoff.
    Locked,
    /// The target (or the partial name) was taken by a file that is not ours.
    Conflict,
    /// A rename met `ERROR_NOT_SAME_DEVICE`: the plan no longer matches the volumes.
    PlanStale,
    /// `ERROR_ACCESS_DENIED`.
    AccessDenied,
    /// H1, H2 and H3 differ. Shown first in the report: it points at a bad reader or card.
    VerifyMismatch,
    /// The copy wrote fewer bytes than the plan expected.
    ShortCopy,
    /// The source's size or mtime changed since the plan (R1).
    SourceChanged,
    /// The source is gone and nothing of ours holds the file.
    SourceMissing,
    /// Alarm: the file is in none of the places the journal says it should be.
    Lost,
    /// Alarm: the volume shows the file in two places at once (a torn FAT rename); nothing was
    /// touched.
    FsInconsistent,
    /// Another operation of the group failed (§9.5).
    GroupFailed,
    /// Any other error, with its Win32 code.
    Io(u32),
}

impl FailReason {
    /// The `operation.last_error` value (`io` carries its code in the entry's detail).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Locked => "locked",
            Self::Conflict => "conflict",
            Self::PlanStale => "plan_stale",
            Self::AccessDenied => "access_denied",
            Self::VerifyMismatch => "verify_mismatch",
            Self::ShortCopy => "short_copy",
            Self::SourceChanged => "source_changed",
            Self::SourceMissing => "source_missing",
            Self::Lost => "lost",
            Self::FsInconsistent => "fs_inconsistent",
            Self::GroupFailed => "group_failed",
            Self::Io(_) => "io",
        }
    }

    /// An alarm: shown first in the report and the log, with advice to run `chkdsk` (§10.2).
    pub fn is_alarm(self) -> bool {
        matches!(self, Self::Lost | Self::FsInconsistent)
    }
}

mod table {
    use super::OpState;
    use super::OpState::{
        Committed, CopiedSourceKept, Copying, Done, Failed, Pending, Renaming, SourceRemoved,
        Verifying,
    };

    pub(super) const RENAME_EDGES: &[(OpState, OpState)] = &[
        (Pending, Renaming),
        (Renaming, Committed),
        (Renaming, Pending),
        (Committed, Done),
        (Committed, Pending),
        (Pending, Failed),
        (Renaming, Failed),
        (Committed, Failed),
        (Failed, Pending),
    ];

    pub(super) const COPY_EDGES: &[(OpState, OpState)] = &[
        (Pending, Copying),
        (Copying, Verifying),
        (Copying, Pending),
        (Verifying, Committed),
        (Verifying, Pending),
        (Committed, Done),
        (Committed, Pending),
        (Pending, Failed),
        (Copying, Failed),
        (Verifying, Failed),
        (Committed, Failed),
        (Failed, Pending),
    ];

    pub(super) const XMOVE_EDGES: &[(OpState, OpState)] = &[
        (Pending, Copying),
        (Copying, Verifying),
        (Copying, Pending),
        (Verifying, Committed),
        (Verifying, Pending),
        (Committed, Pending),
        (Pending, Failed),
        (Copying, Failed),
        (Verifying, Failed),
        (Committed, Failed),
        (Failed, Pending),
        (Committed, SourceRemoved),
        (Committed, CopiedSourceKept),
        (CopiedSourceKept, Committed),
        (SourceRemoved, Done),
        (SourceRemoved, Pending),
        (SourceRemoved, Failed),
    ];

    pub(super) const FINISH_EDGES: &[(OpState, OpState)] = &[
        (Pending, Verifying),
        (Verifying, Committed),
        (Verifying, Pending),
        (Committed, SourceRemoved),
        (Committed, Pending),
        (SourceRemoved, Done),
        (SourceRemoved, Pending),
        (Pending, Failed),
        (Verifying, Failed),
        (Committed, Failed),
        (SourceRemoved, Failed),
        (Failed, Pending),
    ];
}

/// The legal transitions of a kind (§9.2), as seeded into `op_transition`.
pub fn edges(kind: Kind) -> &'static [(OpState, OpState)] {
    match kind {
        Kind::Rename => table::RENAME_EDGES,
        Kind::Copy => table::COPY_EDGES,
        Kind::XMove => table::XMOVE_EDGES,
        Kind::Finish => table::FINISH_EDGES,
    }
}

/// `from → to` is a legal transition for `kind`.
pub fn is_legal(kind: Kind, from: OpState, to: OpState) -> bool {
    edges(kind).contains(&(from, to))
}

/// One operation of a confirmed plan: the `operation` row without its journal-derived columns.
/// Paths are components from their root, as in [`crate::plan::PlannedOp`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Op {
    /// `operation.id`.
    pub id: OpId,
    /// Position in the plan.
    pub ord: u32,
    /// The group.
    pub group: GroupId,
    /// The member.
    pub member: MemberId,
    /// Keep or reject.
    pub action: Verdict,
    /// The kind, fixed at plan time.
    pub kind: Kind,
    /// Source path from the source root.
    pub src: Vec<String>,
    /// Target root.
    pub root: Root,
    /// Target path from its root.
    pub dst: Vec<String>,
    /// The scanned size.
    pub expected_size: u64,
    /// The scanned mtime.
    pub src_mtime_ns: Option<i64>,
    /// For a `finish` operation: H1 of the `done` operation it finishes. `None` otherwise.
    pub finish_hash: Option<Hash>,
}

impl Op {
    /// The partial of attempt `attempt` (from 1): `<target name>.<op id>-<attempt>.culla-partial`
    /// next to the target (§7.2), as components from the target root.
    pub fn partial(&self, attempt: u32) -> Vec<String> {
        let mut p = self.dst.clone();
        if let Some(name) = p.last_mut() {
            *name = partial_name(name, self.id, attempt);
        }
        p
    }
}

/// `<target name>.<op id>-<attempt>.culla-partial` (§7.2).
pub fn partial_name(target_name: &str, op: OpId, attempt: u32) -> String {
    format!("{target_name}.{op}-{attempt}.culla-partial")
}

/// What one journal entry records besides its states (`journal_entry.detail`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Detail {
    /// Nothing more.
    None,
    /// `pending → copying`: the new attempt number and its partial.
    Attempt {
        /// From 1.
        attempt: u32,
    },
    /// `copying → verifying`: H1 and the bytes written.
    Copied {
        /// H1, the source hash taken during the copy.
        hash: Hash,
        /// Bytes written; equal to the plan's size.
        size: u64,
    },
    /// `→ failed`.
    Failed(FailReason),
}

/// One journal entry: a transition of one operation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Entry {
    /// The operation.
    pub op: OpId,
    /// Its state before.
    pub from: OpState,
    /// Its state after.
    pub to: OpState,
    /// What else it records.
    pub detail: Detail,
}

/// What the journal says about one operation so far (`operation`'s derived columns).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Progress {
    /// The current state.
    pub state: Option<OpState>,
    /// Copy attempts started (`operation.attempts`).
    pub attempts: u32,
    /// H1 of the current attempt, once `verifying`; for `finish`, the finished move's H1.
    pub hash: Option<Hash>,
    /// The reason of the last `→ failed`.
    pub last_error: Option<FailReason>,
    /// Some entry reached `committed` (§5.4 "applied", §9.6 "Retry failed").
    pub ever_committed: bool,
}

impl Progress {
    /// The current state (`pending` before any entry).
    pub fn state(&self) -> OpState {
        self.state.unwrap_or(OpState::Pending)
    }
}

/// Why the journal refuses an entry or a set of operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JournalError {
    /// Two operations with one id.
    DuplicateOp(OpId),
    /// No operation with this id.
    UnknownOp(OpId),
    /// Operations of one group are not contiguous in plan order, or the order has gaps or
    /// repeats.
    BadOrder,
    /// The entry's `from` is not the operation's current state.
    NotCurrent {
        /// The entry.
        entry: Entry,
        /// The operation's state.
        state: OpState,
    },
    /// The transition is not an edge of §9.2 for the operation's kind.
    Illegal(Entry),
    /// The detail does not fit the transition (a new attempt must be the next number, a
    /// `verifying` entry must carry H1 and the plan's size, a `failed` entry its reason).
    BadDetail(Entry),
    /// The plan cannot close: this operation was left mid-way (in a step, at
    /// `source_removed`, `committed` other than a rename, or `pending` in a cross-volume group
    /// that has started).
    Unsettled {
        /// The operation.
        op: OpId,
        /// Its state.
        state: OpState,
    },
}

impl fmt::Display for JournalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateOp(id) => write!(f, "operation {id} is given twice"),
            Self::UnknownOp(id) => write!(f, "no operation {id}"),
            Self::BadOrder => f.write_str("operations are not in plan order by group"),
            Self::NotCurrent { entry, state } => {
                write!(f, "operation {} is {state}, not {}", entry.op, entry.from)
            }
            Self::Illegal(e) => write!(
                f,
                "illegal transition {} → {} for operation {}",
                e.from, e.to, e.op
            ),
            Self::BadDetail(e) => write!(
                f,
                "detail does not fit {} → {} for operation {}",
                e.from, e.to, e.op
            ),
            Self::Unsettled { op, state } => {
                write!(f, "operation {op} is left {state}: the plan cannot close")
            }
        }
    }
}

impl std::error::Error for JournalError {}

/// The operations of one plan and what the journal says about each. Changes only through
/// [`Journal::apply`], one legal entry at a time (INV-16).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Journal {
    ops: Vec<Op>,
    progress: Vec<Progress>,
    index: BTreeMap<OpId, usize>,
    /// The user stopped the plan (`plan.status = stopping`, §9.1, proposed): every run of it
    /// cancels.
    stopping: bool,
}

impl Journal {
    /// The journal of a confirmed plan: operation `ord` gets the id `first_id + ord`, every
    /// operation `pending`. A plan from [`crate::plan::plan`] is always in a valid order.
    ///
    /// # Errors
    ///
    /// As [`Journal::new`].
    pub fn from_plan(plan: &Plan, first_id: OpId) -> Result<Self, JournalError> {
        let ops = plan
            .ops
            .iter()
            .map(|o| Op {
                id: first_id + i64::from(o.ord),
                ord: o.ord,
                group: o.group,
                member: o.member,
                action: o.action,
                kind: o.kind.into(),
                src: o.src.clone(),
                root: o.root,
                dst: o.dst.clone(),
                expected_size: o.expected_size,
                src_mtime_ns: o.src_mtime_ns,
                finish_hash: None,
            })
            .collect();
        Self::new(ops)
    }

    /// A journal over stored operations, all `pending` until [`Journal::replay`].
    ///
    /// # Errors
    ///
    /// [`JournalError::DuplicateOp`]; [`JournalError::BadOrder`] when `ord` is not `0, 1, …`
    /// in the given order or a group's operations are not contiguous.
    pub fn new(ops: Vec<Op>) -> Result<Self, JournalError> {
        let mut index = BTreeMap::new();
        let mut closed = std::collections::BTreeSet::new();
        for (i, o) in ops.iter().enumerate() {
            if index.insert(o.id, i).is_some() {
                return Err(JournalError::DuplicateOp(o.id));
            }
            if usize::try_from(o.ord).ok() != Some(i) {
                return Err(JournalError::BadOrder);
            }
            if i > 0 && ops[i - 1].group != o.group && !closed.insert(ops[i - 1].group) {
                return Err(JournalError::BadOrder);
            }
            if closed.contains(&o.group) {
                return Err(JournalError::BadOrder);
            }
        }
        let progress = ops
            .iter()
            .map(|o| Progress {
                hash: o.finish_hash,
                ..Progress::default()
            })
            .collect();
        Ok(Self {
            ops,
            progress,
            index,
            stopping: false,
        })
    }

    /// Applies stored entries in order, as after a restart.
    ///
    /// # Errors
    ///
    /// The first entry [`Journal::apply`] refuses; the entries before it are applied.
    pub fn replay<'a>(
        &mut self,
        entries: impl IntoIterator<Item = &'a Entry>,
    ) -> Result<(), JournalError> {
        for e in entries {
            self.apply(e)?;
        }
        Ok(())
    }

    /// Checks an entry against the operation's current state and kind, without applying it.
    ///
    /// # Errors
    ///
    /// See [`JournalError`].
    pub fn check(&self, e: &Entry) -> Result<(), JournalError> {
        let i = *self.index.get(&e.op).ok_or(JournalError::UnknownOp(e.op))?;
        let (op, p) = (&self.ops[i], &self.progress[i]);
        if p.state() != e.from {
            return Err(JournalError::NotCurrent {
                entry: e.clone(),
                state: p.state(),
            });
        }
        if !is_legal(op.kind, e.from, e.to) {
            return Err(JournalError::Illegal(e.clone()));
        }
        let fits = match (&e.detail, e.to) {
            (Detail::Attempt { attempt }, OpState::Copying) => {
                p.attempts.checked_add(1) == Some(*attempt)
            }
            (Detail::Copied { size, .. }, OpState::Verifying) if e.from == OpState::Copying => {
                *size == op.expected_size
            }
            (Detail::Failed(_), OpState::Failed) => true,
            (Detail::None, to) => {
                !matches!(to, OpState::Copying | OpState::Failed)
                    && !(to == OpState::Verifying && e.from == OpState::Copying)
            }
            _ => false,
        };
        if !fits {
            return Err(JournalError::BadDetail(e.clone()));
        }
        Ok(())
    }

    /// Applies one entry: checks it ([`Journal::check`]) and moves the operation on.
    ///
    /// # Errors
    ///
    /// As [`Journal::check`]; nothing changes then.
    pub fn apply(&mut self, e: &Entry) -> Result<(), JournalError> {
        self.check(e)?;
        let i = self.index[&e.op];
        let p = &mut self.progress[i];
        match &e.detail {
            Detail::Attempt { attempt } => {
                p.attempts = *attempt;
                p.hash = None;
            }
            Detail::Copied { hash, .. } => p.hash = Some(*hash),
            Detail::Failed(r) => p.last_error = Some(*r),
            Detail::None => {}
        }
        if e.to == OpState::Committed {
            p.ever_committed = true;
        }
        p.state = Some(e.to);
        Ok(())
    }

    /// The user stopped this plan ([`Store::stop`] recorded it): every run of it cancels, so a
    /// resume at launch settles the groups left mid-way and starts nothing new (proposed, SPEC
    /// §10).
    pub fn stopping(&self) -> bool {
        self.stopping
    }

    /// Marks the plan stopped, as its stored status says (`stopping`): the caller sets this
    /// when it loads a plan whose status is `stopping`, after [`Journal::replay`]; [`run`] sets
    /// it once [`Store::stop`] has returned. It is never cleared.
    pub fn set_stopping(&mut self) {
        self.stopping = true;
    }

    /// The operations in plan order.
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    /// What the journal says about the operation at position `i` in plan order.
    pub fn progress(&self, i: usize) -> &Progress {
        &self.progress[i]
    }

    /// The operation with id `id`, with its progress.
    pub fn get(&self, id: OpId) -> Option<(&Op, &Progress)> {
        self.index
            .get(&id)
            .map(|&i| (&self.ops[i], &self.progress[i]))
    }

    /// The partials operation `i` has used, attempt 1 first (§2.1: read from its entries).
    pub fn partials(&self, i: usize) -> Vec<Vec<String>> {
        let op = &self.ops[i];
        if !op.kind.has_partial() {
            return Vec::new();
        }
        (1..=self.progress[i].attempts)
            .map(|a| op.partial(a))
            .collect()
    }

    /// Every operation is final, or a rename held at `committed` until the plan closes: the run
    /// has nothing left to do.
    pub fn finished(&self) -> bool {
        self.ops.iter().zip(&self.progress).all(|(o, p)| {
            p.state().is_final() || (o.kind == Kind::Rename && p.state() == OpState::Committed)
        })
    }

    /// Positions in plan order of each group's operations, groups in plan order.
    pub fn groups(&self) -> Vec<std::ops::Range<usize>> {
        let mut out: Vec<std::ops::Range<usize>> = Vec::new();
        for (i, o) in self.ops.iter().enumerate() {
            match out.last_mut() {
                Some(r) if self.ops[r.start].group == o.group => r.end = i + 1,
                _ => out.push(i..i + 1),
            }
        }
        out
    }
}

/// A file as a lookup saw it, without following links.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileSeen {
    /// A regular file, not a link, a placeholder or a folder.
    pub file: bool,
    /// Size in bytes.
    pub size: u64,
    /// Modification time.
    pub mtime_ns: Option<i64>,
}

/// What [`Disk::probe`] saw for one operation (S, P and F of §10.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    /// How mtimes compare on the source's volume (§6.2).
    pub rule: TimeRule,
    /// The source, if anything is there.
    pub source: Option<FileSeen>,
    /// The target, if anything is there.
    pub target: Option<FileSeen>,
    /// For each partial asked about, attempt 1 first: something is there.
    pub partials: Vec<bool>,
}

impl Seen {
    /// The source is there with the planned size and mtime.
    pub fn source_as_planned(&self, op: &Op) -> bool {
        self.source.is_some_and(|s| {
            s.file && s.size == op.expected_size && self.rule.matches(s.mtime_ns, op.src_mtime_ns)
        })
    }

    /// The target is there with the planned size and mtime (a rename keeps both).
    pub fn target_as_planned(&self, op: &Op) -> bool {
        self.target.is_some_and(|t| {
            t.file && t.size == op.expected_size && self.rule.matches(t.mtime_ns, op.src_mtime_ns)
        })
    }
}

/// A `done` operation of a closed plan that the disk no longer matches (§9.6), found at the
/// next launch and by reconciliation. Nothing runs until the user confirms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Reverted {
    /// A rename the file system put back: the source is there as planned, the target is not.
    /// Offered as "Redo moves".
    Rename,
    /// A cross-volume move whose source removal was lost: both files are there, the target
    /// with the planned size. Offered as "Finish moves".
    SourceBack,
    /// Both or neither (a torn rename): reported, never offered.
    Torn,
    /// The target of a cross-volume move or `finish` is gone (with or without its source), or a
    /// rename's source is back but not as planned: an alarm, reported, never offered.
    Lost,
}

/// The launch check of §9.6 on one `done` operation of a closed plan. `None` when the disk
/// matches the journal, or the operation is not `done`, or is a copy (whose source stays).
pub fn reverted(op: &Op, progress: &Progress, seen: &Seen) -> Option<Reverted> {
    if progress.state() != OpState::Done {
        return None;
    }
    let s = seen.source.is_some();
    let f = seen.target.is_some();
    match op.kind {
        Kind::Rename => match (s, f) {
            (true, false) if seen.source_as_planned(op) => Some(Reverted::Rename),
            (true, false) => Some(Reverted::Lost),
            (true, true) | (false, false) => Some(Reverted::Torn),
            (false, true) => None,
        },
        Kind::XMove | Kind::Finish => {
            let planned = seen
                .target
                .is_some_and(|t| t.file && t.size == op.expected_size);
            match (s, f) {
                (true, true) if planned => Some(Reverted::SourceBack),
                (_, false) => Some(Reverted::Lost),
                _ => None,
            }
        }
        Kind::Copy => None,
    }
}
