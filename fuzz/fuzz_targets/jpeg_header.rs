//! Fuzz target: the decode-size guard in front of turbojpeg (docs/testing.md §7, target 4).
//!
//! Runs Culla's frame check and preview plan on arbitrary bytes, in every orientation. Pass
//! bar: no panic, no timeout; an accepted JPEG declares at most the pixel cap (the lower cap
//! for a progressive frame), fits the box and is never decoded larger than it is. Pure core
//! code: turbojpeg itself is not run here. Seed corpus: `testdata/synthetic/` (the JPEGs).
#![no_main]

use culla_core::exif::Orientation;
use culla_core::preview::{self, Limits, PreviewBox};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let limits = Limits::DEFAULT;
    let target = PreviewBox::G14;
    for v in 1..=8 {
        let o = Orientation::from_exif(v).expect("1..=8");
        let Ok(plan) = preview::plan_for_jpeg(data, o, target, &limits) else {
            return;
        };
        assert!(plan.source.pixels() <= limits.max_pixels);
        // A progressive frame (all its coefficients held in memory) is under the lower cap.
        let frame = preview::check_frame(data, &limits).expect("planned, so checked");
        if frame.progressive {
            assert!(plan.source.pixels() <= limits.max_progressive_pixels);
        }
        assert!(plan.source.pixels() > 0);
        assert!(plan.output.long_edge() <= target.long_edge());
        assert!(plan.output.short_edge() <= target.short_edge());
        assert!(plan.decoded.width >= plan.resized.width);
        assert!(plan.decoded.height >= plan.resized.height);
        // The decode never grows the image.
        assert!(plan.decoded.width <= plan.source.width);
        assert!(plan.decoded.height <= plan.source.height);
    }
});
