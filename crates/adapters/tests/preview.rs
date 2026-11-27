//! Preview pipeline (US-07): JPEGs built here with turbojpeg, and the synthetic fixtures.
//! Everything stays in memory; the fixtures are only read.

use std::fs;
use std::path::{Path, PathBuf};

use culla_adapters::metadata::exif::read_photo_meta;
use culla_adapters::metadata::nef_preview::{MAX_PREVIEW_BYTES, find_preview, read_preview_bytes};
use culla_adapters::preview::{PreviewError, PreviewOptions, QUALITY, build_preview};
use culla_core::exif::Orientation;
use culla_core::ifd::Reject;
use culla_core::preview::{Limits, PlanError, PreviewBox, Scale, Size};

const RED: [u8; 3] = [230, 30, 30];
const GREEN: [u8; 3] = [30, 200, 30];
const BLUE: [u8; 3] = [30, 30, 230];
const WHITE: [u8; 3] = [240, 240, 240];

fn synthetic(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/synthetic")
        .join(rel)
}

/// A `w` x `h` RGB JPEG in four flat quadrants: red, green on top; blue, white below.
fn quadrants(w: usize, h: usize) -> Vec<u8> {
    let mut px = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            let c = match (x < w / 2, y < h / 2) {
                (true, true) => RED,
                (false, true) => GREEN,
                (true, false) => BLUE,
                (false, false) => WHITE,
            };
            px.extend_from_slice(&c);
        }
    }
    let mut c = turbojpeg::Compressor::new().unwrap();
    c.set_quality(95).unwrap();
    c.set_subsamp(turbojpeg::Subsamp::Sub2x2).unwrap();
    c.compress_to_vec(turbojpeg::Image {
        pixels: px.as_slice(),
        width: w,
        pitch: w * 3,
        height: h,
        format: turbojpeg::PixelFormat::RGB,
    })
    .unwrap()
}

/// Decodes a JPEG in full to RGB.
fn decode(jpeg: &[u8]) -> (Vec<u8>, Size) {
    let mut d = turbojpeg::Decompressor::new().unwrap();
    let h = d.read_header(jpeg).unwrap();
    let mut px = vec![0u8; h.width * h.height * 3];
    d.decompress(
        jpeg,
        turbojpeg::Image {
            pixels: px.as_mut_slice(),
            width: h.width,
            pitch: h.width * 3,
            height: h.height,
            format: turbojpeg::PixelFormat::RGB,
        },
    )
    .unwrap();
    (
        px,
        Size::new(
            u32::try_from(h.width).unwrap(),
            u32::try_from(h.height).unwrap(),
        ),
    )
}

/// The colour at the centre of each quadrant: top-left, top-right, bottom-left, bottom-right.
fn quadrant_colours(px: &[u8], s: Size) -> [[u8; 3]; 4] {
    let (w, h) = (s.width as usize, s.height as usize);
    let at = |x: usize, y: usize| {
        let i = (y * w + x) * 3;
        [px[i], px[i + 1], px[i + 2]]
    };
    [
        at(w / 4, h / 4),
        at(3 * w / 4, h / 4),
        at(w / 4, 3 * h / 4),
        at(3 * w / 4, 3 * h / 4),
    ]
}

fn close(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(&x, y)| x.abs_diff(y) <= 40)
}

#[test]
fn every_orientation_comes_out_upright_and_scaled() {
    // Stored R G / B W; where each quadrant ends up when shown (EXIF definition).
    let want: [(Orientation, [[u8; 3]; 4]); 8] = [
        (Orientation::Normal, [RED, GREEN, BLUE, WHITE]),
        (Orientation::MirrorHorizontal, [GREEN, RED, WHITE, BLUE]),
        (Orientation::Rotate180, [WHITE, BLUE, GREEN, RED]),
        (Orientation::MirrorVertical, [BLUE, WHITE, RED, GREEN]),
        (Orientation::Transpose, [RED, BLUE, GREEN, WHITE]),
        (Orientation::Rotate90, [BLUE, RED, WHITE, GREEN]),
        (Orientation::Transverse, [WHITE, GREEN, BLUE, RED]),
        (Orientation::Rotate270, [GREEN, WHITE, RED, BLUE]),
    ];
    let jpeg = quadrants(128, 64);
    // 128x64 into 40x20: 3/8 decodes 48x24, then a resize to 40x20.
    let options = PreviewOptions {
        target: PreviewBox::new(40, 20).unwrap(),
        ..PreviewOptions::default()
    };
    for (o, colours) in want {
        let p = build_preview(&jpeg, o, &options).unwrap();
        assert_eq!(p.plan.scale, Scale::eighths(3).unwrap(), "{o:?}");
        assert_eq!(p.plan.decoded, Size::new(48, 24), "{o:?}");
        assert_eq!(p.plan.resized, Size::new(40, 20), "{o:?}");
        let upright = Size::new(40, 20).oriented(o);
        assert_eq!(p.size, upright, "{o:?}");
        let (px, s) = decode(&p.jpeg);
        assert_eq!(s, upright, "{o:?}: encoded size");
        let got = quadrant_colours(&px, s);
        for (g, w) in got.iter().zip(colours) {
            assert!(close(*g, w), "{o:?}: got {got:?}, want {colours:?}");
        }
    }
}

#[test]
fn a_jpeg_inside_the_box_is_decoded_whole() {
    let jpeg = quadrants(64, 32);
    let p = build_preview(&jpeg, Orientation::Rotate90, &PreviewOptions::default()).unwrap();
    assert_eq!(p.plan.scale, Scale::ONE);
    assert!(!p.plan.needs_resize());
    assert_eq!(p.size, Size::new(32, 64));
    assert_eq!(decode(&p.jpeg).1, Size::new(32, 64));
    assert!(p.timings.total() >= p.timings.decode);
}

#[test]
fn quality_is_clamped_and_changes_the_output() {
    let jpeg = quadrants(256, 128);
    let at = |q| {
        build_preview(
            &jpeg,
            Orientation::Normal,
            &PreviewOptions {
                quality: q,
                ..PreviewOptions::default()
            },
        )
        .unwrap()
    };
    assert_eq!(QUALITY, 88);
    assert!(at(0).jpeg.len() > 100);
    assert!(at(255).jpeg.len() >= at(QUALITY).jpeg.len());
}

#[test]
fn caps_refuse_before_decoding() {
    let jpeg = quadrants(64, 32);
    let tight = |limits| PreviewOptions {
        limits,
        ..PreviewOptions::default()
    };
    assert_eq!(
        build_preview(
            &jpeg,
            Orientation::Normal,
            &tight(Limits {
                max_pixels: 64 * 31,
                ..Limits::DEFAULT
            })
        ),
        Err(PreviewError::Plan(PlanError::TooManyPixels {
            pixels: 64 * 32,
            cap: 64 * 31
        }))
    );
    assert_eq!(
        build_preview(
            &jpeg,
            Orientation::Normal,
            &tight(Limits {
                max_bytes: 100,
                ..Limits::DEFAULT
            })
        ),
        Err(PreviewError::Plan(PlanError::Rejected(Reject::TooLarge)))
    );

    // The real JPEG with its frame header patched to claim 30000x30000 (900 MP): refused by
    // Culla's check, so turbojpeg never allocates for it.
    let mut bomb = jpeg.clone();
    let sof = bomb
        .windows(2)
        .position(|w| w == [0xFF, 0xC0])
        .expect("baseline SOF0");
    bomb[sof + 5..sof + 7].copy_from_slice(&30_000u16.to_be_bytes());
    bomb[sof + 7..sof + 9].copy_from_slice(&30_000u16.to_be_bytes());
    assert_eq!(
        build_preview(&bomb, Orientation::Normal, &PreviewOptions::default()),
        Err(PreviewError::Plan(PlanError::TooManyPixels {
            pixels: 900_000_000,
            cap: 100_000_000
        }))
    );
    // The same JPEG marked progressive (SOF2) and claiming 45 MP: under the general cap but
    // over the multi-scan one, so refused before turbojpeg holds any coefficients.
    let mut prog = jpeg.clone();
    prog[sof + 1] = 0xC2;
    prog[sof + 5..sof + 7].copy_from_slice(&5504u16.to_be_bytes());
    prog[sof + 7..sof + 9].copy_from_slice(&8256u16.to_be_bytes());
    assert_eq!(
        build_preview(&prog, Orientation::Normal, &PreviewOptions::default()),
        Err(PreviewError::Plan(PlanError::MultiScanTooManyPixels {
            pixels: 8256 * 5504,
            cap: 25_000_000
        }))
    );
    // The baseline JPEG made non-interleaved: its interleaved scan header (all three
    // components) replaced by one for the first component only, and 45 MP claimed. Still
    // SOF0, but libjpeg-turbo would buffer every coefficient, so it is refused the same way.
    assert_eq!(jpeg[sof + 9], 3, "a three-component frame");
    let mut split = jpeg.clone();
    split[sof + 5..sof + 7].copy_from_slice(&5504u16.to_be_bytes());
    split[sof + 7..sof + 9].copy_from_slice(&8256u16.to_be_bytes());
    let sos = split
        .windows(2)
        .position(|w| w == [0xFF, 0xDA])
        .expect("scan header");
    let sos_len = usize::from(u16::from_be_bytes([split[sos + 2], split[sos + 3]]));
    let first = split[sos + 5];
    split.splice(
        sos..sos + 2 + sos_len,
        [0xFF, 0xDA, 0x00, 0x08, 0x01, first, 0x00, 0x00, 0x3F, 0x00],
    );
    assert_eq!(
        build_preview(&split, Orientation::Normal, &PreviewOptions::default()),
        Err(PreviewError::Plan(PlanError::MultiScanTooManyPixels {
            pixels: 8256 * 5504,
            cap: 25_000_000
        }))
    );
}

#[test]
fn broken_jpegs_fail_one_preview_each() {
    let jpeg = quadrants(64, 32);
    let o = Orientation::Normal;
    let opts = PreviewOptions::default();
    // Truncated (no EOI), empty, not a JPEG.
    assert_eq!(
        build_preview(&jpeg[..jpeg.len() / 2], o, &opts),
        Err(PreviewError::Plan(PlanError::Rejected(Reject::NoEoi)))
    );
    assert!(matches!(
        build_preview(&[], o, &opts),
        Err(PreviewError::Plan(_))
    ));
    assert!(matches!(
        build_preview(b"GIF89a", o, &opts),
        Err(PreviewError::Plan(_))
    ));
    // Damaged entropy data with intact markers: whatever the decoder does, it is an error or
    // an image of the planned size, never a panic out of build_preview.
    let mut damaged = jpeg.clone();
    let sos = damaged.windows(2).position(|w| w == [0xFF, 0xDA]).unwrap();
    for b in &mut damaged[sos + 20..jpeg.len() - 2] {
        *b = 0xFF;
    }
    match build_preview(&damaged, o, &opts) {
        Ok(p) => assert_eq!(p.size, Size::new(64, 32)),
        Err(e) => assert!(
            matches!(e, PreviewError::Decode(_) | PreviewError::Plan(_)),
            "{e:?}"
        ),
    }
    // The pipeline still works after the failures.
    assert!(build_preview(&jpeg, o, &opts).is_ok());
}

#[test]
fn synthetic_jpegs_with_exif_orientation() {
    // 8x8 greyscale JPEGs; orientation comes from the EXIF reader, as in the app.
    for (rel, o) in [
        ("pair/DSC_0001.JPG", Orientation::Normal),
        ("exif/DSC_0101.JPG", Orientation::Rotate90),
        ("exif/DSC_0102.JPG", Orientation::Rotate270),
        ("exif/DSC_0103.JPG", Orientation::Normal),
    ] {
        let path = synthetic(rel);
        let meta = read_photo_meta(&path, None).unwrap();
        assert_eq!(meta.orientation, o, "{rel}");
        let bytes = fs::read(&path).unwrap();
        let p = build_preview(&bytes, meta.orientation, &PreviewOptions::default()).unwrap();
        assert_eq!(p.size, Size::new(8, 8), "{rel}");
        assert_eq!(decode(&p.jpeg).1, Size::new(8, 8), "{rel}");
    }
}

#[test]
fn synthetic_lone_nef_preview_is_its_largest_jpeg() {
    let path = synthetic("lone/DSC_0002.NEF");
    let best = find_preview(&path).unwrap();
    let bytes = read_preview_bytes(&path, best.range, MAX_PREVIEW_BYTES).unwrap();
    let p = build_preview(&bytes, Orientation::Normal, &PreviewOptions::default()).unwrap();
    assert_eq!(p.plan.source, Size::new(48, 32));
    assert_eq!(p.size, Size::new(48, 32));
    // Scaled into a small box: 48x32 into 12x8 is exactly 1/4.
    let small = PreviewOptions {
        target: PreviewBox::new(12, 8).unwrap(),
        ..PreviewOptions::default()
    };
    let p = build_preview(&bytes, Orientation::Rotate270, &small).unwrap();
    assert_eq!(p.plan.scale, Scale::eighths(2).unwrap());
    assert!(!p.plan.needs_resize());
    assert_eq!(p.size, Size::new(8, 12));
    assert_eq!(decode(&p.jpeg).1, Size::new(8, 12));
}
