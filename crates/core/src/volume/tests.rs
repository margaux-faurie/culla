//! Volume identity tests: keys that ignore the drive letter (US-09), the run binding of
//! design/file-engine.md §6.3, and the supported path forms.

use super::*;

const GUID_E: &str = "\\\\?\\Volume{0a1b2c3d-1111-2222-3333-444455556666}\\";
const GUID_F: &str = "\\\\?\\Volume{9f8e7d6c-aaaa-bbbb-cccc-ddddeeeeffff}\\";

fn info(serial: u32, guid: &str, mount: &str) -> VolumeInfo {
    VolumeInfo {
        id: VolumeId::new(serial),
        guid_path: VolumeGuidPath::parse(guid).unwrap(),
        mount_point: mount.to_owned(),
        label: Some("NIKON Z6_3".to_owned()),
        fs_name: "exFAT".to_owned(),
        flags: VolumeFlags(0),
        max_component_len: 255,
        drive_kind: DriveKind::Removable,
    }
}

fn source(serial: u32, path: &str, root: &str) -> Source {
    Source::new(VolumeId::new(serial), rel_under_root(path, root).unwrap()).unwrap()
}

// ---------------------------------------------------------------------------------------------
// US-09: identity survives a drive letter change
// ---------------------------------------------------------------------------------------------

#[test]
fn a_card_back_under_another_letter_keeps_its_key() {
    // The session was made for E:\DCIM on the card with serial S; the card comes back as F:,
    // through a verbatim path, with other case, and as a folder mount.
    let first = source(0x1A2B_3C4D, "E:\\DCIM", "E:\\");
    for (path, root) in [
        ("F:\\DCIM", "F:\\"),
        ("\\\\?\\F:\\DCIM\\", "F:\\"),
        ("f:\\dcim", "F:\\"),
        ("C:\\Mounts\\Card\\DCIM", "C:\\Mounts\\Card\\"),
        ("c:\\mounts\\card\\DCIM", "C:\\Mounts\\Card\\"),
    ] {
        let again = source(0x1A2B_3C4D, path, root);
        assert_eq!(again.key(), first.key(), "{path}");
    }
    assert_eq!(first.key().rel_key, "DCIM");
    assert_eq!(first.rel_path(), "DCIM");
}

#[test]
fn two_cards_with_the_same_folders_get_two_keys() {
    let a = source(0x1111_1111, "E:\\DCIM\\100NIKON", "E:\\");
    let b = source(0x2222_2222, "E:\\DCIM\\100NIKON", "E:\\");
    assert_ne!(a.key(), b.key());
    assert_eq!(a.key().rel_key, b.key().rel_key);
}

#[test]
fn the_key_folds_each_component() {
    let s = source(7, "E:\\Dcim\\100nikon", "E:\\");
    assert_eq!(s.components(), ["Dcim", "100nikon"]);
    assert_eq!(s.rel_path(), "Dcim\\100nikon");
    assert_eq!(
        s.key(),
        SourceKey {
            serial: 7,
            rel_key: "DCIM\\100NIKON".to_owned()
        }
    );
    // The volume root is a source too: an empty path.
    let root = source(7, "E:\\", "E:\\");
    assert!(root.components().is_empty());
    assert_eq!(root.key().rel_key, "");
}

#[test]
fn the_record_keeps_its_identity_and_refreshes_the_rest() {
    let on_e = info(0x1A2B_3C4D, GUID_E, "E:\\");
    let mut record = VolumeRecord::from_info(&on_e);
    assert_eq!(record.observe(&on_e), Ok(RecordChange::default()));
    assert!(record.observe(&on_e).unwrap().is_empty());

    // Same card, now F:, and Windows gave it another GUID (another PC).
    let on_f = info(0x1A2B_3C4D, GUID_F, "F:\\");
    let change = record.observe(&on_f).unwrap();
    assert_eq!(
        change,
        RecordChange {
            mount: true,
            guid: true,
            label: false,
            fs_name: false
        }
    );
    assert_eq!(record.id, VolumeId::new(0x1A2B_3C4D));
    assert_eq!(record.last_mount.as_deref(), Some("F:\\"));
    assert_eq!(
        record.last_guid.as_ref().map(VolumeGuidPath::as_str),
        Some(GUID_F)
    );

    // Another card is refused and leaves the record as it was.
    let other = info(0x0BAD_CAFE, GUID_E, "E:\\");
    let before = record.clone();
    assert_eq!(record.observe(&other), Err(NotSameVolume));
    assert_eq!(record, before);
}

// ---------------------------------------------------------------------------------------------
// Run binding (§6.3)
// ---------------------------------------------------------------------------------------------

#[test]
fn a_serial_binds_to_exactly_one_mounted_volume() {
    let card = info(0x1A2B_3C4D, GUID_E, "F:\\");
    let internal = info(0x0102_0304, GUID_F, "C:\\");
    let mounted = [internal.clone(), card.clone()];
    assert_eq!(bind(card.id, &mounted), Binding::Bound(&mounted[1]));
    assert_eq!(bind(VolumeId::new(9), &mounted), Binding::Missing);
    assert_eq!(bind(card.id, &[]), Binding::Missing);

    // A second card with the same serial: the run pauses and the user picks.
    let clone = info(0x1A2B_3C4D, GUID_F, "G:\\");
    let mounted = [card.clone(), internal, clone];
    assert_eq!(
        bind(card.id, &mounted),
        Binding::Ambiguous(vec![&mounted[0], &mounted[2]])
    );

    // One volume listed through two mount points is still one volume.
    let mut folder_mount = card.clone();
    folder_mount.mount_point = "C:\\Mounts\\Card\\".to_owned();
    let mounted = [card, folder_mount];
    assert_eq!(bind(mounted[0].id, &mounted), Binding::Bound(&mounted[0]));
}

#[test]
fn same_volume_is_decided_by_guid() {
    let card = info(1, GUID_E, "E:\\");
    let mut same = info(1, GUID_E, "C:\\Mounts\\Card\\");
    same.label = None;
    assert_eq!(relation(&card, &same), VolumeRelation::Same);
    assert_eq!(
        relation(&card, &info(2, GUID_F, "F:\\")),
        VolumeRelation::Different
    );
    assert_eq!(
        relation(&card, &info(1, GUID_F, "F:\\")),
        VolumeRelation::SerialClash
    );
}

#[test]
fn nested_sources_are_found_on_folded_paths() {
    let dcim = source(1, "E:\\DCIM", "E:\\").key();
    let nikon = source(1, "E:\\dcim\\100NIKON", "E:\\").key();
    let dcim2 = source(1, "E:\\DCIM2", "E:\\").key();
    let root = source(1, "E:\\", "E:\\").key();
    let other_card = source(2, "E:\\DCIM\\100NIKON", "E:\\").key();
    assert!(dcim.contains(&nikon));
    assert!(dcim.contains(&dcim));
    assert!(!nikon.contains(&dcim));
    assert!(nikon.nested_with(&dcim));
    assert!(!dcim.nested_with(&dcim2), "a shared prefix is not nesting");
    assert!(root.contains(&dcim2));
    assert!(!dcim.nested_with(&other_card));
}

// ---------------------------------------------------------------------------------------------
// Values from Win32
// ---------------------------------------------------------------------------------------------

#[test]
fn serials_show_like_vol() {
    assert_eq!(VolumeId::new(0x1A2B_3C4D).to_string(), "1A2B-3C4D");
    assert_eq!(VolumeId::new(0x0000_00FF).to_string(), "0000-00FF");
}

#[test]
fn drive_types_and_flags_map() {
    let kinds: Vec<_> = (0..8).map(DriveKind::from_win32).collect();
    assert_eq!(
        kinds,
        [
            DriveKind::Unknown,
            DriveKind::NoRootDir,
            DriveKind::Removable,
            DriveKind::Fixed,
            DriveKind::Remote,
            DriveKind::CdRom,
            DriveKind::RamDisk,
            DriveKind::Unknown
        ]
    );
    assert_eq!(DriveKind::from_win32(u32::MAX), DriveKind::Unknown);
    assert!(VolumeFlags(0x0008_0000).read_only());
    assert!(VolumeFlags(0x0008_0000 | 0x0000_0002).read_only());
    assert!(!VolumeFlags(0x0000_0002).read_only());
    let mut v = info(1, GUID_E, "E:\\");
    assert!(!v.read_only());
    v.flags = VolumeFlags(VolumeFlags::READ_ONLY);
    assert!(v.read_only());
}

#[test]
fn guid_paths_have_one_exact_form() {
    let upper = "\\\\?\\Volume{0A1B2C3D-1111-2222-3333-444455556666}\\";
    assert_eq!(VolumeGuidPath::parse(upper).unwrap().as_str(), GUID_E);
    assert_eq!(
        VolumeGuidPath::parse(upper).unwrap(),
        VolumeGuidPath::parse(GUID_E).unwrap()
    );
    for bad in [
        "",
        "E:\\",
        "\\\\?\\Volume{0a1b2c3d-1111-2222-3333-444455556666}",
        "\\\\?\\Volume{0a1b2c3d-1111-2222-3333-444455556666}\\x",
        "\\\\?\\Volume{0a1b2c3d-1111-2222-3333-44445555666}\\",
        "\\\\?\\Volume{0a1b2c3d-1111-2222-3333-4444555566667}\\",
        "\\\\?\\Volume{0a1b2c3d-1111-2222-3333-444455556666-0}\\",
        "\\\\?\\Volume{0a1b2c3d11112222-3333-444455556666}\\",
        "\\\\?\\Volume{0a1b2c3g-1111-2222-3333-444455556666}\\",
        "\\\\.\\Volume{0a1b2c3d-1111-2222-3333-444455556666}\\",
        "\\\\?\\Volume{0a1b2c3d-1111-2222-3333-44445555666é}\\",
    ] {
        assert_eq!(
            VolumeGuidPath::parse(bad),
            Err(VolumePathError::BadGuidPath),
            "{bad}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Path forms
// ---------------------------------------------------------------------------------------------

#[test]
fn drive_paths_and_their_verbatim_form_are_supported() {
    assert_eq!(classify_path("E:\\DCIM"), Ok("E:\\DCIM"));
    assert_eq!(classify_path("\\\\?\\E:\\DCIM"), Ok("E:\\DCIM"));
    assert_eq!(classify_path("z:\\"), Ok("z:\\"));
    for (path, why) in [
        ("DCIM", UnsupportedPath::NotAbsolute),
        ("E:DCIM", UnsupportedPath::NotAbsolute),
        ("E:", UnsupportedPath::NotAbsolute),
        ("\\DCIM", UnsupportedPath::NotAbsolute),
        ("", UnsupportedPath::NotAbsolute),
        ("1:\\DCIM", UnsupportedPath::NotAbsolute),
        ("é:\\DCIM", UnsupportedPath::NotAbsolute),
        ("\\\\server\\share\\DCIM", UnsupportedPath::Network),
        ("//server/share", UnsupportedPath::Network),
        ("\\\\?\\UNC\\server\\share", UnsupportedPath::Network),
        ("\\\\?\\unc\\server\\share", UnsupportedPath::Network),
        (GUID_E, UnsupportedPath::VolumeGuid),
        (
            "\\\\?\\GLOBALROOT\\Device\\HarddiskVolume3\\",
            UnsupportedPath::Device,
        ),
        ("\\\\.\\E:", UnsupportedPath::Device),
        ("\\\\.\\PhysicalDrive0", UnsupportedPath::Device),
        ("\\??\\E:\\DCIM", UnsupportedPath::Device),
        ("\\\\?\\E:", UnsupportedPath::Device),
    ] {
        assert_eq!(classify_path(path), Err(why), "{path}");
    }
}

#[test]
fn paths_split_under_their_root() {
    let ok = |path: &str, root: &str| rel_under_root(path, root).unwrap();
    assert_eq!(ok("E:\\DCIM\\100NIKON", "E:\\"), ["DCIM", "100NIKON"]);
    assert_eq!(ok("E:\\DCIM\\100NIKON\\", "e:\\"), ["DCIM", "100NIKON"]);
    assert!(ok("E:\\", "E:\\").is_empty());
    assert_eq!(ok("C:\\Mounts\\Card\\DCIM", "C:\\Mounts\\Card\\"), ["DCIM"]);
    assert!(ok("C:\\Mounts\\Card", "C:\\Mounts\\Card\\").is_empty());
    assert_eq!(ok("\\\\?\\E:\\Été\\ß", "E:\\"), ["Été", "ß"]);

    let err = |path: &str, root: &str| rel_under_root(path, root).unwrap_err();
    assert_eq!(err("F:\\DCIM", "E:\\"), VolumePathError::NotUnderRoot);
    assert_eq!(
        err("C:\\Mounts\\Cards\\DCIM", "C:\\Mounts\\Card\\"),
        VolumePathError::NotUnderRoot
    );
    assert_eq!(
        err("C:\\Mounts", "C:\\Mounts\\Card\\"),
        VolumePathError::NotUnderRoot
    );
    for root in ["E:", "E:\\DCIM", "\\\\?\\E:\\", "DCIM\\", ""] {
        assert_eq!(err("E:\\DCIM", root), VolumePathError::BadRoot, "{root:?}");
    }
    assert_eq!(
        err("\\\\server\\share\\DCIM", "E:\\"),
        VolumePathError::Unsupported(UnsupportedPath::Network)
    );
    assert_eq!(
        err("E:\\DCIM\\\\x", "E:\\"),
        VolumePathError::Name(NameError::Empty)
    );
    assert_eq!(
        err("E:\\DCIM\\..", "E:\\"),
        VolumePathError::Name(NameError::DotOrDotDot)
    );
    assert_eq!(
        err("E:\\DCIM\\a/b", "E:\\"),
        VolumePathError::Name(NameError::ReservedChar('/'))
    );
    assert_eq!(
        err("E:\\DCIM\\NUL", "E:\\"),
        VolumePathError::Name(NameError::DeviceName)
    );
}

#[test]
fn sources_take_only_valid_components() {
    assert_eq!(
        Source::new(VolumeId::new(1), vec!["DCIM".into(), "..".into()]),
        Err(NameError::DotOrDotDot)
    );
    assert_eq!(
        Source::new(VolumeId::new(1), vec![String::new()]),
        Err(NameError::Empty)
    );
    let s = Source::new(VolumeId::new(1), vec!["DCIM".into()]).unwrap();
    assert_eq!(s.volume(), VolumeId::new(1));
}

#[test]
fn odd_input_never_panics() {
    for s in [
        "",
        "\\",
        "\\\\",
        "\\\\?",
        "\\\\?\\",
        "\\\\?\\Volume{",
        "E",
        "E:",
        "E:\\\\",
        "é",
        "\u{0}",
        "\\\\?\\é:\\",
        "E:\\\u{0}",
    ] {
        let _ = classify_path(s);
        let _ = rel_under_root(s, "E:\\");
        let _ = rel_under_root("E:\\x", s);
        let _ = VolumeGuidPath::parse(s);
    }
}

// ---------------------------------------------------------------------------------------------
// What v1 refuses (decided by Margaux, 27 Nov 2026)
// ---------------------------------------------------------------------------------------------

#[test]
fn network_drives_invalid_roots_and_serial_zero_are_refused() {
    let ok = info(1, GUID_E, "E:\\");
    assert_eq!(check_volume(&ok), Ok(()));
    for kind in [
        DriveKind::Removable,
        DriveKind::Fixed,
        DriveKind::CdRom,
        DriveKind::RamDisk,
        DriveKind::Unknown,
    ] {
        let v = VolumeInfo {
            drive_kind: kind,
            ..ok.clone()
        };
        assert_eq!(check_volume(&v), Ok(()), "{kind:?}");
    }
    for kind in [DriveKind::Remote, DriveKind::NoRootDir] {
        let v = VolumeInfo {
            drive_kind: kind,
            ..ok.clone()
        };
        assert_eq!(
            check_volume(&v),
            Err(VolumePathError::Unsupported(UnsupportedPath::Network)),
            "{kind:?}"
        );
    }
    let zero = info(0, GUID_E, "E:\\");
    assert_eq!(check_volume(&zero), Err(VolumePathError::NoSerial));
    // A network drive with serial 0 is refused as a network drive.
    let both = VolumeInfo {
        drive_kind: DriveKind::Remote,
        ..zero
    };
    assert_eq!(
        check_volume(&both),
        Err(VolumePathError::Unsupported(UnsupportedPath::Network))
    );
    // GUID-only volumes never get this far: their paths are refused by form.
    assert_eq!(classify_path(GUID_E), Err(UnsupportedPath::VolumeGuid));
}

#[test]
fn the_chosen_folder_must_be_a_plain_folder() {
    let plain = Attributes::default();
    assert_eq!(check_chosen_folder(plain, true), Ok(()));
    // Read-only, hidden or system folders are still folders (camera cards mark DCIM so).
    let marked = Attributes {
        read_only: true,
        hidden: true,
        system: true,
        ..plain
    };
    assert_eq!(check_chosen_folder(marked, true), Ok(()));
    assert_eq!(
        check_chosen_folder(plain, false),
        Err(VolumePathError::NotAFolder)
    );
    let link = Attributes {
        reparse_point: true,
        ..plain
    };
    // A directory symlink is not a dir to symlink_metadata: the link reason wins.
    assert_eq!(
        check_chosen_folder(link, false),
        Err(VolumePathError::ReparsePoint)
    );
    assert_eq!(
        check_chosen_folder(link, true),
        Err(VolumePathError::ReparsePoint)
    );
    let cloud = Attributes {
        cloud_placeholder: true,
        ..plain
    };
    assert_eq!(
        check_chosen_folder(cloud, true),
        Err(VolumePathError::CloudPlaceholder)
    );
}

#[test]
fn every_component_below_the_drive_is_validated() {
    for ok in [
        "E:\\",
        "E:\\DCIM",
        "E:\\DCIM\\",
        "\\\\?\\E:\\DCIM\\100NIKON",
        "C:\\Program Files (x86)\\Été",
    ] {
        assert_eq!(validate_drive_path(ok), Ok(()), "{ok}");
    }
    for (bad, why) in [
        ("E:\\DCIM\\\\x", NameError::Empty),
        ("E:\\\\x", NameError::Empty),
        ("E:\\DCIM\\..\\x", NameError::DotOrDotDot),
        ("E:\\DCIM\\.", NameError::DotOrDotDot),
        ("E:\\a/b", NameError::ReservedChar('/')),
        ("E:\\a:b", NameError::ReservedChar(':')),
        ("E:\\NUL\\x", NameError::DeviceName),
        ("E:\\DCIM.\\x", NameError::TrailingSpaceOrDot),
        ("E:\\x\u{1}", NameError::ControlChar),
    ] {
        assert_eq!(
            validate_drive_path(bad),
            Err(VolumePathError::Name(why)),
            "{bad}"
        );
    }
    assert_eq!(
        validate_drive_path("\\\\server\\share"),
        Err(VolumePathError::Unsupported(UnsupportedPath::Network))
    );
    assert_eq!(
        validate_drive_path("DCIM"),
        Err(VolumePathError::Unsupported(UnsupportedPath::NotAbsolute))
    );
}
