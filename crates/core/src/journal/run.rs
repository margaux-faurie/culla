//! The executor: runs a plan through the [`Disk`] and [`Store`] ports, and rolls it forward
//! after a stop (design/file-engine.md §9.3 to §9.5, §10.2).

use std::collections::BTreeSet;
use std::fmt;
use std::ops::Range;

use super::recovery::{self, Cell};
use super::{
    Detail, Entry, FailReason, Hash, Journal, JournalError, Kind, Op, OpId, OpState, Seen,
};

/// Where [`Disk::matches`] reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum At<'a> {
    /// A partial of the operation, as components from the target root.
    Partial(&'a [String]),
    /// The operation's target.
    Target,
}

/// What [`Disk::copy`] wrote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Copied {
    /// H1: BLAKE3 of the bytes read from the source while copying.
    pub hash: Hash,
    /// Bytes written to the partial.
    pub size: u64,
}

/// An error a disk step reports (§9.4). Sharing and lock violations are retried by the adapter
/// with the backoff of [`BACKOFF_MS`] before it reports [`DiskError::Locked`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiskError {
    /// `ERROR_SHARING_VIOLATION` or `ERROR_LOCK_VIOLATION` after the whole backoff.
    Locked,
    /// `ERROR_ALREADY_EXISTS` or `ERROR_FILE_EXISTS`: the no-replace rename or the create-new
    /// refused an existing name.
    Exists,
    /// `ERROR_NOT_SAME_DEVICE` on a rename.
    NotSameDevice,
    /// The volume went away (`ERROR_NOT_READY`, `ERROR_DEVICE_NOT_CONNECTED`, …). The adapter
    /// has closed every handle and dropped every proof.
    VolumeLost,
    /// `ERROR_WRITE_PROTECT`.
    WriteProtected,
    /// `ERROR_DISK_FULL`, `ERROR_HANDLE_DISK_FULL`.
    DiskFull,
    /// `ERROR_ACCESS_DENIED`.
    AccessDenied,
    /// Anything else, with its Win32 code.
    Io(u32),
}

/// Backoff for sharing and lock violations, in milliseconds (§9.4).
pub const BACKOFF_MS: [u64; 5] = [100, 200, 400, 800, 1600];

/// The file system steps the executor asks for (`culla-adapters::winfs`, §9.3). Every path is
/// built under the run's bound volume GUID paths (§6.3); every open refuses reparse points and
/// placeholders. Each method is one step of §9.3 and reports its outcome; none removes a file
/// except the two `remove_*` methods.
pub trait Disk {
    /// Looks at S, F and the given partials of `op` without following links (read-only).
    ///
    /// # Errors
    ///
    /// A volume error.
    fn probe(&mut self, op: &Op, partials: &[Vec<String>]) -> Result<Seen, DiskError>;

    /// An unbuffered BLAKE3 read of the file at `at` equals `hash`. An absent file does not.
    ///
    /// # Errors
    ///
    /// A read error.
    fn matches(&mut self, op: &Op, at: At<'_>, hash: &Hash) -> Result<bool, DiskError>;

    /// `rename` steps 2 and 3: `make_dirs`, `open_target_dir`, open S with `DELETE |
    /// GENERIC_READ` and check its size and mtime through the handle, `rename_noreplace`.
    ///
    /// # Errors
    ///
    /// [`DiskError::Exists`] when the target is taken; others as they come.
    fn rename(&mut self, op: &Op) -> Result<(), DiskError>;

    /// `copy`/`xmove` step 2: `make_dirs`, `open_target_dir`, `create_partial` (create-new),
    /// `copy_hashing`, `set_times`, flush 1. The adapter keeps `hP` and `hD` for the next steps
    /// of this attempt in this run.
    ///
    /// # Errors
    ///
    /// [`DiskError::Exists`] when the partial's name is taken (not ours); others as they come.
    fn copy(&mut self, op: &Op, partial: &[String]) -> Result<Copied, DiskError>;

    /// Step 3: H3 from S through `hS` (`open_source_verify`), H2 from the partial, both
    /// unbuffered; true when H1 = H2 = H3. For a cross-volume move the adapter keeps the
    /// `VerifiedMove` proof (it owns `hS`) for [`Disk::remove_source`].
    ///
    /// # Errors
    ///
    /// A read error.
    fn verify(&mut self, op: &Op, partial: &[String], hash: &Hash) -> Result<bool, DiskError>;

    /// Step 4: `rename_noreplace` of the partial into the target through its handle, flush 2.
    ///
    /// # Errors
    ///
    /// [`DiskError::Exists`] when the target is taken.
    fn commit(&mut self, op: &Op, partial: &[String]) -> Result<(), DiskError>;

    /// A fresh `VerifiedMove` for a committed cross-volume move or a `finish` operation: H2 from
    /// F and H3 from S (through a new `hS`), both unbuffered, equal to `hash`, F a regular file
    /// with the planned size. When it holds, F (just matched against H1) is flushed (flush 2
    /// again, as [`Disk::flush_target`]) before the proof is kept, so a source is never removed
    /// while the rename that put F in place may still be lost (proposed, SPEC §10). True when
    /// it holds; the adapter keeps the proof.
    ///
    /// # Errors
    ///
    /// A read error.
    fn prove(&mut self, op: &Op, hash: &Hash) -> Result<bool, DiskError>;

    /// `remove_verified_source` with the proof [`Disk::verify`] or [`Disk::prove`] made in this
    /// run (§9.7). Asked only when every operation of the group is committed and none failed,
    /// F is durable (flushed by this run's commit or by [`Disk::prove`]) and its folders too
    /// ([`Disk::flush_dirs`]).
    ///
    /// # Errors
    ///
    /// As the removal reports.
    fn remove_source(&mut self, op: &Op) -> Result<(), DiskError>;

    /// `remove_own_partial` (§9.7) for a partial this operation's journal recorded. Asked only
    /// after a fresh [`Disk::probe`] found S there as planned (INV-01). The adapter checks again
    /// itself: it opens S read-only through a handle it holds across the removal, checks size
    /// and mtime against the plan through it, and refuses (keeps the partial, reports an error)
    /// when S is absent or changed, because the partial may then be the only copy. An absent
    /// partial is not an error.
    ///
    /// # Errors
    ///
    /// As the removal reports, or the refusal above; the partial is then kept and reported.
    fn remove_partial(&mut self, op: &Op, partial: &[String]) -> Result<(), DiskError>;

    /// Flush 2 again, on the target: opens Culla's own committed copy (`FILE_OPEN`, never
    /// create or truncate, reparse points refused) and flushes it, so a rename of the partial
    /// that happened before a crash, before its flush, is durable. Asked only for a target just
    /// matched against H1 (recovery) and never written (proposed, file-engine INV-02, §9.3).
    ///
    /// # Errors
    ///
    /// As the open or flush reports.
    fn flush_target(&mut self, op: &Op) -> Result<(), DiskError>;

    /// Flushes every folder on the target's path below the root, and the parent of each (the
    /// root included), so the target's directory entries are durable before its source is
    /// removed (proposed; whether a folder handle can be flushed on FAT32 and exFAT is open,
    /// file-engine §13). Every folder, whether or not this run created it: one a crashed earlier
    /// run created may still be unflushed. Asked before every [`Disk::remove_source`], for every
    /// member of the group before the first removal.
    ///
    /// # Errors
    ///
    /// As the open or flush reports.
    fn flush_dirs(&mut self, op: &Op) -> Result<(), DiskError>;

    /// Drops any proof and handle held for `op` (its group failed).
    fn release(&mut self, op: &Op);
}

/// The store failed to commit an entry: the run stops at once, as after a crash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StoreError;

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the journal could not be written")
    }
}

impl std::error::Error for StoreError {}

/// The session store's journal table.
pub trait Store {
    /// Commits `entries` in one transaction, durably (`synchronous=FULL`), before returning.
    ///
    /// # Errors
    ///
    /// [`StoreError`]; then none of them is committed.
    fn append(&mut self, entries: &[Entry]) -> Result<(), StoreError>;
}

/// A request from the user, checked between steps (§9.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Halt {
    /// Pause after the current step.
    Pause,
    /// Stop starting new operations; a cross-volume group cut short is treated as failed, as
    /// is one an earlier run left mid-way (§9.5).
    Cancel,
}

/// Why a run paused (`plan.pause_reason`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PauseReason {
    /// The user paused.
    User,
    /// A volume went away.
    VolumeLost,
    /// The volume became write-protected.
    WriteProtected,
    /// The target volume is full; the current operation went back to `pending`.
    DiskFull,
}

/// How a run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum End {
    /// Nothing left to do: every operation is final, or a rename held at `committed` until
    /// [`close`].
    Finished,
    /// Paused; [`run`] again resumes it.
    Paused(PauseReason),
    /// Cancelled by the user, every group settled (none left mid-way); close the plan with
    /// [`close`].
    Cancelled,
}

/// The run cannot go on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunError {
    /// The journal could not be written: stop, as after a crash; resume later.
    Store(StoreError),
    /// The executor asked for an entry the journal refuses: a bug.
    Journal(JournalError),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(e) => e.fmt(f),
            Self::Journal(e) => write!(f, "journal refused an entry (a bug): {e}"),
        }
    }
}

impl std::error::Error for RunError {}

enum Brk {
    Stop(End),
    Err(RunError),
}

impl From<RunError> for Brk {
    fn from(e: RunError) -> Self {
        Self::Err(e)
    }
}

type Step = Result<(), Brk>;

/// Runs, or resumes, the plan in `journal` (§9.3, §9.5, §10.2). Groups in plan order; in each
/// group every operation that is not final is first probed and handled by the recovery table
/// (pending ones too, so the first run and a resume are the same code), nothing starts in a
/// group whose look found a failure, then the operations sent back to the start run in order,
/// and a cross-volume group removes its sources once every member is committed. `halt` is asked
/// between groups and between operations; a cancel settles every group left mid-way first
/// (§9.5), so [`End::Cancelled`] leaves a plan [`close`] accepts.
///
/// # Errors
///
/// [`RunError::Store`] when the journal cannot be written (the run stops where it is; resume
/// later); [`RunError::Journal`] for a bug.
pub fn run<D: Disk + ?Sized, S: Store + ?Sized>(
    journal: &mut Journal,
    disk: &mut D,
    store: &mut S,
    halt: &mut dyn FnMut() -> Option<Halt>,
) -> Result<End, RunError> {
    let mut r = Runner {
        j: journal,
        disk,
        store,
        proved: BTreeSet::new(),
        fresh: BTreeSet::new(),
        deferred: BTreeSet::new(),
        halt,
    };
    let out = r.groups();
    // Whatever stopped the run, no proof outlives it (its `hS` would keep the source open).
    r.release_all();
    out
}

/// A cross-volume group that has started: a member that removes its source is past `pending`,
/// or has had an attempt or a commit.
fn xmove_started(j: &Journal, g: &Range<usize>) -> bool {
    g.clone().any(|i| {
        let p = j.progress(i);
        j.ops()[i].kind.removes_source()
            && (p.state() != OpState::Pending || p.attempts > 0 || p.ever_committed)
    })
}

/// An operation a closed plan may not hold as it is (§9.1, §9.5): in a step (`renaming`,
/// `copying`, `verifying`), at `source_removed`, `committed` other than a rename, or `pending`
/// in a cross-volume group that has started (`started`).
fn unsettled_op(kind: Kind, state: OpState, started: bool) -> bool {
    match state {
        OpState::Renaming | OpState::Copying | OpState::Verifying | OpState::SourceRemoved => true,
        OpState::Committed => kind != Kind::Rename,
        OpState::Pending => started,
        OpState::Done | OpState::Failed | OpState::CopiedSourceKept => false,
    }
}

/// Closes the plan (`completed` or `cancelled`, §9.1): every rename still `committed` is
/// journaled `committed → done`, in one append. The caller sets the plan's status in the same
/// transaction. Only after [`run`] ended [`End::Finished`] or [`End::Cancelled`]: a plan that
/// still holds a group left mid-way is refused, because closing it would leave a cross-volume
/// move `committed` instead of `copied_source_kept`, or an operation in a step, where neither
/// the report nor the launch check would find it (cancel a paused plan by running it with a
/// [`Halt::Cancel`] at the first ask).
///
/// # Errors
///
/// [`RunError::Store`]; nothing is applied then. [`RunError::Journal`] with
/// [`JournalError::Unsettled`] (a bug in the caller) for the first operation left mid-way;
/// nothing is applied then either.
pub fn close<S: Store + ?Sized>(journal: &mut Journal, store: &mut S) -> Result<(), RunError> {
    for g in journal.groups() {
        let started = xmove_started(journal, &g);
        for i in g {
            let state = journal.progress(i).state();
            if unsettled_op(journal.ops()[i].kind, state, started) {
                return Err(RunError::Journal(JournalError::Unsettled {
                    op: journal.ops()[i].id,
                    state,
                }));
            }
        }
    }
    let entries: Vec<Entry> = journal
        .ops()
        .iter()
        .enumerate()
        .filter(|(i, o)| {
            o.kind == Kind::Rename && journal.progress(*i).state() == OpState::Committed
        })
        .map(|(_, o)| Entry {
            op: o.id,
            from: OpState::Committed,
            to: OpState::Done,
            detail: Detail::None,
        })
        .collect();
    if entries.is_empty() {
        return Ok(());
    }
    for e in &entries {
        journal.check(e).map_err(RunError::Journal)?;
    }
    store.append(&entries).map_err(RunError::Store)?;
    for e in &entries {
        journal.apply(e).map_err(RunError::Journal)?;
    }
    Ok(())
}

struct Runner<'a, D: Disk + ?Sized, S: Store + ?Sized> {
    j: &'a mut Journal,
    disk: &'a mut D,
    store: &'a mut S,
    /// Operations whose `VerifiedMove` the adapter holds, made in this run.
    proved: BTreeSet<OpId>,
    /// Operations whose target this run made durable: by its own commit (flush 2), by
    /// [`Disk::flush_target`] or by [`Disk::prove`].
    fresh: BTreeSet<OpId>,
    /// `source_removed` operations whose source came back: removed again by `remove_sources`,
    /// under the same gate as a first removal.
    deferred: BTreeSet<OpId>,
    halt: &'a mut dyn FnMut() -> Option<Halt>,
}

fn stop_for(e: DiskError) -> Option<End> {
    match e {
        DiskError::VolumeLost => Some(End::Paused(PauseReason::VolumeLost)),
        DiskError::WriteProtected => Some(End::Paused(PauseReason::WriteProtected)),
        _ => None,
    }
}

fn reason_for(e: DiskError) -> FailReason {
    match e {
        DiskError::Locked => FailReason::Locked,
        DiskError::Exists => FailReason::Conflict,
        DiskError::NotSameDevice => FailReason::PlanStale,
        DiskError::AccessDenied => FailReason::AccessDenied,
        DiskError::Io(c) => FailReason::Io(c),
        // Not failures: handled before (pause).
        DiskError::VolumeLost | DiskError::WriteProtected | DiskError::DiskFull => {
            FailReason::Io(0)
        }
    }
}

/// A disk call: a volume error stops the run; anything else is handed back.
fn call<T>(r: Result<T, DiskError>) -> Result<Result<T, DiskError>, Brk> {
    if let Err(e) = &r
        && let Some(end) = stop_for(*e)
    {
        return Err(Brk::Stop(end));
    }
    Ok(r)
}

impl<D: Disk + ?Sized, S: Store + ?Sized> Runner<'_, D, S> {
    fn groups(&mut self) -> Result<End, RunError> {
        match self.each_group() {
            Ok(()) => Ok(End::Finished),
            Err(Brk::Stop(end)) => Ok(end),
            Err(Brk::Err(e)) => Err(e),
        }
    }

    fn each_group(&mut self) -> Step {
        for g in self.j.groups() {
            self.halted(None)?;
            self.group(g)?;
        }
        Ok(())
    }

    /// Drops every proof still held.
    fn release_all(&mut self) {
        let held = std::mem::take(&mut self.proved);
        for op in self.j.ops().iter().filter(|o| held.contains(&o.id)) {
            self.disk.release(op);
        }
    }

    fn op(&self, i: usize) -> Op {
        self.j.ops()[i].clone()
    }

    fn state(&self, i: usize) -> OpState {
        self.j.progress(i).state()
    }

    fn hash(&self, i: usize) -> Option<Hash> {
        self.j.progress(i).hash
    }

    fn current_partial(&self, i: usize) -> Option<Vec<String>> {
        let op = &self.j.ops()[i];
        let a = self.j.progress(i).attempts;
        (op.kind.has_partial() && a > 0).then(|| op.partial(a))
    }

    fn record(&mut self, i: usize, to: OpState, detail: Detail) -> Step {
        let e = Entry {
            op: self.j.ops()[i].id,
            from: self.state(i),
            to,
            detail,
        };
        self.j.check(&e).map_err(RunError::Journal)?;
        self.store
            .append(std::slice::from_ref(&e))
            .map_err(RunError::Store)?;
        self.j.apply(&e).map_err(RunError::Journal)?;
        Ok(())
    }

    fn fail(&mut self, i: usize, r: FailReason) -> Step {
        self.record(i, OpState::Failed, Detail::Failed(r))
    }

    /// A member failed, or a cross-volume member was kept at `copied_source_kept`: only a
    /// failed group (§9.5) puts one there, so a resume after a crash part-way through
    /// [`Runner::fail_group`] finishes failing the group instead of starting its other members.
    fn group_failed(&self, g: &Range<usize>) -> bool {
        g.clone()
            .any(|i| matches!(self.state(i), OpState::Failed | OpState::CopiedSourceKept))
    }

    /// Asks `halt`, between groups (`current` is `None`) or between the operations of
    /// `current` once it has been looked at.
    fn halted(&mut self, current: Option<&Range<usize>>) -> Step {
        match (self.halt)() {
            None => Ok(()),
            Some(Halt::Pause) => Err(Brk::Stop(End::Paused(PauseReason::User))),
            Some(Halt::Cancel) => {
                self.cancel(current)?;
                Err(Brk::Stop(End::Cancelled))
            }
        }
    }

    fn xmove_started(&self, g: &Range<usize>) -> bool {
        xmove_started(self.j, g)
    }

    /// A group an earlier run left mid-way, which a closed plan may not hold as it is: a member
    /// in a step (`renaming`, `copying`, `verifying`), at `source_removed`, `committed` other
    /// than a rename (held until the plan closes), or `pending` in a cross-volume group that
    /// has started.
    fn unsettled(&self, g: &Range<usize>) -> bool {
        let started = self.xmove_started(g);
        g.clone()
            .any(|i| unsettled_op(self.j.ops()[i].kind, self.state(i), started))
    }

    /// A cancel (§9.5). A cross-volume group cut short is a failed group; other groups just
    /// stop. `current` (cut between its operations, already looked at) fails if it is a
    /// cross-volume group that has started. Every other group an earlier run ended inside (a
    /// pause, a card pulled, a full disk, a crash) is settled before the plan can close
    /// (proposed, SPEC §10):
    ///
    /// - it is looked at as on a resume: its recovery cells run, so a step that was under way
    ///   finishes or goes back to the start, and the operations sent back stay `pending`. The
    ///   same goes for every group with an operation that is not final and has been touched: a
    ///   rename held at `committed`, which a crash may have reverted, or a `pending` one whose
    ///   partial a crash put back. The final operations of the other groups are swept, as a
    ///   run does;
    /// - a cross-volume group whose source removals had begun (every member committed and
    ///   verified) finishes them, under the usual gate;
    /// - a group with a failed member, or still left mid-way (a cross-volume group that has
    ///   started), fails as §9.5 says.
    ///
    /// The look probes the disk, so with a volume still missing the cancel ends `paused
    /// (volume_lost)` instead, and a later cancel settles the group.
    fn cancel(&mut self, current: Option<&Range<usize>>) -> Step {
        if let Some(g) = current
            && self.xmove_started(g)
        {
            self.fail_group(g)?;
        }
        for g in self.j.groups() {
            // Every operation that is not final and has been touched is looked at too: a
            // rename held at `committed` (a crash may have reverted it; it goes back to
            // `pending`, not redone) and a `pending` one with a partial left behind.
            let touched = g.clone().any(|i| {
                let p = self.j.progress(i);
                !p.state().is_final()
                    && (p.state() != OpState::Pending || p.attempts > 0 || p.ever_committed)
            });
            if Some(&g) == current {
                continue;
            }
            if !(touched || self.unsettled(&g)) {
                // Still swept, as a run sweeps every group: a partial whose removal a crash
                // lost goes while S is there.
                for i in g.clone() {
                    if self.state(i).is_final() {
                        self.sweep(i)?;
                    }
                }
                continue;
            }
            let removing = g.clone().any(|i| {
                self.j.ops()[i].kind.removes_source()
                    && matches!(self.state(i), OpState::SourceRemoved | OpState::Done)
            });
            self.look(&g)?;
            if removing && !self.group_failed(&g) {
                self.remove_sources(&g)?;
            }
            if self.group_failed(&g) || self.unsettled(&g) {
                self.fail_group(&g)?;
            }
        }
        Ok(())
    }

    /// Removes this operation's partial if a fresh probe finds S there as planned (INV-01:
    /// after a failed step the partial may be the only copy). Otherwise, or when the removal
    /// fails for another reason than a lost volume, it is left in place (reported with the
    /// operation).
    fn cleanup(&mut self, i: usize, partial: &[String]) -> Step {
        let op = self.op(i);
        match call(self.disk.probe(&op, &[]))? {
            Ok(seen) if seen.source_as_planned(&op) => {
                let _ = call(self.disk.remove_partial(&op, partial))?;
            }
            _ => {}
        }
        Ok(())
    }

    /// A step failed with `e`: pause, back to pending (disk full while copying or verifying),
    /// or fail with its reason. `partial` is ours to remove if S is still there as planned.
    fn on_error(&mut self, i: usize, e: DiskError, partial: Option<&[String]>) -> Step {
        if let Some(end) = stop_for(e) {
            return Err(Brk::Stop(end));
        }
        let op = self.op(i);
        self.disk.release(&op);
        self.proved.remove(&op.id);
        if e == DiskError::DiskFull {
            // Only an attempt in progress goes back to the start; any other state pauses as it
            // is and the recovery table takes it on resume.
            if matches!(self.state(i), OpState::Copying | OpState::Verifying) {
                if let Some(p) = partial {
                    self.cleanup(i, p)?;
                }
                self.record(i, OpState::Pending, Detail::None)?;
            }
            return Err(Brk::Stop(End::Paused(PauseReason::DiskFull)));
        }
        if let Some(p) = partial {
            self.cleanup(i, p)?;
        }
        self.fail(i, reason_for(e))
    }

    /// Before the journal relies on F (→ `done` or `source_removed` without a removal of its
    /// own): F must be ours and durable. A target this run made durable needs nothing; any
    /// other is matched against H1 and flushed (flush 2 again, proposed, SPEC §10). False when
    /// the operation has been failed (`mismatch` when F ≠ H1) or the run stops.
    fn secure_target(&mut self, i: usize, mismatch: FailReason) -> Result<bool, Brk> {
        let op = self.op(i);
        if op.kind == Kind::Rename || self.fresh.contains(&op.id) {
            return Ok(true);
        }
        let Some(h) = self.hash(i) else {
            self.fail(i, FailReason::Lost)?;
            return Ok(false);
        };
        match call(self.disk.matches(&op, At::Target, &h))? {
            Ok(true) => {}
            Ok(false) => {
                self.fail(i, mismatch)?;
                return Ok(false);
            }
            Err(e) => {
                self.on_error(i, e, None)?;
                return Ok(false);
            }
        }
        match call(self.disk.flush_target(&op))? {
            Ok(()) => {
                self.fresh.insert(op.id);
                Ok(true)
            }
            Err(e) => {
                self.on_error(i, e, None)?;
                Ok(false)
            }
        }
    }

    fn group(&mut self, g: Range<usize>) -> Step {
        // 1. Look at every operation that is not final and carry out its table cell; the ones
        // sent back to the start wait until the whole group has been looked at.
        let queue = self.look(&g)?;
        // 2. Run the operations sent back to the start, unless the group failed.
        for i in queue {
            if self.group_failed(&g) {
                break;
            }
            self.halted(Some(&g))?;
            self.execute(i)?;
        }
        // 3. Group rules (§9.5).
        if self.group_failed(&g) {
            return self.fail_group(&g);
        }
        if g.clone().any(|i| self.j.ops()[i].kind.removes_source()) {
            self.remove_sources(&g)?;
        }
        Ok(())
    }

    /// Step 1 of a group: every operation that is not final is probed and its recovery cell
    /// carried out (final ones are swept). The operations back at `pending`, waiting to be
    /// executed, in plan order.
    fn look(&mut self, g: &Range<usize>) -> Result<Vec<usize>, Brk> {
        let mut queue = Vec::new();
        for i in g.clone() {
            if self.state(i).is_final() {
                self.sweep(i)?;
                continue;
            }
            let op = self.op(i);
            let partials = self.j.partials(i);
            let seen = call(self.disk.probe(&op, &partials))?;
            let seen = match seen {
                Ok(s) => s,
                Err(e) => {
                    self.on_error(i, e, None)?;
                    continue;
                }
            };
            // R1: a source that changed is failed, nothing touched.
            if seen.source.is_some() && !seen.source_as_planned(&op) {
                self.fail(i, FailReason::SourceChanged)?;
                continue;
            }
            let s = seen.source.is_some();
            let current = partials.len();
            // R2: earlier attempts' partials go while S is there.
            if s {
                for (k, p) in partials.iter().enumerate() {
                    if k + 1 < current && seen.partials.get(k).copied().unwrap_or(false) {
                        self.cleanup(i, p)?;
                    }
                }
            }
            let p = current > 0 && seen.partials.get(current - 1).copied().unwrap_or(false);
            let f = seen.target.is_some();
            let Some(cell) = recovery::cell(op.kind, self.state(i), s, p, f) else {
                continue;
            };
            if self.act(i, cell, &seen)? {
                queue.push(i);
            }
        }
        Ok(queue)
    }

    /// Carries out one table cell. True when the operation is back at `pending` and waits to be
    /// executed.
    fn act(&mut self, i: usize, cell: &Cell, seen: &Seen) -> Result<bool, Brk> {
        let op = self.op(i);
        match *cell {
            Cell::Restart => {
                if self.state(i) != OpState::Pending {
                    self.record(i, OpState::Pending, Detail::None)?;
                }
                Ok(true)
            }
            Cell::RemovePRestart => {
                if let Some(p) = self.current_partial(i) {
                    self.cleanup(i, &p)?;
                }
                if self.state(i) != OpState::Pending {
                    self.record(i, OpState::Pending, Detail::None)?;
                }
                Ok(true)
            }
            Cell::Fail(r) => {
                self.fail(i, r)?;
                Ok(false)
            }
            Cell::RemovePFail(r) => {
                if let Some(p) = self.current_partial(i) {
                    self.cleanup(i, &p)?;
                }
                self.fail(i, r)?;
                Ok(false)
            }
            Cell::Reverify => {
                let (Some(h), Some(p)) = (self.hash(i), self.current_partial(i)) else {
                    self.fail(i, FailReason::Lost)?;
                    return Ok(false);
                };
                self.verify_commit(i, &p, &h)?;
                Ok(false)
            }
            Cell::ToCommitted(next) => {
                // From `verifying` the partial's rename may have come just before the crash,
                // before flush 2: flush the target before the journal says committed.
                if op.kind != Kind::Rename && self.state(i) == OpState::Verifying {
                    if let Err(e) = call(self.disk.flush_target(&op))? {
                        self.on_error(i, e, None)?;
                        return Ok(false);
                    }
                    self.fresh.insert(op.id);
                }
                self.record(i, OpState::Committed, Detail::None)?;
                self.act(i, next, seen)
            }
            Cell::CommitP(next) => {
                let Some(p) = self.current_partial(i) else {
                    self.fail(i, FailReason::Lost)?;
                    return Ok(false);
                };
                match call(self.disk.commit(&op, &p))? {
                    Ok(()) => {
                        self.fresh.insert(op.id);
                        if self.state(i) == OpState::Verifying {
                            self.record(i, OpState::Committed, Detail::None)?;
                        }
                        self.act(i, next, seen)
                    }
                    Err(e) => {
                        // P is ours and matches H1; remove it only while S is there.
                        let ours = seen.source.is_some().then_some(p.as_slice());
                        self.on_error(i, e, ours)?;
                        Ok(false)
                    }
                }
            }
            Cell::Continue => {
                self.continue_committed(i)?;
                Ok(false)
            }
            Cell::Finish => {
                // S is gone: F must be ours and durable before the journal says so.
                if !self.secure_target(i, FailReason::Lost)? {
                    return Ok(false);
                }
                match op.kind {
                    Kind::Copy => self.record(i, OpState::Done, Detail::None)?,
                    Kind::XMove | Kind::Finish => {
                        self.record(i, OpState::SourceRemoved, Detail::None)?;
                        self.record(i, OpState::Done, Detail::None)?;
                    }
                    Kind::Rename => {}
                }
                Ok(false)
            }
            Cell::RemoveAgain => {
                // Removed again by `remove_sources`, once every member is committed and none
                // failed (INV-12); a failed group keeps both files (`fail_group`).
                self.deferred.insert(op.id);
                Ok(false)
            }
            Cell::ToDone => {
                if self.secure_target(i, FailReason::Lost)? {
                    self.record(i, OpState::Done, Detail::None)?;
                }
                Ok(false)
            }
            Cell::IfP(a, b) => {
                let m = match (self.hash(i), self.current_partial(i)) {
                    (Some(h), Some(p)) => {
                        match call(self.disk.matches(&op, At::Partial(&p), &h))? {
                            Ok(m) => m,
                            Err(e) => {
                                self.on_error(i, e, None)?;
                                return Ok(false);
                            }
                        }
                    }
                    _ => false,
                };
                self.act(i, if m { a } else { b }, seen)
            }
            Cell::IfF(a, b) => {
                let m = match self.hash(i) {
                    Some(h) => match call(self.disk.matches(&op, At::Target, &h))? {
                        Ok(m) => m,
                        Err(e) => {
                            self.on_error(i, e, None)?;
                            return Ok(false);
                        }
                    },
                    None => false,
                };
                self.act(i, if m { a } else { b }, seen)
            }
            Cell::IfFPlanned(a, b) => {
                let m = seen.target_as_planned(&op);
                self.act(i, if m { a } else { b }, seen)
            }
        }
    }

    /// Step 3 and 4 on the current partial: verify, then rename into place.
    fn verify_commit(&mut self, i: usize, p: &[String], h: &Hash) -> Step {
        let op = self.op(i);
        match call(self.disk.verify(&op, p, h))? {
            Ok(true) => {
                if op.kind == Kind::XMove {
                    self.proved.insert(op.id);
                }
            }
            Ok(false) => {
                self.disk.release(&op);
                self.cleanup(i, p)?;
                return self.fail(i, FailReason::VerifyMismatch);
            }
            Err(e) => return self.on_error(i, e, Some(p)),
        }
        match call(self.disk.commit(&op, p))? {
            Ok(()) => {
                self.fresh.insert(op.id);
                self.record(i, OpState::Committed, Detail::None)?;
                self.continue_committed(i)
            }
            Err(e) => {
                self.proved.remove(&op.id);
                self.on_error(i, e, Some(p))
            }
        }
    }

    /// From `committed`: a copy is done once its target is secured; a rename waits for the
    /// plan to close; a cross-volume move waits for its group.
    fn continue_committed(&mut self, i: usize) -> Step {
        if self.j.ops()[i].kind == Kind::Copy
            && self.state(i) == OpState::Committed
            && self.secure_target(i, FailReason::VerifyMismatch)?
        {
            self.record(i, OpState::Done, Detail::None)?;
        }
        Ok(())
    }

    /// Normal execution from `pending` (§9.3).
    fn execute(&mut self, i: usize) -> Step {
        let op = self.op(i);
        match op.kind {
            Kind::Rename => {
                self.record(i, OpState::Renaming, Detail::None)?;
                match call(self.disk.rename(&op))? {
                    Ok(()) => self.record(i, OpState::Committed, Detail::None),
                    Err(e) => self.on_error(i, e, None),
                }
            }
            Kind::Copy | Kind::XMove => {
                let attempt = self.j.progress(i).attempts.saturating_add(1);
                let p = op.partial(attempt);
                self.record(i, OpState::Copying, Detail::Attempt { attempt })?;
                let c = match call(self.disk.copy(&op, &p))? {
                    Ok(c) => c,
                    // The partial's name is taken by a file that is not ours: leave it.
                    Err(DiskError::Exists) => return self.fail(i, FailReason::Conflict),
                    Err(e) => return self.on_error(i, e, Some(&p)),
                };
                if c.size != op.expected_size {
                    self.cleanup(i, &p)?;
                    return self.fail(i, FailReason::ShortCopy);
                }
                self.record(
                    i,
                    OpState::Verifying,
                    Detail::Copied {
                        hash: c.hash,
                        size: c.size,
                    },
                )?;
                self.verify_commit(i, &p, &c.hash)
            }
            Kind::Finish => {
                let Some(h) = self.hash(i) else {
                    return self.fail(i, FailReason::Lost);
                };
                match call(self.disk.prove(&op, &h))? {
                    Ok(true) => {
                        self.proved.insert(op.id);
                        self.fresh.insert(op.id);
                        self.record(i, OpState::Verifying, Detail::None)?;
                        self.record(i, OpState::Committed, Detail::None)
                    }
                    Ok(false) => self.fail(i, FailReason::VerifyMismatch),
                    Err(e) => self.on_error(i, e, None),
                }
            }
        }
    }

    /// §9.3 step 6 for a group whose members are all committed: a proof for each (from this
    /// run, or made afresh, which also flushes F), then every target's folders flushed, then
    /// every source removed, then each target re-checked. A `source_removed` member whose
    /// source came back ([`Cell::RemoveAgain`]) is proved and removed again here, under the
    /// same gate.
    fn remove_sources(&mut self, g: &Range<usize>) -> Step {
        let all_committed = g.clone().all(|i| {
            matches!(
                self.state(i),
                OpState::Committed | OpState::SourceRemoved | OpState::Done
            )
        });
        if !all_committed {
            return Ok(());
        }
        let to_remove = |r: &Self, i: usize| {
            let op = &r.j.ops()[i];
            op.kind.removes_source()
                && match r.state(i) {
                    OpState::Committed => true,
                    OpState::SourceRemoved => r.deferred.contains(&op.id),
                    _ => false,
                }
        };
        for i in g.clone() {
            let op = self.op(i);
            if !to_remove(self, i) || self.proved.contains(&op.id) {
                continue;
            }
            let Some(h) = self.hash(i) else {
                self.fail(i, FailReason::Lost)?;
                return self.fail_group(g);
            };
            match call(self.disk.prove(&op, &h))? {
                Ok(true) => {
                    self.proved.insert(op.id);
                    self.fresh.insert(op.id);
                }
                Ok(false) => {
                    self.fail(i, FailReason::VerifyMismatch)?;
                    return self.fail_group(g);
                }
                Err(e) => {
                    self.on_error(i, e, None)?;
                    return self.fail_group(g);
                }
            }
        }
        // Every member's folders before the first removal, so a failed flush cannot split
        // the group.
        for i in g.clone() {
            let op = self.op(i);
            if !to_remove(self, i) {
                continue;
            }
            if let Err(e) = call(self.disk.flush_dirs(&op))? {
                self.on_error(i, e, None)?;
                return self.fail_group(g);
            }
        }
        for i in g.clone() {
            let op = self.op(i);
            if !to_remove(self, i) {
                continue;
            }
            match call(self.disk.remove_source(&op))? {
                Ok(()) => {
                    self.proved.remove(&op.id);
                    if !self.deferred.remove(&op.id) {
                        self.record(i, OpState::SourceRemoved, Detail::None)?;
                    }
                }
                Err(e) => {
                    self.on_error(i, e, None)?;
                    return self.fail_group(g);
                }
            }
        }
        for i in g.clone() {
            if self.state(i) == OpState::SourceRemoved {
                self.target_check(i)?;
            }
        }
        Ok(())
    }

    /// `source_removed → done` once the target is there with the planned size; otherwise an
    /// alarm.
    fn target_check(&mut self, i: usize) -> Step {
        let op = self.op(i);
        match call(self.disk.probe(&op, &[]))? {
            Ok(seen)
                if seen
                    .target
                    .is_some_and(|t| t.file && t.size == op.expected_size) =>
            {
                self.record(i, OpState::Done, Detail::None)
            }
            Ok(_) => self.fail(i, FailReason::Lost),
            Err(e) => self.on_error(i, e, None),
        }
    }

    /// A final operation's recorded partials that are still there (a removal the file system
    /// lost in a crash) are removed while S is there as planned, as R2 does for earlier
    /// attempts; otherwise they are kept and reported (proposed, SPEC §10). Not after
    /// `failed (fs_inconsistent)`.
    fn sweep(&mut self, i: usize) -> Step {
        let partials = self.j.partials(i);
        // On a volume found inconsistent nothing is touched (§9.4).
        if partials.is_empty() || self.j.progress(i).last_error == Some(FailReason::FsInconsistent)
        {
            return Ok(());
        }
        let op = self.op(i);
        let Ok(seen) = call(self.disk.probe(&op, &partials))? else {
            return Ok(());
        };
        if !seen.source_as_planned(&op) {
            return Ok(());
        }
        for (k, p) in partials.iter().enumerate() {
            if seen.partials.get(k).copied().unwrap_or(false) {
                self.cleanup(i, p)?;
            }
        }
        Ok(())
    }

    /// A failed group (§9.5): nothing more starts; committed cross-volume members keep both
    /// files; committed copies finish; committed renames stay until the plan closes.
    fn fail_group(&mut self, g: &Range<usize>) -> Step {
        for i in g.clone() {
            let op = self.op(i);
            match self.state(i) {
                OpState::Pending => self.fail(i, FailReason::GroupFailed)?,
                OpState::Copying | OpState::Verifying => {
                    self.disk.release(&op);
                    let partials = self.j.partials(i);
                    if let Ok(seen) = call(self.disk.probe(&op, &partials))? {
                        let present = partials
                            .len()
                            .checked_sub(1)
                            .and_then(|k| seen.partials.get(k).copied())
                            .unwrap_or(false);
                        if present && seen.source_as_planned(&op) {
                            self.cleanup(i, &partials[partials.len() - 1])?;
                        }
                    }
                    self.fail(i, FailReason::GroupFailed)?;
                }
                OpState::Committed => {
                    self.disk.release(&op);
                    self.proved.remove(&op.id);
                    match op.kind {
                        Kind::XMove => {
                            self.record(i, OpState::CopiedSourceKept, Detail::None)?;
                        }
                        Kind::Copy => {
                            if self.secure_target(i, FailReason::VerifyMismatch)? {
                                self.record(i, OpState::Done, Detail::None)?;
                            }
                        }
                        Kind::Finish => self.fail(i, FailReason::GroupFailed)?,
                        Kind::Rename => {}
                    }
                }
                // A source that came back is not removed again: both files are kept (INV-12).
                OpState::SourceRemoved if self.deferred.remove(&op.id) => {
                    self.disk.release(&op);
                    self.proved.remove(&op.id);
                    self.fail(i, FailReason::GroupFailed)?;
                }
                OpState::SourceRemoved => self.target_check(i)?,
                OpState::Renaming | OpState::Done | OpState::CopiedSourceKept | OpState::Failed => {
                }
            }
        }
        Ok(())
    }
}
