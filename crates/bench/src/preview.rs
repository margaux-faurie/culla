//! `culla-bench preview` (B-05, B-08 cache miss; NFR-PERF-05): every photo's preview made
//! from the medium one after another, outside the pipeline (no prefetch, no cache), in walk
//! order, each step timed. Each photo's EXIF is read just before its preview (for its
//! orientation) and timed apart, so no step reads from a cache warmed by an earlier pass.
//!
//! The disk cache does not exist yet, so `--cache hit` (B-08 hit) is not offered.

use culla_adapters::metadata::exif::read_group_meta;
use culla_adapters::preview::PreviewOptions;
use culla_core::exif::Orientation;
use std::time::{Duration, Instant};

use crate::args::PreviewArgs;
use crate::folder::{self, Failure, SourceChoice};
use crate::json::Value;
use crate::run::{Metric, Run, Samples, Unit};
use crate::scan::peak_metric;
use crate::stats::{mb_per_s, ms};

/// Runs the benchmark.
///
/// # Errors
///
/// When the folder is refused or cannot be read.
pub(crate) fn run(a: &PreviewArgs) -> Result<Run, Failure> {
    let (root, medium) = folder::resolve(&a.common.folder)?;
    let scanned = folder::scan_folder(&root, a.common.include_subfolders, |_, _| {})?;
    let groups = &scanned.grouping.groups;
    let n = a.limit.map_or(groups.len(), |l| l.min(groups.len()));
    let options = PreviewOptions::default();

    let mut s = Steps::default();
    let mut failed = 0usize;
    let mut notes = Vec::new();
    let mut from_nef = 0usize;
    let mut read_bytes = 0u64;
    let mut read_time = Duration::ZERO;
    let started = Instant::now();
    for (i, g) in groups.iter().take(n).enumerate() {
        folder::progress("preview", i + 1, n);
        let t = Instant::now();
        let orientation = read_group_meta(&root, g).map_or(Orientation::Normal, |m| m.orientation);
        let exif = t.elapsed();
        let (item, took) = folder::item_for(&root, g, orientation, a.source);
        match folder::make_preview(&item, &options) {
            Ok(p) => {
                if took == SourceChoice::Nef {
                    from_nef += 1;
                    s.walk.push(ms(p.walk));
                    s.walk_kb.push(kb(p.walk_bytes));
                }
                read_bytes += p.read_bytes + p.walk_bytes;
                read_time += p.read + p.walk;
                let t = p.timings;
                s.exif.push(ms(exif));
                s.read.push(ms(p.read));
                s.check.push(ms(t.check));
                s.decode.push(ms(t.decode));
                s.resize.push(ms(t.resize));
                s.orient.push(ms(t.orient));
                s.encode.push(ms(t.encode));
                s.total.push(ms(exif + p.total()));
            }
            Err(e) => {
                failed += 1;
                if notes.len() < 20 {
                    notes.push(format!("photo {} failed: {e}", i + 1));
                }
            }
        }
    }
    let elapsed = started.elapsed();
    folder::progress_done("preview", n);
    if failed > 20 {
        notes.push(format!("{} more failures not listed", failed - 20));
    }

    let made = n - failed;
    let mut metrics = vec![
        Metric::count("photos", n as u64),
        Metric::count("made", made as u64),
        Metric::count("failed", failed as u64),
        Metric::count("from_nef", from_nef as u64),
        Metric::new("elapsed_ms", "", Unit::Ms, Some(ms(elapsed))),
        Metric::new(
            "read_mb_per_s",
            "B-05",
            Unit::MbPerS,
            mb_per_s(read_bytes, read_time),
        ),
        Metric::new("listing_ms", "B-02", Unit::Ms, Some(ms(scanned.listing))),
    ];
    metrics.push(peak_metric(&mut notes));
    Ok(Run {
        benchmark: "preview",
        label: a.common.label.clone(),
        folder: a.common.folder.clone(),
        medium,
        params: vec![
            (
                "include_subfolders",
                Value::Bool(a.common.include_subfolders),
            ),
            ("source", Value::str(a.source.name())),
            ("limit", Value::opt(a.limit, |l| Value::int(l as u64))),
        ],
        metrics,
        samples: s.into_samples(),
        notes,
    })
}

#[allow(clippy::cast_precision_loss, reason = "a display value")]
fn kb(bytes: u64) -> f64 {
    bytes as f64 / 1000.0
}

#[derive(Default)]
struct Steps {
    exif: Vec<f64>,
    walk: Vec<f64>,
    walk_kb: Vec<f64>,
    read: Vec<f64>,
    check: Vec<f64>,
    decode: Vec<f64>,
    resize: Vec<f64>,
    orient: Vec<f64>,
    encode: Vec<f64>,
    total: Vec<f64>,
}

impl Steps {
    fn into_samples(self) -> Vec<Samples> {
        let s = |key, id, values| Samples {
            key,
            id,
            unit: Unit::Ms,
            values,
        };
        vec![
            s("miss_total_ms", "B-08", self.total),
            s("exif_ms", "B-03", self.exif),
            s("nef_walk_ms", "B-05", self.walk),
            Samples {
                key: "nef_walk_read_kb",
                id: "B-05",
                unit: Unit::Kb,
                values: self.walk_kb,
            },
            s("read_ms", "B-05", self.read),
            s("check_ms", "", self.check),
            s("decode_ms", "B-06", self.decode),
            s("resize_ms", "B-07", self.resize),
            s("orient_ms", "B-07", self.orient),
            s("encode_ms", "B-07", self.encode),
        ]
    }
}
