//! The Win32 calls behind [`super::resolve_folder`] and [`super::local_utc_offset_minutes`].
//! Read-only lookups: no call here opens, creates or changes a file.
//!
//! Every buffer handed to Windows is owned here, NUL-terminated when it is an input, and
//! passed with its exact length in UTF-16 units; outputs are read up to their first NUL and
//! never past the buffer. A failed call becomes an error, never a panic.
//!
//! windows-sys 0.61 signatures used (features `Win32_Foundation`, `Win32_Storage_FileSystem`,
//! `Win32_System_Time`); `BOOL` results are compared with 0 so the code does not name the
//! `BOOL` type:
//!
//! - `GetVolumePathNameW(PCWSTR, PWSTR, u32) -> BOOL`
//! - `GetVolumeNameForVolumeMountPointW(PCWSTR, PWSTR, u32) -> BOOL`
//! - `GetVolumeInformationW(PCWSTR, PWSTR, u32, *mut u32, *mut u32, *mut u32, PWSTR, u32)
//!   -> BOOL`
//! - `GetDriveTypeW(PCWSTR) -> u32`
//! - `GetDynamicTimeZoneInformation(*mut DYNAMIC_TIME_ZONE_INFORMATION) -> u32`
//! - `FileTimeToSystemTime(*const FILETIME, *mut SYSTEMTIME) -> BOOL`
//! - `SystemTimeToTzSpecificLocalTimeEx(*const DYNAMIC_TIME_ZONE_INFORMATION,
//!   *const SYSTEMTIME, *mut SYSTEMTIME) -> BOOL`
//! - `SystemTimeToFileTime(*const SYSTEMTIME, *mut FILETIME) -> BOOL`
#![allow(
    unsafe_code,
    reason = "Win32 FFI, confined to winfs; every unsafe block states why it is sound"
)]

use std::ffi::OsStr;
use std::io;
use std::iter;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use culla_core::names::NameError;
use culla_core::volume::{
    DriveKind, Source, VolumeFlags, VolumeGuidPath, VolumeId, VolumeInfo, classify_path,
    rel_under_root,
};
use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows_sys::Win32::Storage::FileSystem::{
    GetDriveTypeW, GetVolumeInformationW, GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
};
use windows_sys::Win32::System::Time::{
    DYNAMIC_TIME_ZONE_INFORMATION, FileTimeToSystemTime, GetDynamicTimeZoneInformation,
    SystemTimeToFileTime, SystemTimeToTzSpecificLocalTimeEx,
};

use super::{ResolvedFolder, VolumeError};

/// `MAX_PATH`: the classic path limit, in UTF-16 units including the NUL.
const MAX_PATH: usize = 260;
/// Label and file system name buffers: `GetVolumeInformationW` documents `MAX_PATH + 1` as the
/// largest size it needs.
const NAME_BUF: usize = MAX_PATH + 1;
/// A volume GUID path is 49 UTF-16 units plus the NUL; the docs suggest 50. Twice that leaves
/// room for anything unexpected, which [`VolumeGuidPath::parse`] then refuses.
const GUID_BUF: usize = 100;
/// `GetDynamicTimeZoneInformation`'s failure value (`TIME_ZONE_ID_INVALID`).
const TIME_ZONE_ID_INVALID: u32 = u32::MAX;
/// 100 ns ticks between 1601-01-01 (the `FILETIME` epoch) and 1970-01-01, both UTC.
const FILETIME_UNIX_EPOCH: i64 = 116_444_736_000_000_000;
/// 100 ns ticks per millisecond (`SYSTEMTIME` resolution) and per minute.
const TICKS_PER_MS: i64 = 10_000;
const TICKS_PER_MINUTE: i64 = 600_000_000;

pub(super) fn resolve_folder(path: &Path) -> Result<ResolvedFolder, VolumeError> {
    let canonical = std::fs::canonicalize(path).map_err(VolumeError::Io)?;
    let text = canonical.to_str().ok_or(NameError::NotUnicode)?;
    let drive = classify_path(text)?;
    // The drive form while it fits MAX_PATH (the documented form), the verbatim form beyond.
    let query = if drive.len() < MAX_PATH { drive } else { text };
    let root = volume_path_name(query)?;
    // Normalise the root the same way (it may come back verbatim) and make sure it ends with
    // `\`, which the next calls require.
    let mut root = classify_path(&root)?.to_owned();
    if !root.ends_with('\\') {
        root.push('\\');
    }
    let guid_path = volume_guid_path(&root)?;
    let volume = volume_information(guid_path, root)?;
    let rel = rel_under_root(drive, &volume.mount_point)?;
    let source = Source::new(volume.id, rel)?;
    Ok(ResolvedFolder {
        canonical,
        volume,
        source,
    })
}

/// `s` as a NUL-terminated UTF-16 string.
fn wide(s: &str) -> Result<Vec<u16>, VolumeError> {
    if s.contains('\0') {
        return Err(NameError::ControlChar.into());
    }
    Ok(OsStr::new(s).encode_wide().chain(iter::once(0)).collect())
}

/// The UTF-16 units of `buf` before its first NUL (all of it when there is none).
fn until_nul(buf: &[u16]) -> &[u16] {
    let end = buf.iter().position(|&u| u == 0).unwrap_or(buf.len());
    &buf[..end]
}

/// The string in `buf`, which must be valid UTF-16 (paths).
fn path_string(buf: &[u16]) -> Result<String, VolumeError> {
    String::from_utf16(until_nul(buf)).map_err(|_| NameError::NotUnicode.into())
}

fn last_error(call: &'static str) -> VolumeError {
    VolumeError::Win32 {
        call,
        error: io::Error::last_os_error(),
    }
}

/// A buffer length as the `u32` Win32 takes. The buffers here are at most a few thousand
/// units, so this never fails in practice.
fn len_u32(buf: &[u16]) -> Result<u32, VolumeError> {
    u32::try_from(buf.len()).map_err(|_| VolumeError::Win32 {
        call: "buffer length",
        error: io::Error::from(io::ErrorKind::InvalidInput),
    })
}

/// The root of the volume `path` is on: `C:\`, or a mounted folder such as
/// `C:\Mounts\Card\`.
fn volume_path_name(path: &str) -> Result<String, VolumeError> {
    let input = wide(path)?;
    // The root is never longer than the input, except for an added trailing `\`.
    let mut buf = vec![0u16; input.len().max(MAX_PATH) + 2];
    let len = len_u32(&buf)?;
    // SAFETY: `input` is NUL-terminated and outlives the call; `buf` is a live, writable
    // buffer of exactly `len` u16 units, and the function writes at most `len` units,
    // including the NUL.
    let ok = unsafe { GetVolumePathNameW(input.as_ptr(), buf.as_mut_ptr(), len) };
    if ok == 0 {
        return Err(last_error("GetVolumePathNameW"));
    }
    path_string(&buf)
}

/// The GUID path of the volume mounted at `root` (which ends with `\`).
fn volume_guid_path(root: &str) -> Result<VolumeGuidPath, VolumeError> {
    let input = wide(root)?;
    let mut buf = [0u16; GUID_BUF];
    let len = len_u32(&buf)?;
    // SAFETY: `input` is NUL-terminated and outlives the call; `buf` is a live, writable
    // array of exactly `len` u16 units, and the function writes at most `len` units,
    // including the NUL.
    let ok = unsafe { GetVolumeNameForVolumeMountPointW(input.as_ptr(), buf.as_mut_ptr(), len) };
    if ok == 0 {
        return Err(last_error("GetVolumeNameForVolumeMountPointW"));
    }
    Ok(VolumeGuidPath::parse(&path_string(&buf)?)?)
}

/// Serial, label, file system and flags of the volume at `guid_path`, and the drive type of
/// `root`. Asking by GUID path ties every field to the volume the GUID names, even if a
/// drive letter moves in between.
fn volume_information(guid_path: VolumeGuidPath, root: String) -> Result<VolumeInfo, VolumeError> {
    let input = wide(guid_path.as_str())?;
    let mut label = [0u16; NAME_BUF];
    let mut fs_name = [0u16; NAME_BUF];
    let label_len = len_u32(&label)?;
    let fs_len = len_u32(&fs_name)?;
    let mut serial = 0u32;
    let mut max_component_len = 0u32;
    let mut flags = 0u32;
    // SAFETY: `input` is NUL-terminated and outlives the call. `label` and `fs_name` are live,
    // writable arrays of exactly `label_len` and `fs_len` u16 units; the function writes at
    // most that many, including the NUL. The three `u32` out-pointers point to live locals.
    let ok = unsafe {
        GetVolumeInformationW(
            input.as_ptr(),
            label.as_mut_ptr(),
            label_len,
            &raw mut serial,
            &raw mut max_component_len,
            &raw mut flags,
            fs_name.as_mut_ptr(),
            fs_len,
        )
    };
    if ok == 0 {
        return Err(last_error("GetVolumeInformationW"));
    }
    let root_w = wide(&root)?;
    // SAFETY: `root_w` is NUL-terminated and outlives the call; the function only reads it.
    let drive_type = unsafe { GetDriveTypeW(root_w.as_ptr()) };
    // Label and file system name are display text: decoded lossily.
    let label = String::from_utf16_lossy(until_nul(&label));
    Ok(VolumeInfo {
        id: VolumeId::new(serial),
        guid_path,
        mount_point: root,
        label: (!label.is_empty()).then_some(label),
        fs_name: String::from_utf16_lossy(until_nul(&fs_name)),
        flags: VolumeFlags(flags),
        max_component_len,
        drive_kind: DriveKind::from_win32(drive_type),
    })
}

pub(super) fn local_utc_offset_minutes(utc_ns: i64) -> Option<i16> {
    // `FILETIME` ticks (100 ns since 1601), truncated to the millisecond so the round trip
    // through `SYSTEMTIME` (millisecond resolution) is exact. Before 1601: no answer.
    let ticks = utc_ns.div_euclid(100).checked_add(FILETIME_UNIX_EPOCH)?;
    let ticks = ticks - ticks.rem_euclid(TICKS_PER_MS);
    let utc_file = filetime(u64::try_from(ticks).ok()?);

    let mut utc_sys = ZERO_SYSTEMTIME;
    let mut local_sys = ZERO_SYSTEMTIME;
    let mut local_file = filetime(0);
    // SAFETY: `DYNAMIC_TIME_ZONE_INFORMATION` is a plain C struct of integers, integer arrays
    // and `SYSTEMTIME`s, for which all zero bytes is a valid value.
    let mut zone = unsafe { std::mem::zeroed::<DYNAMIC_TIME_ZONE_INFORMATION>() };
    // SAFETY: both pointers point to live locals of the right types; `utc_file` is only read.
    // `ticks` is below 2^63, the function's documented upper bound.
    if unsafe { FileTimeToSystemTime(&raw const utc_file, &raw mut utc_sys) } == 0 {
        return None;
    }
    // SAFETY: the pointer points to a live, writable local of the right type.
    if unsafe { GetDynamicTimeZoneInformation(&raw mut zone) } == TIME_ZONE_ID_INVALID {
        return None;
    }
    // SAFETY: all three pointers point to live locals of the right types; `zone` and
    // `utc_sys` are only read. The dynamic rules give the daylight saving rule of that year.
    if unsafe {
        SystemTimeToTzSpecificLocalTimeEx(&raw const zone, &raw const utc_sys, &raw mut local_sys)
    } == 0
    {
        return None;
    }
    // SAFETY: both pointers point to live locals of the right types; `local_sys` is only read.
    if unsafe { SystemTimeToFileTime(&raw const local_sys, &raw mut local_file) } == 0 {
        return None;
    }
    let local = i64::try_from(
        (u64::from(local_file.dwHighDateTime) << 32) | u64::from(local_file.dwLowDateTime),
    )
    .ok()?;
    // Zone offsets are whole minutes; round in case of a stray millisecond.
    let minutes = (local - ticks + TICKS_PER_MINUTE / 2).div_euclid(TICKS_PER_MINUTE);
    i16::try_from(minutes).ok()
}

const ZERO_SYSTEMTIME: SYSTEMTIME = SYSTEMTIME {
    wYear: 0,
    wMonth: 0,
    wDayOfWeek: 0,
    wDay: 0,
    wHour: 0,
    wMinute: 0,
    wSecond: 0,
    wMilliseconds: 0,
};

fn filetime(ticks: u64) -> FILETIME {
    FILETIME {
        dwLowDateTime: (ticks & 0xFFFF_FFFF) as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    }
}
