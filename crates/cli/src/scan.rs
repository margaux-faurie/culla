//! `culla scan`: wiring only. Resolves the folder's volume (`winfs::resolve_folder`), streams
//! the scan (`scan::scan`, grouped per batch by `culla_core::grouping`), reads each group's
//! EXIF (`read_group_meta`) and orders the groups (`culla_core::exif::capture_order`).
//!
//! Read-only: it lists folders and reads the first bytes of each primary, through the
//! adapters. It writes nothing but stdout and stderr: no cache, no session, no log.

use std::io::{self, IsTerminal, Write};
use std::ops::ControlFlow;
use std::path::PathBuf;

use culla_adapters::metadata::exif::read_group_meta;
use culla_adapters::scan::{ScanError, ScanSummary, scan};
use culla_adapters::winfs::{ResolvedFolder, VolumeError, resolve_folder};
use culla_core::exif::{PhotoMeta, capture_order};
use culla_core::grouping::{
    self, EntryKind, ExclusionReason, Group, GroupStatus, Grouping, PhotoKind, ScanPolicy,
};

use crate::args::ScanArgs;
use crate::json::Value;
use crate::labels;
use crate::text::count;

/// The folder's identity, or why there is none.
#[derive(Debug)]
pub(crate) enum Identity {
    /// Looked up on Windows.
    Resolved(ResolvedFolder),
    /// Not looked up (off Windows): the text says why.
    Unavailable(String),
}

/// Everything `culla scan` prints.
#[derive(Debug)]
pub(crate) struct Report {
    /// The folder as given.
    pub folder: PathBuf,
    /// The path scanned: the canonical path when the volume was resolved, else as given.
    pub scanned: PathBuf,
    /// Subfolders included.
    pub include_subfolders: bool,
    /// Volume and source key.
    pub identity: Identity,
    /// Folder and entry counts of the walk.
    pub summary: ScanSummary,
    /// Groups, unknown files and exclusions.
    pub grouping: Grouping,
    /// `meta[i]`: the metadata of `grouping.groups[i]`'s primary, or why it could not be read.
    pub meta: Vec<Result<PhotoMeta, String>>,
    /// Indices of `grouping.groups` in capture-time order.
    pub order: Vec<usize>,
}

/// Why the scan did not run.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    /// The folder is refused: exit code 2.
    Refused(String),
    /// The folder cannot be read: exit code 3.
    Io(String),
}

/// Runs the scan. Progress goes to `progress`.
///
/// # Errors
///
/// [`Failure::Refused`] when the folder or its volume is refused, [`Failure::Io`] when it
/// cannot be read.
pub(crate) fn run(args: &ScanArgs, progress: &mut Progress) -> Result<Report, Failure> {
    let (scanned, identity) = match resolve_folder(&args.folder) {
        Ok(r) => (r.canonical.clone(), Identity::Resolved(r)),
        Err(VolumeError::Unsupported) => (
            args.folder.clone(),
            Identity::Unavailable(VolumeError::Unsupported.to_string()),
        ),
        Err(e @ VolumeError::Path(_)) => return Err(Failure::Refused(e.to_string())),
        Err(e @ (VolumeError::Io(_) | VolumeError::Win32 { .. })) => {
            return Err(Failure::Io(e.to_string()));
        }
    };

    let policy = ScanPolicy::new(args.include_subfolders);
    let mut parts = Vec::new();
    let mut entries = 0u64;
    let summary = scan(&scanned, &policy, |batch| {
        entries += batch.entries.len() as u64;
        progress.scanned(entries);
        parts.push(grouping::group(batch.entries, &policy));
        ControlFlow::Continue(())
    })
    .map_err(|e| match e {
        ScanError::Io(_) => Failure::Io(e.to_string()),
        ScanError::NotADirectory | ScanError::Refused(_) | ScanError::TooLarge { .. } => {
            Failure::Refused(e.to_string())
        }
    })?;
    progress.scan_done(&summary);
    let grouping = Grouping::merge(parts);

    let total = grouping.groups.len();
    let meta: Vec<Result<PhotoMeta, String>> = grouping
        .groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let m = read_group_meta(&scanned, g).map_err(|e| e.to_string());
            progress.read(i + 1, total);
            m
        })
        .collect();
    progress.read_done(total);
    let captured: Vec<_> = meta
        .iter()
        .map(|m| m.as_ref().ok().and_then(|m| m.captured))
        .collect();
    let order = capture_order(&grouping.groups, &captured);

    Ok(Report {
        folder: args.folder.clone(),
        scanned,
        include_subfolders: args.include_subfolders,
        identity,
        summary,
        grouping,
        meta,
        order,
    })
}

/// Counts shown under "totals".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Totals {
    /// Groups.
    pub groups: usize,
    /// JPEG + NEF.
    pub pairs: usize,
    /// NEF without a JPEG.
    pub lone_nefs: usize,
    /// JPEG without a NEF.
    pub jpeg_only: usize,
    /// Groups planned with no operations (file-engine §3.3).
    pub ambiguous: usize,
    /// Excluded entries held back because their folder could not be read completely.
    pub held_back: usize,
    /// Files listed but not grouped.
    pub unknown_files: usize,
    /// Entries left out, every reason.
    pub excluded: usize,
    /// Excluded entries per reason, in reason order, non-zero only.
    pub excluded_by_reason: Vec<(ExclusionReason, usize)>,
    /// Groups whose primary could not be read for its metadata.
    pub metadata_errors: usize,
}

impl Totals {
    /// The totals of `report`.
    #[must_use]
    pub(crate) fn of(report: &Report) -> Self {
        let g = &report.grouping;
        let mut t = Self {
            groups: g.groups.len(),
            unknown_files: g.unknown.len(),
            excluded: g.excluded.len(),
            metadata_errors: report.meta.iter().filter(|m| m.is_err()).count(),
            ..Self::default()
        };
        for group in &g.groups {
            match group_kind(group) {
                GroupKind::Pair => t.pairs += 1,
                GroupKind::LoneNef => t.lone_nefs += 1,
                GroupKind::JpegOnly => t.jpeg_only += 1,
            }
            if group.status == GroupStatus::Ambiguous {
                t.ambiguous += 1;
            }
        }
        let mut reasons: Vec<ExclusionReason> = g.excluded.iter().map(|x| x.reason).collect();
        reasons.sort_unstable();
        for r in reasons {
            match t.excluded_by_reason.last_mut() {
                Some((last, n)) if *last == r => *n += 1,
                _ => t.excluded_by_reason.push((r, 1)),
            }
        }
        t.held_back = g
            .excluded
            .iter()
            .filter(|x| x.reason == ExclusionReason::HeldBack)
            .count();
        t
    }
}

/// What a group holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GroupKind {
    /// A JPEG and its NEF.
    Pair,
    /// A NEF without a JPEG.
    LoneNef,
    /// A JPEG without a NEF.
    JpegOnly,
}

/// The kind of `group`.
#[must_use]
pub(crate) fn group_kind(group: &Group) -> GroupKind {
    match (group.primary.kind, &group.raw) {
        (PhotoKind::Nef, _) => GroupKind::LoneNef,
        (PhotoKind::Jpeg, Some(_)) => GroupKind::Pair,
        (PhotoKind::Jpeg, None) => GroupKind::JpegOnly,
    }
}

/// Every file of `group` with its role (`primary`, `raw`, `duplicate`, `sidecar`) and, for
/// a sidecar, the file it follows.
#[must_use]
pub(crate) fn group_files(group: &Group) -> Vec<(String, &'static str, Option<&'static str>)> {
    let mut out = vec![(group.primary.entry.rel_path(), "primary", None)];
    if let Some(raw) = &group.raw {
        out.push((raw.entry.rel_path(), "raw", None));
    }
    for d in &group.duplicates {
        out.push((d.entry.rel_path(), "duplicate", None));
    }
    for s in &group.sidecars {
        out.push((
            s.entry.rel_path(),
            "sidecar",
            Some(labels::describes(group.follows(s))),
        ));
    }
    out
}

/// The detail of an excluded entry that names a folder the scan could not list.
#[must_use]
pub(crate) fn exclusion_detail(kind: &EntryKind) -> Option<&str> {
    match kind {
        EntryKind::Unlisted { detail, .. } | EntryKind::Unreadable(detail) => Some(detail),
        _ => None,
    }
}

/// The report as one JSON object (`--json`).
#[must_use]
pub(crate) fn to_json(report: &Report) -> Value {
    let (volume, source, unavailable) = match &report.identity {
        Identity::Resolved(r) => {
            let v = &r.volume;
            let key = r.source.key();
            (
                Value::Obj(vec![
                    ("serial", Value::str(v.id.to_string())),
                    ("serial_number", Value::int(v.id.serial)),
                    ("label", Value::opt(v.label.clone(), Value::Str)),
                    ("drive_kind", Value::str(labels::drive_kind(v.drive_kind))),
                    ("mount_point", Value::str(v.mount_point.clone())),
                    ("file_system", Value::str(v.fs_name.clone())),
                    ("read_only", Value::Bool(v.read_only())),
                ]),
                Value::Obj(vec![
                    ("rel_path", Value::str(r.source.rel_path())),
                    (
                        "key",
                        Value::Obj(vec![
                            ("serial", Value::int(key.serial)),
                            ("rel_key", Value::str(key.rel_key)),
                        ]),
                    ),
                ]),
                Value::Null,
            )
        }
        Identity::Unavailable(why) => (Value::Null, Value::Null, Value::str(why.clone())),
    };
    let t = Totals::of(report);
    let totals = Value::Obj(vec![
        ("groups", Value::int(t.groups as u64)),
        ("pairs", Value::int(t.pairs as u64)),
        ("lone_nefs", Value::int(t.lone_nefs as u64)),
        ("jpeg_only", Value::int(t.jpeg_only as u64)),
        ("ambiguous", Value::int(t.ambiguous as u64)),
        ("held_back", Value::int(t.held_back as u64)),
        ("unknown_files", Value::int(t.unknown_files as u64)),
        ("excluded", Value::int(t.excluded as u64)),
        (
            "excluded_by_reason",
            Value::Arr(
                t.excluded_by_reason
                    .iter()
                    .map(|(r, n)| {
                        Value::Obj(vec![
                            ("reason", Value::str(labels::reason(*r))),
                            ("count", Value::int(*n as u64)),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("metadata_errors", Value::int(t.metadata_errors as u64)),
    ]);
    let g = &report.grouping;
    let groups = report
        .order
        .iter()
        .enumerate()
        .map(|(pos, &i)| group_json(pos + 1, &g.groups[i], &report.meta[i]))
        .collect();
    let unknown = g
        .unknown
        .iter()
        .map(|u| {
            Value::Obj(vec![
                ("path", Value::str(u.entry.rel_path())),
                ("kind", Value::str(u.kind.issue_kind())),
            ])
        })
        .collect();
    let excluded = g
        .excluded
        .iter()
        .map(|x| {
            Value::Obj(vec![
                ("path", Value::str(x.entry.rel_path())),
                ("folder", Value::Bool(is_folder(&x.entry.kind))),
                ("reason", Value::str(labels::reason(x.reason))),
                (
                    "detail",
                    Value::opt(exclusion_detail(&x.entry.kind), Value::str),
                ),
            ])
        })
        .collect();
    Value::Obj(vec![
        // The field set is provisional until M2 (SPEC §10): consumers must not rely on it.
        ("schema", Value::str("unstable")),
        ("culla_version", Value::str(culla_adapters::VERSION)),
        (
            "folder",
            Value::str(report.folder.to_string_lossy().into_owned()),
        ),
        (
            "scanned_path",
            Value::str(report.scanned.to_string_lossy().into_owned()),
        ),
        ("include_subfolders", Value::Bool(report.include_subfolders)),
        ("volume", volume),
        ("source", source),
        ("identity_unavailable", unavailable),
        (
            "scan",
            Value::Obj(vec![
                ("folders", Value::int(report.summary.folders)),
                ("entries", Value::int(report.summary.entries)),
            ]),
        ),
        ("totals", totals),
        ("groups", Value::Arr(groups)),
        ("unknown", Value::Arr(unknown)),
        ("excluded", Value::Arr(excluded)),
    ])
}

/// True for an entry that names a folder.
#[must_use]
pub(crate) fn is_folder(kind: &EntryKind) -> bool {
    matches!(kind, EntryKind::Dir | EntryKind::Unlisted { .. })
}

fn group_json(position: usize, group: &Group, meta: &Result<PhotoMeta, String>) -> Value {
    let ok = meta.as_ref().ok();
    let captured = ok.and_then(|m| m.captured).map(|t| {
        Value::Obj(vec![
            ("local", Value::str(t.to_iso8601())),
            ("local_ms", Value::int(t.local_ms)),
            ("offset_minutes", Value::opt(t.offset_minutes, Value::int)),
            ("source", Value::str(labels::time_source_id(t.source))),
        ])
    });
    let files = group_files(group)
        .into_iter()
        .map(|(path, role, follows)| {
            let mut f = vec![("path", Value::str(path)), ("role", Value::str(role))];
            if let Some(follows) = follows {
                f.push(("follows", Value::str(follows)));
            }
            Value::Obj(f)
        })
        .collect();
    Value::Obj(vec![
        ("position", Value::int(position as u64)),
        ("primary", Value::str(group.primary.entry.rel_path())),
        ("kind", Value::str(labels::group_kind(group_kind(group)))),
        (
            "status",
            Value::str(match group.status {
                GroupStatus::Ok => "ok",
                GroupStatus::Ambiguous => "ambiguous",
            }),
        ),
        ("captured", captured.unwrap_or(Value::Null)),
        (
            "orientation",
            Value::opt(ok, |m| Value::int(m.orientation.exif_value())),
        ),
        (
            "lens",
            Value::opt(ok.and_then(|m| m.lens.clone()), Value::Str),
        ),
        ("files", Value::Arr(files)),
        (
            "issues",
            Value::Arr(
                ok.map(|m| {
                    m.issues
                        .iter()
                        .map(|i| Value::str(labels::issue(*i)))
                        .collect()
                })
                .unwrap_or_default(),
            ),
        ),
        (
            "error",
            Value::opt(meta.as_ref().err().cloned(), Value::Str),
        ),
    ])
}

/// Progress on stderr: rewritten in place on a terminal, one line per step otherwise (every
/// batch while scanning, every 100 groups while reading metadata). While scanning it counts
/// entries only: batches do not map one to one to the folders of the summary (`NKSC_PARAM`
/// folders share their parent's batch), so the folder count is given once the scan is done.
pub(crate) struct Progress {
    tty: bool,
    err: io::Stderr,
}

impl Progress {
    /// Progress to this process's stderr.
    #[must_use]
    pub(crate) fn stderr() -> Self {
        let err = io::stderr();
        Self {
            tty: err.is_terminal(),
            err,
        }
    }

    fn line(&mut self, text: &str, last: bool) {
        // Progress is best effort: a closed stderr must not stop the scan.
        let _ = if self.tty {
            write!(self.err, "\r\x1b[2K{text}{}", if last { "\n" } else { "" })
        } else {
            writeln!(self.err, "{text}")
        };
    }

    fn scanned(&mut self, entries: u64) {
        self.line(&format!("scanning: {}", count(entries, "entry")), false);
    }

    fn scan_done(&mut self, s: &ScanSummary) {
        self.line(
            &format!(
                "scanned: {}, {}",
                count(s.folders, "folder"),
                count(s.entries, "entry")
            ),
            true,
        );
    }

    fn read(&mut self, done: usize, total: usize) {
        if self.tty || done.is_multiple_of(100) {
            self.line(&format!("reading metadata: {done} of {total}"), false);
        }
    }

    fn read_done(&mut self, total: usize) {
        self.line(
            &format!("read metadata: {}", count(total as u64, "group")),
            true,
        );
    }
}
