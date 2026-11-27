//! Plan building: decided groups to file operations (design/file-engine.md §7; stories US-12
//! to US-15; ADR 0003, 0004, 0007, 0023).
//!
//! [`plan`] is pure. Its inputs are plain data: the session's groups and members
//! ([`PlanGroup`], [`PlanMember`], buildable from a scan's [`Group`] with
//! [`PlanGroup::from_group`]), the decisions ([`Decisions`]), the session's settings
//! ([`Settings`]) and a snapshot of what already exists under the two target roots
//! ([`Existing`]). Nothing here touches a disk (INV-03); the snapshot is only a hint, and the
//! no-replace rename and `create_new` keep the last word at execution (§9).
//!
//! What it guarantees, each checked by the property tests in `plan/tests.rs`:
//!
//! - Only groups that are kept or rejected, `Ok`, not applied and not locked by another plan
//!   get operations; undecided groups never do (INV-04).
//! - Which files move (§7.1): the primary; the NEF when it is the primary or "RAW follows JPEG"
//!   is on; a bare `X.xmp` or `X.pp3` with the NEF when the group has one, otherwise with the
//!   primary (ADR 0023); a sidecar that describes one file with that file (INV-08, INV-09).
//!   Missing members get nothing and are listed.
//! - Targets mirror the source folders under the destination root (keep) or `_rejected`
//!   (reject), so every target is under its root (INV-05).
//! - One suffix `n` per group, the smallest in `0..=9999` for which no target exists in the
//!   snapshot and none is claimed by an earlier operation of the plan, compared folded
//!   (§7.3, INV-06, INV-07). A target is not free either when it is a folder (or above one)
//!   that any candidate group's targets go into, whichever group comes first. No suffix free,
//!   a target folder blocked by an existing file or by a file an earlier operation claims, or a
//!   target name Windows would refuse ([`validate_component`], length included): the group is
//!   held back with no operations and listed. A kept group that would get operations with no
//!   destination chosen fails the whole plan ([`PlanError::NoDestination`]).
//! - The kind per §7.2: reject `rename`; keep by move `rename` on the same volume, `xmove`
//!   otherwise; keep by copy `copy`; any read-only (camera-protected) planned member makes the
//!   whole group `copy`.
//! - Plan order (§9.5): rejects first, then by folder key, stem key and group id, then members
//!   in the order primary, raw, group sidecars, file sidecars (each by folded folder, then
//!   folded name). Suffixes
//!   are chosen in that order, so the plan is deterministic and independent of the order of the
//!   input (INV-24).
//!
//! Partial names hold the operation's database id (§7.2), so they are made by the store, not
//! here. Preflight (§8) is separate.

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::decisions::{Decisions, GroupId, GroupState, Verdict};
use crate::grouping::{Describes, Group, GroupStatus, Photo, SidecarKind};
use crate::names::{fold, validate_component};

/// A member's id: `member.id` in the session database (§2.3).
pub type MemberId = i64;

/// Largest collision suffix tried (§7.3).
pub const MAX_SUFFIX: u32 = 9999;

/// Keep by move or by copy (SPEC §2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeepMode {
    /// Move: a rename on the same volume, a verified copy then removal across volumes.
    Move,
    /// Copy: the source stays.
    Copy,
}

/// The session's settings, as the plan snapshots them (§2.1 `Plan`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Settings {
    /// "RAW follows JPEG".
    pub raw_follows: bool,
    /// Where kept groups go; `None` when no destination is chosen (then no group may be kept).
    pub keep: Option<KeepTo>,
}

/// How kept groups reach the destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeepTo {
    /// Move or copy.
    pub mode: KeepMode,
    /// The destination is on the source's volume, by GUID (§6.3, [`crate::volume::relation`]).
    pub same_volume: bool,
}

/// Which root a target is under.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Root {
    /// The destination folder (keep).
    Dest,
    /// `<source>\_rejected` (reject).
    Rejected,
}

impl Root {
    /// The `operation.dst_root` value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dest => "dest",
            Self::Rejected => "rejected",
        }
    }
}

/// An operation's kind (§7.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OpKind {
    /// A no-replace rename on one volume.
    Rename,
    /// A verified copy; the source stays.
    Copy,
    /// A verified copy across volumes, then removal of the source.
    XMove,
}

impl OpKind {
    /// The `operation.kind` value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rename => "rename",
            Self::Copy => "copy",
            Self::XMove => "xmove",
        }
    }
}

/// A member's role in its group (§2.1 `FileMember`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// The JPEG, or the NEF of a lone NEF.
    Primary,
    /// The NEF next to a JPEG.
    Raw,
    /// A sidecar, with what it describes.
    Sidecar {
        /// Its kind.
        kind: SidecarKind,
        /// What it describes: [`Describes::Group`] for a bare `X.xmp` or `X.pp3`.
        describes: Describes,
    },
}

/// One file of a group, from the session database (§2.3 `member`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PlanMember {
    /// `member.id`.
    pub id: MemberId,
    /// Its role.
    pub role: Role,
    /// Its folder, as components from the source root (the group's folder, or its
    /// `NKSC_PARAM` for a `.nksc`).
    pub folder: Vec<String>,
    /// Its file name as written.
    pub name: String,
    /// Size in bytes, as scanned.
    pub size: u64,
    /// Modification time as scanned.
    pub mtime_ns: Option<i64>,
    /// Present in the last scan; a missing member gets no operation and is listed.
    pub present: bool,
    /// The read-only attribute (camera "protect"): its group is copied (§7.2, P13).
    pub read_only: bool,
}

/// A group of the session with its members.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PlanGroup {
    /// `photo_group.id`.
    pub id: GroupId,
    /// Its folder, as components from the source root.
    pub folder: Vec<String>,
    /// The stem as written (the primary's).
    pub stem: String,
    /// Ok or ambiguous; an ambiguous group gets no operations (§3.3).
    pub status: GroupStatus,
    /// Some operation of it ever reached `committed` (§5.4).
    pub applied: bool,
    /// It has an operation in another open plan (§5.4).
    pub locked: bool,
    /// Its files.
    pub members: Vec<PlanMember>,
}

impl PlanGroup {
    /// A group as a fresh scan sees it: every file present, not applied, not locked. Members
    /// get the ids `first_member, first_member + 1, …` in the order primary, raw, sidecars.
    /// Duplicates of an ambiguous group are left out (it gets no operations anyway).
    pub fn from_group(id: GroupId, g: &Group, first_member: MemberId) -> Self {
        let mut members = Vec::new();
        let mut next = first_member;
        let mut push = |role, entry: &crate::grouping::ScanEntry| {
            members.push(PlanMember {
                id: next,
                role,
                folder: entry.folder.to_vec(),
                name: entry.name.clone(),
                size: entry.size,
                mtime_ns: entry.mtime_ns,
                present: true,
                read_only: entry.attrs.read_only,
            });
            next += 1;
        };
        let Photo { entry, .. } = &g.primary;
        push(Role::Primary, entry);
        if let Some(raw) = &g.raw {
            push(Role::Raw, &raw.entry);
        }
        for s in &g.sidecars {
            push(
                Role::Sidecar {
                    kind: s.kind,
                    describes: s.describes,
                },
                &s.entry,
            );
        }
        Self {
            id,
            folder: g.folder.to_vec(),
            stem: g.stem.clone(),
            status: g.status,
            applied: false,
            locked: false,
            members,
        }
    }

    fn has_nef(&self) -> bool {
        self.members.iter().any(|m| m.role == Role::Raw)
    }
}

/// A snapshot of the entries that exist under the target roots: what `exists` answers in
/// §7.3. Paths are components from the root, compared folded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Existing {
    taken: BTreeSet<(Root, String)>,
    files: BTreeSet<(Root, String)>,
}

fn key(parts: &[String]) -> String {
    fold(&parts.join("\\"))
}

impl Existing {
    /// No entries: both roots empty or absent.
    pub fn new() -> Self {
        Self::default()
    }

    /// A file at `path` under `root`. Its ancestors are folders, so they count as taken too.
    pub fn add_file(&mut self, root: Root, path: &[&str]) {
        let parts: Vec<String> = path.iter().map(|s| (*s).to_owned()).collect();
        self.add_ancestors(root, &parts);
        self.taken.insert((root, key(&parts)));
        self.files.insert((root, key(&parts)));
    }

    /// A folder (or anything else that is not a file) at `path` under `root`, and its
    /// ancestors.
    pub fn add_dir(&mut self, root: Root, path: &[&str]) {
        let parts: Vec<String> = path.iter().map(|s| (*s).to_owned()).collect();
        self.add_ancestors(root, &parts);
        self.taken.insert((root, key(&parts)));
    }

    fn add_ancestors(&mut self, root: Root, parts: &[String]) {
        for n in 1..parts.len() {
            self.taken.insert((root, key(&parts[..n])));
        }
    }

    /// Something exists at this path (file, folder or other).
    pub fn is_taken(&self, root: Root, path: &[String]) -> bool {
        self.taken.contains(&(root, key(path)))
    }

    /// Something exists under `root`, so the root folder itself exists.
    pub fn has_entries(&self, root: Root) -> bool {
        self.taken.iter().any(|(r, _)| *r == root)
    }

    /// A file sits where `folder` or one of its ancestors should be a folder.
    pub fn is_blocked(&self, root: Root, folder: &[String]) -> bool {
        (1..=folder.len()).any(|n| self.files.contains(&(root, key(&folder[..n]))))
    }
}

/// One planned file operation (§2.1 `Operation`, without its database id and partial).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PlannedOp {
    /// Position in the plan, from 0.
    pub ord: u32,
    /// The group.
    pub group: GroupId,
    /// The member.
    pub member: MemberId,
    /// Keep or reject.
    pub action: Verdict,
    /// Rename, copy or cross-volume move.
    pub kind: OpKind,
    /// Source path, as components from the source root.
    pub src: Vec<String>,
    /// Target root.
    pub root: Root,
    /// Target path, as components from the root.
    pub dst: Vec<String>,
    /// The target path folded, components joined with `\` (`operation.dst_key`).
    pub dst_key: String,
    /// The scanned size.
    pub expected_size: u64,
    /// The scanned modification time.
    pub src_mtime_ns: Option<i64>,
}

/// A group with operations in the plan, for the review screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlannedGroup {
    /// The group.
    pub group: GroupId,
    /// Keep or reject.
    pub action: Verdict,
    /// The kind of all its operations.
    pub kind: OpKind,
    /// The collision suffix: 0 for none, `n` for `X_n`.
    pub suffix: u32,
    /// Copied, though it would be renamed or moved, because a planned member is read-only
    /// (camera "protect"): the review and the report say "camera-protected: copied, original
    /// kept".
    pub protected: bool,
}

/// Why a decided group gets no operations (listed in the review).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Held {
    /// Ambiguous (§3.3).
    Ambiguous,
    /// Already applied (§5.4).
    Applied,
    /// In another open plan (§5.4).
    Locked,
    /// Every file that would move is missing.
    NothingPresent,
    /// No suffix up to [`MAX_SUFFIX`] gives free targets (§7.3).
    NoFreeSuffix,
    /// A file exists, or an earlier operation of the plan puts one, where a target folder
    /// would be: no suffix helps.
    TargetFolderBlocked,
    /// A target name would not be a valid Windows file name ([`validate_component`]): too
    /// long once the suffix is added (more than 255 UTF-16 units), or invalid as scanned.
    /// Larger suffixes are longer, so the search stops there.
    TargetNameInvalid,
    /// A member's name does not start with the group's stem: the session data is
    /// inconsistent (a bug).
    NameMismatch,
}

/// A plan: operations in order, with what the review lists.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Operations in plan order (`ord` 0, 1, …).
    pub ops: Vec<PlannedOp>,
    /// Groups with operations, in plan order.
    pub groups: Vec<PlannedGroup>,
    /// Decided groups held back with no operations, by group id.
    pub held: Vec<(GroupId, Held)>,
    /// Members that would have moved but are missing, by group then member id.
    pub missing: Vec<(GroupId, MemberId)>,
    /// Bytes to copy for `copy` operations.
    pub bytes_copy: u64,
    /// Bytes to copy for `xmove` operations.
    pub bytes_xmove: u64,
}

/// Why no plan can be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlanError {
    /// A kept group would get operations but no destination is chosen. Kept groups held for
    /// a reason that does not depend on the destination (ambiguous, applied, locked, nothing
    /// present) do not count.
    NoDestination,
    /// Two groups have the same id.
    DuplicateGroup(GroupId),
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDestination => {
                f.write_str("a kept group would be moved but no destination is chosen")
            }
            Self::DuplicateGroup(g) => write!(f, "group {g} is given twice"),
        }
    }
}

impl std::error::Error for PlanError {}

/// Which members of a decided group move (§7.1), present or not, in member order.
fn selected(g: &PlanGroup, raw_follows: bool) -> Vec<&PlanMember> {
    let has_nef = g.has_nef();
    let photo_moves = |role: Role| match role {
        Role::Primary => true,
        Role::Raw => raw_follows,
        Role::Sidecar { .. } => false,
    };
    let present = |role: Role| g.members.iter().any(|m| m.role == role && m.present);
    let moves = |m: &PlanMember| match m.role {
        Role::Primary | Role::Raw => photo_moves(m.role),
        Role::Sidecar { describes, .. } => {
            let follows = match describes {
                Describes::Group if has_nef => Role::Raw,
                Describes::Group | Describes::Primary => Role::Primary,
                Describes::Raw => Role::Raw,
                Describes::Duplicate(_) => return false,
            };
            photo_moves(follows) && present(follows)
        }
    };
    let rank = |m: &PlanMember| match m.role {
        Role::Primary => 0,
        Role::Raw => 1,
        Role::Sidecar {
            describes: Describes::Group,
            ..
        } => 2,
        Role::Sidecar { .. } => 3,
    };
    let mut out: Vec<&PlanMember> = g.members.iter().filter(|m| moves(m)).collect();
    out.sort_by(|a, b| {
        (rank(a), key(&a.folder), fold(&a.name), &a.name, a.id).cmp(&(
            rank(b),
            key(&b.folder),
            fold(&b.name),
            &b.name,
            b.id,
        ))
    });
    out
}

/// A member's target name with suffix `n`: the stem at the start of the name becomes `X_n`,
/// the rest keeps its case (§7.2). `None` if the name does not start with the stem (folded).
pub fn target_name(name: &str, stem: &str, n: u32) -> Option<String> {
    let len = stem.chars().count();
    let split = name.char_indices().nth(len).map_or(name.len(), |(i, _)| i);
    let (head, tail) = name.split_at(split);
    if fold(head) != fold(stem) {
        return None;
    }
    Some(if n == 0 {
        name.to_owned()
    } else {
        format!("{head}_{n}{tail}")
    })
}

/// Builds the plan (design/file-engine.md §7). Pure.
pub fn plan(
    groups: &[PlanGroup],
    decisions: &Decisions,
    settings: &Settings,
    existing: &Existing,
) -> Result<Plan, PlanError> {
    let mut seen = BTreeSet::new();
    for g in groups {
        if !seen.insert(g.id) {
            return Err(PlanError::DuplicateGroup(g.id));
        }
    }
    // Decided groups in plan order: rejects first, then folder key, stem key, id.
    let mut order: Vec<(Verdict, &PlanGroup)> = groups
        .iter()
        .filter_map(|g| match decisions.state(g.id) {
            GroupState::Undecided => None,
            GroupState::Kept => Some((Verdict::Keep, g)),
            GroupState::Rejected => Some((Verdict::Reject, g)),
        })
        .collect();
    order.sort_by_cached_key(|(v, g)| {
        (
            *v == Verdict::Keep,
            key(&g.folder),
            fold(&g.stem),
            g.folder.clone(),
            g.stem.clone(),
            g.id,
        )
    });

    let mut out = Plan::default();
    let mut held: BTreeMap<GroupId, Held> = BTreeMap::new();
    let mut missing = BTreeSet::new();
    // First pass: the holds that do not depend on the targets, and what each remaining group
    // would move.
    let mut candidates = Vec::new();
    for (action, g) in order {
        let reason = if g.status == GroupStatus::Ambiguous {
            Some(Held::Ambiguous)
        } else if g.applied {
            Some(Held::Applied)
        } else if g.locked {
            Some(Held::Locked)
        } else {
            None
        };
        if let Some(r) = reason {
            held.insert(g.id, r);
            continue;
        }
        let chosen = selected(g, settings.raw_follows);
        missing.extend(chosen.iter().filter(|m| !m.present).map(|m| (g.id, m.id)));
        let members: Vec<&PlanMember> = chosen.into_iter().filter(|m| m.present).collect();
        if members.is_empty() {
            held.insert(g.id, Held::NothingPresent);
            continue;
        }
        let root = match action {
            Verdict::Keep => Root::Dest,
            Verdict::Reject => Root::Rejected,
        };
        let kind = match (action, settings.keep) {
            (Verdict::Reject, _) => OpKind::Rename,
            (Verdict::Keep, Some(k)) => match (k.mode, k.same_volume) {
                (KeepMode::Copy, _) => OpKind::Copy,
                (KeepMode::Move, true) => OpKind::Rename,
                (KeepMode::Move, false) => OpKind::XMove,
            },
            // A kept group that would get operations needs a destination.
            (Verdict::Keep, None) => return Err(PlanError::NoDestination),
        };
        // Camera-protected (P13): a read-only member turns a rename or xmove group into copies.
        let protected = kind != OpKind::Copy && members.iter().any(|m| m.read_only);
        let kind = if protected { OpKind::Copy } else { kind };
        candidates.push((action, g, members, root, kind, protected));
    }
    // Every folder a candidate's targets go into (and its ancestors) will be a folder: no
    // target may take its name, whichever group comes first in the plan.
    let mut folders: BTreeSet<(Root, String)> = BTreeSet::new();
    for (_, _, members, root, ..) in &candidates {
        for m in members {
            for n in 1..=m.folder.len() {
                folders.insert((*root, key(&m.folder[..n])));
            }
        }
    }
    // Targets claimed by earlier operations.
    let mut claimed: BTreeSet<(Root, String)> = BTreeSet::new();
    for (action, g, members, root, kind, protected) in candidates {
        let claimed_file_at = |folder: &[String]| {
            (1..=folder.len()).any(|n| claimed.contains(&(root, key(&folder[..n]))))
        };
        if members
            .iter()
            .any(|m| existing.is_blocked(root, &m.folder) || claimed_file_at(&m.folder))
        {
            held.insert(g.id, Held::TargetFolderBlocked);
            continue;
        }
        let targets_for = |n: u32| -> Option<Vec<Vec<String>>> {
            members
                .iter()
                .map(|m| {
                    let mut path = m.folder.clone();
                    path.push(target_name(&m.name, &g.stem, n)?);
                    Some(path)
                })
                .collect()
        };
        let free = |paths: &[Vec<String>]| {
            let mut keys = BTreeSet::new();
            paths.iter().all(|p| {
                let k = key(p);
                !existing.is_taken(root, p)
                    && !claimed.contains(&(root, k.clone()))
                    && !folders.contains(&(root, k.clone()))
                    && keys.insert(k)
            })
        };
        // The smallest free suffix; an invalid name ends the search (longer suffixes are
        // longer names).
        let mut found = Err(Held::NoFreeSuffix);
        for n in 0..=MAX_SUFFIX {
            let Some(paths) = targets_for(n) else {
                found = Err(Held::NameMismatch);
                break;
            };
            if paths.iter().any(|p| {
                p.last()
                    .is_none_or(|name| validate_component(name).is_err())
            }) {
                found = Err(Held::TargetNameInvalid);
                break;
            }
            if free(&paths) {
                found = Ok((n, paths));
                break;
            }
        }
        let (suffix, paths) = match found {
            Ok(x) => x,
            Err(r) => {
                held.insert(g.id, r);
                continue;
            }
        };
        for (m, dst) in members.iter().zip(paths) {
            let dst_key = key(&dst);
            claimed.insert((root, dst_key.clone()));
            let mut src = m.folder.clone();
            src.push(m.name.clone());
            match kind {
                OpKind::Copy => out.bytes_copy += m.size,
                OpKind::XMove => out.bytes_xmove += m.size,
                OpKind::Rename => {}
            }
            out.ops.push(PlannedOp {
                ord: u32::try_from(out.ops.len()).unwrap_or(u32::MAX),
                group: g.id,
                member: m.id,
                action,
                kind,
                src,
                root,
                dst,
                dst_key,
                expected_size: m.size,
                src_mtime_ns: m.mtime_ns,
            });
        }
        out.groups.push(PlannedGroup {
            group: g.id,
            action,
            kind,
            suffix,
            protected,
        });
    }
    out.held = held.into_iter().collect();
    out.missing = missing.into_iter().collect();
    Ok(out)
}
