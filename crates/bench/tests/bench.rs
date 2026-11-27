//! `culla-bench` run as a process on the synthetic fixtures: every benchmark runs, prints a
//! result with `"schema": "unstable"`, and leaves the folder exactly as it was (the bench
//! writes nothing but stdout and stderr). `report` reads results back from files that this
//! test writes into its own temp folder, which [`TempDir`] removes on drop (the only removal
//! here, inside its own temp folder; the lint may be switched off in `crates/*/tests/`).
//!
//! Timings are not checked: they depend on the machine.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn synthetic() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/synthetic")
}

fn bench(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_culla-bench"))
        .args(args)
        .output()
        .expect("run culla-bench")
}

fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).expect("stdout is UTF-8")
}

/// Every file and folder under `root` with its length and modification time.
fn snapshot(root: &Path) -> Vec<(PathBuf, u64, Option<SystemTime>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir).unwrap() {
            let e = e.unwrap();
            let m = fs::symlink_metadata(e.path()).unwrap();
            if m.is_dir() {
                stack.push(e.path());
            }
            out.push((e.path(), m.len(), m.modified().ok()));
        }
    }
    out.sort();
    out
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir =
            std::env::temp_dir().join(format!("culla-bench-test-{}-{nanos}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }

    fn file(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        f.write_all(text.as_bytes()).unwrap();
        path
    }
}

impl Drop for TempDir {
    #[allow(clippy::disallowed_methods, reason = "test temp folder only")]
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn every_benchmark_runs_read_only_and_reports() {
    let root = synthetic();
    let folder = root.to_str().unwrap();
    let before = snapshot(&root);
    let tmp = TempDir::new();
    let mut saved = Vec::new();
    for (name, args) in [
        ("scan", vec!["scan", folder, "--exif"]),
        ("preview", vec!["preview", folder, "--source", "nef"]),
        (
            "swipe",
            vec![
                "swipe",
                folder,
                "--cadence",
                "max",
                "--steps",
                "3",
                "--workers",
                "1",
            ],
        ),
        ("io", vec!["io-baseline", folder, "--mb", "1"]),
    ] {
        let text = bench(&args);
        assert_eq!(text.status.code(), Some(0), "{name}: {text:?}");
        let t = stdout(&text);
        assert!(t.contains("written: nothing (read-only)"), "{name}: {t}");

        let mut json_args = args.clone();
        json_args.push("--json");
        json_args.extend(["--label", "test run"]);
        let o = bench(&json_args);
        assert_eq!(o.status.code(), Some(0), "{name}: {o:?}");
        let j = stdout(&o);
        assert!(j.is_ascii(), "{name}");
        assert!(j.contains("\"schema\": \"unstable\""), "{name}: {j}");
        assert!(j.contains("\"label\": \"test run\""), "{name}: {j}");
        assert!(
            !j.contains(folder),
            "{name}: the folder path is not in the JSON"
        );
        saved.push(tmp.file(&format!("{name}.json"), &j));
    }
    let scan = fs::read_to_string(&saved[0]).unwrap();
    for key in [
        "first_group_ms",
        "listing_ms",
        "exif_ms",
        "peak_memory_bytes",
    ] {
        assert!(
            scan.contains(&format!("\"key\": \"{key}\"")),
            "{key}: {scan}"
        );
    }
    let swipe = fs::read_to_string(&saved[2]).unwrap();
    assert!(swipe.contains("\"key\": \"next_photo_ms\""), "{swipe}");

    let mut args = vec!["report"];
    args.extend(saved.iter().map(|p| p.to_str().unwrap()));
    let o = bench(&args);
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    let r = stdout(&o);
    for b in ["scan", "preview", "swipe", "io-baseline"] {
        assert!(r.contains(&format!("benchmark: {b} (1 runs)")), "{b}: {r}");
    }

    assert_eq!(before, snapshot(&root), "the folder was changed");
}

#[test]
fn exit_codes() {
    assert_eq!(bench(&[]).status.code(), Some(1));
    assert_eq!(bench(&["scan"]).status.code(), Some(1));
    assert_eq!(bench(&["help"]).status.code(), Some(0));
    let missing = synthetic().join("no-such-folder");
    assert_eq!(
        bench(&["scan", missing.to_str().unwrap()]).status.code(),
        Some(3)
    );
    let file = synthetic().join("pair").join("DSC_0001.JPG");
    let o = bench(&["scan", file.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(2), "{o:?}");

    let tmp = TempDir::new();
    let bad = tmp.file("bad.json", "{\"tool\": \"other\"}");
    let o = bench(&["report", bad.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(3), "{o:?}");
    assert!(String::from_utf8_lossy(&o.stderr).contains("not a culla-bench result"));
}
