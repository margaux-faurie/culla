//! Grouping and exclusion tests: examples from SPEC §3.1, stories US-02 to US-05 and
//! file-engine §3.2–§3.3, then property-style tests over generated trees (plain loops with a
//! seeded generator: `proptest` is not a dependency).

use super::*;

fn f(folder: &[&str], name: &str) -> ScanEntry {
    ScanEntry::file(folder, name)
}

fn d(folder: &[&str], name: &str) -> ScanEntry {
    ScanEntry::dir(folder, name)
}

fn with(mut e: ScanEntry, attrs: Attributes) -> ScanEntry {
    e.attrs = attrs;
    e
}

fn all() -> ScanPolicy {
    ScanPolicy::new(true)
}

fn names<'a>(it: impl Iterator<Item = &'a ScanEntry>) -> Vec<String> {
    it.map(ScanEntry::rel_path).collect()
}

fn unknown(g: &Grouping) -> Vec<(String, &'static str)> {
    g.unknown
        .iter()
        .map(|u| (u.entry.rel_path(), u.kind.issue_kind()))
        .collect()
}

fn excluded(g: &Grouping) -> Vec<(String, ExclusionReason)> {
    g.excluded
        .iter()
        .map(|x| (x.entry.rel_path(), x.reason))
        .collect()
}

fn sidecars(g: &Group) -> Vec<(String, SidecarKind, Describes)> {
    g.sidecars
        .iter()
        .map(|s| (s.entry.rel_path(), s.kind, s.describes))
        .collect()
}

// --- US-02: a JPEG, its NEF and its sidecars -------------------------------------------------

/// INV-09: each sidecar is bound to what it follows.
#[test]
fn us02_jpeg_nef_and_three_sidecars_form_one_group() {
    let out = group(
        [
            f(&[], "DSC_0001.JPG"),
            f(&[], "dsc_0001.nef"),
            f(&[], "DSC_0001.xmp"),
            f(&[], "DSC_0001.NEF.xmp"),
            d(&[], "NKSC_PARAM"),
            f(&["NKSC_PARAM"], "DSC_0001.NEF.nksc"),
        ],
        &all(),
    );
    assert_eq!(out.groups.len(), 1);
    assert!(out.unknown.is_empty() && out.excluded.is_empty());
    let g = &out.groups[0];
    assert_eq!(g.status, GroupStatus::Ok);
    assert_eq!(g.stem, "DSC_0001");
    assert_eq!(g.stem_key, "DSC_0001");
    assert_eq!(g.primary.kind, PhotoKind::Jpeg);
    assert_eq!(g.primary.entry.name, "DSC_0001.JPG");
    assert_eq!(
        g.raw.as_ref().map(|r| r.entry.name.as_str()),
        Some("dsc_0001.nef")
    );
    assert!(g.duplicates.is_empty());
    assert_eq!(
        sidecars(g),
        [
            (
                "DSC_0001.NEF.xmp".into(),
                SidecarKind::FileXmp,
                Describes::Raw
            ),
            ("DSC_0001.xmp".into(), SidecarKind::Xmp, Describes::Group),
            (
                "NKSC_PARAM\\DSC_0001.NEF.nksc".into(),
                SidecarKind::Nksc,
                Describes::Raw
            ),
        ]
    );
    // ADR 0023: the bare X.xmp follows the NEF.
    assert!(g.sidecars.iter().all(|s| g.follows(s) == Describes::Raw));
}

#[test]
fn us02_each_sidecar_records_the_file_it_describes() {
    let out = group(
        [
            f(&[], "X.JPG"),
            f(&[], "X.NEF"),
            f(&[], "X.JPG.xmp"),
            f(&[], "x.jpg.PP3"),
            f(&[], "X.NEF.pp3"),
            f(&[], "X.pp3"),
            f(&["NKSC_PARAM"], "X.JPG.nksc"),
        ],
        &all(),
    );
    let g = &out.groups[0];
    assert_eq!(
        sidecars(g),
        [
            ("x.jpg.PP3".into(), SidecarKind::FilePp3, Describes::Primary),
            ("X.JPG.xmp".into(), SidecarKind::FileXmp, Describes::Primary),
            ("X.NEF.pp3".into(), SidecarKind::FilePp3, Describes::Raw),
            ("X.pp3".into(), SidecarKind::Pp3, Describes::Group),
            (
                "NKSC_PARAM\\X.JPG.nksc".into(),
                SidecarKind::Nksc,
                Describes::Primary
            ),
        ]
    );
    let follows: Vec<_> = g.sidecars.iter().map(|s| g.follows(s)).collect();
    assert_eq!(
        follows,
        [
            Describes::Primary,
            Describes::Primary,
            Describes::Raw,
            Describes::Raw,
            Describes::Primary
        ]
    );
}

#[test]
fn us02_bare_sidecars_follow_the_primary_in_a_jpeg_only_group() {
    let out = group([f(&[], "X.JPG"), f(&[], "X.xmp"), f(&[], "X.pp3")], &all());
    let g = &out.groups[0];
    assert!(g.raw.is_none());
    assert!(g.sidecars.iter().all(|s| s.describes == Describes::Group));
    assert!(
        g.sidecars
            .iter()
            .all(|s| g.follows(s) == Describes::Primary)
    );
}

/// INV-09: matching never crosses folders.
#[test]
fn us02_same_name_in_two_folders_is_two_groups() {
    let out = group(
        [
            d(&[], "100NIKON"),
            d(&[], "101NIKON"),
            f(&["100NIKON"], "DSC_0001.JPG"),
            f(&["101NIKON"], "DSC_0001.NEF"),
            f(&["101NIKON"], "DSC_0001.JPG.xmp"),
        ],
        &all(),
    );
    assert_eq!(out.groups.len(), 2);
    assert_eq!(*out.groups[0].folder, ["100NIKON"]);
    assert_eq!(out.groups[0].primary.kind, PhotoKind::Jpeg);
    assert!(out.groups[0].raw.is_none());
    assert_eq!(*out.groups[1].folder, ["101NIKON"]);
    assert!(out.groups[1].is_lone_nef());
    // X.JPG.xmp in a folder without X.JPG is an orphan, never matched across folders.
    assert_eq!(
        unknown(&out),
        [("101NIKON\\DSC_0001.JPG.xmp".into(), "orphan_sidecar")]
    );
    assert!(out.groups.iter().all(|g| g.status == GroupStatus::Ok));
}

#[test]
fn stems_may_contain_dots_and_there_is_no_prefix_matching() {
    let out = group(
        [
            f(&[], "DSC_0001.JPG"),
            f(&[], "DSC_0001.edit.JPG"),
            f(&[], "DSC_0001.edit.xmp"),
        ],
        &all(),
    );
    assert_eq!(out.groups.len(), 2);
    let edit = out
        .groups
        .iter()
        .find(|g| g.stem == "DSC_0001.edit")
        .unwrap();
    assert_eq!(edit.sidecars.len(), 1);
    let plain = out.groups.iter().find(|g| g.stem == "DSC_0001").unwrap();
    assert!(plain.sidecars.is_empty());
}

#[test]
fn a_full_name_match_wins_over_a_stem_match() {
    // X.NEF.JPG is a JPEG with stem "X.NEF"; X.NEF.xmp still describes the file X.NEF (rule 1).
    let out = group(
        [f(&[], "X.NEF"), f(&[], "X.NEF.JPG"), f(&[], "X.NEF.xmp")],
        &all(),
    );
    assert_eq!(out.groups.len(), 2);
    let x = out.groups.iter().find(|g| g.stem_key == "X").unwrap();
    assert_eq!(
        sidecars(x),
        [("X.NEF.xmp".into(), SidecarKind::FileXmp, Describes::Primary)]
    );
    let xnef = out.groups.iter().find(|g| g.stem_key == "X.NEF").unwrap();
    assert!(xnef.sidecars.is_empty());
}

// --- US-03: lone NEF -------------------------------------------------------------------------

#[test]
fn us03_lone_nef_is_its_own_primary() {
    let out = group(
        [
            f(&[], "DSC_0002.NEF"),
            f(&[], "DSC_0002.NEF.xmp"),
            f(&[], "DSC_0002.xmp"),
            f(&["NKSC_PARAM"], "DSC_0002.NEF.nksc"),
        ],
        &all(),
    );
    let g = &out.groups[0];
    assert!(g.is_lone_nef());
    assert_eq!(g.primary.kind, PhotoKind::Nef);
    assert!(g.raw.is_none());
    assert_eq!(g.status, GroupStatus::Ok);
    assert_eq!(g.sidecars.len(), 3);
    // ADR 0023: in a lone-NEF group the NEF is the primary, so everything follows it.
    assert!(
        g.sidecars
            .iter()
            .all(|s| g.follows(s) == Describes::Primary)
    );
}

// --- US-04: unknown files are listed ---------------------------------------------------------

/// INV-20: unknown, orphan and same-basename files are listed, never grouped.
#[test]
fn us04_unknown_files_are_listed_with_their_paths() {
    let out = group(
        [
            f(&[], "DSC_0001.JPG"),
            f(&[], "DSC_0001.WAV"),
            f(&[], "DSC_0001"),
            f(&[], "CLIP_0001.MOV"),
            f(&[], "notes.txt"),
            f(&[], "NC_FLLST.DAT"),
            f(&[], "Y.xmp"),
            f(&[], "DSC_0001.NEF.xmp"),
            f(&[], "DSC_0001.nksc"),
            f(&[], "DSC_0001.JPG.nksc"),
            f(&["NKSC_PARAM"], "DSC_0001.JPG.xmp"),
            f(&["NKSC_PARAM"], "readme.txt"),
            f(&["NKSC_PARAM"], "Y.NEF.nksc"),
        ],
        &all(),
    );
    assert_eq!(out.groups.len(), 1);
    assert!(out.groups[0].sidecars.is_empty());
    assert_eq!(
        unknown(&out),
        [
            ("CLIP_0001.MOV".into(), "unknown_ext"),
            ("DSC_0001".into(), "same_basename_unknown"),
            ("DSC_0001.JPG.nksc".into(), "unknown_ext"),
            ("DSC_0001.NEF.xmp".into(), "orphan_sidecar"),
            ("DSC_0001.nksc".into(), "same_basename_unknown"),
            ("DSC_0001.WAV".into(), "same_basename_unknown"),
            ("NC_FLLST.DAT".into(), "unknown_ext"),
            ("notes.txt".into(), "unknown_ext"),
            ("Y.xmp".into(), "orphan_sidecar"),
            ("NKSC_PARAM\\DSC_0001.JPG.xmp".into(), "unknown_ext"),
            ("NKSC_PARAM\\readme.txt".into(), "unknown_ext"),
            ("NKSC_PARAM\\Y.NEF.nksc".into(), "orphan_sidecar"),
        ]
    );
    let same = out
        .unknown
        .iter()
        .find(|u| u.entry.name == "DSC_0001.WAV")
        .unwrap();
    assert_eq!(
        same.kind,
        UnknownKind::SameBasename {
            stem_key: "DSC_0001".into()
        }
    );
}

/// INV-20.
#[test]
fn photos_inside_nksc_param_are_never_grouped() {
    let out = group(
        [
            f(&["NKSC_PARAM"], "X.JPG"),
            f(&["NKSC_PARAM", "sub"], "Y.NEF"),
            f(&["NKSC_PARAM", "NKSC_PARAM"], "X.JPG.nksc"),
            d(&["NKSC_PARAM"], "sub"),
            d(&["NKSC_PARAM"], "NKSC_PARAM"),
        ],
        &all(),
    );
    assert!(out.groups.is_empty());
    assert!(out.excluded.is_empty());
    assert_eq!(out.unknown.len(), 3);
    assert!(
        out.unknown
            .iter()
            .all(|u| u.kind == UnknownKind::UnknownExt)
    );
    // A nested NKSC_PARAM is an ordinary folder.
    assert_eq!(
        all().dir_action(&d(&["NKSC_PARAM"], "NKSC_PARAM")),
        DirAction::Enter
    );
}

// --- Ambiguity -------------------------------------------------------------------------------

#[test]
fn jpg_and_jpeg_with_one_stem_make_an_ambiguous_group() {
    let out = group(
        [
            f(&[], "X.JPG"),
            f(&[], "X.jpeg"),
            f(&[], "X.NEF"),
            f(&[], "X.JPG.xmp"),
        ],
        &all(),
    );
    assert_eq!(out.groups.len(), 1);
    let g = &out.groups[0];
    assert_eq!(g.status, GroupStatus::Ambiguous);
    // Primary: the first JPEG in folded-name order ("X.JPEG" < "X.JPG").
    assert_eq!(g.primary.entry.name, "X.jpeg");
    assert_eq!(g.raw.as_ref().unwrap().entry.name, "X.NEF");
    assert_eq!(names(g.duplicates.iter().map(|p| &p.entry)), ["X.JPG"]);
    assert_eq!(g.sidecars[0].describes, Describes::Duplicate(0));
    assert_eq!(g.files().count(), 4);
}

#[test]
fn case_variants_in_a_case_sensitive_folder_are_ambiguous() {
    let out = group([f(&[], "X.NEF"), f(&[], "x.nef")], &all());
    assert_eq!(out.groups.len(), 1);
    let g = &out.groups[0];
    assert_eq!(g.status, GroupStatus::Ambiguous);
    assert!(g.is_lone_nef());
    assert!(g.raw.is_none());
    assert_eq!(g.duplicates.len(), 1);
}

#[test]
fn folders_that_fold_to_one_name_stay_apart_and_their_groups_are_ambiguous() {
    let out = group(
        [f(&["a"], "X.JPG"), f(&["A"], "X.NEF"), f(&["A"], "Y.NEF")],
        &all(),
    );
    assert_eq!(out.groups.len(), 3);
    let status: Vec<_> = out
        .groups
        .iter()
        .map(|g| (g.folder[0].as_str(), g.stem.as_str(), g.status))
        .collect();
    assert_eq!(
        status,
        [
            ("A", "X", GroupStatus::Ambiguous),
            ("A", "Y", GroupStatus::Ok),
            ("a", "X", GroupStatus::Ambiguous),
        ]
    );
}

// --- US-05 and file-engine §3.2: exclusions --------------------------------------------------

/// INV-19.
#[test]
fn us05_rejected_folders_are_never_entered_at_any_depth() {
    let p = all();
    assert_eq!(
        p.dir_action(&d(&[], "_rejected")),
        DirAction::Skip(ExclusionReason::RejectedFolder)
    );
    assert_eq!(
        p.dir_action(&d(&["100NIKON"], "_REJECTED")),
        DirAction::Skip(ExclusionReason::RejectedFolder)
    );
    // Even with the files listed, nothing under _rejected is grouped.
    let out = group(
        [
            d(&[], "_rejected"),
            f(&["_rejected"], "X.JPG"),
            f(&["100NIKON", "_Rejected", "deep"], "Y.NEF"),
        ],
        &p,
    );
    assert!(out.groups.is_empty());
    assert!(
        out.excluded
            .iter()
            .all(|x| x.reason == ExclusionReason::RejectedFolder)
    );
    assert_eq!(out.excluded.len(), 3);
    assert_eq!(ExclusionReason::RejectedFolder.issue_kind(true), None);
}

/// INV-19.
#[test]
fn us05_an_in_source_destination_is_never_entered() {
    let p = ScanPolicy {
        destination: Some(vec!["Keep".into(), "2026".into()]),
        ..ScanPolicy::new(true)
    };
    assert_eq!(
        p.dir_action(&d(&["keep"], "2026")),
        DirAction::Skip(ExclusionReason::Destination)
    );
    assert_eq!(p.dir_action(&d(&[], "Keep")), DirAction::Enter);
    let out = group(
        [
            f(&["Keep"], "A.JPG"),
            f(&["Keep", "2026"], "B.JPG"),
            f(&["KEEP", "2026", "sub"], "C.JPG"),
        ],
        &p,
    );
    assert_eq!(out.groups.len(), 1);
    assert_eq!(out.groups[0].stem, "A");
    assert_eq!(
        excluded(&out),
        [
            ("Keep\\2026\\B.JPG".into(), ExclusionReason::Destination),
            (
                "KEEP\\2026\\sub\\C.JPG".into(),
                ExclusionReason::Destination
            ),
        ]
    );
    // The source itself as destination excludes nothing here (preflight P6 refuses it).
    let root = ScanPolicy {
        destination: Some(Vec::new()),
        ..ScanPolicy::new(true)
    };
    assert_eq!(group([f(&[], "A.JPG")], &root).groups.len(), 1);
}

/// INV-10, INV-19.
#[test]
fn us05_reparse_points_and_cloud_placeholders_are_never_followed_or_grouped() {
    let reparse = Attributes {
        reparse_point: true,
        ..Attributes::default()
    };
    let cloud = Attributes {
        cloud_placeholder: true,
        ..Attributes::default()
    };
    let p = all();
    assert_eq!(
        p.dir_action(&with(d(&[], "junction"), reparse)),
        DirAction::Skip(ExclusionReason::ReparsePoint)
    );
    assert_eq!(
        p.dir_action(&with(d(&[], "OneDrive"), cloud)),
        DirAction::Skip(ExclusionReason::CloudPlaceholder)
    );
    // A cloud reparse point is reported as a placeholder.
    let both = Attributes {
        reparse_point: true,
        cloud_placeholder: true,
        ..Attributes::default()
    };
    // Even NKSC_PARAM is not followed through a link.
    assert_eq!(
        p.dir_action(&with(d(&[], "NKSC_PARAM"), reparse)),
        DirAction::Skip(ExclusionReason::ReparsePoint)
    );
    let mut link = with(ScanEntry::new(&[], "X.NEF", EntryKind::Other), reparse);
    link.size = 10;
    let out = group(
        [
            f(&[], "X.JPG"),
            link,
            with(f(&[], "X.xmp"), cloud),
            with(f(&[], "Y.JPG"), both),
            with(f(&["NKSC_PARAM"], "X.JPG.nksc"), reparse),
        ],
        &p,
    );
    assert_eq!(out.groups.len(), 1);
    let g = &out.groups[0];
    assert!(g.raw.is_none() && g.sidecars.is_empty());
    // The NEF behind the link shares the stem: the shot is not complete, so it is held back.
    assert_eq!(g.status, GroupStatus::Ambiguous);
    assert_eq!(
        excluded(&out),
        [
            ("X.NEF".into(), ExclusionReason::ReparsePoint),
            ("X.xmp".into(), ExclusionReason::CloudPlaceholder),
            ("Y.JPG".into(), ExclusionReason::CloudPlaceholder),
            (
                "NKSC_PARAM\\X.JPG.nksc".into(),
                ExclusionReason::ReparsePoint
            ),
        ]
    );
    assert_eq!(
        ExclusionReason::ReparsePoint.issue_kind(false),
        Some("reparse_point")
    );
    assert_eq!(
        ExclusionReason::CloudPlaceholder.issue_kind(true),
        Some("cloud_placeholder")
    );
}

/// INV-19.
#[test]
fn hidden_and_system_entries_are_skipped_but_nksc_param_is_read_even_when_hidden() {
    let hidden = Attributes {
        hidden: true,
        ..Attributes::default()
    };
    let system = Attributes {
        system: true,
        ..Attributes::default()
    };
    let p = all();
    assert_eq!(
        p.dir_action(&with(d(&[], "NKSC_PARAM"), hidden)),
        DirAction::Sidecars
    );
    assert_eq!(
        p.dir_action(&d(&["100NIKON"], "nksc_param")),
        DirAction::Sidecars
    );
    for dir in [
        with(d(&[], "secret"), hidden),
        with(d(&[], "sys"), system),
        d(&[], ".Trashes"),
        d(&[], "System Volume Information"),
        d(&[], "$Recycle.Bin"),
    ] {
        assert_eq!(
            p.dir_action(&dir),
            DirAction::Skip(ExclusionReason::HiddenOrSystem),
            "{}",
            dir.name
        );
    }
    let out = group(
        [
            f(&[], "X.JPG"),
            with(f(&[], "X.NEF"), hidden),
            with(f(&[], "X.xmp"), system),
            f(&[], "._X.JPG"),
            f(&[".Spotlight-V100"], "Z.JPG"),
            f(&["NKSC_PARAM"], "X.JPG.nksc"),
        ],
        &p,
    );
    assert_eq!(out.groups.len(), 1);
    assert_eq!(out.groups[0].sidecars.len(), 1);
    // The hidden X.NEF belongs to the shot: the group is ambiguous, never planned partially.
    assert_eq!(out.groups[0].status, GroupStatus::Ambiguous);
    assert_eq!(out.excluded.len(), 4);
    assert!(
        out.excluded
            .iter()
            .all(|x| x.reason == ExclusionReason::HiddenOrSystem)
    );
    assert_eq!(ExclusionReason::HiddenOrSystem.issue_kind(true), None);
    assert_eq!(
        ExclusionReason::HiddenOrSystem.issue_kind(false),
        Some("hidden_or_system")
    );
}

/// INV-20: foreign partials are listed, never grouped.
#[test]
fn culla_partials_are_never_grouped() {
    let out = group(
        [
            f(&[], "X.JPG"),
            f(&[], "X.NEF.12-1.culla-partial"),
            f(&[], "X.JPG.3-2.CULLA-PARTIAL"),
        ],
        &all(),
    );
    assert_eq!(out.groups.len(), 1);
    assert!(out.groups[0].raw.is_none());
    assert_eq!(out.excluded.len(), 2);
    assert!(
        out.excluded
            .iter()
            .all(|x| x.reason == ExclusionReason::CullaPartial)
    );
    assert_eq!(
        ExclusionReason::CullaPartial.issue_kind(false),
        Some("foreign_partial")
    );
}

/// INV-20: invalid names are listed, never grouped.
#[test]
fn invalid_names_are_skipped() {
    let mut lossy = f(&[], "X\u{FFFD}.JPG");
    lossy.name_lossy = true;
    let out = group(
        [
            f(&[], "CON.JPG"),
            f(&[], "X.JPG."),
            f(&[], "X.JPG "),
            f(&[], "a:b.JPG"),
            lossy,
            f(&["bad."], "Y.JPG"),
            d(&[], "AUX"),
        ],
        &all(),
    );
    assert!(out.groups.is_empty());
    assert_eq!(out.excluded.len(), 7);
    assert!(
        out.excluded
            .iter()
            .all(|x| x.reason == ExclusionReason::InvalidName)
    );
    assert_eq!(
        all().dir_action(&d(&[], "NUL")),
        DirAction::Skip(ExclusionReason::InvalidName)
    );
}

#[test]
fn unreadable_and_special_entries_are_excluded() {
    let out = group(
        [
            ScanEntry::new(&[], "X.JPG", EntryKind::Unreadable("denied".into())),
            ScanEntry::new(&[], "fifo", EntryKind::Other),
            ScanEntry::new(&[], "lost", EntryKind::Unreadable("gone".into())),
        ],
        &all(),
    );
    assert!(out.groups.is_empty());
    assert_eq!(
        excluded(&out),
        [
            ("fifo".into(), ExclusionReason::NotAFile),
            ("lost".into(), ExclusionReason::Unreadable),
            ("X.JPG".into(), ExclusionReason::Unreadable),
        ]
    );
    assert_eq!(
        ExclusionReason::NotAFile.issue_kind(false),
        Some("unreadable")
    );
}

#[test]
fn subfolders_off_reads_only_the_root_and_its_nksc_param() {
    let p = ScanPolicy::default();
    assert!(!p.include_subfolders);
    assert_eq!(
        p.dir_action(&d(&[], "100NIKON")),
        DirAction::Skip(ExclusionReason::NotIncluded)
    );
    assert_eq!(p.dir_action(&d(&[], "NKSC_PARAM")), DirAction::Sidecars);
    assert_eq!(
        p.dir_action(&d(&["NKSC_PARAM"], "sub")),
        DirAction::Skip(ExclusionReason::NotIncluded)
    );
    let out = group(
        [
            f(&[], "X.JPG"),
            f(&["NKSC_PARAM"], "X.JPG.nksc"),
            f(&["100NIKON"], "Y.JPG"),
            d(&[], "100NIKON"),
        ],
        &p,
    );
    assert_eq!(out.groups.len(), 1);
    assert_eq!(out.groups[0].sidecars.len(), 1);
    assert_eq!(
        excluded(&out),
        [
            ("100NIKON".into(), ExclusionReason::NotIncluded),
            ("100NIKON\\Y.JPG".into(), ExclusionReason::NotIncluded),
        ]
    );
    assert_eq!(ExclusionReason::NotIncluded.issue_kind(true), None);
}

#[test]
fn entered_folders_are_not_listed() {
    let out = group([d(&[], "100NIKON"), d(&[], "NKSC_PARAM")], &all());
    assert_eq!(out, Grouping::default());
}

#[test]
fn camera_protected_files_are_grouped_with_their_attribute() {
    let ro = Attributes {
        read_only: true,
        ..Attributes::default()
    };
    let mut jpg = with(f(&[], "X.JPG"), ro);
    jpg.size = 1234;
    jpg.mtime_ns = Some(-5);
    let out = group([jpg.clone(), f(&[], "X.NEF")], &all());
    let g = &out.groups[0];
    assert_eq!(g.primary.entry, jpg);
    assert!(g.primary.entry.attrs.read_only);
    assert!(!g.raw.as_ref().unwrap().entry.attrs.read_only);
}

#[test]
fn output_is_in_folder_then_stem_order() {
    let out = group(
        [
            f(&["b"], "A.JPG"),
            f(&[], "b.JPG"),
            f(&["a", "z"], "A.JPG"),
            f(&[], "A.JPG"),
            f(&["a"], "C.JPG"),
            f(&["B"], "A.JPG"),
            f(&["a"], "b.nef"),
        ],
        &all(),
    );
    let order: Vec<_> = out
        .groups
        .iter()
        .map(|g| format!("{}|{}", g.folder.join("\\"), g.stem))
        .collect();
    assert_eq!(order, ["|A", "|b", "a|b", "a|C", "a\\z|A", "B|A", "b|A"]);
}

#[test]
fn rel_path_and_keys() {
    let e = f(&["100NIKON", "été"], "dsc_0001.jpg");
    assert_eq!(e.rel_path(), "100NIKON\\été\\dsc_0001.jpg");
    let out = group([e], &all());
    assert_eq!(out.groups[0].folder_key(), "100NIKON\\ÉTÉ");
    assert_eq!(out.groups[0].stem_key, "DSC_0001");
    assert_eq!(SidecarKind::FileXmp.as_str(), "file_xmp");
    assert_eq!(SidecarKind::Nksc.as_str(), "nksc");
}

// --- Property-style tests --------------------------------------------------------------------

/// xorshift64*: small, seeded, deterministic.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    fn shuffle<T>(&mut self, xs: &mut [T]) {
        for i in (1..xs.len()).rev() {
            let j = self.below(i + 1);
            xs.swap(i, j);
        }
    }
}

const CASES: u64 = 500;

const FOLDERS: &[&[&str]] = &[
    &[],
    &["100NIKON"],
    &["101NIKON"],
    &["100nikon"],
    &["100NIKON", "sub"],
    &["_rejected"],
    &["100NIKON", "_Rejected"],
    &["Keep"],
    &["Keep", "x"],
    &[".hidden"],
    &["été"],
    &["NKSC_PARAM", "deep"],
];
const STEMS: &[&str] = &[
    "DSC_0001", "dsc_0001", "DSC_0002", "X.edit", "_DSC0003", "Été", "ÉTÉ", "CON", "a.",
];
const EXTS: &[&str] = &[
    "JPG",
    "jpg",
    "JPEG",
    "jpeg",
    "NEF",
    "nef",
    "xmp",
    "XMP",
    "pp3",
    "NEF.xmp",
    "JPG.xmp",
    "nef.PP3",
    "jpg.xmp",
    "MOV",
    "txt",
    "WAV",
    "12-1.culla-partial",
    "nksc",
    "NEF.nksc",
];
const NKSC_EXTS: &[&str] = &["NEF.nksc", "JPG.nksc", "nef.NKSC", "nksc", "txt", "JPG"];

/// A random listing: files in random folders (some in the folder's NKSC_PARAM), some folders,
/// random attributes and kinds. No two entries share a folder and name, as in a real listing.
fn gen_tree(rng: &mut Rng) -> Vec<ScanEntry> {
    let mut out: Vec<ScanEntry> = Vec::new();
    let n = rng.below(40);
    for _ in 0..n {
        // Mostly ordinary folders and a few stems, so groups get several members.
        let folder = if rng.chance(70) {
            &FOLDERS[..3]
        } else {
            FOLDERS
        };
        let mut folder: Vec<String> = rng.pick(folder).iter().map(|s| (*s).to_owned()).collect();
        let stem = if rng.chance(70) {
            STEMS[rng.below(3)]
        } else {
            *rng.pick(STEMS)
        };
        let ext = if rng.chance(15) {
            folder.push(
                if rng.chance(50) {
                    "NKSC_PARAM"
                } else {
                    "nksc_param"
                }
                .to_owned(),
            );
            *rng.pick(NKSC_EXTS)
        } else {
            *rng.pick(EXTS)
        };
        let mut e = ScanEntry {
            folder: folder.into(),
            name: format!("{stem}.{ext}"),
            name_lossy: rng.chance(1),
            kind: EntryKind::File,
            size: rng.next() % 1000,
            mtime_ns: rng.chance(80).then(|| (rng.next() % 1_000_000) as i64),
            attrs: Attributes {
                read_only: rng.chance(10),
                hidden: rng.chance(2),
                system: rng.chance(1),
                reparse_point: rng.chance(2),
                cloud_placeholder: rng.chance(2),
            },
        };
        if rng.chance(1) {
            e.kind = EntryKind::Other;
        } else if rng.chance(1) {
            e.kind = EntryKind::Unreadable("error".into());
        }
        out.push(e);
    }
    for _ in 0..rng.below(6) {
        let folder: Vec<&str> = rng.pick(FOLDERS).to_vec();
        let name = *rng.pick(&["NKSC_PARAM", "_rejected", "sub", "Keep", ".git", "AUX"]);
        let mut e = ScanEntry::dir(&folder, name);
        e.attrs.hidden = rng.chance(20);
        e.attrs.reparse_point = rng.chance(20);
        if rng.chance(10) {
            e.kind = EntryKind::Unlisted {
                reason: *rng.pick(&[ExclusionReason::Unreadable, ExclusionReason::TooLarge]),
                detail: String::new(),
            };
        }
        out.push(e);
    }
    let mut seen = BTreeSet::new();
    out.retain(|e| seen.insert((e.folder.clone(), e.name.clone())));
    out
}

fn gen_policy(rng: &mut Rng) -> ScanPolicy {
    ScanPolicy {
        include_subfolders: rng.chance(80),
        destination: rng.chance(30).then(|| vec!["keep".to_owned()]),
        max_depth: if rng.chance(10) { 1 } else { DEFAULT_MAX_DEPTH },
        max_entries_per_folder: if rng.chance(10) {
            8
        } else {
            DEFAULT_MAX_ENTRIES_PER_FOLDER
        },
    }
}

fn sorted(mut v: Vec<ScanEntry>) -> Vec<ScanEntry> {
    v.sort_by(|a, b| (&a.folder, &a.name).cmp(&(&b.folder, &b.name)));
    v
}

/// The owner folder of a grouped file: its folder, minus a trailing NKSC_PARAM.
fn owner(e: &ScanEntry) -> Vec<String> {
    match e.folder.split_last() {
        Some((last, parent)) if eq_folded(last, NKSC_DIR) => parent.to_vec(),
        _ => e.folder.to_vec(),
    }
}

/// INV-20: every entry is accounted for, so the review can list it.
#[test]
fn prop_every_entry_lands_in_exactly_one_place() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for case in 0..CASES {
        let input = gen_tree(&mut rng);
        let policy = gen_policy(&mut rng);
        let out = group(input.clone(), &policy);

        let mut files_in: Vec<ScanEntry> = input
            .iter()
            .filter(|e| e.kind != EntryKind::Dir)
            .cloned()
            .collect();
        let mut files_out: Vec<ScanEntry> =
            out.groups.iter().flat_map(Group::files).cloned().collect();
        files_out.extend(out.unknown.iter().map(|u| u.entry.clone()));
        files_out.extend(
            out.excluded
                .iter()
                .filter(|x| x.entry.kind != EntryKind::Dir)
                .map(|x| x.entry.clone()),
        );
        files_in = sorted(files_in);
        assert_eq!(files_in, sorted(files_out), "case {case}");

        for dir in input.iter().filter(|e| e.kind == EntryKind::Dir) {
            let listed = out.excluded.iter().filter(|x| &x.entry == dir).count();
            let skipped = matches!(policy.dir_action(dir), DirAction::Skip(_));
            assert_eq!(listed, usize::from(skipped), "case {case}: {dir:?}");
        }
    }
}

/// INV-04, INV-09, INV-10, INV-19.
#[test]
fn prop_groups_hold_only_safe_matching_files() {
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    let (mut n_groups, mut n_sidecars, mut n_ambiguous, mut n_raw) = (0, 0, 0, 0);
    for case in 0..CASES {
        let input = gen_tree(&mut rng);
        let policy = gen_policy(&mut rng);
        let out = group(input.clone(), &policy);
        let mut identities = BTreeSet::new();
        n_groups += out.groups.len();
        for g in &out.groups {
            n_sidecars += g.sidecars.len();
            n_ambiguous += usize::from(g.status == GroupStatus::Ambiguous);
            n_raw += usize::from(g.raw.is_some());
            assert!(
                identities.insert((g.folder.clone(), g.stem_key.clone())),
                "case {case}"
            );
            for e in g.files() {
                // INV-10, INV-19: never a reparse point, placeholder, hidden or excluded entry.
                assert_eq!(e.kind, EntryKind::File, "case {case}");
                assert!(
                    !e.attrs.reparse_point && !e.attrs.cloud_placeholder,
                    "case {case}"
                );
                assert!(
                    !e.attrs.hidden && !e.attrs.system && !e.name_lossy,
                    "case {case}"
                );
                assert!(
                    policy.path_exclusion(&e.folder).is_none(),
                    "case {case}: {e:?}"
                );
                assert!(
                    !e.folder.iter().any(|c| eq_folded(c, REJECTED_DIR)),
                    "case {case}"
                );
                assert_eq!(
                    owner(e),
                    g.folder.to_vec(),
                    "case {case}: within one folder only"
                );
            }
            let photos: Vec<&Photo> = std::iter::once(&g.primary)
                .chain(&g.raw)
                .chain(&g.duplicates)
                .collect();
            for p in &photos {
                assert_eq!(p.entry.folder, g.folder, "case {case}");
                let (stem, ext) = split_ext(&p.entry.name);
                assert_eq!(fold(stem), g.stem_key, "case {case}");
                assert_eq!(
                    ext.and_then(PhotoKind::from_ext),
                    Some(p.kind),
                    "case {case}"
                );
            }
            let jpegs = photos.iter().filter(|p| p.kind == PhotoKind::Jpeg).count();
            let nefs = photos.len() - jpegs;
            assert_eq!(g.primary.kind == PhotoKind::Jpeg, jpegs > 0, "case {case}");
            if let Some(r) = &g.raw {
                assert_eq!(r.kind, PhotoKind::Nef, "case {case}");
                assert_eq!(g.primary.kind, PhotoKind::Jpeg, "case {case}");
            } else {
                assert!(
                    jpegs == 0 || nefs == 0,
                    "case {case}: a NEF next to a JPEG is the raw"
                );
            }
            if jpegs > 1 || nefs > 1 {
                assert_eq!(g.status, GroupStatus::Ambiguous, "case {case}");
            }
            if g.status == GroupStatus::Ok {
                // Never split a shot: an `Ok` group holds every photo file of its stem in its
                // folder, no two of its files fold alike, and its folder was read completely.
                assert!(g.duplicates.is_empty(), "case {case}");
                let mut folded = BTreeSet::new();
                assert!(
                    g.files().all(|e| folded.insert(fold(&e.rel_path()))),
                    "case {case}"
                );
                for e in &input {
                    if *e.folder != *g.folder {
                        continue;
                    }
                    assert!(!matches!(e.kind, EntryKind::Unreadable(_)), "case {case}");
                    if eq_folded(&e.name, NKSC_DIR) {
                        assert_eq!(e.kind, EntryKind::Dir, "case {case}");
                        assert!(!e.attrs.reparse_point && !e.attrs.cloud_placeholder);
                    }
                    let (stem, ext) = split_ext(&e.name);
                    if ext.and_then(PhotoKind::from_ext).is_some() && fold(stem) == g.stem_key {
                        assert!(photos.iter().any(|p| p.entry == *e), "case {case}: {e:?}");
                    }
                }
            }
            for s in &g.sidecars {
                let (base, _) = split_ext(&s.entry.name);
                let in_nksc = s.entry.folder.len() == g.folder.len() + 1;
                assert_eq!(in_nksc, s.kind == SidecarKind::Nksc, "case {case}");
                let target = match s.describes {
                    Describes::Group => {
                        assert!(matches!(s.kind, SidecarKind::Xmp | SidecarKind::Pp3));
                        assert_eq!(fold(base), g.stem_key, "case {case}");
                        assert!(
                            !photos.iter().any(|p| eq_folded(&p.entry.name, base)),
                            "case {case}: a full-name match wins"
                        );
                        assert_ne!(g.follows(s), Describes::Group);
                        continue;
                    }
                    Describes::Primary => &g.primary,
                    Describes::Raw => g.raw.as_ref().unwrap(),
                    Describes::Duplicate(i) => &g.duplicates[i],
                };
                assert!(eq_folded(&target.entry.name, base), "case {case}: {s:?}");
                assert_eq!(g.follows(s), s.describes, "case {case}");
            }
        }
    }
    // The generator must reach every shape, or the properties above prove little.
    println!("{n_groups} groups, {n_sidecars} sidecars, {n_ambiguous} ambiguous, {n_raw} raw");
    assert!(n_groups > 1000 && n_sidecars > 300 && n_ambiguous > 100 && n_raw > 100);
}

#[test]
fn prop_output_does_not_depend_on_input_order() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    for case in 0..CASES {
        let input = gen_tree(&mut rng);
        let policy = gen_policy(&mut rng);
        let expected = group(input.clone(), &policy);
        for _ in 0..3 {
            let mut shuffled = input.clone();
            rng.shuffle(&mut shuffled);
            assert_eq!(group(shuffled, &policy), expected, "case {case}");
        }
    }
}

#[test]
fn prop_grouping_folder_batches_matches_grouping_the_tree() {
    let mut rng = Rng(0x0123_4567_89AB_CDEF);
    for case in 0..CASES {
        let input = gen_tree(&mut rng);
        let policy = gen_policy(&mut rng);
        let whole = group(input.clone(), &policy);

        // One batch per owner folder, as the adapter emits them.
        let mut batches: BTreeMap<Vec<String>, Vec<ScanEntry>> = BTreeMap::new();
        for e in input {
            batches.entry(owner(&e)).or_default().push(e);
        }
        let merged = Grouping::merge(batches.into_values().map(|b| group(b, &policy)));
        assert_eq!(merged, whole, "case {case}");
    }
}

/// A group seen without case: folder, primary, sidecars and what they follow, duplicates.
type Shape = (String, String, Vec<(String, Describes)>, usize);

#[test]
fn prop_matching_ignores_case() {
    // Trees with no two names that fold alike: changing the case of every name must not
    // change which files group together, nor the roles and sidecar kinds.
    let mut rng = Rng(0xA076_1D64_78BD_642F);
    let stems = ["DSC_0001", "DSC_0002", "X.edit", "été"];
    let exts = [
        "JPG", "JPEG", "NEF", "xmp", "pp3", "NEF.xmp", "JPG.pp3", "MOV",
    ];
    let shape = |g: &Grouping| -> Vec<Shape> {
        g.groups
            .iter()
            .map(|g| {
                let mut s: Vec<_> = g
                    .sidecars
                    .iter()
                    .map(|s| (fold(&s.entry.name), g.follows(s)))
                    .collect();
                s.sort_by(|a, b| a.0.cmp(&b.0));
                (
                    fold(&g.folder.join("\\")),
                    fold(&g.primary.entry.name),
                    s,
                    g.duplicates.len(),
                )
            })
            .collect()
    };
    let recase = |rng: &mut Rng, s: &str| -> String {
        s.chars()
            .map(|c| {
                if rng.chance(50) {
                    c.to_uppercase().next().unwrap_or(c)
                } else {
                    c.to_lowercase().next().unwrap_or(c)
                }
            })
            .collect()
    };
    for case in 0..CASES {
        let mut seen = BTreeSet::new();
        let mut base = Vec::new();
        for _ in 0..rng.below(16) {
            let folder = if rng.chance(50) { "100NIKON" } else { "" };
            let name = format!("{}.{}", rng.pick(&stems), rng.pick(&exts));
            if seen.insert((folder, fold(&name))) {
                base.push((folder, name));
            }
        }
        let upper: Vec<ScanEntry> = base
            .iter()
            .map(|(dir, n)| {
                let folder: Vec<&str> = if dir.is_empty() { vec![] } else { vec![*dir] };
                f(&folder, &fold(n))
            })
            .collect();
        // One spelling per folder, as a real (case-insensitive) folder has.
        let sub = recase(&mut rng, "100NIKON");
        let mixed: Vec<ScanEntry> = base
            .iter()
            .map(|(dir, n)| {
                let folder: Vec<&str> = if dir.is_empty() {
                    vec![]
                } else {
                    vec![sub.as_str()]
                };
                f(&folder, &recase(&mut rng, n))
            })
            .collect();
        let a = group(upper, &all());
        let b = group(mixed, &all());
        assert_eq!(shape(&a), shape(&b), "case {case}");
        assert_eq!(a.unknown.len(), b.unknown.len(), "case {case}");
    }
}

// --- Never split a shot (ADR 0027) --------------------------------------------------

fn held_back(g: &Grouping) -> Vec<String> {
    g.excluded
        .iter()
        .filter(|x| x.reason == ExclusionReason::HeldBack)
        .map(|x| x.entry.rel_path())
        .collect()
}

/// INV-04: a folder with an entry that could not be described groups nothing.
#[test]
fn an_unreadable_entry_holds_back_its_whole_folder() {
    let out = group(
        [
            f(&[], "X.JPG"),
            f(&[], "X.NEF"),
            f(&[], "notes.txt"),
            f(&["NKSC_PARAM"], "X.NEF.nksc"),
            ScanEntry::new(&[], "X.xmp", EntryKind::Unreadable("denied".into())),
            f(&["100NIKON"], "Y.JPG"),
        ],
        &all(),
    );
    assert_eq!(out.groups.len(), 1);
    assert_eq!(*out.groups[0].folder, ["100NIKON"]);
    assert!(out.unknown.is_empty());
    assert_eq!(
        held_back(&out),
        ["notes.txt", "X.JPG", "X.NEF", "NKSC_PARAM\\X.NEF.nksc"]
    );
    assert_eq!(
        ExclusionReason::HeldBack.issue_kind(false),
        Some("held_back")
    );
}

/// INV-04, INV-09, INV-10: an `NKSC_PARAM` that cannot be read through holds back its parent.
#[test]
fn an_unreadable_or_linked_nksc_param_holds_back_its_parent() {
    let reparse = Attributes {
        reparse_point: true,
        ..Attributes::default()
    };
    let cloud = Attributes {
        cloud_placeholder: true,
        ..Attributes::default()
    };
    let unlisted = ScanEntry::new(
        &[],
        "NKSC_PARAM",
        EntryKind::Unlisted {
            reason: ExclusionReason::Unreadable,
            detail: "denied".into(),
        },
    );
    for broken in [
        with(d(&[], "NKSC_PARAM"), reparse),
        with(d(&[], "nksc_param"), cloud),
        with(ScanEntry::new(&[], "NKSC_PARAM", EntryKind::Other), reparse),
        with(f(&[], "NKSC_PARAM"), reparse),
        ScanEntry::new(&[], "NKSC_PARAM", EntryKind::Unreadable("gone".into())),
        unlisted,
    ] {
        let out = group(
            [
                f(&[], "X.JPG"),
                f(&[], "X.NEF"),
                broken.clone(),
                f(&["100NIKON"], "Y.JPG"),
            ],
            &all(),
        );
        assert_eq!(out.groups.len(), 1, "{broken:?}");
        assert_eq!(*out.groups[0].folder, ["100NIKON"]);
        assert_eq!(held_back(&out), ["X.JPG", "X.NEF"], "{broken:?}");
        assert!(
            out.excluded.iter().any(|x| x.entry == broken),
            "the cause is listed"
        );
    }
    // A plain file named NKSC_PARAM is just an unknown file.
    let out = group([f(&[], "X.JPG"), f(&[], "NKSC_PARAM")], &all());
    assert_eq!(out.groups[0].status, GroupStatus::Ok);
    assert_eq!(unknown(&out), [("NKSC_PARAM".into(), "unknown_ext")]);
    // An NKSC_PARAM inside an excluded folder changes nothing outside it.
    let out = group(
        [
            f(&[], "X.JPG"),
            with(d(&["_rejected"], "NKSC_PARAM"), reparse),
        ],
        &all(),
    );
    assert_eq!(out.groups[0].status, GroupStatus::Ok);
}

/// INV-04, INV-06: two members of a group whose paths fold alike make it ambiguous.
#[test]
fn case_variant_members_make_the_group_ambiguous() {
    let cases: [&[ScanEntry]; 3] = [
        &[f(&[], "X.JPG"), f(&[], "X.xmp"), f(&[], "x.XMP")],
        &[
            f(&[], "X.JPG"),
            f(&[], "X.NEF"),
            f(&[], "X.NEF.xmp"),
            f(&[], "x.nef.xmp"),
        ],
        &[
            f(&[], "X.JPG"),
            f(&["NKSC_PARAM"], "X.JPG.nksc"),
            f(&["nksc_param"], "X.JPG.nksc"),
        ],
    ];
    for entries in cases {
        let out = group(entries.to_vec(), &all());
        assert_eq!(out.groups.len(), 1);
        let g = &out.groups[0];
        assert_eq!(
            g.files().count(),
            entries.len(),
            "every file stays in the group"
        );
        assert_eq!(g.status, GroupStatus::Ambiguous, "{entries:?}");
    }
    // Different files in two NKSC_PARAM spellings do not collide.
    let out = group(
        [
            f(&[], "X.JPG"),
            f(&[], "X.NEF"),
            f(&["NKSC_PARAM"], "X.JPG.nksc"),
            f(&["nksc_param"], "X.NEF.nksc"),
        ],
        &all(),
    );
    assert_eq!(out.groups[0].status, GroupStatus::Ok);
}

#[test]
fn merge_checks_member_collisions_again() {
    let mut part = group([f(&[], "X.JPG"), f(&[], "X.xmp")], &all());
    assert_eq!(part.groups[0].status, GroupStatus::Ok);
    let mut extra = part.groups[0].sidecars[0].clone();
    extra.entry.name = "x.XMP".into();
    part.groups[0].sidecars.push(extra);
    let merged = Grouping::merge([part]);
    assert_eq!(merged.groups[0].status, GroupStatus::Ambiguous);
}

/// INV-04, INV-10, INV-19: a JPEG or NEF excluded for its own attributes keeps its shot out of
/// any plan.
#[test]
fn an_excluded_photo_with_the_same_stem_makes_the_group_ambiguous() {
    let attrs = [
        Attributes {
            hidden: true,
            ..Attributes::default()
        },
        Attributes {
            system: true,
            ..Attributes::default()
        },
        Attributes {
            cloud_placeholder: true,
            ..Attributes::default()
        },
        Attributes {
            reparse_point: true,
            ..Attributes::default()
        },
    ];
    for a in attrs {
        for name in ["x.nef", "X.jpeg"] {
            let out = group([f(&[], "X.JPG"), with(f(&[], name), a)], &all());
            assert_eq!(out.groups.len(), 1);
            assert_eq!(out.groups[0].status, GroupStatus::Ambiguous, "{name} {a:?}");
        }
    }
    let mut lossy = f(&[], "X.NEF");
    lossy.name_lossy = true;
    let other = ScanEntry::new(&[], "X.NEF", EntryKind::Other);
    for e in [lossy, other] {
        let out = group([f(&[], "X.JPG"), e], &all());
        assert_eq!(out.groups[0].status, GroupStatus::Ambiguous);
    }
    // Not for another stem, another folder, a sidecar, or a photo inside NKSC_PARAM.
    let hidden = Attributes {
        hidden: true,
        ..Attributes::default()
    };
    let out = group(
        [
            f(&[], "X.JPG"),
            with(f(&[], "Y.NEF"), hidden),
            with(f(&["100NIKON"], "X.NEF"), hidden),
            with(f(&[], "X.xmp"), hidden),
            with(f(&["NKSC_PARAM"], "X.NEF"), hidden),
        ],
        &all(),
    );
    assert_eq!(out.groups[0].status, GroupStatus::Ok);
}

/// Threat model T19: folders deeper than the cap are not entered.
#[test]
fn folders_deeper_than_the_cap_are_not_entered() {
    let p = ScanPolicy {
        max_depth: 2,
        ..ScanPolicy::new(true)
    };
    assert_eq!(p.dir_action(&d(&["a"], "b")), DirAction::Enter);
    assert_eq!(
        p.dir_action(&d(&["a", "b"], "c")),
        DirAction::Skip(ExclusionReason::TooDeep)
    );
    // NKSC_PARAM is part of its parent, so it is read at the cap.
    assert_eq!(
        p.dir_action(&d(&["a", "b"], "NKSC_PARAM")),
        DirAction::Sidecars
    );
    let out = group(
        [
            f(&["a", "b"], "X.JPG"),
            f(&["a", "b", "NKSC_PARAM"], "X.JPG.nksc"),
            f(&["a", "b", "c"], "Y.JPG"),
            d(&["a", "b"], "c"),
        ],
        &p,
    );
    assert_eq!(out.groups.len(), 1);
    assert_eq!(out.groups[0].sidecars.len(), 1);
    assert_eq!(
        excluded(&out),
        [
            ("a\\b\\c".into(), ExclusionReason::TooDeep),
            ("a\\b\\c\\Y.JPG".into(), ExclusionReason::TooDeep),
        ]
    );
    assert_eq!(ExclusionReason::TooDeep.issue_kind(true), Some("too_deep"));
}

/// Threat model T19: a folder over the entry cap groups nothing.
#[test]
fn a_folder_over_the_entry_cap_groups_nothing() {
    let p = ScanPolicy {
        max_entries_per_folder: 4,
        ..ScanPolicy::new(true)
    };
    let four = [
        f(&[], "X.JPG"),
        f(&[], "X.NEF"),
        f(&["NKSC_PARAM"], "X.NEF.nksc"),
        d(&[], "NKSC_PARAM"),
    ];
    assert_eq!(group(four.clone(), &p).groups.len(), 1);
    let mut five = four.to_vec();
    five.push(f(&[], "notes.txt"));
    five.push(f(&["sub"], "Y.JPG"));
    let out = group(five, &p);
    assert_eq!(groups_of(&out), ["sub|Y"]);
    assert!(out.unknown.is_empty());
    let too_large = out
        .excluded
        .iter()
        .filter(|x| x.reason == ExclusionReason::TooLarge)
        .count();
    assert_eq!(too_large, 4);
    assert_eq!(
        ExclusionReason::TooLarge.issue_kind(false),
        Some("too_large")
    );
}

fn groups_of(g: &Grouping) -> Vec<String> {
    g.groups
        .iter()
        .map(|g| format!("{}|{}", g.folder.join("\\"), g.stem))
        .collect()
}

#[test]
fn default_caps_are_generous() {
    let p = ScanPolicy::default();
    assert!(!p.include_subfolders && p.destination.is_none());
    assert_eq!(p.max_depth, 32);
    assert_eq!(p.max_entries_per_folder, 100_000);
    assert_eq!(ScanPolicy::new(true).max_depth, DEFAULT_MAX_DEPTH);
}

#[test]
fn entries_of_a_folder_share_its_components() {
    let folder: Arc<[String]> = vec!["100NIKON".to_owned()].into();
    let a = ScanEntry::in_folder(Arc::clone(&folder), "X.JPG", EntryKind::File);
    let b = ScanEntry::in_folder(Arc::clone(&folder), "X.NEF", EntryKind::File);
    assert!(Arc::ptr_eq(&a.folder, &b.folder));
    let out = group([a, b], &all());
    assert!(Arc::ptr_eq(
        &out.groups[0].primary.entry.folder,
        &out.groups[0].raw.as_ref().unwrap().entry.folder
    ));
}
