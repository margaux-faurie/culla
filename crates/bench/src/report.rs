//! `culla-bench report`: reads saved results (the `--json` output of earlier runs) and prints,
//! for each set of runs with the same benchmark and parameters, the median, min and max of
//! every metric across runs (benchmarks.md §4.4: a result passes on the median of 5 cold runs)
//! and every per-item sample pooled over the runs (p50, p95, p99). Reads only the files
//! given.

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

/// Reads and checks one result file.
///
/// # Errors
///
/// When the file cannot be read, is not JSON or is not a culla-bench result.
pub(crate) fn load(path: &Path) -> Result<Loaded, String> {
    let shown_path = shown(&path.to_string_lossy());
    let bytes = read_all(path, MAX_FILE).map_err(|e| format!("{shown_path}: {e}"))?;
    let text = String::from_utf8(bytes).map_err(|_| {
        format!(
            "{shown_path}: not UTF-8 (in Windows PowerShell 5.1, save with Out-File -Encoding utf8)"
        )
    })?;
    let value = json::parse(&text).map_err(|e| format!("{shown_path}: {e}"))?;
    check(&value).map_err(|e| format!("{shown_path}: {e}"))?;
    Ok(Loaded {
        path: path.to_path_buf(),
        value,
    })
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

/// Runs of one benchmark with the same parameters.
struct Set<'a> {
    benchmark: String,
    params: String,
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
        let benchmark = r
            .value
            .get("benchmark")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let params = r
            .value
            .get("params")
            .map(Value::to_compact)
            .unwrap_or_default();
        match sets
            .iter_mut()
            .find(|s| s.benchmark == benchmark && s.params == params)
        {
            Some(s) => s.runs.push(r),
            None => sets.push(Set {
                benchmark,
                params,
                runs: vec![r],
            }),
        }
    }

    let mut o = String::new();
    for (i, set) in sets.iter().enumerate() {
        if i > 0 {
            let _ = writeln!(o);
        }
        let _ = writeln!(o, "benchmark: {} ({} runs)", set.benchmark, set.runs.len());
        let _ = writeln!(o, "params:    {}", shown(&set.params));
        for r in &set.runs {
            let label = r.value.get("label").and_then(Value::as_str).unwrap_or("-");
            let profile = r
                .value
                .get("build")
                .and_then(|b| b.get("profile"))
                .and_then(Value::as_str)
                .unwrap_or("?");
            let _ = writeln!(
                o,
                "run:       {} ({}){}",
                shown(&r.path.to_string_lossy()),
                shown(label),
                if profile == "release" {
                    String::new()
                } else {
                    format!(", {} build: timings not comparable", shown(profile))
                }
            );
        }
        let _ = writeln!(o);
        let _ = writeln!(
            o,
            "{:<5} {:<30} {:>4} {:>12} {:>12} {:>12}  unit",
            "", "metric (across runs)", "n", "median", "min", "max"
        );
        for (key, id, unit) in keys(&set.runs, "metrics") {
            let vals: Vec<f64> = set
                .runs
                .iter()
                .flat_map(|r| entries(&r.value, "metrics"))
                .filter(|(k, ..)| *k == key)
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
        for (key, id, unit) in sample_keys {
            let vals: Vec<f64> = set
                .runs
                .iter()
                .flat_map(|r| entries(&r.value, "samples"))
                .filter(|(k, ..)| *k == key)
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

    fn run(label: &str, cadence: &str, first: f64, lat: Vec<f64>) -> Loaded {
        let r = Run {
            benchmark: "swipe",
            label: Some(label.to_owned()),
            folder: PathBuf::from("/x"),
            medium: Medium::Unavailable("test".to_owned()),
            params: vec![("cadence", Value::str(cadence))],
            metrics: vec![
                Metric::new("first_photo_ms", "B-01", Unit::Ms, Some(first)),
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
        Loaded {
            path: PathBuf::from(format!("{label}.json")),
            value,
        }
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
        let first = t.lines().find(|l| l.contains("first_photo_ms")).unwrap();
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
        // Debug builds are flagged (the tests run one).
        if cfg!(debug_assertions) {
            assert!(t.contains("debug build: timings not comparable"), "{t}");
        }
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
