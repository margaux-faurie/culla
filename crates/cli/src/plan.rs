//! `culla plan`: a dry run of apply, wiring only. Scans the folder like `culla scan`, reads a
//! decision log the user names (`culla_adapters::facts::read_text`), parses and folds it
//! (`culla_core::decisions`), takes a snapshot of the target folders
//! (`culla_adapters::facts::snapshot_targets`), builds the plan (`culla_core::plan::plan`),
//! gathers the facts preflight needs with read-only lookups (`culla_adapters::facts::lookup`)
//! and runs preflight (`culla_core::preflight::preflight`).
//!
//! Read-only (ADR 0020, INV-03): it lists folders, looks at paths without following links,
//! reads the first bytes of each primary for its EXIF and reads the decision log. It writes
//! nothing but stdout and stderr: no `_rejected`, no destination, no session, no journal.
//!
//! Group ids: the log's `group_id` is the position `culla scan` prints for the same folder and
//! options (1 is the first group in capture-time order), since there is no session database
//! yet. Member ids follow from it, in the order primary, raw, sidecars. The scan is the one
//! `culla scan` does, so a destination inside the source is scanned too and numbered with the
//! rest; its groups are then left out of the plan, and decisions on them listed as ignored.
//!
//! Facts the adapters cannot look up yet are either left unknown, so preflight fails closed on
//! them (file-engine §8), or not checked at all (locks, a second volume with the same serial);
//! the report lists both, apart ([`Unchecked`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use culla_adapters::facts::{
    Lookup, ReadTextError, TargetFacts, canonical, lookup, read_text, snapshot_targets,
};
use culla_adapters::winfs::{VolumeError, resolve_folder};
use culla_core::decisions::codec::parse_log;
use culla_core::decisions::{GroupId, GroupState, fold};
use culla_core::grouping::ScanPolicy;
use culla_core::names::eq_folded;
use culla_core::plan::{
    Existing, Held, KeepMode, KeepTo, MemberId, OpKind, Plan, PlanError, PlanGroup, Root, Settings,
    plan,
};
use culla_core::preflight::{
    self, Blocker, DestRoot, Facts, FolderFact, Hold, REJECTED, Replan, Roots, SourceFact, Tree,
    Warning,
};
use culla_core::volume::{Source, VolumeId, VolumeInfo, VolumeRelation, relation};

use crate::args::{PlanArgs, ScanArgs};
use crate::json::Value;
use crate::scan::{self, Failure, Identity, Progress};
use crate::text::{count, shown};

/// Largest decision log read, in bytes (64 MiB: about 600,000 events).
pub(crate) const MAX_LOG_BYTES: u64 = 64 << 20;

/// Why no plan was printed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PlanFailure {
    /// The folder or the destination could not be scanned or resolved (exit 2 or 3).
    Scan(Failure),
    /// The decision log could not be read: exit 3.
    LogUnreadable(String),
    /// The decision log is not valid (too large, not UTF-8, a bad line, a corrupt fold):
    /// exit 4.
    LogInvalid(String),
    /// No plan can be built (kept groups but no destination): exit 5.
    NoPlan(String),
}

impl From<Failure> for PlanFailure {
    fn from(f: Failure) -> Self {
        Self::Scan(f)
    }
}

/// A fact the dry run did not look up: either preflight treats it as unknown and fails closed
/// ([`Unchecked::fails_closed`]), or it is not checked at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Unchecked {
    /// Free space is not looked up: P5 blocks any copy or cross-volume move.
    FreeSpace,
    /// Whether the destination is in a cloud-synced folder: P17 warns.
    SyncRoot,
    /// Locks held by other programs (P12) are not checked (`SourceFact::locked` is always
    /// false, so P12 never warns): opening the files is left to apply.
    Locks,
    /// Windows: only the source's and the destination's volumes are looked up, by path, so a
    /// second mounted volume with the same serial is not seen (P1, P2).
    MountedVolumes,
    /// Off Windows: no volume is looked up, so P1 and P2 block.
    Volumes,
    /// Off Windows with `--keep move`: whether the destination is on the source's volume is
    /// unknown, so kept groups are planned as cross-volume moves.
    VolumeRelation,
}

impl Unchecked {
    /// The JSON id.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::FreeSpace => "free_space",
            Self::SyncRoot => "sync_root",
            Self::Locks => "locks",
            Self::MountedVolumes => "mounted_volumes",
            Self::Volumes => "volumes",
            Self::VolumeRelation => "volume_relation",
        }
    }

    /// Whether preflight treats the fact as unknown and fails closed on it (a blocker, a hold or
    /// a warning). The others are simply not checked: no finding can come from them.
    pub(crate) fn fails_closed(self) -> bool {
        match self {
            Self::FreeSpace | Self::SyncRoot | Self::Volumes | Self::VolumeRelation => true,
            Self::Locks | Self::MountedVolumes => false,
        }
    }

    /// The text.
    pub(crate) fn text(self) -> &'static str {
        match self {
            Self::FreeSpace => {
                "free space is not looked up: copies and cross-volume moves are blocked (P5)"
            }
            Self::SyncRoot => {
                "whether the destination is in a cloud-synced folder is not looked up (P17)"
            }
            Self::Locks => {
                "locks held by other programs are not checked: P12 never warns, apply finds them"
            }
            Self::MountedVolumes => {
                "only the source and destination volumes are looked up, by path: a second \
                 mounted volume with the same serial is not detected (P1, P2)"
            }
            Self::Volumes => {
                "volumes are not looked up off Windows: the volume checks block (P1, P2)"
            }
            Self::VolumeRelation => {
                "whether the destination is on the source's volume is unknown off Windows: \
                 kept photos are planned as cross-volume moves"
            }
        }
    }
}

/// Everything `culla plan` prints.
#[derive(Debug)]
pub(crate) struct Report {
    /// The scan, as `culla scan` sees it.
    pub scan: scan::Report,
    /// The decision log as given.
    pub decisions_path: PathBuf,
    /// Events in the log.
    pub events: usize,
    /// Decided group ids the scan does not have (ignored).
    pub unknown_ids: Vec<GroupId>,
    /// Decided group ids of groups inside the destination (ignored: never planned).
    pub dest_ids: Vec<GroupId>,
    /// Keep mode and destination as given.
    pub keep: Option<(KeepMode, PathBuf)>,
    /// The destination's final path, when it could be resolved.
    pub dest_resolved: Option<PathBuf>,
    /// The destination lies inside the source: its groups are left out of the plan.
    pub dest_inside_source: bool,
    /// "RAW follows JPEG".
    pub raw_follows: bool,
    /// The destination is on the source's volume; `None` when unknown.
    pub same_volume: Option<bool>,
    /// The groups planned, by id (position in capture-time order, from 1), the destination's
    /// left out.
    pub groups: Vec<PlanGroup>,
    /// Each group's state after the fold, by id, the destination's left out.
    pub states: BTreeMap<GroupId, GroupState>,
    /// The plan.
    pub plan: Plan,
    /// Preflight on it.
    pub preflight: preflight::Report,
    /// Facts left unknown.
    pub unchecked: Vec<Unchecked>,
}

impl Report {
    /// A member's path from the source root, `\`-separated, or `?` for an unknown id.
    pub(crate) fn member_path(&self, member: MemberId) -> String {
        self.groups
            .iter()
            .flat_map(|g| &g.members)
            .find(|m| m.id == member)
            .map_or_else(|| "?".to_owned(), |m| join(&m.folder, &m.name))
    }

    /// A group's primary path from the source root.
    pub(crate) fn group_path(&self, group: GroupId) -> String {
        self.groups
            .iter()
            .find(|g| g.id == group)
            .and_then(|g| g.members.first())
            .map_or_else(|| "?".to_owned(), |m| join(&m.folder, &m.name))
    }
}

fn join(folder: &[String], name: &str) -> String {
    let mut s = String::new();
    for c in folder {
        s.push_str(c);
        s.push('\\');
    }
    s.push_str(name);
    s
}

/// The destination once resolved: its final path, its volume when known, and it as a source.
struct Dest {
    path: PathBuf,
    info: Option<VolumeInfo>,
    source: Option<Source>,
}

/// Runs the dry run. Progress goes to `progress`.
///
/// # Errors
///
/// [`PlanFailure`] says why nothing was printed.
pub(crate) fn run(args: &PlanArgs, progress: &mut Progress) -> Result<Report, PlanFailure> {
    // The log first: it is cheap, and a bad one should not cost a scan.
    let text = read_text(&args.decisions, MAX_LOG_BYTES).map_err(|e| match e {
        ReadTextError::TooLarge { .. } | ReadTextError::NotUtf8 => {
            PlanFailure::LogInvalid(e.to_string())
        }
        ReadTextError::Io(_) | ReadTextError::NotAFile | ReadTextError::Placeholder => {
            PlanFailure::LogUnreadable(e.to_string())
        }
    })?;
    let events = parse_log(&text).map_err(|e| PlanFailure::LogInvalid(e.to_string()))?;
    let decisions = fold(&events)
        .map_err(|e| PlanFailure::LogInvalid(format!("the log is corrupt at {}", e.event)))?;

    let (scanned, identity) = scan::resolve(&args.folder)?;
    let dest = match &args.keep {
        Some((_, d)) => Some(resolve_dest(d)?),
        None => None,
    };
    let src_canon = canonical(&scanned).unwrap_or_else(|_| scanned.clone());
    let inside = dest
        .as_ref()
        .and_then(|d| d.path.strip_prefix(&src_canon).ok())
        .and_then(|rel| {
            rel.components()
                .map(|c| match c {
                    Component::Normal(s) => s.to_str().map(str::to_owned),
                    _ => None,
                })
                .collect::<Option<Vec<String>>>()
        });
    let scan_args = ScanArgs {
        folder: args.folder.clone(),
        include_subfolders: args.include_subfolders,
        json: args.json,
    };
    // The same scan as `culla scan` (no destination left out), so the log's ids are the
    // positions it prints; the destination's groups are dropped afterwards.
    let report = scan::run_resolved(&scan_args, scanned.clone(), identity, None, progress)?;

    // Groups by position in capture-time order, as `culla scan` prints them.
    let mut groups = Vec::with_capacity(report.order.len());
    let mut in_dest: BTreeSet<GroupId> = BTreeSet::new();
    let mut next_member: MemberId = 1;
    for (pos, &i) in report.order.iter().enumerate() {
        let id = GroupId::try_from(pos + 1).unwrap_or(GroupId::MAX);
        let group = &report.grouping.groups[i];
        let g = PlanGroup::from_group(id, group, next_member);
        next_member += MemberId::try_from(g.members.len()).unwrap_or(0);
        if inside
            .as_deref()
            .is_some_and(|d| under_folder(&group.folder, d))
        {
            in_dest.insert(id);
        } else {
            groups.push(g);
        }
    }
    let known: BTreeSet<GroupId> = groups.iter().map(|g| g.id).collect();
    let mut states: BTreeMap<GroupId, GroupState> = decisions.decided().collect();
    let unknown_ids: Vec<GroupId> = states
        .keys()
        .copied()
        .filter(|g| !known.contains(g) && !in_dest.contains(g))
        .collect();
    let dest_ids: Vec<GroupId> = states
        .keys()
        .copied()
        .filter(|g| in_dest.contains(g))
        .collect();
    states.retain(|g, _| !in_dest.contains(g));

    // Volumes and roots.
    let mut unchecked = vec![Unchecked::FreeSpace, Unchecked::SyncRoot, Unchecked::Locks];
    let mut mounted: Vec<VolumeInfo> = Vec::new();
    let (src_info, source) = match &report.identity {
        Identity::Resolved(r) => {
            mounted.push(r.volume.clone());
            (Some(r.volume.clone()), r.source.clone())
        }
        Identity::Unavailable(_) => (None, placeholder_source(&src_canon)),
    };
    if src_info.is_some() {
        unchecked.push(Unchecked::MountedVolumes);
    } else {
        unchecked.push(Unchecked::Volumes);
    }
    let same_volume = match (&src_info, dest.as_ref().and_then(|d| d.info.as_ref())) {
        (Some(s), Some(d)) => {
            if !mounted.iter().any(|m| m.guid_path == d.guid_path) {
                mounted.push(d.clone());
            }
            Some(relation(s, d) == VolumeRelation::Same)
        }
        _ => None,
    };
    if same_volume.is_none() && matches!(args.keep, Some((KeepMode::Move, _))) {
        unchecked.push(Unchecked::VolumeRelation);
    }
    let dest_source = dest.as_ref().map(|d| {
        d.source
            .clone()
            .unwrap_or_else(|| placeholder_source(&d.path))
    });

    // What exists under the target roots, on the way to the folders decided groups go into.
    let max_entries = ScanPolicy::default().max_entries_per_folder;
    let mut need: BTreeMap<Root, BTreeSet<Vec<String>>> = BTreeMap::new();
    for g in &groups {
        let root = match states.get(&g.id) {
            Some(GroupState::Kept) if dest.is_some() => Root::Dest,
            Some(GroupState::Rejected) => Root::Rejected,
            _ => continue,
        };
        let set = need.entry(root).or_default();
        for m in &g.members {
            set.insert(m.folder.clone());
        }
    }
    let mut existing = Existing::new();
    let rejected_dir = scanned.join(REJECTED);
    let rejected_facts = snapshot_targets(
        &rejected_dir,
        Root::Rejected,
        need.get(&Root::Rejected).unwrap_or(&BTreeSet::new()),
        max_entries,
        &mut existing,
    );
    let dest_facts = dest.as_ref().map(|d| {
        snapshot_targets(
            &d.path,
            Root::Dest,
            need.get(&Root::Dest).unwrap_or(&BTreeSet::new()),
            max_entries,
            &mut existing,
        )
    });

    let settings = Settings {
        raw_follows: args.raw_follows,
        keep: args.keep.as_ref().map(|(mode, _)| KeepTo {
            mode: *mode,
            same_volume: same_volume.unwrap_or(false),
        }),
    };
    let built = plan(&groups, &decisions, &settings, &existing).map_err(|e| match e {
        PlanError::NoDestination => PlanFailure::NoPlan(
            "photos are kept but no destination is given: add --keep move|copy --dest <folder>"
                .to_owned(),
        ),
        PlanError::DuplicateGroup(_) => PlanFailure::NoPlan(e.to_string()),
    })?;

    // Facts, read-only.
    let source_root = match lookup(&scanned) {
        Lookup::Folder(a) | Lookup::Other(a) => FolderFact::Exists(a),
        _ => FolderFact::Unreadable,
    };
    let dest_root = dest
        .as_ref()
        .map_or(DestRoot::Missing, |d| dest_root_fact(&d.path));
    let mut facts = Facts::new(source_root, dest_root);
    facts.mounted = mounted;
    // This process runs no preview pipeline.
    facts.preview_idle = true;
    facts.targets = existing;
    let mut src_folders: BTreeSet<Vec<String>> = BTreeSet::new();
    for o in &built.ops {
        let path = scanned.join(o.src.join(std::path::MAIN_SEPARATOR_STR));
        match lookup(&path) {
            Lookup::File {
                size,
                mtime_ns,
                attrs,
            } => facts.add_source(
                &o.src,
                SourceFact {
                    file: true,
                    size,
                    mtime_ns,
                    attrs,
                    locked: false,
                },
            ),
            Lookup::Folder(attrs) | Lookup::Other(attrs) => facts.add_source(
                &o.src,
                SourceFact {
                    file: false,
                    size: 0,
                    mtime_ns: None,
                    attrs,
                    locked: false,
                },
            ),
            // No fact: preflight takes it as changed since the scan.
            Lookup::Missing | Lookup::Unreadable(_) => {}
        }
        for n in 1..o.src.len() {
            src_folders.insert(o.src[..n].to_vec());
        }
    }
    for f in &src_folders {
        add_folder_fact(&mut facts, Tree::Source, f, &lookup(&path_of(&scanned, f)));
    }
    add_target_facts(&mut facts, &rejected_facts, Root::Rejected);
    if let Some(t) = &dest_facts {
        add_target_facts(&mut facts, t, Root::Dest);
    }
    add_ancestor_facts(&mut facts, &source, &src_canon);
    if let (Some(ds), Some(d)) = (&dest_source, &dest) {
        add_ancestor_facts(&mut facts, ds, &d.path);
    }
    let roots = Roots {
        source,
        dest: dest_source,
    };
    let checked = preflight::preflight(&built, &roots, &facts);

    Ok(Report {
        scan: report,
        decisions_path: args.decisions.clone(),
        events: events.len(),
        unknown_ids,
        dest_ids,
        keep: args.keep.clone(),
        dest_resolved: dest.map(|d| d.path),
        dest_inside_source: inside.is_some(),
        raw_follows: args.raw_follows,
        same_volume,
        groups,
        states,
        plan: built,
        preflight: checked,
        unchecked,
    })
}

/// Resolves the destination: its volume on Windows, its final path everywhere. A destination
/// that does not exist yet is resolved through its parent (apply would create it).
fn resolve_dest(dest: &Path) -> Result<Dest, Failure> {
    let refused = |why: String| Failure::Refused(format!("destination: {why}"));
    match resolve_folder(dest) {
        Ok(r) => Ok(Dest {
            path: r.canonical,
            info: Some(r.volume),
            source: Some(r.source),
        }),
        Err(e @ VolumeError::Path(_)) => Err(refused(e.to_string())),
        Err(e @ VolumeError::Win32 { .. }) => Err(Failure::Io(format!("destination: {e}"))),
        Err(e @ (VolumeError::Unsupported | VolumeError::Io(_))) => {
            if !matches!(lookup(dest), Lookup::Missing) {
                // There but not resolvable as a folder: off Windows, or not a folder. Preflight
                // looks at what it is (P2, P6).
                return match (e, canonical(dest)) {
                    (VolumeError::Unsupported, Ok(path)) => Ok(Dest {
                        path,
                        info: None,
                        source: None,
                    }),
                    (VolumeError::Unsupported, Err(_)) => Ok(Dest {
                        path: dest.to_path_buf(),
                        info: None,
                        source: None,
                    }),
                    (e, _) => Err(refused(e.to_string())),
                };
            }
            let (Some(parent), Some(name)) = (dest.parent(), dest.file_name()) else {
                return Err(refused("not a folder Culla can use".to_owned()));
            };
            let Some(name) = name.to_str().map(str::to_owned) else {
                return Err(refused("the name is not valid Unicode".to_owned()));
            };
            let parent = if parent.as_os_str().is_empty() {
                Path::new(".")
            } else {
                parent
            };
            match resolve_folder(parent) {
                Ok(r) => {
                    let mut rel = r.source.components().to_vec();
                    rel.push(name.clone());
                    let source =
                        Source::new(r.volume.id, rel).map_err(|e| refused(e.to_string()))?;
                    Ok(Dest {
                        path: r.canonical.join(&name),
                        info: Some(r.volume),
                        source: Some(source),
                    })
                }
                Err(e @ VolumeError::Path(_)) => Err(refused(e.to_string())),
                Err(e @ VolumeError::Win32 { .. }) => Err(Failure::Io(format!("destination: {e}"))),
                Err(VolumeError::Unsupported | VolumeError::Io(_)) => Ok(Dest {
                    path: canonical(parent).map_or_else(|_| dest.to_path_buf(), |p| p.join(&name)),
                    info: None,
                    source: None,
                }),
            }
        }
    }
}

/// A source on an unknown volume (serial 0, which no mounted volume has, so P1 and P2 block),
/// with the path's own components: off Windows, or a destination whose volume is unknown.
fn placeholder_source(path: &Path) -> Source {
    let unknown = VolumeId { serial: 0 };
    let parts: Option<Vec<String>> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_str().map(str::to_owned)),
            _ => None,
        })
        .collect();
    parts
        .and_then(|p| Source::new(unknown, p).ok())
        .or_else(|| Source::new(unknown, Vec::new()).ok())
        .unwrap_or_else(|| unreachable!("an empty path is a valid source"))
}

fn dest_root_fact(path: &Path) -> DestRoot {
    match lookup(path) {
        Lookup::Folder(a) => DestRoot::Folder(a),
        // A link is described by its own attributes: P6 refuses it.
        Lookup::Other(a) if a.reparse_point => DestRoot::Folder(a),
        Lookup::Other(_) | Lookup::File { .. } => DestRoot::NotAFolder,
        Lookup::Unreadable(_) => DestRoot::Unreadable,
        Lookup::Missing => match path.parent().map(lookup) {
            Some(Lookup::Folder(a) | Lookup::Other(a)) => DestRoot::ParentExists(a),
            Some(Lookup::Missing) | None => DestRoot::Missing,
            Some(Lookup::File { .. } | Lookup::Unreadable(_)) => DestRoot::Unreadable,
        },
    }
}

fn add_folder_fact(facts: &mut Facts, tree: Tree, path: &[String], l: &Lookup) {
    match l {
        Lookup::Folder(a) | Lookup::Other(a) => facts.add_folder(tree, path, *a),
        // A file where a folder should be: it cannot be looked at as a folder.
        Lookup::File { .. } | Lookup::Unreadable(_) => facts.add_unreadable_folder(tree, path),
        // No fact: a folder that must exist holds or blocks (fails closed).
        Lookup::Missing => {}
    }
}

fn add_target_facts(facts: &mut Facts, t: &TargetFacts, root: Root) {
    for (p, f) in &t.folders {
        let (tree, path) = match root {
            Root::Dest if p.is_empty() => continue,
            Root::Dest => (Tree::Dest, p.clone()),
            Root::Rejected => {
                let mut path = vec![REJECTED.to_owned()];
                path.extend(p.iter().cloned());
                (Tree::Source, path)
            }
        };
        match f {
            FolderFact::Exists(a) => facts.add_folder(tree, &path, *a),
            FolderFact::Unreadable => facts.add_unreadable_folder(tree, &path),
        }
    }
}

/// The folders above a root, from the volume root down: `root.components()[..n]` is the
/// `len - n`-th ancestor of the root's final path.
fn add_ancestor_facts(facts: &mut Facts, root: &Source, path: &Path) {
    let c = root.components();
    for n in 1..c.len() {
        if let Some(p) = path.ancestors().nth(c.len() - n) {
            add_folder_fact(facts, Tree::Volume(root.volume()), &c[..n], &lookup(p));
        }
    }
}

/// Whether `folder` is `dest` or below it (components from the source root, folded). An empty
/// `dest` (the source itself) holds nothing here: preflight refuses it (P6).
fn under_folder(folder: &[String], dest: &[String]) -> bool {
    !dest.is_empty()
        && folder.len() >= dest.len()
        && folder.iter().zip(dest).all(|(a, b)| eq_folded(a, b))
}

fn path_of(root: &Path, components: &[String]) -> PathBuf {
    let mut p = root.to_path_buf();
    p.extend(components);
    p
}

/// Counts shown under "totals".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Totals {
    /// Groups scanned, the destination's left out.
    pub groups: usize,
    /// Groups kept, after the fold (ids the scan has).
    pub kept: usize,
    /// Groups rejected, after the fold (ids the scan has).
    pub rejected: usize,
    /// Groups with operations.
    pub planned_groups: usize,
    /// Operations.
    pub operations: usize,
    /// Renames.
    pub renames: usize,
    /// Copies.
    pub copies: usize,
    /// Cross-volume moves.
    pub xmoves: usize,
    /// Bytes copied by copies.
    pub bytes_copy: u64,
    /// Bytes copied by cross-volume moves.
    pub bytes_xmove: u64,
    /// Decided groups the plan holds back.
    pub held_by_plan: usize,
    /// Planned groups preflight holds back.
    pub held_by_preflight: usize,
    /// Members that would have moved but are missing.
    pub missing: usize,
    /// Operations left once preflight's held groups are taken out.
    pub runnable: usize,
}

impl Totals {
    /// The totals of `r`.
    pub(crate) fn of(r: &Report) -> Self {
        let known: BTreeSet<GroupId> = r.groups.iter().map(|g| g.id).collect();
        let state_count = |s: GroupState| {
            r.states
                .iter()
                .filter(|(g, st)| **st == s && known.contains(g))
                .count()
        };
        let kind = |k: OpKind| r.plan.ops.iter().filter(|o| o.kind == k).count();
        Self {
            groups: r.groups.len(),
            kept: state_count(GroupState::Kept),
            rejected: state_count(GroupState::Rejected),
            planned_groups: r.plan.groups.len(),
            operations: r.plan.ops.len(),
            renames: kind(OpKind::Rename),
            copies: kind(OpKind::Copy),
            xmoves: kind(OpKind::XMove),
            bytes_copy: r.plan.bytes_copy,
            bytes_xmove: r.plan.bytes_xmove,
            held_by_plan: r.plan.held.len(),
            held_by_preflight: r.preflight.held.len(),
            missing: r.plan.missing.len(),
            runnable: r.preflight.runnable(&r.plan).len(),
        }
    }
}

/// The JSON id and text of a plan hold.
pub(crate) fn held_label(h: Held) -> (&'static str, &'static str) {
    match h {
        Held::Ambiguous => (
            "ambiguous",
            "ambiguous: more than one JPEG or NEF with this name",
        ),
        Held::Applied => ("applied", "already applied"),
        Held::Locked => ("locked", "in another open plan"),
        Held::NothingPresent => ("nothing_present", "every file that would move is missing"),
        Held::NoFreeSuffix => ("no_free_suffix", "no free name up to _9999"),
        Held::TargetFolderBlocked => (
            "target_folder_blocked",
            "a file is where a target folder should be",
        ),
        Held::TargetNameInvalid => (
            "target_name_invalid",
            "a target name would not be a valid Windows name",
        ),
        Held::NameMismatch => (
            "name_mismatch",
            "a member's name does not start with the stem",
        ),
    }
}

/// The JSON id, text and member of a preflight hold.
pub(crate) fn hold_label(h: Hold) -> (&'static str, &'static str, MemberId) {
    match h {
        Hold::PathTooLong { member } => ("path_too_long", "a target path is too long", member),
        Hold::Link { member } => ("link", "a link on the way", member),
        Hold::Placeholder { member } => ("placeholder", "a cloud placeholder on the way", member),
        Hold::FolderUnknown { member } => (
            "folder_unknown",
            "a folder on the way could not be looked at",
            member,
        ),
        Hold::ChangedSinceScan { member } => (
            "changed_since_scan",
            "changed since the scan, rescan",
            member,
        ),
        Hold::TooLargeForFat32 { member } => (
            "too_large_for_fat32",
            "over 4 GiB for a FAT32 volume",
            member,
        ),
    }
}

/// The JSON id, text and member of a re-plan.
pub(crate) fn replan_label(r: Replan) -> (&'static str, &'static str, Option<MemberId>) {
    match r {
        Replan::TargetTaken { member } => {
            ("target_taken", "a target is no longer free", Some(member))
        }
        Replan::ReadOnlyAppeared { member } => (
            "read_only_appeared",
            "a file became read-only",
            Some(member),
        ),
        Replan::VolumeRelationChanged => (
            "volume_relation_changed",
            "the destination's volume relation changed",
            None,
        ),
    }
}

/// The JSON id of a blocker.
pub(crate) fn blocker_id(b: &Blocker) -> &'static str {
    match b {
        Blocker::SourceVolume(_) => "source_volume",
        Blocker::SourceVolumeRefused(_) => "source_volume_refused",
        Blocker::SourceRootLink => "source_root_link",
        Blocker::SourceRootUnreadable => "source_root_unreadable",
        Blocker::SourceAncestor(_) => "source_ancestor",
        Blocker::NoDestination => "no_destination",
        Blocker::DestVolume(_) => "dest_volume",
        Blocker::DestRootMissing => "dest_root_missing",
        Blocker::DestRootUnreadable => "dest_root_unreadable",
        Blocker::DestAncestor(_) => "dest_ancestor",
        Blocker::SourceReadOnly => "source_read_only",
        Blocker::DestReadOnly => "dest_read_only",
        Blocker::NoSpace { .. } => "no_space",
        Blocker::SpaceUnknown(_) => "space_unknown",
        Blocker::DestIsSource => "dest_is_source",
        Blocker::DestInRejected => "dest_in_rejected",
        Blocker::DestPlaceholder => "dest_placeholder",
        Blocker::DestLink => "dest_link",
        Blocker::DestNotAFolder => "dest_not_a_folder",
        Blocker::DestVolumeRefused(_) => "dest_volume_refused",
        Blocker::Inconsistent { .. } => "inconsistent",
        Blocker::PreviewBusy => "preview_busy",
    }
}

/// The JSON id and text of a warning.
pub(crate) fn warning_label(w: &Warning) -> (&'static str, String) {
    match w {
        Warning::Locked { .. } => ("locked", "another program holds the file".to_owned()),
        Warning::LongPath { len, .. } => (
            "long_path",
            format!("the target path is {len} characters: some programs may not open it"),
        ),
        Warning::CloudSynced => (
            "cloud_synced",
            "the destination is in a cloud-synced folder".to_owned(),
        ),
        Warning::SyncRootUnknown => (
            "sync_root_unknown",
            "whether the destination is in a cloud-synced folder is unknown".to_owned(),
        ),
    }
}

fn warning_member(w: &Warning) -> Option<(GroupId, MemberId)> {
    match w {
        Warning::Locked { group, member } | Warning::LongPath { group, member, .. } => {
            Some((*group, *member))
        }
        Warning::CloudSynced | Warning::SyncRootUnknown => None,
    }
}

fn mode_str(m: KeepMode) -> &'static str {
    match m {
        KeepMode::Move => "move",
        KeepMode::Copy => "copy",
    }
}

fn target_str(root: Root, dst: &[String]) -> String {
    let base = match root {
        Root::Dest => "<dest>",
        Root::Rejected => REJECTED,
    };
    join(&[base.to_owned()], &dst.join("\\"))
}

/// The report as text, ending with a newline. Untrusted text (paths, names) is escaped.
pub(crate) fn render(r: &Report) -> String {
    let mut o = String::new();
    let mut line = |s: String| {
        o.push_str(&s);
        o.push('\n');
    };
    line(format!(
        "folder:      {}",
        shown(&r.scan.folder.to_string_lossy())
    ));
    if r.scan.scanned != r.scan.folder {
        line(format!(
            "resolved:    {}",
            shown(&r.scan.scanned.to_string_lossy())
        ));
    }
    line(format!(
        "decisions:   {} ({})",
        shown(&r.decisions_path.to_string_lossy()),
        count(r.events as u64, "event")
    ));
    match &r.keep {
        Some((mode, d)) => {
            line(format!(
                "keep:        {} to {}",
                mode_str(*mode),
                shown(&d.to_string_lossy())
            ));
            if let Some(p) = &r.dest_resolved
                && p != d
            {
                line(format!("dest:        {}", shown(&p.to_string_lossy())));
            }
            if r.dest_inside_source {
                line("             inside the source: its photos are not planned".to_owned());
            }
            line(format!(
                "same volume: {}",
                match r.same_volume {
                    Some(true) => "yes",
                    Some(false) => "no",
                    None => "unknown",
                }
            ));
        }
        None => line("keep:        no destination (rejects only)".to_owned()),
    }
    line(format!(
        "raw follows: {}",
        if r.raw_follows { "yes" } else { "no" }
    ));
    line(format!(
        "subfolders:  {}",
        if r.scan.include_subfolders {
            "included"
        } else {
            "not included"
        }
    ));
    line("This is a dry run: nothing was moved, copied or written.".to_owned());
    line(String::new());

    if !r.unknown_ids.is_empty() {
        let ids: Vec<String> = r.unknown_ids.iter().map(ToString::to_string).collect();
        line(format!(
            "decisions for groups the scan does not have (ignored): {}",
            ids.join(", ")
        ));
        line(String::new());
    }
    if !r.dest_ids.is_empty() {
        let ids: Vec<String> = r.dest_ids.iter().map(ToString::to_string).collect();
        line(format!(
            "decisions for groups inside the destination (ignored): {}",
            ids.join(", ")
        ));
        line(String::new());
    }

    line(format!("operations ({}):", r.plan.ops.len()));
    for op in &r.plan.ops {
        let held = if r.preflight.is_held(op.group) {
            "  [held]"
        } else {
            ""
        };
        line(format!(
            "  {:>4}  #{:<4} {:<6} {:<6} {} -> {}{held}",
            op.ord,
            op.group,
            op.action.as_str(),
            op.kind.as_str(),
            shown(&op.src.join("\\")),
            shown(&target_str(op.root, &op.dst)),
        ));
    }
    let notable: Vec<_> = r
        .plan
        .groups
        .iter()
        .filter(|g| g.suffix > 0 || g.protected)
        .collect();
    if !notable.is_empty() {
        line(String::new());
        line("renamed or camera-protected groups:".to_owned());
        for g in notable {
            let mut what = Vec::new();
            if g.suffix > 0 {
                what.push(format!("suffix _{}", g.suffix));
            }
            if g.protected {
                what.push("camera-protected: copied, original kept".to_owned());
            }
            line(format!(
                "  #{:<4} {}: {}",
                g.group,
                shown(&r.group_path(g.group)),
                what.join(", ")
            ));
        }
    }
    if !r.plan.held.is_empty() {
        line(String::new());
        line(format!("held back by the plan ({}):", r.plan.held.len()));
        for (g, h) in &r.plan.held {
            line(format!(
                "  #{g:<4} {}: {}",
                shown(&r.group_path(*g)),
                held_label(*h).1
            ));
        }
    }
    if !r.plan.missing.is_empty() {
        line(String::new());
        line(format!("missing, not moved ({}):", r.plan.missing.len()));
        for (g, m) in &r.plan.missing {
            line(format!("  #{g:<4} {}", shown(&r.member_path(*m))));
        }
    }

    line(String::new());
    let p = &r.preflight;
    line(format!(
        "preflight: {}",
        if p.can_apply() {
            "apply could start"
        } else {
            "apply could not start"
        }
    ));
    if !p.blockers.is_empty() {
        line(format!("  blockers ({}):", p.blockers.len()));
        for b in &p.blockers {
            line(format!("    {}", shown(&b.to_string())));
        }
    }
    if !p.held.is_empty() {
        line(format!("  held back ({}):", p.held.len()));
        for (g, h) in &p.held {
            let (_, text, m) = hold_label(*h);
            line(format!("    #{g:<4} {}: {text}", shown(&r.member_path(m))));
        }
    }
    if !p.replan.is_empty() {
        line(format!("  plan again ({}):", p.replan.len()));
        for (g, rp) in &p.replan {
            let (_, text, m) = replan_label(*rp);
            let path = m.map_or_else(|| r.group_path(*g), |m| r.member_path(m));
            line(format!("    #{g:<4} {}: {text}", shown(&path)));
        }
    }
    if !p.warnings.is_empty() {
        line(format!("  warnings ({}):", p.warnings.len()));
        for w in &p.warnings {
            let (_, text) = warning_label(w);
            match warning_member(w) {
                Some((g, m)) => line(format!("    #{g:<4} {}: {text}", shown(&r.member_path(m)))),
                None => line(format!("    {text}")),
            }
        }
    }
    for (closed, heading) in [
        (true, "  unknown, preflight fails closed:"),
        (false, "  not checked:"),
    ] {
        let items: Vec<_> = r
            .unchecked
            .iter()
            .filter(|u| u.fails_closed() == closed)
            .collect();
        if !items.is_empty() {
            line(heading.to_owned());
            for u in items {
                line(format!("    {}", u.text()));
            }
        }
    }

    let t = Totals::of(r);
    line(String::new());
    line("totals:".to_owned());
    line(format!(
        "  decided:     {} kept, {} rejected, of {}",
        t.kept,
        t.rejected,
        count(t.groups as u64, "group")
    ));
    line(format!(
        "  planned:     {}, {} ({} rename, {} copy, {} cross-volume move)",
        count(t.planned_groups as u64, "group"),
        count(t.operations as u64, "operation"),
        t.renames,
        t.copies,
        t.xmoves
    ));
    line(format!(
        "  bytes:       {} copied, {} moved across volumes",
        t.bytes_copy, t.bytes_xmove
    ));
    line(format!(
        "  held back:   {} by the plan, {} by preflight; {} missing",
        t.held_by_plan,
        t.held_by_preflight,
        count(t.missing as u64, "file")
    ));
    line(format!(
        "  runnable:    {}",
        count(t.runnable as u64, "operation")
    ));
    o
}

/// The report as one JSON object (`--json`).
pub(crate) fn to_json(r: &Report) -> Value {
    let p = &r.preflight;
    let ops = r
        .plan
        .ops
        .iter()
        .map(|o| {
            Value::Obj(vec![
                ("ord", Value::int(o.ord)),
                ("group", Value::int(o.group)),
                ("member", Value::int(o.member)),
                ("action", Value::str(o.action.as_str())),
                ("kind", Value::str(o.kind.as_str())),
                ("src", Value::str(o.src.join("\\"))),
                ("root", Value::str(o.root.as_str())),
                ("dst", Value::str(o.dst.join("\\"))),
                ("size", Value::int(o.expected_size)),
                ("held", Value::Bool(p.is_held(o.group))),
            ])
        })
        .collect();
    let planned = r
        .plan
        .groups
        .iter()
        .map(|g| {
            Value::Obj(vec![
                ("group", Value::int(g.group)),
                ("primary", Value::str(r.group_path(g.group))),
                ("action", Value::str(g.action.as_str())),
                ("kind", Value::str(g.kind.as_str())),
                ("suffix", Value::int(g.suffix)),
                ("camera_protected", Value::Bool(g.protected)),
            ])
        })
        .collect();
    let held = r
        .plan
        .held
        .iter()
        .map(|(g, h)| {
            Value::Obj(vec![
                ("group", Value::int(*g)),
                ("primary", Value::str(r.group_path(*g))),
                ("reason", Value::str(held_label(*h).0)),
            ])
        })
        .collect();
    let missing = r
        .plan
        .missing
        .iter()
        .map(|(g, m)| {
            Value::Obj(vec![
                ("group", Value::int(*g)),
                ("path", Value::str(r.member_path(*m))),
            ])
        })
        .collect();
    let blockers = p
        .blockers
        .iter()
        .map(|b| {
            Value::Obj(vec![
                ("id", Value::str(blocker_id(b))),
                ("text", Value::str(b.to_string())),
            ])
        })
        .collect();
    let pf_held = p
        .held
        .iter()
        .map(|(g, h)| {
            let (id, _, m) = hold_label(*h);
            Value::Obj(vec![
                ("group", Value::int(*g)),
                ("reason", Value::str(id)),
                ("path", Value::str(r.member_path(m))),
            ])
        })
        .collect();
    let replan = p
        .replan
        .iter()
        .map(|(g, rp)| {
            let (id, _, m) = replan_label(*rp);
            Value::Obj(vec![
                ("group", Value::int(*g)),
                ("reason", Value::str(id)),
                ("path", Value::opt(m, |m| Value::str(r.member_path(m)))),
            ])
        })
        .collect();
    let warnings = p
        .warnings
        .iter()
        .map(|w| {
            let (id, text) = warning_label(w);
            let at = warning_member(w);
            Value::Obj(vec![
                ("id", Value::str(id)),
                ("text", Value::str(text)),
                ("group", Value::opt(at, |(g, _)| Value::int(g))),
                (
                    "path",
                    Value::opt(at, |(_, m)| Value::str(r.member_path(m))),
                ),
            ])
        })
        .collect();
    let unchecked = r
        .unchecked
        .iter()
        .map(|u| {
            Value::Obj(vec![
                ("id", Value::str(u.id())),
                ("text", Value::str(u.text())),
                ("fails_closed", Value::Bool(u.fails_closed())),
            ])
        })
        .collect();
    let t = Totals::of(r);
    let n = |x: usize| Value::int(x as u64);
    let totals = Value::Obj(vec![
        ("groups", n(t.groups)),
        ("kept", n(t.kept)),
        ("rejected", n(t.rejected)),
        ("planned_groups", n(t.planned_groups)),
        ("operations", n(t.operations)),
        ("renames", n(t.renames)),
        ("copies", n(t.copies)),
        ("xmoves", n(t.xmoves)),
        ("bytes_copy", Value::int(t.bytes_copy)),
        ("bytes_xmove", Value::int(t.bytes_xmove)),
        ("held_by_plan", n(t.held_by_plan)),
        ("held_by_preflight", n(t.held_by_preflight)),
        ("missing", n(t.missing)),
        ("runnable", n(t.runnable)),
    ]);
    Value::Obj(vec![
        // The field set is provisional until M2 (SPEC §10): consumers must not rely on it.
        ("schema", Value::str("unstable")),
        ("culla_version", Value::str(culla_adapters::VERSION)),
        ("dry_run", Value::Bool(true)),
        (
            "folder",
            Value::str(r.scan.folder.to_string_lossy().into_owned()),
        ),
        (
            "scanned_path",
            Value::str(r.scan.scanned.to_string_lossy().into_owned()),
        ),
        ("include_subfolders", Value::Bool(r.scan.include_subfolders)),
        (
            "decisions",
            Value::Obj(vec![
                (
                    "path",
                    Value::str(r.decisions_path.to_string_lossy().into_owned()),
                ),
                ("events", n(r.events)),
                (
                    "unknown_groups",
                    Value::Arr(r.unknown_ids.iter().map(|g| Value::int(*g)).collect()),
                ),
                (
                    "destination_groups",
                    Value::Arr(r.dest_ids.iter().map(|g| Value::int(*g)).collect()),
                ),
            ]),
        ),
        (
            "settings",
            Value::Obj(vec![
                ("raw_follows", Value::Bool(r.raw_follows)),
                (
                    "keep",
                    Value::opt(r.keep.as_ref(), |(m, _)| Value::str(mode_str(*m))),
                ),
                (
                    "dest",
                    Value::opt(r.keep.as_ref(), |(_, d)| {
                        Value::str(d.to_string_lossy().into_owned())
                    }),
                ),
                (
                    "dest_resolved",
                    Value::opt(r.dest_resolved.as_ref(), |d| {
                        Value::str(d.to_string_lossy().into_owned())
                    }),
                ),
                ("dest_inside_source", Value::Bool(r.dest_inside_source)),
                ("same_volume", Value::opt(r.same_volume, Value::Bool)),
            ]),
        ),
        ("operations", Value::Arr(ops)),
        ("planned_groups", Value::Arr(planned)),
        ("held_by_plan", Value::Arr(held)),
        ("missing", Value::Arr(missing)),
        (
            "preflight",
            Value::Obj(vec![
                ("can_apply", Value::Bool(p.can_apply())),
                ("blockers", Value::Arr(blockers)),
                ("held", Value::Arr(pf_held)),
                ("replan", Value::Arr(replan)),
                ("warnings", Value::Arr(warnings)),
                ("unchecked", Value::Arr(unchecked)),
            ]),
        ),
        ("totals", totals),
    ])
}
