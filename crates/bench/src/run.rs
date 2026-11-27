//! The result of one benchmark run, printed as text or as JSON (`"schema": "unstable"`).
//!
//! The JSON leaves out the folder path and file names (they can hold a user name or a place);
//! the text shows the folder so a run can be told apart in the terminal.

use std::fmt::Write as _;
use std::path::PathBuf;

use culla_core::volume::{DriveKind, VolumeInfo};

use crate::json::Value;
use crate::stats::Summary;

/// The unit of a metric.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
    Ms,
    Count,
    Bytes,
    MbPerS,
    PerS,
    Percent,
    /// Kilobytes (10^3 bytes), for per-item read sizes.
    Kb,
}

impl Unit {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Ms => "ms",
            Self::Count => "count",
            Self::Bytes => "bytes",
            Self::MbPerS => "MB/s",
            Self::PerS => "per_s",
            Self::Percent => "percent",
            Self::Kb => "kB",
        }
    }
}

/// One measured value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Metric {
    /// Stable key in the JSON, e.g. `first_group_ms`.
    pub key: &'static str,
    /// The benchmark id of benchmarks.md (`B-01`), or empty.
    pub id: &'static str,
    pub unit: Unit,
    /// `None`: not measured (with a note saying why).
    pub value: Option<f64>,
}

impl Metric {
    pub(crate) fn new(key: &'static str, id: &'static str, unit: Unit, value: Option<f64>) -> Self {
        Self {
            key,
            id,
            unit,
            value,
        }
    }

    #[allow(clippy::cast_precision_loss, reason = "counts far below 2^53")]
    pub(crate) fn count(key: &'static str, n: u64) -> Self {
        Self::new(key, "", Unit::Count, Some(n as f64))
    }
}

/// Per-item values, summarised as p50, p95, p99 and max.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Samples {
    pub key: &'static str,
    pub id: &'static str,
    pub unit: Unit,
    pub values: Vec<f64>,
}

/// The volume the folder is on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Medium {
    Known {
        label: Option<String>,
        file_system: String,
        drive_kind: &'static str,
        mount_point: String,
    },
    /// Not looked up (off Windows), with why.
    Unavailable(String),
}

impl Medium {
    pub(crate) fn of(v: &VolumeInfo) -> Self {
        Self::Known {
            label: v.label.clone(),
            file_system: v.fs_name.clone(),
            drive_kind: drive_kind(v.drive_kind),
            mount_point: v.mount_point.clone(),
        }
    }
}

fn drive_kind(k: DriveKind) -> &'static str {
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

/// One run.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Run {
    /// `scan`, `preview`, `swipe` or `io-baseline`.
    pub benchmark: &'static str,
    pub label: Option<String>,
    /// Text output only.
    pub folder: PathBuf,
    pub medium: Medium,
    /// What the run was asked to do; runs with equal params are summarised together by
    /// `report`.
    pub params: Vec<(&'static str, Value)>,
    pub metrics: Vec<Metric>,
    pub samples: Vec<Samples>,
    pub notes: Vec<String>,
}

/// The build profile: timings of a debug build mean nothing.
#[must_use]
pub(crate) fn profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

fn logical_cpus() -> u64 {
    std::thread::available_parallelism().map_or(0, |n| n.get() as u64)
}

/// The run as one JSON object.
#[must_use]
pub(crate) fn to_json(r: &Run) -> Value {
    let medium = match &r.medium {
        Medium::Known {
            label,
            file_system,
            drive_kind,
            mount_point,
        } => Value::obj([
            ("label", Value::opt(label.clone(), Value::Str)),
            ("file_system", Value::str(file_system.clone())),
            ("drive_kind", Value::str(*drive_kind)),
            ("mount_point", Value::str(mount_point.clone())),
        ]),
        Medium::Unavailable(why) => Value::obj([("unavailable", Value::str(why.clone()))]),
    };
    let metrics = r
        .metrics
        .iter()
        .map(|m| {
            Value::obj([
                ("key", Value::str(m.key)),
                ("id", Value::str(m.id)),
                ("unit", Value::str(m.unit.name())),
                ("value", Value::opt(m.value, Value::Num)),
            ])
        })
        .collect();
    let samples = r
        .samples
        .iter()
        .map(|s| {
            Value::obj([
                ("key", Value::str(s.key)),
                ("id", Value::str(s.id)),
                ("unit", Value::str(s.unit.name())),
                (
                    "values",
                    Value::Arr(s.values.iter().map(|v| Value::Num(*v)).collect()),
                ),
            ])
        })
        .collect();
    Value::obj([
        ("schema", Value::str("unstable")),
        ("tool", Value::str("culla-bench")),
        ("version", Value::str(culla_core::VERSION)),
        ("benchmark", Value::str(r.benchmark)),
        ("label", Value::opt(r.label.clone(), Value::Str)),
        ("build", Value::obj([("profile", Value::str(profile()))])),
        (
            "machine",
            Value::obj([
                ("os", Value::str(std::env::consts::OS)),
                ("arch", Value::str(std::env::consts::ARCH)),
                ("logical_cpus", Value::int(logical_cpus())),
            ]),
        ),
        ("medium", medium),
        ("params", Value::obj(r.params.iter().cloned())),
        ("metrics", Value::Arr(metrics)),
        ("samples", Value::Arr(samples)),
        (
            "notes",
            Value::Arr(r.notes.iter().map(|n| Value::str(n.clone())).collect()),
        ),
    ])
}

/// A value for the text output.
#[must_use]
pub(crate) fn fmt_value(v: Option<f64>, unit: Unit) -> String {
    let Some(v) = v else {
        return "n/a".to_owned();
    };
    match unit {
        Unit::Ms => format!("{v:.1} ms"),
        Unit::Count => format!("{v:.0}"),
        Unit::Bytes => format!("{:.1} MB", v / 1_000_000.0),
        Unit::MbPerS => format!("{v:.1} MB/s"),
        Unit::PerS => format!("{v:.2} per second"),
        Unit::Percent => format!("{v:.1} %"),
        Unit::Kb => format!("{v:.1} kB"),
    }
}

/// Printable text: control and invisible characters as `\u{..}` (threat model T6b).
#[must_use]
pub(crate) fn shown(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_control()
            || matches!(c, '\u{00AD}' | '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}'
                | '\u{2028}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FEFF}'
                | '\u{FFF9}'..='\u{FFFB}' | '\u{E0000}'..='\u{E007F}')
        {
            let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
        } else {
            out.push(c);
        }
    }
    out
}

/// The run as text, ending with a newline.
#[must_use]
pub(crate) fn render(r: &Run) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "benchmark: {}", r.benchmark);
    if let Some(l) = &r.label {
        let _ = writeln!(o, "label:     {}", shown(l));
    }
    let _ = writeln!(o, "folder:    {}", shown(&r.folder.to_string_lossy()));
    match &r.medium {
        Medium::Known {
            label,
            file_system,
            drive_kind,
            mount_point,
        } => {
            let _ = writeln!(
                o,
                "medium:    {} ({}, {drive_kind}, label {})",
                shown(mount_point),
                shown(file_system),
                label.as_deref().map_or_else(|| "none".to_owned(), shown),
            );
        }
        Medium::Unavailable(why) => {
            let _ = writeln!(o, "medium:    unavailable ({why})");
        }
    }
    let _ = writeln!(
        o,
        "build:     {} on {} {}, {} logical CPUs, culla {}",
        profile(),
        std::env::consts::OS,
        std::env::consts::ARCH,
        logical_cpus(),
        culla_core::VERSION
    );
    if !r.params.is_empty() {
        let p: Vec<String> = r
            .params
            .iter()
            .map(|(k, v)| format!("{k} {}", v.to_compact().trim_matches('"')))
            .collect();
        let _ = writeln!(o, "params:    {}", p.join(", "));
    }
    let _ = writeln!(o);
    for m in &r.metrics {
        let id = if m.id.is_empty() { "" } else { m.id };
        let _ = writeln!(o, "{id:<5} {:<28} {}", m.key, fmt_value(m.value, m.unit));
    }
    if !r.samples.is_empty() {
        let _ = writeln!(o);
        let _ = writeln!(
            o,
            "{:<5} {:<28} {:>6} {:>9} {:>9} {:>9} {:>9}  unit",
            "", "per item", "n", "p50", "p95", "p99", "max"
        );
        for s in &r.samples {
            match Summary::of(&s.values) {
                Some(x) => {
                    let _ = writeln!(
                        o,
                        "{:<5} {:<28} {:>6} {:>9.1} {:>9.1} {:>9.1} {:>9.1}  {}",
                        s.id,
                        s.key,
                        x.n,
                        x.p50,
                        x.p95,
                        x.p99,
                        x.max,
                        s.unit.name()
                    );
                }
                None => {
                    let _ = writeln!(o, "{:<5} {:<28} {:>6}", s.id, s.key, 0);
                }
            }
        }
    }
    if !r.notes.is_empty() {
        let _ = writeln!(o);
        for n in &r.notes {
            let _ = writeln!(o, "note: {}", shown(n));
        }
    }
    let _ = writeln!(o);
    let _ = writeln!(o, "written: nothing (read-only)");
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;

    fn sample_run() -> Run {
        Run {
            benchmark: "scan",
            label: Some("cold 1 \u{202e}".to_owned()),
            folder: PathBuf::from("/media/card/DCIM"),
            medium: Medium::Unavailable("off Windows".to_owned()),
            params: vec![("exif", Value::Bool(true))],
            metrics: vec![
                Metric::new("first_group_ms", "B-01", Unit::Ms, Some(12.25)),
                Metric::new("peak_memory_bytes", "B-15", Unit::Bytes, None),
                Metric::count("groups", 3),
            ],
            samples: vec![Samples {
                key: "exif_ms",
                id: "B-03",
                unit: Unit::Ms,
                values: vec![1.0, 2.0, 3.0],
            }],
            notes: vec!["timings with stubs".to_owned()],
        }
    }

    #[test]
    fn json_has_the_schema_and_no_folder() {
        let r = sample_run();
        let text = to_json(&r).to_pretty();
        assert!(text.is_ascii());
        assert!(!text.contains("/media/card"), "{text}");
        let v = json::parse(&text).unwrap();
        assert_eq!(v.get("schema").and_then(Value::as_str), Some("unstable"));
        assert_eq!(v.get("benchmark").and_then(Value::as_str), Some("scan"));
        assert_eq!(
            v.get("label").and_then(Value::as_str),
            Some("cold 1 \u{202e}")
        );
        let m = v.get("metrics").and_then(Value::as_arr).unwrap();
        assert_eq!(m.len(), 3);
        assert_eq!(m[0].get("value").and_then(Value::as_f64), Some(12.25));
        assert_eq!(m[1].get("value"), Some(&Value::Null));
        let s = v.get("samples").and_then(Value::as_arr).unwrap();
        assert_eq!(s[0].get("values").and_then(Value::as_arr).unwrap().len(), 3);
    }

    #[test]
    fn text_shows_metrics_and_escapes() {
        let t = render(&sample_run());
        assert!(t.contains("B-01  first_group_ms"), "{t}");
        assert!(t.contains("12.2 ms") || t.contains("12.3 ms"), "{t}");
        assert!(t.contains("peak_memory_bytes"), "{t}");
        assert!(t.contains("n/a"), "{t}");
        assert!(t.contains("cold 1 \\u{202e}"), "{t}");
        assert!(t.contains("exif_ms"), "{t}");
        assert!(t.ends_with("written: nothing (read-only)\n"), "{t}");
    }

    #[test]
    fn values_in_their_units() {
        assert_eq!(fmt_value(Some(1.25), Unit::Ms), "1.2 ms");
        assert_eq!(fmt_value(Some(3.0), Unit::Count), "3");
        assert_eq!(fmt_value(Some(2_500_000.0), Unit::Bytes), "2.5 MB");
        assert_eq!(fmt_value(Some(90.0), Unit::MbPerS), "90.0 MB/s");
        assert_eq!(fmt_value(Some(3.5), Unit::PerS), "3.50 per second");
        assert_eq!(fmt_value(Some(12.5), Unit::Percent), "12.5 %");
        assert_eq!(fmt_value(None, Unit::Ms), "n/a");
    }
}
