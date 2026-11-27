//! EXIF on the synthetic fixtures (testdata/synthetic, `cargo xtask fixtures`), through
//! kamadak-exif. Read-only: the tests open the committed fixtures and never write.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use culla_adapters::metadata::exif::{
    EXIF_HEAD_BYTES, local_file_time, meta_from_head, read_group_meta, read_head, read_photo_meta,
    read_photo_meta_capped,
};
use culla_adapters::scan::scan_and_group;
use culla_core::exif::{
    CaptureTime, FileTime, MetaIssue, Orientation, Rational, TimeSource, TimeTag, capture_order,
    parse_date_time,
};
use culla_core::grouping::ScanPolicy;

fn synthetic() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/synthetic")
}

fn fixture(rel: &str) -> PathBuf {
    synthetic().join(rel)
}

fn local_ms(s: &str) -> i64 {
    parse_date_time(s.as_bytes()).unwrap().unwrap() * 1000
}

const MTIME_NS: i64 = 1_700_000_000_000_000_000;

/// A uniquely named folder under the system temp folder, removed with everything in it on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(test: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir =
            std::env::temp_dir().join(format!("culla-exif-{test}-{}-{nanos}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }

    /// Creates a new file (never an existing one) with `bytes`.
    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        f.write_all(bytes).unwrap();
        path
    }
}

impl Drop for TempDir {
    #[allow(clippy::disallowed_methods, reason = "test temp folder only")]
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn other_containers_never_reach_the_parser() {
    // A PNG (even one with an eXIf chunk), a HEIF and a WebP named like camera files: only
    // TIFF and JPEG magic bytes are parsed, the rest has no EXIF for Culla.
    let tmp = TempDir::new("containers");
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend_from_slice(&[0, 0, 0, 8]);
    png.extend_from_slice(b"eXIfMM\0*\0\0\0\x08");
    png.extend_from_slice(&[0; 4]);
    let mut heif = b"\0\0\0\x18ftypheic\0\0\0\0mif1heic".to_vec();
    heif.extend_from_slice(&[0; 32]);
    let webp = b"RIFF\x1a\0\0\0WEBPEXIF\x0e\0\0\0MM\0*\0\0\0\x08\0\0\0\0\0\0".to_vec();
    for (name, bytes) in [("a.JPG", png), ("b.NEF", heif), ("c.JPG", webp)] {
        let path = tmp.file(name, &bytes);
        let m = read_photo_meta(&path, Some(MTIME_NS)).unwrap();
        assert_eq!(m.issues, [MetaIssue::NoExif], "{name}");
        assert_eq!(
            m.captured.unwrap().source,
            TimeSource::FileModified,
            "{name}"
        );
    }
    // A JPEG whose only Exif APP1 comes after SOS: none either.
    let jpeg = fs::read(fixture("exif/DSC_0101.JPG")).unwrap();
    let mut late = fs::read(fixture("exif/DSC_0103.JPG")).unwrap();
    late.truncate(late.len() - 2); // drop EOI
    late.extend_from_slice(&jpeg[2..]);
    let path = tmp.file("late.JPG", &late);
    let m = read_photo_meta(&path, None).unwrap();
    assert_eq!(m.issues, [MetaIssue::NoExif]);
}

#[test]
fn full_jpeg_yields_every_field() {
    let m = read_photo_meta(&fixture("exif/DSC_0101.JPG"), Some(MTIME_NS)).unwrap();
    assert!(m.issues.is_empty(), "{:?}", m.issues);
    assert_eq!(
        m.captured,
        Some(CaptureTime {
            local_ms: local_ms("2025:02:15 03:59:50") + 550,
            offset_minutes: Some(660),
            source: TimeSource::DateTimeOriginal,
        })
    );
    assert_eq!(m.orientation, Orientation::Rotate90);
    assert_eq!(m.lens.as_deref(), Some("NIKKOR Z 24-200mm f/4-6.3 VR"));
    assert_eq!(m.camera_model.as_deref(), Some("NIKON Z6_3"));
    assert_eq!(m.shutter, Some(Rational::new(10, 1250)));
    assert_eq!(m.aperture, Some(Rational::new(560, 100)));
    assert_eq!(m.iso, Some(450));
    assert_eq!(m.focal_length, Some(Rational::new(480, 10)));
    assert_eq!((m.width, m.height), (Some(8), Some(8)));
}

#[test]
fn big_endian_jpeg_falls_back_to_create_date_and_lens_specification() {
    let m = read_photo_meta(&fixture("exif/DSC_0102.JPG"), Some(MTIME_NS)).unwrap();
    assert!(m.issues.is_empty(), "{:?}", m.issues);
    let t = m.captured.unwrap();
    assert_eq!(t.source, TimeSource::CreateDate);
    assert_eq!(t.local_ms, local_ms("2025:05:11 01:12:27") + 370);
    assert_eq!(t.offset_minutes, Some(-210));
    assert_eq!(m.orientation, Orientation::Rotate270);
    assert_eq!(m.lens.as_deref(), Some("24-200mm f/4-6.3"));
    assert_eq!((m.shutter, m.iso, m.width), (None, None, None));
}

#[test]
fn jpeg_without_exif_uses_the_file_time() {
    let m = read_photo_meta(&fixture("exif/DSC_0103.JPG"), Some(MTIME_NS)).unwrap();
    assert_eq!(m.issues, [MetaIssue::NoExif]);
    let t = m.captured.unwrap();
    assert_eq!(t.source, TimeSource::FileModified);
    // In the PC's local time zone (ADR 0028); UTC off Windows.
    assert_eq!(t, CaptureTime::from_file_time(local_file_time(MTIME_NS)));
    assert_eq!(t.utc_ms(), Some(MTIME_NS / 1_000_000));
    #[cfg(not(windows))]
    assert_eq!(t.local_ms, MTIME_NS / 1_000_000);
    assert_eq!(m.orientation, Orientation::Normal);
    assert_eq!(m.lens, None);
    let m = read_photo_meta(&fixture("exif/DSC_0103.JPG"), None).unwrap();
    assert_eq!(m.captured, None);
}

#[test]
fn bad_values_are_recorded_and_fall_back() {
    let m = read_photo_meta(&fixture("exif/DSC_0104.JPG"), Some(MTIME_NS)).unwrap();
    assert_eq!(m.orientation, Orientation::Normal);
    assert_eq!(m.captured.unwrap().source, TimeSource::FileModified);
    assert_eq!(
        m.issues,
        [
            MetaIssue::BadDateTime(TimeTag::Digitized),
            MetaIssue::BadOrientation(9)
        ]
    );
}

#[test]
fn the_pair_jpeg_has_orientation_only() {
    let m = read_photo_meta(&fixture("pair/DSC_0001.JPG"), Some(MTIME_NS)).unwrap();
    assert!(m.issues.is_empty(), "{:?}", m.issues);
    assert_eq!(m.orientation, Orientation::Normal);
    assert_eq!(m.captured.unwrap().source, TimeSource::FileModified);
    assert_eq!(m.lens, None);
}

#[test]
fn nef_exif_is_read_from_its_tiff_ifd0_and_exif_ifd() {
    let m = read_photo_meta(&fixture("exif/DSC_0105.NEF"), Some(MTIME_NS)).unwrap();
    assert!(m.issues.is_empty(), "{:?}", m.issues);
    let t = m.captured.unwrap();
    assert_eq!(t.source, TimeSource::DateTimeOriginal);
    assert_eq!(t.local_ms, local_ms("2025:02:15 03:59:50") + 550);
    assert_eq!(t.offset_minutes, Some(660));
    assert_eq!(m.orientation, Orientation::Rotate270);
    assert_eq!(m.lens.as_deref(), Some("NIKKOR Z 24-200mm f/4-6.3 VR"));
    assert_eq!(m.camera_model.as_deref(), Some("CULLA SYNTHETIC"));
    // A NEF's IFD0 size is its thumbnail's: not reported as the image size.
    assert_eq!((m.width, m.height), (None, None));
    // Both byte orders of the plain NEFs parse (no EXIF times there).
    for rel in ["pair/DSC_0001.NEF", "lone/DSC_0002.NEF"] {
        let m = read_photo_meta(&fixture(rel), None).unwrap();
        assert!(m.issues.is_empty(), "{rel}: {:?}", m.issues);
        assert_eq!(m.camera_model.as_deref(), Some("CULLA SYNTHETIC"), "{rel}");
        assert_eq!(m.captured, None, "{rel}");
    }
}

#[test]
fn a_maker_note_past_the_cap_is_skipped_and_the_rest_kept() {
    // The IFDs and their values sit below byte 1024; the MakerNote is stored at the end.
    let path = fixture("exif/DSC_0105.NEF");
    let m = read_photo_meta_capped(&path, None, 1024).unwrap();
    assert_eq!(m.issues, [MetaIssue::SkippedFields(1)]);
    assert_eq!(m.captured.unwrap().source, TimeSource::DateTimeOriginal);
    assert_eq!(m.orientation, Orientation::Rotate270);
    assert_eq!(m.lens.as_deref(), Some("NIKKOR Z 24-200mm f/4-6.3 VR"));
}

#[test]
fn reads_stop_at_the_cap() {
    let path = fixture("exif/DSC_0105.NEF");
    let len = fs::metadata(&path).unwrap().len();
    assert!(len < EXIF_HEAD_BYTES);
    assert_eq!(read_head(&path, EXIF_HEAD_BYTES).unwrap().len() as u64, len);
    assert_eq!(read_head(&path, 100).unwrap().len(), 100);
    assert!(read_head(&path, 0).unwrap().is_empty());
}

#[test]
fn truncated_and_broken_files_never_fail_the_read() {
    for rel in [
        "broken/truncated.NEF",
        "broken/ifd-loop.NEF",
        "exif/DSC_0101.JPG",
        "exif/DSC_0105.NEF",
    ] {
        let path = fixture(rel);
        let bytes = fs::read(&path).unwrap();
        for cut in 0..bytes.len() {
            let m = meta_from_head(bytes[..cut].to_vec(), Some(FileTime::utc(MTIME_NS)));
            assert!(m.captured.is_some(), "{rel} cut at {cut}");
        }
        // Every byte flipped in turn.
        for i in 0..bytes.len() {
            let mut b = bytes.clone();
            b[i] ^= 0xFF;
            let _ = meta_from_head(b, None);
        }
    }
}

#[test]
fn a_missing_file_is_an_io_error() {
    assert!(read_photo_meta(&fixture("exif/missing.JPG"), None).is_err());
}

#[test]
fn groups_read_their_primary_and_sort_by_capture_time() {
    let root = synthetic();
    let (g, _) = scan_and_group(&root, &ScanPolicy::new(true)).unwrap();
    let metas: Vec<_> = g
        .groups
        .iter()
        .map(|group| read_group_meta(&root, group).unwrap())
        .collect();
    let times: Vec<_> = metas.iter().map(|m| m.captured).collect();
    let order = capture_order(&g.groups, &times);
    let names: Vec<_> = order
        .iter()
        .map(|&i| g.groups[i].primary.entry.rel_path())
        .collect();
    // DSC_0101 and DSC_0105 share a time: by path. DSC_0102 is later. Every other file falls
    // back to its listing time, which is the checkout's, after 2025.
    assert_eq!(
        names[..3],
        [
            "exif\\DSC_0101.JPG",
            "exif\\DSC_0105.NEF",
            "exif\\DSC_0102.JPG"
        ]
    );
    assert_eq!(names.len(), g.groups.len());
    let lone = g.groups.iter().position(|x| x.stem == "DSC_0105").unwrap();
    assert!(g.groups[lone].is_lone_nef());
    assert_eq!(metas[lone].orientation, Orientation::Rotate270);
}

#[test]
fn group_paths_are_built_under_the_root() {
    let root = synthetic();
    let (g, _) = scan_and_group(&root, &ScanPolicy::new(true)).unwrap();
    let pair = g.groups.iter().find(|x| x.stem == "DSC_0001").unwrap();
    // The pair's primary is its JPEG: orientation 1, no lens.
    let m = read_group_meta(&root, pair).unwrap();
    assert_eq!(m.orientation, Orientation::Normal);
    assert!(read_group_meta(Path::new("/nonexistent-culla-root"), pair).is_err());
}
