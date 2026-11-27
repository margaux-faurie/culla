//! `culla preview`: wiring only. Scans the folder like `culla scan` ([`crate::scan::run`]),
//! then runs every group through the preview pipeline (`culla_adapters::pipeline`: reader
//! thread, decode workers, prefetch window, in-memory cache) in capture-time order, as a user
//! swiping forward as fast as the previews come, and reports the time of each step.
//!
//! It reads the source folder only and writes nothing but stdout and stderr (ADR 0020,
//! file-engine INV-03): each preview is measured and dropped. Each row keeps only the numbers
//! it prints, so memory does not grow with the number of photos.

use std::fmt::Write as _;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use culla_adapters::pipeline::{
    Config, FolderSource, Pipeline, PipelineError, Prepared, Stats, Status, Turbo,
};
use culla_adapters::preview::Timings;
use culla_core::exif::Orientation;
use culla_core::prefetch::Direction;
use culla_core::preview::Size;

use crate::args::{PreviewArgs, ScanArgs};
use crate::scan::{self, Failure, GroupKind, Progress, group_kind};
use crate::text::shown;

/// What a row prints of a preview that was made; the pixels and the JPEG are not kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Measured {
    pub timings: Timings,
    /// Reading the source JPEG, budget waits included.
    pub read: Duration,
    pub size: Size,
    /// Bytes of the encoded preview.
    pub bytes: usize,
}

impl Measured {
    fn of(p: &Prepared) -> Self {
        Self {
            timings: p.preview.timings,
            read: p.read,
            size: p.preview.size,
            bytes: p.preview.jpeg.len(),
        }
    }
}

/// One photo's result.
#[derive(Debug)]
pub(crate) struct Row {
    /// 1-based position in capture-time order.
    pub position: usize,
    /// The primary's path relative to the scanned folder.
    pub primary: String,
    /// Where the preview came from: `jpeg` or `nef` (a lone NEF's embedded JPEG).
    pub source: &'static str,
    /// From asking for the photo to its preview being ready (prefetch hides part of it).
    pub wait: Duration,
    pub result: Result<Measured, PipelineError>,
}

/// Everything `culla preview` prints.
#[derive(Debug)]
pub(crate) struct Report {
    pub folder: PathBuf,
    pub groups: usize,
    pub rows: Vec<Row>,
    pub elapsed: Duration,
    pub stats: Stats,
    pub config: Config,
    pub cached: (usize, u64),
}

/// Runs the scan, then the previews.
///
/// # Errors
///
/// As `culla scan`, and [`Failure::Io`] when the pipeline cannot start.
pub(crate) fn run(args: &PreviewArgs, progress: &mut Progress) -> Result<Report, Failure> {
    let scan_args = ScanArgs {
        folder: args.folder.clone(),
        include_subfolders: args.include_subfolders,
        json: false,
    };
    let scanned = scan::run(&scan_args, progress)?;

    let groups = &scanned.grouping.groups;
    let orientations: Vec<Orientation> = scanned
        .meta
        .iter()
        .map(|m| m.as_ref().map_or(Orientation::Normal, |m| m.orientation))
        .collect();
    let source = FolderSource::from_groups(&scanned.scanned, groups, &orientations);
    let config = Config::default();
    let pipeline = Pipeline::start(source, Turbo, scanned.order.clone(), config)
        .map_err(|e| Failure::Io(format!("cannot start the pipeline: {e}")))?;

    let started = Instant::now();
    let total = scanned.order.len();
    let mut rows = Vec::with_capacity(total);
    for (pos, &i) in scanned.order.iter().enumerate() {
        let group = &groups[i];
        let asked = Instant::now();
        pipeline.show(pos, Direction::Forward);
        let result = loop {
            match pipeline.wait(i, Duration::from_secs(60)) {
                // Measure, then drop this handle: only the cache keeps the preview.
                Status::Ready(p) => break Ok(Measured::of(&p)),
                Status::Failed(e) => break Err(e),
                Status::Pending => {}
                // Not expected (the current photo leads the window and ranks first in the
                // cache); ask again.
                Status::NotWanted | Status::NotAdmitted => pipeline.show(pos, Direction::Forward),
            }
        };
        let wait = asked.elapsed();
        rows.push(Row {
            position: pos + 1,
            primary: group.primary.entry.rel_path(),
            source: match group_kind(group) {
                GroupKind::LoneNef => "nef",
                GroupKind::Pair | GroupKind::JpegOnly => "jpeg",
            },
            wait,
            result,
        });
        progress_line(pos + 1, total);
    }
    let elapsed = started.elapsed();
    progress_done(total);
    Ok(Report {
        folder: args.folder.clone(),
        groups: total,
        rows,
        elapsed,
        stats: pipeline.stats(),
        config,
        cached: pipeline.cached(),
    })
}

fn progress_line(done: usize, total: usize) {
    let err = io::stderr();
    // Best effort, like the scan's progress.
    let _ = if err.is_terminal() {
        write!(err.lock(), "\r\x1b[2Kpreviewing: {done} of {total}")
    } else if done.is_multiple_of(100) {
        writeln!(err.lock(), "previewing: {done} of {total}")
    } else {
        Ok(())
    };
}

fn progress_done(total: usize) {
    let err = io::stderr();
    let _ = if err.is_terminal() {
        writeln!(err.lock(), "\r\x1b[2Kpreviewed: {total}")
    } else {
        writeln!(err.lock(), "previewed: {total}")
    };
}

fn ms(d: Duration) -> String {
    format!("{:.1}", d.as_secs_f64() * 1000.0)
}

fn mb(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss, reason = "a display value")]
    let m = bytes as f64 / 1_000_000.0;
    format!("{m:.1} MB")
}

/// The `q`-quantile (0 to 1) of sorted `v`, nearest rank.
fn quantile(v: &[Duration], q: f64) -> Duration {
    if v.is_empty() {
        return Duration::ZERO;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a rank within the slice"
    )]
    let rank = ((q * v.len() as f64).ceil() as usize).clamp(1, v.len());
    v[rank - 1]
}

/// The report as text, ending with a newline. Times in milliseconds.
#[must_use]
pub(crate) fn render(r: &Report) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "folder:   {}", shown(&r.folder.to_string_lossy()));
    let _ = writeln!(
        o,
        "pipeline: {} reader, {} decode workers, prefetch {} ahead and {} behind, cache {} previews or {}",
        r.config.readers,
        r.config.workers,
        r.config.window.ahead,
        r.config.window.behind,
        r.config.capacity.max_entries,
        mb(r.config.capacity.max_bytes),
    );
    let _ = writeln!(o);
    let _ = writeln!(
        o,
        "{:>5}  {:<4}  {:>7}  {:>6}  {:>7}  {:>7}  {:>7}  {:>7}  {:>7}  {:>9}  {:>8}  file",
        "pos",
        "from",
        "read",
        "check",
        "decode",
        "resize",
        "orient",
        "encode",
        "wait",
        "size",
        "bytes"
    );
    for row in &r.rows {
        let file = shown(&row.primary);
        match &row.result {
            Ok(m) => {
                let t = m.timings;
                let size = format!("{}x{}", m.size.width, m.size.height);
                let _ = writeln!(
                    o,
                    "{:>5}  {:<4}  {:>7}  {:>6}  {:>7}  {:>7}  {:>7}  {:>7}  {:>7}  {:>9}  {:>8}  {file}",
                    row.position,
                    row.source,
                    ms(m.read),
                    ms(t.check),
                    ms(t.decode),
                    ms(t.resize),
                    ms(t.orient),
                    ms(t.encode),
                    ms(row.wait),
                    size,
                    m.bytes,
                );
            }
            Err(e) => {
                let _ = writeln!(
                    o,
                    "{:>5}  {:<4}  failed: {}  {file}",
                    row.position,
                    row.source,
                    shown(&e.to_string())
                );
            }
        }
    }
    let made = r.rows.iter().filter(|x| x.result.is_ok()).count();
    let failed = r.rows.len() - made;
    let secs = r.elapsed.as_secs_f64();
    #[allow(clippy::cast_precision_loss, reason = "a display value")]
    let rate = if secs > 0.0 { made as f64 / secs } else { 0.0 };
    let mut waits: Vec<Duration> = r
        .rows
        .iter()
        .filter(|x| x.result.is_ok())
        .map(|x| x.wait)
        .collect();
    waits.sort_unstable();
    let _ = writeln!(o);
    let _ = writeln!(
        o,
        "previews: {made} made, {failed} failed, of {} groups, in {:.3} s ({rate:.1} per second)",
        r.groups, secs
    );
    let _ = writeln!(
        o,
        "wait:     p50 {} ms, p95 {} ms, max {} ms",
        ms(quantile(&waits, 0.5)),
        ms(quantile(&waits, 0.95)),
        ms(waits.last().copied().unwrap_or_default()),
    );
    let s = r.stats;
    let _ = writeln!(
        o,
        "memory:   peak input {}, peak working {}, peak decodes {} ({} multi-scan), cache {} previews, {}",
        mb(s.peak_input),
        mb(s.peak_working),
        s.peak_decodes,
        s.peak_multi_scan,
        r.cached.0,
        mb(r.cached.1),
    );
    let _ = writeln!(
        o,
        "pipeline: {} cancelled, {} evicted, {} not admitted, {} panics caught",
        s.cancelled, s.evicted, s.not_admitted, s.panics
    );
    let _ = writeln!(o, "written:  nothing (read-only)");
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantiles_by_nearest_rank() {
        let v: Vec<Duration> = (1..=20).map(Duration::from_millis).collect();
        assert_eq!(quantile(&v, 0.5), Duration::from_millis(10));
        assert_eq!(quantile(&v, 0.95), Duration::from_millis(19));
        assert_eq!(quantile(&v, 1.0), Duration::from_millis(20));
        assert_eq!(quantile(&[], 0.5), Duration::ZERO);
    }
}
