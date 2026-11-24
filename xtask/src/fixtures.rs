//! Synthetic test files (docs/testing.md §9), written by our own minimal writers, never by the
//! parsers under test, so a parser bug cannot hide in the fixtures.
//!
//! Output is a pure function of this code: same bytes on every run and every OS.
//!
//! Format limits of the M0 skeleton (extended in M1 with the parsers):
//! - JPEG: one 8x8 greyscale baseline image (a single DC value, so a flat grey square), with
//!   an APP1 Exif block holding IFD0 with Orientation = 1 only. No Exif SubIFD, so no
//!   DateTimeOriginal or LensModel yet; no JFIF APP0, no colour, no thumbnail.
//! - NEF: big-endian TIFF ("MM") with one IFD0: NewSubfileType, Compression = 6 (old JPEG),
//!   Make, Model, JPEGInterchangeFormat and its length, pointing at the same 8x8 JPEG (without
//!   Exif). No SubIFDs, no MakerNote or PreviewIFD, no raw CFA data, no thumbnail strip. It is
//!   NEF-shaped enough for an IFD walker and an embedded-JPEG finder, not for a RAW decoder.
//! - The pair shares a basename in one folder (ADR 0007), so it doubles as a pairing case.
//!
//! Writing: the target files must not exist. xtask follows the workspace rule (no truncate, no
//! overwrite, no removal), so it cannot refresh files in place. To regenerate, delete
//! `testdata/synthetic/` by hand and run `fixtures` again. CI does not regenerate: it runs
//! `fixtures --check`, which builds the files in memory and compares them byte for byte with the
//! committed ones, so drift fails without writing anything.

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Folder under the repository root, as in docs/testing.md §9.
pub const DIR: &str = "testdata/synthetic";

/// Every fixture as (path relative to `DIR`, bytes), in a fixed order.
pub fn all() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("pair/DSC_0001.JPG", jpeg_with_exif()),
        ("pair/DSC_0001.NEF", nef_with_preview()),
    ]
}

pub fn write(root: &Path) -> Result<(), String> {
    let dir = root.join(DIR);
    let files = all();
    // Refuse before writing anything, so a run never leaves a half-refreshed folder.
    let existing: Vec<PathBuf> = files
        .iter()
        .map(|(rel, _)| dir.join(rel))
        .filter(|p| p.exists())
        .collect();
    if !existing.is_empty() {
        return Err(format!(
            "{} file(s) already exist, first {}. xtask never overwrites or removes files: \
             delete {DIR}/ by hand to regenerate, or run `fixtures --check` to compare",
            existing.len(),
            existing[0].display()
        ));
    }
    for (rel, bytes) in &files {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        println!("wrote {DIR}/{rel} ({} bytes)", bytes.len());
    }
    Ok(())
}

pub fn check(root: &Path) -> Result<(), String> {
    let dir = root.join(DIR);
    let files = all();
    let mut problems = Vec::new();
    for (rel, expected) in &files {
        match fs::read(dir.join(rel)) {
            Ok(actual) if actual == *expected => {}
            Ok(_) => problems.push(format!("{DIR}/{rel} differs from the generator output")),
            Err(e) => problems.push(format!("{DIR}/{rel}: {e}")),
        }
    }
    let known: BTreeSet<String> = files.iter().map(|(rel, _)| (*rel).to_owned()).collect();
    for rel in list_files(&dir, &dir) {
        if !known.contains(&rel) {
            problems.push(format!(
                "{DIR}/{rel} is not produced by the generator (stale or hand-made)"
            ));
        }
    }
    if problems.is_empty() {
        println!("{DIR}: {} fixture(s) match the generator", files.len());
        Ok(())
    } else {
        Err(format!(
            "fixtures out of date:\n  {}\nDelete {DIR}/ by hand and run `fixtures` to regenerate",
            problems.join("\n  ")
        ))
    }
}

/// Relative paths with `/` separators of every file under `dir`; empty if `dir` is missing.
fn list_files(base: &Path, dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(list_files(base, &path));
        } else if let Ok(rel) = path.strip_prefix(base) {
            let parts: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            out.push(parts.join("/"));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// JPEG
// ---------------------------------------------------------------------------------------------

fn segment(out: &mut Vec<u8>, marker: u8, payload: &[u8]) {
    let len = u16::try_from(payload.len() + 2).expect("JPEG segment under 64 KiB");
    out.extend_from_slice(&[0xFF, marker]);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(payload);
}

/// An 8x8 greyscale baseline JPEG. `app1` is inserted right after SOI when given.
fn jpeg(app1: Option<&[u8]>) -> Vec<u8> {
    let mut out = vec![0xFF, 0xD8]; // SOI
    if let Some(app1) = app1 {
        segment(&mut out, 0xE1, app1);
    }
    // DQT: table 0, 8-bit, all ones.
    let mut dqt = vec![0x00];
    dqt.extend_from_slice(&[1; 64]);
    segment(&mut out, 0xDB, &dqt);
    // SOF0: 8-bit, 8x8, one component (id 1, 1x1 sampling, table 0).
    segment(
        &mut out,
        0xC0,
        &[0x08, 0x00, 0x08, 0x00, 0x08, 0x01, 0x01, 0x11, 0x00],
    );
    // DHT: one code of length 1 ("0") per table. DC: symbol 0 (difference 0). AC: symbol 0 (EOB).
    for class_and_id in [0x00, 0x10] {
        let mut dht = vec![class_and_id, 1];
        dht.extend_from_slice(&[0; 15]);
        dht.push(0x00);
        segment(&mut out, 0xC4, &dht);
    }
    // SOS: one component, tables 0/0, spectral 0..63, no approximation.
    segment(&mut out, 0xDA, &[0x01, 0x01, 0x00, 0x00, 0x3F, 0x00]);
    // Entropy data: DC "0" + EOB "0", padded with ones.
    out.push(0b0011_1111);
    out.extend_from_slice(&[0xFF, 0xD9]); // EOI
    out
}

fn jpeg_with_exif() -> Vec<u8> {
    // Little-endian TIFF inside Exif, IFD0 at offset 8 with Orientation = 1 (top-left).
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend_from_slice(b"II*\0");
    app1.extend_from_slice(&8u32.to_le_bytes());
    app1.extend_from_slice(&1u16.to_le_bytes()); // entry count
    app1.extend_from_slice(&0x0112u16.to_le_bytes()); // Orientation
    app1.extend_from_slice(&3u16.to_le_bytes()); // SHORT
    app1.extend_from_slice(&1u32.to_le_bytes()); // count
    app1.extend_from_slice(&[1, 0, 0, 0]); // value, left-justified
    app1.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
    jpeg(Some(&app1))
}

// ---------------------------------------------------------------------------------------------
// NEF-shaped TIFF
// ---------------------------------------------------------------------------------------------

const ASCII: u16 = 2;
const SHORT: u16 = 3;
const LONG: u16 = 4;

fn nef_with_preview() -> Vec<u8> {
    let preview = jpeg(None);
    let make = b"NIKON CORPORATION\0";
    let model = b"CULLA SYNTHETIC\0";

    const ENTRIES: u32 = 6;
    let ifd_offset: u32 = 8;
    let data_start = ifd_offset + 2 + 12 * ENTRIES + 4;
    let make_offset = data_start;
    let model_offset = make_offset + len32(make);
    // TIFF offsets should be word aligned.
    let jpeg_offset = (model_offset + len32(model) + 1) & !1;

    let mut out = b"MM\0*".to_vec();
    out.extend_from_slice(&ifd_offset.to_be_bytes());
    out.extend_from_slice(&u16::try_from(ENTRIES).expect("few entries").to_be_bytes());
    // Entries in ascending tag order.
    let short = |v: u16| {
        let mut b = v.to_be_bytes().to_vec();
        b.extend_from_slice(&[0, 0]);
        b
    };
    let entries: [(u16, u16, u32, Vec<u8>); 6] = [
        (0x00FE, LONG, 1, 1u32.to_be_bytes().to_vec()), // NewSubfileType: reduced resolution
        (0x0103, SHORT, 1, short(6)),                   // Compression: old-style JPEG
        // Make, Model
        (
            0x010F,
            ASCII,
            len32(make),
            make_offset.to_be_bytes().to_vec(),
        ),
        (
            0x0110,
            ASCII,
            len32(model),
            model_offset.to_be_bytes().to_vec(),
        ),
        (0x0201, LONG, 1, jpeg_offset.to_be_bytes().to_vec()), // JPEGInterchangeFormat
        (0x0202, LONG, 1, len32(&preview).to_be_bytes().to_vec()), // ...Length
    ];
    for (tag, kind, count, value) in &entries {
        out.extend_from_slice(&tag.to_be_bytes());
        out.extend_from_slice(&kind.to_be_bytes());
        out.extend_from_slice(&count.to_be_bytes());
        out.extend_from_slice(value);
    }
    out.extend_from_slice(&0u32.to_be_bytes()); // no next IFD
    debug_assert_eq!(len32(&out), data_start);
    out.extend_from_slice(make);
    out.extend_from_slice(model);
    out.resize(jpeg_offset as usize, 0);
    out.extend_from_slice(&preview);
    out
}

fn len32(bytes: &[u8]) -> u32 {
    u32::try_from(bytes.len()).expect("fixture under 4 GiB")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn be16(b: &[u8], at: usize) -> u16 {
        u16::from_be_bytes([b[at], b[at + 1]])
    }
    fn be32(b: &[u8], at: usize) -> u32 {
        u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    }

    #[test]
    fn committed_fixtures_match_generator() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        check(&root).unwrap();
    }

    #[test]
    fn output_is_deterministic() {
        assert_eq!(all(), all());
    }

    #[test]
    fn jpeg_has_markers_and_exif_orientation() {
        let j = jpeg_with_exif();
        assert_eq!(&j[..4], &[0xFF, 0xD8, 0xFF, 0xE1]);
        assert_eq!(&j[j.len() - 2..], &[0xFF, 0xD9]);
        let app1_len = usize::from(be16(&j, 4));
        let app1 = &j[6..4 + app1_len];
        assert_eq!(&app1[..6], b"Exif\0\0");
        let tiff = &app1[6..];
        assert_eq!(&tiff[..4], b"II*\0");
        assert_eq!(u16::from_le_bytes([tiff[10], tiff[11]]), 0x0112);
        assert_eq!(u16::from_le_bytes([tiff[18], tiff[19]]), 1);
        // The next segment starts right after APP1.
        assert_eq!(j[4 + app1_len], 0xFF);
    }

    #[test]
    fn nef_ifd0_points_at_embedded_jpeg() {
        let n = nef_with_preview();
        assert_eq!(&n[..4], b"MM\0*");
        let ifd = be32(&n, 4) as usize;
        let count = usize::from(be16(&n, ifd));
        let mut tags = Vec::new();
        let (mut offset, mut length) = (0, 0);
        for i in 0..count {
            let e = ifd + 2 + 12 * i;
            let tag = be16(&n, e);
            tags.push(tag);
            match tag {
                0x0201 => offset = be32(&n, e + 8) as usize,
                0x0202 => length = be32(&n, e + 8) as usize,
                0x010F => {
                    let at = be32(&n, e + 8) as usize;
                    assert_eq!(&n[at..at + 5], b"NIKON");
                }
                _ => {}
            }
        }
        assert!(tags.windows(2).all(|w| w[0] < w[1]), "tags ascending");
        assert_eq!(be32(&n, ifd + 2 + 12 * count), 0, "single IFD");
        assert_eq!(offset % 2, 0, "word aligned");
        assert_eq!(offset + length, n.len());
        assert_eq!(&n[offset..offset + 2], &[0xFF, 0xD8]);
        assert_eq!(&n[n.len() - 2..], &[0xFF, 0xD9]);
    }
}
