//! Fuzz target: TIFF IFD walker (docs/testing.md §7, target 1).
//!
//! Runs the core walker on arbitrary bytes: IFD0, SubIFDs, MakerNote PreviewIFD, bad offsets
//! and counts, IFD loops. Pass bar: no panic, no timeout, bounded memory; every valid
//! candidate lies inside the input and within the pixel cap. Seed corpus: `testdata/synthetic/` (the NEFs).
#![no_main]

use culla_core::ifd::{self, Limits};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let limits = Limits::DEFAULT;
    let Ok(walk) = ifd::walk(&mut &data[..], &limits) else {
        return;
    };
    assert!(walk.candidates.len() <= limits.max_candidates);
    assert!(walk.issues.len() <= limits.max_issues);
    for c in walk.candidates.iter().filter(|c| c.is_valid()) {
        let end = c.range.end().expect("valid range does not overflow");
        assert!(end <= data.len() as u64);
        assert!(c.range.length <= limits.max_preview_bytes);
        assert!(c.pixels() > 0);
        assert!(c.pixels() <= limits.max_pixels);
    }
    if let Some(best) = walk.best() {
        let start = best.range.offset as usize;
        assert_eq!(&data[start..start + 2], &[0xFF, 0xD8]);
    }
});
