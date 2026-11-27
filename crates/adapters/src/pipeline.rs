//! The preview pipeline (ADR 0012, US-08): reader threads, a bounded pool of decode workers,
//! prefetch around the current photo, and the in-memory cache of encoded previews.
//!
//! The decisions are pure code in [`culla_core::prefetch`]: which photos to have ready
//! ([`wanted`], 5 ahead and 2 behind), which to read next ([`Scheduler`]), what the cache
//! evicts ([`Lru`]) and how much memory may be in flight ([`Budgets`]). This module runs them
//! on threads, std only (threads, one `Mutex`, one `Condvar`):
//!
//! * **Readers** (1 by default, SPEC §5.1: SD cards suffer under parallel reads) take the
//!   highest-priority wanted photo that is not cached, in flight or failed, reserve its length
//!   in the input budget, read it into a buffer through the [`PreviewSource`] (never mmap,
//!   NFR-REL-12) and queue it.
//! * **Decode workers** (2 to 4, [`default_workers`]) take the queued photo of highest
//!   priority, check and plan it with Culla's own parser (`preview::check_frame`, before any
//!   decoder), reserve [`Plan::working_bytes`] and, for a multi-scan JPEG, its coefficients
//!   ([`coefficient_bytes`]) in the working budget, and run the [`Decoder`] (turbojpeg,
//!   [`crate::preview::build_preview`]). At most one multi-scan decode runs at a time. While a
//!   job waits for working memory, no job of lower priority is admitted ahead of it, so small
//!   prefetches cannot starve the current photo. A job still queued (not yet taken by a
//!   worker) does not hold back the workers' own jobs: with every worker waiting, the current
//!   photo can sit in the queue until one of them is done (a known limit, architecture, "Preview
//!   pipeline threads, prefetch and cache").
//! * **Cancellation.** Moving the window ([`Pipeline::show`], [`Pipeline::set_wanted`]) makes
//!   the photos that left it stale: a reader drops one before reading it or while waiting for
//!   budget, a worker before decoding it or while waiting; a decode already running finishes
//!   (libjpeg-turbo cannot be interrupted) and its result is dropped.
//! * **Isolation.** A panic in a source read, the frame check or the decoder fails that photo
//!   only ([`PipelineError::Panicked`]); the thread carries on. A poisoned lock is recovered:
//!   no foreign code runs while it is held. Each job also has a drop guard: should the thread
//!   unwind anywhere else, the guard gives back the job's budgets and marks the photo failed,
//!   and the thread's loop is started again (`supervise`), so the pipeline never loses a
//!   reader or a worker.
//!
//! Nothing here writes a file. The disk cache (`%LOCALAPPDATA%\Culla\cache`) comes later.

use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use culla_core::exif::Orientation;
use culla_core::grouping::{Group, PhotoKind};
use culla_core::prefetch::{
    Admission, Budgets, Capacity, Direction, Lru, Outcome, Refusal, Reservation, Scheduler, Window,
    wanted,
};
use culla_core::preview::{self as core_preview, Plan, PlanError, Size, coefficient_bytes};

use crate::metadata::nef_preview::{self, MAX_PREVIEW_BYTES};
use crate::preview::{Preview, PreviewError, PreviewOptions, build_preview};

/// A photo, as the [`PreviewSource`] numbers them (an index into its list). The swipe order
/// is a list of these ([`Pipeline::set_order`]).
pub type PhotoId = usize;

/// The JPEG to decode for one photo, in memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceJpeg {
    pub jpeg: Vec<u8>,
    /// From the photo's EXIF (`metadata::exif`).
    pub orientation: Orientation,
}

/// Why a photo's JPEG could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// No such photo.
    Unknown,
    /// Opening or reading failed.
    Io(String),
    /// Longer than the cap.
    TooLarge { length: u64, cap: u64 },
    /// The file changed while it was read.
    Changed,
    /// A lone NEF without a usable embedded JPEG.
    NoPreview(String),
    /// More than the whole input budget.
    OverBudget { bytes: u64, limit: u64 },
    /// The photo left the window while it waited (not a failure).
    Cancelled,
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => f.write_str("no such photo"),
            Self::Io(e) => write!(f, "cannot read the file: {e}"),
            Self::TooLarge { length, cap } => {
                write!(f, "the JPEG is {length} bytes, over the {cap}-byte cap")
            }
            Self::Changed => f.write_str("the file changed while it was read"),
            Self::NoPreview(e) => write!(f, "{e}"),
            Self::OverBudget { bytes, limit } => {
                write!(f, "{bytes} bytes do not fit the {limit}-byte input budget")
            }
            Self::Cancelled => f.write_str("no longer wanted"),
        }
    }
}

impl std::error::Error for ReadError {}

/// Where the pipeline gets each photo's JPEG.
pub trait PreviewSource: Send + Sync + 'static {
    /// Reads the JPEG of `id`. Before allocating its buffer, the source calls `reserve` once
    /// with the number of bytes it will read; `reserve` blocks until the input budget has room
    /// and fails with [`ReadError::Cancelled`] when the photo is no longer wanted (return that
    /// error as is) or [`ReadError::OverBudget`].
    ///
    /// # Errors
    ///
    /// See [`ReadError`].
    fn read(
        &self,
        id: PhotoId,
        reserve: &mut dyn FnMut(u64) -> Result<(), ReadError>,
    ) -> Result<SourceJpeg, ReadError>;
}

/// Turns JPEG bytes into a screen preview.
pub trait Decoder: Send + Sync + 'static {
    /// # Errors
    ///
    /// See [`PreviewError`].
    fn decode(
        &self,
        jpeg: &[u8],
        orientation: Orientation,
        options: &PreviewOptions,
    ) -> Result<Preview, PreviewError>;
}

/// A shared decoder (the caller keeps a handle, for counters or a test double).
impl<T: Decoder> Decoder for Arc<T> {
    fn decode(
        &self,
        jpeg: &[u8],
        orientation: Orientation,
        options: &PreviewOptions,
    ) -> Result<Preview, PreviewError> {
        T::decode(self, jpeg, orientation, options)
    }
}

/// The real decoder: [`build_preview`] (turbojpeg, `fast_image_resize`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Turbo;

impl Decoder for Turbo {
    fn decode(
        &self,
        jpeg: &[u8],
        orientation: Orientation,
        options: &PreviewOptions,
    ) -> Result<Preview, PreviewError> {
        build_preview(jpeg, orientation, options)
    }
}

/// Decode workers by default: half the logical CPUs, at least 2 and at most 4 (SPEC §5.1,
/// architecture §8.3). 4 on the G14.
#[must_use]
pub fn default_workers() -> usize {
    thread::available_parallelism().map_or(2, |n| (n.get() / 2).clamp(2, 4))
}

/// How the pipeline runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Reader threads (at least 1).
    pub readers: usize,
    /// Decode workers (at least 1).
    pub workers: usize,
    pub window: Window,
    pub capacity: Capacity,
    pub budgets: Budgets,
    pub options: PreviewOptions,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            readers: 1,
            workers: default_workers(),
            window: Window::DEFAULT,
            capacity: Capacity::DEFAULT,
            budgets: Budgets::DEFAULT,
            options: PreviewOptions::default(),
        }
    }
}

/// A preview in the cache, with what it took.
#[derive(Debug, PartialEq, Eq)]
pub struct Prepared {
    pub preview: Preview,
    /// Reading the source JPEG, budget waits included.
    pub read: Duration,
    /// Bytes read.
    pub source_bytes: u64,
    /// The source JPEG is multi-scan (progressive or non-interleaved).
    pub multi_scan: bool,
}

/// Why a photo has no preview. Kept until [`Pipeline::retry`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PipelineError {
    Read(ReadError),
    Preview(PreviewError),
    /// The call needs more working memory than the whole budget.
    OverBudget {
        bytes: u64,
        limit: u64,
    },
    /// The preview is larger than the whole cache.
    TooLargeForCache {
        bytes: u64,
    },
    /// A panic outside the decoder's own isolation (in the source or the frame check).
    Panicked,
}

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(e) => write!(f, "{e}"),
            Self::Preview(e) => write!(f, "{e}"),
            Self::OverBudget { bytes, limit } => write!(
                f,
                "the preview needs {bytes} bytes, over the {limit}-byte working budget"
            ),
            Self::TooLargeForCache { bytes } => {
                write!(f, "the preview is {bytes} bytes, larger than the cache")
            }
            Self::Panicked => f.write_str("reading the photo failed unexpectedly"),
        }
    }
}

impl std::error::Error for PipelineError {}

/// Where a photo stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Ready(Arc<Prepared>),
    Failed(PipelineError),
    /// Wanted and not ready yet.
    Pending,
    /// Wanted, made, but refused by the cache (everything it holds ranks higher). Not made
    /// again until the window changes ([`Pipeline::show`], [`Pipeline::set_wanted`]).
    NotAdmitted,
    /// Not cached, not wanted, not in flight.
    NotWanted,
}

/// Counters since the pipeline started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Previews made and cached.
    pub cached: u64,
    /// Photos that failed.
    pub failed: u64,
    /// Work dropped because the photo left the window.
    pub cancelled: u64,
    /// Panics caught.
    pub panics: u64,
    /// Previews made but refused by the cache (everything held ranked higher).
    pub not_admitted: u64,
    /// Previews evicted from the cache.
    pub evicted: u64,
    /// Highest input and working reservations seen, in bytes.
    pub peak_input: u64,
    pub peak_working: u64,
    /// Most decodes running at once, and most multi-scan decodes (at most 1).
    pub peak_decodes: usize,
    pub peak_multi_scan: usize,
}

struct Loaded {
    id: PhotoId,
    source: SourceJpeg,
    input: u64,
    read: Duration,
}

struct State {
    sched: Scheduler<PhotoId>,
    lru: Lru<PhotoId>,
    ready: HashMap<PhotoId, Arc<Prepared>>,
    failed: HashMap<PhotoId, PipelineError>,
    loaded: Vec<Loaded>,
    input: culla_core::prefetch::Budget,
    working: culla_core::prefetch::Budget,
    decodes: usize,
    multi_scan: usize,
    /// Jobs waiting for room in the working budget: none of lower priority is admitted first.
    waiting: Vec<PhotoId>,
    order: Vec<PhotoId>,
    shutdown: bool,
    stats: Stats,
}

impl State {
    fn fail(&mut self, id: PhotoId, e: PipelineError) {
        self.sched.finish(id, Outcome::Failed);
        self.failed.insert(id, e);
        self.stats.failed += 1;
    }

    fn cancel(&mut self, id: PhotoId) {
        self.sched.finish(id, Outcome::Cancelled);
        self.stats.cancelled += 1;
    }

    fn live(&self, id: PhotoId) -> bool {
        !self.shutdown && self.sched.is_wanted(&id)
    }

    /// Where `id` stands, without waiting. `touch` marks a ready preview as just used.
    fn status(&mut self, id: PhotoId, touch: bool) -> Option<Status> {
        if let Some(p) = self.ready.get(&id).cloned() {
            if touch {
                self.lru.touch(&id);
            }
            return Some(Status::Ready(p));
        }
        if let Some(e) = self.failed.get(&id) {
            return Some(Status::Failed(e.clone()));
        }
        if self.sched.in_flight(&id) {
            return None;
        }
        if !self.sched.is_wanted(&id) {
            return Some(Status::NotWanted);
        }
        if self.sched.not_admitted(&id) {
            return Some(Status::NotAdmitted);
        }
        None
    }

    /// Whether a job of higher priority than `id` is waiting for working memory.
    fn outranked(&self, id: PhotoId) -> bool {
        let rank = |k: &PhotoId| self.sched.priority(k).unwrap_or(usize::MAX);
        let mine = rank(&id);
        self.waiting.iter().any(|w| *w != id && rank(w) < mine)
    }
}

struct Shared<S, D> {
    state: Mutex<State>,
    changed: Condvar,
    source: S,
    decoder: D,
    config: Config,
}

impl<S, D> Shared<S, D> {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wait<'a>(&self, g: MutexGuard<'a, State>) -> MutexGuard<'a, State> {
        self.changed.wait(g).unwrap_or_else(PoisonError::into_inner)
    }
}

/// The running pipeline. Dropping it stops and joins its threads (a decode in progress
/// finishes first).
pub struct Pipeline<S: PreviewSource, D: Decoder = Turbo> {
    shared: Arc<Shared<S, D>>,
    threads: Vec<JoinHandle<()>>,
}

impl<S: PreviewSource, D: Decoder> Pipeline<S, D> {
    /// Starts the threads. `order` is the swipe order (ids of `source`). Nothing is read
    /// until [`Self::show`] or [`Self::set_wanted`].
    ///
    /// # Errors
    ///
    /// When a thread cannot be spawned (the threads already started are stopped).
    pub fn start(source: S, decoder: D, order: Vec<PhotoId>, config: Config) -> io::Result<Self> {
        let state = State {
            sched: Scheduler::new(),
            lru: Lru::new(config.capacity),
            ready: HashMap::new(),
            failed: HashMap::new(),
            loaded: Vec::new(),
            input: culla_core::prefetch::Budget::new(config.budgets.input),
            working: culla_core::prefetch::Budget::new(config.budgets.working),
            decodes: 0,
            multi_scan: 0,
            waiting: Vec::new(),
            order,
            shutdown: false,
            stats: Stats::default(),
        };
        let shared = Arc::new(Shared {
            state: Mutex::new(state),
            changed: Condvar::new(),
            source,
            decoder,
            config,
        });
        let mut pipeline = Self {
            shared,
            threads: Vec::new(),
        };
        for i in 0..config.readers.max(1) {
            let sh = Arc::clone(&pipeline.shared);
            let t = thread::Builder::new()
                .name(format!("culla-reader-{i}"))
                .spawn(move || supervise(&sh, reader))?;
            pipeline.threads.push(t);
        }
        for i in 0..config.workers.max(1) {
            let sh = Arc::clone(&pipeline.shared);
            let t = thread::Builder::new()
                .name(format!("culla-decode-{i}"))
                .spawn(move || supervise(&sh, worker))?;
            pipeline.threads.push(t);
        }
        Ok(pipeline)
    }

    /// The configuration.
    #[must_use]
    pub fn config(&self) -> Config {
        self.shared.config
    }

    /// Replaces the swipe order (the EXIF pass may reorder it). The window is not moved.
    pub fn set_order(&self, order: Vec<PhotoId>) {
        self.shared.lock().order = order;
    }

    /// The user is at `position` of the swipe order, moving in `direction`: wants the
    /// configured window around it ([`wanted`]) and cancels what left it.
    pub fn show(&self, position: usize, direction: Direction) {
        let ids: Vec<PhotoId> = {
            let st = self.shared.lock();
            wanted(
                position,
                st.order.len(),
                direction,
                self.shared.config.window,
            )
            .into_iter()
            .map(|p| st.order[p])
            .collect()
        };
        self.set_wanted(ids);
    }

    /// Wants exactly `ids`, highest priority first, and cancels what left the list.
    pub fn set_wanted(&self, ids: impl IntoIterator<Item = PhotoId>) {
        {
            let mut st = self.shared.lock();
            // Stale work is noticed by its thread at the next check (`State::live`).
            let _stale = st.sched.set_wanted(ids);
        }
        self.shared.changed.notify_all();
    }

    /// The cached preview of `id`, marked as just used.
    #[must_use]
    pub fn get(&self, id: PhotoId) -> Option<Arc<Prepared>> {
        let mut st = self.shared.lock();
        let p = st.ready.get(&id).cloned();
        if p.is_some() {
            st.lru.touch(&id);
        }
        p
    }

    /// Where `id` stands now. A look only: a ready preview is not marked as used, so the
    /// cache's eviction order does not change.
    #[must_use]
    pub fn status(&self, id: PhotoId) -> Status {
        self.shared
            .lock()
            .status(id, false)
            .unwrap_or(Status::Pending)
    }

    /// Waits up to `timeout` for `id` to be ready, to fail, or to be refused by the cache
    /// ([`Status::NotAdmitted`]). A ready preview is marked as just used.
    #[must_use]
    pub fn wait(&self, id: PhotoId, timeout: Duration) -> Status {
        let deadline = Instant::now().checked_add(timeout);
        let mut st = self.shared.lock();
        loop {
            if let Some(status) = st.status(id, true) {
                return status;
            }
            let now = Instant::now();
            let left = match deadline {
                Some(d) if d > now => d - now,
                Some(_) => return Status::Pending,
                None => Duration::from_secs(3600),
            };
            st = self
                .shared
                .changed
                .wait_timeout(st, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// Lets a failed photo be tried again. Returns whether it had failed.
    pub fn retry(&self, id: PhotoId) -> bool {
        let had = {
            let mut st = self.shared.lock();
            st.failed.remove(&id);
            st.sched.retry(&id)
        };
        self.shared.changed.notify_all();
        had
    }

    /// Previews held in memory, and their bytes.
    #[must_use]
    pub fn cached(&self) -> (usize, u64) {
        let st = self.shared.lock();
        (st.lru.len(), st.lru.bytes())
    }

    /// The counters.
    #[must_use]
    pub fn stats(&self) -> Stats {
        self.shared.lock().stats
    }

    /// Jobs waiting now for room in the working budget.
    #[must_use]
    pub fn waiting(&self) -> usize {
        self.shared.lock().waiting.len()
    }

    /// Bytes reserved now in the input and the working budget.
    #[must_use]
    pub fn in_use(&self) -> (u64, u64) {
        let st = self.shared.lock();
        (st.input.used(), st.working.used())
    }
}

/// What one job holds, given back exactly once: by [`Job::settle`] on every normal path, or by
/// `Drop` if the thread unwinds outside the `catch_unwind` regions, which then also marks the
/// photo failed ([`PipelineError::Panicked`]) so it is not left in flight.
struct Job<'a, S, D> {
    sh: &'a Shared<S, D>,
    id: PhotoId,
    input: u64,
    working: u64,
    /// Counted in `decodes` (and `multi_scan` if `multi_scan`).
    running: bool,
    multi_scan: bool,
    settled: bool,
}

impl<'a, S, D> Job<'a, S, D> {
    fn new(sh: &'a Shared<S, D>, id: PhotoId) -> Self {
        Self {
            sh,
            id,
            input: 0,
            working: 0,
            running: false,
            multi_scan: false,
            settled: false,
        }
    }

    /// Gives back everything the job holds. The caller then records the outcome.
    fn settle(&mut self, st: &mut State) {
        if self.settled {
            return;
        }
        self.settled = true;
        st.input.release(self.input);
        st.working.release(self.working);
        if self.running {
            st.decodes = st.decodes.saturating_sub(1);
            if self.multi_scan {
                st.multi_scan = st.multi_scan.saturating_sub(1);
            }
        }
        st.waiting.retain(|w| *w != self.id);
    }
}

impl<S, D> Drop for Job<'_, S, D> {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        let mut st = self.sh.lock();
        self.settle(&mut st);
        st.stats.panics += 1;
        st.fail(self.id, PipelineError::Panicked);
        drop(st);
        self.sh.changed.notify_all();
    }
}

impl<S: PreviewSource, D: Decoder> Drop for Pipeline<S, D> {
    fn drop(&mut self) {
        self.shared.lock().shutdown = true;
        self.shared.changed.notify_all();
        for t in self.threads.drain(..) {
            // A thread only ends by returning on shutdown: `supervise` restarts its loop after
            // any unwind, so a join error is not expected.
            let _ = t.join();
        }
    }
}

/// Blocks until `bytes` fit the input budget, or the photo is no longer wanted.
fn reserve_input<S, D>(
    sh: &Shared<S, D>,
    id: PhotoId,
    bytes: u64,
    reserved: &mut u64,
) -> Result<(), ReadError> {
    let mut st = sh.lock();
    loop {
        if !st.live(id) {
            return Err(ReadError::Cancelled);
        }
        match st.input.reserve(bytes) {
            Reservation::Granted => {
                *reserved += bytes;
                st.stats.peak_input = st.stats.peak_input.max(st.input.used());
                return Ok(());
            }
            Reservation::Wait => st = sh.wait(st),
            Reservation::Never => {
                return Err(ReadError::OverBudget {
                    bytes,
                    limit: st.input.limit(),
                });
            }
        }
    }
}

/// Shortest pause before a thread's loop is started again after an unwind (provisional,
/// SPEC §10).
const RESTART_MIN: Duration = Duration::from_millis(10);
/// Longest pause between restarts (provisional, SPEC §10). A loop that ran at least this long
/// before unwinding is started again after [`RESTART_MIN`].
const RESTART_MAX: Duration = Duration::from_secs(1);

/// Runs a thread's loop (`reader` or `worker`, which return only on shutdown) and starts it
/// again after an unwind outside the per-photo `catch_unwind` regions. If the thread held a
/// job, its drop guard has already given back the job's budgets and failed its photo; an
/// unwind with no job held fails nothing. The thread then carries on with the next photo, so
/// a single reader cannot be lost and stall the pipeline. Restarts back off exponentially
/// from [`RESTART_MIN`] to [`RESTART_MAX`], so a loop that unwinds before it takes a job does
/// not spin; the pause waits on the shared condvar and ends at once on shutdown.
fn supervise<S, D>(sh: &Shared<S, D>, body: impl Fn(&Shared<S, D>)) {
    supervise_with(sh, RESTART_MIN, RESTART_MAX, body);
}

/// [`supervise`] with the backoff bounds as arguments, so tests can pick them.
fn supervise_with<S, D>(
    sh: &Shared<S, D>,
    min: Duration,
    max: Duration,
    body: impl Fn(&Shared<S, D>),
) {
    let mut pause = Duration::ZERO;
    loop {
        let started = Instant::now();
        if panic::catch_unwind(AssertUnwindSafe(|| body(sh))).is_ok() {
            return;
        }
        pause = next_pause(pause, started.elapsed(), min, max);
        let deadline = Instant::now() + pause;
        let mut st = sh.lock();
        loop {
            if st.shutdown {
                return;
            }
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            st = sh
                .changed
                .wait_timeout(st, deadline - now)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

/// The pause before the next restart, given the previous pause (zero before the first) and
/// how long the loop ran before it unwound: `min` after a run of at least `max`, otherwise
/// twice the previous pause, kept within `min..=max`.
fn next_pause(pause: Duration, ran_for: Duration, min: Duration, max: Duration) -> Duration {
    if ran_for >= max {
        min
    } else {
        pause.saturating_mul(2).max(min).min(max)
    }
}

fn reader<S: PreviewSource, D: Decoder>(sh: &Shared<S, D>) {
    loop {
        let id = {
            let mut st = sh.lock();
            loop {
                if st.shutdown {
                    return;
                }
                let State { sched, lru, .. } = &mut *st;
                if let Some(id) = sched.next(|k| lru.contains(k)) {
                    break id;
                }
                st = sh.wait(st);
            }
        };
        let mut job = Job::new(sh, id);
        let mut reserved = 0u64;
        let started = Instant::now();
        let result = {
            let mut reserve = |bytes| reserve_input(sh, id, bytes, &mut reserved);
            panic::catch_unwind(AssertUnwindSafe(|| sh.source.read(id, &mut reserve)))
        };
        job.input = reserved;
        let read = started.elapsed();
        let mut st = sh.lock();
        match result {
            Ok(Ok(source)) if st.live(id) => {
                // The input reservation moves with the queued job.
                job.settled = true;
                st.loaded.push(Loaded {
                    id,
                    source,
                    input: reserved,
                    read,
                });
            }
            Ok(Ok(_) | Err(ReadError::Cancelled)) => {
                job.settle(&mut st);
                st.cancel(id);
            }
            Ok(Err(e)) => {
                job.settle(&mut st);
                st.fail(id, PipelineError::Read(e));
            }
            Err(_) => {
                job.settle(&mut st);
                st.stats.panics += 1;
                st.fail(id, PipelineError::Panicked);
            }
        }
        drop(st);
        sh.changed.notify_all();
    }
}

/// Takes the queued photo of highest priority, dropping stale ones. `None` on shutdown.
fn take_loaded<S, D>(sh: &Shared<S, D>) -> Option<Loaded> {
    let mut st = sh.lock();
    loop {
        if st.shutdown {
            return None;
        }
        let mut dropped = false;
        let mut i = 0;
        while i < st.loaded.len() {
            let id = st.loaded[i].id;
            if st.sched.is_wanted(&id) {
                i += 1;
            } else {
                let l = st.loaded.swap_remove(i);
                st.input.release(l.input);
                st.cancel(l.id);
                dropped = true;
            }
        }
        if dropped {
            sh.changed.notify_all();
        }
        let best = (0..st.loaded.len()).min_by_key(|&i| st.sched.priority(&st.loaded[i].id));
        if let Some(i) = best {
            return Some(st.loaded.swap_remove(i));
        }
        st = sh.wait(st);
    }
}

/// Culla's frame check and the plan, before any decoder sees the bytes.
fn plan_of(source: &SourceJpeg, options: &PreviewOptions) -> Result<(Plan, bool, u64), PlanError> {
    let frame = core_preview::check_frame(&source.jpeg, &options.limits)?;
    let plan = core_preview::plan(
        Size::new(frame.width, frame.height),
        source.orientation,
        options.target,
        &options.limits,
    )?;
    Ok((
        plan,
        frame.multi_scan,
        plan.working_bytes()
            .saturating_add(coefficient_bytes(&frame)),
    ))
}

enum Admit {
    Run,
    Cancelled,
    Never { limit: u64 },
}

/// Blocks until the call fits the working budget (and the multi-scan slot, if needed) with no
/// job of higher priority waiting for room, or the photo is no longer wanted. On
/// [`Admit::Run`] the job holds `need` working bytes and a decode slot.
fn admit<S, D>(sh: &Shared<S, D>, job: &mut Job<'_, S, D>, need: u64, multi_scan: bool) -> Admit {
    let id = job.id;
    let mut st = sh.lock();
    let leave = |st: &mut State| {
        let before = st.waiting.len();
        st.waiting.retain(|w| *w != id);
        before != st.waiting.len()
    };
    loop {
        if !st.live(id) {
            if leave(&mut st) {
                sh.changed.notify_all();
            }
            return Admit::Cancelled;
        }
        if multi_scan && st.multi_scan > 0 {
            st = sh.wait(st);
            continue;
        }
        if st.outranked(id) {
            st = sh.wait(st);
            continue;
        }
        match st.working.reserve(need) {
            Reservation::Granted => {
                if leave(&mut st) {
                    sh.changed.notify_all();
                }
                job.working = need;
                job.running = true;
                job.multi_scan = multi_scan;
                st.decodes += 1;
                st.multi_scan += usize::from(multi_scan);
                let (used, decodes, multi) = (st.working.used(), st.decodes, st.multi_scan);
                let s = &mut st.stats;
                s.peak_working = s.peak_working.max(used);
                s.peak_decodes = s.peak_decodes.max(decodes);
                s.peak_multi_scan = s.peak_multi_scan.max(multi);
                return Admit::Run;
            }
            Reservation::Wait => {
                if !st.waiting.contains(&id) {
                    st.waiting.push(id);
                }
                st = sh.wait(st);
            }
            Reservation::Never => {
                if leave(&mut st) {
                    sh.changed.notify_all();
                }
                return Admit::Never {
                    limit: st.working.limit(),
                };
            }
        }
    }
}

fn worker<S: PreviewSource, D: Decoder>(sh: &Shared<S, D>) {
    let options = sh.config.options;
    while let Some(loaded) = take_loaded(sh) {
        let Loaded {
            id,
            source,
            input,
            read,
        } = loaded;
        let mut job = Job::new(sh, id);
        job.input = input;
        let planned = panic::catch_unwind(AssertUnwindSafe(|| plan_of(&source, &options)));
        let (multi_scan, need) = match planned {
            Ok(Ok((_, multi_scan, need))) => (multi_scan, need),
            Ok(Err(e)) => {
                drop(source);
                end(sh, &mut job, |st| {
                    st.fail(id, PipelineError::Preview(PreviewError::Plan(e)));
                });
                continue;
            }
            Err(_) => {
                drop(source);
                end(sh, &mut job, |st| {
                    st.stats.panics += 1;
                    st.fail(id, PipelineError::Panicked);
                });
                continue;
            }
        };
        match admit(sh, &mut job, need, multi_scan) {
            Admit::Run => {}
            Admit::Cancelled => {
                drop(source);
                end(sh, &mut job, |st| st.cancel(id));
                continue;
            }
            Admit::Never { limit } => {
                drop(source);
                end(sh, &mut job, |st| {
                    st.fail(id, PipelineError::OverBudget { bytes: need, limit });
                });
                continue;
            }
        }
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            sh.decoder
                .decode(&source.jpeg, source.orientation, &options)
        }));
        let source_bytes = source.jpeg.len() as u64;
        drop(source);
        end(sh, &mut job, |st| match result {
            Ok(Ok(preview)) => {
                let prepared = Prepared {
                    preview,
                    read,
                    source_bytes,
                    multi_scan,
                };
                store(st, id, prepared);
            }
            Ok(Err(e)) => st.fail(id, PipelineError::Preview(e)),
            Err(_) => {
                st.stats.panics += 1;
                st.fail(id, PipelineError::Preview(PreviewError::Panicked));
            }
        });
    }
}

/// Settles `job` and records its outcome under one lock, then wakes the waiters.
fn end<S, D>(sh: &Shared<S, D>, job: &mut Job<'_, S, D>, outcome: impl FnOnce(&mut State)) {
    let mut st = sh.lock();
    // The outcome first: should it unwind, the job is not settled yet and its guard fails it.
    outcome(&mut st);
    job.settle(&mut st);
    drop(st);
    sh.changed.notify_all();
}

/// Puts a finished preview in the cache, if it is still wanted and the cache admits it.
fn store(st: &mut State, id: PhotoId, prepared: Prepared) {
    if !st.live(id) {
        st.cancel(id);
        return;
    }
    let bytes = prepared.preview.jpeg.len() as u64;
    let State {
        sched, lru, ready, ..
    } = &mut *st;
    match lru.insert(id, bytes, sched.wanted()) {
        Admission::Admitted { evicted } => {
            for e in &evicted {
                ready.remove(e);
            }
            ready.insert(id, Arc::new(prepared));
            sched.finish(id, Outcome::Cached);
            st.stats.cached += 1;
            st.stats.evicted += evicted.len() as u64;
        }
        Admission::Refused(Refusal::LowestRank) => {
            sched.finish(id, Outcome::NotAdmitted);
            st.stats.not_admitted += 1;
        }
        Admission::Refused(Refusal::TooLarge) => {
            st.fail(id, PipelineError::TooLargeForCache { bytes });
        }
    }
}

/// What a [`FolderSource`] reads for one photo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    /// The camera JPEG (a pair's, or a lone JPEG).
    Jpeg {
        path: PathBuf,
        orientation: Orientation,
    },
    /// A lone NEF: its largest valid embedded JPEG (ADR 0025).
    LoneNef {
        path: PathBuf,
        orientation: Orientation,
    },
}

/// Photos of a scanned folder, read from disk into buffers (never mapped). Read-only.
#[derive(Clone, Debug)]
pub struct FolderSource {
    items: Vec<Item>,
    max_bytes: u64,
}

impl FolderSource {
    /// One item per group of the scan of `root`, in the same order (so a [`PhotoId`] is the
    /// group's index), with the orientation from each group's EXIF (`orientations[i]`,
    /// upright when missing). Pass only groups from the current scan of `root`.
    #[must_use]
    pub fn from_groups(root: &Path, groups: &[Group], orientations: &[Orientation]) -> Self {
        let items = groups
            .iter()
            .enumerate()
            .map(|(i, g)| {
                let mut path = root.to_path_buf();
                for c in g.primary.entry.components() {
                    path.push(c);
                }
                let orientation = orientations.get(i).copied().unwrap_or(Orientation::Normal);
                match g.primary.kind {
                    PhotoKind::Jpeg => Item::Jpeg { path, orientation },
                    PhotoKind::Nef => Item::LoneNef { path, orientation },
                }
            })
            .collect();
        Self::new(items)
    }

    /// A source over `items`, capped at the preview byte cap (64 MiB) per JPEG.
    #[must_use]
    pub fn new(items: Vec<Item>) -> Self {
        Self {
            items,
            max_bytes: MAX_PREVIEW_BYTES,
        }
    }

    /// The items.
    #[must_use]
    pub fn items(&self) -> &[Item] {
        &self.items
    }
}

impl PreviewSource for FolderSource {
    fn read(
        &self,
        id: PhotoId,
        reserve: &mut dyn FnMut(u64) -> Result<(), ReadError>,
    ) -> Result<SourceJpeg, ReadError> {
        match self.items.get(id) {
            None => Err(ReadError::Unknown),
            Some(Item::Jpeg { path, orientation }) => Ok(SourceJpeg {
                jpeg: read_capped(path, self.max_bytes, reserve)?,
                orientation: *orientation,
            }),
            Some(Item::LoneNef { path, orientation }) => {
                let nef_err = |e: nef_preview::PreviewError| match e {
                    nef_preview::PreviewError::Io(e) => ReadError::Io(e.to_string()),
                    nef_preview::PreviewError::TooLarge { length, cap } => {
                        ReadError::TooLarge { length, cap }
                    }
                    nef_preview::PreviewError::Changed => ReadError::Changed,
                    e @ nef_preview::PreviewError::Walk(_) => ReadError::NoPreview(e.to_string()),
                };
                let best = nef_preview::find_preview(path).map_err(nef_err)?;
                if best.range.length > self.max_bytes {
                    return Err(ReadError::TooLarge {
                        length: best.range.length,
                        cap: self.max_bytes,
                    });
                }
                reserve(best.range.length)?;
                let jpeg = nef_preview::read_preview_bytes(path, best.range, self.max_bytes)
                    .map_err(nef_err)?;
                Ok(SourceJpeg {
                    jpeg,
                    orientation: *orientation,
                })
            }
        }
    }
}

/// Reads all of `path` (opened read-only), refusing more than `cap` bytes, after reserving
/// its length.
fn read_capped(
    path: &Path,
    cap: u64,
    reserve: &mut dyn FnMut(u64) -> Result<(), ReadError>,
) -> Result<Vec<u8>, ReadError> {
    let io = |e: io::Error| ReadError::Io(e.to_string());
    let file = File::open(path).map_err(io)?;
    let length = file.metadata().map_err(io)?.len();
    if length > cap {
        return Err(ReadError::TooLarge { length, cap });
    }
    let n = usize::try_from(length).map_err(|_| ReadError::TooLarge { length, cap })?;
    reserve(length)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(n)
        .map_err(|_| io(io::Error::from(io::ErrorKind::OutOfMemory)))?;
    // One byte more than listed shows a file that grew; a shorter read one that shrank.
    file.take(length + 1).read_to_end(&mut bytes).map_err(io)?;
    if bytes.len() != n {
        return Err(ReadError::Changed);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared() -> Shared<(), ()> {
        shared_with((), (), vec![0])
    }

    fn shared_with<S, D>(source: S, decoder: D, order: Vec<PhotoId>) -> Shared<S, D> {
        let config = Config::default();
        Shared {
            state: Mutex::new(State {
                sched: Scheduler::new(),
                lru: Lru::new(config.capacity),
                ready: HashMap::new(),
                failed: HashMap::new(),
                loaded: Vec::new(),
                input: culla_core::prefetch::Budget::new(1000),
                working: culla_core::prefetch::Budget::new(1000),
                decodes: 0,
                multi_scan: 0,
                waiting: Vec::new(),
                order,
                shutdown: false,
                stats: Stats::default(),
            }),
            changed: Condvar::new(),
            source,
            decoder,
            config,
        }
    }

    #[test]
    fn a_job_unwinding_outside_catch_unwind_gives_everything_back_and_fails() {
        let sh = shared();
        {
            let mut st = sh.lock();
            st.sched.set_wanted([0]);
            assert_eq!(st.sched.next(|_| false), Some(0));
            assert_eq!(st.input.reserve(300), Reservation::Granted);
            assert_eq!(st.working.reserve(400), Reservation::Granted);
            st.decodes = 1;
            st.multi_scan = 1;
            st.waiting.push(0);
        }
        let r = panic::catch_unwind(AssertUnwindSafe(|| {
            let mut job = Job::new(&sh, 0);
            job.input = 300;
            job.working = 400;
            job.running = true;
            job.multi_scan = true;
            panic!("outside the isolated regions");
        }));
        assert!(r.is_err());
        let st = sh.lock();
        assert_eq!((st.input.used(), st.working.used()), (0, 0));
        assert_eq!((st.decodes, st.multi_scan), (0, 0));
        assert!(st.waiting.is_empty());
        assert!(!st.sched.in_flight(&0));
        assert_eq!(st.failed.get(&0), Some(&PipelineError::Panicked));
        assert_eq!(st.stats.panics, 1);
    }

    /// Ten bytes for any photo.
    struct Tiny;

    impl PreviewSource for Tiny {
        fn read(
            &self,
            _: PhotoId,
            reserve: &mut dyn FnMut(u64) -> Result<(), ReadError>,
        ) -> Result<SourceJpeg, ReadError> {
            reserve(10)?;
            Ok(SourceJpeg {
                jpeg: vec![0; 10],
                orientation: Orientation::Normal,
            })
        }
    }

    /// Never called: only the reader runs in these tests.
    struct NoDecode;

    impl Decoder for NoDecode {
        fn decode(
            &self,
            _: &[u8],
            _: Orientation,
            _: &PreviewOptions,
        ) -> Result<Preview, PreviewError> {
            unreachable!("no worker runs")
        }
    }

    #[test]
    fn a_thread_unwinding_outside_catch_unwind_goes_on_with_the_next_photo() {
        let sh = shared_with(Tiny, NoDecode, vec![0, 1]);
        sh.lock().sched.set_wanted([0, 1]);
        let runs = std::sync::atomic::AtomicUsize::new(0);
        thread::scope(|s| {
            let t = s.spawn(|| {
                supervise(&sh, |sh| {
                    if runs.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                        // The first run takes photo 0 and unwinds while its job is held.
                        let id = sh.lock().sched.next(|_| false);
                        assert_eq!(id, Some(0));
                        let _job = Job::new(sh, 0);
                        panic!("outside the isolated regions");
                    }
                    reader(sh);
                });
            });
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut st = sh.lock();
            while !st.loaded.iter().any(|l| l.id == 1) {
                let now = Instant::now();
                assert!(now < deadline, "photo 1 was never read");
                st = sh
                    .changed
                    .wait_timeout(st, deadline - now)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
            }
            assert_eq!(st.failed.get(&0), Some(&PipelineError::Panicked));
            assert_eq!(st.stats.panics, 1);
            assert_eq!(st.input.used(), 10);
            st.shutdown = true;
            drop(st);
            sh.changed.notify_all();
            assert!(t.join().is_ok(), "the thread ends normally on shutdown");
        });
        assert_eq!(runs.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    /// Polls `count` until it reaches `at_least`, for at most 10 s.
    fn wait_for_count(count: impl Fn() -> usize, at_least: usize) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while count() < at_least {
            assert!(Instant::now() < deadline, "count stayed below {at_least}");
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn the_restart_pause_doubles_caps_and_resets_after_a_long_run() {
        let ms = Duration::from_millis;
        let (min, max) = (ms(10), ms(1000));
        assert_eq!(next_pause(Duration::ZERO, ms(0), min, max), min);
        assert_eq!(next_pause(ms(10), ms(0), min, max), ms(20));
        assert_eq!(next_pause(ms(20), ms(999), min, max), ms(40));
        assert_eq!(next_pause(ms(640), ms(0), min, max), max);
        assert_eq!(next_pause(max, ms(0), min, max), max);
        assert_eq!(next_pause(max, max, min, max), min);
        assert_eq!(next_pause(ms(80), ms(5000), min, max), min);
    }

    #[test]
    fn a_loop_that_always_unwinds_does_not_spin() {
        let sh = shared();
        let runs = std::sync::atomic::AtomicUsize::new(0);
        let count = || runs.load(std::sync::atomic::Ordering::SeqCst);
        thread::scope(|s| {
            let t = s.spawn(|| {
                supervise(&sh, |_| {
                    runs.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    panic!("before any job is taken");
                });
            });
            wait_for_count(count, 1);
            let window = Instant::now();
            thread::sleep(Duration::from_millis(100));
            let early = count();
            let elapsed = window.elapsed();
            assert!(early < 20, "{early} restarts in {elapsed:?}: no backoff");
            sh.lock().shutdown = true;
            sh.changed.notify_all();
            assert!(t.join().is_ok(), "the thread returns on shutdown");
        });
    }

    #[test]
    fn shutdown_ends_a_restart_pause_at_once() {
        let sh = shared();
        let runs = std::sync::atomic::AtomicUsize::new(0);
        let count = || runs.load(std::sync::atomic::Ordering::SeqCst);
        let long = Duration::from_secs(60);
        thread::scope(|s| {
            let t = s.spawn(|| {
                supervise_with(&sh, long, long, |_| {
                    runs.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    panic!("before any job is taken");
                });
            });
            wait_for_count(count, 1);
            // Give the thread time to enter its 60 s pause.
            thread::sleep(Duration::from_millis(50));
            let asked = Instant::now();
            sh.lock().shutdown = true;
            sh.changed.notify_all();
            assert!(t.join().is_ok(), "the thread returns on shutdown");
            let took = asked.elapsed();
            assert!(took < Duration::from_secs(10), "shutdown took {took:?}");
        });
        assert_eq!(count(), 1);
    }

    #[test]
    fn a_settled_job_gives_back_once_and_records_nothing_on_drop() {
        let sh = shared();
        {
            let mut st = sh.lock();
            assert_eq!(st.input.reserve(300), Reservation::Granted);
        }
        {
            let mut job = Job::new(&sh, 0);
            job.input = 300;
            let mut st = sh.lock();
            job.settle(&mut st);
            job.settle(&mut st);
            assert_eq!(st.input.used(), 0);
        }
        let st = sh.lock();
        assert!(st.failed.is_empty());
        assert_eq!(st.stats.panics, 0);
    }
}
