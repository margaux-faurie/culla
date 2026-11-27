//! Preflight: the checks P1 to P17 of design/file-engine.md §8 over a plan and a snapshot of
//! facts (story US-16; INV-03, INV-05, INV-06, INV-10).
//!
//! [`preflight`] is pure. Preflight is read-only on user volumes (§8): the adapter gathers the
//! facts first, with lookups that open nothing for writing and create nothing, and hands them
//! over as plain data ([`Facts`]): the mounted volumes and their flags, free space and cluster
//! size per volume, what each planned source looks like now (size, mtime, attributes, a lock
//! held by another process), the attributes of the folders between each root and the files
//! that exist and of the folders above each root ([`FolderFact`]), the source and destination
//! roots, whether the destination is in a sync root, a fresh snapshot of the target roots
//! ([`Existing`]), and whether the preview pipeline is idle. Facts fail closed: a folder that
//! must exist and has no fact, or one the adapter could not read, holds its group or blocks,
//! and a fact the constructor does not ask for defaults to the answer that stops apply. It runs at review and again in the instant before the
//! first operation, on the same facts gathered afresh.
//!
//! The result ([`Report`]) sorts every finding into one of four kinds, as §8 says:
//!
//! - [`Blocker`]: apply cannot start (P1 to P7, P16). A P7 finding is a bug in `plan()` or the
//!   store, logged as such.
//! - [`Hold`]: the group is held back and listed; the rest of the plan can run (P8, P9, P10,
//!   P14). [`Report::runnable`] gives the operations that remain.
//! - [`Replan`]: the plan no longer matches the disk and must be built again before confirm
//!   (P11, P13, and a destination whose volume relation to the source changed since planning).
//! - [`Warning`]: listed in the review, apply is allowed (P12, P15, P17).
//!
//! A group gets at most one hold: its first member in plan order that is held, with that
//! member's first rule in rule order (P8, P9, P10, P14); replans are found only for groups that are not held, and every volume-level
//! check (P3, P4, P5, P6, P17) counts only the operations of groups that are not held, so a held
//! group never blocks the others. The destination volume is still bound (P2) when any operation
//! keeps into it, held or not, and P7 runs over every operation, held ones included. These
//! orderings, the margin of P5, the length bound of P8 for
//! partials and the bound of P15 are proposed by the agent and await Margaux (SPEC §10).

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::decisions::GroupId;
use crate::grouping::Attributes;
use crate::names::{fold, validate_component};
use crate::plan::{Existing, MemberId, OpKind, Plan, PlannedOp, Root};
use crate::volume::{
    Binding, Source, VolumeGuidPath, VolumeId, VolumeInfo, VolumePathError, VolumeRelation, bind,
    check_volume, relation,
};

/// The `_rejected` folder's name, as `plan()` and the scan use it.
pub const REJECTED: &str = "_rejected";

/// Longest path Windows takes with the `\\?\` prefix, in UTF-16 units (P8).
pub const MAX_PATH_UTF16: usize = 32_767;

/// Longest path, in UTF-16 units, that fits `MAX_PATH` (260) with its terminating NUL. Longer
/// target paths get a warning (P15): Explorer and some apps may fail to open them.
pub const MAX_PATH_SHORT: usize = 259;

/// Length of `\\?\Volume{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}\` in UTF-16 units.
pub const GUID_PREFIX_UTF16: usize = 49;

/// Worst-case length, in UTF-16 units, of what a partial adds to its target name (§7.2):
/// `.<op id>-<attempt>.culla-partial`, with a 19-digit op id (`i64::MAX`) and a 10-digit
/// attempt (`u32::MAX`). P8 checks the partial name at this length, since the op id is not
/// known at review.
pub const PARTIAL_EXTRA_UTF16: usize = 1 + 19 + 1 + 10 + ".culla-partial".len();

/// The largest file FAT32 can hold: 4 GiB − 1 byte (P14).
pub const FAT32_MAX_FILE: u64 = u32::MAX as u64;

/// The smallest free-space margin P5 keeps on a volume: 1 GiB.
pub const MIN_MARGIN: u64 = 1 << 30;

/// The tree a folder fact belongs to. `_rejected` is under the source root, so its folders are
/// `Source` paths starting with [`REJECTED`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tree {
    /// Paths from the source root.
    Source,
    /// Paths from the destination root.
    Dest,
    /// Paths from the root of this volume: the folders above the source and destination roots.
    Volume(VolumeId),
}

/// How a folder looks, through a lookup that does not follow links.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderFact {
    /// It exists, with these attributes.
    Exists(Attributes),
    /// The lookup failed (access denied, an I/O error…): how it looks is unknown, so it holds
    /// or blocks like a link would.
    Unreadable,
}

/// What is wrong with a folder on the way to a root, a source or a target (P9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FolderIssue {
    /// A link (symlink, junction, mount point).
    Link,
    /// A cloud placeholder.
    Placeholder,
    /// It must exist but has no fact, or it could not be read.
    Unknown,
}

fn attrs_issue(a: &Attributes) -> Option<FolderIssue> {
    if a.reparse_point {
        Some(FolderIssue::Link)
    } else if a.cloud_placeholder {
        Some(FolderIssue::Placeholder)
    } else {
        None
    }
}

/// The issue of a folder that must exist: no fact is an issue too.
fn issue_of(fact: Option<&FolderFact>) -> Option<FolderIssue> {
    match fact {
        Some(FolderFact::Exists(a)) => attrs_issue(a),
        Some(FolderFact::Unreadable) | None => Some(FolderIssue::Unknown),
    }
}

/// How a planned source looks now, through a lookup that does not follow links.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceFact {
    /// A regular file (not a folder, not a device).
    pub file: bool,
    /// Size in bytes.
    pub size: u64,
    /// Modification time.
    pub mtime_ns: Option<i64>,
    /// Attributes (read-only, reparse point, placeholder…).
    pub attrs: Attributes,
    /// Opening it with `share_mode(FILE_SHARE_READ)` and no write or delete sharing failed with
    /// a sharing or lock violation (P12).
    pub locked: bool,
}

/// What is at the destination root (P2, P6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DestRoot {
    /// A folder, with its attributes.
    Folder(Attributes),
    /// Something that is not a folder.
    NotAFolder,
    /// Nothing, but its parent folder exists, with these attributes (`make_dirs` will create
    /// the root). A parent that is a link or a placeholder blocks.
    ParentExists(Attributes),
    /// Neither it nor its parent.
    Missing,
    /// The lookup failed: blocks.
    Unreadable,
}

/// Free space on one volume (P5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Space {
    /// Bytes free for the user.
    pub free: u64,
    /// The cluster size, in bytes: each file takes a whole number of clusters.
    pub cluster: u64,
}

/// How modification times compare between a scan and a later look at the same volume (§6.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeRule {
    /// NTFS and others: equal.
    Exact,
    /// FAT32 and exFAT: equal, or exactly one hour apart (a local timestamp read across a DST
    /// change).
    DstHour,
}

impl TimeRule {
    /// The rule for a file system name: [`TimeRule::DstHour`] for `FAT`, `FAT32` and `exFAT`
    /// (any case), [`TimeRule::Exact`] otherwise.
    pub fn for_fs(fs_name: &str) -> Self {
        if is_fat_family(fs_name) {
            Self::DstHour
        } else {
            Self::Exact
        }
    }

    /// The two times match under this rule. An unknown time matches nothing, not even another
    /// unknown one: a change cannot be ruled out, so P10 holds the group (fails closed).
    pub fn matches(self, a: Option<i64>, b: Option<i64>) -> bool {
        const HOUR_NS: i64 = 3_600 * 1_000_000_000;
        match (a, b) {
            (Some(a), Some(b)) => {
                a == b || (self == Self::DstHour && a.abs_diff(b) == HOUR_NS.unsigned_abs())
            }
            _ => false,
        }
    }
}

fn is_fat_family(fs: &str) -> bool {
    ["FAT", "FAT32", "exFAT"]
        .iter()
        .any(|f| f.eq_ignore_ascii_case(fs))
}

fn is_fat32(fs: &str) -> bool {
    ["FAT", "FAT32"].iter().any(|f| f.eq_ignore_ascii_case(fs))
}

fn key(parts: &[String]) -> String {
    fold(&parts.join("\\"))
}

fn utf16(s: &str) -> usize {
    s.encode_utf16().count()
}

/// The facts the adapter gathers for one preflight run. Paths are components from a root,
/// compared folded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facts {
    /// Every mounted volume, as enumerated (P1, P2).
    pub mounted: Vec<VolumeInfo>,
    /// Free space per volume, by GUID path (P5). A volume that needs space and has no entry
    /// blocks apply.
    pub space: BTreeMap<VolumeGuidPath, Space>,
    /// The source root (P9: no link or placeholder at the root; unreadable blocks).
    pub source_root: FolderFact,
    /// What is at the destination root (ignored when the plan keeps nothing).
    pub dest_root: DestRoot,
    /// The destination is inside a folder that a cloud sync client registered (P17). `None`:
    /// the lookup failed, which gives its own warning.
    pub dest_in_sync_root: Option<bool>,
    /// Culla's preview pipeline is stopped and holds no handle (P16). `false` until the
    /// pipeline says so.
    pub preview_idle: bool,
    /// A fresh snapshot of the entries under the target roots (P11).
    pub targets: Existing,
    sources: BTreeMap<String, SourceFact>,
    folders: BTreeMap<(Tree, String), FolderFact>,
}

impl Facts {
    /// The two root facts, which every run needs; no volumes, no space, no sources, no
    /// folders; the sync-root lookup not done; the preview pipeline not known to be idle.
    pub fn new(source_root: FolderFact, dest_root: DestRoot) -> Self {
        Self {
            mounted: Vec::new(),
            space: BTreeMap::new(),
            source_root,
            dest_root,
            dest_in_sync_root: None,
            preview_idle: false,
            targets: Existing::new(),
            sources: BTreeMap::new(),
            folders: BTreeMap::new(),
        }
    }

    /// The source file at `path` (from the source root) exists and looks like `fact`. A source
    /// with no fact is missing.
    pub fn add_source(&mut self, path: &[String], fact: SourceFact) {
        self.sources.insert(key(path), fact);
    }

    /// The folder at `path` under `tree` exists with these attributes.
    ///
    /// A folder with no fact is taken as absent only where that is possible: a target folder
    /// the snapshot ([`Facts::targets`]) does not show (`make_dirs` creates it at apply). A
    /// folder between the source root and a planned source, a target folder the snapshot
    /// shows, and a folder above a root must exist, so no fact holds the group or blocks.
    pub fn add_folder(&mut self, tree: Tree, path: &[String], attrs: Attributes) {
        self.folders
            .insert((tree, key(path)), FolderFact::Exists(attrs));
    }

    /// The lookup of the folder at `path` under `tree` failed: it holds or blocks.
    pub fn add_unreadable_folder(&mut self, tree: Tree, path: &[String]) {
        self.folders
            .insert((tree, key(path)), FolderFact::Unreadable);
    }

    fn source(&self, path: &[String]) -> Option<&SourceFact> {
        self.sources.get(&key(path))
    }

    fn folder(&self, tree: Tree, path: &[String]) -> Option<&FolderFact> {
        self.folders.get(&(tree, key(path)))
    }

    /// The first issue among the folders above `root`, from the top (the volume's own root is
    /// not looked at).
    fn ancestor_issue(&self, root: &Source) -> Option<FolderIssue> {
        let c = root.components();
        (1..c.len()).find_map(|n| issue_of(self.folder(Tree::Volume(root.volume()), &c[..n])))
    }
}

/// The plan's roots: the source folder, and the destination folder when one is chosen. Both
/// are a volume and components from its root ([`Source`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Roots {
    /// The source folder.
    pub source: Source,
    /// The destination folder.
    pub dest: Option<Source>,
}

/// Why a volume did not bind (P1, P2; §6.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unbound {
    /// No mounted volume has the serial: "Insert the card".
    Missing,
    /// This many mounted volumes have it: the user picks one.
    Ambiguous(usize),
}

/// A finding that stops apply from starting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Blocker {
    /// P1: the source volume is not bound to exactly one mounted volume.
    SourceVolume(Unbound),
    /// P1: the source volume is of a type v1 refuses (network, no root, serial 0).
    SourceVolumeRefused(VolumePathError),
    /// P9 at the root: the source folder is now a link or a cloud placeholder.
    SourceRootLink,
    /// P9 at the root: the source folder could not be looked at.
    SourceRootUnreadable,
    /// P9 above the root: a folder above the source folder is a link or a placeholder, could
    /// not be read, or has no fact.
    SourceAncestor(FolderIssue),
    /// P2: the plan keeps groups but no destination is given (a bug in the caller).
    NoDestination,
    /// P2: the destination volume is not bound to exactly one mounted volume.
    DestVolume(Unbound),
    /// P2: neither the destination folder nor its parent exists.
    DestRootMissing,
    /// P2: the destination folder could not be looked at.
    DestRootUnreadable,
    /// P6 above the root: a folder above the destination folder (its parent included) is a
    /// link or a placeholder, could not be read, or has no fact.
    DestAncestor(FolderIssue),
    /// P3: the source volume is read-only (the SD card's lock switch) and the plan renames,
    /// moves or copies into `_rejected` on it.
    SourceReadOnly,
    /// P4: the destination volume is read-only.
    DestReadOnly,
    /// P5: not enough free space on a volume for the copies and cross-volume moves into it.
    NoSpace {
        /// The volume.
        volume: VolumeId,
        /// Bytes the plan writes there, rounded up to whole clusters, plus the margin.
        needed: u64,
        /// Bytes free.
        free: u64,
    },
    /// P5: the adapter gave no free space for a volume the plan writes to.
    SpaceUnknown(VolumeId),
    /// P6: the destination is the source folder.
    DestIsSource,
    /// P6: the destination is inside a `_rejected` folder.
    DestInRejected,
    /// P6: the destination folder is a cloud placeholder.
    DestPlaceholder,
    /// P6: the destination folder is a link (symlink, junction, mount point).
    DestLink,
    /// P6: the destination is not a folder.
    DestNotAFolder,
    /// P6: the destination volume is of a type v1 refuses.
    DestVolumeRefused(VolumePathError),
    /// P7: an operation's target is not a valid path under its root, equals a source, or is
    /// shared with another operation. A bug in `plan()` or the store.
    Inconsistent {
        /// The operation's position in the plan.
        ord: u32,
        /// What is wrong.
        why: Inconsistency,
    },
    /// P16: the preview pipeline still holds handles; apply waits for it.
    PreviewBusy,
}

/// What P7 found wrong with an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inconsistency {
    /// The target path is empty, or a component is not a valid name (`..`, a separator…).
    BadTarget,
    /// The target is the path of a planned source.
    TargetIsSource,
    /// Another operation has the same target (folded).
    DuplicateTarget,
}

/// Why a group is held back (it keeps no operations and is listed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Hold {
    /// P8: a target or partial path, or one of its components, is too long for the target
    /// volume, or a name is not valid on it.
    PathTooLong {
        /// The member.
        member: MemberId,
    },
    /// P9: a source, one of its folders, or a target folder is a link (symlink, junction,
    /// mount point).
    Link {
        /// The member.
        member: MemberId,
    },
    /// P9: a source, one of its folders, or a target folder is a cloud placeholder.
    Placeholder {
        /// The member.
        member: MemberId,
    },
    /// P9: a folder of a source, or a target folder the snapshot shows, has no fact or could
    /// not be read, so links and placeholders cannot be ruled out.
    FolderUnknown {
        /// The member.
        member: MemberId,
    },
    /// P10: a source is missing, no longer a file, or its size or mtime changed: "changed since
    /// scan, rescan".
    ChangedSinceScan {
        /// The member.
        member: MemberId,
    },
    /// P14: a file over 4 GiB − 1 byte would be copied to a FAT32 volume.
    TooLargeForFat32 {
        /// The member.
        member: MemberId,
    },
}

/// Why the plan must be built again (the review then shows the new plan before confirm).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Replan {
    /// P11: a target is no longer free, or a file now sits where a target folder would be.
    TargetTaken {
        /// The member.
        member: MemberId,
    },
    /// P13: a source of a group planned as `rename` or `xmove` is now read-only.
    ReadOnlyAppeared {
        /// The member.
        member: MemberId,
    },
    /// §6.3 step 4: a kept group's kind assumed the destination on the source's volume (or
    /// not), and the bound volumes now say otherwise.
    VolumeRelationChanged,
}

/// A finding that is listed but does not stop apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Warning {
    /// P12: another process holds the file; apply retries with backoff (§9.4).
    Locked {
        /// The group.
        group: GroupId,
        /// The member.
        member: MemberId,
    },
    /// P15: the target path is longer than 259 characters in its drive-letter form.
    LongPath {
        /// The group.
        group: GroupId,
        /// The member.
        member: MemberId,
        /// Its length in UTF-16 units.
        len: usize,
    },
    /// P17: the destination is inside a cloud-synced folder; the sync client may upload,
    /// change or move the files during and after the apply.
    CloudSynced,
    /// P17: whether the destination is inside a cloud-synced folder could not be found out.
    SyncRootUnknown,
}

/// The result of a preflight run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Apply cannot start while any is listed, in rule order.
    pub blockers: Vec<Blocker>,
    /// Groups held back, by group id, each with its first reason.
    pub held: Vec<(GroupId, Hold)>,
    /// Groups whose plan no longer matches, by group id, each with its first reason.
    pub replan: Vec<(GroupId, Replan)>,
    /// Listed, apply allowed. Group warnings in plan order, then the plan-wide ones.
    pub warnings: Vec<Warning>,
}

impl Report {
    /// Apply may start: nothing blocks and nothing needs a new plan. Held groups do not stop
    /// the others.
    pub fn can_apply(&self) -> bool {
        self.blockers.is_empty() && self.replan.is_empty()
    }

    /// The group is held back.
    pub fn is_held(&self, group: GroupId) -> bool {
        self.held.iter().any(|(g, _)| *g == group)
    }

    /// The operations that remain once the held groups are taken out, in plan order.
    pub fn runnable<'a>(&self, plan: &'a Plan) -> Vec<&'a PlannedOp> {
        plan.ops.iter().filter(|o| !self.is_held(o.group)).collect()
    }
}

/// A volume as preflight sees it: bound or not.
struct Vol<'a> {
    id: VolumeId,
    info: Option<&'a VolumeInfo>,
}

impl Vol<'_> {
    fn prefix_len(&self) -> usize {
        self.info
            .map_or(GUID_PREFIX_UTF16, |i| utf16(i.guid_path.as_str()))
    }

    fn max_component(&self) -> usize {
        self.info
            .map_or(crate::names::MAX_COMPONENT_UTF16, |i| {
                usize::try_from(i.max_component_len).unwrap_or(usize::MAX)
            })
            .min(crate::names::MAX_COMPONENT_UTF16)
    }

    fn fs_name(&self) -> &str {
        self.info.map_or("", |i| i.fs_name.as_str())
    }
}

fn bound(id: VolumeId, mounted: &[VolumeInfo]) -> Result<&VolumeInfo, Unbound> {
    match bind(id, mounted) {
        Binding::Bound(v) => Ok(v),
        Binding::Missing => Err(Unbound::Missing),
        Binding::Ambiguous(vs) => Err(Unbound::Ambiguous(vs.len())),
    }
}

/// Rounds `size` up to whole clusters.
fn on_disk(size: u64, cluster: u64) -> u64 {
    let c = cluster.max(1);
    size.div_ceil(c).saturating_mul(c)
}

/// The P5 margin for `needed` bytes: max(1 GiB, 2 % of them) (proposed, SPEC §10).
pub fn margin(needed: u64) -> u64 {
    MIN_MARGIN.max(needed / 50)
}

/// Runs the checks of §8 on `plan`. Pure: everything it knows about the disk is in `facts`.
pub fn preflight(plan: &Plan, roots: &Roots, facts: &Facts) -> Report {
    let mut r = Report::default();

    // P1: the source volume.
    let src_info = match bound(roots.source.volume(), &facts.mounted) {
        Ok(v) => {
            if let Err(e) = check_volume(v) {
                r.blockers.push(Blocker::SourceVolumeRefused(e));
            }
            Some(v)
        }
        Err(u) => {
            r.blockers.push(Blocker::SourceVolume(u));
            None
        }
    };
    match facts.source_root {
        FolderFact::Exists(a) => {
            if attrs_issue(&a).is_some() {
                r.blockers.push(Blocker::SourceRootLink);
            }
        }
        FolderFact::Unreadable => r.blockers.push(Blocker::SourceRootUnreadable),
    }
    if let Some(i) = facts.ancestor_issue(&roots.source) {
        r.blockers.push(Blocker::SourceAncestor(i));
    }
    let src = Vol {
        id: roots.source.volume(),
        info: src_info,
    };
    let keeps = plan.ops.iter().any(|o| o.root == Root::Dest);
    let dst = match (&roots.dest, keeps) {
        (Some(d), true) => {
            let info = match bound(d.volume(), &facts.mounted) {
                Ok(v) => Some(v),
                Err(u) => {
                    r.blockers.push(Blocker::DestVolume(u));
                    None
                }
            };
            Some((
                d,
                Vol {
                    id: d.volume(),
                    info,
                },
            ))
        }
        (None, true) => {
            r.blockers.push(Blocker::NoDestination);
            None
        }
        (_, false) => None,
    };

    // Per group: holds (P8, P9, P10, P14), then replans (P11, P13) and warnings (P12, P15).
    let mut held: BTreeMap<GroupId, Hold> = BTreeMap::new();
    let mut replan: BTreeMap<GroupId, Replan> = BTreeMap::new();
    let mut group_warnings = Vec::new();
    let same_volume = match (&dst, src.info) {
        (Some((_, Vol { info: Some(d), .. })), Some(s)) => Some(relation(s, d)),
        _ => None,
    };
    for o in &plan.ops {
        if held.contains_key(&o.group) {
            continue;
        }
        let (root_parts, vol): (Vec<String>, &Vol<'_>) = match o.root {
            Root::Dest => match &dst {
                Some((d, v)) => (d.components().to_vec(), v),
                None => continue,
            },
            Root::Rejected => {
                let mut p = roots.source.components().to_vec();
                p.push(REJECTED.to_owned());
                (p, &src)
            }
        };
        if let Some(h) = hold_for(o, &root_parts, vol, &src, facts) {
            held.insert(o.group, h);
            continue;
        }
        let fact = facts.source(&o.src);
        if let std::collections::btree_map::Entry::Vacant(e) = replan.entry(o.group) {
            let folder = &o.dst[..o.dst.len().saturating_sub(1)];
            let stale = o.root == Root::Dest
                && matches!(
                    (same_volume, o.kind),
                    (Some(VolumeRelation::Same), OpKind::XMove)
                        | (Some(VolumeRelation::Different), OpKind::Rename)
                );
            if facts.targets.is_taken(o.root, &o.dst) || facts.targets.is_blocked(o.root, folder) {
                e.insert(Replan::TargetTaken { member: o.member });
            } else if fact.is_some_and(|f| f.attrs.read_only)
                && matches!(o.kind, OpKind::Rename | OpKind::XMove)
            {
                e.insert(Replan::ReadOnlyAppeared { member: o.member });
            } else if stale {
                e.insert(Replan::VolumeRelationChanged);
            }
        }
        if fact.is_some_and(|f| f.locked) {
            group_warnings.push(Warning::Locked {
                group: o.group,
                member: o.member,
            });
        }
        if let Some(info) = vol.info {
            let mut parts = root_parts.clone();
            parts.extend(o.dst.iter().cloned());
            let len = utf16(&info.mount_point) + utf16(&parts.join("\\"));
            if len > MAX_PATH_SHORT {
                group_warnings.push(Warning::LongPath {
                    group: o.group,
                    member: o.member,
                    len,
                });
            }
        }
    }
    // Warnings of held groups are dropped: they will not run.
    group_warnings.retain(|w| match w {
        Warning::Locked { group, .. } | Warning::LongPath { group, .. } => {
            !held.contains_key(group)
        }
        Warning::CloudSynced | Warning::SyncRootUnknown => true,
    });
    let runnable: Vec<&PlannedOp> = plan
        .ops
        .iter()
        .filter(|o| !held.contains_key(&o.group))
        .collect();

    // P2, P4, P6, P17: the destination, when a runnable operation keeps into it.
    let keeps_now = runnable.iter().any(|o| o.root == Root::Dest);
    if let (Some((d, v)), true) = (&dst, keeps_now) {
        match facts.dest_root {
            DestRoot::Folder(a) => {
                if a.reparse_point {
                    r.blockers.push(Blocker::DestLink);
                } else if a.cloud_placeholder {
                    r.blockers.push(Blocker::DestPlaceholder);
                }
            }
            DestRoot::NotAFolder => r.blockers.push(Blocker::DestNotAFolder),
            DestRoot::ParentExists(_) => {}
            DestRoot::Missing => r.blockers.push(Blocker::DestRootMissing),
            DestRoot::Unreadable => r.blockers.push(Blocker::DestRootUnreadable),
        }
        if facts.dest_root != DestRoot::Missing {
            let parent = match facts.dest_root {
                DestRoot::ParentExists(a) => attrs_issue(&a),
                _ => None,
            };
            if let Some(i) = facts.ancestor_issue(d).or(parent) {
                r.blockers.push(Blocker::DestAncestor(i));
            }
        }
        if let Some(info) = v.info {
            if info.read_only() {
                r.blockers.push(Blocker::DestReadOnly);
            }
            if let Err(e) = check_volume(info) {
                r.blockers.push(Blocker::DestVolumeRefused(e));
            }
        }
        let same = match same_volume {
            Some(rel) => rel == VolumeRelation::Same,
            None => d.volume() == roots.source.volume(),
        };
        if same && key(d.components()) == key(roots.source.components()) {
            r.blockers.push(Blocker::DestIsSource);
        }
        let rejected = fold(REJECTED);
        if d.components().iter().any(|c| fold(c) == rejected) {
            r.blockers.push(Blocker::DestInRejected);
        }
    }

    // P3: the source volume must be writable when a runnable operation changes it.
    let writes_source = runnable.iter().any(|o| {
        matches!(o.kind, OpKind::Rename | OpKind::XMove)
            || (o.kind == OpKind::Copy && o.root == Root::Rejected)
    });
    if writes_source && src.info.is_some_and(VolumeInfo::read_only) {
        r.blockers.push(Blocker::SourceReadOnly);
    }

    // P5: free space per volume written to by copies and cross-volume moves.
    let mut needs: BTreeMap<VolumeGuidPath, (VolumeId, Vec<u64>)> = BTreeMap::new();
    let mut unknown: BTreeSet<VolumeId> = BTreeSet::new();
    for o in &runnable {
        if o.kind == OpKind::Rename {
            continue;
        }
        let v = match o.root {
            Root::Dest => match &dst {
                Some((_, v)) => v,
                None => continue,
            },
            Root::Rejected => &src,
        };
        match v.info {
            Some(info) => needs
                .entry(info.guid_path.clone())
                .or_insert_with(|| (v.id, Vec::new()))
                .1
                .push(o.expected_size),
            // An unbound volume is blocked already (P1, P2).
            None => {
                unknown.insert(v.id);
            }
        }
    }
    for (guid, (id, sizes)) in &needs {
        match facts.space.get(guid) {
            Some(s) => {
                let bytes = sizes
                    .iter()
                    .fold(0u64, |acc, &n| acc.saturating_add(on_disk(n, s.cluster)));
                let needed = bytes.saturating_add(margin(bytes));
                if s.free < needed {
                    r.blockers.push(Blocker::NoSpace {
                        volume: *id,
                        needed,
                        free: s.free,
                    });
                }
            }
            None => r.blockers.push(Blocker::SpaceUnknown(*id)),
        }
    }

    // P7: every target, held groups included, is a valid path under its root, equals no
    // source and is unique. Compared as (volume, folded absolute path).
    let abs = |vol: VolumeId, root: &[String], rel: &[String]| {
        let mut p = root.to_vec();
        p.extend(rel.iter().cloned());
        (vol, key(&p))
    };
    let src_parts = roots.source.components().to_vec();
    let sources: BTreeSet<(VolumeId, String)> = plan
        .ops
        .iter()
        .map(|o| abs(roots.source.volume(), &src_parts, &o.src))
        .collect();
    let mut seen = BTreeSet::new();
    for o in &plan.ops {
        let (vol, root_parts) = match o.root {
            Root::Dest => match &roots.dest {
                Some(d) => (d.volume(), d.components().to_vec()),
                None => continue,
            },
            Root::Rejected => {
                let mut p = src_parts.clone();
                p.push(REJECTED.to_owned());
                (roots.source.volume(), p)
            }
        };
        let why = if o.dst.is_empty() || o.dst.iter().any(|c| validate_component(c).is_err()) {
            Some(Inconsistency::BadTarget)
        } else {
            let t = abs(vol, &root_parts, &o.dst);
            if sources.contains(&t) {
                Some(Inconsistency::TargetIsSource)
            } else if !seen.insert(t) {
                Some(Inconsistency::DuplicateTarget)
            } else {
                None
            }
        };
        if let Some(why) = why {
            r.blockers.push(Blocker::Inconsistent { ord: o.ord, why });
        }
    }

    // P16.
    if !facts.preview_idle {
        r.blockers.push(Blocker::PreviewBusy);
    }

    r.held = held.into_iter().collect();
    r.replan = replan
        .into_iter()
        .filter(|(g, _)| !r.held.iter().any(|(h, _)| h == g))
        .collect();
    r.warnings = group_warnings;
    if keeps_now {
        match facts.dest_in_sync_root {
            Some(true) => r.warnings.push(Warning::CloudSynced),
            Some(false) => {}
            None => r.warnings.push(Warning::SyncRootUnknown),
        }
    }
    r
}

/// The first hold for one operation, in rule order: P8, P9, P10, P14.
fn hold_for(
    o: &PlannedOp,
    root_parts: &[String],
    vol: &Vol<'_>,
    src: &Vol<'_>,
    facts: &Facts,
) -> Option<Hold> {
    let member = o.member;
    // P8: lengths on the target volume, the partial at its longest.
    let mut parts = root_parts.to_vec();
    parts.extend(o.dst.iter().cloned());
    let path_len = vol.prefix_len() + utf16(&parts.join("\\"));
    let name_len = o.dst.last().map_or(0, |n| utf16(n));
    // A rename has no partial; a copy or cross-volume move writes one next to its target.
    let extra = if o.kind == OpKind::Rename {
        0
    } else {
        PARTIAL_EXTRA_UTF16
    };
    let too_long = path_len + extra > MAX_PATH_UTF16
        || parts.iter().any(|c| utf16(c) > vol.max_component())
        || name_len + extra > vol.max_component();
    if too_long {
        return Some(Hold::PathTooLong { member });
    }
    // P9: the source, its folders, the target folders (the roots are checked apart).
    let fact = facts.source(&o.src);
    let mut issues: Vec<FolderIssue> = fact
        .and_then(|f| attrs_issue(&f.attrs))
        .into_iter()
        .collect();
    // The folders between the source root and the source must exist.
    let src_folder = &o.src[..o.src.len().saturating_sub(1)];
    issues.extend(
        (1..=src_folder.len())
            .filter_map(|n| issue_of(facts.folder(Tree::Source, &src_folder[..n]))),
    );
    // A target folder with no fact is absent, unless the snapshot shows it.
    let dst_folder = &o.dst[..o.dst.len().saturating_sub(1)];
    let shown = |n: usize| {
        let root = o.root;
        let t = &facts.targets;
        if n == 0 {
            // `_rejected` itself exists if anything under it does.
            t.has_entries(root)
        } else {
            // A file in the way is P11's (a re-plan), not a folder to look at.
            t.is_taken(root, &dst_folder[..n]) && !t.is_blocked(root, &dst_folder[..n])
        }
    };
    let target = |tree: Tree, path: &[String], n: usize| match facts.folder(tree, path) {
        Some(f) => issue_of(Some(f)),
        None if shown(n) => Some(FolderIssue::Unknown),
        None => None,
    };
    match o.root {
        Root::Dest => {
            issues.extend(
                (1..=dst_folder.len()).filter_map(|n| target(Tree::Dest, &dst_folder[..n], n)),
            );
        }
        Root::Rejected => {
            for n in 0..=dst_folder.len() {
                let mut p = vec![REJECTED.to_owned()];
                p.extend(dst_folder[..n].iter().cloned());
                issues.extend(target(Tree::Source, &p, n));
            }
        }
    }
    if issues.contains(&FolderIssue::Link) {
        return Some(Hold::Link { member });
    }
    if issues.contains(&FolderIssue::Placeholder) {
        return Some(Hold::Placeholder { member });
    }
    if issues.contains(&FolderIssue::Unknown) {
        return Some(Hold::FolderUnknown { member });
    }
    // P10: still there, a file, with the scanned size and mtime.
    let rule = TimeRule::for_fs(src.fs_name());
    let unchanged = fact.is_some_and(|f| {
        f.file && f.size == o.expected_size && rule.matches(f.mtime_ns, o.src_mtime_ns)
    });
    if !unchanged {
        return Some(Hold::ChangedSinceScan { member });
    }
    // P14: copies onto FAT32.
    if o.kind != OpKind::Rename && is_fat32(vol.fs_name()) && o.expected_size > FAT32_MAX_FILE {
        return Some(Hold::TooLargeForFat32 { member });
    }
    None
}

impl fmt::Display for Unbound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => f.write_str("no mounted volume has its serial"),
            Self::Ambiguous(n) => write!(f, "{n} mounted volumes have its serial"),
        }
    }
}

impl fmt::Display for FolderIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Link => "is a link",
            Self::Placeholder => "is a cloud placeholder",
            Self::Unknown => "could not be read",
        })
    }
}

impl fmt::Display for Blocker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceVolume(u) => write!(f, "source volume not bound: {u}"),
            Self::SourceVolumeRefused(e) => write!(f, "source volume refused: {e}"),
            Self::SourceRootLink => f.write_str("the source folder is a link or a placeholder"),
            Self::SourceRootUnreadable => f.write_str("the source folder could not be read"),
            Self::SourceAncestor(i) => write!(f, "a folder above the source folder {i}"),
            Self::NoDestination => f.write_str("the plan keeps groups but has no destination"),
            Self::DestVolume(u) => write!(f, "destination volume not bound: {u}"),
            Self::DestRootMissing => {
                f.write_str("the destination folder and its parent are missing")
            }
            Self::DestRootUnreadable => f.write_str("the destination folder could not be read"),
            Self::DestAncestor(i) => write!(f, "a folder above the destination folder {i}"),
            Self::SourceReadOnly => {
                f.write_str("the card is write-protected: check the lock switch on its side")
            }
            Self::DestReadOnly => f.write_str("the destination volume is read-only"),
            Self::NoSpace {
                volume,
                needed,
                free,
            } => write!(f, "volume {volume} needs {needed} bytes free, has {free}"),
            Self::SpaceUnknown(v) => write!(f, "free space on volume {v} is unknown"),
            Self::DestIsSource => f.write_str("the destination is the source folder"),
            Self::DestInRejected => f.write_str("the destination is inside a _rejected folder"),
            Self::DestPlaceholder => f.write_str("the destination is a cloud placeholder"),
            Self::DestLink => f.write_str("the destination is a link"),
            Self::DestNotAFolder => f.write_str("the destination is not a folder"),
            Self::DestVolumeRefused(e) => write!(f, "destination volume refused: {e}"),
            Self::Inconsistent { ord, why } => {
                write!(f, "operation {ord} is inconsistent ({why:?}): a bug")
            }
            Self::PreviewBusy => f.write_str("the preview pipeline still holds files"),
        }
    }
}
