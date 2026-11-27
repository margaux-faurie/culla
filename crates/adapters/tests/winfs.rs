//! `winfs` read-only lookups: a folder's volume identity (ADR 0008, US-09) and the PC's UTC
//! offset (ADR 0028). On Windows (CI's `windows-latest` job) they query the volume of the
//! system temp folder, which needs no admin rights and no VHDX; elsewhere they check the
//! "unsupported" stub. The checks on the chosen folder itself run on every OS.

use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use culla_adapters::winfs::{VolumeError, local_utc_offset_minutes, resolve_folder};
use culla_core::volume::VolumePathError;

/// 2025-01-15 12:00 UTC and 2025-07-15 12:00 UTC, in nanoseconds.
const WINTER_NS: i64 = 1_736_942_400_000_000_000;
const SUMMER_NS: i64 = 1_752_580_800_000_000_000;

/// A uniquely named folder under the system temp folder, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(test: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir =
            std::env::temp_dir().join(format!("culla-winfs-{test}-{}-{nanos}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    #[allow(clippy::disallowed_methods, reason = "test temp folder only")]
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_missing_folder_is_an_io_error() {
    let tmp = TempDir::new("missing");
    let err = resolve_folder(&tmp.0.join("not-there")).unwrap_err();
    assert!(matches!(err, VolumeError::Io(_)), "{err:?}");
}

#[test]
fn a_file_is_not_a_folder() {
    let tmp = TempDir::new("file");
    let path = tmp.0.join("DSC_0001.JPG");
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    let err = resolve_folder(&path).unwrap_err();
    assert!(
        matches!(err, VolumeError::Path(VolumePathError::NotAFolder)),
        "{err:?}"
    );
}

/// A symlink to a folder is refused before canonicalization would follow it. (Junctions and
/// Windows symlinks, which need a privilege to create, are in the `windows-it` suite, M2.)
#[cfg(unix)]
#[test]
fn a_link_to_a_folder_is_refused() {
    let tmp = TempDir::new("link");
    let target = tmp.0.join("DCIM");
    fs::create_dir(&target).unwrap();
    let link = tmp.0.join("card");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let err = resolve_folder(&link).unwrap_err();
    assert!(
        matches!(err, VolumeError::Path(VolumePathError::ReparsePoint)),
        "{err:?}"
    );
}

#[cfg(not(windows))]
mod stub {
    use super::*;

    #[test]
    fn volumes_are_unsupported_off_windows() {
        let err = resolve_folder(&std::env::temp_dir()).unwrap_err();
        assert!(matches!(err, VolumeError::Unsupported), "{err:?}");
        assert_eq!(
            err.to_string(),
            "volume lookups are only supported on Windows"
        );
    }

    #[test]
    fn the_offset_is_utc_off_windows() {
        for ns in [0, -1, i64::MIN, i64::MAX, WINTER_NS, SUMMER_NS] {
            assert_eq!(local_utc_offset_minutes(ns), Some(0), "{ns}");
        }
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use culla_core::volume::DriveKind;

    #[test]
    fn the_temp_folder_resolves_to_a_sane_volume() {
        let tmp = TempDir::new("volume");
        let r = resolve_folder(&tmp.0).unwrap();
        let v = &r.volume;
        assert_ne!(v.id.serial, 0, "{v:?}");
        assert!(v.guid_path.as_str().starts_with("\\\\?\\Volume{"), "{v:?}");
        assert!(v.mount_point.ends_with('\\'), "{v:?}");
        assert!(!v.fs_name.is_empty(), "{v:?}");
        assert!(v.max_component_len >= 255, "{v:?}");
        assert!(!v.read_only(), "{v:?}");
        assert!(
            matches!(
                v.drive_kind,
                DriveKind::Fixed | DriveKind::Removable | DriveKind::RamDisk
            ),
            "{v:?}"
        );
        assert_eq!(r.source.volume(), v.id);
        // The folder is the last component of its path from the volume root.
        let name = tmp.0.file_name().unwrap().to_str().unwrap();
        assert_eq!(r.source.components().last().map(String::as_str), Some(name));
        assert!(r.canonical.is_absolute());
    }

    #[test]
    fn a_subfolder_is_on_the_same_volume_one_component_deeper() {
        let tmp = TempDir::new("nested");
        let sub = tmp.0.join("DCIM");
        fs::create_dir(&sub).unwrap();
        let parent = resolve_folder(&tmp.0).unwrap();
        let child = resolve_folder(&sub).unwrap();
        assert_eq!(child.volume.id, parent.volume.id);
        assert_eq!(child.volume.guid_path, parent.volume.guid_path);
        assert_eq!(
            child.source.components()[..child.source.components().len() - 1],
            *parent.source.components()
        );
        assert!(parent.source.key().contains(&child.source.key()));
        // A relative spelling and a trailing separator resolve to the same source.
        let again = resolve_folder(&sub.join(".")).unwrap();
        assert_eq!(again.source.key(), child.source.key());
    }

    #[test]
    fn the_volume_root_has_an_empty_path() {
        let tmp = TempDir::new("root");
        let r = resolve_folder(&tmp.0).unwrap();
        let root = resolve_folder(std::path::Path::new(&r.volume.mount_point)).unwrap();
        assert!(root.source.components().is_empty(), "{root:?}");
        assert_eq!(root.volume.id, r.volume.id);
    }

    #[test]
    fn the_offset_is_known_and_within_the_zones_that_exist() {
        let now = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        )
        .unwrap();
        for ns in [now, WINTER_NS, SUMMER_NS, 0, -1] {
            let m = local_utc_offset_minutes(ns).unwrap();
            assert!((-12 * 60..=14 * 60).contains(&m), "{ns}: {m}");
        }
        // Before 1601, FILETIME has no value: no answer, never a panic.
        assert_eq!(local_utc_offset_minutes(i64::MIN), None);
        let _ = local_utc_offset_minutes(i64::MAX);
    }
}
