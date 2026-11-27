//! NEF previews, EXIF and the scan on the author's real samples, which are not in the
//! repository (they hold GPS). The EXIF test never reads or prints GPS tags.
//!
//! Ignored by default. Run with the folder of samples (searched recursively for `*.NEF`):
//! `CULLA_REAL_SAMPLES=/path/to/samples cargo test -p culla-adapters --test real_samples -- --ignored --nocapture`
//! Read-only: the files are opened with `File::open` and never written; the scan only lists
//! folders. The preview test can write its previews for a visual check into a folder named by
//! `CULLA_PREVIEW_OUT` (outside the repository: they show the photos), never replacing a file.

use std::fs;
use std::path::{Path, PathBuf};

use culla_adapters::metadata::exif::{read_group_meta, read_photo_meta};
use culla_adapters::metadata::nef_preview::{
    FileSource, MAX_PREVIEW_BYTES, find_preview, read_preview_bytes,
};
use culla_adapters::preview::{PreviewOptions, build_preview};
use culla_adapters::scan::scan_and_group;
use culla_core::exif::{MetaIssue, Orientation, TimeSource, capture_order};
use culla_core::grouping::{GroupStatus, PhotoKind, ScanPolicy};
use culla_core::ifd::{self, Limits};
use culla_core::preview::{Scale, Size};

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

/// Bytes of the JPEG a preview is made from: the camera JPEG, or a lone NEF's largest
/// embedded JPEG (ADR 0025).
fn preview_source(path: &Path) -> Vec<u8> {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("nef"))
    {
        let best = find_preview(path).unwrap();
        read_preview_bytes(path, best.range, MAX_PREVIEW_BYTES).unwrap()
    } else {
        fs::read(path).unwrap()
    }
}

/// The size of an encoded preview, from turbojpeg's header.
fn encoded_size(jpeg: &[u8]) -> Size {
    let h = turbojpeg::Decompressor::new()
        .unwrap()
        .read_header(jpeg)
        .unwrap();
    Size::new(
        u32::try_from(h.width).unwrap(),
        u32::try_from(h.height).unwrap(),
    )
}

#[test]
#[ignore = "needs the author's samples: set CULLA_REAL_SAMPLES"]
fn real_samples_preview_upright_at_g14_size() {
    let Ok(root) = std::env::var("CULLA_REAL_SAMPLES") else {
        println!("skipped: CULLA_REAL_SAMPLES is not set");
        return;
    };
    let root = Path::new(&root);
    let out = std::env::var_os("CULLA_PREVIEW_OUT").map(PathBuf::from);
    let (g, _) = scan_and_group(root, &ScanPolicy::new(true)).unwrap();
    assert!(!g.groups.is_empty(), "no photo under {}", root.display());
    let options = PreviewOptions::default();
    let (mut portrait, mut total_ms) = (0, 0.0);
    for group in &g.groups {
        let name = group.primary.entry.rel_path();
        let mut path = root.to_path_buf();
        for c in group.primary.entry.components() {
            path.push(c);
        }
        let meta = read_group_meta(root, group).unwrap();
        let jpeg = preview_source(&path);
        let p = build_preview(&jpeg, meta.orientation, &options).unwrap();
        let t = p.timings;
        println!(
            "{name}: {}x{} {:?} at {}/{} -> {}x{} -> {}x{}, {} KB; check {:?} decode {:?} \
             resize {:?} orient {:?} encode {:?}, total {:?}",
            p.plan.source.width,
            p.plan.source.height,
            meta.orientation,
            p.plan.scale.num(),
            p.plan.scale.denom(),
            p.plan.decoded.width,
            p.plan.decoded.height,
            p.size.width,
            p.size.height,
            p.jpeg.len() / 1024,
            t.check,
            t.decode,
            t.resize,
            t.orient,
            t.encode,
            t.total()
        );
        total_ms += t.total().as_secs_f64() * 1000.0;
        // Z6III: 6048x4032, decoded at 1/2, 2700x1800 upright (US-07, ADR 0012).
        assert_eq!(p.plan.source, Size::new(6048, 4032), "{name}");
        assert_eq!(p.plan.scale, Scale::eighths(4).unwrap(), "{name}");
        assert_eq!(p.plan.decoded, Size::new(3024, 2016), "{name}");
        let want = if meta.orientation.swaps_dimensions() {
            portrait += 1;
            Size::new(1800, 2700)
        } else {
            Size::new(2700, 1800)
        };
        assert_eq!(p.size, want, "{name}");
        assert_eq!(encoded_size(&p.jpeg), want, "{name}");
        assert!(
            p.jpeg.len() < 4 * 1024 * 1024,
            "{name}: {} bytes",
            p.jpeg.len()
        );
        if let Some(dir) = &out {
            let file = dir.join(format!("{}.preview.jpg", group.stem));
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&file)
                .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
            std::io::Write::write_all(&mut f, &p.jpeg).unwrap();
        }
    }
    println!(
        "{} preview(s), {portrait} portrait, mean {:.1} ms",
        g.groups.len(),
        total_ms / g.groups.len() as f64
    );
}
