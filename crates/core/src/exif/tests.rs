//! EXIF interpretation and capture-order tests: values as the Z6III writes them, the
//! documented fallbacks, and malformed input (never a panic).

use super::*;
use crate::grouping::{self, Group, ScanEntry, ScanPolicy};

fn b(s: &str) -> Option<Vec<u8>> {
    Some(s.as_bytes().to_vec())
}

/// Seconds since the epoch of a date and time, through the parser.
fn secs(s: &str) -> i64 {
    parse_date_time(s.as_bytes()).unwrap().unwrap()
}

/// The tags of a Z6III JPEG (values from the author's samples, docs/research/z6iii-nef-layout.md).
fn z6iii() -> RawExif {
    RawExif {
        date_time_original: b("2025:02:15 03:59:50"),
        sub_sec_time_original: b("55"),
        offset_time_original: b("+11:00"),
        create_date: b("2025:02:15 03:59:50"),
        sub_sec_time_digitized: b("55"),
        offset_time_digitized: b("+11:00"),
        orientation: Some(8),
        model: b("NIKON Z6_3"),
        lens_model: b("NIKKOR Z 24-200mm f/4-6.3 VR"),
        lens_specification: vec![
            Rational::new(240, 10),
            Rational::new(2000, 10),
            Rational::new(400, 100),
            Rational::new(630, 100),
        ],
        exposure_time: Some(Rational::new(10, 1250)),
        f_number: Some(Rational::new(560, 100)),
        iso: Some(450),
        focal_length: Some(Rational::new(480, 10)),
        pixel_x: Some(6048),
        pixel_y: Some(4032),
    }
}

// ---------------------------------------------------------------------------------------------
// Date and time
// ---------------------------------------------------------------------------------------------

#[test]
fn date_time_epoch_and_known_values() {
    assert_eq!(secs("1970:01:01 00:00:00"), 0);
    assert_eq!(secs("1970:01:02 00:00:01"), 86_401);
    // 2025-02-15T03:59:50Z is 1739591990.
    assert_eq!(secs("2025:02:15 03:59:50"), 1_739_591_990);
    assert_eq!(secs("2000:02:29 12:00:00"), 951_825_600);
    assert_eq!(secs("1969:12:31 23:59:59"), -1);
    assert_eq!(secs("0001:01:01 00:00:00"), -62_135_596_800);
    assert_eq!(secs("9999:12:31 23:59:59"), 253_402_300_799);
}

#[test]
fn date_time_accepts_padding_and_common_variants() {
    let t = secs("2025:02:15 03:59:50");
    assert_eq!(secs("2025:02:15 03:59:50\0"), t);
    assert_eq!(secs("2025:02:15 03:59:50  \0\0"), t);
    assert_eq!(secs("2025-02-15 03:59:50"), t);
    assert_eq!(secs("2025:02:15T03:59:50"), t);
}

#[test]
fn blank_and_zero_date_times_are_unknown_not_invalid() {
    for s in [
        "",
        "\0",
        "    :  :     :  :  ",
        "0000:00:00 00:00:00",
        "                   ",
    ] {
        assert_eq!(parse_date_time(s.as_bytes()), Ok(None), "{s:?}");
    }
}

#[test]
fn malformed_date_times_are_invalid() {
    for s in [
        "2025:02:15",
        "2025:02:15 03:59",
        "2025:02:15 03:59:50Z",
        "2025:02:15 03:59:50+01:00",
        "2025/02/15 03:59:50",
        "2025:02-15 03:59:50",
        "2025:02:15_03:59:50",
        "2025:13:15 03:59:50",
        "2025:00:15 03:59:50",
        "2025:02:30 03:59:50",
        "2025:02:29 03:59:50",
        "1900:02:29 03:59:50",
        "2025:04:31 03:59:50",
        "2025:02:00 03:59:50",
        "2025:02:15 24:00:00",
        "2025:02:15 23:60:00",
        "2025:02:15 23:59:60",
        "0000:01:01 00:00:00",
        "2a25:02:15 03:59:50",
        "+025:02:15 03:59:50",
        " 2025:02:15 03:59:50",
        "２０２５:02:15 03:59:50",
        "2025:02:15 03:59:5\u{e9}",
    ] {
        assert_eq!(parse_date_time(s.as_bytes()), Err(InvalidValue), "{s:?}");
    }
    assert_eq!(parse_date_time(&[0xFF; 19]), Err(InvalidValue));
}

#[test]
fn sub_seconds_are_milliseconds() {
    for (s, ms) in [
        ("5", 500),
        ("52", 520),
        ("055", 55),
        ("07", 70),
        ("123456", 123),
        ("999999999999", 999),
        ("0", 0),
        ("55\0", 550),
        (" 55 ", 550),
    ] {
        assert_eq!(parse_sub_sec(s.as_bytes()), Ok(Some(ms)), "{s:?}");
    }
    for s in ["", "   ", "\0"] {
        assert_eq!(parse_sub_sec(s.as_bytes()), Ok(None), "{s:?}");
    }
    for s in ["5a", "-5", "0.5", "5 5", "+1"] {
        assert_eq!(parse_sub_sec(s.as_bytes()), Err(InvalidValue), "{s:?}");
    }
}

#[test]
fn offsets_are_minutes_east_of_utc() {
    for (s, m) in [
        ("+11:00", 660),
        ("+01:00", 60),
        ("-05:30", -330),
        ("+00:00", 0),
        ("-00:00", 0),
        ("+05:45", 345),
        ("+14:00", 840),
        ("-12:00", -720),
        ("+01:00\0", 60),
    ] {
        assert_eq!(parse_offset(s.as_bytes()), Ok(Some(m)), "{s:?}");
    }
    for s in ["", "   :  ", "\0"] {
        assert_eq!(parse_offset(s.as_bytes()), Ok(None), "{s:?}");
    }
    for s in [
        "+1100",
        "11:00",
        "+11:60",
        "+14:01",
        "+15:00",
        "+1:00",
        "Z",
        "+11:00:00",
        "~11:00",
        "+aa:00",
    ] {
        assert_eq!(parse_offset(s.as_bytes()), Err(InvalidValue), "{s:?}");
    }
}

#[test]
fn utc_instant_from_the_offset() {
    let t = CaptureTime {
        local_ms: secs("2025:02:15 03:59:50") * 1000,
        offset_minutes: Some(660),
        source: TimeSource::DateTimeOriginal,
    };
    assert_eq!(t.utc_ms(), Some(secs("2025:02:14 16:59:50") * 1000));
    let no_offset = CaptureTime {
        offset_minutes: None,
        ..t
    };
    assert_eq!(no_offset.utc_ms(), None);
    let edge = CaptureTime {
        local_ms: i64::MIN,
        offset_minutes: Some(60),
        source: TimeSource::FileModified,
    };
    assert_eq!(edge.utc_ms(), None);
}

// ---------------------------------------------------------------------------------------------
// Capture time fallbacks
// ---------------------------------------------------------------------------------------------

#[test]
fn capture_time_from_date_time_original_with_sub_seconds_and_offset() {
    let m = PhotoMeta::from_raw(&z6iii(), Some(FileTime::utc(5)));
    let t = m.captured.unwrap();
    assert_eq!(t.source, TimeSource::DateTimeOriginal);
    assert_eq!(t.local_ms, 1_739_591_990_550);
    assert_eq!(t.offset_minutes, Some(660));
    assert!(m.issues.is_empty(), "{:?}", m.issues);
}

#[test]
fn capture_time_falls_back_to_create_date_with_its_own_sub_seconds_and_offset() {
    let raw = RawExif {
        date_time_original: None,
        create_date: b("2025:02:15 04:00:00"),
        sub_sec_time_digitized: b("1"),
        offset_time_digitized: b("-03:00"),
        ..z6iii()
    };
    let t = PhotoMeta::from_raw(&raw, None).captured.unwrap();
    assert_eq!(t.source, TimeSource::CreateDate);
    assert_eq!(t.local_ms, secs("2025:02:15 04:00:00") * 1000 + 100);
    assert_eq!(t.offset_minutes, Some(-180));
}

#[test]
fn a_blank_original_falls_back_silently_and_a_bad_one_is_recorded() {
    let blank = RawExif {
        date_time_original: b("0000:00:00 00:00:00"),
        ..z6iii()
    };
    let m = PhotoMeta::from_raw(&blank, None);
    assert_eq!(m.captured.unwrap().source, TimeSource::CreateDate);
    assert!(m.issues.is_empty());

    let bad = RawExif {
        date_time_original: b("2025:02:30 03:59:50"),
        ..z6iii()
    };
    let m = PhotoMeta::from_raw(&bad, None);
    assert_eq!(m.captured.unwrap().source, TimeSource::CreateDate);
    assert_eq!(m.issues, [MetaIssue::BadDateTime(TimeTag::Original)]);
}

#[test]
fn capture_time_falls_back_to_the_file_time_in_utc() {
    let raw = RawExif {
        date_time_original: b("garbage"),
        create_date: b("    :  :     :  :  "),
        ..z6iii()
    };
    let mtime_ns = 1_739_591_990_123_456_789;
    let m = PhotoMeta::from_raw(&raw, Some(FileTime::utc(mtime_ns)));
    let t = m.captured.unwrap();
    assert_eq!(t.source, TimeSource::FileModified);
    assert_eq!(t.local_ms, 1_739_591_990_123);
    assert_eq!(t.offset_minutes, Some(0));
    assert_eq!(t.utc_ms(), Some(1_739_591_990_123));
    assert_eq!(m.issues, [MetaIssue::BadDateTime(TimeTag::Original)]);
    // Before 1970 the division rounds down, so the millisecond is still the one it falls in.
    assert_eq!(CaptureTime::from_file_time(FileTime::utc(-1)).local_ms, -1);
    assert_eq!(
        CaptureTime::from_file_time(FileTime::utc(i64::MIN)).local_ms,
        i64::MIN / 1_000_000 - 1
    );
    // No source at all.
    assert_eq!(
        PhotoMeta::from_raw(&RawExif::default(), None).captured,
        None
    );
}

#[test]
fn the_file_time_is_read_at_the_pc_offset() {
    // ADR 0028: the file time is shown and sorted in the PC's local time zone. 15 Feb 2025
    // 03:59:50.123 UTC is 04:59:50.123 in Paris (+01:00) and 22:59:50.123 the day before in
    // New York (-05:00); the instant stays the same.
    let utc_ms = 1_739_591_990_123;
    let mtime_ns = utc_ms * 1_000_000 + 456_789;
    for (offset, local) in [
        (60, utc_ms + 3_600_000),
        (-300, utc_ms - 18_000_000),
        (0, utc_ms),
    ] {
        let m = PhotoMeta::without_exif(MetaIssue::NoExif, Some(FileTime::new(mtime_ns, offset)));
        let t = m.captured.unwrap();
        assert_eq!(t.source, TimeSource::FileModified);
        assert_eq!(t.local_ms, local, "{offset}");
        assert_eq!(t.offset_minutes, Some(offset));
        assert_eq!(t.utc_ms(), Some(utc_ms), "{offset}");
    }
    // An EXIF time wins over the file time whatever the PC's offset.
    let m = PhotoMeta::from_raw(&z6iii(), Some(FileTime::new(mtime_ns, 120)));
    assert_eq!(m.captured.unwrap().source, TimeSource::DateTimeOriginal);
    assert_eq!(m.captured.unwrap().offset_minutes, Some(660));
    // The extremes stay in range: nanoseconds become milliseconds before the offset is added.
    let max = CaptureTime::from_file_time(FileTime::new(i64::MAX, i16::MAX));
    assert_eq!(max.local_ms, i64::MAX / 1_000_000 + 32_767 * 60_000);
    let min = CaptureTime::from_file_time(FileTime::new(i64::MIN, i16::MIN));
    assert_eq!(min.local_ms, i64::MIN / 1_000_000 - 1 - 32_768 * 60_000);
}

#[test]
fn bad_sub_seconds_and_offsets_are_dropped_and_the_time_kept() {
    let raw = RawExif {
        sub_sec_time_original: b("x5"),
        offset_time_original: b("+25:00"),
        ..z6iii()
    };
    let m = PhotoMeta::from_raw(&raw, None);
    let t = m.captured.unwrap();
    assert_eq!(t.source, TimeSource::DateTimeOriginal);
    assert_eq!(t.local_ms, 1_739_591_990_000);
    assert_eq!(t.offset_minutes, None);
    assert_eq!(
        m.issues,
        [
            MetaIssue::BadSubSec(TimeTag::Original),
            MetaIssue::BadOffset(TimeTag::Original)
        ]
    );
}

#[test]
fn without_exif_keeps_the_file_time_and_the_issue() {
    let m = PhotoMeta::without_exif(MetaIssue::NoExif, Some(FileTime::utc(2_000_000)));
    assert_eq!(m.captured.unwrap().local_ms, 2);
    assert_eq!(m.orientation, Orientation::Normal);
    assert_eq!(m.lens, None);
    assert_eq!(m.issues, [MetaIssue::NoExif]);
}

// ---------------------------------------------------------------------------------------------
// Orientation, lens, text, exposure
// ---------------------------------------------------------------------------------------------

#[test]
fn orientation_values() {
    for v in 1..=8u32 {
        let o = Orientation::from_exif(v).unwrap();
        assert_eq!(u32::from(o.exif_value()), v);
        assert_eq!(o.swaps_dimensions(), v >= 5, "{v}");
    }
    for v in [0, 9, 255, 65_535, u32::MAX] {
        assert_eq!(Orientation::from_exif(v), None, "{v}");
    }
    assert_eq!(Orientation::default(), Orientation::Normal);
    assert_eq!(Orientation::from_exif(6), Some(Orientation::Rotate90));
    assert_eq!(Orientation::from_exif(8), Some(Orientation::Rotate270));
}

#[test]
fn orientation_in_meta_defaults_to_1_and_records_invalid_values() {
    let m = PhotoMeta::from_raw(&z6iii(), None);
    assert_eq!(m.orientation, Orientation::Rotate270);
    let absent = RawExif {
        orientation: None,
        ..z6iii()
    };
    let m = PhotoMeta::from_raw(&absent, None);
    assert_eq!(m.orientation, Orientation::Normal);
    assert!(m.issues.is_empty());
    for v in [0, 9, u32::MAX] {
        let bad = RawExif {
            orientation: Some(v),
            ..z6iii()
        };
        let m = PhotoMeta::from_raw(&bad, None);
        assert_eq!(m.orientation, Orientation::Normal);
        assert_eq!(m.issues, [MetaIssue::BadOrientation(v)]);
    }
}

#[test]
fn lens_model_first_then_specification_then_none() {
    let spec = z6iii().lens_specification;
    assert_eq!(
        lens(Some(b"NIKKOR Z 24-200mm f/4-6.3 VR"), &spec).as_deref(),
        Some("NIKKOR Z 24-200mm f/4-6.3 VR")
    );
    assert_eq!(lens(None, &spec).as_deref(), Some("24-200mm f/4-6.3"));
    // A LensModel with no text falls back too.
    assert_eq!(
        lens(Some(b"  \0"), &spec).as_deref(),
        Some("24-200mm f/4-6.3")
    );
    assert_eq!(lens(None, &[]), None);
    assert_eq!(lens(Some(b""), &[]), None);
}

#[test]
fn lens_from_specification_formats() {
    let r = Rational::new;
    let unknown = r(0, 0);
    for (spec, want) in [
        (
            vec![r(50, 1), r(50, 1), r(18, 10), r(18, 10)],
            Some("50mm f/1.8"),
        ),
        (
            vec![r(24, 1), r(70, 1), r(28, 10), r(28, 10)],
            Some("24-70mm f/2.8"),
        ),
        (vec![r(24, 1), r(70, 1), unknown, unknown], Some("24-70mm")),
        (
            vec![r(24, 1), r(70, 1), r(4, 1), unknown],
            Some("24-70mm f/4"),
        ),
        (
            vec![r(45, 10), r(45, 10), r(28, 10), unknown],
            Some("4.5mm f/2.8"),
        ),
        (
            vec![r(200, 1), r(24, 1), r(4, 1), r(63, 10)],
            Some("24-200mm f/4-6.3"),
        ),
        (
            vec![r(85, 1), unknown, r(12, 10), unknown],
            Some("85mm f/1.2"),
        ),
        (vec![unknown, unknown, r(4, 1), r(4, 1)], None),
        (vec![r(0, 1), r(0, 5), unknown, unknown], None),
        (vec![r(24, 1), r(70, 1), r(28, 10)], None),
        (vec![r(24, 1); 5], None),
        (
            vec![
                r(u32::MAX, 1),
                r(u32::MAX, 1),
                r(u32::MAX, 3),
                r(1, u32::MAX),
            ],
            Some("4294967295mm f/1431655765"),
        ),
    ] {
        assert_eq!(lens_from_specification(&spec).as_deref(), want, "{spec:?}");
    }
}

#[test]
fn text_is_cleaned_and_capped() {
    assert_eq!(clean_text(b"NIKON Z6_3").as_deref(), Some("NIKON Z6_3"));
    assert_eq!(
        clean_text(b"  NIKON Z6_3 \0\0").as_deref(),
        Some("NIKON Z6_3")
    );
    assert_eq!(clean_text(b"a\x07b\nc\x1b[31m").as_deref(), Some("abc[31m"));
    assert_eq!(clean_text(b"caf\xe9").as_deref(), Some("caf\u{fffd}"));
    assert_eq!(clean_text(b"<script>").as_deref(), Some("<script>"));
    // Bidi overrides and isolates, zero-width characters, BOM, soft hyphen, tags, separators.
    for c in [
        '\u{202A}',
        '\u{202B}',
        '\u{202C}',
        '\u{202D}',
        '\u{202E}',
        '\u{2066}',
        '\u{2067}',
        '\u{2068}',
        '\u{2069}',
        '\u{200B}',
        '\u{200C}',
        '\u{200D}',
        '\u{200E}',
        '\u{200F}',
        '\u{FEFF}',
        '\u{00AD}',
        '\u{2060}',
        '\u{061C}',
        '\u{E0041}',
        '\u{2028}',
        '\u{2029}',
    ] {
        let s = format!("NIK{c}KOR");
        assert_eq!(
            clean_text(s.as_bytes()).as_deref(),
            Some("NIKKOR"),
            "{:04X}",
            u32::from(c)
        );
        assert_eq!(clean_text(c.to_string().as_bytes()), None);
    }
    assert_eq!(
        clean_text("lens \u{202E}gnp.exe".as_bytes()).as_deref(),
        Some("lens gnp.exe")
    );
    // Ordinary letters and symbols around those ranges stay.
    assert_eq!(
        clean_text("Ø 24–70 ƒ/2.8 µ".as_bytes()).as_deref(),
        Some("Ø 24–70 ƒ/2.8 µ")
    );
    assert_eq!(clean_text(b""), None);
    assert_eq!(clean_text(b" \0 \t"), None);
    let long = "x".repeat(1000);
    let capped = clean_text(long.as_bytes()).unwrap();
    assert_eq!(capped.chars().count(), MAX_TEXT_CHARS);
    let multibyte = "é".repeat(200);
    assert_eq!(
        clean_text(multibyte.as_bytes()).unwrap().chars().count(),
        MAX_TEXT_CHARS
    );
}

#[test]
fn exposure_fields_drop_zero_and_unknown_values() {
    let m = PhotoMeta::from_raw(&z6iii(), None);
    assert_eq!(m.camera_model.as_deref(), Some("NIKON Z6_3"));
    assert_eq!(m.shutter, Some(Rational::new(10, 1250)));
    assert_eq!(m.aperture.and_then(Rational::tenths), Some(56));
    assert_eq!(m.iso, Some(450));
    assert_eq!(m.focal_length.and_then(Rational::tenths), Some(480));
    assert_eq!((m.width, m.height), (Some(6048), Some(4032)));

    let zero = RawExif {
        exposure_time: Some(Rational::new(0, 1)),
        f_number: Some(Rational::new(28, 0)),
        iso: Some(0),
        focal_length: Some(Rational::new(0, 0)),
        pixel_x: Some(0),
        pixel_y: None,
        model: b("\0"),
        ..z6iii()
    };
    let m = PhotoMeta::from_raw(&zero, None);
    assert_eq!(
        (
            m.shutter,
            m.aperture,
            m.iso,
            m.focal_length,
            m.width,
            m.height
        ),
        (None, None, None, None, None, None)
    );
    assert_eq!(m.camera_model, None);
}

#[test]
fn rational_tenths_rounds_half_up_without_overflow() {
    assert_eq!(Rational::new(63, 10).tenths(), Some(63));
    assert_eq!(Rational::new(1, 3).tenths(), Some(3));
    assert_eq!(Rational::new(1, 4).tenths(), Some(3));
    assert_eq!(Rational::new(1, 0).tenths(), None);
    assert_eq!(Rational::new(u32::MAX, 1).tenths(), Some(42_949_672_950));
}

#[test]
fn no_input_panics() {
    // A small exhaustive-ish sweep over short byte strings for every parser.
    let alphabet = [b'0', b'9', b':', b' ', b'-', b'+', b'T', 0, 0xFF, b'a'];
    let mut inputs: Vec<Vec<u8>> = vec![Vec::new()];
    for len in 1..=4 {
        let mut next = Vec::new();
        for s in inputs.iter().filter(|s| s.len() == len - 1) {
            for &c in &alphabet {
                let mut t = s.clone();
                t.push(c);
                next.push(t);
            }
        }
        inputs.extend(next);
    }
    // Plus long and nearly valid strings, mutated one byte at a time.
    let valid = b"2025:02:15 03:59:50".to_vec();
    for i in 0..valid.len() {
        for &c in &alphabet {
            let mut t = valid.clone();
            t[i] = c;
            inputs.push(t);
        }
    }
    inputs.push(vec![b'9'; 10_000]);
    for s in &inputs {
        let _ = parse_date_time(s);
        let _ = parse_sub_sec(s);
        let _ = parse_offset(s);
        let _ = clean_text(s);
        let raw = RawExif {
            date_time_original: Some(s.clone()),
            sub_sec_time_original: Some(s.clone()),
            offset_time_original: Some(s.clone()),
            lens_model: Some(s.clone()),
            ..RawExif::default()
        };
        let _ = PhotoMeta::from_raw(&raw, Some(FileTime::new(i64::MAX, i16::MAX)));
    }
}

// ---------------------------------------------------------------------------------------------
// Capture order (US-06)
// ---------------------------------------------------------------------------------------------

fn groups(files: &[(&[&str], &str)]) -> Vec<Group> {
    let entries = files
        .iter()
        .map(|(folder, name)| ScanEntry::file(folder, name));
    grouping::group(entries, &ScanPolicy::new(true)).groups
}

fn at(s: &str) -> Option<CaptureTime> {
    Some(CaptureTime {
        local_ms: secs(s) * 1000,
        offset_minutes: None,
        source: TimeSource::DateTimeOriginal,
    })
}

fn stems(groups: &[Group], order: &[usize]) -> Vec<String> {
    order
        .iter()
        .map(|&i| {
            let g = &groups[i];
            let mut c = g.folder.to_vec();
            c.push(g.stem.clone());
            c.join("\\")
        })
        .collect()
}

#[test]
fn capture_order_sorts_by_time_across_folders() {
    let g = groups(&[
        (&["100NIKON"], "DSC_0001.JPG"),
        (&["100NIKON"], "DSC_0001.NEF"),
        (&["100NIKON"], "DSC_0002.NEF"),
        (&["101NIKON"], "DSC_0001.JPG"),
    ]);
    assert_eq!(
        stems(&g, &filename_order(&g)),
        [
            "100NIKON\\DSC_0001",
            "100NIKON\\DSC_0002",
            "101NIKON\\DSC_0001"
        ]
    );
    // The counter rolled over: 101NIKON\DSC_0001 was shot first.
    let times = [
        at("2025:02:15 10:00:02"),
        at("2025:02:15 10:00:03"),
        at("2025:02:15 10:00:01"),
    ];
    assert_eq!(
        stems(&g, &capture_order(&g, &times)),
        [
            "101NIKON\\DSC_0001",
            "100NIKON\\DSC_0001",
            "100NIKON\\DSC_0002"
        ]
    );
}

#[test]
fn equal_times_are_ordered_by_folded_path() {
    // Folders differing by case are separate groups (marked ambiguous, still ordered).
    let g = groups(&[
        (&["b"], "X.JPG"),
        (&["A"], "X.JPG"),
        (&["a"], "X.JPG"),
        (&["C"], "X.JPG"),
    ]);
    let same = at("2025:02:15 10:00:00");
    let times = vec![same; g.len()];
    // Folded first ("A" == "a"), then as written ("A" < "a").
    assert_eq!(
        stems(&g, &capture_order(&g, &times)),
        ["A\\X", "a\\X", "b\\X", "C\\X"]
    );
    // Sub-seconds separate a burst.
    let mut burst = times.clone();
    for (i, t) in burst.iter_mut().enumerate() {
        let shift = 30 - 10 * i64::try_from(i).unwrap();
        t.as_mut().unwrap().local_ms += shift;
    }
    let order = capture_order(&g, &burst);
    assert_eq!(order, [3, 2, 1, 0]);
}

#[test]
fn groups_without_a_time_keep_their_filename_place() {
    let g = groups(&[
        (&[], "DSC_0001.JPG"),
        (&[], "DSC_0002.JPG"),
        (&[], "DSC_0003.JPG"),
        (&[], "DSC_0004.JPG"),
        (&[], "DSC_0005.JPG"),
    ]);
    let times = [
        None,
        at("2025:02:15 10:00:05"),
        None,
        None,
        at("2025:02:15 10:00:01"),
    ];
    // 0001 has no dated predecessor: first. 0003 and 0004 follow 0002, in filename order.
    assert_eq!(
        stems(&g, &capture_order(&g, &times)),
        ["DSC_0001", "DSC_0005", "DSC_0002", "DSC_0003", "DSC_0004"]
    );
    // A shorter slice: the missing entries have no time.
    assert_eq!(
        stems(&g, &capture_order(&g, &times[..2])),
        ["DSC_0001", "DSC_0002", "DSC_0003", "DSC_0004", "DSC_0005"]
    );
    // An undated group goes right after its dated predecessor, even when another group shares
    // that predecessor's time.
    let tie = [
        at("2025:02:15 10:00:00"),
        None,
        at("2025:02:15 10:00:00"),
        at("2025:02:15 09:00:00"),
        None,
    ];
    assert_eq!(
        stems(&g, &capture_order(&g, &tie)),
        ["DSC_0004", "DSC_0005", "DSC_0001", "DSC_0002", "DSC_0003"]
    );
}

#[test]
fn capture_order_ignores_input_order_and_is_a_permutation() {
    let mut g = groups(&[
        (&["a"], "DSC_0001.JPG"),
        (&["a"], "DSC_0002.NEF"),
        (&["B"], "DSC_0001.JPG"),
        (&["b"], "DSC_0003.JPG"),
        (&[], "DSC_0009.JPG"),
    ]);
    let mut times = vec![
        at("2025:02:15 10:00:00"),
        None,
        at("2025:02:15 09:00:00"),
        at("2025:02:15 10:00:00"),
        None,
    ];
    let expected = stems(&g, &capture_order(&g, &times));
    let mut sorted = capture_order(&g, &times);
    sorted.sort_unstable();
    assert_eq!(sorted, (0..g.len()).collect::<Vec<_>>());
    // Reverse the input (and its times): same result by path.
    g.reverse();
    times.reverse();
    assert_eq!(stems(&g, &capture_order(&g, &times)), expected);
    // Rotate it.
    g.rotate_left(2);
    times.rotate_left(2);
    assert_eq!(stems(&g, &capture_order(&g, &times)), expected);
    assert!(capture_order(&[], &[]).is_empty());
}

#[test]
fn a_lone_nef_sorts_by_its_own_time_and_a_pair_by_its_jpeg() {
    let g = groups(&[
        (&[], "DSC_0001.JPG"),
        (&[], "DSC_0001.NEF"),
        (&[], "DSC_0002.NEF"),
    ]);
    assert_eq!(g[0].primary.entry.name, "DSC_0001.JPG");
    assert!(g[1].is_lone_nef());
    let times = [at("2025:02:15 10:00:02"), at("2025:02:15 10:00:01")];
    assert_eq!(
        stems(&g, &capture_order(&g, &times)),
        ["DSC_0002", "DSC_0001"]
    );
}
