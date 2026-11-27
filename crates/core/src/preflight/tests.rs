//! Preflight tests: one or more per rule of design/file-engine.md §8 (US-16), then a seeded
//! property run: a clean snapshot gives an empty report, and one changed fact gives exactly the
//! finding of its rule.

use super::*;
use crate::decisions::{Decisions, Verdict};
use crate::grouping::{ScanEntry, ScanPolicy, group};
use crate::plan::{KeepMode, KeepTo, PlanGroup, Settings, plan};
use crate::volume::{DriveKind, VolumeFlags};

const GUID_CARD: &str = "\\\\?\\Volume{0a1b2c3d-1111-2222-3333-444455556666}\\";
const GUID_DISK: &str = "\\\\?\\Volume{9f8e7d6c-aaaa-bbbb-cccc-ddddeeeeffff}\\";
const GUID_OTHER: &str = "\\\\?\\Volume{12345678-aaaa-bbbb-cccc-ddddeeeeffff}\\";
const CARD: u32 = 0x1A2B_3C4D;
const DISK: u32 = 0x5E6F_7081;
const MTIME: i64 = 1_700_000_000_000_000_000;
const HOUR: i64 = 3_600_000_000_000;

fn vol(serial: u32, guid: &str, mount: &str, fs: &str) -> VolumeInfo {
    VolumeInfo {
        id: VolumeId::new(serial),
        guid_path: VolumeGuidPath::parse(guid).unwrap(),
        mount_point: mount.to_owned(),
        label: None,
        fs_name: fs.to_owned(),
        flags: VolumeFlags(0),
        max_component_len: 255,
        drive_kind: DriveKind::Removable,
    }
}

fn card() -> VolumeInfo {
    vol(CARD, GUID_CARD, "E:\\", "exFAT")
}

fn disk() -> VolumeInfo {
    vol(DISK, GUID_DISK, "C:\\", "NTFS")
}

fn parts(p: &str) -> Vec<String> {
    p.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Groups for `paths` (from the source root), ids 1, 2, … in grouping order.
fn scan(paths: &[&str], read_only: &[&str]) -> Vec<PlanGroup> {
    let entries = paths.iter().map(|p| {
        let ps: Vec<&str> = p.split('/').collect();
        let (name, folder) = ps.split_last().unwrap();
        let mut e = ScanEntry::file(folder, name);
        e.size = 1000 + name.len() as u64;
        e.mtime_ns = Some(MTIME);
        e.attrs.read_only = read_only.contains(p);
        e
    });
    group(entries, &ScanPolicy::new(true))
        .groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let id = i as GroupId + 1;
            PlanGroup::from_group(id, g, 100 * id)
        })
        .collect()
}

fn decide(groups: &[PlanGroup], verdicts: &[Verdict]) -> Decisions {
    let mut d = Decisions::new();
    for (i, (g, v)) in groups.iter().zip(verdicts.iter().cycle()).enumerate() {
        let e = d.decide(g.id, *v, &|_| false).unwrap().at(i as i64 + 1, 0);
        d.apply(&e).unwrap();
    }
    d
}

fn settings(mode: KeepMode, same_volume: bool) -> Settings {
    Settings {
        raw_follows: true,
        keep: Some(KeepTo { mode, same_volume }),
    }
}

/// Source `E:\DCIM`, destination `C:\Photos`.
fn roots() -> Roots {
    Roots {
        source: Source::new(VolumeId::new(CARD), parts("DCIM")).unwrap(),
        dest: Some(Source::new(VolumeId::new(DISK), parts("Photos")).unwrap()),
    }
}

/// Plain folders above the roots.
fn add_ancestors(f: &mut Facts, roots: &Roots) {
    for r in std::iter::once(&roots.source).chain(roots.dest.iter()) {
        let c = r.components();
        for n in 1..c.len() {
            f.add_folder(Tree::Volume(r.volume()), &c[..n], Attributes::default());
        }
    }
}

/// The facts of a disk that matches the plan exactly, for [`roots`].
fn clean(p: &Plan, groups: &[PlanGroup], mounted: Vec<VolumeInfo>) -> Facts {
    clean_at(p, groups, mounted, &roots())
}

/// The facts of a disk that matches the plan exactly: every source as scanned with its folders
/// plain, the folders above the roots plain, plenty of space, the destination root a folder
/// outside any sync root, the targets free, the preview pipeline idle.
fn clean_at(p: &Plan, groups: &[PlanGroup], mounted: Vec<VolumeInfo>, roots: &Roots) -> Facts {
    let mut f = Facts::new(
        FolderFact::Exists(Attributes::default()),
        DestRoot::Folder(Attributes::default()),
    );
    f.preview_idle = true;
    f.dest_in_sync_root = Some(false);
    add_ancestors(&mut f, roots);
    for v in &mounted {
        f.space.insert(
            v.guid_path.clone(),
            Space {
                free: 1 << 40,
                cluster: 4096,
            },
        );
    }
    f.mounted = mounted;
    for o in &p.ops {
        let folder = &o.src[..o.src.len() - 1];
        for n in 1..=folder.len() {
            f.add_folder(Tree::Source, &folder[..n], Attributes::default());
        }
        let m = groups
            .iter()
            .flat_map(|g| &g.members)
            .find(|m| m.id == o.member)
            .unwrap();
        f.add_source(
            &o.src,
            SourceFact {
                file: true,
                size: o.expected_size,
                mtime_ns: o.src_mtime_ns,
                attrs: Attributes {
                    read_only: m.read_only,
                    ..Attributes::default()
                },
                locked: false,
            },
        );
    }
    f
}

const PAIRS: &[&str] = &[
    "100NIKON/DSC_0001.JPG",
    "100NIKON/DSC_0001.NEF",
    "100NIKON/DSC_0001.NEF.xmp",
    "100NIKON/DSC_0002.JPG",
    "100NIKON/DSC_0002.NEF",
    "101NIKON/DSC_0003.JPG",
];

/// Group 1 kept, 2 rejected, 3 kept; keep by move to another volume.
fn standard() -> (Vec<PlanGroup>, Plan) {
    let groups = scan(PAIRS, &[]);
    let d = decide(&groups, &[Verdict::Keep, Verdict::Reject]);
    let p = plan(
        &groups,
        &d,
        &settings(KeepMode::Move, false),
        &Existing::new(),
    )
    .unwrap();
    (groups, p)
}

fn op_of(p: &Plan, name: &str) -> PlannedOp {
    p.ops
        .iter()
        .find(|o| o.src.last().is_some_and(|n| n == name))
        .unwrap()
        .clone()
}

/// The target folders of `o`, as P9 looks at them: under the destination root, or `_rejected`
/// and its folders under the source root.
fn target_folders(o: &PlannedOp) -> Vec<(Tree, Vec<String>)> {
    let folder = &o.dst[..o.dst.len() - 1];
    match o.root {
        Root::Dest => (1..=folder.len())
            .map(|n| (Tree::Dest, folder[..n].to_vec()))
            .collect(),
        Root::Rejected => (0..=folder.len())
            .map(|n| {
                let mut p = vec![REJECTED.to_owned()];
                p.extend(folder[..n].iter().cloned());
                (Tree::Source, p)
            })
            .collect(),
    }
}

/// The folders between the source root and the source of `o`.
fn source_folders(o: &PlannedOp) -> Vec<(Tree, Vec<String>)> {
    let folder = &o.src[..o.src.len() - 1];
    (1..=folder.len())
        .map(|n| (Tree::Source, folder[..n].to_vec()))
        .collect()
}

/// Plain facts for the target folders of `o`, as an adapter that sees them gives.
fn add_target_folders(f: &mut Facts, o: &PlannedOp) {
    for (t, p) in target_folders(o) {
        f.add_folder(t, &p, Attributes::default());
    }
}

fn fact_of<'a>(f: &'a mut Facts, o: &PlannedOp) -> &'a mut SourceFact {
    f.sources.get_mut(&key(&o.src)).unwrap()
}

// --- Clean -----------------------------------------------------------------------------------

#[test]
fn a_matching_disk_gives_an_empty_report() {
    let (groups, p) = standard();
    assert_eq!(p.ops.len(), 6);
    let f = clean(&p, &groups, vec![card(), disk()]);
    let r = preflight(&p, &roots(), &f);
    assert_eq!(r, Report::default());
    assert!(r.can_apply());
    assert_eq!(r.runnable(&p).len(), 6);
}

// --- P1, P2: volumes -------------------------------------------------------------------------

#[test]
fn p1_source_volume_missing_or_ambiguous_or_refused() {
    let (groups, p) = standard();
    let f = clean(&p, &groups, vec![disk()]);
    let r = preflight(&p, &roots(), &f);
    assert_eq!(r.blockers, vec![Blocker::SourceVolume(Unbound::Missing)]);
    assert!(!r.can_apply());

    let twin = vol(CARD, GUID_OTHER, "F:\\", "exFAT");
    let f = clean(&p, &groups, vec![card(), twin, disk()]);
    let r = preflight(&p, &roots(), &f);
    assert_eq!(
        r.blockers,
        vec![Blocker::SourceVolume(Unbound::Ambiguous(2))]
    );

    let mut net = card();
    net.drive_kind = DriveKind::Remote;
    let f = clean(&p, &groups, vec![net, disk()]);
    let r = preflight(&p, &roots(), &f);
    assert!(matches!(
        r.blockers.as_slice(),
        [Blocker::SourceVolumeRefused(_)]
    ));
}

#[test]
fn p9_source_root_that_became_a_link_blocks() {
    let (groups, p) = standard();
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.source_root = FolderFact::Exists(Attributes {
        reparse_point: true,
        ..Attributes::default()
    });
    assert_eq!(
        preflight(&p, &roots(), &f).blockers,
        vec![Blocker::SourceRootLink]
    );
    f.source_root = FolderFact::Unreadable;
    assert_eq!(
        preflight(&p, &roots(), &f).blockers,
        vec![Blocker::SourceRootUnreadable]
    );
}

#[test]
fn p2_destination_volume_and_root() {
    let (groups, p) = standard();
    let f = clean(&p, &groups, vec![card()]);
    let r = preflight(&p, &roots(), &f);
    assert_eq!(r.blockers, vec![Blocker::DestVolume(Unbound::Missing)]);

    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.dest_root = DestRoot::Missing;
    assert_eq!(
        preflight(&p, &roots(), &f).blockers,
        vec![Blocker::DestRootMissing]
    );
    f.dest_root = DestRoot::ParentExists(Attributes::default());
    assert!(preflight(&p, &roots(), &f).can_apply());
    f.dest_root = DestRoot::Unreadable;
    assert_eq!(
        preflight(&p, &roots(), &f).blockers,
        vec![Blocker::DestRootUnreadable]
    );
    f.dest_root = DestRoot::ParentExists(Attributes::default());

    let mut no_dest = roots();
    no_dest.dest = None;
    assert_eq!(
        preflight(&p, &no_dest, &f).blockers,
        vec![Blocker::NoDestination]
    );
}

#[test]
fn p2_a_rejects_only_plan_needs_no_destination() {
    let groups = scan(PAIRS, &[]);
    let d = decide(&groups, &[Verdict::Reject]);
    let p = plan(
        &groups,
        &d,
        &Settings {
            raw_follows: true,
            keep: None,
        },
        &Existing::new(),
    )
    .unwrap();
    let mut f = clean(&p, &groups, vec![card()]);
    f.dest_root = DestRoot::Missing;
    let mut r = roots();
    r.dest = None;
    assert!(preflight(&p, &r, &f).can_apply());
    // A destination chosen but not used by this plan is not checked.
    assert!(preflight(&p, &roots(), &f).can_apply());
}

// --- P3, P4: writable ------------------------------------------------------------------------

#[test]
fn p3_write_protected_card_blocks_renames_and_moves() {
    let (groups, p) = standard();
    let mut locked = card();
    locked.flags = VolumeFlags(VolumeFlags::READ_ONLY);
    let f = clean(&p, &groups, vec![locked.clone(), disk()]);
    let r = preflight(&p, &roots(), &f);
    assert_eq!(r.blockers, vec![Blocker::SourceReadOnly]);
    assert!(r.blockers[0].to_string().contains("lock switch"));

    // Keep by copy to another volume, nothing rejected: the card is only read.
    let groups = scan(PAIRS, &[]);
    let d = decide(&groups, &[Verdict::Keep]);
    let p = plan(
        &groups,
        &d,
        &settings(KeepMode::Copy, false),
        &Existing::new(),
    )
    .unwrap();
    let f = clean(&p, &groups, vec![locked.clone(), disk()]);
    assert!(preflight(&p, &roots(), &f).can_apply());

    // A camera-protected reject is copied into `_rejected`, on the card: it writes there.
    let groups = scan(PAIRS, &["101NIKON/DSC_0003.JPG"]);
    let d = decide(&groups, &[Verdict::Reject]);
    let p = plan(
        &groups,
        &d,
        &settings(KeepMode::Copy, false),
        &Existing::new(),
    )
    .unwrap();
    let f = clean(&p, &groups, vec![locked, disk()]);
    assert!(
        preflight(&p, &roots(), &f)
            .blockers
            .contains(&Blocker::SourceReadOnly)
    );
}

#[test]
fn p4_read_only_destination_blocks() {
    let (groups, p) = standard();
    let mut d = disk();
    d.flags = VolumeFlags(VolumeFlags::READ_ONLY);
    let f = clean(&p, &groups, vec![card(), d]);
    assert_eq!(
        preflight(&p, &roots(), &f).blockers,
        vec![Blocker::DestReadOnly]
    );
}

// --- P5: free space --------------------------------------------------------------------------

#[test]
fn p5_free_space_counts_whole_clusters_and_a_margin() {
    let (groups, p) = standard();
    let sizes: Vec<u64> = p
        .ops
        .iter()
        .filter(|o| o.kind == OpKind::XMove)
        .map(|o| o.expected_size)
        .collect();
    assert_eq!(sizes.len(), 4);
    let cluster = 32_768;
    let bytes = sizes.len() as u64 * cluster;
    let needed = bytes + MIN_MARGIN;
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    let space = |free| Space { free, cluster };
    f.space.insert(disk().guid_path, space(needed));
    assert!(preflight(&p, &roots(), &f).can_apply());
    f.space.insert(disk().guid_path, space(needed - 1));
    assert_eq!(
        preflight(&p, &roots(), &f).blockers,
        vec![Blocker::NoSpace {
            volume: VolumeId::new(DISK),
            needed,
            free: needed - 1
        }]
    );
    f.space.remove(&disk().guid_path);
    assert_eq!(
        preflight(&p, &roots(), &f).blockers,
        vec![Blocker::SpaceUnknown(VolumeId::new(DISK))]
    );
    // Renames need no space: the card's entry may be missing.
    f.space.insert(disk().guid_path, space(needed));
    f.space.remove(&card().guid_path);
    assert!(preflight(&p, &roots(), &f).can_apply());
}

#[test]
fn p5_margin_is_two_percent_above_fifty_gib() {
    assert_eq!(margin(0), MIN_MARGIN);
    assert_eq!(margin(50 << 30), MIN_MARGIN);
    assert_eq!(margin(100 << 30), 2 << 30);
    assert_eq!(on_disk(0, 4096), 0);
    assert_eq!(on_disk(1, 4096), 4096);
    assert_eq!(on_disk(4096, 4096), 4096);
    assert_eq!(on_disk(4097, 0), 4097);
    assert_eq!(on_disk(u64::MAX, 4096), u64::MAX);
}

#[test]
fn p5_camera_protected_rejects_need_space_on_the_card() {
    let groups = scan(PAIRS, &["101NIKON/DSC_0003.JPG"]);
    let d = decide(&groups, &[Verdict::Reject]);
    let p = plan(
        &groups,
        &d,
        &settings(KeepMode::Move, false),
        &Existing::new(),
    )
    .unwrap();
    assert!(p.ops.iter().any(|o| o.kind == OpKind::Copy));
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.space.insert(
        card().guid_path,
        Space {
            free: 10,
            cluster: 4096,
        },
    );
    assert!(matches!(
        preflight(&p, &roots(), &f).blockers.as_slice(),
        [Blocker::NoSpace { volume, .. }] if *volume == VolumeId::new(CARD)
    ));
}

#[test]
fn p5_one_volume_for_source_and_destination_adds_up() {
    // Keep by copy on the card itself, plus a camera-protected reject copied into `_rejected`.
    let groups = scan(PAIRS, &["100NIKON/DSC_0002.JPG"]);
    let d = decide(&groups, &[Verdict::Keep, Verdict::Reject]);
    let p = plan(
        &groups,
        &d,
        &settings(KeepMode::Copy, true),
        &Existing::new(),
    )
    .unwrap();
    let copies = p.ops.iter().filter(|o| o.kind == OpKind::Copy).count() as u64;
    assert_eq!(copies, 6);
    let mut r = roots();
    r.dest = Some(Source::new(VolumeId::new(CARD), parts("Keep")).unwrap());
    let mut f = clean(&p, &groups, vec![card()]);
    let needed = copies * 4096 + MIN_MARGIN;
    f.space.insert(
        card().guid_path,
        Space {
            free: needed - 1,
            cluster: 4096,
        },
    );
    assert_eq!(
        preflight(&p, &r, &f).blockers,
        vec![Blocker::NoSpace {
            volume: VolumeId::new(CARD),
            needed,
            free: needed - 1
        }]
    );
}

// --- P6: where the destination is ------------------------------------------------------------

#[test]
fn p6_destination_rules() {
    let groups = scan(PAIRS, &[]);
    let d = decide(&groups, &[Verdict::Keep]);
    let p = plan(
        &groups,
        &d,
        &settings(KeepMode::Move, true),
        &Existing::new(),
    )
    .unwrap();
    let on_card = |rel: &str| Roots {
        source: Source::new(VolumeId::new(CARD), parts("DCIM")).unwrap(),
        dest: Some(Source::new(VolumeId::new(CARD), parts(rel)).unwrap()),
    };
    let mut f = clean(&p, &groups, vec![card()]);
    add_ancestors(&mut f, &on_card("DCIM/_REJECTED/x"));
    assert!(preflight(&p, &on_card("Keep"), &f).can_apply());
    // Inside the source is allowed (the scan does not enter it).
    assert!(preflight(&p, &on_card("DCIM/Keep"), &f).can_apply());
    let r = preflight(&p, &on_card("dcim"), &f);
    assert!(r.blockers.contains(&Blocker::DestIsSource), "{r:?}");
    // Its targets are the sources too: P7 says so.
    assert!(r.blockers.iter().any(|b| matches!(
        b,
        Blocker::Inconsistent {
            why: Inconsistency::TargetIsSource,
            ..
        }
    )));
    let r = preflight(&p, &on_card("DCIM/_REJECTED/x"), &f);
    assert_eq!(r.blockers, vec![Blocker::DestInRejected]);

    let (groups, p) = standard();
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    for (attrs, b) in [
        (
            Attributes {
                cloud_placeholder: true,
                ..Attributes::default()
            },
            Blocker::DestPlaceholder,
        ),
        (
            Attributes {
                reparse_point: true,
                ..Attributes::default()
            },
            Blocker::DestLink,
        ),
    ] {
        f.dest_root = DestRoot::Folder(attrs);
        assert_eq!(preflight(&p, &roots(), &f).blockers, vec![b]);
    }
    f.dest_root = DestRoot::NotAFolder;
    assert_eq!(
        preflight(&p, &roots(), &f).blockers,
        vec![Blocker::DestNotAFolder]
    );

    let mut net = disk();
    net.drive_kind = DriveKind::Remote;
    let f = clean(&p, &groups, vec![card(), net]);
    assert!(matches!(
        preflight(&p, &roots(), &f).blockers.as_slice(),
        [Blocker::DestVolumeRefused(_)]
    ));
}

#[test]
fn p6_two_volumes_with_one_serial_block() {
    let (groups, p) = standard();
    let twin = vol(CARD, GUID_DISK, "C:\\", "NTFS");
    let mut r = roots();
    r.dest = Some(Source::new(VolumeId::new(CARD), parts("Photos")).unwrap());
    // The card and the twin share a serial: the source does not bind either.
    let f = clean(&p, &groups, vec![card(), twin]);
    let rep = preflight(&p, &r, &f);
    assert!(
        rep.blockers
            .contains(&Blocker::SourceVolume(Unbound::Ambiguous(2)))
    );
    assert!(
        rep.blockers
            .contains(&Blocker::DestVolume(Unbound::Ambiguous(2)))
    );
}

// --- P7: consistency (a bug) -----------------------------------------------------------------

#[test]
fn p7_bad_duplicate_or_source_targets_are_bugs() {
    let (groups, p) = standard();
    let f = clean(&p, &groups, vec![card(), disk()]);
    let mut bad = p.clone();
    bad.ops[0].dst = vec!["..".to_owned(), "x.JPG".to_owned()];
    let r = preflight(&bad, &roots(), &f);
    assert_eq!(
        r.blockers,
        vec![Blocker::Inconsistent {
            ord: 0,
            why: Inconsistency::BadTarget
        }]
    );
    assert!(r.blockers[0].to_string().contains("bug"));

    let mut dup = p.clone();
    let last = dup.ops.len() - 1;
    dup.ops[last].dst = dup.ops[last - 1].dst.clone();
    dup.ops[last].root = dup.ops[last - 1].root;
    assert!(
        preflight(&dup, &roots(), &f)
            .blockers
            .contains(&Blocker::Inconsistent {
                ord: dup.ops[last].ord,
                why: Inconsistency::DuplicateTarget
            })
    );

    // A reject whose target is `_rejected/…` can never be a source; one pointing back at a
    // source through the root is: make the rejected root's target equal a source path.
    let mut back = p.clone();
    let rej = back
        .ops
        .iter_mut()
        .find(|o| o.root == Root::Rejected)
        .unwrap();
    rej.root = Root::Dest;
    let mut r = roots();
    r.dest = Some(roots().source);
    let rep = preflight(&back, &r, &clean(&p, &groups, vec![card()]));
    assert!(rep.blockers.iter().any(|b| matches!(
        b,
        Blocker::Inconsistent {
            why: Inconsistency::TargetIsSource,
            ..
        }
    )));
}

// --- P8: lengths -----------------------------------------------------------------------------

#[test]
fn p8_long_paths_and_names_hold_the_group() {
    let (groups, p) = standard();
    // A destination 32,700 units deep: every target with its partial is over the limit.
    let deep: Vec<String> = (0..300).map(|_| "a".repeat(108)).collect();
    let mut r = roots();
    r.dest = Some(Source::new(VolumeId::new(DISK), deep).unwrap());
    let f = clean_at(&p, &groups, vec![card(), disk()], &r);
    let rep = preflight(&p, &r, &f);
    let kept: BTreeSet<GroupId> = p
        .ops
        .iter()
        .filter(|o| o.root == Root::Dest)
        .map(|o| o.group)
        .collect();
    assert_eq!(
        rep.held.iter().map(|(g, _)| *g).collect::<BTreeSet<_>>(),
        kept
    );
    assert!(
        rep.held
            .iter()
            .all(|(_, h)| matches!(h, Hold::PathTooLong { .. }))
    );
    // The rejected group still runs.
    assert!(rep.can_apply());
    assert!(rep.runnable(&p).iter().all(|o| o.root == Root::Rejected));
}

#[test]
fn p8_a_partial_name_must_fit_but_a_rename_has_none() {
    let stem = "D".repeat(220);
    let name = format!("{stem}.JPG");
    let path = format!("100NIKON/{name}");
    let groups = scan(&[path.as_str()], &[]);
    for (mode, same, held) in [
        (KeepMode::Move, false, true),
        (KeepMode::Copy, false, true),
        (KeepMode::Move, true, false),
    ] {
        let d = decide(&groups, &[Verdict::Keep]);
        let p = plan(&groups, &d, &settings(mode, same), &Existing::new()).unwrap();
        let mounted = if same {
            vec![card()]
        } else {
            vec![card(), disk()]
        };
        let mut r = roots();
        if same {
            r.dest = Some(Source::new(VolumeId::new(CARD), parts("Keep")).unwrap());
        }
        let rep = preflight(&p, &r, &clean(&p, &groups, mounted));
        assert_eq!(rep.is_held(1), held, "{mode:?} {same}");
    }
}

#[test]
fn p8_the_volume_limit_on_components_applies() {
    let (groups, p) = standard();
    let mut short = disk();
    short.max_component_len = 8;
    let f = clean(&p, &groups, vec![card(), short]);
    let rep = preflight(&p, &roots(), &f);
    assert!(rep.is_held(1) && rep.is_held(3) && !rep.is_held(2));
}

// --- P9: links and placeholders --------------------------------------------------------------

#[test]
fn p9_links_and_placeholders_hold_the_group() {
    let (groups, p) = standard();
    let o = op_of(&p, "DSC_0001.NEF");
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    fact_of(&mut f, &o).attrs.reparse_point = true;
    let r = preflight(&p, &roots(), &f);
    assert_eq!(r.held, vec![(1, Hold::Link { member: o.member })]);
    assert!(r.can_apply());

    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.add_folder(
        Tree::Source,
        &parts("100NIKON"),
        Attributes {
            cloud_placeholder: true,
            ..Attributes::default()
        },
    );
    let r = preflight(&p, &roots(), &f);
    let first = |g| {
        p.ops
            .iter()
            .find(|o| o.group == g)
            .map(|o| o.member)
            .unwrap()
    };
    assert_eq!(
        r.held,
        vec![
            (1, Hold::Placeholder { member: first(1) }),
            (2, Hold::Placeholder { member: first(2) })
        ]
    );

    // A target folder under the destination that is a junction.
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.add_folder(
        Tree::Dest,
        &parts("101NIKON"),
        Attributes {
            reparse_point: true,
            ..Attributes::default()
        },
    );
    assert_eq!(
        preflight(&p, &roots(), &f).held,
        vec![(3, Hold::Link { member: first(3) })]
    );

    // `_rejected` itself a junction holds every reject.
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.add_folder(
        Tree::Source,
        &parts("_rejected"),
        Attributes {
            reparse_point: true,
            ..Attributes::default()
        },
    );
    assert_eq!(
        preflight(&p, &roots(), &f).held,
        vec![(2, Hold::Link { member: first(2) })]
    );

    // A plain folder is fine.
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.add_folder(Tree::Dest, &parts("100NIKON"), Attributes::default());
    assert_eq!(preflight(&p, &roots(), &f), Report::default());
}

// --- P10: changed since scan -----------------------------------------------------------------

#[test]
fn p10_missing_or_changed_sources_hold_the_group() {
    let (groups, p) = standard();
    let o = op_of(&p, "DSC_0003.JPG");
    let held = vec![(3, Hold::ChangedSinceScan { member: o.member })];

    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.sources.remove(&key(&o.src));
    assert_eq!(preflight(&p, &roots(), &f).held, held);

    let mut f = clean(&p, &groups, vec![card(), disk()]);
    fact_of(&mut f, &o).size += 1;
    assert_eq!(preflight(&p, &roots(), &f).held, held);

    let mut f = clean(&p, &groups, vec![card(), disk()]);
    fact_of(&mut f, &o).file = false;
    assert_eq!(preflight(&p, &roots(), &f).held, held);

    let mut f = clean(&p, &groups, vec![card(), disk()]);
    fact_of(&mut f, &o).mtime_ns = Some(MTIME + 1);
    assert_eq!(preflight(&p, &roots(), &f).held, held);

    // Exactly one hour off on exFAT is a DST shift, not a change; on NTFS it is a change.
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    fact_of(&mut f, &o).mtime_ns = Some(MTIME - HOUR);
    assert!(preflight(&p, &roots(), &f).held.is_empty());
    let ntfs_card = vol(CARD, GUID_CARD, "E:\\", "NTFS");
    f.mounted = vec![ntfs_card, disk()];
    assert_eq!(preflight(&p, &roots(), &f).held, held);
}

#[test]
fn time_rule() {
    assert_eq!(TimeRule::for_fs("exfat"), TimeRule::DstHour);
    assert_eq!(TimeRule::for_fs("FAT32"), TimeRule::DstHour);
    assert_eq!(TimeRule::for_fs("FAT"), TimeRule::DstHour);
    assert_eq!(TimeRule::for_fs("NTFS"), TimeRule::Exact);
    assert_eq!(TimeRule::for_fs("ReFS"), TimeRule::Exact);
    let d = TimeRule::DstHour;
    assert!(d.matches(Some(5), Some(5)));
    assert!(d.matches(Some(HOUR + 5), Some(5)));
    assert!(d.matches(Some(5), Some(HOUR + 5)));
    assert!(!d.matches(Some(5), Some(HOUR + 6)));
    assert!(!d.matches(Some(5), Some(2 * HOUR + 5)));
    assert!(!d.matches(None, None));
    assert!(!TimeRule::Exact.matches(None, None));
    assert!(!d.matches(None, Some(5)));
    assert!(!TimeRule::Exact.matches(Some(HOUR), Some(0)));
    assert!(!d.matches(Some(i64::MIN), Some(i64::MAX)));
}

// --- P11, P13: re-plan -----------------------------------------------------------------------

#[test]
fn p11_a_target_taken_since_planning_needs_a_new_plan() {
    let (groups, p) = standard();
    let o = op_of(&p, "DSC_0001.NEF.xmp");
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    let path: Vec<&str> = o.dst.iter().map(String::as_str).collect();
    f.targets.add_file(Root::Dest, &path);
    // The snapshot now shows the target folder, so it needs a fact: without one the group is
    // held (fail closed), with one it needs a new plan.
    let first = p.ops.iter().find(|x| x.group == 1).unwrap().member;
    assert_eq!(
        preflight(&p, &roots(), &f).held,
        vec![(1, Hold::FolderUnknown { member: first })]
    );
    add_target_folders(&mut f, &o);
    let r = preflight(&p, &roots(), &f);
    assert_eq!(
        r.replan,
        vec![(1, Replan::TargetTaken { member: o.member })]
    );
    assert!(!r.can_apply());

    // A file where the target folder should be.
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.targets.add_file(Root::Rejected, &["100NIKON"]);
    // The snapshot shows `_rejected`, so its fact is there too.
    f.add_folder(Tree::Source, &parts("_rejected"), Attributes::default());
    let r = preflight(&p, &roots(), &f);
    assert_eq!(r.replan.len(), 1);
    assert_eq!(r.replan[0].0, 2);
}

#[test]
fn p13_read_only_since_planning_needs_a_new_plan_unless_copied() {
    let (groups, p) = standard();
    let o = op_of(&p, "DSC_0002.NEF");
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    fact_of(&mut f, &o).attrs.read_only = true;
    assert_eq!(
        preflight(&p, &roots(), &f).replan,
        vec![(2, Replan::ReadOnlyAppeared { member: o.member })]
    );

    let groups = scan(PAIRS, &[]);
    let d = decide(&groups, &[Verdict::Keep]);
    let p = plan(
        &groups,
        &d,
        &settings(KeepMode::Copy, false),
        &Existing::new(),
    )
    .unwrap();
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    let o = op_of(&p, "DSC_0002.NEF");
    fact_of(&mut f, &o).attrs.read_only = true;
    assert!(preflight(&p, &roots(), &f).replan.is_empty());
}

#[test]
fn replan_when_the_volume_relation_changed() {
    // Planned as on the source's volume, but the destination is now another volume.
    let groups = scan(PAIRS, &[]);
    let d = decide(&groups, &[Verdict::Keep]);
    let p = plan(
        &groups,
        &d,
        &settings(KeepMode::Move, true),
        &Existing::new(),
    )
    .unwrap();
    let f = clean(&p, &groups, vec![card(), disk()]);
    let r = preflight(&p, &roots(), &f);
    assert_eq!(
        r.replan,
        vec![
            (1, Replan::VolumeRelationChanged),
            (2, Replan::VolumeRelationChanged),
            (3, Replan::VolumeRelationChanged)
        ]
    );
    // And the other way round.
    let (groups, p) = standard();
    let mut r = roots();
    r.dest = Some(Source::new(VolumeId::new(CARD), parts("Keep")).unwrap());
    let rep = preflight(&p, &r, &clean(&p, &groups, vec![card()]));
    assert_eq!(rep.replan.len(), 2);
}

// --- P12, P15, P17: warnings -----------------------------------------------------------------

#[test]
fn p12_locked_files_are_a_warning() {
    let (groups, p) = standard();
    let o = op_of(&p, "DSC_0001.JPG");
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    fact_of(&mut f, &o).locked = true;
    let r = preflight(&p, &roots(), &f);
    assert_eq!(
        r.warnings,
        vec![Warning::Locked {
            group: 1,
            member: o.member
        }]
    );
    assert!(r.can_apply());
}

#[test]
fn p15_paths_over_259_characters_are_a_warning() {
    let (groups, p) = standard();
    let f = clean(&p, &groups, vec![card(), disk()]);
    let mut r = roots();
    let long = "p".repeat(240);
    r.dest = Some(Source::new(VolumeId::new(DISK), vec![long]).unwrap());
    let rep = preflight(&p, &r, &f);
    assert!(rep.can_apply());
    let lens: Vec<usize> = rep
        .warnings
        .iter()
        .map(|w| match w {
            Warning::LongPath { len, .. } => *len,
            _ => panic!("{w:?}"),
        })
        .collect();
    // C:\ + 240 + \100NIKON\DSC_0001.JPG = 3 + 240 + 22.
    assert_eq!(lens.len(), 4);
    assert!(lens.contains(&(3 + 240 + 22)));
    assert!(lens.iter().all(|l| *l > MAX_PATH_SHORT));
}

#[test]
fn p17_cloud_synced_destination_is_a_warning_only_when_keeping() {
    let (groups, p) = standard();
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.dest_in_sync_root = Some(true);
    let r = preflight(&p, &roots(), &f);
    assert_eq!(r.warnings, vec![Warning::CloudSynced]);
    assert!(r.can_apply());

    let groups = scan(PAIRS, &[]);
    let d = decide(&groups, &[Verdict::Reject]);
    let p = plan(
        &groups,
        &d,
        &settings(KeepMode::Move, false),
        &Existing::new(),
    )
    .unwrap();
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.dest_in_sync_root = Some(true);
    assert!(preflight(&p, &roots(), &f).warnings.is_empty());
}

// --- P14, P16 --------------------------------------------------------------------------------

#[test]
fn p14_files_over_4_gib_to_fat32_are_held() {
    let groups = scan(&["100NIKON/BIG.NEF", "100NIKON/SMALL.NEF"], &[]);
    let mut groups = groups;
    groups[0].members[0].size = FAT32_MAX_FILE + 1;
    let d = decide(&groups, &[Verdict::Keep]);
    for (fs, mode, same, held) in [
        ("FAT32", KeepMode::Copy, false, true),
        ("FAT32", KeepMode::Move, false, true),
        ("NTFS", KeepMode::Copy, false, false),
        ("exFAT", KeepMode::Copy, false, false),
    ] {
        let p = plan(&groups, &d, &settings(mode, same), &Existing::new()).unwrap();
        let mut f = clean(&p, &groups, vec![card(), vol(DISK, GUID_DISK, "C:\\", fs)]);
        f.space.insert(
            disk().guid_path,
            Space {
                free: u64::MAX,
                cluster: 4096,
            },
        );
        let r = preflight(&p, &roots(), &f);
        assert_eq!(r.is_held(1), held, "{fs}");
        assert!(!r.is_held(2));
        assert!(r.can_apply(), "{r:?}");
    }
    // At exactly 4 GiB − 1 it fits.
    groups[0].members[0].size = FAT32_MAX_FILE;
    let p = plan(
        &groups,
        &d,
        &settings(KeepMode::Copy, false),
        &Existing::new(),
    )
    .unwrap();
    let f = clean(
        &p,
        &groups,
        vec![card(), vol(DISK, GUID_DISK, "C:\\", "FAT32")],
    );
    assert!(!preflight(&p, &roots(), &f).is_held(1));
}

#[test]
fn p16_a_busy_preview_pipeline_blocks() {
    let (groups, p) = standard();
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.preview_idle = false;
    assert_eq!(
        preflight(&p, &roots(), &f).blockers,
        vec![Blocker::PreviewBusy]
    );
}

// --- Fail closed: folders, roots and facts not given ----------------------------------------

#[test]
fn p9_a_source_folder_with_no_fact_or_unreadable_holds() {
    let (groups, p) = standard();
    let first = |g| p.ops.iter().find(|o| o.group == g).unwrap().member;
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.folders.remove(&(Tree::Source, key(&parts("101NIKON"))));
    assert_eq!(
        preflight(&p, &roots(), &f).held,
        vec![(3, Hold::FolderUnknown { member: first(3) })]
    );
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.add_unreadable_folder(Tree::Source, &parts("100NIKON"));
    assert_eq!(
        preflight(&p, &roots(), &f).held,
        vec![
            (1, Hold::FolderUnknown { member: first(1) }),
            (2, Hold::FolderUnknown { member: first(2) })
        ]
    );
    // A link found elsewhere on the same member wins over an unknown folder.
    let o = op_of(&p, "DSC_0003.JPG");
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.add_unreadable_folder(Tree::Source, &parts("101NIKON"));
    fact_of(&mut f, &o).attrs.reparse_point = true;
    assert_eq!(
        preflight(&p, &roots(), &f).held,
        vec![(3, Hold::Link { member: o.member })]
    );
}

#[test]
fn p9_a_target_folder_shown_existing_needs_a_fact() {
    let (groups, p) = standard();
    let first = |g| p.ops.iter().find(|o| o.group == g).unwrap().member;
    // Under the destination.
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.targets.add_dir(Root::Dest, &["101NIKON"]);
    assert_eq!(
        preflight(&p, &roots(), &f).held,
        vec![(3, Hold::FolderUnknown { member: first(3) })]
    );
    f.add_folder(Tree::Dest, &parts("101NIKON"), Attributes::default());
    assert_eq!(preflight(&p, &roots(), &f), Report::default());
    f.add_unreadable_folder(Tree::Dest, &parts("101NIKON"));
    assert_eq!(
        preflight(&p, &roots(), &f).held,
        vec![(3, Hold::FolderUnknown { member: first(3) })]
    );
    // An unreadable target folder holds even when the snapshot does not show it.
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.add_unreadable_folder(Tree::Dest, &parts("100NIKON"));
    assert_eq!(
        preflight(&p, &roots(), &f).held,
        vec![(1, Hold::FolderUnknown { member: first(1) })]
    );
    // Under `_rejected`: anything shown under it shows `_rejected` itself.
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.targets.add_dir(Root::Rejected, &["999NIKON"]);
    assert_eq!(
        preflight(&p, &roots(), &f).held,
        vec![(2, Hold::FolderUnknown { member: first(2) })]
    );
    f.add_folder(Tree::Source, &parts("_rejected"), Attributes::default());
    assert_eq!(preflight(&p, &roots(), &f), Report::default());
}

#[test]
fn p9_folders_above_the_roots_block() {
    let (groups, p) = standard();
    let r = Roots {
        source: Source::new(VolumeId::new(CARD), parts("Card/DCIM")).unwrap(),
        dest: Some(Source::new(VolumeId::new(DISK), parts("Users/Photos")).unwrap()),
    };
    let base = clean_at(&p, &groups, vec![card(), disk()], &r);
    assert_eq!(preflight(&p, &r, &base), Report::default());
    let link = Attributes {
        reparse_point: true,
        ..Attributes::default()
    };
    let placeholder = Attributes {
        cloud_placeholder: true,
        ..Attributes::default()
    };
    let src_up = (Tree::Volume(VolumeId::new(CARD)), key(&parts("Card")));
    let dst_up = (Tree::Volume(VolumeId::new(DISK)), key(&parts("Users")));
    for (at, fact, want) in [
        (
            &src_up,
            Some(FolderFact::Exists(link)),
            Blocker::SourceAncestor(FolderIssue::Link),
        ),
        (
            &src_up,
            Some(FolderFact::Exists(placeholder)),
            Blocker::SourceAncestor(FolderIssue::Placeholder),
        ),
        (
            &src_up,
            Some(FolderFact::Unreadable),
            Blocker::SourceAncestor(FolderIssue::Unknown),
        ),
        (&src_up, None, Blocker::SourceAncestor(FolderIssue::Unknown)),
        (
            &dst_up,
            Some(FolderFact::Exists(link)),
            Blocker::DestAncestor(FolderIssue::Link),
        ),
        (&dst_up, None, Blocker::DestAncestor(FolderIssue::Unknown)),
    ] {
        let mut f = base.clone();
        match fact {
            Some(x) => f.folders.insert(at.clone(), x),
            None => f.folders.remove(at),
        };
        assert_eq!(
            preflight(&p, &r, &f).blockers,
            vec![want],
            "{at:?} {fact:?}"
        );
    }
    // The destination's parent, when the destination does not exist yet.
    let mut f = base.clone();
    for (a, want) in [
        (link, Some(Blocker::DestAncestor(FolderIssue::Link))),
        (
            placeholder,
            Some(Blocker::DestAncestor(FolderIssue::Placeholder)),
        ),
        (Attributes::default(), None),
    ] {
        f.dest_root = DestRoot::ParentExists(a);
        assert_eq!(
            preflight(&p, &r, &f).blockers,
            want.into_iter().collect::<Vec<_>>()
        );
    }
    // A missing destination is blocked as such, its folders above not looked at.
    f.dest_root = DestRoot::Missing;
    f.folders.remove(&dst_up);
    assert_eq!(
        preflight(&p, &r, &f).blockers,
        vec![Blocker::DestRootMissing]
    );
}

#[test]
fn facts_not_given_fail_closed() {
    let (groups, p) = standard();
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    let fresh = Facts::new(
        FolderFact::Exists(Attributes::default()),
        DestRoot::Folder(Attributes::default()),
    );
    assert!(!fresh.preview_idle);
    assert_eq!(fresh.dest_in_sync_root, None);
    f.preview_idle = fresh.preview_idle;
    f.dest_in_sync_root = fresh.dest_in_sync_root;
    let r = preflight(&p, &roots(), &f);
    assert_eq!(r.blockers, vec![Blocker::PreviewBusy]);
    assert_eq!(r.warnings, vec![Warning::SyncRootUnknown]);
    // No sync-root warning when nothing is kept.
    let groups = scan(PAIRS, &[]);
    let d = decide(&groups, &[Verdict::Reject]);
    let p = plan(
        &groups,
        &d,
        &settings(KeepMode::Move, false),
        &Existing::new(),
    )
    .unwrap();
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.dest_in_sync_root = None;
    assert!(preflight(&p, &roots(), &f).warnings.is_empty());
}

#[test]
fn p10_unknown_mtimes_hold() {
    let (groups, p) = standard();
    let mut p = p;
    let o = op_of(&p, "DSC_0003.JPG");
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    fact_of(&mut f, &o).mtime_ns = None;
    for x in p.ops.iter_mut().filter(|x| x.member == o.member) {
        x.src_mtime_ns = None;
    }
    assert_eq!(
        preflight(&p, &roots(), &f).held,
        vec![(3, Hold::ChangedSinceScan { member: o.member })]
    );
}

#[test]
fn p7_covers_held_groups_too() {
    let (groups, p) = standard();
    let mut dup = p.clone();
    let last = dup.ops.len() - 1;
    dup.ops[last].dst = dup.ops[last - 1].dst.clone();
    dup.ops[last].root = dup.ops[last - 1].root;
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    // Hold the group of the last operation: P7 still sees the duplicate.
    let o = dup.ops[last].clone();
    f.sources.remove(&key(&o.src));
    let r = preflight(&dup, &roots(), &f);
    assert!(r.is_held(o.group), "{r:?}");
    assert!(r.blockers.contains(&Blocker::Inconsistent {
        ord: o.ord,
        why: Inconsistency::DuplicateTarget
    }));
}

// --- Interplay -------------------------------------------------------------------------------

#[test]
fn held_groups_do_not_count_for_volume_checks() {
    // The only kept group is held: the destination is not needed, its space not counted,
    // and its warnings dropped.
    let groups = scan(&["100NIKON/A.JPG", "100NIKON/B.JPG"], &[]);
    let d = decide(&groups, &[Verdict::Keep, Verdict::Reject]);
    let p = plan(
        &groups,
        &d,
        &settings(KeepMode::Move, false),
        &Existing::new(),
    )
    .unwrap();
    let o = op_of(&p, "A.JPG");
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    f.sources.remove(&key(&o.src));
    f.dest_root = DestRoot::Missing;
    f.dest_in_sync_root = Some(true);
    f.space.clear();
    let r = preflight(&p, &roots(), &f);
    assert_eq!(
        r.held,
        vec![(1, Hold::ChangedSinceScan { member: o.member })]
    );
    assert!(r.blockers.is_empty() && r.warnings.is_empty(), "{r:?}");
    assert_eq!(r.runnable(&p).len(), 1);
}

#[test]
fn a_group_gets_its_first_hold_in_rule_order() {
    let (groups, p) = standard();
    let jpg = op_of(&p, "DSC_0001.JPG");
    let nef = op_of(&p, "DSC_0001.NEF");
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    fact_of(&mut f, &jpg).size += 1;
    fact_of(&mut f, &nef).attrs.reparse_point = true;
    // The JPEG comes first in plan order, so its reason wins.
    assert_eq!(
        preflight(&p, &roots(), &f).held,
        vec![(1, Hold::ChangedSinceScan { member: jpg.member })]
    );
    // On one member, P9 comes before P10.
    let mut f = clean(&p, &groups, vec![card(), disk()]);
    fact_of(&mut f, &jpg).size += 1;
    fact_of(&mut f, &jpg).attrs.reparse_point = true;
    assert_eq!(
        preflight(&p, &roots(), &f).held,
        vec![(1, Hold::Link { member: jpg.member })]
    );
}

#[test]
fn messages_are_plain() {
    for b in [
        Blocker::SourceVolume(Unbound::Missing),
        Blocker::DestVolume(Unbound::Ambiguous(2)),
        Blocker::SourceReadOnly,
        Blocker::NoSpace {
            volume: VolumeId::new(1),
            needed: 2,
            free: 1,
        },
        Blocker::PreviewBusy,
        Blocker::SourceAncestor(FolderIssue::Link),
        Blocker::DestAncestor(FolderIssue::Unknown),
        Blocker::SourceRootUnreadable,
        Blocker::DestRootUnreadable,
    ] {
        let s = b.to_string();
        assert!(!s.is_empty() && !s.contains('!'), "{s}");
    }
}

// --- Seeded property run ---------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

/// Cases per property: `CULLA_PROP_CASES` if set (10,000 for a long run), else 300.
fn cases() -> u64 {
    std::env::var("CULLA_PROP_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300)
}

const FILES: &[&str] = &[
    "100NIKON/DSC_0001.JPG",
    "100NIKON/DSC_0001.NEF",
    "100NIKON/DSC_0001.xmp",
    "100NIKON/DSC_0002.NEF",
    "100NIKON/sub/DSC_0003.JPG",
    "100NIKON/sub/DSC_0003.NEF.xmp",
    "100NIKON/sub/DSC_0003.NEF",
    "101NIKON/DSC_0001.JPG",
    "101NIKON/NKSC_PARAM/DSC_0001.JPG.nksc",
    "Été/X.JPG",
];

#[derive(Clone, Copy, Debug)]
enum Change {
    // One source fact of one operation.
    Missing,
    Size,
    Mtime,
    Link,
    Placeholder,
    Locked,
    ReadOnly,
    Taken,
    // One folder on the way to a source or a target.
    SourceFolderLink,
    SourceFolderPlaceholder,
    SourceFolderRemoved,
    SourceFolderUnreadable,
    TargetFolderLink,
    TargetFolderPlaceholder,
    TargetFolderUnreadable,
    TargetFolderShownNoFact,
    // A folder above a root.
    AncestorLink,
    AncestorPlaceholder,
    AncestorRemoved,
    AncestorUnreadable,
    // The roots.
    SourceRootLink,
    SourceRootUnreadable,
    DestRootMissing,
    DestRootUnreadable,
    DestRootNotAFolder,
    DestRootLink,
    DestParentPlaceholder,
    DestParentPlain,
    // Volumes, space, the pipeline, the sync root.
    SourceVolumeMissing,
    DestVolumeMissing,
    CardReadOnly,
    DiskReadOnly,
    NoSpace,
    SpaceUnknown,
    PreviewBusy,
    Synced,
    SyncUnknown,
}

const CHANGES: &[Change] = &[
    Change::Missing,
    Change::Size,
    Change::Mtime,
    Change::Link,
    Change::Placeholder,
    Change::Locked,
    Change::ReadOnly,
    Change::Taken,
    Change::SourceFolderLink,
    Change::SourceFolderPlaceholder,
    Change::SourceFolderRemoved,
    Change::SourceFolderUnreadable,
    Change::TargetFolderLink,
    Change::TargetFolderPlaceholder,
    Change::TargetFolderUnreadable,
    Change::TargetFolderShownNoFact,
    Change::AncestorLink,
    Change::AncestorPlaceholder,
    Change::AncestorRemoved,
    Change::AncestorUnreadable,
    Change::SourceRootLink,
    Change::SourceRootUnreadable,
    Change::DestRootMissing,
    Change::DestRootUnreadable,
    Change::DestRootNotAFolder,
    Change::DestRootLink,
    Change::DestParentPlaceholder,
    Change::DestParentPlain,
    Change::SourceVolumeMissing,
    Change::DestVolumeMissing,
    Change::CardReadOnly,
    Change::DiskReadOnly,
    Change::NoSpace,
    Change::SpaceUnknown,
    Change::PreviewBusy,
    Change::Synced,
    Change::SyncUnknown,
];

/// The holds that a set of bad folders gives: for each group, its first operation in plan
/// order whose source or target folders include one of them.
fn holds_for(
    p: &Plan,
    bad: &BTreeSet<(Tree, String)>,
    hold: fn(MemberId) -> Hold,
) -> Vec<(GroupId, Hold)> {
    let mut held: BTreeMap<GroupId, Hold> = BTreeMap::new();
    for o in &p.ops {
        if held.contains_key(&o.group) {
            continue;
        }
        let hit = source_folders(o)
            .into_iter()
            .chain(target_folders(o))
            .any(|(t, f)| bad.contains(&(t, key(&f))));
        if hit {
            held.insert(o.group, hold(o.member));
        }
    }
    held.into_iter().collect()
}

fn run_property(n: u64) -> BTreeMap<String, u64> {
    let mut seen: BTreeMap<String, u64> = BTreeMap::new();
    let link = Attributes {
        reparse_point: true,
        ..Attributes::default()
    };
    let placeholder = Attributes {
        cloud_placeholder: true,
        ..Attributes::default()
    };
    for case in 0..n {
        let mut rng = Rng::new(case + 1);
        let paths: Vec<&str> = FILES.iter().copied().filter(|_| rng.chance(70)).collect();
        let ro: Vec<&str> = paths.iter().copied().filter(|_| rng.chance(10)).collect();
        let groups = scan(&paths, &ro);
        let mut d = Decisions::new();
        for (i, g) in groups.iter().enumerate() {
            let v = match rng.below(3) {
                0 => continue,
                1 => Verdict::Keep,
                _ => Verdict::Reject,
            };
            let e = d.decide(g.id, v, &|_| false).unwrap().at(i as i64 + 1, 0);
            d.apply(&e).unwrap();
        }
        let same = rng.chance(30);
        let mode = if rng.chance(50) {
            KeepMode::Move
        } else {
            KeepMode::Copy
        };
        let Ok(p) = plan(
            &groups,
            &d,
            &Settings {
                raw_follows: rng.chance(60),
                keep: Some(KeepTo {
                    mode,
                    same_volume: same,
                }),
            },
            &Existing::new(),
        ) else {
            continue;
        };
        // Roots one folder down, so each has a folder above it.
        let mut r = Roots {
            source: Source::new(VolumeId::new(CARD), parts("DCIM/Shoot")).unwrap(),
            dest: Some(Source::new(VolumeId::new(DISK), parts("Photos/2026")).unwrap()),
        };
        let mounted = if same {
            r.dest = Some(Source::new(VolumeId::new(CARD), parts("Keep/2026")).unwrap());
            vec![card()]
        } else {
            vec![card(), disk()]
        };
        let dest_vol = VolumeId::new(if same { CARD } else { DISK });
        let f = clean_at(&p, &groups, mounted, &r);
        let rep = preflight(&p, &r, &f);
        assert_eq!(rep, Report::default(), "case {case}: clean");
        if p.ops.is_empty() {
            continue;
        }
        let keeps = p.ops.iter().any(|o| o.root == Root::Dest);
        let writes_source = p.ops.iter().any(|o| {
            matches!(o.kind, OpKind::Rename | OpKind::XMove)
                || (o.kind == OpKind::Copy && o.root == Root::Rejected)
        });
        let o = p.ops[rng.below(p.ops.len())].clone();
        let change = CHANGES[rng.below(CHANGES.len())];
        let m = o.member;
        let g = o.group;
        let mut f = f;
        let mut want = Report::default();
        let only_keeping = |b: Blocker| if keeps { vec![b] } else { Vec::new() };
        match change {
            Change::Missing => {
                f.sources.remove(&key(&o.src));
                want.held = vec![(g, Hold::ChangedSinceScan { member: m })];
            }
            Change::Size => {
                fact_of(&mut f, &o).size += 1 + rng.below(10) as u64;
                want.held = vec![(g, Hold::ChangedSinceScan { member: m })];
            }
            Change::Mtime => {
                fact_of(&mut f, &o).mtime_ns = Some(MTIME + 1);
                want.held = vec![(g, Hold::ChangedSinceScan { member: m })];
            }
            Change::Link => {
                fact_of(&mut f, &o).attrs.reparse_point = true;
                want.held = vec![(g, Hold::Link { member: m })];
            }
            Change::Placeholder => {
                fact_of(&mut f, &o).attrs.cloud_placeholder = true;
                want.held = vec![(g, Hold::Placeholder { member: m })];
            }
            Change::Locked => {
                fact_of(&mut f, &o).locked = true;
                want.warnings = vec![Warning::Locked {
                    group: g,
                    member: m,
                }];
            }
            Change::ReadOnly => {
                fact_of(&mut f, &o).attrs.read_only = true;
                if matches!(o.kind, OpKind::Rename | OpKind::XMove) {
                    want.replan = vec![(g, Replan::ReadOnlyAppeared { member: m })];
                }
            }
            Change::Taken => {
                let path: Vec<&str> = o.dst.iter().map(String::as_str).collect();
                f.targets.add_file(o.root, &path);
                // An adapter that sees the target sees its folders too.
                add_target_folders(&mut f, &o);
                want.replan = vec![(g, Replan::TargetTaken { member: m })];
            }
            Change::SourceFolderLink
            | Change::SourceFolderPlaceholder
            | Change::SourceFolderRemoved
            | Change::SourceFolderUnreadable => {
                let folders = source_folders(&o);
                let (t, folder) = folders[rng.below(folders.len())].clone();
                let at = (t, key(&folder));
                let hold: fn(MemberId) -> Hold = match change {
                    Change::SourceFolderLink => {
                        f.add_folder(t, &folder, link);
                        |member| Hold::Link { member }
                    }
                    Change::SourceFolderPlaceholder => {
                        f.add_folder(t, &folder, placeholder);
                        |member| Hold::Placeholder { member }
                    }
                    Change::SourceFolderRemoved => {
                        f.folders.remove(&at);
                        |member| Hold::FolderUnknown { member }
                    }
                    _ => {
                        f.add_unreadable_folder(t, &folder);
                        |member| Hold::FolderUnknown { member }
                    }
                };
                want.held = holds_for(&p, &BTreeSet::from([at]), hold);
            }
            Change::TargetFolderLink
            | Change::TargetFolderPlaceholder
            | Change::TargetFolderUnreadable => {
                let folders = target_folders(&o);
                let (t, folder) = folders[rng.below(folders.len())].clone();
                let hold: fn(MemberId) -> Hold = match change {
                    Change::TargetFolderLink => {
                        f.add_folder(t, &folder, link);
                        |member| Hold::Link { member }
                    }
                    Change::TargetFolderPlaceholder => {
                        f.add_folder(t, &folder, placeholder);
                        |member| Hold::Placeholder { member }
                    }
                    _ => {
                        f.add_unreadable_folder(t, &folder);
                        |member| Hold::FolderUnknown { member }
                    }
                };
                want.held = holds_for(&p, &BTreeSet::from([(t, key(&folder))]), hold);
            }
            Change::TargetFolderShownNoFact => {
                // The snapshot shows a target folder, and every folder above it, with no fact.
                let folders = target_folders(&o);
                let (t, folder) = folders[rng.below(folders.len())].clone();
                let mut bad = BTreeSet::new();
                match o.root {
                    Root::Dest => {
                        let rel: Vec<&str> = folder.iter().map(String::as_str).collect();
                        f.targets.add_dir(Root::Dest, &rel);
                        for n in 1..=folder.len() {
                            bad.insert((t, key(&folder[..n])));
                        }
                    }
                    Root::Rejected => {
                        // `folder` starts with `_rejected`; the snapshot is relative to it.
                        let rel: Vec<&str> = folder[1..].iter().map(String::as_str).collect();
                        if rel.is_empty() {
                            f.targets.add_dir(Root::Rejected, &["zz-not-planned"]);
                        } else {
                            f.targets.add_dir(Root::Rejected, &rel);
                        }
                        for n in 1..=folder.len() {
                            bad.insert((t, key(&folder[..n])));
                        }
                    }
                }
                want.held = holds_for(&p, &bad, |member| Hold::FolderUnknown { member });
            }
            Change::AncestorLink
            | Change::AncestorPlaceholder
            | Change::AncestorRemoved
            | Change::AncestorUnreadable => {
                let on_source = rng.chance(50);
                let (vol, up) = if on_source {
                    (VolumeId::new(CARD), parts("DCIM"))
                } else {
                    (dest_vol, parts(if same { "Keep" } else { "Photos" }))
                };
                let tree = Tree::Volume(vol);
                let issue = match change {
                    Change::AncestorLink => {
                        f.add_folder(tree, &up, link);
                        FolderIssue::Link
                    }
                    Change::AncestorPlaceholder => {
                        f.add_folder(tree, &up, placeholder);
                        FolderIssue::Placeholder
                    }
                    Change::AncestorRemoved => {
                        f.folders.remove(&(tree, key(&up)));
                        FolderIssue::Unknown
                    }
                    _ => {
                        f.add_unreadable_folder(tree, &up);
                        FolderIssue::Unknown
                    }
                };
                want.blockers = if on_source {
                    vec![Blocker::SourceAncestor(issue)]
                } else {
                    only_keeping(Blocker::DestAncestor(issue))
                };
            }
            Change::SourceRootLink => {
                f.source_root = FolderFact::Exists(if rng.chance(50) { link } else { placeholder });
                want.blockers = vec![Blocker::SourceRootLink];
            }
            Change::SourceRootUnreadable => {
                f.source_root = FolderFact::Unreadable;
                want.blockers = vec![Blocker::SourceRootUnreadable];
            }
            Change::DestRootMissing => {
                f.dest_root = DestRoot::Missing;
                want.blockers = only_keeping(Blocker::DestRootMissing);
            }
            Change::DestRootUnreadable => {
                f.dest_root = DestRoot::Unreadable;
                want.blockers = only_keeping(Blocker::DestRootUnreadable);
            }
            Change::DestRootNotAFolder => {
                f.dest_root = DestRoot::NotAFolder;
                want.blockers = only_keeping(Blocker::DestNotAFolder);
            }
            Change::DestRootLink => {
                f.dest_root = DestRoot::Folder(link);
                want.blockers = only_keeping(Blocker::DestLink);
            }
            Change::DestParentPlaceholder => {
                f.dest_root = DestRoot::ParentExists(placeholder);
                want.blockers = only_keeping(Blocker::DestAncestor(FolderIssue::Placeholder));
            }
            Change::DestParentPlain => f.dest_root = DestRoot::ParentExists(Attributes::default()),
            Change::SourceVolumeMissing => {
                f.mounted.retain(|v| v.id != VolumeId::new(CARD));
                want.blockers = vec![Blocker::SourceVolume(Unbound::Missing)];
                if same {
                    want.blockers
                        .extend(only_keeping(Blocker::DestVolume(Unbound::Missing)));
                }
            }
            Change::DestVolumeMissing => {
                if !same {
                    f.mounted.retain(|v| v.id != VolumeId::new(DISK));
                    want.blockers = only_keeping(Blocker::DestVolume(Unbound::Missing));
                }
            }
            Change::CardReadOnly => {
                for v in f.mounted.iter_mut().filter(|v| v.id == VolumeId::new(CARD)) {
                    v.flags = VolumeFlags(VolumeFlags::READ_ONLY);
                }
                if same {
                    want.blockers = only_keeping(Blocker::DestReadOnly);
                }
                if writes_source {
                    want.blockers.push(Blocker::SourceReadOnly);
                }
            }
            Change::DiskReadOnly => {
                if !same {
                    for v in f.mounted.iter_mut().filter(|v| v.id == VolumeId::new(DISK)) {
                        v.flags = VolumeFlags(VolumeFlags::READ_ONLY);
                    }
                    want.blockers = only_keeping(Blocker::DestReadOnly);
                }
            }
            Change::NoSpace | Change::SpaceUnknown => {
                let v = f.mounted[rng.below(f.mounted.len())].clone();
                let sizes: Vec<u64> = p
                    .ops
                    .iter()
                    .filter(|x| x.kind != OpKind::Rename)
                    .filter(|x| match x.root {
                        Root::Dest => dest_vol == v.id,
                        Root::Rejected => v.id == VolumeId::new(CARD),
                    })
                    .map(|x| x.expected_size)
                    .collect();
                if matches!(change, Change::NoSpace) {
                    f.space.insert(
                        v.guid_path.clone(),
                        Space {
                            free: 0,
                            cluster: 4096,
                        },
                    );
                    if !sizes.is_empty() {
                        let bytes: u64 = sizes.iter().map(|s| s.div_ceil(4096) * 4096).sum();
                        want.blockers = vec![Blocker::NoSpace {
                            volume: v.id,
                            needed: bytes + margin(bytes),
                            free: 0,
                        }];
                    }
                } else {
                    f.space.remove(&v.guid_path);
                    if !sizes.is_empty() {
                        want.blockers = vec![Blocker::SpaceUnknown(v.id)];
                    }
                }
            }
            Change::PreviewBusy => {
                f.preview_idle = false;
                want.blockers = vec![Blocker::PreviewBusy];
            }
            Change::Synced => {
                f.dest_in_sync_root = Some(true);
                if keeps {
                    want.warnings = vec![Warning::CloudSynced];
                }
            }
            Change::SyncUnknown => {
                f.dest_in_sync_root = None;
                if keeps {
                    want.warnings = vec![Warning::SyncRootUnknown];
                }
            }
        }
        let rep = preflight(&p, &r, &f);
        assert_eq!(rep, want, "case {case}: {change:?} on {o:?}");
        // Held groups keep no runnable operation; the others all remain.
        for x in rep.runnable(&p) {
            assert!(!rep.is_held(x.group));
        }
        let held_ops = p.ops.iter().filter(|x| rep.is_held(x.group)).count();
        assert_eq!(rep.runnable(&p).len() + held_ops, p.ops.len());
        *seen.entry(format!("{change:?}")).or_default() += 1;
        *seen.entry(format!("{:?}", o.kind)).or_default() += 1;
    }
    seen
}

#[test]
fn prop_one_changed_fact_gives_one_finding() {
    let n = cases();
    let seen = run_property(n);
    if n >= 300 {
        for c in CHANGES {
            assert!(seen.contains_key(&format!("{c:?}")), "{c:?} never came up");
        }
        for k in ["Rename", "Copy", "XMove"] {
            assert!(seen.contains_key(k), "{k} never came up");
        }
    }
}
