//! Names the CLI prints for core values: snake_case ids for JSON, words for text.

use culla_core::exif::{MetaIssue, Orientation, TimeSource, TimeTag};
use culla_core::grouping::{Describes, ExclusionReason};
use culla_core::volume::DriveKind;

use crate::scan::GroupKind;

/// The JSON id of an exclusion reason.
#[must_use]
pub(crate) fn reason(r: ExclusionReason) -> &'static str {
    match r {
        ExclusionReason::RejectedFolder => "rejected_folder",
        ExclusionReason::Destination => "destination",
        ExclusionReason::NotIncluded => "not_included",
        ExclusionReason::TooDeep => "too_deep",
        ExclusionReason::ReparsePoint => "reparse_point",
        ExclusionReason::CloudPlaceholder => "cloud_placeholder",
        ExclusionReason::HiddenOrSystem => "hidden_or_system",
        ExclusionReason::InvalidName => "invalid_name",
        ExclusionReason::CullaPartial => "culla_partial",
        ExclusionReason::NotAFile => "not_a_file",
        ExclusionReason::Unreadable => "unreadable",
        ExclusionReason::TooLarge => "too_large",
        ExclusionReason::HeldBack => "held_back",
    }
}

/// The JSON id of a drive kind (`GetDriveTypeW`).
#[must_use]
pub(crate) fn drive_kind(k: DriveKind) -> &'static str {
    match k {
        DriveKind::Unknown => "unknown",
        DriveKind::NoRootDir => "no_root_dir",
        DriveKind::Removable => "removable",
        DriveKind::Fixed => "fixed",
        DriveKind::Remote => "remote",
        DriveKind::CdRom => "cdrom",
        DriveKind::RamDisk => "ramdisk",
    }
}

/// The JSON id of a capture-time source.
#[must_use]
pub(crate) fn time_source_id(s: TimeSource) -> &'static str {
    match s {
        TimeSource::DateTimeOriginal => "date_time_original",
        TimeSource::CreateDate => "create_date",
        TimeSource::FileModified => "file_modified",
    }
}

/// A capture-time source in text.
#[must_use]
pub(crate) fn time_source_text(s: TimeSource) -> &'static str {
    match s {
        TimeSource::DateTimeOriginal => "DateTimeOriginal",
        TimeSource::CreateDate => "CreateDate",
        TimeSource::FileModified => "file time",
    }
}

/// An orientation in text.
#[must_use]
pub(crate) fn orientation_text(o: Orientation) -> &'static str {
    match o {
        Orientation::Normal => "upright",
        Orientation::MirrorHorizontal => "mirrored left to right",
        Orientation::Rotate180 => "rotate 180°",
        Orientation::MirrorVertical => "mirrored top to bottom",
        Orientation::Transpose => "transposed",
        Orientation::Rotate90 => "rotate 90° clockwise",
        Orientation::Transverse => "transversed",
        Orientation::Rotate270 => "rotate 90° counter-clockwise",
    }
}

/// The JSON id of a group kind.
#[must_use]
pub(crate) fn group_kind(k: GroupKind) -> &'static str {
    match k {
        GroupKind::Pair => "pair",
        GroupKind::LoneNef => "lone_nef",
        GroupKind::JpegOnly => "jpeg_only",
    }
}

/// The file a sidecar follows, as resolved by `Group::follows`.
#[must_use]
pub(crate) fn describes(d: Describes) -> &'static str {
    match d {
        Describes::Primary | Describes::Group => "primary",
        Describes::Raw => "raw",
        Describes::Duplicate(_) => "duplicate",
    }
}

/// A metadata issue as `id` or `id:detail`.
#[must_use]
pub(crate) fn issue(i: MetaIssue) -> String {
    let tag = |t: TimeTag| match t {
        TimeTag::Original => "original",
        TimeTag::Digitized => "digitized",
    };
    match i {
        MetaIssue::NoExif => "no_exif".to_owned(),
        MetaIssue::UnreadableExif => "unreadable_exif".to_owned(),
        MetaIssue::SkippedFields(n) => format!("skipped_fields:{n}"),
        MetaIssue::BadDateTime(t) => format!("bad_date_time:{}", tag(t)),
        MetaIssue::BadSubSec(t) => format!("bad_sub_sec:{}", tag(t)),
        MetaIssue::BadOffset(t) => format!("bad_offset:{}", tag(t)),
        MetaIssue::BadOrientation(v) => format!("bad_orientation:{v}"),
    }
}
