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
//!   [`crate::preview::build_preview`]). At most one multi-scan decode runs at a time.
//! * **Cancellation.** Moving the window ([`Pipeline::show`], [`Pipeline::set_wanted`]) makes
//!   the photos that left it stale: a reader drops one before reading it or while waiting for
//!   budget, a worker before decoding it or while waiting; a decode already running finishes
//!   (libjpeg-turbo cannot be interrupted) and its result is dropped.
//! * **Isolation.** A panic in a source read, the frame check or the decoder fails that photo
//!   only ([`PipelineError::Panicked`]); the thread carries on. A poisoned lock is recovered:
//!   no foreign code runs while it is held.
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
                .spawn(move || reader(&sh))?;
            pipeline.threads.push(t);
        }
        for i in 0..config.workers.max(1) {
            let sh = Arc::clone(&pipeline.shared);
            let t = thread::Builder::new()
                .name(format!("culla-decode-{i}"))
                .spawn(move || worker(&sh))?;
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

    /// Where `id` stands now.
    #[must_use]
    pub fn status(&self, id: PhotoId) -> Status {
        self.wait(id, Duration::ZERO)
    }

    /// Waits up to `timeout` for `id` to be ready or to fail. A ready preview is marked as
    /// just used.
    #[must_use]
    pub fn wait(&self, id: PhotoId, timeout: Duration) -> Status {
        let deadline = Instant::now().checked_add(timeout);
        let mut st = self.shared.lock();
        loop {
            if let Some(p) = st.ready.get(&id).cloned() {
                st.lru.touch(&id);
                return Status::Ready(p);
            }
            if let Some(e) = st.failed.get(&id) {
                return Status::Failed(e.clone());
            }
            if !st.sched.is_wanted(&id) && !st.sched.in_flight(&id) {
                return Status::NotWanted;
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
}

impl<S: PreviewSource, D: Decoder> Drop for Pipeline<S, D> {
    fn drop(&mut self) {
        self.shared.lock().shutdown = true;
        self.shared.changed.notify_all();
        for t in self.threads.drain(..) {
            // A thread only ends by returning: panics are caught per photo.
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
        let mut reserved = 0u64;
        let started = Instant::now();
        let result = {
            let mut reserve = |bytes| reserve_input(sh, id, bytes, &mut reserved);
            panic::catch_unwind(AssertUnwindSafe(|| sh.source.read(id, &mut reserve)))
        };
        let read = started.elapsed();
        let mut st = sh.lock();
        match result {
            Ok(Ok(source)) if st.live(id) => st.loaded.push(Loaded {
                id,
                source,
                input: reserved,
                read,
            }),
            Ok(Ok(_) | Err(ReadError::Cancelled)) => {
                st.input.release(reserved);
                st.cancel(id);
            }
            Ok(Err(e)) => {
                st.input.release(reserved);
                st.fail(id, PipelineError::Read(e));
            }
            Err(_) => {
                st.input.release(reserved);
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

/// Blocks until the call fits the working budget (and the multi-scan slot, if needed), or the
/// photo is no longer wanted.
fn admit<S, D>(sh: &Shared<S, D>, id: PhotoId, need: u64, multi_scan: bool) -> Admit {
    let mut st = sh.lock();
    loop {
        if !st.live(id) {
            return Admit::Cancelled;
        }
        if multi_scan && st.multi_scan > 0 {
            st = sh.wait(st);
            continue;
        }
        match st.working.reserve(need) {
            Reservation::Granted => {
                st.decodes += 1;
                st.multi_scan += usize::from(multi_scan);
                let (used, decodes, multi) = (st.working.used(), st.decodes, st.multi_scan);
                let s = &mut st.stats;
                s.peak_working = s.peak_working.max(used);
                s.peak_decodes = s.peak_decodes.max(decodes);
                s.peak_multi_scan = s.peak_multi_scan.max(multi);
                return Admit::Run;
            }
            Reservation::Wait => st = sh.wait(st),
            Reservation::Never => {
                return Admit::Never {
                    limit: st.working.limit(),
                };
            }
        }
    }
}

fn worker<S: PreviewSource, D: Decoder>(sh: &Shared<S, D>) {
    let options = sh.config.options;
    while let Some(job) = take_loaded(sh) {
        let Loaded {
            id,
            source,
            input,
            read,
        } = job;
        let planned = panic::catch_unwind(AssertUnwindSafe(|| plan_of(&source, &options)));
        let (multi_scan, need) = match planned {
            Ok(Ok((_, multi_scan, need))) => (multi_scan, need),
            Ok(Err(e)) => {
                drop(source);
                let mut st = sh.lock();
                st.input.release(input);
                st.fail(id, PipelineError::Preview(PreviewError::Plan(e)));
                drop(st);
                sh.changed.notify_all();
                continue;
            }
            Err(_) => {
                drop(source);
                let mut st = sh.lock();
                st.input.release(input);
                st.stats.panics += 1;
                st.fail(id, PipelineError::Panicked);
                drop(st);
                sh.changed.notify_all();
                continue;
            }
        };
        match admit(sh, id, need, multi_scan) {
            Admit::Run => {}
            Admit::Cancelled => {
                drop(source);
                let mut st = sh.lock();
                st.input.release(input);
                st.cancel(id);
                drop(st);
                sh.changed.notify_all();
                continue;
            }
            Admit::Never { limit } => {
                drop(source);
                let mut st = sh.lock();
                st.input.release(input);
                st.fail(id, PipelineError::OverBudget { bytes: need, limit });
                drop(st);
                sh.changed.notify_all();
                continue;
            }
        }
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            sh.decoder
                .decode(&source.jpeg, source.orientation, &options)
        }));
        let source_bytes = source.jpeg.len() as u64;
        drop(source);
        let mut st = sh.lock();
        st.input.release(input);
        st.working.release(need);
        st.decodes -= 1;
        st.multi_scan -= usize::from(multi_scan);
        match result {
            Ok(Ok(preview)) => {
                let prepared = Prepared {
                    preview,
                    read,
                    source_bytes,
                    multi_scan,
                };
                store(&mut st, id, prepared);
            }
            Ok(Err(e)) => st.fail(id, PipelineError::Preview(e)),
            Err(_) => {
                st.stats.panics += 1;
                st.fail(id, PipelineError::Preview(PreviewError::Panicked));
            }
        }
        drop(st);
        sh.changed.notify_all();
    }
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
