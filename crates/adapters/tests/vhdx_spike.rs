//! VHDX spike (M0, docs/testing.md section 5.1).
//!
//! These tests observe what a process sees on a VHDX volume mounted by
//! `scripts/ci/vhdx.ps1`. They are `#[ignore]` and do nothing unless
//! `CULLA_VHDX_LETTER` names the volume, and they only run on Windows.
//! `.github/workflows/vhdx-spike.yml` drives them.
//!
//! Environment:
//! - `CULLA_VHDX_LETTER`: drive letter of the test volume, for example `T`.
//! - `CULLA_VHDX_SIGNAL_DIR`: (detach spike) a folder on another volume. The test
//!   creates `ready` there once its handles are open, then waits for `detached`,
//!   written by the workflow after it detached the volume.
//! - `CULLA_VHDX_REPORT`: optional file the observations are appended to.
//! - `CULLA_VHDX_TIMEOUT_SECS`: how long to wait for `detached` (default 180).
//!
//! Files are only ever created with `create_new` under unique names, so the tests
//! never overwrite or remove anything, also on a reused volume.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SPIKE_DIR: &str = "culla-spike";

/// The volume root, or `None` when the spike is not configured on this machine.
fn volume_root(test: &str) -> Option<PathBuf> {
    if !cfg!(windows) {
        println!("{test}: skipped, Windows only");
        return None;
    }
    let Ok(raw) = std::env::var("CULLA_VHDX_LETTER") else {
        println!("{test}: skipped, CULLA_VHDX_LETTER is not set");
        return None;
    };
    let letter = raw.trim().trim_end_matches(['\\', ':']);
    let mut chars = letter.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => {
            Some(PathBuf::from(format!("{}:\\", c.to_ascii_uppercase())))
        }
        _ => panic!("CULLA_VHDX_LETTER must be a single drive letter, got {raw:?}"),
    }
}

fn unique(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("{prefix}-{}-{nanos}", std::process::id())
}

fn create_new(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
}

fn pattern(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

/// Names for the Win32 error codes this spike is likely to meet.
fn win32_name(code: i32) -> &'static str {
    match code {
        2 => "ERROR_FILE_NOT_FOUND",
        3 => "ERROR_PATH_NOT_FOUND",
        5 => "ERROR_ACCESS_DENIED",
        6 => "ERROR_INVALID_HANDLE",
        15 => "ERROR_INVALID_DRIVE",
        19 => "ERROR_WRITE_PROTECT",
        21 => "ERROR_NOT_READY",
        31 => "ERROR_GEN_FAILURE",
        32 => "ERROR_SHARING_VIOLATION",
        53 => "ERROR_BAD_NETPATH",
        55 => "ERROR_DEV_NOT_EXIST",
        80 => "ERROR_FILE_EXISTS",
        87 => "ERROR_INVALID_PARAMETER",
        112 => "ERROR_DISK_FULL",
        183 => "ERROR_ALREADY_EXISTS",
        483 => "ERROR_DEVICE_HARDWARE_ERROR",
        1005 => "ERROR_UNRECOGNIZED_VOLUME",
        1006 => "ERROR_FILE_INVALID",
        1117 => "ERROR_IO_DEVICE",
        1167 => "ERROR_DEVICE_NOT_CONNECTED",
        _ => "unknown",
    }
}

fn outcome<T>(result: &io::Result<T>) -> String {
    match result {
        Ok(_) => "ok".to_owned(),
        Err(e) => match e.raw_os_error() {
            Some(code) => format!(
                "err kind={:?} raw_os_error={code} ({}) msg={e}",
                e.kind(),
                win32_name(code)
            ),
            None => format!("err kind={:?} raw_os_error=none msg={e}", e.kind()),
        },
    }
}

/// Collects one line per observation, prints it, and appends all of them to
/// `CULLA_VHDX_REPORT` at the end.
struct Report {
    test: &'static str,
    lines: Vec<String>,
}

impl Report {
    fn new(test: &'static str) -> Self {
        Self {
            test,
            lines: Vec::new(),
        }
    }

    fn note(&mut self, op: &str, detail: &str) {
        let line = format!("{} | {op} | {detail}", self.test);
        println!("SPIKE {line}");
        self.lines.push(line);
    }

    /// Runs `f`, records its outcome and duration, and returns its result.
    fn op<T>(&mut self, op: &str, f: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
        let start = Instant::now();
        let result = f();
        let ms = start.elapsed().as_millis();
        self.note(op, &format!("{} ({ms} ms)", outcome(&result)));
        result
    }

    fn flush(&self) {
        let Ok(path) = std::env::var("CULLA_VHDX_REPORT") else {
            return;
        };
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap_or_else(|e| panic!("open report {path}: {e}"));
        for line in &self.lines {
            writeln!(file, "{line}").unwrap_or_else(|e| panic!("write report {path}: {e}"));
        }
    }
}

/// Writes a file on the volume, reads it back and lists it. Every step must succeed.
#[test]
#[ignore = "needs a mounted VHDX volume (CULLA_VHDX_LETTER), see scripts/ci/README.md"]
fn vhdx_write_placeholder() {
    let Some(root) = volume_root("vhdx_write_placeholder") else {
        return;
    };
    let mut report = Report::new("vhdx_write_placeholder");
    report.note("volume", &root.display().to_string());

    let dir = root.join(SPIKE_DIR);
    let made = report.op("create_dir_all", || fs::create_dir_all(&dir));
    let path = dir.join(unique("placeholder"));
    let data = pattern(1024 * 1024, 7);
    let written = report.op("create_new + write 1 MiB + sync_all", || {
        let mut f = create_new(&path)?;
        f.write_all(&data)?;
        f.sync_all()
    });
    let read = report.op("reopen + read back", || fs::read(&path));
    let listed = report.op("read_dir", || {
        fs::read_dir(&dir)?
            .map(|entry| entry.map(|e| e.file_name()))
            .collect::<io::Result<Vec<_>>>()
    });
    report.flush();

    made.expect("create_dir_all");
    written.expect("write");
    assert!(
        read.expect("read") == data,
        "read-back differs from what was written"
    );
    let name = path.file_name().expect("file name");
    assert!(
        listed.expect("read_dir").iter().any(|n| n == name),
        "written file missing from the listing"
    );
}

fn wait_for(path: &Path, timeout: Duration) -> Option<String> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(text) = fs::read_to_string(path) {
            return Some(text.trim().to_owned());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    None
}

/// Opens files on the volume, keeps the handles, signals `ready`, waits for the
/// workflow to detach the volume, then records what each kind of access returns.
/// It fails only when the protocol breaks (no signal dir, no `detached` in time).
#[test]
#[ignore = "needs a mounted VHDX volume and the workflow's detach step, see scripts/ci/README.md"]
fn vhdx_detach_spike() {
    let Some(root) = volume_root("vhdx_detach_spike") else {
        return;
    };
    let signal_dir = PathBuf::from(
        std::env::var("CULLA_VHDX_SIGNAL_DIR").expect("CULLA_VHDX_SIGNAL_DIR must be set"),
    );
    let timeout = Duration::from_secs(
        std::env::var("CULLA_VHDX_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(180),
    );
    let mut report = Report::new("vhdx_detach_spike");
    report.note("volume", &root.display().to_string());

    // Before the detach: one read-write handle with a synced head and an unsynced
    // tail, and one read-only handle on a second, fully synced file.
    let dir = root.join(SPIKE_DIR);
    fs::create_dir_all(&dir).expect("create_dir_all");
    let held_path = dir.join(unique("held-rw"));
    let mut held = create_new(&held_path).expect("create held-rw");
    held.write_all(&pattern(256 * 1024, 1))
        .expect("write synced head");
    held.sync_all().expect("sync head");
    held.write_all(&pattern(64 * 1024, 2))
        .expect("write unsynced tail");
    report.note(
        "before detach",
        &format!(
            "{} has 256 KiB synced + 64 KiB unsynced",
            held_path.display()
        ),
    );

    let ro_path = dir.join(unique("held-ro"));
    {
        let mut f = create_new(&ro_path).expect("create held-ro");
        f.write_all(&pattern(64 * 1024, 3)).expect("write held-ro");
        f.sync_all().expect("sync held-ro");
    }
    let mut reader = File::open(&ro_path).expect("open held-ro for reading");

    fs::create_dir_all(&signal_dir).expect("create signal dir");
    create_new(&signal_dir.join("ready"))
        .and_then(|mut f| writeln!(f, "pid={}", std::process::id()))
        .expect("signal ready");
    println!(
        "SPIKE ready, handles open, waiting for {}",
        signal_dir.join("detached").display()
    );

    let Some(method) = wait_for(&signal_dir.join("detached"), timeout) else {
        report.note("detached signal", "timed out");
        report.flush();
        panic!("no detached signal within {timeout:?}");
    };
    report.note("detached signal", &method);
    report.note("volume root exists", &root.exists().to_string());

    // After the detach, through the handles opened before it.
    let mut buf = vec![0_u8; 4096];
    let _ = report.op("held-rw: write 4 KiB", || held.write_all(&pattern(4096, 4)));
    let _ = report.op("held-rw: flush (FlushFileBuffers)", || held.sync_all());
    let _ = report.op("held-rw: seek 0 + read 4 KiB", || {
        held.seek(SeekFrom::Start(0))?;
        held.read_exact(&mut buf)
    });
    let _ = report.op("held-rw: metadata by handle", || held.metadata());
    let _ = report.op("held-ro: read 4 KiB", || reader.read_exact(&mut buf));

    // After the detach, through paths.
    let _ = report.op("path: metadata of held-rw", || fs::metadata(&held_path));
    let _ = report.op("path: open held-ro for reading", || File::open(&ro_path));
    let _ = report.op("path: create_new new file", || {
        create_new(&dir.join(unique("after-detach")))
    });
    let _ = report.op("path: read_dir volume root", || {
        fs::read_dir(&root).map(|it| it.count())
    });

    let start = Instant::now();
    drop(held);
    drop(reader);
    report.note(
        "close handles",
        &format!("returned ({} ms)", start.elapsed().as_millis()),
    );
    report.flush();
}

/// Runs on a volume the workflow made read-only. Passes only if creating a file,
/// creating a folder and writing to an existing file all fail, so its exit code tells
/// the workflow whether a read-only method works.
///
/// First CI run (24 Nov 2026): opening an existing file with write access *succeeds* on
/// a write-protected volume; only the write fails (ERROR_WRITE_PROTECT, 19). So the
/// engine's preflight can't detect an SD lock by opening a file; it uses the volume's
/// read-only flag (design/file-engine.md, preflight).
#[test]
#[ignore = "needs a read-only VHDX volume (CULLA_VHDX_LETTER), see scripts/ci/README.md"]
fn vhdx_readonly_spike() {
    let Some(root) = volume_root("vhdx_readonly_spike") else {
        return;
    };
    let mut report = Report::new("vhdx_readonly_spike");
    report.note("volume", &root.display().to_string());

    let dir = root.join(SPIKE_DIR);
    let existing = fs::read_dir(&dir).ok().and_then(|it| {
        it.filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.is_file())
    });

    let created = report.op("create_new new file", || {
        create_new(&dir.join(unique("readonly-probe")))
    });
    let made_dir = report.op("create_dir new folder", || {
        fs::create_dir(root.join(unique("readonly-dir")))
    });
    let opened_rw = existing.as_ref().map(|path| {
        report.op("open existing for writing", || {
            OpenOptions::new().write(true).open(path)
        })
    });
    let wrote = match &opened_rw {
        Some(Ok(file)) => {
            let mut file = file;
            Some(report.op("write 1 byte at end + sync_all", || {
                use std::io::{Seek, SeekFrom, Write};
                file.seek(SeekFrom::End(0))?;
                file.write_all(b"x")?;
                file.sync_all()
            }))
        }
        _ => None,
    };
    if let Some(path) = &existing {
        let _ = report.op("read existing", || fs::read(path).map(|d| d.len()));
    } else {
        report.note(
            "existing file",
            "none found (run vhdx_write_placeholder first)",
        );
    }
    report.flush();

    assert!(
        created.is_err(),
        "create_new succeeded on a read-only volume"
    );
    assert!(
        made_dir.is_err(),
        "create_dir succeeded on a read-only volume"
    );
    if let Some(r) = wrote {
        assert!(r.is_err(), "a write succeeded on a read-only volume");
    }
}
