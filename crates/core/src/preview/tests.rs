//! Preview sizing and orientation tests.

use super::*;

const ALL: [Orientation; 8] = [
    Orientation::Normal,
    Orientation::MirrorHorizontal,
    Orientation::Rotate180,
    Orientation::MirrorVertical,
    Orientation::Transpose,
    Orientation::Rotate90,
    Orientation::Transverse,
    Orientation::Rotate270,
];

const Z6: Size = Size::new(6048, 4032);
const Z8: Size = Size::new(8256, 5504);

fn g14(source: Size, o: Orientation) -> Plan {
    plan(source, o, PreviewBox::G14, &Limits::DEFAULT).unwrap()
}

// ---------------------------------------------------------------------------------------------
// Sizes, box, scale
// ---------------------------------------------------------------------------------------------

#[test]
fn size_helpers() {
    let s = Size::new(30, 20);
    assert_eq!(s.pixels(), 600);
    assert_eq!(s.transposed(), Size::new(20, 30));
    assert_eq!((s.long_edge(), s.short_edge()), (30, 20));
    assert_eq!(Size::new(20, 30).long_edge(), 30);
    assert_eq!(s.rgb_bytes(), Some(1800));
    assert_eq!(
        Size::new(u32::MAX, u32::MAX).pixels(),
        u64::from(u32::MAX).pow(2)
    );
    for o in ALL {
        let want = if o.swaps_dimensions() {
            Size::new(20, 30)
        } else {
            s
        };
        assert_eq!(s.oriented(o), want, "{o:?}");
    }
}

#[test]
fn preview_box_is_orientation_free() {
    assert_eq!(PreviewBox::new(1800, 2880), Some(PreviewBox::G14));
    assert_eq!(PreviewBox::new(2880, 1800), Some(PreviewBox::G14));
    assert_eq!(PreviewBox::default(), PreviewBox::G14);
    assert_eq!(PreviewBox::new(0, 10), None);
    assert_eq!(PreviewBox::new(10, 0), None);
    let b = PreviewBox::new(7, 7).unwrap();
    assert_eq!((b.long_edge(), b.short_edge()), (7, 7));
}

#[test]
fn scales_are_reduced_eighths() {
    let got: Vec<(u8, u8)> = (1..=8)
        .map(|m| {
            let s = Scale::eighths(m).unwrap();
            (s.num(), s.denom())
        })
        .collect();
    assert_eq!(
        got,
        [
            (1, 8),
            (1, 4),
            (3, 8),
            (1, 2),
            (5, 8),
            (3, 4),
            (7, 8),
            (1, 1)
        ]
    );
    assert_eq!(Scale::eighths(0), None);
    assert_eq!(Scale::eighths(9), None);
    assert_eq!(Scale::eighths(8), Some(Scale::ONE));
}

#[test]
fn scale_rounds_up_like_libjpeg_turbo() {
    let half = Scale::eighths(4).unwrap();
    assert_eq!(half.apply(6048), 3024);
    assert_eq!(half.apply(6049), 3025);
    assert_eq!(half.apply(1), 1);
    assert_eq!(half.apply(0), 0);
    let three = Scale::eighths(3).unwrap();
    assert_eq!(three.apply(8256), 3096);
    assert_eq!(three.apply(5504), 2064);
    assert_eq!(three.apply(7), 3); // 21 / 8 = 2.625
    assert_eq!(Scale::eighths(1).unwrap().apply(9), 2);
    assert_eq!(Scale::ONE.apply(u32::MAX), u32::MAX);
    assert_eq!(Scale::eighths(7).unwrap().apply(u32::MAX), 3_758_096_384);
}

// ---------------------------------------------------------------------------------------------
// Fit
// ---------------------------------------------------------------------------------------------

#[test]
fn fit_fills_the_g14_in_the_photo_orientation() {
    let b = PreviewBox::G14;
    // 3:2 is limited by the short edge: it fills the screen height.
    assert_eq!(fit(Z6, b), Size::new(2700, 1800));
    assert_eq!(fit(Z6.transposed(), b), Size::new(1800, 2700));
    assert_eq!(fit(Z8, b), Size::new(2700, 1800));
    // 16:9 is limited by the long edge.
    assert_eq!(fit(Size::new(3840, 2160), b), Size::new(2880, 1620));
    assert_eq!(fit(Size::new(2160, 3840), b), Size::new(1620, 2880));
    // 1:1 and 5:4 crop modes.
    assert_eq!(fit(Size::new(4032, 4032), b), Size::new(1800, 1800));
    assert_eq!(fit(Size::new(5040, 4032), b), Size::new(2250, 1800));
    // Exactly the box's aspect ratio.
    assert_eq!(fit(Size::new(5760, 3600), b), Size::new(2880, 1800));
}

#[test]
fn fit_never_upscales() {
    let b = PreviewBox::G14;
    for s in [
        Size::new(1620, 1080),
        Size::new(640, 424),
        Size::new(2880, 1800),
        Size::new(1800, 2880),
        Size::new(1, 1),
    ] {
        assert_eq!(fit(s, b), s);
    }
    // One side over is enough to shrink.
    assert_eq!(fit(Size::new(2881, 100), b), Size::new(2880, 100));
    assert_eq!(fit(Size::new(2000, 1801), b), Size::new(1999, 1800));
}

#[test]
fn fit_keeps_extreme_shapes_at_least_one_pixel() {
    let b = PreviewBox::G14;
    assert_eq!(fit(Size::new(65_535, 1_525), b), Size::new(2880, 67));
    assert_eq!(fit(Size::new(65_535, 1), b), Size::new(2880, 1));
    assert_eq!(fit(Size::new(1, 65_535), b), Size::new(1, 2880));
    assert_eq!(fit(Size::new(0, 10), b), Size::new(0, 10));
}

// ---------------------------------------------------------------------------------------------
// Plans
// ---------------------------------------------------------------------------------------------

#[test]
fn z6_decodes_at_one_half_in_every_orientation() {
    for o in ALL {
        let p = g14(Z6, o);
        assert_eq!(p.source, Z6);
        assert_eq!(p.orientation, o);
        assert_eq!(p.scale, Scale::eighths(4).unwrap(), "{o:?}");
        assert_eq!(p.decoded, Size::new(3024, 2016), "{o:?}");
        assert_eq!(p.resized, Size::new(2700, 1800), "{o:?}");
        let upright = if o.swaps_dimensions() {
            Size::new(1800, 2700)
        } else {
            Size::new(2700, 1800)
        };
        assert_eq!(p.output, upright, "{o:?}");
        assert!(p.needs_resize());
    }
}

#[test]
fn z8_decodes_at_three_eighths_in_every_orientation() {
    for o in ALL {
        let p = g14(Z8, o);
        assert_eq!(p.scale, Scale::eighths(3).unwrap(), "{o:?}");
        assert_eq!(p.decoded, Size::new(3096, 2064), "{o:?}");
        assert_eq!(p.resized, Size::new(2700, 1800), "{o:?}");
        assert_eq!(p.output.long_edge(), 2700);
        assert_eq!(p.output.width < p.output.height, o.swaps_dimensions());
    }
}

#[test]
fn portrait_stored_as_portrait() {
    // A JPEG already stored upright as a portrait, orientation 1.
    let p = g14(Z6.transposed(), Orientation::Normal);
    assert_eq!(p.scale, Scale::eighths(4).unwrap());
    assert_eq!(p.decoded, Size::new(2016, 3024));
    assert_eq!(p.resized, Size::new(1800, 2700));
    assert_eq!(p.output, Size::new(1800, 2700));
    // Stored portrait with orientation 6 shows as landscape.
    let p = g14(Z6.transposed(), Orientation::Rotate90);
    assert_eq!(p.resized, Size::new(1800, 2700));
    assert_eq!(p.output, Size::new(2700, 1800));
}

#[test]
fn small_previews_are_decoded_whole_and_not_resized() {
    for s in [Size::new(1620, 1080), Size::new(640, 424), Size::new(8, 8)] {
        for o in ALL {
            let p = g14(s, o);
            assert_eq!(p.scale, Scale::ONE);
            assert_eq!(p.decoded, s);
            assert_eq!(p.resized, s);
            assert_eq!(p.output, s.oriented(o));
            assert!(!p.needs_resize());
        }
    }
}

#[test]
fn a_scaled_decode_can_land_on_the_exact_size() {
    // 5760x3840 at 1/2 is 2880x1920; fit is 2700x1800, so a resize is still needed. 5760x3600
    // at 1/2 is exactly the box.
    let p = g14(Size::new(5760, 3600), Orientation::Normal);
    assert_eq!(p.scale, Scale::eighths(4).unwrap());
    assert_eq!(p.decoded, Size::new(2880, 1800));
    assert!(!p.needs_resize());
}

/// Every plan over a grid of sizes, boxes and orientations: the output fits the box in the
/// photo's orientation and keeps its aspect ratio, the decode covers the resize, and the next
/// smaller scale would not.
#[test]
fn plans_hold_their_invariants() {
    let sides = [
        1, 2, 7, 8, 9, 15, 16, 17, 100, 999, 1080, 1800, 1801, 2880, 2881, 4032, 5504, 6048, 8256,
        11_648, 65_535,
    ];
    let boxes = [
        PreviewBox::G14,
        PreviewBox::new(1920, 1080).unwrap(),
        PreviewBox::new(100, 100).unwrap(),
        PreviewBox::new(1, 1).unwrap(),
    ];
    let limits = Limits {
        max_bytes: u64::MAX,
        max_pixels: u64::MAX,
    };
    let mut n = 0;
    for &w in &sides {
        for &h in &sides {
            for b in boxes {
                for o in ALL {
                    let s = Size::new(w, h);
                    let p = plan(s, o, b, &limits).unwrap();
                    let up = s.oriented(o);
                    assert!(p.output.long_edge() <= b.long_edge());
                    assert!(p.output.short_edge() <= b.short_edge());
                    assert!(p.output.width <= up.width && p.output.height <= up.height);
                    assert!(p.output.width >= 1 && p.output.height >= 1);
                    if p.output.width != p.output.height {
                        assert_eq!(p.output.width > p.output.height, up.width > up.height);
                    }
                    assert_eq!(p.resized, p.output.oriented(o));
                    assert_eq!(p.decoded, p.scale.apply_size(s));
                    assert!(p.decoded.width >= p.resized.width);
                    assert!(p.decoded.height >= p.resized.height);
                    let m = (1..=8u8)
                        .find(|&m| Scale::eighths(m) == Some(p.scale))
                        .unwrap();
                    if m > 1 {
                        let smaller = Scale::eighths(m - 1).unwrap().apply_size(s);
                        assert!(
                            smaller.width < p.resized.width || smaller.height < p.resized.height,
                            "{s:?} {o:?}: {m}/8 is not the smallest"
                        );
                    }
                    // Aspect ratio within one pixel of rounding on the short side.
                    let (ol, os) = (
                        u64::from(p.output.long_edge()),
                        u64::from(p.output.short_edge()),
                    );
                    let (sl, ss) = (u64::from(up.long_edge()), u64::from(up.short_edge()));
                    let exact_short_x2 = 2 * ss * ol;
                    assert!(
                        (2 * os * sl).abs_diff(exact_short_x2) <= 2 * sl || os == 1,
                        "{s:?} in {b:?}: {:?}",
                        p.output
                    );
                    n += 1;
                }
            }
        }
    }
    assert_eq!(n, sides.len() * sides.len() * boxes.len() * 8);
}

#[test]
fn decode_buffer_stays_bounded_for_any_source_under_the_cap() {
    // The worst case is just over 2x the box's sides at M = 2 or 1/1 near the box; the decode
    // buffer stays far below the source size for big sources.
    for s in [Z6, Z8, Size::new(11_648, 8_584), Size::new(10_000, 10_000)] {
        let p = g14(s, Orientation::Normal);
        let bytes = p.decoded.rgb_bytes().unwrap();
        assert!(
            bytes <= 4 * PreviewBox::G14.long_edge() as usize * 1800 * CHANNELS,
            "{s:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Pixel cap (threat model T5)
// ---------------------------------------------------------------------------------------------

#[test]
fn pixel_cap_is_checked_before_anything_else() {
    let l = Limits::DEFAULT;
    assert_eq!(l.max_pixels, 100_000_000);
    assert_eq!(l.max_bytes, 64 * 1024 * 1024);
    assert!(
        plan(
            Size::new(10_000, 10_000),
            Orientation::Normal,
            PreviewBox::G14,
            &l
        )
        .is_ok()
    );
    assert_eq!(
        plan(
            Size::new(10_001, 10_000),
            Orientation::Normal,
            PreviewBox::G14,
            &l
        ),
        Err(PlanError::TooManyPixels {
            pixels: 100_010_000,
            cap: 100_000_000
        })
    );
    assert_eq!(
        plan(
            Size::new(65_535, 65_535),
            Orientation::Rotate90,
            PreviewBox::G14,
            &l
        ),
        Err(PlanError::TooManyPixels {
            pixels: 65_535 * 65_535,
            cap: 100_000_000
        })
    );
    assert_eq!(
        plan(Size::new(0, 10), Orientation::Normal, PreviewBox::G14, &l),
        Err(PlanError::ZeroSize)
    );
    let tight = Limits {
        max_bytes: 1024,
        max_pixels: 64,
    };
    assert!(check_size(Size::new(8, 8), &tight).is_ok());
    assert!(check_size(Size::new(9, 8), &tight).is_err());
}

/// SOI, an APP1, a frame header, a stand-in scan and EOI (as in the IFD walker tests).
fn jpeg_with(marker: u8, precision: u8, w: u16, h: u16) -> Vec<u8> {
    let mut j = vec![0xFF, 0xD8];
    j.extend_from_slice(&[0xFF, 0xE1, 0x00, 0x06, b'E', b'x', b'i', b'f']);
    j.extend_from_slice(&[0xFF, marker, 0x00, 0x0B, precision]);
    j.extend_from_slice(&h.to_be_bytes());
    j.extend_from_slice(&w.to_be_bytes());
    j.extend_from_slice(&[0x01, 0x01, 0x11, 0x00]);
    j.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x02, 0x12, 0x34, 0xFF, 0xD9]);
    j
}

#[test]
fn frame_check_reads_culla_s_own_parser() {
    let l = Limits::DEFAULT;
    let f = check_frame(&jpeg_with(0xC0, 8, 6048, 4032), &l).unwrap();
    assert_eq!((f.width, f.height), (6048, 4032));
    // Progressive and extended sequential are previews too.
    assert!(check_frame(&jpeg_with(0xC2, 8, 64, 48), &l).is_ok());
    assert!(check_frame(&jpeg_with(0xC1, 8, 64, 48), &l).is_ok());
    // Zero padding after EOI is accepted.
    let mut padded = jpeg_with(0xC0, 8, 64, 48);
    padded.extend_from_slice(&[0; 16]);
    assert!(check_frame(&padded, &l).is_ok());

    let p = plan_for_jpeg(
        &jpeg_with(0xC0, 8, 8256, 5504),
        Orientation::Rotate270,
        PreviewBox::G14,
        &l,
    )
    .unwrap();
    assert_eq!(p.scale, Scale::eighths(3).unwrap());
    assert_eq!(p.output, Size::new(1800, 2700));
}

#[test]
fn frame_check_refuses_bombs_and_broken_bytes() {
    let l = Limits::DEFAULT;
    // A few hundred bytes claiming 65535 x 65535 (4.3 GP): refused before any decode.
    assert_eq!(
        check_frame(&jpeg_with(0xC0, 8, 65_535, 65_535), &l),
        Err(PlanError::TooManyPixels {
            pixels: 65_535 * 65_535,
            cap: 100_000_000
        })
    );
    assert_eq!(
        plan_for_jpeg(
            &jpeg_with(0xC2, 8, 20_000, 20_000),
            Orientation::Normal,
            PreviewBox::G14,
            &l
        ),
        Err(PlanError::TooManyPixels {
            pixels: 400_000_000,
            cap: 100_000_000
        })
    );
    assert_eq!(
        check_frame(&jpeg_with(0xC0, 8, 0, 10), &l),
        Err(PlanError::ZeroSize)
    );
    // Lossless (raw data stored as JPEG) and 12-bit frames.
    assert_eq!(
        check_frame(&jpeg_with(0xC3, 8, 64, 48), &l),
        Err(PlanError::Rejected(Reject::UnsupportedFrame {
            marker: 0xC3,
            precision: 8
        }))
    );
    assert!(matches!(
        check_frame(&jpeg_with(0xC1, 12, 64, 48), &l),
        Err(PlanError::Rejected(Reject::UnsupportedFrame { .. }))
    ));
    // Truncated: no EOI.
    let good = jpeg_with(0xC0, 8, 64, 48);
    assert_eq!(
        check_frame(&good[..good.len() - 1], &l),
        Err(PlanError::Rejected(Reject::NoEoi))
    );
    assert_eq!(
        check_frame(b"not a jpeg at all", &l),
        Err(PlanError::Rejected(Reject::NoSoi))
    );
    assert_eq!(
        check_frame(&[], &l),
        Err(PlanError::Rejected(Reject::TooShort))
    );
    // Over the byte cap.
    let small = Limits {
        max_bytes: 16,
        ..Limits::DEFAULT
    };
    assert_eq!(
        check_frame(&good, &small),
        Err(PlanError::Rejected(Reject::TooLarge))
    );
    // A tighter pixel cap.
    let tight = Limits {
        max_pixels: 64 * 47,
        ..Limits::DEFAULT
    };
    assert_eq!(
        check_frame(&good, &tight),
        Err(PlanError::TooManyPixels {
            pixels: 64 * 48,
            cap: 64 * 47
        })
    );
    // Every truncation of a valid header fails cleanly.
    for n in 0..good.len() {
        assert!(check_frame(&good[..n], &l).is_err(), "prefix {n}");
    }
}

#[test]
fn plan_errors_display() {
    let e = PlanError::TooManyPixels { pixels: 5, cap: 4 };
    assert_eq!(e.to_string(), "the JPEG declares 5 pixels, over the 4 cap");
    assert!(PlanError::ZeroSize.to_string().contains("zero"));
    assert!(
        PlanError::Rejected(Reject::NoEoi)
            .to_string()
            .contains("NoEoi")
    );
}

// ---------------------------------------------------------------------------------------------
// Orientation
// ---------------------------------------------------------------------------------------------

/// A 3x2 single-channel image:
/// ```text
/// 1 2 3
/// 4 5 6
/// ```
fn img() -> (Vec<u8>, Size) {
    (vec![1, 2, 3, 4, 5, 6], Size::new(3, 2))
}

#[test]
fn orient_moves_pixels_for_every_exif_value() {
    let cases: [(Orientation, Size, [u8; 6]); 8] = [
        (Orientation::Normal, Size::new(3, 2), [1, 2, 3, 4, 5, 6]),
        // 2: mirror left to right.
        (
            Orientation::MirrorHorizontal,
            Size::new(3, 2),
            [3, 2, 1, 6, 5, 4],
        ),
        (Orientation::Rotate180, Size::new(3, 2), [6, 5, 4, 3, 2, 1]),
        (
            Orientation::MirrorVertical,
            Size::new(3, 2),
            [4, 5, 6, 1, 2, 3],
        ),
        // 5: transpose.
        (Orientation::Transpose, Size::new(2, 3), [1, 4, 2, 5, 3, 6]),
        // 6: 90° clockwise.
        (Orientation::Rotate90, Size::new(2, 3), [4, 1, 5, 2, 6, 3]),
        // 7: transverse.
        (Orientation::Transverse, Size::new(2, 3), [6, 3, 5, 2, 4, 1]),
        // 8: 90° counter-clockwise.
        (Orientation::Rotate270, Size::new(2, 3), [3, 6, 2, 5, 1, 4]),
    ];
    for (o, size, want) in cases {
        let (p, s) = img();
        let (out, out_size) = orient(p, s, 1, o).unwrap();
        assert_eq!(out_size, size, "{o:?}");
        assert_eq!(out, want, "{o:?}");
    }
}

/// The EXIF definition: for each value, which side of the shown image the stored 0th row and
/// 0th column end up on.
#[test]
fn orient_follows_the_exif_row_and_column_definition() {
    #[derive(Debug, PartialEq)]
    enum Side {
        Top,
        Bottom,
        Left,
        Right,
    }
    use Side::*;
    let def = [
        (Orientation::Normal, Top, Left),
        (Orientation::MirrorHorizontal, Top, Right),
        (Orientation::Rotate180, Bottom, Right),
        (Orientation::MirrorVertical, Bottom, Left),
        (Orientation::Transpose, Left, Top),
        (Orientation::Rotate90, Right, Top),
        (Orientation::Transverse, Right, Bottom),
        (Orientation::Rotate270, Left, Bottom),
    ];
    // 5x4: row 0 holds 1, column 0 holds 2, the corner holds 3, the rest 0.
    let (w, h) = (5usize, 4usize);
    let mut p = vec![0u8; w * h];
    p[..w].fill(1);
    for y in 0..h {
        p[y * w] = 2;
    }
    p[0] = 3;
    for (o, row_side, col_side) in def {
        let (out, s) = orient(p.clone(), Size::new(5, 4), 1, o).unwrap();
        let (ow, oh) = (s.width as usize, s.height as usize);
        let at = |x: usize, y: usize| out[y * ow + x];
        let side_of = |v: u8| {
            if (1..ow - 1).all(|x| at(x, 0) == v) {
                Top
            } else if (1..ow - 1).all(|x| at(x, oh - 1) == v) {
                Bottom
            } else if (1..oh - 1).all(|y| at(0, y) == v) {
                Left
            } else if (1..oh - 1).all(|y| at(ow - 1, y) == v) {
                Right
            } else {
                panic!("{o:?}: value {v} not on a side")
            }
        };
        assert_eq!(side_of(1), row_side, "{o:?} 0th row");
        assert_eq!(side_of(2), col_side, "{o:?} 0th column");
    }
}

#[test]
fn orient_keeps_multi_byte_pixels_whole() {
    // 2x1 RGB: red, blue. Rotated 90° clockwise it is 1x2: red on top.
    let px = vec![255, 0, 0, 0, 0, 255];
    let (out, s) = orient(px.clone(), Size::new(2, 1), 3, Orientation::Rotate90).unwrap();
    assert_eq!(s, Size::new(1, 2));
    assert_eq!(out, px);
    let (out, _) = orient(px, Size::new(2, 1), 3, Orientation::Rotate270).unwrap();
    assert_eq!(out, [0, 0, 255, 255, 0, 0]);
}

#[test]
fn orient_inverses_round_trip() {
    let (w, h) = (7u32, 4u32);
    let px: Vec<u8> = (0..w * h * 3).map(|i| (i * 7 % 251) as u8).collect();
    let size = Size::new(w, h);
    let inverse = |o: Orientation| match o {
        Orientation::Rotate90 => Orientation::Rotate270,
        Orientation::Rotate270 => Orientation::Rotate90,
        other => other, // the others are their own inverse
    };
    for o in ALL {
        let (a, s) = orient(px.clone(), size, 3, o).unwrap();
        let (b, s2) = orient(a, s, 3, inverse(o)).unwrap();
        assert_eq!((b.as_slice(), s2), (px.as_slice(), size), "{o:?}");
    }
}

#[test]
fn orient_refuses_a_wrong_buffer() {
    assert!(orient(vec![0; 5], Size::new(3, 2), 1, Orientation::Rotate90).is_none());
    assert!(orient(vec![0; 6], Size::new(3, 2), 0, Orientation::Normal).is_none());
    assert!(orient(vec![0; 18], Size::new(3, 2), 3, Orientation::Normal).is_some());
    assert!(orient(vec![], Size::new(0, 2), 3, Orientation::Rotate90).is_some());
    assert!(
        orient(
            vec![0; 6],
            Size::new(u32::MAX, u32::MAX),
            3,
            Orientation::Normal
        )
        .is_none()
    );
}
