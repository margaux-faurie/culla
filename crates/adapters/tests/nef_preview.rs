//! NEF preview extraction on the synthetic fixtures (testdata/synthetic, `cargo xtask fixtures`).
//! Read-only: the tests open the committed fixtures and never write.

use std::path::PathBuf;

use culla_adapters::metadata::nef_preview::{
    FileSource, MAX_PREVIEW_BYTES, PreviewError, find_preview, read_preview_bytes,
};
use culla_core::ifd::{self, ByteRange, CandidateSource, JpegCheck, Limits, Reject, WalkError};

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/synthetic")
        .join(rel)
}

#[test]
fn pair_and_lone_nef_yield_the_full_size_subifd_jpeg() {
    for rel in ["pair/DSC_0001.NEF", "lone/DSC_0002.NEF"] {
        let path = fixture(rel);
        let best = find_preview(&path).unwrap();
        assert_eq!(best.source, CandidateSource::SubIfd { index: 0 }, "{rel}");
        assert_eq!((best.width, best.height), (Some(48), Some(32)), "{rel}");
        let bytes = read_preview_bytes(&path, best.range, MAX_PREVIEW_BYTES).unwrap();
        assert_eq!(bytes.len() as u64, best.range.length);
        assert_eq!(&bytes[..2], &[0xFF, 0xD8]);
        assert_eq!(&bytes[bytes.len() - 2..], &[0xFF, 0xD9]);
    }
}

#[test]
fn a_maker_note_stored_last_still_yields_its_preview() {
    let path = fixture("exif/DSC_0105.NEF");
    let best = find_preview(&path).unwrap();
    assert_eq!(best.source, CandidateSource::SubIfd { index: 0 });
    let mut src = FileSource::open(&path).unwrap();
    let walk = ifd::walk(&mut src, &Limits::DEFAULT).unwrap();
    assert!(walk.issues.is_empty(), "{:?}", walk.issues);
    assert!(
        walk.candidates
            .iter()
            .any(|c| c.source == CandidateSource::MakerNotePreview && c.check == JpegCheck::Valid)
    );
}

#[test]
fn every_candidate_is_found_with_both_byte_orders() {
    for rel in ["pair/DSC_0001.NEF", "lone/DSC_0002.NEF"] {
        let mut src = FileSource::open(&fixture(rel)).unwrap();
        let walk = ifd::walk(&mut src, &Limits::DEFAULT).unwrap();
        assert!(walk.issues.is_empty(), "{rel}: {:?}", walk.issues);
        let found: Vec<_> = walk
            .candidates
            .iter()
            .map(|c| (c.source, c.width, c.height, c.check))
            .collect();
        assert_eq!(
            found,
            vec![
                (
                    CandidateSource::Ifd { index: 0 },
                    Some(8),
                    Some(8),
                    JpegCheck::Valid
                ),
                (
                    CandidateSource::SubIfd { index: 0 },
                    Some(48),
                    Some(32),
                    JpegCheck::Valid
                ),
                (
                    CandidateSource::SubIfd { index: 1 },
                    Some(24),
                    Some(16),
                    JpegCheck::Valid
                ),
                (
                    CandidateSource::MakerNotePreview,
                    Some(32),
                    Some(24),
                    JpegCheck::Valid
                ),
            ],
            "{rel}"
        );
        // The whole fixture is under the head buffer: one read from disk.
        assert_eq!(src.disk_reads().0, 1);
    }
}

#[test]
fn ifd_loop_fixture_ends_and_yields_the_thumbnail() {
    let best = find_preview(&fixture("broken/ifd-loop.NEF")).unwrap();
    assert_eq!(best.source, CandidateSource::Ifd { index: 0 });
    let mut src = FileSource::open(&fixture("broken/ifd-loop.NEF")).unwrap();
    let walk = ifd::walk(&mut src, &Limits::DEFAULT).unwrap();
    assert!(
        walk.issues
            .iter()
            .any(|i| i.kind == ifd::IssueKind::IfdLoop)
    );
}

#[test]
fn truncated_fixture_falls_back_to_the_maker_note_preview() {
    let path = fixture("broken/truncated.NEF");
    let mut src = FileSource::open(&path).unwrap();
    let walk = ifd::walk(&mut src, &Limits::DEFAULT).unwrap();
    let full = walk
        .candidates
        .iter()
        .find(|c| c.source == CandidateSource::SubIfd { index: 0 })
        .unwrap();
    assert_eq!(full.check, JpegCheck::Invalid(Reject::OutOfBounds));
    let best = walk.best().unwrap();
    assert_eq!(best.source, CandidateSource::MakerNotePreview);
    assert_eq!((best.width, best.height), (Some(32), Some(24)));
    read_preview_bytes(&path, best.range, MAX_PREVIEW_BYTES).unwrap();
}

#[test]
fn a_jpeg_is_not_a_nef() {
    let e = find_preview(&fixture("pair/DSC_0001.JPG")).unwrap_err();
    assert!(matches!(e, PreviewError::Walk(WalkError::NotTiff)), "{e}");
}

#[test]
fn a_missing_file_is_an_io_error() {
    let e = find_preview(&fixture("pair/missing.NEF")).unwrap_err();
    assert!(matches!(e, PreviewError::Io(_)), "{e}");
}

#[test]
fn read_preview_bytes_checks_cap_bounds_and_markers() {
    let path = fixture("pair/DSC_0001.NEF");
    let best = find_preview(&path).unwrap();
    let e = read_preview_bytes(&path, best.range, best.range.length - 1).unwrap_err();
    assert!(matches!(e, PreviewError::TooLarge { .. }), "{e}");
    let past = ByteRange {
        offset: u64::MAX - 1,
        length: 10,
    };
    let e = read_preview_bytes(&path, past, MAX_PREVIEW_BYTES).unwrap_err();
    assert!(matches!(e, PreviewError::Changed), "{e}");
    // A range that is not a JPEG (the file changed since the walk).
    let shifted = ByteRange {
        offset: best.range.offset + 1,
        length: best.range.length,
    };
    let e = read_preview_bytes(&path, shifted, MAX_PREVIEW_BYTES).unwrap_err();
    assert!(matches!(e, PreviewError::Changed), "{e}");
}

#[test]
fn file_source_reads_match_the_file() {
    let path = fixture("pair/DSC_0001.NEF");
    let all = std::fs::read(&path).unwrap();
    let mut src = FileSource::open(&path).unwrap();
    use culla_core::ifd::ReadAt;
    assert_eq!(src.len(), all.len() as u64);
    let mut buf = [0u8; 16];
    src.read_exact_at(1000, &mut buf).unwrap();
    assert_eq!(&buf[..], &all[1000..1016]);
    let end = all.len() as u64;
    assert!(src.read_exact_at(end - 8, &mut buf).is_err());
    assert!(src.read_exact_at(u64::MAX, &mut buf).is_err());
}
