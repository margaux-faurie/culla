//! `culla-bench scan` (B-01, B-02, B-03; NFR-PERF-01 and NFR-PERF-08): the walk and grouping,
//! the first group's preview made inside the walk as the app would show it, and with `--exif`
//! the EXIF pass and the capture-time sort.
//!
//! The first-photo figure of NFR-PERF-01 (`first_photo_ms`) runs from the folder's
//! resolution (`winfs::resolve_folder`: canonical path and volume lookups, as the app does
//! when a folder is opened) to the first preview made; `resolve_ms` reports that first step
//! apart, and `first_group_ms` (B-01) still starts at the walk.

use std::time::{Duration, Instant};

use culla_adapters::preview::PreviewOptions;
use culla_core::exif::Orientation;
use culla_core::grouping::PhotoKind;

use crate::args::ScanArgs;
use crate::folder::{self, Failure, OnePreview, SourceChoice};
use crate::json::Value;
use crate::memory;
use crate::run::{Metric, Run, Unit};
use crate::stats::ms;

/// Runs the benchmark.
///
/// # Errors
///
/// When the folder is refused or cannot be read.
pub(crate) fn run(a: &ScanArgs) -> Result<Run, Failure> {
    let t = Instant::now();
    let (root, medium) = folder::resolve(&a.common.folder)?;
    let resolve = t.elapsed();
    let options = PreviewOptions::default();
    let mut first: Option<Result<OnePreview, String>> = None;
    let mut notes = Vec::new();
    let scanned = folder::scan_folder(&root, a.common.include_subfolders, |root, g| {
        if a.first_preview {
            // Upright: the EXIF pass has not run yet when the app shows the first photo.
            let (item, _) = folder::item_for(root, g, Orientation::Normal, SourceChoice::Jpeg);
            first = Some(folder::make_preview(&item, &options));
        }
    })?;
    let first_photo = match (&scanned.first_group, &first) {
        (Some(g), Some(Ok(p))) => Some(resolve + *g + p.total()),
        (_, Some(Err(e))) => {
            notes.push(format!("the first photo's preview failed: {e}"));
            None
        }
        _ => None,
    };
    if scanned.first_group.is_none() {
        notes.push("no photo group found".to_owned());
    }

    let groups = &scanned.grouping.groups;
    let pairs = groups.iter().filter(|g| g.raw.is_some()).count();
    let lone = groups
        .iter()
        .filter(|g| g.primary.kind == PhotoKind::Nef)
        .count();
    let mut metrics = vec![
        Metric::new("resolve_ms", "", Unit::Ms, Some(ms(resolve))),
        Metric::new(
            "first_group_ms",
            "B-01",
            Unit::Ms,
            scanned.first_group.map(ms),
        ),
        Metric::new("first_photo_ms", "", Unit::Ms, first_photo.map(ms)),
        Metric::new("listing_ms", "B-02", Unit::Ms, Some(ms(scanned.listing))),
    ];
    if a.exif {
        let e = folder::exif_pass(&root, groups);
        let errors = e.meta.iter().filter(|m| m.is_err()).count();
        let total = scanned.listing + e.pass + e.sort;
        metrics.extend([
            Metric::new("exif_ms", "B-03", Unit::Ms, Some(ms(e.pass))),
            Metric::new("sort_ms", "B-03", Unit::Ms, Some(ms(e.sort))),
            Metric::new("scan_total_ms", "B-02", Unit::Ms, Some(ms(total))),
            Metric::count("metadata_errors", errors as u64),
        ]);
    }
    metrics.extend([
        Metric::count("folders", scanned.summary.folders),
        Metric::count("entries", scanned.summary.entries),
        Metric::count("groups", groups.len() as u64),
        Metric::count("pairs", pairs as u64),
        Metric::count("lone_nefs", lone as u64),
        Metric::count("jpeg_only", (groups.len() - pairs - lone) as u64),
        Metric::count("unknown_files", scanned.grouping.unknown.len() as u64),
        Metric::count("excluded", scanned.grouping.excluded.len() as u64),
    ]);
    if let Some(Ok(p)) = &first {
        metrics.extend(first_steps(p));
    }
    metrics.push(peak_metric(&mut notes));
    Ok(Run {
        benchmark: "scan",
        label: a.common.label.clone(),
        folder: a.common.folder.clone(),
        medium,
        params: vec![
            (
                "include_subfolders",
                Value::Bool(a.common.include_subfolders),
            ),
            ("exif", Value::Bool(a.exif)),
            ("first_preview", Value::Bool(a.first_preview)),
        ],
        metrics,
        samples: Vec::new(),
        notes,
    })
}

/// The first photo's steps: walk (a lone NEF), read, then the preview's own steps.
fn first_steps(p: &OnePreview) -> [Metric; 7] {
    let t = p.timings;
    let m = |key, d: Duration| Metric::new(key, "", Unit::Ms, Some(ms(d)));
    [
        m("first_photo_walk_ms", p.walk),
        m("first_photo_read_ms", p.read),
        m("first_photo_check_ms", t.check),
        m("first_photo_decode_ms", t.decode),
        m("first_photo_resize_ms", t.resize),
        m("first_photo_orient_ms", t.orient),
        m("first_photo_encode_ms", t.encode),
    ]
}

/// B-15: the process's peak memory, or a note saying why it is missing.
pub(crate) fn peak_metric(notes: &mut Vec<String>) -> Metric {
    let v = match memory::peak_bytes() {
        Ok(b) =>
        {
            #[allow(clippy::cast_precision_loss, reason = "a byte count")]
            Some(b as f64)
        }
        Err(e) => {
            notes.push(format!("peak memory unavailable: {e}"));
            None
        }
    };
    Metric::new("peak_memory_bytes", "B-15", Unit::Bytes, v)
}
