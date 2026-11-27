//! The decision event log and its fold (ADR 0005, 0006, 0024; design/file-engine.md §5;
//! story US-10).
//!
//! Every user decision is an [`Event`] appended to a log that only grows. The current state of
//! every group and the undo and redo stacks are never stored: they are [`fold`] of the log, so
//! undo, redo, autosave and resume all come from one structure.
//!
//! - [`EventBody`] is one of `Decided {group, verdict}`, `Cleared {group}`, `Undo {target}` and
//!   `Redo {target}`; [`Cause`] says whether the user or reconciliation appended it. Only a
//!   `Cleared` may have the reconcile cause (ADR 0024).
//! - [`Decisions`] is the folded result: group states, `done` (the undo stack) and `undone`
//!   (the redo stack), both as stacks of sequence numbers with the top last.
//! - The command methods ([`Decisions::decide`], [`Decisions::clear`], [`Decisions::undo`],
//!   [`Decisions::redo`], [`Decisions::reconcile_clear`]) check the preconditions of §5.1,
//!   including the lock on groups that are in an open plan or applied (§5.4), and return the
//!   [`NewEvent`] to append, or [`Refused`]: a refused command appends nothing (INV-17). A
//!   locked group is refused as [`Refused::Locked`] before any other check, so the UI says
//!   "Already applied" rather than "Press U to undo" on it (proposed, SPEC §10). The
//!   caller (the session store) gives the event its `seq` and `ts`, writes it, then feeds it to
//!   [`Decisions::apply`].
//! - [`fold`] replays a stored log. It checks every precondition it can see (all but the locks,
//!   which come from plans and the journal) and stops at the first event that breaks one:
//!   such a log cannot come from the core, so it is reported as [`CorruptLog`] together with the
//!   state folded up to that event.
//!
//! The text form of events and of a fold, used by the golden fixtures shared with the TS reducer
//! (`testdata/event-log/`, INV-15), is in [`codec`].
//!
//! Undecided is the default: a group with no event, or whose last effective event left it
//! undecided, has no entry in [`Decisions::decided`].

pub mod codec;

use std::collections::BTreeMap;
use std::fmt;

#[cfg(test)]
mod tests;

/// A group's id: the `photo_group.id` row id of the session database (§2.3).
pub type GroupId = i64;

/// An event's sequence number: `decision_event.seq`. Strictly increasing within a log; gaps are
/// allowed (one database holds the events of several sessions).
pub type Seq = i64;

/// Keep or reject.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Verdict {
    /// → , drag right, the Keep button.
    Keep,
    /// ← , drag left, the Reject button.
    Reject,
}

impl Verdict {
    /// The stored and serialized name: `keep` or `reject`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::Reject => "reject",
        }
    }

    /// The group state this verdict leads to.
    pub fn state(self) -> GroupState {
        match self {
            Self::Keep => GroupState::Kept,
            Self::Reject => GroupState::Rejected,
        }
    }
}

/// A group's decision state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GroupState {
    /// No decision: never touched by a plan.
    #[default]
    Undecided,
    /// Kept: moved or copied to the destination on apply.
    Kept,
    /// Rejected: moved to `_rejected` on apply.
    Rejected,
}

impl GroupState {
    /// The serialized name: `undecided`, `kept` or `rejected`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Undecided => "undecided",
            Self::Kept => "kept",
            Self::Rejected => "rejected",
        }
    }

    /// Kept or rejected.
    pub fn is_decided(self) -> bool {
        self != Self::Undecided
    }
}

/// Who appended an event (ADR 0024).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Cause {
    /// A key, a drag or a button.
    #[default]
    User,
    /// Reconciliation: a new file joined a decided group (§6.2). Only on `Cleared`.
    Reconcile,
}

impl Cause {
    /// The stored and serialized name: `user` or `reconcile`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Reconcile => "reconcile",
        }
    }
}

/// What an event says (ADR 0005).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventBody {
    /// The group was kept or rejected.
    Decided {
        /// The group decided on.
        group: GroupId,
        /// Keep or reject.
        verdict: Verdict,
    },
    /// The group became undecided: U (cause user) or a file joining it (cause reconcile).
    Cleared {
        /// The group cleared.
        group: GroupId,
    },
    /// Ctrl+Z: the event `target` (the top of `done`) is reverted.
    Undo {
        /// The `seq` of the undone event.
        target: Seq,
    },
    /// Ctrl+Shift+Z: the event `target` (the top of `undone`) is applied again.
    Redo {
        /// The `seq` of the redone event.
        target: Seq,
    },
}

impl EventBody {
    /// The stored and serialized type: `decided`, `cleared`, `undo` or `redo`.
    pub fn type_str(&self) -> &'static str {
        match self {
            Self::Decided { .. } => "decided",
            Self::Cleared { .. } => "cleared",
            Self::Undo { .. } => "undo",
            Self::Redo { .. } => "redo",
        }
    }
}

/// One stored event: `{seq, ts, cause, body}` (design/file-engine.md §2.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Event {
    /// Sequence number, strictly increasing within the log.
    pub seq: Seq,
    /// When it was appended, Unix milliseconds (UTC). Informational: the fold ignores it.
    pub ts: i64,
    /// User or reconcile.
    pub cause: Cause,
    /// What happened.
    pub body: EventBody,
}

/// An event a command produced, before the store gives it a `seq` and a `ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NewEvent {
    /// User or reconcile.
    pub cause: Cause,
    /// What happened.
    pub body: EventBody,
}

impl NewEvent {
    /// The stored event, with the sequence number and time the store assigned.
    pub fn at(self, seq: Seq, ts: i64) -> Event {
        Event {
            seq,
            ts,
            cause: self.cause,
            body: self.body,
        }
    }
}

/// Why a command appends nothing (design/file-engine.md §5.2, §5.4; INV-17).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Refused {
    /// Keep or reject on a group that already has a decision: undo first (ADR 0006). The UI
    /// shakes the card and says "Press U to undo".
    AlreadyDecided {
        /// The group.
        group: GroupId,
    },
    /// U, or a reconcile clear, on a group that is undecided.
    NotDecided {
        /// The group.
        group: GroupId,
    },
    /// Ctrl+Z with an empty undo stack.
    NothingToUndo,
    /// Ctrl+Shift+Z with an empty redo stack.
    NothingToRedo,
    /// The group is in an open plan or already applied (§5.4): its decision cannot change. For
    /// undo and redo, the group of the event on top of the stack. The UI says "Already applied".
    Locked {
        /// The group.
        group: GroupId,
    },
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyDecided { group } => write!(f, "group {group} already has a decision"),
            Self::NotDecided { group } => write!(f, "group {group} has no decision"),
            Self::NothingToUndo => f.write_str("nothing to undo"),
            Self::NothingToRedo => f.write_str("nothing to redo"),
            Self::Locked { group } => write!(f, "group {group} is in a plan or already applied"),
        }
    }
}

impl std::error::Error for Refused {}

/// Which precondition a stored event breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CorruptReason {
    /// Its `seq` is not greater than the previous event's.
    SeqNotIncreasing,
    /// `Decided` on a group that already has a decision.
    DecidedOnDecided,
    /// `Cleared` on a group that is undecided.
    ClearedOnUndecided,
    /// The reconcile cause on an event other than `Cleared`.
    ReconcileNotCleared,
    /// `Undo` of an event that is not the top of `done` (or with `done` empty).
    UndoNotTop,
    /// `Redo` of an event that is not the top of `undone` (or with `undone` empty).
    RedoNotTop,
}

impl CorruptReason {
    /// The serialized name, used in golden fixtures.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SeqNotIncreasing => "seq_not_increasing",
            Self::DecidedOnDecided => "decided_on_decided",
            Self::ClearedOnUndecided => "cleared_on_undecided",
            Self::ReconcileNotCleared => "reconcile_not_cleared",
            Self::UndoNotTop => "undo_not_top",
            Self::RedoNotTop => "redo_not_top",
        }
    }

    /// The reason with this serialized name.
    pub fn from_name(s: &str) -> Option<Self> {
        [
            Self::SeqNotIncreasing,
            Self::DecidedOnDecided,
            Self::ClearedOnUndecided,
            Self::ReconcileNotCleared,
            Self::UndoNotTop,
            Self::RedoNotTop,
        ]
        .into_iter()
        .find(|r| r.as_str() == s)
    }
}

impl fmt::Display for CorruptReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SeqNotIncreasing => "sequence number not increasing",
            Self::DecidedOnDecided => "decided on a group that already has a decision",
            Self::ClearedOnUndecided => "cleared a group that has no decision",
            Self::ReconcileNotCleared => "reconcile cause on an event other than cleared",
            Self::UndoNotTop => "undo of an event that is not the last one done",
            Self::RedoNotTop => "redo of an event that is not the last one undone",
        })
    }
}

/// An event that [`Decisions::apply`] or [`fold`] refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BadEvent {
    /// The event's `seq`.
    pub seq: Seq,
    /// The precondition it breaks.
    pub reason: CorruptReason,
}

impl fmt::Display for BadEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "event {}: {}", self.seq, self.reason)
    }
}

impl std::error::Error for BadEvent {}

/// A stored log that breaks a precondition: [`fold`] stopped at `event`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorruptLog {
    /// The first event that breaks a precondition.
    pub event: BadEvent,
    /// The fold of every event before it.
    pub before: Box<Decisions>,
}

impl fmt::Display for CorruptLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "corrupt decision log at {}", self.event)
    }
}

impl std::error::Error for CorruptLog {}

/// A user `Decided` or `Cleared` that is on one of the stacks: what undo and redo need.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Step {
    group: GroupId,
    before: GroupState,
    after: GroupState,
}

/// The fold of a log: group states and the undo and redo stacks (design/file-engine.md §5.1).
///
/// Two folds compare equal when they have the same states, stacks and last `seq`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Decisions {
    /// Decided groups only: undecided is the absence of an entry.
    state: BTreeMap<GroupId, GroupState>,
    done: Vec<Seq>,
    undone: Vec<Seq>,
    /// One entry per seq in `done` or `undone`, and no other.
    steps: BTreeMap<Seq, Step>,
    last_seq: Option<Seq>,
}

/// Folds a stored log (design/file-engine.md §5.1). Stops at the first event that breaks a
/// precondition (see [`CorruptLog`]); locks are not checked here (they are checked when the
/// event is made, by the command methods of [`Decisions`]).
pub fn fold<'a>(events: impl IntoIterator<Item = &'a Event>) -> Result<Decisions, CorruptLog> {
    let mut d = Decisions::default();
    for e in events {
        if let Err(event) = d.apply(e) {
            return Err(CorruptLog {
                event,
                before: Box::new(d),
            });
        }
    }
    Ok(d)
}

impl Decisions {
    /// No events: every group undecided, both stacks empty.
    pub fn new() -> Self {
        Self::default()
    }

    /// A group's state; undecided if it has no decision.
    pub fn state(&self, group: GroupId) -> GroupState {
        self.state.get(&group).copied().unwrap_or_default()
    }

    /// Every decided group with its state, in group id order.
    pub fn decided(&self) -> impl Iterator<Item = (GroupId, GroupState)> + '_ {
        self.state.iter().map(|(g, s)| (*g, *s))
    }

    /// The undo stack: seqs of user `Decided` and `Cleared` events, the next one to undo last.
    pub fn done(&self) -> &[Seq] {
        &self.done
    }

    /// The redo stack: seqs of undone events, the next one to redo last.
    pub fn undone(&self) -> &[Seq] {
        &self.undone
    }

    /// The `seq` of the last event folded, if any.
    pub fn last_seq(&self) -> Option<Seq> {
        self.last_seq
    }

    /// What Ctrl+Z would undo: the event's seq and its group (the UI jumps to it).
    pub fn undo_target(&self) -> Option<(Seq, GroupId)> {
        let seq = *self.done.last()?;
        Some((seq, self.steps[&seq].group))
    }

    /// What Ctrl+Shift+Z would redo: the event's seq and its group.
    pub fn redo_target(&self) -> Option<(Seq, GroupId)> {
        let seq = *self.undone.last()?;
        Some((seq, self.steps[&seq].group))
    }

    /// Keep or reject (→, ←, drag, the buttons): only on an undecided group (ADR 0006).
    pub fn decide(
        &self,
        group: GroupId,
        verdict: Verdict,
        locked: &dyn Fn(GroupId) -> bool,
    ) -> Result<NewEvent, Refused> {
        if locked(group) {
            return Err(Refused::Locked { group });
        }
        if self.state(group).is_decided() {
            return Err(Refused::AlreadyDecided { group });
        }
        Ok(NewEvent {
            cause: Cause::User,
            body: EventBody::Decided { group, verdict },
        })
    }

    /// U: the group becomes undecided. Only on a decided group.
    pub fn clear(
        &self,
        group: GroupId,
        locked: &dyn Fn(GroupId) -> bool,
    ) -> Result<NewEvent, Refused> {
        self.clear_with(group, Cause::User, locked)
    }

    /// A new file joined a decided group on reopen (§6.2, ADR 0024): the group becomes
    /// undecided, and its earlier events leave both stacks.
    pub fn reconcile_clear(
        &self,
        group: GroupId,
        locked: &dyn Fn(GroupId) -> bool,
    ) -> Result<NewEvent, Refused> {
        self.clear_with(group, Cause::Reconcile, locked)
    }

    fn clear_with(
        &self,
        group: GroupId,
        cause: Cause,
        locked: &dyn Fn(GroupId) -> bool,
    ) -> Result<NewEvent, Refused> {
        if locked(group) {
            return Err(Refused::Locked { group });
        }
        if !self.state(group).is_decided() {
            return Err(Refused::NotDecided { group });
        }
        Ok(NewEvent {
            cause,
            body: EventBody::Cleared { group },
        })
    }

    /// Ctrl+Z: undo the top of `done`. Refused if its group is locked or applied, which also
    /// blocks older events behind it (§5.4, to confirm with the author).
    pub fn undo(&self, locked: &dyn Fn(GroupId) -> bool) -> Result<NewEvent, Refused> {
        let (target, group) = self.undo_target().ok_or(Refused::NothingToUndo)?;
        if locked(group) {
            return Err(Refused::Locked { group });
        }
        Ok(NewEvent {
            cause: Cause::User,
            body: EventBody::Undo { target },
        })
    }

    /// Ctrl+Shift+Z: redo the top of `undone`.
    pub fn redo(&self, locked: &dyn Fn(GroupId) -> bool) -> Result<NewEvent, Refused> {
        let (target, group) = self.redo_target().ok_or(Refused::NothingToRedo)?;
        if locked(group) {
            return Err(Refused::Locked { group });
        }
        Ok(NewEvent {
            cause: Cause::User,
            body: EventBody::Redo { target },
        })
    }

    /// Folds one more event. On error nothing changes.
    ///
    /// Returns the group the event changed (the group the UI jumps to on undo and redo).
    pub fn apply(&mut self, e: &Event) -> Result<GroupId, BadEvent> {
        let bad = |reason| BadEvent { seq: e.seq, reason };
        if self.last_seq.is_some_and(|last| e.seq <= last) {
            return Err(bad(CorruptReason::SeqNotIncreasing));
        }
        if e.cause == Cause::Reconcile && !matches!(e.body, EventBody::Cleared { .. }) {
            return Err(bad(CorruptReason::ReconcileNotCleared));
        }
        let group = match e.body {
            EventBody::Decided { group, verdict } => {
                let before = self.state(group);
                if before.is_decided() {
                    return Err(bad(CorruptReason::DecidedOnDecided));
                }
                self.push_user_step(e.seq, group, before, verdict.state());
                group
            }
            EventBody::Cleared { group } => {
                let before = self.state(group);
                if !before.is_decided() {
                    return Err(bad(CorruptReason::ClearedOnUndecided));
                }
                match e.cause {
                    Cause::User => self.push_user_step(e.seq, group, before, GroupState::Undecided),
                    Cause::Reconcile => {
                        self.set(group, GroupState::Undecided);
                        let steps = &mut self.steps;
                        let mut keep = |s: &Seq| {
                            let drop = steps[s].group == group;
                            if drop {
                                steps.remove(s);
                            }
                            !drop
                        };
                        self.done.retain(&mut keep);
                        self.undone.retain(&mut keep);
                    }
                }
                group
            }
            EventBody::Undo { target } => {
                if self.done.last() != Some(&target) {
                    return Err(bad(CorruptReason::UndoNotTop));
                }
                let step = self.steps[&target];
                debug_assert_eq!(self.state(step.group), step.after);
                self.done.pop();
                self.undone.push(target);
                self.set(step.group, step.before);
                step.group
            }
            EventBody::Redo { target } => {
                if self.undone.last() != Some(&target) {
                    return Err(bad(CorruptReason::RedoNotTop));
                }
                let step = self.steps[&target];
                debug_assert_eq!(self.state(step.group), step.before);
                self.undone.pop();
                self.done.push(target);
                self.set(step.group, step.after);
                step.group
            }
        };
        self.last_seq = Some(e.seq);
        Ok(group)
    }

    /// A new user `Decided` or `Cleared`: push on `done`, empty `undone`.
    fn push_user_step(&mut self, seq: Seq, group: GroupId, before: GroupState, after: GroupState) {
        for s in self.undone.drain(..) {
            self.steps.remove(&s);
        }
        self.steps.insert(
            seq,
            Step {
                group,
                before,
                after,
            },
        );
        self.done.push(seq);
        self.set(group, after);
    }

    fn set(&mut self, group: GroupId, state: GroupState) {
        if state.is_decided() {
            self.state.insert(group, state);
        } else {
            self.state.remove(&group);
        }
    }
}
