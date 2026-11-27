//! `culla-bench io-baseline` (B-16): the medium's sequential read speed, buffered reads, over
//! the photo files already on it (primaries and NEFs, in scan order) until `--mb` MB are read
//! or the files run out. Nothing is written, so no test file is created on the card: the
//! speed is measured on the files Culla reads, as they lie on the medium.

use std::fs::File;
use std::io::Read;
use std::time::{Duration, Instant};

use crate::args::IoArgs;
use crate::folder::{self, Failure};
use crate::json::Value;
use crate::run::{Metric, Run, Samples, Unit};
use crate::scan::peak_metric;
use crate::stats::{mb_per_s, ms};

/// The read buffer (1 MiB, a proposal).
const BUFFER: usize = 1024 * 1024;

/// Runs the benchmark.
///
/// # Errors
///
/// When the folder is refused or cannot be read.
pub(crate) fn run(a: &IoArgs) -> Result<Run, Failure> {
    let (root, medium) = folder::resolve(&a.common.folder)?;
    let scanned = folder::scan_folder(&root, a.common.include_subfolders, |_, _| {})?;
    let files = folder::photo_files(&root, &scanned.grouping.groups);
    let limit = a.mb.saturating_mul(1_000_000);

    let mut buf = vec![0u8; BUFFER];
    let mut total = 0u64;
    let mut read_files = 0usize;
    let mut per_file = Vec::new();
    let mut notes = Vec::new();
    let mut time = Duration::ZERO;
    for path in &files {
        if total >= limit {
            break;
        }
        let t = Instant::now();
        let mut f =
            File::open(path).map_err(|e| Failure::Io(format!("cannot open a photo: {e}")))?;
        let mut n = 0u64;
        loop {
            let got = f
                .read(&mut buf)
                .map_err(|e| Failure::Io(format!("cannot read a photo: {e}")))?;
            if got == 0 {
                break;
            }
            n += got as u64;
            if total + n >= limit {
                break;
            }
        }
        let d = t.elapsed();
        time += d;
        total += n;
        read_files += 1;
        if let Some(r) = mb_per_s(n, d) {
            per_file.push(r);
        }
        folder::progress(
            "read MB",
            usize::try_from(total / 1_000_000).unwrap_or(0),
            usize::try_from(a.mb).unwrap_or(usize::MAX),
        );
    }
    folder::progress_done("read MB", usize::try_from(total / 1_000_000).unwrap_or(0));
    if total < limit {
        notes.push(format!(
            "the folder holds {} MB of photos, less than the {} MB asked",
            total / 1_000_000,
            a.mb
        ));
    }
    #[allow(clippy::cast_precision_loss, reason = "a byte count")]
    let total_f = total as f64;
    let mut metrics = vec![
        Metric::new("read_mb_per_s", "B-16", Unit::MbPerS, mb_per_s(total, time)),
        Metric::new("read_bytes", "B-16", Unit::Bytes, Some(total_f)),
        Metric::new("read_ms", "B-16", Unit::Ms, Some(ms(time))),
        Metric::count("files_read", read_files as u64),
    ];
    metrics.push(peak_metric(&mut notes));
    Ok(Run {
        benchmark: "io-baseline",
        label: a.common.label.clone(),
        folder: a.common.folder.clone(),
        medium,
        params: vec![
            (
                "include_subfolders",
                Value::Bool(a.common.include_subfolders),
            ),
            ("mb", Value::int(a.mb)),
            ("buffer_bytes", Value::int(BUFFER as u64)),
        ],
        metrics,
        samples: vec![Samples {
            key: "file_mb_per_s",
            id: "B-16",
            unit: Unit::MbPerS,
            values: per_file,
        }],
        notes,
    })
}
