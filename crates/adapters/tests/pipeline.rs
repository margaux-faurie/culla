//! Preview pipeline (US-08): prefetch, cancellation, the cache, the memory budgets, the
//! multi-scan gate and panic isolation, with a fake source and a fake decoder (the JPEGs are
//! frame headers only, which Culla's check accepts), then the real decoder on the synthetic
//! fixtures. Nothing is written.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use culla_adapters::pipeline::{
    Config, Decoder, FolderSource, Item, PhotoId, Pipeline, PipelineError, PreviewSource,
    ReadError, SourceJpeg, Status, Turbo,
};
use culla_adapters::preview::{Preview, PreviewError, PreviewOptions, Timings};
use culla_core::exif::Orientation;
use culla_core::prefetch::{Budgets, Capacity, Direction, Window, wanted};
use culla_core::preview::{self, Size};

const LONG: Duration = Duration::from_secs(20);

/// A JPEG made of its frame header only: SOI, SOF0 (or SOF2 for a progressive, multi-scan
/// frame), three components, one interleaved scan header, EOI. Width `100 + id` tags it.
fn header_jpeg(id: PhotoId, multi_scan: bool) -> Vec<u8> {
    let w = u16::try_from(100 + id).unwrap();
    let h: u16 = 80;
    let sof = if multi_scan { 0xC2 } else { 0xC0 };
    let mut j = vec![0xFF, 0xD8, 0xFF, sof, 0x00, 17, 8];
    j.extend_from_slice(&h.to_be_bytes());
    j.extend_from_slice(&w.to_be_bytes());
    j.push(3);
    for c in 1..=3 {
        j.extend_from_slice(&[c, 0x11, 0]);
    }
    j.extend_from_slice(&[0xFF, 0xDA, 0, 12, 3, 1, 0, 2, 0, 3, 0, 0, 0x3F, 0]);
    j.extend_from_slice(&[0xFF, 0xD9]);
    j
}

fn id_of(jpeg: &[u8]) -> PhotoId {
    usize::from(u16::from_be_bytes([jpeg[9], jpeg[10]])) - 100
}

#[derive(Default)]
struct FakeSource {
    len: usize,
    multi_scan: HashSet<PhotoId>,
    panic_on: HashSet<PhotoId>,
    fail_on: Arc<Mutex<HashSet<PhotoId>>>,
    reads: Arc<Mutex<Vec<PhotoId>>>,
}

impl FakeSource {
    fn new(len: usize) -> Self {
        Self {
            len,
            ..Self::default()
        }
    }
}

impl PreviewSource for FakeSource {
    fn read(
        &self,
        id: PhotoId,
        reserve: &mut dyn FnMut(u64) -> Result<(), ReadError>,
    ) -> Result<SourceJpeg, ReadError> {
        if id >= self.len {
            return Err(ReadError::Unknown);
        }
        self.reads.lock().unwrap().push(id);
        assert!(!self.panic_on.contains(&id), "source panic on {id}");
        if self.fail_on.lock().unwrap().contains(&id) {
            return Err(ReadError::Io("card pulled".into()));
        }
        let jpeg = header_jpeg(id, self.multi_scan.contains(&id));
        reserve(jpeg.len() as u64)?;
        Ok(SourceJpeg {
            jpeg,
            orientation: Orientation::Normal,
        })
    }
}

/// Shut until [`Gate::open`] (or [`LONG`]): holds decodes so a test can move the window while they run.
#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    fn open(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }

    /// Waits for the gate, at most [`LONG`]: a test that fails while it is shut still ends.
    fn pass(&self) {
        let open = self.open.lock().unwrap();
        let _ = self
            .changed
            .wait_timeout_while(open, LONG, |open| !*open)
            .unwrap();
    }
}

/// Plans like the real pipeline, waits at its gate (photos below `gated_below`), sleeps
/// `delay`, and returns a preview of `out_bytes` bytes.
struct FakeDecoder {
    delay: Duration,
    gate: Arc<Gate>,
    gated_below: PhotoId,
    started: Mutex<Vec<PhotoId>>,
    out_bytes: usize,
    panic_on: HashSet<PhotoId>,
    running: AtomicUsize,
    max_running: AtomicUsize,
    multi_running: AtomicUsize,
    max_multi_running: AtomicUsize,
    decoded: Mutex<Vec<PhotoId>>,
}

impl FakeDecoder {
    fn new(delay_ms: u64) -> Self {
        Self {
            delay: Duration::from_millis(delay_ms),
            gate: Arc::new(Gate::default()),
            gated_below: 0,
            started: Mutex::new(Vec::new()),
            out_bytes: 1000,
            panic_on: HashSet::new(),
            running: AtomicUsize::new(0),
            max_running: AtomicUsize::new(0),
            multi_running: AtomicUsize::new(0),
            max_multi_running: AtomicUsize::new(0),
            decoded: Mutex::new(Vec::new()),
        }
    }
}

impl Decoder for FakeDecoder {
    fn decode(
        &self,
        jpeg: &[u8],
        orientation: Orientation,
        options: &PreviewOptions,
    ) -> Result<Preview, PreviewError> {
        let id = id_of(jpeg);
        let multi = jpeg[3] == 0xC2;
        self.started.lock().unwrap().push(id);
        let n = self.running.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_running.fetch_max(n, Ordering::SeqCst);
        if multi {
            let m = self.multi_running.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_multi_running.fetch_max(m, Ordering::SeqCst);
        }
        if id < self.gated_below {
            self.gate.pass();
        }
        thread::sleep(self.delay);
        self.running.fetch_sub(1, Ordering::SeqCst);
        if multi {
            self.multi_running.fetch_sub(1, Ordering::SeqCst);
        }
        self.decoded.lock().unwrap().push(id);
        assert!(!self.panic_on.contains(&id), "decoder panic on {id}");
        let plan = preview::plan_for_jpeg(jpeg, orientation, options.target, &options.limits)
            .map_err(PreviewError::Plan)?;
        Ok(Preview {
            jpeg: vec![0; self.out_bytes],
            size: plan.output,
            plan,
            timings: Timings::default(),
        })
    }
}

fn config(workers: usize) -> Config {
    Config {
        readers: 1,
        workers,
        ..Config::default()
    }
}

fn start(
    source: FakeSource,
    decoder: &Arc<FakeDecoder>,
    config: Config,
) -> Pipeline<FakeSource, Arc<FakeDecoder>> {
    let order = (0..source.len).collect();
    Pipeline::start(source, Arc::clone(decoder), order, config).unwrap()
}

/// Waits until nothing is reserved in either budget (in-flight work ends on its own).
fn settles(p: &Pipeline<impl PreviewSource, impl Decoder>) {
    let deadline = Instant::now() + LONG;
    while p.in_use() != (0, 0) {
        assert!(
            Instant::now() < deadline,
            "still reserved: {:?}",
            p.in_use()
        );
        thread::sleep(Duration::from_millis(1));
    }
}

/// Polls `cond` until it holds.
fn until(mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + LONG;
    while !cond() {
        assert!(Instant::now() < deadline, "timed out");
        thread::sleep(Duration::from_millis(1));
    }
}

fn ready(p: &Pipeline<impl PreviewSource, impl Decoder>, id: PhotoId) {
    match p.wait(id, LONG) {
        Status::Ready(_) => {}
        other => panic!("{id}: {other:?}"),
    }
}

#[test]
fn prefetches_the_window_around_the_position() {
    let d = Arc::new(FakeDecoder::new(1));
    let p = start(FakeSource::new(30), &d, config(2));
    p.show(10, Direction::Forward);
    for id in [10, 11, 12, 13, 14, 15, 9, 8] {
        ready(&p, id);
    }
    assert_eq!(p.status(16), Status::NotWanted);
    assert_eq!(p.status(7), Status::NotWanted);
    let decoded: BTreeSet<_> = d.decoded.lock().unwrap().iter().copied().collect();
    assert_eq!(decoded, (8..=15).collect());
    // The current photo was read first.
    assert_eq!(p.stats().cached, 8);
    let first = d.decoded.lock().unwrap()[0];
    assert!([10, 11].contains(&first), "{first}");

    // Moving back prefetches earlier photos.
    p.show(9, Direction::Backward);
    for id in [9, 8, 7, 6, 5, 4, 10, 11] {
        ready(&p, id);
    }
    assert!(p.get(4).is_some());
}

#[test]
fn the_order_maps_positions_to_photos() {
    let d = Arc::new(FakeDecoder::new(0));
    let source = FakeSource::new(5);
    let p = Pipeline::start(source, Arc::clone(&d), vec![4, 3, 2, 1, 0], config(2)).unwrap();
    p.show(0, Direction::Forward);
    ready(&p, 4);
    ready(&p, 0);
    p.set_order(vec![0, 1, 2, 3, 4]);
    p.show(4, Direction::Forward);
    ready(&p, 4);
    ready(&p, 2);
}

#[test]
fn moving_the_window_cancels_stale_prefetches() {
    let mut decoder = FakeDecoder::new(0);
    decoder.gated_below = 10;
    let d = Arc::new(decoder);
    let source = FakeSource::new(100);
    let reads = Arc::clone(&source.reads);
    let p = start(source, &d, config(1));
    p.show(0, Direction::Forward);
    // The reader queues the old window (0 to 5) and the only worker is held decoding 0.
    until(|| reads.lock().unwrap().len() >= 6 && d.running.load(Ordering::SeqCst) == 1);
    p.show(50, Direction::Forward);
    d.gate.open();
    for id in [50, 51, 52, 53, 54, 55, 49, 48] {
        ready(&p, id);
    }
    let old: Vec<_> = d
        .decoded
        .lock()
        .unwrap()
        .iter()
        .copied()
        .filter(|id| *id < 10)
        .collect();
    // Only the decode running at the jump finished, and its result was dropped, not cached;
    // the five queued photos were dropped unread by the worker.
    assert_eq!(old.len(), 1, "{old:?}");
    assert_eq!(p.stats().cancelled, 6, "{:?}", p.stats());
    assert_eq!(p.stats().cached, 8);
    for id in 0..6 {
        assert_eq!(p.status(id), Status::NotWanted, "{id}");
    }
    settles(&p);
}

#[test]
fn a_waiting_current_photo_is_admitted_before_lower_prefetches() {
    let need = |id| {
        preview::plan_for_jpeg(
            &header_jpeg(id, false),
            Orientation::Normal,
            PreviewOptions::default().target,
            &PreviewOptions::default().limits,
        )
        .unwrap()
        .working_bytes()
    };
    let mut decoder = FakeDecoder::new(0);
    decoder.gated_below = 1;
    let d = Arc::new(decoder);
    let mut c = config(3);
    // Room for one call at a time.
    c.budgets.working = need(2) + need(0) / 2;
    let p = start(FakeSource::new(3), &d, c);
    // 0 decodes, held at the gate; then 1 waits for memory.
    p.set_wanted([0]);
    until(|| d.running.load(Ordering::SeqCst) == 1);
    p.set_wanted([0, 1]);
    until(|| p.waiting() == 1);
    // 2 comes in as the current photo: it waits for memory too and holds 1 back.
    p.set_wanted([2, 0, 1]);
    until(|| p.waiting() == 2);
    d.gate.open();
    for id in 0..3 {
        ready(&p, id);
    }
    assert_eq!(*d.started.lock().unwrap(), vec![0, 2, 1]);
    settles(&p);
}

#[test]
fn a_preview_the_cache_refused_is_reported_not_pending() {
    let d = Arc::new(FakeDecoder::new(0));
    let mut c = config(1);
    c.window = Window {
        ahead: 1,
        behind: 0,
    };
    c.capacity = Capacity {
        max_entries: 1,
        max_bytes: 1 << 20,
    };
    let p = start(FakeSource::new(5), &d, c);
    p.show(0, Direction::Forward);
    ready(&p, 0);
    // 1 is made, but the cache holds only the current photo, which ranks higher.
    assert_eq!(p.wait(1, LONG), Status::NotAdmitted);
    assert_eq!(p.status(1), Status::NotAdmitted);
    assert_eq!(p.stats().not_admitted, 1);
    // Once the window moves, it is made again and admitted.
    p.show(1, Direction::Forward);
    ready(&p, 1);
    settles(&p);
}

#[test]
fn status_does_not_change_the_eviction_order() {
    let d = Arc::new(FakeDecoder::new(0));
    let mut c = config(1);
    c.window = Window {
        ahead: 0,
        behind: 0,
    };
    c.capacity = Capacity {
        max_entries: 2,
        max_bytes: 1 << 20,
    };
    let p = start(FakeSource::new(5), &d, c);
    p.show(0, Direction::Forward);
    ready(&p, 0);
    p.show(1, Direction::Forward);
    ready(&p, 1);
    // 0 is the least recently used; looking at it does not change that.
    for _ in 0..3 {
        assert!(matches!(p.status(0), Status::Ready(_)));
    }
    p.show(2, Direction::Forward);
    ready(&p, 2);
    assert_eq!(p.status(0), Status::NotWanted);
    assert!(matches!(p.status(1), Status::Ready(_)));
}

#[test]
fn a_panic_fails_one_photo_and_the_pool_carries_on() {
    let mut source = FakeSource::new(30);
    source.panic_on.insert(2);
    let mut decoder = FakeDecoder::new(1);
    decoder.panic_on.insert(4);
    let d = Arc::new(decoder);
    let p = start(source, &d, config(2));
    p.show(0, Direction::Forward);
    assert_eq!(p.wait(2, LONG), Status::Failed(PipelineError::Panicked));
    assert_eq!(
        p.wait(4, LONG),
        Status::Failed(PipelineError::Preview(PreviewError::Panicked))
    );
    for id in [0, 1, 3, 5] {
        ready(&p, id);
    }
    // Every thread is still there: a full new window comes through.
    p.show(20, Direction::Forward);
    for id in 18..=25 {
        ready(&p, id);
    }
    let s = p.stats();
    assert_eq!(s.panics, 2);
    assert_eq!(s.failed, 2);
    settles(&p);
}

#[test]
fn one_multi_scan_decode_at_a_time() {
    let mut source = FakeSource::new(8);
    source.multi_scan = (0..8).collect();
    let d = Arc::new(FakeDecoder::new(20));
    let mut c = config(4);
    c.window = Window {
        ahead: 7,
        behind: 0,
    };
    let p = start(source, &d, c);
    p.show(0, Direction::Forward);
    for id in 0..8 {
        ready(&p, id);
    }
    assert_eq!(d.max_multi_running.load(Ordering::SeqCst), 1);
    assert_eq!(p.stats().peak_multi_scan, 1);
    let got = p.get(3).unwrap();
    assert!(got.multi_scan);

    // Single-scan photos run on several workers at once.
    let d = Arc::new(FakeDecoder::new(20));
    let p = start(FakeSource::new(8), &d, c);
    p.show(0, Direction::Forward);
    for id in 0..8 {
        ready(&p, id);
    }
    assert!(d.max_running.load(Ordering::SeqCst) >= 2);
}

#[test]
fn the_working_budget_bounds_parallel_decodes() {
    let need = {
        let j = header_jpeg(0, false);
        preview::plan_for_jpeg(
            &j,
            Orientation::Normal,
            PreviewOptions::default().target,
            &PreviewOptions::default().limits,
        )
        .unwrap()
        .working_bytes()
    };
    let d = Arc::new(FakeDecoder::new(10));
    let mut c = config(4);
    c.budgets = Budgets {
        input: Budgets::DEFAULT.input,
        working: need + need / 2,
    };
    let p = start(FakeSource::new(10), &d, c);
    p.show(0, Direction::Forward);
    for id in 0..6 {
        ready(&p, id);
    }
    assert_eq!(d.max_running.load(Ordering::SeqCst), 1);
    let s = p.stats();
    assert_eq!(s.peak_decodes, 1);
    assert!(s.peak_working <= c.budgets.working);
}

#[test]
fn work_larger_than_a_budget_fails_that_photo() {
    let d = Arc::new(FakeDecoder::new(0));
    let mut c = config(2);
    c.budgets.working = 1024;
    let p = start(FakeSource::new(3), &d, c);
    p.show(0, Direction::Forward);
    assert!(matches!(
        p.wait(0, LONG),
        Status::Failed(PipelineError::OverBudget { limit: 1024, .. })
    ));
    settles(&p);

    let mut c = config(2);
    c.budgets.input = 10;
    let p = start(FakeSource::new(3), &d, c);
    p.show(0, Direction::Forward);
    assert!(matches!(
        p.wait(0, LONG),
        Status::Failed(PipelineError::Read(ReadError::OverBudget { limit: 10, .. }))
    ));
    settles(&p);
}

#[test]
fn the_cache_keeps_to_its_capacity_while_swiping() {
    let d = Arc::new(FakeDecoder::new(0));
    let mut c = config(2);
    c.capacity = Capacity {
        max_entries: 4,
        max_bytes: 3500,
    };
    let p = start(FakeSource::new(40), &d, c);
    for pos in 0..30 {
        p.show(pos, Direction::Forward);
        ready(&p, pos);
        // Let the whole window settle: each photo is cached or refused, never left pending.
        let window = wanted(pos, 40, Direction::Forward, Window::DEFAULT);
        for &id in &window {
            let s = p.wait(id, LONG);
            assert!(
                matches!(s, Status::Ready(_) | Status::NotAdmitted),
                "{id}: {s:?}"
            );
        }
        let (n, bytes) = p.cached();
        assert!(n <= 3 && bytes <= 3500, "{n} {bytes}");
    }
    let s = p.stats();
    assert!(s.evicted > 0);
    // The window (8) is larger than the cache (3 by bytes): the lowest-ranked were refused,
    // never the current photo.
    assert!(s.not_admitted > 0);
    assert!(p.get(29).is_some());
}

#[test]
fn a_failure_is_kept_until_retried() {
    let source = FakeSource::new(5);
    let fail_on = Arc::clone(&source.fail_on);
    fail_on.lock().unwrap().insert(1);
    let d = Arc::new(FakeDecoder::new(0));
    let p = start(source, &d, config(2));
    p.show(0, Direction::Forward);
    assert_eq!(
        p.wait(1, LONG),
        Status::Failed(PipelineError::Read(ReadError::Io("card pulled".into())))
    );
    ready(&p, 2);
    // Still failed, not read again, until asked.
    p.show(1, Direction::Forward);
    assert!(matches!(p.status(1), Status::Failed(_)));
    assert!(!p.retry(3));
    // The card is back.
    fail_on.lock().unwrap().clear();
    assert!(p.retry(1));
    ready(&p, 1);
}

#[test]
fn an_unknown_photo_fails() {
    let d = Arc::new(FakeDecoder::new(0));
    let p = Pipeline::start(FakeSource::new(2), Arc::clone(&d), vec![0, 1, 9], config(1)).unwrap();
    p.show(2, Direction::Forward);
    assert_eq!(
        p.wait(9, LONG),
        Status::Failed(PipelineError::Read(ReadError::Unknown))
    );
}

#[test]
fn a_broken_jpeg_fails_before_the_decoder() {
    struct Broken;
    impl PreviewSource for Broken {
        fn read(
            &self,
            _: PhotoId,
            reserve: &mut dyn FnMut(u64) -> Result<(), ReadError>,
        ) -> Result<SourceJpeg, ReadError> {
            reserve(4)?;
            Ok(SourceJpeg {
                jpeg: vec![0xFF, 0xD8, 0xFF, 0xD9],
                orientation: Orientation::Normal,
            })
        }
    }
    let d = Arc::new(FakeDecoder::new(0));
    let p = Pipeline::start(Broken, Arc::clone(&d), vec![0], config(1)).unwrap();
    p.show(0, Direction::Forward);
    assert!(matches!(
        p.wait(0, LONG),
        Status::Failed(PipelineError::Preview(PreviewError::Plan(_)))
    ));
    assert!(d.decoded.lock().unwrap().is_empty());
}

#[test]
fn dropping_the_pipeline_stops_its_threads() {
    let d = Arc::new(FakeDecoder::new(30));
    let p = start(FakeSource::new(50), &d, config(4));
    p.show(0, Direction::Forward);
    thread::sleep(Duration::from_millis(5));
    drop(p);
    // Every thread has returned: only this test's reference is left.
    assert_eq!(Arc::strong_count(&d), 1);
}

fn synthetic(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/synthetic")
        .join(rel)
}

#[test]
fn real_decoder_on_the_synthetic_fixtures() {
    let items = vec![
        Item::Jpeg {
            path: synthetic("pair/DSC_0001.JPG"),
            orientation: Orientation::Normal,
        },
        Item::LoneNef {
            path: synthetic("lone/DSC_0002.NEF"),
            orientation: Orientation::Rotate90,
        },
        Item::Jpeg {
            path: synthetic("does-not-exist.JPG"),
            orientation: Orientation::Normal,
        },
        // Not a TIFF: no embedded JPEG to find.
        Item::LoneNef {
            path: synthetic("pair/DSC_0001.JPG"),
            orientation: Orientation::Normal,
        },
    ];
    let p = Pipeline::start(FolderSource::new(items), Turbo, vec![0, 1, 2, 3], config(2)).unwrap();
    p.show(0, Direction::Forward);
    let Status::Ready(jpeg) = p.wait(0, LONG) else {
        panic!("pair JPEG")
    };
    assert_eq!(jpeg.preview.size, Size::new(8, 8));
    assert_eq!(&jpeg.preview.jpeg[..2], &[0xFF, 0xD8]);
    let Status::Ready(nef) = p.wait(1, LONG) else {
        panic!("lone NEF")
    };
    // The 48x32 embedded JPEG, turned upright.
    assert_eq!(nef.preview.size, Size::new(32, 48));
    assert!(matches!(
        p.wait(2, LONG),
        Status::Failed(PipelineError::Read(ReadError::Io(_)))
    ));
    let s = p.wait(3, LONG);
    assert!(
        matches!(
            s,
            Status::Failed(PipelineError::Read(ReadError::NoPreview(_)))
        ),
        "{s:?}"
    );
}
