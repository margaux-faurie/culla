//! Fuzz target: TIFF IFD walker (docs/testing.md §7, target 1).
//!
//! Placeholder until the walker exists. TODO(M1): depend on `culla-adapters` and call the IFD
//! walker on `data` (IFD0, SubIFDs, MakerNote PreviewIFD, bad offsets and counts, IFD loops).
//! Seed corpus: `testdata/synthetic/`.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Touch the input so the harness, corpus and CI wiring can be exercised end to end.
    let _ = std::hint::black_box(data.iter().fold(0u8, |acc, b| acc ^ b));
});
