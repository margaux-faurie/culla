//! Fuzz target: EXIF fields (docs/testing.md §7, target 3).
//!
//! Runs the adapter's EXIF path on arbitrary bytes, as the first bytes of a file:
//! container recognition and Culla's JPEG APP1 extraction, `kamadak-exif`'s `read_raw` on
//! the TIFF data, then the core interpretation (`culla_core::exif`), and the core value
//! parsers directly. cargo-fuzz builds with `panic=abort`, so the adapter's `catch_unwind`
//! does not hide a parser panic from the fuzzer.
//! Pass bar: no panic, no timeout, bounded memory; text values stay within the cap.
//! Seed corpus: `testdata/synthetic/` (the JPEGs and NEFs).
#![no_main]

use culla_adapters::metadata::exif::meta_from_head;
use culla_core::exif::{self, MAX_TEXT_CHARS};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let meta = meta_from_head(data.to_vec(), Some(0));
    assert!(meta.captured.is_some());
    for text in [&meta.lens, &meta.camera_model].into_iter().flatten() {
        assert!(!text.is_empty() && text.chars().count() <= MAX_TEXT_CHARS);
    }
    let _ = exif::parse_date_time(data);
    let _ = exif::parse_sub_sec(data);
    let _ = exif::parse_offset(data);
    let _ = exif::clean_text(data);
});
