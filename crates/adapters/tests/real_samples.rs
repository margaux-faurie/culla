//! NEF previews, EXIF and the scan on the author's real samples, which are not in the
//! repository (they hold GPS). The EXIF test never reads or prints GPS tags.
//!
//! Ignored by default. Run with the folder of samples (searched recursively for `*.NEF`):
//! `CULLA_REAL_SAMPLES=/path/to/samples cargo test -p culla-adapters --test real_samples -- --ignored --nocapture`
//! Read-only: the files are opened with `File::open` and never written; the scan only lists
//! folders.

use std::fs;
use std::path::{Path, PathBuf};

use culla_adapters::metadata::exif::{read_group_meta, read_photo_meta};
use culla_adapters::metadata::nef_preview::{
    FileSource, MAX_PREVIEW_BYTES, find_preview, read_preview_bytes,
};
use culla_adapters::scan::scan_and_group;
use culla_core::exif::{MetaIssue, Orientation, TimeSource, capture_order};
use culla_core::grouping::{GroupStatus, PhotoKind, ScanPolicy};
use culla_core::ifd::{self, Limits};

/// Long edge of the G14 screen (SPEC): a preview below it would look soft.
const SCREEN_LONG_EDGE: u32 = 2880;

fn nefs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            nefs(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("nef"))
        {
            out.push(path);
        }
    }
}

fn has_jpeg_sibling(nef: &Path) -> bool {
    ["JPG", "jpg", "JPEG", "jpeg"]
        .iter()
        .any(|ext| nef.with_extension(ext).is_file())
}

#[test]
#[ignore = "needs the author's samples: set CULLA_REAL_SAMPLES"]
fn real_nefs_yield_a_screen_sized_preview() {
    let Ok(root) = std::env::var("CULLA_REAL_SAMPLES") else {
        println!("skipped: CULLA_REAL_SAMPLES is not set");
        return;
    };
    let mut files = Vec::new();
    nefs(Path::new(&root), &mut files);
    files.sort();
    assert!(!files.is_empty(), "no NEF under {root}");
    let mut lone = 0;
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy();
        let mut src = FileSource::open(path).unwrap();
        let walk = ifd::walk(&mut src, &Limits::DEFAULT).unwrap();
        let (reads, bytes) = src.disk_reads();
        println!(
            "{name}: {:?}, {reads} disk reads, {bytes} bytes",
            walk.order
        );
        for c in &walk.candidates {
            println!(
                "  {:?} {:?} {:?}x{:?} at {} len {} nst {:?} comp {:?} {:?}",
                c.source,
                c.pointer,
                c.width,
                c.height,
                c.range.offset,
                c.range.length,
                c.new_subfile_type,
                c.compression,
                c.check
            );
        }
        for i in &walk.issues {
            println!("  issue {:?} at {}", i.kind, i.offset);
        }
        assert!(walk.issues.is_empty(), "{name}: {:?}", walk.issues);
        let best = *walk.best().unwrap();
        let max = walk
            .candidates
            .iter()
            .filter(|c| c.is_valid())
            .map(|c| c.pixels())
            .max();
        assert_eq!(Some(best.pixels()), max, "{name}");
        let long_edge = best.width.unwrap().max(best.height.unwrap());
        assert!(
            long_edge >= SCREEN_LONG_EDGE,
            "{name}: preview {long_edge} px"
        );

        assert_eq!(find_preview(path).unwrap(), best);
        let jpeg = read_preview_bytes(path, best.range, MAX_PREVIEW_BYTES).unwrap();
        assert_eq!(jpeg.len() as u64, best.range.length);
        if !has_jpeg_sibling(path) {
            lone += 1;
            println!(
                "  lone NEF, preview {}x{}",
                best.width.unwrap(),
                best.height.unwrap()
            );
        }
    }
    println!("{} NEF(s), {lone} lone", files.len());
}

#[test]
#[ignore = "needs the author's samples: set CULLA_REAL_SAMPLES"]
fn real_samples_scan_into_pairs_and_lone_nefs() {
    let Ok(root) = std::env::var("CULLA_REAL_SAMPLES") else {
        println!("skipped: CULLA_REAL_SAMPLES is not set");
        return;
    };
    let policy = ScanPolicy::new(true);
    let (g, summary) = scan_and_group(Path::new(&root), &policy).unwrap();
    let mut files = Vec::new();
    nefs(Path::new(&root), &mut files);
    let (mut pairs, mut lone) = (0, 0);
    for group in &g.groups {
        assert_eq!(group.status, GroupStatus::Ok, "{}", group.stem);
        match (&group.primary.kind, &group.raw) {
            (PhotoKind::Jpeg, Some(_)) => pairs += 1,
            (PhotoKind::Nef, None) => lone += 1,
            _ => {}
        }
        println!(
            "{}\\{}: {:?}, raw {}, {} sidecar(s)",
            group.folder.join("\\"),
            group.stem,
            group.primary.kind,
            group.raw.is_some(),
            group.sidecars.len()
        );
    }
    for u in &g.unknown {
        println!("  unknown {} ({})", u.entry.rel_path(), u.kind.issue_kind());
    }
    for x in &g.excluded {
        println!("  excluded {} ({})", x.entry.rel_path(), x.reason);
    }
    println!(
        "{} group(s): {pairs} pair(s), {lone} lone NEF(s), {} folder(s)",
        g.groups.len(),
        summary.folders
    );
    // Every NEF is either a pair's raw or a lone NEF.
    assert_eq!(pairs + lone, files.len());
}

#[test]
#[ignore = "needs the author's samples: set CULLA_REAL_SAMPLES"]
fn real_samples_have_capture_time_orientation_and_lens() {
    let Ok(root) = std::env::var("CULLA_REAL_SAMPLES") else {
        println!("skipped: CULLA_REAL_SAMPLES is not set");
        return;
    };
    let root = Path::new(&root);
    let (g, _) = scan_and_group(root, &ScanPolicy::new(true)).unwrap();
    assert!(!g.groups.is_empty(), "no photo under {}", root.display());
    let mut times = Vec::new();
    for group in &g.groups {
        let name = group.primary.entry.rel_path();
        let m = read_group_meta(root, group).unwrap();
        let t = m
            .captured
            .unwrap_or_else(|| panic!("{name}: no capture time"));
        println!(
            "{name}: {:?} local_ms {} offset {:?}, orientation {}, lens {:?}, model {:?}, \
             {:?} s f/{:?} ISO {:?} {:?} mm, {:?}x{:?}, issues {:?}",
            t.source,
            t.local_ms,
            t.offset_minutes,
            m.orientation.exif_value(),
            m.lens,
            m.camera_model,
            m.shutter,
            m.aperture,
            m.iso,
            m.focal_length,
            m.width,
            m.height,
            m.issues
        );
        assert_eq!(t.source, TimeSource::DateTimeOriginal, "{name}");
        assert!(
            matches!(
                m.orientation,
                Orientation::Normal | Orientation::Rotate90 | Orientation::Rotate270
            ),
            "{name}: orientation {:?}",
            m.orientation
        );
        assert!(m.lens.is_some(), "{name}: no lens");
        assert!(m.camera_model.is_some(), "{name}: no model");
        assert!(
            m.issues
                .iter()
                .all(|i| matches!(i, MetaIssue::SkippedFields(_))),
            "{name}: {:?}",
            m.issues
        );
        // The pair's NEF carries the same time and orientation as its JPEG.
        if let Some(raw) = &group.raw {
            let mut path = root.to_path_buf();
            for c in raw.entry.components() {
                path.push(c);
            }
            let n = read_photo_meta(&path, None).unwrap();
            assert_eq!(n.captured, m.captured, "{name}: NEF time");
            assert_eq!(n.orientation, m.orientation, "{name}: NEF orientation");
            assert_eq!(n.lens, m.lens, "{name}: NEF lens");
        }
        times.push(m.captured);
    }
    let order = capture_order(&g.groups, &times);
    let sorted: Vec<_> = order.iter().map(|&i| times[i].unwrap().local_ms).collect();
    assert!(sorted.windows(2).all(|w| w[0] <= w[1]));
    for &i in &order {
        println!("  {}", g.groups[i].primary.entry.rel_path());
    }
}
