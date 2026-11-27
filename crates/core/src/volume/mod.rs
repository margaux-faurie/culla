//! Volume identity and source keys (ADR 0008; design/file-engine.md §2.1, §3.1, §6.3; story
//! US-09).
//!
//! A source is identified by the volume's 32-bit serial and the folder's path from the volume
//! root, never by its drive letter, so a card that comes back as `F:` instead of `E:` finds its
//! session again. Everything here is pure: the adapter (`culla_adapters::winfs`) asks Windows
//! for a folder's volume and hands the answers over as plain data ([`VolumeInfo`], the path
//! strings); the rules that turn them into keys and bindings live here, tested on any OS.
//!
//! - [`VolumeId`] is the identity: the serial only.
//! - [`VolumeInfo`] is what one lookup saw: the serial, the volume GUID path the run binds to
//!   (§6.3), the mount point, label, file system, flags and drive type.
//! - [`VolumeRecord`] is the stored `volume` row (§2.3): the serial plus the last label, mount
//!   point and GUID seen, which are informational and refreshed by [`VolumeRecord::observe`].
//! - [`Source`] and [`SourceKey`]: (serial, path from the volume root) and its folded form,
//!   which keys the session database, the journal and the cache.
//! - [`bind`] and [`relation`]: the run binding of §6.3 (one mounted volume per serial, "same
//!   volume" decided by GUID).
//!
//! Paths are Windows paths as text: [`classify_path`] says which forms are supported,
//! [`validate_drive_path`] checks every name in one, and [`rel_under_root`] splits a folder's
//! path under its volume root into validated components (design/file-engine.md §3.4).
//!
//! What v1 refuses (decided by Margaux, 27 Nov 2026): network shares and mapped network
//! drives, volumes with no drive letter or mounted folder, and volumes with serial 0
//! ([`classify_path`], [`check_volume`]); a chosen folder that is a link or a cloud placeholder
//! ([`check_chosen_folder`]).
//!
//! A [`SourceKey`] built from a path lookup is a session lookup key only, never a run binding:
//! M2 re-reads the serial from the opened folder's handle and binds by GUID before any
//! operation (design/file-engine.md §3.1, §6.3).

#[cfg(test)]
mod tests;

use std::fmt;

use crate::grouping::Attributes;
use crate::names::{NameError, fold, validate_component};

/// The identity of a volume: its 32-bit serial number (ADR 0008). Two volumes with the same
/// serial are the same volume for sessions and caches; [`bind`] and [`relation`] catch the
/// case where two mounted volumes share one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VolumeId {
    /// The serial number `GetVolumeInformationW` reports (`vol` shows it as `XXXX-XXXX`).
    pub serial: u32,
}

impl VolumeId {
    /// The identity of the volume with `serial`.
    #[must_use]
    pub const fn new(serial: u32) -> Self {
        Self { serial }
    }
}

impl fmt::Display for VolumeId {
    /// As `vol` and Explorer show it: `1A2B-3C4D`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04X}-{:04X}", self.serial >> 16, self.serial & 0xFFFF)
    }
}

/// What `GetDriveTypeW` says about a volume's root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DriveKind {
    /// `DRIVE_UNKNOWN` (0), or a value Windows does not document.
    Unknown,
    /// `DRIVE_NO_ROOT_DIR` (1): the root is not valid.
    NoRootDir,
    /// `DRIVE_REMOVABLE` (2): SD cards in a reader, USB sticks.
    Removable,
    /// `DRIVE_FIXED` (3): internal disks, and many USB card readers and external drives.
    Fixed,
    /// `DRIVE_REMOTE` (4): a network drive.
    Remote,
    /// `DRIVE_CDROM` (5).
    CdRom,
    /// `DRIVE_RAMDISK` (6).
    RamDisk,
}

impl DriveKind {
    /// Maps `GetDriveTypeW`'s return value.
    #[must_use]
    pub const fn from_win32(value: u32) -> Self {
        match value {
            1 => Self::NoRootDir,
            2 => Self::Removable,
            3 => Self::Fixed,
            4 => Self::Remote,
            5 => Self::CdRom,
            6 => Self::RamDisk,
            _ => Self::Unknown,
        }
    }
}

/// The file system flags `GetVolumeInformationW` reports, kept as raw bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct VolumeFlags(pub u32);

impl VolumeFlags {
    /// `FILE_READ_ONLY_VOLUME`: the volume is read-only, which covers an SD card's lock switch
    /// (preflight P3, design/file-engine.md §8).
    pub const READ_ONLY: u32 = 0x0008_0000;

    /// True when the volume is read-only.
    #[must_use]
    pub const fn read_only(self) -> bool {
        self.0 & Self::READ_ONLY != 0
    }
}

/// A volume GUID path, `\\?\Volume{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}\`, with the hex
/// digits in lower case. A run builds every path from the GUID path its serial was bound to
/// (design/file-engine.md §6.3), so a drive letter that changes mid-run cannot redirect it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VolumeGuidPath(String);

impl VolumeGuidPath {
    const PREFIX: &'static str = "\\\\?\\Volume{";
    const SUFFIX: &'static str = "}\\";
    /// Group lengths of a GUID's hex digits.
    const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];

    /// Checks the exact form `GetVolumeNameForVolumeMountPointW` returns. The hex digits are
    /// folded to lower case, so two spellings of one GUID compare equal.
    ///
    /// # Errors
    ///
    /// [`VolumePathError::BadGuidPath`] for anything else.
    pub fn parse(s: &str) -> Result<Self, VolumePathError> {
        let guid = s
            .strip_prefix(Self::PREFIX)
            .and_then(|rest| rest.strip_suffix(Self::SUFFIX))
            .ok_or(VolumePathError::BadGuidPath)?;
        let mut groups = guid.split('-');
        for len in Self::GROUPS {
            let group = groups.next().ok_or(VolumePathError::BadGuidPath)?;
            if group.len() != len || !group.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(VolumePathError::BadGuidPath);
            }
        }
        if groups.next().is_some() {
            return Err(VolumePathError::BadGuidPath);
        }
        Ok(Self(format!(
            "{}{}{}",
            Self::PREFIX,
            guid.to_ascii_lowercase(),
            Self::SUFFIX
        )))
    }

    /// The path, with its trailing backslash.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VolumeGuidPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What one lookup of a folder's volume saw (`culla_adapters::winfs::resolve_folder`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VolumeInfo {
    /// The identity.
    pub id: VolumeId,
    /// The volume GUID path, for binding a run (§6.3) and for "same volume" checks.
    pub guid_path: VolumeGuidPath,
    /// The volume's mount point as the lookup reached it, with a trailing backslash: `E:\`, or
    /// a mounted folder such as `C:\Mounts\Card\`. Display only: it changes between mounts.
    pub mount_point: String,
    /// The volume label, `None` when it has none.
    pub label: Option<String>,
    /// The file system name: `NTFS`, `exFAT`, `FAT32`…
    pub fs_name: String,
    /// The file system flags.
    pub flags: VolumeFlags,
    /// The longest name component the file system allows, in UTF-16 units (255 on NTFS,
    /// exFAT and FAT32 with long names).
    pub max_component_len: u32,
    /// The drive type of the mount point.
    pub drive_kind: DriveKind,
}

impl VolumeInfo {
    /// True when the volume is read-only (`FILE_READ_ONLY_VOLUME`, the SD lock switch).
    #[must_use]
    pub const fn read_only(&self) -> bool {
        self.flags.read_only()
    }
}

/// The stored `volume` row (design/file-engine.md §2.3): the identity plus what was last seen
/// of the volume. Only [`VolumeRecord::id`] identifies it; the rest is shown to the user
/// ("Insert the card <label>") and refreshed on every lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VolumeRecord {
    /// The identity (the `serial` column).
    pub id: VolumeId,
    /// The file system name last seen.
    pub fs_name: String,
    /// The label last seen.
    pub label: Option<String>,
    /// The mount point last seen (`last_mount`), display only.
    pub last_mount: Option<String>,
    /// The GUID path last seen (`last_guid`), informational: a run binds afresh (§6.3).
    pub last_guid: Option<VolumeGuidPath>,
}

/// What [`VolumeRecord::observe`] changed. A changed mount point is the US-09 case: the same
/// card under another letter keeps its identity, its session and its cache.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecordChange {
    /// The mount point differs (another drive letter or mounted folder).
    pub mount: bool,
    /// The GUID path differs (another PC, or Windows forgot the volume).
    pub guid: bool,
    /// The label differs (renamed in Explorer or in the camera).
    pub label: bool,
    /// The file system name differs (a reformatted card that kept its serial).
    pub fs_name: bool,
}

impl RecordChange {
    /// True when nothing changed.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        !(self.mount || self.guid || self.label || self.fs_name)
    }
}

/// [`VolumeRecord::observe`] was given another volume (another serial).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotSameVolume;

impl fmt::Display for NotSameVolume {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the volume has another serial number")
    }
}

impl std::error::Error for NotSameVolume {}

impl VolumeRecord {
    /// The record of a volume seen for the first time.
    #[must_use]
    pub fn from_info(info: &VolumeInfo) -> Self {
        Self {
            id: info.id,
            fs_name: info.fs_name.clone(),
            label: info.label.clone(),
            last_mount: Some(info.mount_point.clone()),
            last_guid: Some(info.guid_path.clone()),
        }
    }

    /// Refreshes the informational fields from a new lookup of the same volume and says what
    /// changed. The identity never changes.
    ///
    /// # Errors
    ///
    /// [`NotSameVolume`] when `info` has another serial; the record is left as it was.
    pub fn observe(&mut self, info: &VolumeInfo) -> Result<RecordChange, NotSameVolume> {
        if info.id != self.id {
            return Err(NotSameVolume);
        }
        let change = RecordChange {
            mount: self.last_mount.as_deref() != Some(info.mount_point.as_str()),
            guid: self.last_guid.as_ref() != Some(&info.guid_path),
            label: self.label != info.label,
            fs_name: self.fs_name != info.fs_name,
        };
        *self = Self::from_info(info);
        Ok(change)
    }
}

/// A source folder: the volume and the folder's path from the volume root, as written
/// (design/file-engine.md §2.1). Built only from valid components (§3.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    volume: VolumeId,
    rel: Vec<String>,
}

impl Source {
    /// The source at `rel` (components from the volume root; empty for the root itself) on
    /// `volume`.
    ///
    /// # Errors
    ///
    /// The first component that is not a valid name (design/file-engine.md §3.4).
    pub fn new(volume: VolumeId, rel: Vec<String>) -> Result<Self, NameError> {
        for c in &rel {
            validate_component(c)?;
        }
        Ok(Self { volume, rel })
    }

    /// The volume.
    #[must_use]
    pub const fn volume(&self) -> VolumeId {
        self.volume
    }

    /// The components from the volume root, as written.
    #[must_use]
    pub fn components(&self) -> &[String] {
        &self.rel
    }

    /// The path from the volume root, components joined with `\` (`''` for the root): the
    /// `source.rel_path` column.
    #[must_use]
    pub fn rel_path(&self) -> String {
        self.rel.join("\\")
    }

    /// The key that finds this source's session, journal and cache: the serial and the folded
    /// path. It does not depend on the drive letter, the mount point or the GUID (US-09).
    #[must_use]
    pub fn key(&self) -> SourceKey {
        SourceKey {
            serial: self.volume.serial,
            rel_key: self
                .rel
                .iter()
                .map(|c| fold(c))
                .collect::<Vec<_>>()
                .join("\\"),
        }
    }
}

/// The key of a source (ADR 0008): the volume serial and the path from the volume root,
/// folded component by component and joined with `\` (the `source.rel_key` column). Two
/// spellings of one folder (`DCIM\100NIKON`, `dcim\100nikon`) get one key, as on the file
/// system, which compares names case-insensitively.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceKey {
    /// The volume serial.
    pub serial: u32,
    /// The folded path from the volume root.
    pub rel_key: String,
}

impl SourceKey {
    /// True when `other` is this source or inside it (on the same volume): the nested-source
    /// check of design/file-engine.md §6.1, step 2, compared on folded paths.
    #[must_use]
    pub fn contains(&self, other: &Self) -> bool {
        self.serial == other.serial
            && (self.rel_key.is_empty()
                || other.rel_key == self.rel_key
                || other
                    .rel_key
                    .strip_prefix(self.rel_key.as_str())
                    .is_some_and(|rest| rest.starts_with('\\')))
    }

    /// True when one of the two sources contains the other.
    #[must_use]
    pub fn nested_with(&self, other: &Self) -> bool {
        self.contains(other) || other.contains(self)
    }
}

/// How a serial binds to the mounted volumes at the start of a run (design/file-engine.md
/// §6.3, step 2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Binding<'a> {
    /// Exactly one mounted volume has the serial: the run uses its GUID path.
    Bound(&'a VolumeInfo),
    /// None has it: the plan pauses (`volume_lost`), "Insert the card <label>".
    Missing,
    /// More than one has it: the plan pauses (`volume_ambiguous`) and the user picks one.
    /// Listed once per GUID path, in the order given.
    Ambiguous(Vec<&'a VolumeInfo>),
}

/// Binds `id` to the mounted volumes in `mounted` (one entry per volume, as enumerated). The
/// same GUID path listed twice (one volume seen through two mount points) counts once.
#[must_use]
pub fn bind(id: VolumeId, mounted: &[VolumeInfo]) -> Binding<'_> {
    let mut matches: Vec<&VolumeInfo> = Vec::new();
    for v in mounted.iter().filter(|v| v.id == id) {
        if !matches.iter().any(|m| m.guid_path == v.guid_path) {
            matches.push(v);
        }
    }
    match matches.len() {
        0 => Binding::Missing,
        1 => Binding::Bound(matches[0]),
        _ => Binding::Ambiguous(matches),
    }
}

/// How two looked-up volumes relate (design/file-engine.md §6.3, step 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeRelation {
    /// The same GUID path: one volume (a rename, not a copy, can move between them).
    Same,
    /// Different GUID paths and serials.
    Different,
    /// Different GUID paths but the same serial: two volumes that identity by serial cannot
    /// tell apart. A destination in this relation to its source is refused at plan time.
    SerialClash,
}

/// Decides "same volume" by GUID path, never by serial or drive letter.
#[must_use]
pub fn relation(a: &VolumeInfo, b: &VolumeInfo) -> VolumeRelation {
    if a.guid_path == b.guid_path {
        VolumeRelation::Same
    } else if a.id == b.id {
        VolumeRelation::SerialClash
    } else {
        VolumeRelation::Different
    }
}

/// A path form Culla does not take as a source or destination in v1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedPath {
    /// Not an absolute drive path: relative (`DCIM`), drive-relative (`E:DCIM`) or rooted on
    /// the current drive (`\DCIM`).
    NotAbsolute,
    /// A network path (`\\server\share`, `\\?\UNC\…`) or a volume Windows reports as a
    /// network drive (`DRIVE_REMOTE`) or without a valid root (`DRIVE_NO_ROOT_DIR`). Refused in
    /// v1 (decided by Margaux, 27 Nov 2026; design/file-engine.md §6.2).
    Network,
    /// A volume GUID path without a drive letter or mounted folder (`\\?\Volume{…}\…`).
    VolumeGuid,
    /// A device or object-manager path (`\\.\…`, `\??\…`, `\\?\GLOBALROOT\…`).
    Device,
}

impl fmt::Display for UnsupportedPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotAbsolute => "not an absolute path with a drive letter",
            Self::Network => "network paths are not supported",
            Self::VolumeGuid => "a volume without a drive letter or mounted folder",
            Self::Device => "device paths are not supported",
        })
    }
}

/// Why a path could not be turned into a volume and a relative path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumePathError {
    /// A path form that is not supported.
    Unsupported(UnsupportedPath),
    /// The volume root is not a drive path ending with `\`.
    BadRoot,
    /// The path is not under the volume root.
    NotUnderRoot,
    /// A component of the path from the root is not a valid name (design/file-engine.md §3.4).
    Name(NameError),
    /// Not a volume GUID path of the form `\\?\Volume{…}\`.
    BadGuidPath,
    /// The volume reports serial 0, which cannot identify it (refused in v1, decided by Margaux,
    /// 27 Nov 2026).
    NoSerial,
    /// The chosen folder is itself a symlink, junction, mount point or other reparse point.
    ReparsePoint,
    /// The chosen folder is a cloud placeholder (`OFFLINE`, `RECALL_ON_OPEN`,
    /// `RECALL_ON_DATA_ACCESS`; threat model T2b).
    CloudPlaceholder,
    /// The chosen path is not a folder.
    NotAFolder,
}

impl fmt::Display for VolumePathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(u) => u.fmt(f),
            Self::BadRoot => f.write_str("the volume root is not a drive path"),
            Self::NotUnderRoot => f.write_str("the folder is not under its volume root"),
            Self::Name(e) => write!(f, "invalid name in the path: {e}"),
            Self::BadGuidPath => f.write_str("not a volume GUID path"),
            Self::NoSerial => f.write_str("the volume has no serial number"),
            Self::ReparsePoint => f.write_str("the folder is a link, junction or mount point"),
            Self::CloudPlaceholder => f.write_str("the folder is a cloud placeholder"),
            Self::NotAFolder => f.write_str("not a folder"),
        }
    }
}

impl std::error::Error for VolumePathError {}

impl From<NameError> for VolumePathError {
    fn from(e: NameError) -> Self {
        Self::Name(e)
    }
}

impl From<UnsupportedPath> for VolumePathError {
    fn from(e: UnsupportedPath) -> Self {
        Self::Unsupported(e)
    }
}

/// The drive form of a supported absolute path, or why it is not supported.
///
/// Supported: `X:\…` (any ASCII drive letter) and its verbatim form `\\?\X:\…`, which is what
/// `std::fs::canonicalize` returns; the `\\?\` is removed. A folder on a volume mounted in a
/// folder (`C:\Mounts\Card\DCIM`) is a drive path too. Everything else is an
/// [`UnsupportedPath`].
///
/// # Errors
///
/// The [`UnsupportedPath`] the path falls under.
pub fn classify_path(path: &str) -> Result<&str, UnsupportedPath> {
    let drive = if let Some(rest) = path.strip_prefix("\\\\?\\") {
        if starts_with_ignore_case(rest, "UNC\\") {
            return Err(UnsupportedPath::Network);
        }
        if starts_with_ignore_case(rest, "Volume{") {
            return Err(UnsupportedPath::VolumeGuid);
        }
        if !is_drive_path(rest) {
            return Err(UnsupportedPath::Device);
        }
        rest
    } else if path.starts_with("\\\\.\\") || path.starts_with("\\??\\") {
        return Err(UnsupportedPath::Device);
    } else if path.starts_with("\\\\") || path.starts_with("//") {
        return Err(UnsupportedPath::Network);
    } else {
        path
    };
    if is_drive_path(drive) {
        Ok(drive)
    } else {
        Err(UnsupportedPath::NotAbsolute)
    }
}

/// `X:\` followed by anything, with an ASCII drive letter.
fn is_drive_path(s: &str) -> bool {
    matches!(s.as_bytes(), [letter, b':', b'\\', ..] if letter.is_ascii_alphabetic())
}

fn starts_with_ignore_case(s: &str, prefix: &str) -> bool {
    s.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// Splits `path` under its volume root `root` into validated components (design/file-engine.md
/// §3.4); empty when `path` is the root. Both are drive paths as [`classify_path`] returns
/// them; `root` ends with `\` (`E:\`, or a mounted folder such as `C:\Mounts\Card\`). The root
/// is matched component by component under [`fold`], as the file system compares names.
///
/// # Errors
///
/// [`VolumePathError::BadRoot`] when `root` is not a drive path ending with `\`,
/// [`VolumePathError::NotUnderRoot`] when `path` is not under it, and
/// [`VolumePathError::Name`] for an empty or invalid component below it (`E:\DCIM\\x`,
/// `E:\DCIM\..`, a reserved character).
pub fn rel_under_root(path: &str, root: &str) -> Result<Vec<String>, VolumePathError> {
    let root_inner = root
        .strip_suffix('\\')
        .filter(|_| is_drive_path(root))
        .ok_or(VolumePathError::BadRoot)?;
    let path = classify_path(path)?;
    // `E:\` has the single component `E:`; a trailing `\` on the path adds no component.
    let path = path.strip_suffix('\\').unwrap_or(path);
    let mut root_parts = root_inner.split('\\');
    let mut path_parts = path.split('\\');
    for r in root_parts.by_ref() {
        match path_parts.next() {
            Some(p) if fold(p) == fold(r) => {}
            _ => return Err(VolumePathError::NotUnderRoot),
        }
    }
    let rel: Vec<String> = path_parts.map(str::to_owned).collect();
    for c in &rel {
        validate_component(c)?;
    }
    Ok(rel)
}

/// Checks the chosen folder before anything else is asked about it, from its own entry read
/// without following links (`symlink_metadata`): it must be a real folder, not a reparse point
/// and not a cloud placeholder (design/file-engine.md §3.1, threat model T2, T2b). A link
/// *above* the folder is resolved by canonicalization afterwards.
///
/// # Errors
///
/// [`VolumePathError::ReparsePoint`], [`VolumePathError::CloudPlaceholder`] or
/// [`VolumePathError::NotAFolder`], checked in that order.
pub fn check_chosen_folder(attrs: Attributes, is_dir: bool) -> Result<(), VolumePathError> {
    if attrs.reparse_point {
        Err(VolumePathError::ReparsePoint)
    } else if attrs.cloud_placeholder {
        Err(VolumePathError::CloudPlaceholder)
    } else if !is_dir {
        Err(VolumePathError::NotAFolder)
    } else {
        Ok(())
    }
}

/// Validates every component of a drive path below its drive letter (`E:\DCIM\100NIKON`:
/// `DCIM` and `100NIKON`) with [`validate_component`], so no name Culla would refuse reaches a
/// Win32 call (design/file-engine.md §3.4). A trailing `\` is allowed; `E:\` has none.
///
/// # Errors
///
/// [`VolumePathError::Unsupported`] when `path` is not a supported form ([`classify_path`]),
/// [`VolumePathError::Name`] for the first invalid or empty component.
pub fn validate_drive_path(path: &str) -> Result<(), VolumePathError> {
    let drive = classify_path(path)?;
    let below = &drive[3..];
    let below = below.strip_suffix('\\').unwrap_or(below);
    if below.is_empty() {
        return Ok(());
    }
    for c in below.split('\\') {
        validate_component(c)?;
    }
    Ok(())
}

/// Refuses the volumes v1 does not take (decided by Margaux, 27 Nov 2026): network drives
/// (`DRIVE_REMOTE`) and roots Windows calls invalid (`DRIVE_NO_ROOT_DIR`), as
/// [`UnsupportedPath::Network`], and serial 0, as [`VolumePathError::NoSerial`]. Volumes
/// reachable only by GUID path are refused earlier, by [`classify_path`].
///
/// # Errors
///
/// As above.
pub fn check_volume(info: &VolumeInfo) -> Result<(), VolumePathError> {
    match info.drive_kind {
        DriveKind::Remote | DriveKind::NoRootDir => {
            Err(VolumePathError::Unsupported(UnsupportedPath::Network))
        }
        _ if info.id.serial == 0 => Err(VolumePathError::NoSerial),
        _ => Ok(()),
    }
}
