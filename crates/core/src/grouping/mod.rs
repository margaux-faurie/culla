//! Scan exclusions and grouping (SPEC §3.1, design/file-engine.md §3.2–§3.3, stories US-02 to
//! US-05).
//!
//! The adapter walks the source and describes each directory entry as plain data
//! ([`ScanEntry`]). This module decides, without any I/O:
//!
//! - which folders the walk enters ([`ScanPolicy::dir_action`]): never `_rejected` (at any
//!   depth), an in-source destination, a reparse point, a cloud placeholder, a hidden or system
//!   folder, or a folder with an invalid name; `NKSC_PARAM` is read as part of its parent;
//! - how the files of one folder form groups ([`group`]): a primary (the JPEG, else the NEF), an
//!   optional raw (the NEF next to a JPEG), sidecars bound to the file they describe, plus the
//!   files that are listed but never touched ([`Unknown`]) and the entries left out with a reason
//!   ([`Excluded`]).
//!
//! # Matching
//!
//! Everything is matched within one folder, never across folders (ADR 0007), so `DSC_0001.JPG`
//! in `100NIKON` and `DSC_0001.NEF` in `101NIKON` are two groups. Extensions compare ASCII
//! case-insensitively and stems and names with [`fold`], as Windows does. Two JPEGs or two NEFs
//! whose stems fold to the same value (`X.JPG` and `X.jpeg`, or `X.JPG` and `x.JPG` in a
//! case-sensitive folder) make the group [`GroupStatus::Ambiguous`]: it is shown and can be
//! decided, but the plan gives it no operations (file-engine §3.3). Two folders whose names fold
//! to the same value (only possible in a case-sensitive folder) are kept apart, and every group
//! whose folder and stem collide with another's under folding is marked ambiguous too
//! (proposed ADR 0027).
//!
//! # Order
//!
//! Output is deterministic whatever the input order: groups by folder (component by component)
//! then stem, each compared folded first and as written second; within a group, sidecars by
//! folder then name; unknown and excluded entries by folder then name. Sorting folders
//! component by component is the pre-order of a depth-first walk with sorted children, which is
//! the order the adapter's streamed scan emits folders in.
//!
//! # Batches
//!
//! [`group`] needs every entry of a folder at once, with the entries of its `NKSC_PARAM`
//! subfolder: the adapter emits one batch per folder that way, so grouping a batch gives the
//! groups of that folder (US-01). Grouping the whole tree in one call gives the same result as
//! grouping each batch and combining the parts with [`Grouping::merge`]. The one thing a batch
//! cannot see is a folder whose name folds like another folder's (case-sensitive folders only):
//! `merge` marks those groups ambiguous, so a group streamed as `Ok` can turn `Ambiguous` at the
//! end of the scan in that case.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt;

use crate::names::{eq_folded, fold, split_ext, validate_component};

/// Folder that receives rejected groups. Never entered, at any depth (file-engine §3.2).
pub const REJECTED_DIR: &str = "_rejected";
/// NX Studio's sidecar folder: its `*.nksc` files belong to the parent folder.
pub const NKSC_DIR: &str = "NKSC_PARAM";
/// Extension of Culla's own partial copies (file-engine §9.4). Never grouped.
pub const PARTIAL_EXT: &str = "culla-partial";

/// File attributes the scan needs, as read from the directory listing without following links.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Attributes {
    /// `FILE_ATTRIBUTE_READONLY` (camera "protect"): the group is planned as a copy (SPEC §2).
    pub read_only: bool,
    /// `FILE_ATTRIBUTE_HIDDEN`.
    pub hidden: bool,
    /// `FILE_ATTRIBUTE_SYSTEM`.
    pub system: bool,
    /// A symlink, junction, mount point or any other reparse point (threat model T2).
    pub reparse_point: bool,
    /// A cloud placeholder: `RECALL_ON_DATA_ACCESS`, `RECALL_ON_OPEN` or `OFFLINE`, or a cloud
    /// reparse tag (threat model T2b).
    pub cloud_placeholder: bool,
}

/// What a directory entry is, from its file type (never through a link).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum EntryKind {
    /// A regular file.
    File,
    /// A directory.
    Dir,
    /// Anything else: a symlink, a device, a socket.
    Other,
    /// The entry, or the folder it names, could not be read; the text says why.
    Unreadable(String),
}

/// One directory entry, as the adapter saw it. Plain data: building one does no I/O.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ScanEntry {
    /// The folder holding the entry, as components from the source root (empty for the root).
    pub folder: Vec<String>,
    /// The entry's name as listed (lossily converted when `name_lossy` is set).
    pub name: String,
    /// The listed name was not valid Unicode, so `name` is a lossy display form. Such an entry
    /// is excluded as an invalid name (file-engine §3.4).
    pub name_lossy: bool,
    /// File, folder or other.
    pub kind: EntryKind,
    /// Size in bytes (0 for folders).
    pub size: u64,
    /// Modification time in nanoseconds since the Unix epoch, when known.
    pub mtime_ns: Option<i64>,
    /// Attributes from the listing.
    pub attrs: Attributes,
}

impl ScanEntry {
    /// An entry of `kind` named `name` in `folder`, with size 0, no mtime and no attributes.
    #[must_use]
    pub fn new(folder: &[&str], name: &str, kind: EntryKind) -> Self {
        Self {
            folder: folder.iter().map(|c| (*c).to_owned()).collect(),
            name: name.to_owned(),
            name_lossy: false,
            kind,
            size: 0,
            mtime_ns: None,
            attrs: Attributes::default(),
        }
    }

    /// A regular file.
    #[must_use]
    pub fn file(folder: &[&str], name: &str) -> Self {
        Self::new(folder, name, EntryKind::File)
    }

    /// A folder.
    #[must_use]
    pub fn dir(folder: &[&str], name: &str) -> Self {
        Self::new(folder, name, EntryKind::Dir)
    }

    /// The path from the source root, components joined with `\` (the `RelPath` form,
    /// file-engine §2.1).
    #[must_use]
    pub fn rel_path(&self) -> String {
        let mut s = String::new();
        for c in &self.folder {
            s.push_str(c);
            s.push('\\');
        }
        s.push_str(&self.name);
        s
    }

    /// The folder and the name as one list of components.
    #[must_use]
    pub fn components(&self) -> Vec<String> {
        let mut path = self.folder.clone();
        path.push(self.name.clone());
        path
    }
}

/// Why an entry is left out of grouping. Excluded entries are never touched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ExclusionReason {
    /// A `_rejected` folder or anything under one.
    RejectedFolder,
    /// The session's destination folder inside the source, or anything under it.
    Destination,
    /// A folder below the source root while "include subfolders" is off.
    NotIncluded,
    /// A reparse point (symlink, junction, mount point): never followed (threat model T2).
    ReparsePoint,
    /// A cloud placeholder: reading it would trigger a download (threat model T2b).
    CloudPlaceholder,
    /// Hidden or system attribute, a name starting with `.`, `System Volume Information` or
    /// `$RECYCLE.BIN`.
    HiddenOrSystem,
    /// A name that fails validation (file-engine §3.4).
    InvalidName,
    /// A `*.culla-partial` file. Recovery owns it if this source's journal lists it; otherwise
    /// it is reported as foreign and never touched.
    CullaPartial,
    /// Neither a regular file nor a folder.
    NotAFile,
    /// Could not be read.
    Unreadable,
}

impl ExclusionReason {
    /// The `scan_issue.kind` this exclusion is reported as (file-engine §2.3), or `None` when it
    /// is only counted. Hidden and system folders are counted, hidden files reported.
    #[must_use]
    pub fn issue_kind(self, is_dir: bool) -> Option<&'static str> {
        match self {
            Self::RejectedFolder | Self::Destination | Self::NotIncluded => None,
            Self::ReparsePoint => Some("reparse_point"),
            Self::CloudPlaceholder => Some("cloud_placeholder"),
            Self::HiddenOrSystem => (!is_dir).then_some("hidden_or_system"),
            Self::InvalidName => Some("invalid_name"),
            Self::CullaPartial => Some("foreign_partial"),
            Self::NotAFile | Self::Unreadable => Some("unreadable"),
        }
    }
}

impl fmt::Display for ExclusionReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::RejectedFolder => "inside a _rejected folder",
            Self::Destination => "inside the destination folder",
            Self::NotIncluded => "in a subfolder, and subfolders are not included",
            Self::ReparsePoint => "a link or junction, not followed",
            Self::CloudPlaceholder => "a cloud placeholder, not downloaded",
            Self::HiddenOrSystem => "hidden or system",
            Self::InvalidName => "invalid name",
            Self::CullaPartial => "a partial copy left by Culla",
            Self::NotAFile => "not a file or folder",
            Self::Unreadable => "unreadable",
        })
    }
}

/// What the walk does with a folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirAction {
    /// List it as a folder of its own.
    Enter,
    /// `NKSC_PARAM`: list it now and put its entries in the parent folder's batch, because its
    /// `*.nksc` files are matched against the parent's photos. Entered even when hidden or when
    /// subfolders are off.
    Sidecars,
    /// Do not list it. The reason is reported in [`Grouping::excluded`].
    Skip(ExclusionReason),
}

/// Session options that shape the scan.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanPolicy {
    /// "Include subfolders" (SPEC §2). When off, only the root folder and its `NKSC_PARAM` are
    /// read.
    pub include_subfolders: bool,
    /// The destination folder when it lies inside the source, as components from the source
    /// root. The caller resolves the destination against the source (final paths and volume
    /// identity, file-engine §3.2); this module compares components folded. An empty list (the
    /// source itself) excludes nothing here: preflight P6 refuses it.
    pub destination: Option<Vec<String>>,
}

impl ScanPolicy {
    /// What the walk does with the folder `dir` (an entry of kind [`EntryKind::Dir`]).
    ///
    /// Any other kind of entry gets the reason it is left out ([`DirAction::Skip`]).
    #[must_use]
    pub fn dir_action(&self, dir: &ScanEntry) -> DirAction {
        if let Some(r) = entry_exclusion(dir) {
            return DirAction::Skip(r);
        }
        if dir.kind != EntryKind::Dir {
            return DirAction::Skip(ExclusionReason::NotAFile);
        }
        let path = dir.components();
        if let Some(r) = self.path_exclusion(&path) {
            return DirAction::Skip(r);
        }
        let sidecars = is_sidecar_dir(&dir.folder, &dir.name);
        if !sidecars && (dir.attrs.hidden || dir.attrs.system) {
            return DirAction::Skip(ExclusionReason::HiddenOrSystem);
        }
        if sidecars {
            DirAction::Sidecars
        } else {
            DirAction::Enter
        }
    }

    /// Whether anything under the folder `path` is left out, from the folder names alone.
    /// The first component that is excluded gives the reason.
    fn path_exclusion(&self, path: &[String]) -> Option<ExclusionReason> {
        for (i, comp) in path.iter().enumerate() {
            if validate_component(comp).is_err() {
                return Some(ExclusionReason::InvalidName);
            }
            if eq_folded(comp, REJECTED_DIR) {
                return Some(ExclusionReason::RejectedFolder);
            }
            if self
                .destination
                .as_deref()
                .is_some_and(|d| same_path(d, &path[..=i]))
            {
                return Some(ExclusionReason::Destination);
            }
            if !is_sidecar_dir(&path[..i], comp) && is_hidden_name(comp) {
                return Some(ExclusionReason::HiddenOrSystem);
            }
        }
        let root_only = match path {
            [] => true,
            [only] => eq_folded(only, NKSC_DIR),
            _ => false,
        };
        if !self.include_subfolders && !root_only {
            return Some(ExclusionReason::NotIncluded);
        }
        None
    }

    /// Why the file `e` is left out of grouping, if it is.
    fn file_exclusion(&self, e: &ScanEntry) -> Option<ExclusionReason> {
        if let Some(r) = entry_exclusion(e) {
            return Some(r);
        }
        if e.kind != EntryKind::File {
            return Some(ExclusionReason::NotAFile);
        }
        if let Some(r) = self.path_exclusion(&e.folder) {
            return Some(r);
        }
        if e.attrs.hidden || e.attrs.system || is_hidden_name(&e.name) {
            return Some(ExclusionReason::HiddenOrSystem);
        }
        if split_ext(&e.name)
            .1
            .is_some_and(|x| x.eq_ignore_ascii_case(PARTIAL_EXT))
        {
            return Some(ExclusionReason::CullaPartial);
        }
        None
    }
}

/// Exclusions that hold for any entry, from its own listing data.
fn entry_exclusion(e: &ScanEntry) -> Option<ExclusionReason> {
    if matches!(e.kind, EntryKind::Unreadable(_)) {
        return Some(ExclusionReason::Unreadable);
    }
    if e.attrs.cloud_placeholder {
        return Some(ExclusionReason::CloudPlaceholder);
    }
    if e.attrs.reparse_point {
        return Some(ExclusionReason::ReparsePoint);
    }
    if e.name_lossy || validate_component(&e.name).is_err() {
        return Some(ExclusionReason::InvalidName);
    }
    None
}

fn is_hidden_name(name: &str) -> bool {
    name.starts_with('.')
        || eq_folded(name, "System Volume Information")
        || eq_folded(name, "$RECYCLE.BIN")
}

/// `NKSC_PARAM` directly under a folder that is not itself inside an `NKSC_PARAM`.
fn is_sidecar_dir(parent: &[String], name: &str) -> bool {
    eq_folded(name, NKSC_DIR) && !parent.iter().any(|c| eq_folded(c, NKSC_DIR))
}

fn same_path(a: &[String], b: &[String]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| eq_folded(x, y))
}

/// JPEG or NEF.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PhotoKind {
    /// `.jpg` or `.jpeg`.
    Jpeg,
    /// `.nef`.
    Nef,
}

impl PhotoKind {
    /// The kind of a photo file from its extension (ASCII case-insensitive).
    #[must_use]
    pub fn from_ext(ext: &str) -> Option<Self> {
        if ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg") {
            Some(Self::Jpeg)
        } else if ext.eq_ignore_ascii_case("nef") {
            Some(Self::Nef)
        } else {
            None
        }
    }
}

/// A photo file of a group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Photo {
    /// JPEG or NEF.
    pub kind: PhotoKind,
    /// The file as scanned.
    pub entry: ScanEntry,
}

/// Sidecar kinds, as in the `member.sidecar_kind` column (file-engine §2.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SidecarKind {
    /// Bare `X.xmp` (Lightroom, for the RAW).
    Xmp,
    /// Bare `X.pp3` (RawTherapee, for the RAW).
    Pp3,
    /// `X.NEF.xmp` or `X.JPG.xmp`: describes one file.
    FileXmp,
    /// `X.NEF.pp3` or `X.JPG.pp3`: describes one file.
    FilePp3,
    /// `NKSC_PARAM/X.NEF.nksc` or `NKSC_PARAM/X.JPG.nksc` (NX Studio): describes one file.
    Nksc,
}

impl SidecarKind {
    /// The `member.sidecar_kind` value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Xmp => "xmp",
            Self::Pp3 => "pp3",
            Self::FileXmp => "file_xmp",
            Self::FilePp3 => "file_pp3",
            Self::Nksc => "nksc",
        }
    }
}

/// Which file of its group a sidecar describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Describes {
    /// A bare `X.xmp` or `X.pp3`: it describes the RAW, so it follows the NEF when the group
    /// has one and otherwise the primary (ADR 0023). See [`Group::follows`].
    Group,
    /// The group's primary.
    Primary,
    /// The group's raw (the NEF next to a JPEG).
    Raw,
    /// One of the group's duplicate photo files (ambiguous groups only), by index.
    Duplicate(usize),
}

/// A sidecar of a group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sidecar {
    /// What kind of sidecar it is.
    pub kind: SidecarKind,
    /// The file it describes.
    pub describes: Describes,
    /// The file as scanned (its folder ends in `NKSC_PARAM` for `.nksc`).
    pub entry: ScanEntry,
}

/// Whether a group can be planned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupStatus {
    /// One JPEG at most and one NEF at most.
    Ok,
    /// Two JPEGs or two NEFs with the same folded stem, or a folder and stem that collide with
    /// another group's under folding: shown and decidable, but planned with no operations.
    Ambiguous,
}

/// The unit the user decides on (SPEC §3.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// The folder, as components from the source root.
    pub folder: Vec<String>,
    /// The stem of the primary, as written.
    pub stem: String,
    /// The stem folded: with the folded folder, the group's identity in a session.
    pub stem_key: String,
    /// Whether the group can be planned.
    pub status: GroupStatus,
    /// The JPEG if the group has one, otherwise the NEF (lone NEF, US-03).
    pub primary: Photo,
    /// The NEF of a group whose primary is a JPEG. `None` for a JPEG-only group and for a lone
    /// NEF, whose NEF is the primary.
    pub raw: Option<Photo>,
    /// Further JPEGs or NEFs with the same folded stem. Non-empty only when ambiguous.
    pub duplicates: Vec<Photo>,
    /// Sidecars, each bound to the file it describes.
    pub sidecars: Vec<Sidecar>,
}

impl Group {
    /// The folded folder, components joined with `\` (`photo_group.folder_key`).
    #[must_use]
    pub fn folder_key(&self) -> String {
        fold(&self.folder.join("\\"))
    }

    /// True for a lone NEF: a group whose primary is a NEF.
    #[must_use]
    pub fn is_lone_nef(&self) -> bool {
        self.primary.kind == PhotoKind::Nef
    }

    /// The file `sidecar` moves with: [`Describes::Group`] resolves to the raw when there is
    /// one, otherwise to the primary (ADR 0023). Never returns [`Describes::Group`].
    #[must_use]
    pub fn follows(&self, sidecar: &Sidecar) -> Describes {
        match sidecar.describes {
            Describes::Group if self.raw.is_some() => Describes::Raw,
            Describes::Group => Describes::Primary,
            other => other,
        }
    }

    /// Every file of the group: primary, raw, duplicates, then sidecars.
    pub fn files(&self) -> impl Iterator<Item = &ScanEntry> {
        std::iter::once(&self.primary.entry)
            .chain(self.raw.as_ref().map(|p| &p.entry))
            .chain(self.duplicates.iter().map(|p| &p.entry))
            .chain(self.sidecars.iter().map(|s| &s.entry))
    }
}

/// Why a file is listed but not grouped (US-04). Never touched.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum UnknownKind {
    /// An extension Culla does not handle, or a non-`.nksc` file in `NKSC_PARAM`.
    UnknownExt,
    /// An unknown file with a group's stem (`X.WAV`, `X.MOV`), listed against that group.
    SameBasename {
        /// The group's `stem_key`.
        stem_key: String,
    },
    /// A sidecar name that matches no photo file and no group.
    OrphanSidecar,
}

impl UnknownKind {
    /// The `scan_issue.kind` value.
    #[must_use]
    pub fn issue_kind(&self) -> &'static str {
        match self {
            Self::UnknownExt => "unknown_ext",
            Self::SameBasename { .. } => "same_basename_unknown",
            Self::OrphanSidecar => "orphan_sidecar",
        }
    }
}

/// A file listed in the review screen, not part of any group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unknown {
    /// Why it is listed.
    pub kind: UnknownKind,
    /// The file as scanned.
    pub entry: ScanEntry,
}

/// An entry left out of grouping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Excluded {
    /// Why.
    pub reason: ExclusionReason,
    /// The entry as scanned.
    pub entry: ScanEntry,
}

/// The result of grouping a set of entries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Grouping {
    /// Groups in folder, then stem order.
    pub groups: Vec<Group>,
    /// Files listed but not grouped, in folder, then name order.
    pub unknown: Vec<Unknown>,
    /// Entries left out, in folder, then name order. Folders that are entered are not listed.
    pub excluded: Vec<Excluded>,
}

impl Grouping {
    /// Combines the results of grouping separate batches into the result for all their entries:
    /// equal to calling [`group`] once on every entry, provided each folder's entries were in
    /// one batch.
    #[must_use]
    pub fn merge(parts: impl IntoIterator<Item = Grouping>) -> Grouping {
        let mut out = Grouping::default();
        for part in parts {
            out.groups.extend(part.groups);
            out.unknown.extend(part.unknown);
            out.excluded.extend(part.excluded);
        }
        mark_folded_collisions(&mut out.groups);
        sort_output(&mut out);
        out
    }
}

/// A file waiting to be grouped, with whether it sits in its owner's `NKSC_PARAM`.
struct Pending {
    entry: ScanEntry,
    in_nksc: bool,
}

/// Groups `entries` (file-engine §3.3). Every file entry ends up in exactly one place: a group,
/// [`Grouping::unknown`] or [`Grouping::excluded`]. Folders end up in `excluded` when the walk
/// would not enter them, and are dropped otherwise.
///
/// The entries of a folder must all be in the same call, together with those of its
/// `NKSC_PARAM` (the adapter's batches are built that way). The result does not depend on the
/// order of `entries`.
#[must_use]
pub fn group(entries: impl IntoIterator<Item = ScanEntry>, policy: &ScanPolicy) -> Grouping {
    let mut out = Grouping::default();
    // Owner folder (exact components) -> its files.
    let mut folders: BTreeMap<Vec<String>, Vec<Pending>> = BTreeMap::new();
    for entry in entries {
        if entry.kind == EntryKind::Dir {
            if let DirAction::Skip(reason) = policy.dir_action(&entry) {
                out.excluded.push(Excluded { reason, entry });
            }
            continue;
        }
        if let Some(reason) = policy.file_exclusion(&entry) {
            out.excluded.push(Excluded { reason, entry });
            continue;
        }
        let (owner, in_nksc) = match entry.folder.split_last() {
            Some((last, parent)) if is_sidecar_dir(parent, last) => (parent.to_vec(), true),
            _ => (entry.folder.clone(), false),
        };
        folders
            .entry(owner)
            .or_default()
            .push(Pending { entry, in_nksc });
    }
    for (owner, files) in folders {
        if owner.iter().any(|c| eq_folded(c, NKSC_DIR)) {
            // Inside NX Studio's folder, below the level its sidecars live at: listed only.
            out.unknown.extend(files.into_iter().map(|p| Unknown {
                kind: UnknownKind::UnknownExt,
                entry: p.entry,
            }));
        } else {
            group_folder(&owner, files, &mut out);
        }
    }
    mark_folded_collisions(&mut out.groups);
    sort_output(&mut out);
    out
}

/// Groups the files of one folder (with those of its `NKSC_PARAM`).
fn group_folder(owner: &[String], files: Vec<Pending>, out: &mut Grouping) {
    let mut photos: BTreeMap<String, Vec<Photo>> = BTreeMap::new();
    let mut others = Vec::new();
    for p in files {
        let (stem, ext) = split_ext(&p.entry.name);
        match ext.and_then(PhotoKind::from_ext) {
            Some(kind) if !p.in_nksc => {
                photos.entry(fold(stem)).or_default().push(Photo {
                    kind,
                    entry: p.entry,
                });
            }
            _ => others.push(p),
        }
    }

    // Build the groups, and index every photo file by its folded full name.
    let mut groups: Vec<Group> = Vec::with_capacity(photos.len());
    let mut by_name: BTreeMap<String, (usize, Describes)> = BTreeMap::new();
    let mut by_stem: BTreeMap<String, usize> = BTreeMap::new();
    for (stem_key, mut set) in photos {
        set.sort_by_cached_key(|p| (p.kind, fold(&p.entry.name), p.entry.name.clone()));
        let jpegs = set.iter().filter(|p| p.kind == PhotoKind::Jpeg).count();
        let nefs = set.len() - jpegs;
        let status = if jpegs > 1 || nefs > 1 {
            GroupStatus::Ambiguous
        } else {
            GroupStatus::Ok
        };
        let mut iter = set.into_iter();
        let Some(primary) = iter.next() else {
            continue;
        };
        let mut raw = None;
        let mut duplicates = Vec::new();
        for p in iter {
            if primary.kind == PhotoKind::Jpeg && p.kind == PhotoKind::Nef && raw.is_none() {
                raw = Some(p);
            } else {
                duplicates.push(p);
            }
        }
        let idx = groups.len();
        let mut index = |p: &Photo, d: Describes| {
            // The first file in sort order wins a folded-name tie; the group is ambiguous then.
            by_name.entry(fold(&p.entry.name)).or_insert((idx, d));
        };
        index(&primary, Describes::Primary);
        if let Some(r) = &raw {
            index(r, Describes::Raw);
        }
        for (i, d) in duplicates.iter().enumerate() {
            index(d, Describes::Duplicate(i));
        }
        by_stem.insert(stem_key.clone(), idx);
        groups.push(Group {
            folder: owner.to_vec(),
            stem: split_ext(&primary.entry.name).0.to_owned(),
            stem_key,
            status,
            primary,
            raw,
            duplicates,
            sidecars: Vec::new(),
        });
    }

    for p in others {
        let (base, ext) = split_ext(&p.entry.name);
        let is = |x: &str| ext.is_some_and(|e| e.eq_ignore_ascii_case(x));
        let file_kind = if p.in_nksc {
            is("nksc").then_some(SidecarKind::Nksc)
        } else if is("xmp") {
            Some(SidecarKind::FileXmp)
        } else if is("pp3") {
            Some(SidecarKind::FilePp3)
        } else {
            None
        };
        let base_key = fold(base);
        let Some(file_kind) = file_kind else {
            // Not a sidecar name. In NKSC_PARAM: other content. Else matched by stem.
            let kind = match by_stem.get(&base_key) {
                Some(_) if !p.in_nksc => UnknownKind::SameBasename { stem_key: base_key },
                _ => UnknownKind::UnknownExt,
            };
            out.unknown.push(Unknown {
                kind,
                entry: p.entry,
            });
            continue;
        };
        // Rule 1: the base is the full name of a photo file.
        if let Some(&(g, describes)) = by_name.get(&base_key) {
            groups[g].sidecars.push(Sidecar {
                kind: file_kind,
                describes,
                entry: p.entry,
            });
            continue;
        }
        // Rule 2: a bare .xmp or .pp3 whose base is a group's stem.
        let bare = match file_kind {
            SidecarKind::FileXmp => Some(SidecarKind::Xmp),
            SidecarKind::FilePp3 => Some(SidecarKind::Pp3),
            _ => None,
        };
        match (bare, by_stem.get(&base_key)) {
            (Some(kind), Some(&g)) => groups[g].sidecars.push(Sidecar {
                kind,
                describes: Describes::Group,
                entry: p.entry,
            }),
            // Rule 3: an orphan.
            _ => out.unknown.push(Unknown {
                kind: UnknownKind::OrphanSidecar,
                entry: p.entry,
            }),
        }
    }
    out.groups.extend(groups);
}

/// Groups in two folders whose paths fold to the same value, with stems that fold to the same
/// value, would share a session identity (`folder_key`, `stem_key`): both become ambiguous.
fn mark_folded_collisions(groups: &mut [Group]) {
    let mut seen: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (i, g) in groups.iter().enumerate() {
        seen.entry((g.folder_key(), g.stem_key.clone()))
            .or_default()
            .push(i);
    }
    for idxs in seen.values().filter(|v| v.len() > 1) {
        for &i in idxs {
            groups[i].status = GroupStatus::Ambiguous;
        }
    }
}

type FolderKey = (Vec<String>, Vec<String>);

fn folder_key(folder: &[String]) -> FolderKey {
    (folder.iter().map(|c| fold(c)).collect(), folder.to_vec())
}

fn entry_key(e: &ScanEntry) -> (FolderKey, String, String) {
    (folder_key(&e.folder), fold(&e.name), e.name.clone())
}

fn sort_output(out: &mut Grouping) {
    for g in &mut out.groups {
        g.sidecars.sort_by_cached_key(|s| entry_key(&s.entry));
    }
    out.groups
        .sort_by_cached_key(|g| (folder_key(&g.folder), g.stem_key.clone(), g.stem.clone()));
    out.unknown.sort_by_cached_key(|u| entry_key(&u.entry));
    out.excluded
        .sort_by_cached_key(|x| (entry_key(&x.entry), x.reason));
}
