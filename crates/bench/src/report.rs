//! `culla-bench report`: reads saved results (the `--json` output of earlier runs) and prints,
//! for each set of comparable runs, the median, min and max of every metric across runs
//! (benchmarks.md §4.4: a result passes on the median of 5 cold runs) and every per-item
//! sample pooled over the runs (p50, p95, p99). Runs are comparable when they share the
//! benchmark, the parameters, the medium's drive kind and file system, the build profile and
//! the culla version. Runs of a build other than release are listed, flagged, and left out of
//! the figures. An empty file (a run that failed before printing) is skipped with a note.
//! Reads only the files given. Everything taken from a file is printed through
//! [`shown`] (threat model T6b).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::folder::read_all;
use crate::json::{self, Value};
use crate::run::shown;
use crate::stats::{Summary, median};

/// A result file larger than this is refused.
const MAX_FILE: u64 = 256 * 1024 * 1024;

/// One run read back.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Loaded {
    pub path: PathBuf,
    pub value: Value,
}

/// Reads and checks one result file. `Ok(None)` when it is empty (nothing but a byte order
/// mark or white space).
///
/// # Errors
///
/// When the file cannot be read, is not JSON or is not a culla-bench result.
pub(crate) fn load(path: &Path) -> Result<Option<Loaded>, String> {
    let shown_path = shown(&path.to_string_lossy());
    let bytes = read_all(path, MAX_FILE).map_err(|e| format!("{shown_path}: {e}"))?;
    let value = from_bytes(bytes).map_err(|e| format!("{shown_path}: {e}"))?;
    Ok(value.map(|value| Loaded {
        path: path.to_path_buf(),
        value,
    }))
}

/// A result file's content checked, `None` when empty.
fn from_bytes(bytes: Vec<u8>) -> Result<Option<Value>, String> {
    let text = String::from_utf8(bytes).map_err(|_| {
        "not UTF-8 (in Windows PowerShell 5.1, save with Out-File -Encoding utf8)".to_owned()
    })?;
    if text.trim_start_matches('\u{feff}').trim().is_empty() {
        return Ok(None);
    }
    let value = json::parse(&text)?;
    check(&value)?;
    Ok(Some(value))
}

fn check(v: &Value) -> Result<(), String> {
    if v.get("tool").and_then(Value::as_str) != Some("culla-bench") {
        return Err("not a culla-bench result".to_owned());
    }
    if v.get("schema").and_then(Value::as_str) != Some("unstable") {
        return Err("unknown schema".to_owned());
    }
    if v.get("benchmark").and_then(Value::as_str).is_none() {
        return Err("no benchmark".to_owned());
    }
    Ok(())
}

/// What makes runs comparable.
#[derive(Debug, PartialEq, Eq)]
struct SetKey {
    benchmark: String,
    params: String,
    /// Drive kind and file system, or `unavailable`.
    medium: String,
    profile: String,
    version: String,
}

impl SetKey {
    fn of(v: &Value) -> Self {
        let text = |x: Option<&Value>| x.and_then(Value::as_str).unwrap_or("?").to_owned();
        let m = v.get("medium");
        let medium = if m.and_then(|m| m.get("unavailable")).is_some() {
            "unavailable".to_owned()
        } else {
            format!(
                "{}, {}",
                text(m.and_then(|m| m.get("drive_kind"))),
                text(m.and_then(|m| m.get("file_system")))
            )
        };
        Self {
            benchmark: text(v.get("benchmark")),
            params: v.get("params").map(Value::to_compact).unwrap_or_default(),
            medium,
            profile: text(v.get("build").and_then(|b| b.get("profile"))),
            version: text(v.get("version")),
        }
    }
}

/// Comparable runs.
struct Set<'a> {
    key: SetKey,
    runs: Vec<&'a Loaded>,
}

/// The values of `field` (`metrics` or `samples`) of `run` by key, in file order.
fn entries<'a>(run: &'a Value, field: &str) -> Vec<(&'a str, &'a str, &'a str, &'a Value)> {
    run.get(field)
        .and_then(Value::as_arr)
        .unwrap_or_default()
        .iter()
        .filter_map(|m| {
            Some((
                m.get("key")?.as_str()?,
                m.get("id").and_then(Value::as_str).unwrap_or(""),
                m.get("unit").and_then(Value::as_str).unwrap_or(""),
                m.get(if field == "metrics" {
                    "value"
                } else {
                    "values"
                })?,
            ))
        })
        .collect()
}

/// Keys in first-seen order, with their id and unit.
fn keys<'a>(runs: &[&'a Loaded], field: &str) -> Vec<(&'a str, &'a str, &'a str)> {
    let mut out: Vec<(&str, &str, &str)> = Vec::new();
    for r in runs {
        for (k, id, unit, _) in entries(&r.value, field) {
            if !out.iter().any(|(x, _, _)| *x == k) {
                out.push((k, id, unit));
            }
        }
    }
    out
}

fn num(v: f64) -> String {
    if v.abs() >= 1_000_000.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

/// The report over `runs`, as text ending with a newline.
#[must_use]
pub(crate) fn render(runs: &[Loaded]) -> String {
    let mut sets: Vec<Set<'_>> = Vec::new();
    for r in runs {
        let key = SetKey::of(&r.value);
        match sets.iter_mut().find(|s| s.key == key) {
            Some(s) => s.runs.push(r),
            None => sets.push(Set { key, runs: vec![r] }),
        }
    }

    let mut o = String::new();
    for (i, set) in sets.iter().enumerate() {
        if i > 0 {
            let _ = writeln!(o);
        }
        let k = &set.key;
        let release = k.profile == "release";
        let _ = writeln!(
            o,
            "benchmark: {} ({} runs)",
            shown(&k.benchmark),
            set.runs.len()
        );
        let _ = writeln!(o, "params:    {}", shown(&k.params));
        let _ = writeln!(o, "medium:    {}", shown(&k.medium));
        let _ = writeln!(
            o,
            "build:     {}, culla {}",
            shown(&k.profile),
            shown(&k.version)
        );
        for r in &set.runs {
            let label = r.value.get("label").and_then(Value::as_str).unwrap_or("-");
            let _ = writeln!(
                o,
                "run:       {} ({}){}",
                shown(&r.path.to_string_lossy()),
                shown(label),
                if release {
                    String::new()
                } else {
                    format!(", {} build: timings not comparable", shown(&k.profile))
                }
            );
        }
        let _ = writeln!(o);
        if !release {
            let _ = writeln!(
                o,
                "no figures: only release builds are summarised (benchmarks.md §7.1)"
            );
            continue;
        }
        let _ = writeln!(
            o,
            "{:<5} {:<30} {:>4} {:>12} {:>12} {:>12}  unit",
            "", "metric (across runs)", "n", "median", "min", "max"
        );
        for (raw, id, unit) in keys(&set.runs, "metrics") {
            let (key, id, unit) = (shown(raw), shown(id), shown(unit));
            let vals: Vec<f64> = set
                .runs
                .iter()
                .flat_map(|r| entries(&r.value, "metrics"))
                .filter(|(k, ..)| *k == raw)
                .filter_map(|(.., v)| v.as_f64())
                .collect();
            let (Some(med), Some(lo), Some(hi)) = (
                median(&vals),
                vals.iter().copied().reduce(f64::min),
                vals.iter().copied().reduce(f64::max),
            ) else {
                let _ = writeln!(o, "{id:<5} {key:<30} {:>4} {:>12}", 0, "n/a");
                continue;
            };
            let _ = writeln!(
                o,
                "{id:<5} {key:<30} {:>4} {:>12} {:>12} {:>12}  {unit}",
                vals.len(),
                num(med),
                num(lo),
                num(hi)
            );
        }
        let sample_keys = keys(&set.runs, "samples");
        if !sample_keys.is_empty() {
            let _ = writeln!(o);
            let _ = writeln!(
                o,
                "{:<5} {:<30} {:>6} {:>10} {:>10} {:>10} {:>10}  unit",
                "", "per item (pooled)", "n", "p50", "p95", "p99", "max"
            );
        }
        for (raw, id, unit) in sample_keys {
            let (key, id, unit) = (shown(raw), shown(id), shown(unit));
            let vals: Vec<f64> = set
                .runs
                .iter()
                .flat_map(|r| entries(&r.value, "samples"))
                .filter(|(k, ..)| *k == raw)
                .flat_map(|(.., v)| {
                    v.as_arr()
                        .unwrap_or_default()
                        .iter()
                        .filter_map(Value::as_f64)
                })
                .collect();
            match Summary::of(&vals) {
                Some(s) => {
                    let _ = writeln!(
                        o,
                        "{id:<5} {key:<30} {:>6} {:>10.1} {:>10.1} {:>10.1} {:>10.1}  {unit}",
                        s.n, s.p50, s.p95, s.p99, s.max
                    );
                }
                None => {
                    let _ = writeln!(o, "{id:<5} {key:<30} {:>6}", 0);
                }
            }
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::{Medium, Metric, Run, Samples, Unit, to_json};

    /// Sets `field` of a JSON object, as a saved file from another build would hold it.
    fn set(v: &mut Value, field: &str, to: Value) {
        let Value::Obj(fields) = v else { panic!() };
        fields.iter_mut().find(|(k, _)| k == field).unwrap().1 = to;
    }

    fn release(mut l: Loaded) -> Loaded {
        set(
            &mut l.value,
            "build",
            Value::obj([("profile", Value::str("release"))]),
        );
        l
    }

    fn run(label: &str, cadence: &str, first: f64, lat: Vec<f64>) -> Loaded {
        let r = Run {
            benchmark: "swipe",
            label: Some(label.to_owned()),
            folder: PathBuf::from("/x"),
            medium: Medium::Unavailable("test".to_owned()),
            params: vec![("cadence", Value::str(cadence))],
            metrics: vec![
                Metric::new("first_preview_ms", "", Unit::Ms, Some(first)),
                Metric::new("peak_memory_bytes", "B-15", Unit::Bytes, None),
            ],
            samples: vec![Samples {
                key: "next_photo_ms",
                id: "B-09",
                unit: Unit::Ms,
                values: lat,
            }],
            notes: vec![],
        };
        // Through text and back, as a saved file would be.
        let value = json::parse(&to_json(&r).to_pretty()).unwrap();
        check(&value).unwrap();
        release(Loaded {
            path: PathBuf::from(format!("{label}.json")),
            value,
        })
    }

    #[test]
    fn medians_across_runs_and_pooled_quantiles() {
        let runs = vec![
            run("cold 1", "steady", 900.0, (1..=50).map(f64::from).collect()),
            run(
                "cold 2",
                "steady",
                1100.0,
                (51..=100).map(f64::from).collect(),
            ),
            run("cold 3", "steady", 1000.0, vec![]),
            run("fast 1", "fast", 5.0, vec![7.0]),
        ];
        let t = render(&runs);
        assert!(t.contains("benchmark: swipe (3 runs)"), "{t}");
        assert!(t.contains("benchmark: swipe (1 runs)"), "{t}");
        assert!(t.contains("medium:    unavailable"), "{t}");
        assert!(!t.contains("not comparable"), "{t}");
        let first = t.lines().find(|l| l.contains("first_preview_ms")).unwrap();
        // Median 1000 of 900, 1000, 1100; min 900; max 1100.
        assert!(
            first.contains("1000.0") && first.contains("900.0") && first.contains("1100.0"),
            "{first}"
        );
        let lat = t.lines().find(|l| l.contains("next_photo_ms")).unwrap();
        // Pooled 1..=100: p50 50, p95 95, p99 99, max 100.
        assert!(
            lat.contains(" 100 ") && lat.contains("95.0") && lat.contains("99.0"),
            "{lat}"
        );
        let mem = t.lines().find(|l| l.contains("peak_memory_bytes")).unwrap();
        assert!(mem.contains("n/a"), "{mem}");
    }

    #[test]
    fn medium_profile_and_version_split_the_sets() {
        let base = run("a", "steady", 10.0, vec![]);
        let mut fixed = run("b", "steady", 20.0, vec![]);
        set(
            &mut fixed.value,
            "medium",
            Value::obj([
                ("drive_kind", Value::str("fixed")),
                ("file_system", Value::str("NTFS")),
            ]),
        );
        let mut card = run("c", "steady", 30.0, vec![]);
        set(
            &mut card.value,
            "medium",
            Value::obj([
                ("drive_kind", Value::str("removable")),
                ("file_system", Value::str("exFAT")),
            ]),
        );
        let mut other_version = run("d", "steady", 40.0, vec![]);
        set(&mut other_version.value, "version", Value::str("9.9.9"));
        let mut debug = run("e", "steady", 50.0, vec![1.0]);
        set(
            &mut debug.value,
            "build",
            Value::obj([("profile", Value::str("debug"))]),
        );
        let t = render(&[base, fixed, card, other_version, debug]);
        assert_eq!(t.matches("(1 runs)").count(), 5, "{t}");
        assert!(t.contains("medium:    fixed, NTFS"), "{t}");
        assert!(t.contains("medium:    removable, exFAT"), "{t}");
        assert!(t.contains("culla 9.9.9"), "{t}");
        // The debug run is listed and flagged, with no figures.
        assert!(
            t.contains("e.json (e), debug build: timings not comparable"),
            "{t}"
        );
        assert!(t.contains("no figures: only release builds"), "{t}");
        assert!(!t.contains("50.0"), "{t}");
    }

    #[test]
    fn text_from_a_file_is_shown_escaped() {
        let mut r = run("a", "steady", 10.0, vec![1.0]);
        set(&mut r.value, "benchmark", Value::str("swipe\u{1b}[2J"));
        set(
            &mut r.value,
            "metrics",
            Value::Arr(vec![Value::obj([
                ("key", Value::str("k\u{202e}")),
                ("id", Value::str("B\u{7}")),
                ("unit", Value::str("ms\r")),
                ("value", Value::Num(1.0)),
            ])]),
        );
        set(
            &mut r.value,
            "samples",
            Value::Arr(vec![Value::obj([
                ("key", Value::str("s\u{200b}")),
                ("id", Value::str("")),
                ("unit", Value::str("u\n")),
                ("values", Value::Arr(vec![Value::Num(2.0)])),
            ])]),
        );
        let t = render(&[r]);
        assert!(!t.chars().any(|c| c.is_control() && c != '\n'), "{t:?}");
        assert!(!t.contains('\u{202e}') && !t.contains('\u{200b}'), "{t:?}");
        assert!(t.contains("swipe\\u{1b}[2J"), "{t}");
        assert!(t.contains("k\\u{202e}") && t.contains("B\\u{7}"), "{t}");
        assert!(t.contains("ms\\u{d}") && t.contains("u\\u{a}"), "{t}");
        assert!(t.contains("s\\u{200b}"), "{t}");
    }

    #[test]
    fn empty_files_are_skipped() {
        assert_eq!(from_bytes(Vec::new()), Ok(None));
        assert_eq!(from_bytes(b"\xef\xbb\xbf\r\n".to_vec()), Ok(None));
        assert!(from_bytes(b"nope".to_vec()).is_err());
        assert!(from_bytes(vec![0xff]).unwrap_err().contains("UTF-8"));
        let ok = br#"{"tool":"culla-bench","schema":"unstable","benchmark":"scan"}"#;
        assert!(from_bytes(ok.to_vec()).unwrap().is_some());
    }

    #[test]
    fn refuses_what_is_not_a_result() {
        assert!(check(&json::parse("{}").unwrap()).is_err());
        let v = json::parse(r#"{"tool":"culla-bench","schema":"v9","benchmark":"scan"}"#).unwrap();
        assert_eq!(check(&v).unwrap_err(), "unknown schema");
        let v = json::parse(r#"{"tool":"culla-bench","schema":"unstable"}"#).unwrap();
        assert_eq!(check(&v).unwrap_err(), "no benchmark");
    }
}
