use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::decisions::{Decisions, GroupId, Verdict};
use crate::grouping::{ScanEntry, ScanPolicy, group};

const NO_LOCKS: &dyn Fn(GroupId) -> bool = &|_| false;

/// Groups for files given as `folder/…/name` paths, as a scan of them would give, with ids
/// 1, 2, … in grouping order and member ids `100 * id + k`.
fn scan(paths: &[&str]) -> Vec<PlanGroup> {
    let entries = paths.iter().map(|p| {
        let parts: Vec<&str> = p.split('/').collect();
        let (name, folder) = parts.split_last().unwrap();
        let mut e = ScanEntry::file(folder, name);
        e.size = 1000 + name.len() as u64;
        e.mtime_ns = Some(1_700_000_000_000_000_000);
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

fn decide(pairs: &[(GroupId, Verdict)]) -> Decisions {
    let mut d = Decisions::new();
    for (i, (g, v)) in pairs.iter().enumerate() {
        let e = d.decide(*g, *v, NO_LOCKS).unwrap().at(i as i64 + 1, 0);
        d.apply(&e).unwrap();
    }
    d
}

fn all(groups: &[PlanGroup], v: Verdict) -> Decisions {
    let pairs: Vec<_> = groups.iter().map(|g| (g.id, v)).collect();
    decide(&pairs)
}

const MOVE_SAME: Settings = Settings {
    raw_follows: true,
    keep: Some(KeepTo {
        mode: KeepMode::Move,
        same_volume: true,
    }),
};

fn with(raw_follows: bool, mode: KeepMode, same_volume: bool) -> Settings {
    Settings {
        raw_follows,
        keep: Some(KeepTo { mode, same_volume }),
    }
}

/// (source path, root, target path) of every op, paths joined with `/`.
fn moves(p: &Plan) -> Vec<(String, Root, String)> {
    p.ops
        .iter()
        .map(|o| (o.src.join("/"), o.root, o.dst.join("/")))
        .collect()
}

fn targets(p: &Plan) -> Vec<String> {
    p.ops.iter().map(|o| o.dst.join("/")).collect()
}

// --- Examples from the stories and file-engine §12 ------------------------------------------

/// US-12: rejects go to `_rejected/`, mirroring their folders; nothing is deleted.
#[test]
fn us12_rejects_mirror_into_rejected() {
    let groups = scan(&[
        "100NIKON/DSC_0001.JPG",
        "100NIKON/DSC_0001.NEF",
        "101NIKON/DSC_0001.JPG",
    ]);
    let p = plan(
        &groups,
        &all(&groups, Verdict::Reject),
        &MOVE_SAME,
        &Existing::new(),
    )
    .unwrap();
    assert_eq!(
        moves(&p),
        vec![
            (
                "100NIKON/DSC_0001.JPG".into(),
                Root::Rejected,
                "100NIKON/DSC_0001.JPG".into()
            ),
            (
                "100NIKON/DSC_0001.NEF".into(),
                Root::Rejected,
                "100NIKON/DSC_0001.NEF".into()
            ),
            (
                "101NIKON/DSC_0001.JPG".into(),
                Root::Rejected,
                "101NIKON/DSC_0001.JPG".into()
            ),
        ]
    );
    assert!(
        p.ops
            .iter()
            .all(|o| o.kind == OpKind::Rename && o.action == Verdict::Reject)
    );
    // A reject without a destination is fine: only keeps need one.
    let no_dest = Settings {
        raw_follows: true,
        keep: None,
    };
    assert_eq!(
        plan(
            &groups,
            &all(&groups, Verdict::Reject),
            &no_dest,
            &Existing::new()
        )
        .unwrap(),
        p
    );
}

/// US-13: keep by move or copy mirrors the folders; undecided groups get nothing.
#[test]
fn us13_keep_by_move_or_copy() {
    let groups = scan(&["A/X.JPG", "A/X.NEF", "A/B/Y.JPG", "Z.JPG"]);
    // Groups in order: Z (root), A/X, A/B/Y. Keep X and Y, leave Z undecided.
    let ids: BTreeMap<String, GroupId> = groups.iter().map(|g| (g.stem.clone(), g.id)).collect();
    let d = decide(&[(ids["X"], Verdict::Keep), (ids["Y"], Verdict::Keep)]);
    for (mode, same, kind) in [
        (KeepMode::Move, true, OpKind::Rename),
        (KeepMode::Move, false, OpKind::XMove),
        (KeepMode::Copy, true, OpKind::Copy),
        (KeepMode::Copy, false, OpKind::Copy),
    ] {
        let p = plan(&groups, &d, &with(true, mode, same), &Existing::new()).unwrap();
        assert_eq!(
            moves(&p),
            vec![
                ("A/X.JPG".into(), Root::Dest, "A/X.JPG".into()),
                ("A/X.NEF".into(), Root::Dest, "A/X.NEF".into()),
                ("A/B/Y.JPG".into(), Root::Dest, "A/B/Y.JPG".into()),
            ]
        );
        assert!(p.ops.iter().all(|o| o.kind == kind));
        assert!(p.ops.iter().all(|o| o.group != ids["Z"]));
        let bytes: u64 = p.ops.iter().map(|o| o.expected_size).sum();
        match kind {
            OpKind::Rename => assert_eq!((p.bytes_copy, p.bytes_xmove), (0, 0)),
            OpKind::XMove => assert_eq!((p.bytes_copy, p.bytes_xmove), (0, bytes)),
            OpKind::Copy => assert_eq!((p.bytes_copy, p.bytes_xmove), (bytes, 0)),
        }
    }
}

const FULL_SET: &[&str] = &[
    "F/X.JPG",
    "F/X.NEF",
    "F/X.NEF.xmp",
    "F/X.NEF.pp3",
    "F/X.JPG.xmp",
    "F/X.xmp",
    "F/X.pp3",
    "F/NKSC_PARAM/X.NEF.nksc",
    "F/NKSC_PARAM/X.JPG.nksc",
];

/// US-14 and ADR 0023: with "RAW follows JPEG" off, the NEF, its sidecars and the bare
/// `X.xmp` and `X.pp3` stay; the JPEG and its own sidecars move. INV-08.
#[test]
fn us14_inv08_raw_follows_off_leaves_the_nef_and_its_sidecars() {
    let groups = scan(FULL_SET);
    assert_eq!(groups.len(), 1);
    let d = all(&groups, Verdict::Keep);
    let p = plan(
        &groups,
        &d,
        &with(false, KeepMode::Move, true),
        &Existing::new(),
    )
    .unwrap();
    assert_eq!(
        targets(&p),
        vec!["F/X.JPG", "F/X.JPG.xmp", "F/NKSC_PARAM/X.JPG.nksc"]
    );
    // On: the whole group, in member order (primary, raw, group sidecars, then file sidecars
    // by folder and name).
    let p = plan(
        &groups,
        &d,
        &with(true, KeepMode::Move, true),
        &Existing::new(),
    )
    .unwrap();
    assert_eq!(
        targets(&p),
        vec![
            "F/X.JPG",
            "F/X.NEF",
            "F/X.pp3",
            "F/X.xmp",
            "F/X.JPG.xmp",
            "F/X.NEF.pp3",
            "F/X.NEF.xmp",
            "F/NKSC_PARAM/X.JPG.nksc",
            "F/NKSC_PARAM/X.NEF.nksc",
        ]
    );
}

/// US-14: a bare sidecar next to a JPEG with no NEF follows the JPEG; a lone NEF moves like
/// any primary, whatever the toggle.
#[test]
fn us14_jpeg_only_and_lone_nef() {
    let groups = scan(&["J/X.JPG", "J/X.xmp", "N/Y.NEF", "N/Y.xmp", "N/Y.NEF.xmp"]);
    let d = all(&groups, Verdict::Reject);
    let p = plan(
        &groups,
        &d,
        &with(false, KeepMode::Move, true),
        &Existing::new(),
    )
    .unwrap();
    assert_eq!(
        targets(&p),
        vec!["J/X.JPG", "J/X.xmp", "N/Y.NEF", "N/Y.xmp", "N/Y.NEF.xmp"]
    );
}

/// US-15, INV-07: a taken target gives the whole group a suffix, inside sidecar names too.
#[test]
fn us15_inv07_collisions_suffix_the_whole_group() {
    let groups = scan(&[
        "100NIKON/DSC_0001.JPG",
        "100NIKON/DSC_0001.NEF",
        "100NIKON/DSC_0001.NEF.xmp",
        "100NIKON/NKSC_PARAM/DSC_0001.NEF.nksc",
    ]);
    let d = all(&groups, Verdict::Keep);
    let mut existing = Existing::new();
    existing.add_file(Root::Dest, &["100NIKON", "DSC_0001.JPG"]);
    let p = plan(&groups, &d, &MOVE_SAME, &existing).unwrap();
    assert_eq!(
        targets(&p),
        vec![
            "100NIKON/DSC_0001_1.JPG",
            "100NIKON/DSC_0001_1.NEF",
            "100NIKON/DSC_0001_1.NEF.xmp",
            "100NIKON/NKSC_PARAM/DSC_0001_1.NEF.nksc",
        ]
    );
    assert_eq!(p.groups[0].suffix, 1);
    // `_1` taken by any member (folded: case does not matter): the whole group gets `_2`.
    existing.add_file(
        Root::Dest,
        &["100nikon", "nksc_param", "dsc_0001_1.nef.NKSC"],
    );
    let p = plan(&groups, &d, &MOVE_SAME, &existing).unwrap();
    assert_eq!(p.groups[0].suffix, 2);
    assert!(targets(&p).iter().all(|t| t.contains("DSC_0001_2.")));
    // A folder with the target's name counts as taken too.
    let mut existing = Existing::new();
    existing.add_dir(Root::Dest, &["100NIKON", "DSC_0001.NEF"]);
    assert_eq!(
        plan(&groups, &d, &MOVE_SAME, &existing).unwrap().groups[0].suffix,
        1
    );
}

/// §7.3: targets claimed earlier in the plan count as taken; suffixes go in plan order.
#[test]
fn inv06_claims_within_the_plan() {
    let groups = scan(&["F/DSC_0001.JPG", "F/DSC_0001_1.JPG"]);
    let d = all(&groups, Verdict::Keep);
    let mut existing = Existing::new();
    existing.add_file(Root::Dest, &["F", "DSC_0001.JPG"]);
    let p = plan(&groups, &d, &MOVE_SAME, &existing).unwrap();
    assert_eq!(targets(&p), vec!["F/DSC_0001_1.JPG", "F/DSC_0001_1_1.JPG"]);
    // Rejects come first, so a reject claims before a keep, but under its own root.
    let d = decide(&[(1, Verdict::Keep), (2, Verdict::Reject)]);
    let p = plan(&groups, &d, &MOVE_SAME, &existing).unwrap();
    assert_eq!(
        moves(&p),
        vec![
            (
                "F/DSC_0001_1.JPG".into(),
                Root::Rejected,
                "F/DSC_0001_1.JPG".into()
            ),
            (
                "F/DSC_0001.JPG".into(),
                Root::Dest,
                "F/DSC_0001_1.JPG".into()
            ),
        ]
    );
}

/// §7.2, P13: a read-only member turns a rename or xmove group into copies, rejects included.
#[test]
fn camera_protected_groups_are_copied() {
    let mut groups = scan(&["F/X.JPG", "F/X.NEF", "F/X.xmp"]);
    groups[0].members[1].read_only = true; // the NEF
    let d = all(&groups, Verdict::Reject);
    let p = plan(&groups, &d, &MOVE_SAME, &Existing::new()).unwrap();
    assert!(
        p.ops
            .iter()
            .all(|o| o.kind == OpKind::Copy && o.root == Root::Rejected)
    );
    assert!(p.groups[0].protected);
    assert_eq!(
        p.bytes_copy,
        p.ops.iter().map(|o| o.expected_size).sum::<u64>()
    );
    // With the NEF left in place, its attribute does not matter.
    let p = plan(
        &groups,
        &d,
        &with(false, KeepMode::Move, true),
        &Existing::new(),
    )
    .unwrap();
    assert!(p.ops.iter().all(|o| o.kind == OpKind::Rename));
    assert!(!p.groups[0].protected);
    // Keep by move across volumes: copies too.
    let d = all(&groups, Verdict::Keep);
    let p = plan(
        &groups,
        &d,
        &with(true, KeepMode::Move, false),
        &Existing::new(),
    )
    .unwrap();
    assert!(p.ops.iter().all(|o| o.kind == OpKind::Copy) && p.groups[0].protected);
    assert_eq!(p.bytes_xmove, 0);
    // Keep by copy is a copy anyway: not reported as protected.
    let p = plan(
        &groups,
        &d,
        &with(true, KeepMode::Copy, true),
        &Existing::new(),
    )
    .unwrap();
    assert!(!p.groups[0].protected);
}

/// INV-04: ambiguous, applied, locked and empty groups get no operations and are listed.
#[test]
fn inv04_held_groups() {
    let mut groups = scan(&["A/X.JPG", "B/X.JPG", "C/X.JPG", "D/X.JPG", "E/X.JPG"]);
    groups[0].status = GroupStatus::Ambiguous;
    groups[1].applied = true;
    groups[2].locked = true;
    groups[3].members[0].present = false;
    let d = all(&groups, Verdict::Keep);
    let p = plan(&groups, &d, &MOVE_SAME, &Existing::new()).unwrap();
    assert_eq!(targets(&p), vec!["E/X.JPG"]);
    assert_eq!(
        p.held,
        vec![
            (1, Held::Ambiguous),
            (2, Held::Applied),
            (3, Held::Locked),
            (4, Held::NothingPresent)
        ]
    );
    assert_eq!(p.missing, vec![(4, 400)]);
}

/// Missing members get nothing and are listed; a sidecar of a missing file stays.
#[test]
fn missing_members_are_listed() {
    let mut groups = scan(&["F/X.JPG", "F/X.NEF", "F/X.NEF.xmp", "F/X.xmp"]);
    groups[0].members[1].present = false; // the NEF
    let d = all(&groups, Verdict::Keep);
    let p = plan(&groups, &d, &MOVE_SAME, &Existing::new()).unwrap();
    assert_eq!(targets(&p), vec!["F/X.JPG"]);
    assert_eq!(p.missing, vec![(1, 101)]);
    // With RAW follows off, the missing NEF would not move anyway: not listed.
    let p = plan(
        &groups,
        &d,
        &with(false, KeepMode::Move, true),
        &Existing::new(),
    )
    .unwrap();
    assert!(p.missing.is_empty());
}

#[test]
fn no_free_suffix_or_blocked_folder_holds_the_group() {
    let groups = scan(&["F/X.JPG"]);
    let d = all(&groups, Verdict::Keep);
    let mut existing = Existing::new();
    for n in 0..=MAX_SUFFIX {
        let name = target_name("X.JPG", "X", n).unwrap();
        existing.add_file(Root::Dest, &["F", &name]);
    }
    let p = plan(&groups, &d, &MOVE_SAME, &existing).unwrap();
    assert!(p.ops.is_empty());
    assert_eq!(p.held, vec![(1, Held::NoFreeSuffix)]);
    let mut existing = Existing::new();
    existing.add_file(Root::Dest, &["f"]);
    let p = plan(&groups, &d, &MOVE_SAME, &existing).unwrap();
    assert_eq!(p.held, vec![(1, Held::TargetFolderBlocked)]);
    // A folder that exists is fine.
    let mut existing = Existing::new();
    existing.add_dir(Root::Dest, &["F"]);
    assert_eq!(
        plan(&groups, &d, &MOVE_SAME, &existing).unwrap().ops.len(),
        1
    );
}

#[test]
fn inconsistent_names_hold_the_group() {
    let mut groups = scan(&["F/X.JPG"]);
    groups[0].members[0].name = "Y.JPG".into();
    let p = plan(
        &groups,
        &all(&groups, Verdict::Keep),
        &MOVE_SAME,
        &Existing::new(),
    )
    .unwrap();
    assert_eq!(p.held, vec![(1, Held::NameMismatch)]);
}

#[test]
fn errors() {
    let groups = scan(&["F/X.JPG"]);
    let no_dest = Settings {
        raw_follows: true,
        keep: None,
    };
    assert_eq!(
        plan(
            &groups,
            &all(&groups, Verdict::Keep),
            &no_dest,
            &Existing::new()
        ),
        Err(PlanError::NoDestination)
    );
    let twice = [groups[0].clone(), groups[0].clone()];
    assert_eq!(
        plan(&twice, &Decisions::new(), &MOVE_SAME, &Existing::new()),
        Err(PlanError::DuplicateGroup(1))
    );
    assert_eq!(
        PlanError::NoDestination.to_string(),
        "a group is kept but no destination is chosen"
    );
}

#[test]
fn target_names() {
    assert_eq!(target_name("X.JPG", "X", 0).as_deref(), Some("X.JPG"));
    assert_eq!(target_name("X.JPG", "X", 1).as_deref(), Some("X_1.JPG"));
    assert_eq!(
        target_name("X.NEF.xmp", "X", 12).as_deref(),
        Some("X_12.NEF.xmp")
    );
    assert_eq!(
        target_name("DSC_0001.edit.JPG", "DSC_0001.edit", 1).as_deref(),
        Some("DSC_0001.edit_1.JPG")
    );
    // The member's own case is kept: only the suffix is added.
    assert_eq!(target_name("x.nef", "X", 3).as_deref(), Some("x_3.nef"));
    assert_eq!(
        target_name("Été.jpg", "ÉTÉ", 1).as_deref(),
        Some("Été_1.jpg")
    );
    assert_eq!(target_name("Y.JPG", "X", 1), None);
    assert_eq!(target_name("X", "XY", 1), None);
}

/// §9.5: rejects before keeps, then folder and stem, ords from 0.
#[test]
fn plan_order() {
    let groups = scan(&["B/K.JPG", "A/R.JPG", "A/K.JPG", "B/R.JPG"]);
    let pairs: Vec<_> = groups
        .iter()
        .map(|g| {
            let v = if g.stem == "R" {
                Verdict::Reject
            } else {
                Verdict::Keep
            };
            (g.id, v)
        })
        .collect();
    let p = plan(&groups, &decide(&pairs), &MOVE_SAME, &Existing::new()).unwrap();
    assert_eq!(
        targets(&p),
        vec!["A/R.JPG", "B/R.JPG", "A/K.JPG", "B/K.JPG"]
    );
    assert_eq!(
        p.ops.iter().map(|o| o.ord).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
}

// --- Property-style tests (docs/testing.md §3.1) -------------------------------------------

/// xorshift64*: small, seeded, deterministic.
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

/// Cases per property: `CULLA_PROP_CASES` if set (10,000 for a long run), else 300.
fn cases() -> u64 {
    std::env::var("CULLA_PROP_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300)
}

const FOLDERS: &[&str] = &[
    "",
    "100NIKON",
    "101NIKON",
    "100NIKON/sub",
    "Été/Ünï",
    "a/b/c/d",
];
const STEMS: &[&str] = &["DSC_0001", "DSC_0001_1", "dsc_0002", "Été", "X.edit", "X"];

/// A random source tree: per folder a few shots, each a JPEG, a NEF or both, with random
/// sidecars. Returns the file paths.
fn random_tree(rng: &mut Rng) -> Vec<String> {
    let mut paths = Vec::new();
    let mut used = BTreeSet::new();
    for _ in 0..1 + rng.below(6) {
        let folder = *rng.pick(FOLDERS);
        let stem = *rng.pick(STEMS);
        if !used.insert(fold(&format!("{folder}/{stem}"))) {
            continue;
        }
        let at = |name: String| {
            if folder.is_empty() {
                name
            } else {
                format!("{folder}/{name}")
            }
        };
        let jpg = match rng.below(3) {
            0 => None,
            _ => Some(*rng.pick(&["JPG", "jpg", "JPEG"])),
        };
        let nef = if jpg.is_none() || rng.chance(60) {
            Some(*rng.pick(&["NEF", "nef"]))
        } else {
            None
        };
        for ext in jpg.iter().chain(nef.iter()) {
            paths.push(at(format!("{stem}.{ext}")));
            if rng.chance(30) {
                paths.push(at(format!("{stem}.{ext}.xmp")));
            }
            if rng.chance(20) {
                paths.push(at(format!("{stem}.{ext}.pp3")));
            }
            if rng.chance(30) {
                paths.push(at(format!("NKSC_PARAM/{stem}.{ext}.nksc")));
            }
        }
        if rng.chance(40) {
            paths.push(at(format!("{stem}.xmp")));
        }
        if rng.chance(20) {
            paths.push(at(format!("{stem}.pp3")));
        }
    }
    paths
}

struct Case {
    groups: Vec<PlanGroup>,
    decisions: Decisions,
    settings: Settings,
    existing: Existing,
}

fn random_case(rng: &mut Rng) -> Case {
    let paths = random_tree(rng);
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let mut groups = scan(&refs);
    for g in &mut groups {
        if rng.chance(5) {
            g.status = GroupStatus::Ambiguous;
        }
        g.applied = rng.chance(5);
        g.locked = rng.chance(5);
        for m in &mut g.members {
            m.present = !rng.chance(10);
            m.read_only = rng.chance(8);
        }
    }
    let mut pairs = Vec::new();
    for g in &groups {
        match rng.below(3) {
            0 => {}
            1 => pairs.push((g.id, Verdict::Keep)),
            _ => pairs.push((g.id, Verdict::Reject)),
        }
    }
    let settings = Settings {
        raw_follows: rng.chance(50),
        keep: Some(KeepTo {
            mode: if rng.chance(50) {
                KeepMode::Move
            } else {
                KeepMode::Copy
            },
            same_volume: rng.chance(50),
        }),
    };
    // Existing entries where targets would land, with suffixes, other case, folders, and the
    // odd file where a folder should be.
    let mut existing = Existing::new();
    for g in &groups {
        for m in &g.members {
            for root in [Root::Dest, Root::Rejected] {
                if !rng.chance(25) {
                    continue;
                }
                let n = *rng.pick(&[0, 0, 1, 2]);
                let mut name = target_name(&m.name, &g.stem, n).unwrap();
                if rng.chance(30) {
                    name = name.to_lowercase();
                }
                let mut path: Vec<&str> = m.folder.iter().map(String::as_str).collect();
                path.push(&name);
                if rng.chance(15) {
                    existing.add_dir(root, &path);
                } else {
                    existing.add_file(root, &path);
                }
                if rng.chance(3) && !m.folder.is_empty() {
                    existing.add_file(root, &[m.folder[0].as_str()]);
                }
            }
        }
    }
    Case {
        groups,
        decisions: decide(&pairs),
        settings,
        existing,
    }
}

/// §7.1, written again from the spec text: which members of a decided group move.
fn expected_members(g: &PlanGroup, raw_follows: bool) -> BTreeSet<MemberId> {
    let has_nef = g.members.iter().any(|m| m.role == Role::Raw);
    let photo = |role: Role| {
        g.members
            .iter()
            .find(|m| m.role == role)
            .is_some_and(|m| m.present && (role == Role::Primary || raw_follows))
    };
    g.members
        .iter()
        .filter(|m| m.present)
        .filter(|m| match m.role {
            Role::Primary => true,
            Role::Raw => raw_follows,
            Role::Sidecar {
                describes: Describes::Group,
                ..
            } => photo(if has_nef { Role::Raw } else { Role::Primary }),
            Role::Sidecar {
                describes: Describes::Primary,
                ..
            } => photo(Role::Primary),
            Role::Sidecar {
                describes: Describes::Raw,
                ..
            } => photo(Role::Raw),
            Role::Sidecar { .. } => false,
        })
        .map(|m| m.id)
        .collect()
}

fn check_invariants(c: &Case, p: &Plan, case: u64) {
    // Ords in sequence; rejects before keeps.
    for (i, o) in p.ops.iter().enumerate() {
        assert_eq!(o.ord as usize, i, "case {case}");
    }
    let first_keep = p.ops.iter().position(|o| o.action == Verdict::Keep);
    if let Some(k) = first_keep {
        assert!(
            p.ops[k..].iter().all(|o| o.action == Verdict::Keep),
            "case {case}"
        );
    }
    // INV-06: no two operations share a folded target.
    let mut keys = BTreeSet::new();
    for o in &p.ops {
        assert_eq!(o.dst_key, fold(&o.dst.join("\\")));
        assert!(
            keys.insert((o.root, o.dst_key.clone())),
            "case {case}: {o:?}"
        );
    }
    // Never overwrite: no target exists, no target folder is blocked by a file.
    for o in &p.ops {
        assert!(!c.existing.is_taken(o.root, &o.dst), "case {case}: {o:?}");
        assert!(
            !c.existing.is_blocked(o.root, &o.dst[..o.dst.len() - 1]),
            "case {case}: {o:?}"
        );
    }
    let held: BTreeMap<GroupId, Held> = p.held.iter().copied().collect();
    let planned: BTreeMap<GroupId, &PlannedGroup> = p.groups.iter().map(|g| (g.group, g)).collect();
    for g in &c.groups {
        let state = c.decisions.state(g.id);
        let ops: Vec<&PlannedOp> = p.ops.iter().filter(|o| o.group == g.id).collect();
        // INV-04: undecided groups appear nowhere.
        if state == GroupState::Undecided {
            assert!(ops.is_empty() && !held.contains_key(&g.id), "case {case}");
            assert!(p.missing.iter().all(|(id, _)| *id != g.id));
            assert!(!planned.contains_key(&g.id));
            continue;
        }
        let blocked = if g.status == GroupStatus::Ambiguous {
            Some(Held::Ambiguous)
        } else if g.applied {
            Some(Held::Applied)
        } else if g.locked {
            Some(Held::Locked)
        } else {
            None
        };
        if let Some(r) = blocked {
            assert!(ops.is_empty(), "case {case}");
            assert_eq!(held.get(&g.id), Some(&r), "case {case}");
            continue;
        }
        let want = expected_members(g, c.settings.raw_follows);
        let Some(pg) = planned.get(&g.id) else {
            assert!(ops.is_empty());
            let r = held[&g.id];
            assert!(
                matches!(r, Held::NoFreeSuffix | Held::TargetFolderBlocked)
                    || (r == Held::NothingPresent && want.is_empty()),
                "case {case}: {r:?}"
            );
            continue;
        };
        // The whole group moves (INV-09): exactly the expected members, one root, one kind,
        // one suffix (INV-07), mirrored folders (INV-05).
        let got: BTreeSet<MemberId> = ops.iter().map(|o| o.member).collect();
        assert_eq!(got, want, "case {case}");
        assert_eq!(got.len(), ops.len());
        let root = if state == GroupState::Kept {
            Root::Dest
        } else {
            Root::Rejected
        };
        let protected = ops
            .iter()
            .any(|o| g.members.iter().any(|m| m.id == o.member && m.read_only));
        let keep = c.settings.keep.unwrap();
        let base = match (state, keep.mode, keep.same_volume) {
            (GroupState::Rejected, ..) => OpKind::Rename,
            (_, KeepMode::Copy, _) => OpKind::Copy,
            (_, KeepMode::Move, true) => OpKind::Rename,
            (_, KeepMode::Move, false) => OpKind::XMove,
        };
        let kind = if protected { OpKind::Copy } else { base };
        assert_eq!(pg.kind, kind, "case {case}");
        assert_eq!(pg.protected, protected && base != OpKind::Copy);
        for o in &ops {
            let m = g.members.iter().find(|m| m.id == o.member).unwrap();
            assert!(m.present);
            assert_eq!(o.root, root);
            assert_eq!(o.kind, kind);
            assert_eq!(o.src, [m.folder.clone(), vec![m.name.clone()]].concat());
            let name = target_name(&m.name, &g.stem, pg.suffix).unwrap();
            assert_eq!(
                o.dst,
                [m.folder.clone(), vec![name]].concat(),
                "case {case}"
            );
            assert_eq!(o.expected_size, m.size);
            // INV-08: with RAW follows off, nothing of the NEF's moves.
            if !c.settings.raw_follows && g.members.iter().any(|x| x.role == Role::Raw) {
                assert!(
                    !matches!(
                        m.role,
                        Role::Raw
                            | Role::Sidecar {
                                describes: Describes::Raw | Describes::Group,
                                ..
                            }
                    ),
                    "case {case}: {m:?}"
                );
            }
        }
        // The suffix is the smallest that works (§7.3).
        let first = ops[0].ord as usize;
        let earlier: BTreeSet<(Root, String)> = p.ops[..first]
            .iter()
            .map(|o| (o.root, o.dst_key.clone()))
            .collect();
        for n in 0..pg.suffix {
            let clash = ops.iter().any(|o| {
                let m = g.members.iter().find(|m| m.id == o.member).unwrap();
                let path = [
                    m.folder.clone(),
                    vec![target_name(&m.name, &g.stem, n).unwrap()],
                ]
                .concat();
                c.existing.is_taken(root, &path) || earlier.contains(&(root, key(&path)))
            });
            assert!(clash, "case {case}: suffix {n} was free");
        }
    }
    // Missing members that would have moved are listed, for groups past the held checks.
    let mut want_missing = BTreeSet::new();
    for g in &c.groups {
        let open = g.status == GroupStatus::Ok && !g.applied && !g.locked;
        if c.decisions.state(g.id) != GroupState::Undecided && open {
            for m in expected_missing(g, c.settings.raw_follows) {
                want_missing.insert((g.id, m));
            }
        }
    }
    assert_eq!(
        p.missing.iter().copied().collect::<BTreeSet<_>>(),
        want_missing,
        "case {case}"
    );
    let copy: u64 = p
        .ops
        .iter()
        .filter(|o| o.kind == OpKind::Copy)
        .map(|o| o.expected_size)
        .sum();
    let xmove: u64 = p
        .ops
        .iter()
        .filter(|o| o.kind == OpKind::XMove)
        .map(|o| o.expected_size)
        .sum();
    assert_eq!((p.bytes_copy, p.bytes_xmove), (copy, xmove));
}

/// Missing members that would have moved had they been present (only their own presence
/// flipped: a sidecar of a missing photo would not move anyway).
fn expected_missing(g: &PlanGroup, raw_follows: bool) -> BTreeSet<MemberId> {
    g.members
        .iter()
        .filter(|m| !m.present)
        .filter(|m| {
            let mut x = g.clone();
            for y in &mut x.members {
                if y.id == m.id {
                    y.present = true;
                }
            }
            expected_members(&x, raw_follows).contains(&m.id)
        })
        .map(|m| m.id)
        .collect()
}

/// INV-04 to INV-09: every invariant of §3.1 over random trees, decisions, settings and
/// collisions.
#[test]
fn prop_plan_invariants() {
    // How often each situation came up, so a generator change cannot make the test vacuous.
    let mut seen: BTreeMap<&str, u64> = BTreeMap::new();
    for case in 0..cases() {
        let mut rng = Rng::new(case);
        let c = random_case(&mut rng);
        let p = plan(&c.groups, &c.decisions, &c.settings, &c.existing).unwrap();
        check_invariants(&c, &p, case);
        let mut count = |k| *seen.entry(k).or_default() += 1;
        if !p.ops.is_empty() {
            count("ops");
        }
        if p.groups.iter().any(|g| g.suffix > 0) {
            count("suffix");
        }
        if p.groups.iter().any(|g| g.suffix > 1) {
            count("suffix > 1");
        }
        if p.groups.iter().any(|g| g.protected) {
            count("protected");
        }
        if !p.missing.is_empty() {
            count("missing");
        }
        for (_, h) in &p.held {
            count(match h {
                Held::Ambiguous => "ambiguous",
                Held::Applied => "applied",
                Held::Locked => "locked",
                Held::NothingPresent => "nothing present",
                Held::TargetFolderBlocked => "blocked",
                Held::NoFreeSuffix | Held::NameMismatch => "other",
            });
        }
        for k in [OpKind::Rename, OpKind::Copy, OpKind::XMove] {
            if p.ops.iter().any(|o| o.kind == k) {
                count(k.as_str());
            }
        }
    }
    for k in [
        "ops",
        "suffix",
        "suffix > 1",
        "protected",
        "missing",
        "ambiguous",
        "applied",
        "locked",
        "nothing present",
        "blocked",
        "rename",
        "copy",
        "xmove",
    ] {
        assert!(
            seen.get(k).copied().unwrap_or(0) > 0,
            "never saw {k}: {seen:?}"
        );
    }
}

/// INV-24: same inputs, same plan, whatever the order of groups and members.
#[test]
fn prop_inv24_plan_is_deterministic() {
    for case in 0..cases() {
        let mut rng = Rng::new(case ^ 0xD5);
        let c = random_case(&mut rng);
        let p = plan(&c.groups, &c.decisions, &c.settings, &c.existing).unwrap();
        assert_eq!(
            plan(&c.groups, &c.decisions, &c.settings, &c.existing).unwrap(),
            p
        );
        let mut shuffled = c.groups.clone();
        rng.shuffle(&mut shuffled);
        for g in &mut shuffled {
            rng.shuffle(&mut g.members);
        }
        assert_eq!(
            plan(&shuffled, &c.decisions, &c.settings, &c.existing).unwrap(),
            p,
            "case {case}"
        );
    }
}

/// The long run: `cargo test -p culla-core -- --ignored prop_plan_10k`.
#[test]
#[ignore = "long run (10,000 cases); CI runs the default count"]
fn prop_plan_10k() {
    for case in 0..10_000 {
        let mut rng = Rng::new(case);
        let c = random_case(&mut rng);
        let p = plan(&c.groups, &c.decisions, &c.settings, &c.existing).unwrap();
        check_invariants(&c, &p, case);
    }
}
