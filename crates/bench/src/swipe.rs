//! `culla-bench swipe` (B-09, B-10, B-11, B-15; NFR-PERF-03, -04, -09): replays a decision
//! sequence ([`cadence::moves`]) at a cadence against the real preview pipeline
//! (`culla_adapters::pipeline`: reader thread, decode workers, prefetch window, in-memory
//! cache), after a scan and EXIF pass, in capture-time order.
//!
//! Next-photo latency is the time from an advance ([`Pipeline::show`]) to the photo's preview
//! being in the in-memory cache: the headless proxy of B-09. The next advance comes at the
//! cadence's gap after the previous one, but never before the current photo is ready (a user
//! cannot decide on a photo they have not seen). Read-only, like the pipeline.

use std::time::{Duration, Instant};

use culla_adapters::pipeline::{Config, FolderSource, Pipeline, Status, Turbo};
use culla_core::prefetch::{Capacity, Direction, Window};

use crate::args::SwipeArgs;
use crate::cadence::{self, Cadence, RampTracker};
use crate::folder::{self, Failure, SourceChoice};
use crate::json::Value;
use crate::run::{Metric, Run, Samples, Unit};
use crate::scan::peak_metric;
use crate::stats::{ms, per_s};

/// The longest wait for one photo before it counts as failed (timed out).
const PHOTO_TIMEOUT: Duration = Duration::from_secs(120);

/// When to make the next advance: `gap` after the previous advance, but not before the
/// previous photo was ready.
#[must_use]
pub(crate) fn next_advance(prev_advance: Instant, gap: Duration, prev_ready: Instant) -> Instant {
    (prev_advance + gap).max(prev_ready)
}

/// The pipeline configuration: its defaults, with the options given.
#[must_use]
pub(crate) fn config(a: &SwipeArgs) -> Config {
    let d = Config::default();
    Config {
        readers: a.readers.unwrap_or(d.readers),
        workers: a.workers.unwrap_or(d.workers),
        window: Window {
            ahead: a.ahead.unwrap_or(d.window.ahead),
            behind: a.behind.unwrap_or(d.window.behind),
        },
        capacity: Capacity {
            max_entries: a.lru.unwrap_or(d.capacity.max_entries),
            // `--lru-mb` is capped at parse time; saturating all the same.
            max_bytes: a
                .lru_mb
                .map_or(d.capacity.max_bytes, |m| m.saturating_mul(1024 * 1024)),
        },
        ..d
    }
}

/// Sleeps until `at`: in steps while far, then yielding for the last two milliseconds, so the
/// cadence does not depend on the timer resolution.
fn sleep_until(at: Instant) {
    loop {
        let now = Instant::now();
        if now >= at {
            return;
        }
        let left = at - now;
        if left > Duration::from_millis(2) {
            std::thread::sleep(left - Duration::from_millis(2));
        } else {
            std::thread::yield_now();
        }
    }
}

/// How waiting for one photo ended.
#[derive(Debug, PartialEq, Eq)]
enum Waited {
    Ready,
    /// Made, but the cache refused it (everything it holds ranks higher). The pipeline will
    /// not make it again until the window changes, so waiting longer cannot help.
    NotAdmitted,
    /// Failed or timed out, with why.
    Failed(String),
}

/// Waits for `id` at `position`, re-asking when the pipeline reports it not wanted.
fn wait_ready(
    p: &Pipeline<FolderSource, Turbo>,
    id: usize,
    position: usize,
    direction: Direction,
) -> Waited {
    let deadline = Instant::now() + PHOTO_TIMEOUT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Waited::Failed("timed out".to_owned());
        }
        match p.wait(id, left) {
            Status::Ready(_) => return Waited::Ready,
            Status::Failed(e) => return Waited::Failed(e.to_string()),
            Status::NotAdmitted => return Waited::NotAdmitted,
            Status::Pending => {}
            Status::NotWanted => p.show(position, direction),
        }
    }
}

/// Runs the benchmark.
///
/// # Errors
///
/// When the folder is refused or cannot be read, or the pipeline cannot start.
#[allow(
    clippy::too_many_lines,
    reason = "one measured loop, kept in one place"
)]
pub(crate) fn run(a: &SwipeArgs) -> Result<Run, Failure> {
    let (root, medium) = folder::resolve(&a.common.folder)?;
    let scanned = folder::scan_folder(&root, a.common.include_subfolders, |_, _| {})?;
    let groups = &scanned.grouping.groups;
    let exif = folder::exif_pass(&root, groups);
    let orientations = folder::orientations(&exif.meta);
    let mut from_nef = 0usize;
    let items = groups
        .iter()
        .zip(&orientations)
        .map(|(g, o)| {
            let (item, took) = folder::item_for(&root, g, *o, a.source);
            from_nef += usize::from(took == SourceChoice::Nef);
            item
        })
        .collect();
    let order = exif.order.clone();
    let config = config(a);
    let pipeline = Pipeline::start(FolderSource::new(items), Turbo, order.clone(), config)
        .map_err(|e| Failure::Io(format!("cannot start the pipeline: {e}")))?;

    let moves = cadence::moves(order.len(), a.steps, a.back_percent, a.seed);
    let mut notes = Vec::new();
    let mut latencies = Vec::with_capacity(moves.len());
    let mut lateness = Vec::with_capacity(moves.len());
    let mut prefetched = 0usize;
    let mut failed = 0usize;
    let mut refused = 0usize;
    let mut ramp = RampTracker::new(a.ramp);
    let mut stopped_early = false;

    // The first photo: shown cold, before any prefetch, but after the scan and the EXIF pass
    // (so not B-01: `culla-bench scan` measures that).
    let started = Instant::now();
    let first = if order.is_empty() {
        notes.push("no photo group found".to_owned());
        None
    } else {
        pipeline.show(0, Direction::Forward);
        match wait_ready(&pipeline, order[0], 0, Direction::Forward) {
            Waited::Ready => Some(started.elapsed()),
            Waited::NotAdmitted => {
                notes.push("the first photo was not admitted to the cache".to_owned());
                None
            }
            Waited::Failed(e) => {
                notes.push(format!("the first photo failed: {e}"));
                None
            }
        }
    };

    let mut prev_advance = started;
    let mut prev_ready = Instant::now();
    let mut done = 0usize;
    for (i, mv) in moves.iter().enumerate() {
        let target = next_advance(prev_advance, a.cadence.gap(i, &a.ramp), prev_ready);
        sleep_until(target);
        let id = order[mv.position];
        if matches!(pipeline.status(id), Status::Ready(_)) {
            prefetched += 1;
        }
        let at = Instant::now();
        lateness.push(ms(at - target));
        pipeline.show(mv.position, mv.direction);
        let result = wait_ready(&pipeline, id, mv.position, mv.direction);
        let ready = Instant::now();
        let latency = match result {
            Waited::Ready => {
                let l = ms(ready - at);
                latencies.push(l);
                Some(l)
            }
            Waited::NotAdmitted => {
                refused += 1;
                if refused <= 20 {
                    notes.push(format!("advance {}: not admitted to the cache", i + 1));
                }
                None
            }
            Waited::Failed(e) => {
                failed += 1;
                if failed <= 20 {
                    notes.push(format!("advance {} failed: {e}", i + 1));
                }
                None
            }
        };
        prev_advance = at;
        prev_ready = ready;
        done += 1;
        folder::progress("swipe", done, moves.len());
        if a.cadence == Cadence::Ramp && ramp.push(latency) {
            stopped_early = done < moves.len();
            break;
        }
    }
    let elapsed = prev_ready.saturating_duration_since(started);
    folder::progress_done("swipe", done);
    if failed > 20 {
        notes.push(format!("{} more failures not listed", failed - 20));
    }
    if refused > 20 {
        notes.push(format!(
            "{} more advances not admitted not listed",
            refused - 20
        ));
    }
    if failed + refused > 0 {
        notes.push("next_photo_ms covers the advances whose photo was shown only".to_owned());
    }

    let stats = pipeline.stats();
    let (cached_n, cached_bytes) = pipeline.cached();
    drop(pipeline);

    #[allow(clippy::cast_precision_loss, reason = "a ratio of counts")]
    let prefetched_pct = (done > 0).then(|| prefetched as f64 * 100.0 / done as f64);
    #[allow(clippy::cast_precision_loss, reason = "byte counts")]
    let bytes = |b: u64| Some(b as f64);
    let mut metrics = vec![
        Metric::new("first_preview_ms", "", Unit::Ms, first.map(ms)),
        Metric::count("advances", done as u64),
        Metric::count("failed", failed as u64),
        Metric::count("not_admitted_advances", refused as u64),
        Metric::new("prefetched", "B-09", Unit::Percent, prefetched_pct),
        Metric::new("advances_per_s", "B-10", Unit::PerS, per_s(done, elapsed)),
        Metric::count("groups", order.len() as u64),
        Metric::count("from_nef", from_nef as u64),
        Metric::new("exif_ms", "B-03", Unit::Ms, Some(ms(exif.pass))),
    ];
    if a.cadence == Cadence::Ramp {
        metrics.push(Metric::new(
            "sustained_rate_per_s",
            "B-10",
            Unit::PerS,
            ramp.sustained(),
        ));
        for b in ramp.blocks() {
            notes.push(format!(
                "ramp {:.1} photos/s: p95 {:.1} ms, {} not shown, {}",
                b.rate,
                b.p95_ms,
                b.failed,
                if b.passed {
                    "within budget"
                } else if b.failed > 0 {
                    "failed (an advance was not shown)"
                } else {
                    "over budget"
                }
            ));
        }
        if !stopped_early && ramp.blocks().last().is_none_or(|b| b.passed) {
            // An advance that was not shown can sit in the last, unfinished block: the ramp
            // then did not stay within budget, so say so instead of claiming it did.
            let not_shown = failed + refused;
            notes.push(if not_shown == 0 && ramp.unfinished_within_budget() {
                "the ramp ran out of photos before p95 went over budget".to_owned()
            } else if not_shown == 0 {
                "the ramp ran out of photos in an unfinished block whose p95 was over budget"
                    .to_owned()
            } else {
                format!(
                    "the ramp ran out of photos with {not_shown} advances not shown, so it did \
                     not stay within budget"
                )
            });
        }
    }
    metrics.extend([
        Metric::new(
            "pipeline_peak_input_bytes",
            "B-15",
            Unit::Bytes,
            bytes(stats.peak_input),
        ),
        Metric::new(
            "pipeline_peak_working_bytes",
            "B-15",
            Unit::Bytes,
            bytes(stats.peak_working),
        ),
        Metric::new(
            "cache_bytes_at_end",
            "B-15",
            Unit::Bytes,
            bytes(cached_bytes),
        ),
        Metric::count("cache_previews_at_end", cached_n as u64),
        Metric::count("peak_decodes", stats.peak_decodes as u64),
        Metric::count("cancelled", stats.cancelled),
        Metric::count("evicted", stats.evicted),
        Metric::count("not_admitted", stats.not_admitted),
        Metric::count("panics", stats.panics),
    ]);
    metrics.push(peak_metric(&mut notes));

    let opt = |v: Option<usize>| Value::opt(v, |n| Value::int(n as u64));
    Ok(Run {
        benchmark: "swipe",
        label: a.common.label.clone(),
        folder: a.common.folder.clone(),
        medium,
        params: vec![
            (
                "include_subfolders",
                Value::Bool(a.common.include_subfolders),
            ),
            ("cadence", Value::str(a.cadence.name())),
            ("preview_source", Value::str(a.source.name())),
            ("readers", Value::int(config.readers as u64)),
            ("workers", Value::int(config.workers as u64)),
            ("ahead", Value::int(config.window.ahead as u64)),
            ("behind", Value::int(config.window.behind as u64)),
            ("lru", Value::int(config.capacity.max_entries as u64)),
            ("lru_bytes", Value::int(config.capacity.max_bytes)),
            ("steps", opt(a.steps)),
            ("back_percent", Value::int(a.back_percent)),
            ("seed", Value::int(a.seed)),
            ("budget_ms", Value::Num(a.ramp.budget_ms)),
        ],
        metrics,
        samples: vec![
            Samples {
                key: "next_photo_ms",
                id: "B-09",
                unit: Unit::Ms,
                values: latencies,
            },
            Samples {
                key: "advance_late_ms",
                id: "",
                unit: Unit::Ms,
                values: lateness,
            },
        ],
        notes,
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;
    use crate::args::{Command, parse};

    #[test]
    fn next_advance_waits_for_the_gap_and_the_photo() {
        let t0 = Instant::now();
        let gap = Duration::from_millis(400);
        // The photo was ready early: the gap decides.
        assert_eq!(
            next_advance(t0, gap, t0 + Duration::from_millis(30)),
            t0 + gap
        );
        // The photo came late: the user decides once it is shown.
        let late = t0 + Duration::from_millis(900);
        assert_eq!(next_advance(t0, gap, late), late);
        // No gap (max cadence): as soon as it is shown.
        assert_eq!(next_advance(t0, Duration::ZERO, late), late);
    }

    #[test]
    fn config_takes_the_pipeline_defaults_and_the_options() {
        let args = |v: &[&str]| match parse(v.iter().map(OsString::from)).unwrap() {
            Command::Swipe(a) => a,
            _ => panic!(),
        };
        let d = Config::default();
        assert_eq!(config(&args(&["swipe", "f"])), d);
        let c = config(&args(&[
            "swipe",
            "f",
            "--readers",
            "2",
            "--workers",
            "3",
            "--ahead",
            "6",
            "--behind",
            "1",
            "--lru",
            "50",
            "--lru-mb",
            "96",
        ]));
        assert_eq!((c.readers, c.workers), (2, 3));
        assert_eq!((c.window.ahead, c.window.behind), (6, 1));
        assert_eq!(c.capacity.max_entries, 50);
        assert_eq!(c.capacity.max_bytes, 96 * 1024 * 1024);
        assert_eq!(c.budgets, d.budgets);
        assert_eq!(c.options, d.options);
        let c = config(&args(&["swipe", "f", "--lru-mb", "65536"]));
        assert_eq!(c.capacity.max_bytes, 65536 * 1024 * 1024);
    }
}
