//! NEF previews on the author's real samples, which are not in the repository (they hold GPS).
//!
//! Ignored by default. Run with the folder of samples (searched recursively for `*.NEF`):
//! `CULLA_REAL_SAMPLES=/path/to/samples cargo test -p culla-adapters --test real_samples -- --ignored --nocapture`
//! Read-only: the files are opened with `File::open` and never written.

use std::fs;
use std::path::{Path, PathBuf};

use culla_adapters::metadata::nef_preview::{
    FileSource, MAX_PREVIEW_BYTES, find_preview, read_preview_bytes,
};
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
