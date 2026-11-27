//! `culla preview`: wiring only. Scans the folder like `culla scan` ([`crate::scan::run`]),
//! then runs every group through the preview pipeline (`culla_adapters::pipeline`: reader
//! thread, decode workers, prefetch window, in-memory cache) in capture-time order, as a user
//! swiping forward as fast as the previews come, and reports the time of each step.
//!
//! It reads the source folder only. With `--out <dir>` it writes each preview into `<dir>`, a
//! folder that must be new (its parent existing) or empty and outside the scanned folder,
//! each file created with `create_new` (never replaced). Without `--out` it writes nothing
//! but stdout and stderr.

use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use culla_adapters::pipeline::{
    Config, FolderSource, Pipeline, PipelineError, Prepared, Stats, Status, Turbo,
};
use culla_core::exif::Orientation;
use culla_core::names;
use culla_core::prefetch::Direction;

use crate::args::{PreviewArgs, ScanArgs};
use crate::scan::{self, Failure, GroupKind, Progress, group_kind};
use crate::text::shown;

/// Why `culla preview` stopped.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PreviewFailure {
    /// The scan did not run (exit 2 or 3).
    Scan(Failure),
    /// The output folder is refused or cannot be written (exit 4).
    Out(String),
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
    pub result: Result<std::sync::Arc<Prepared>, PipelineError>,
    /// The file written, if any.
    pub written: Option<String>,
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
    pub out: Option<PathBuf>,
}

/// Runs the scan, then the previews.
///
/// # Errors
///
/// [`PreviewFailure::Scan`] as `culla scan`; [`PreviewFailure::Out`] when `--out` is refused
/// or a preview cannot be written.
pub(crate) fn run(args: &PreviewArgs, progress: &mut Progress) -> Result<Report, PreviewFailure> {
    // Refuse a bad --out before the scan; create it only after.
    if let Some(out) = &args.out {
        check_out(out, &args.folder).map_err(PreviewFailure::Out)?;
    }
    let scan_args = ScanArgs {
        folder: args.folder.clone(),
        include_subfolders: args.include_subfolders,
        json: false,
    };
    let scanned = scan::run(&scan_args, progress).map_err(PreviewFailure::Scan)?;
    let out = match &args.out {
        Some(out) => {
            // Again against the path actually scanned (canonical on Windows).
            check_out(out, &scanned.scanned).map_err(PreviewFailure::Out)?;
            Some(create_out(out).map_err(PreviewFailure::Out)?)
        }
        None => None,
    };

    let groups = &scanned.grouping.groups;
    let orientations: Vec<Orientation> = scanned
        .meta
        .iter()
        .map(|m| m.as_ref().map_or(Orientation::Normal, |m| m.orientation))
        .collect();
    let source = FolderSource::from_groups(&scanned.scanned, groups, &orientations);
    let config = Config::default();
    let pipeline = Pipeline::start(source, Turbo, scanned.order.clone(), config).map_err(|e| {
        PreviewFailure::Scan(Failure::Io(format!("cannot start the pipeline: {e}")))
    })?;

    let started = Instant::now();
    let total = scanned.order.len();
    let mut rows = Vec::with_capacity(total);
    for (pos, &i) in scanned.order.iter().enumerate() {
        let group = &groups[i];
        let asked = Instant::now();
        pipeline.show(pos, Direction::Forward);
        let result = loop {
            match pipeline.wait(i, Duration::from_secs(60)) {
                Status::Ready(p) => break Ok(p),
                Status::Failed(e) => break Err(e),
                Status::Pending => {}
                // Not expected (the current photo leads the window); ask again.
                Status::NotWanted => pipeline.show(pos, Direction::Forward),
            }
        };
        let wait = asked.elapsed();
        let primary = group.primary.entry.rel_path();
        let written = match (&out, &result) {
            (Some(dir), Ok(p)) => {
                let name = file_name(pos + 1, &group.primary.entry.name);
                write_new(&dir.join(&name), &p.preview.jpeg).map_err(|e| {
                    PreviewFailure::Out(format!("cannot write {}: {e}", shown(&name)))
                })?;
                Some(name)
            }
            _ => None,
        };
        rows.push(Row {
            position: pos + 1,
            primary,
            source: match group_kind(group) {
                GroupKind::LoneNef => "nef",
                GroupKind::Pair | GroupKind::JpegOnly => "jpeg",
            },
            wait,
            result,
            written,
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
        out,
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

/// `NNNN-<primary name>.jpg`: the position keeps names unique and in order.
#[must_use]
pub(crate) fn file_name(position: usize, primary: &str) -> String {
    let (stem, _) = names::split_ext(primary);
    format!("{position:04}-{stem}.jpg")
}

/// Creates `path` and writes `bytes`; an existing file is an error, never replaced.
fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

/// Folded components of `path` made absolute and lexically clean, resolving the deepest
/// part that exists (so links in it are followed, as the OS would).
fn resolved(path: &Path) -> io::Result<Vec<String>> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    // Canonicalize the longest existing prefix, keep the rest as written.
    let mut existing = abs.clone();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    let base = loop {
        match fs::canonicalize(&existing) {
            Ok(c) => break c,
            Err(_) => match (existing.file_name(), existing.parent()) {
                (Some(name), Some(parent)) => {
                    tail.push(name.to_os_string());
                    existing = parent.to_path_buf();
                }
                _ => break existing,
            },
        }
    };
    let mut parts: Vec<String> = Vec::new();
    for c in base
        .components()
        .map(|c| c.as_os_str().to_os_string())
        .chain(tail.into_iter().rev())
    {
        match Path::new(&c).components().next() {
            Some(Component::CurDir) | None => {}
            Some(Component::ParentDir) => {
                parts.pop();
            }
            _ => parts.push(names::fold(&c.to_string_lossy())),
        }
    }
    Ok(parts)
}

/// Refuses an output folder inside the scanned folder (or the same folder), or one that
/// exists and is not an empty folder (a file, a link, a folder with entries).
fn check_out(out: &Path, folder: &Path) -> Result<(), String> {
    let o = resolved(out).map_err(|e| format!("cannot resolve the output folder: {e}"))?;
    let f = resolved(folder).map_err(|e| format!("cannot resolve the folder: {e}"))?;
    if o.starts_with(&f) {
        return Err("the output folder is inside the scanned folder".into());
    }
    match fs::symlink_metadata(out) {
        Ok(m) if m.file_type().is_symlink() => Err("the output folder is a link".into()),
        Ok(m) if !m.is_dir() => Err("the output folder is not a folder".into()),
        Ok(_) => match fs::read_dir(out) {
            Ok(mut entries) => {
                if entries.next().is_some() {
                    Err("the output folder is not empty".into())
                } else {
                    Ok(())
                }
            }
            Err(e) => Err(format!("cannot list the output folder: {e}")),
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("cannot read the output folder: {e}")),
    }
}

/// Creates the output folder if it does not exist (its parent must), else checks again that
/// it is empty. Returns it.
fn create_out(out: &Path) -> Result<PathBuf, String> {
    match fs::create_dir(out) {
        Ok(()) => Ok(out.to_path_buf()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            let m = fs::symlink_metadata(out).map_err(|e| e.to_string())?;
            if !m.is_dir() || m.file_type().is_symlink() {
                return Err("the output folder is not a folder".into());
            }
            let mut entries =
                fs::read_dir(out).map_err(|e| format!("cannot list the output folder: {e}"))?;
            if entries.next().is_some() {
                return Err("the output folder is not empty".into());
            }
            Ok(out.to_path_buf())
        }
        Err(e) => Err(format!("cannot create the output folder: {e}")),
    }
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
            Ok(p) => {
                let t = p.preview.timings;
                let size = format!("{}x{}", p.preview.size.width, p.preview.size.height);
                let _ = writeln!(
                    o,
                    "{:>5}  {:<4}  {:>7}  {:>6}  {:>7}  {:>7}  {:>7}  {:>7}  {:>7}  {:>9}  {:>8}  {file}",
                    row.position,
                    row.source,
                    ms(p.read),
                    ms(t.check),
                    ms(t.decode),
                    ms(t.resize),
                    ms(t.orient),
                    ms(t.encode),
                    ms(row.wait),
                    size,
                    p.preview.jpeg.len(),
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
    match &r.out {
        Some(dir) => {
            let n = r.rows.iter().filter(|x| x.written.is_some()).count();
            let _ = writeln!(
                o,
                "written:  {} to {}",
                crate::text::count(n as u64, "file"),
                shown(&dir.to_string_lossy())
            );
        }
        None => {
            let _ = writeln!(o, "written:  nothing (no --out)");
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_keep_order_and_drop_the_extension() {
        assert_eq!(file_name(1, "DSC_0001.JPG"), "0001-DSC_0001.jpg");
        assert_eq!(file_name(12_345, "a.b.NEF"), "12345-a.b.jpg");
        assert_eq!(file_name(7, "noext"), "0007-noext.jpg");
    }

    #[test]
    fn quantiles_by_nearest_rank() {
        let v: Vec<Duration> = (1..=20).map(Duration::from_millis).collect();
        assert_eq!(quantile(&v, 0.5), Duration::from_millis(10));
        assert_eq!(quantile(&v, 0.95), Duration::from_millis(19));
        assert_eq!(quantile(&v, 1.0), Duration::from_millis(20));
        assert_eq!(quantile(&[], 0.5), Duration::ZERO);
    }

    #[test]
    fn out_inside_the_folder_is_refused_lexically() {
        let cwd = std::env::current_dir().unwrap();
        assert!(check_out(&cwd.join("sub/new"), &cwd).is_err());
        assert!(check_out(&cwd.join("sub/../x"), &cwd.join("x")).is_err());
        assert!(check_out(&cwd, &cwd).is_err());
    }
}
