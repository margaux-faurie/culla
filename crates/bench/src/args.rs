//! Command line parsing, std only.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;

use crate::cadence::{Cadence, Ramp};
use crate::folder::SourceChoice;

/// Options every folder benchmark takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Common {
    pub folder: PathBuf,
    pub include_subfolders: bool,
    pub json: bool,
    /// Free text kept in the result (medium, run number, cold or warm).
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScanArgs {
    pub common: Common,
    /// Run the EXIF pass and the capture-time sort (B-03).
    pub exif: bool,
    /// Make the first group's preview during the walk (NFR-PERF-01).
    pub first_preview: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreviewArgs {
    pub common: Common,
    pub source: SourceChoice,
    /// At most this many photos.
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SwipeArgs {
    pub common: Common,
    pub cadence: Cadence,
    pub source: SourceChoice,
    pub readers: Option<usize>,
    pub workers: Option<usize>,
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
    pub lru: Option<usize>,
    pub lru_mb: Option<u64>,
    pub steps: Option<usize>,
    pub back_percent: u8,
    pub seed: u64,
    pub ramp: Ramp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IoArgs {
    pub common: Common,
    /// Stop after this many MB (10^6 bytes).
    pub mb: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReportArgs {
    pub files: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Command {
    Help,
    Version,
    Scan(ScanArgs),
    Preview(PreviewArgs),
    Swipe(SwipeArgs),
    IoBaseline(IoArgs),
    Report(ReportArgs),
}

/// Defaults, all proposals (SPEC §10).
pub(crate) const DEFAULT_BACK_PERCENT: u8 = 10;
pub(crate) const DEFAULT_SEED: u64 = 1;
/// The largest `--lru-mb`: 64 GiB, far above any machine Culla targets, so the byte count
/// cannot overflow.
pub(crate) const MAX_LRU_MB: u64 = 64 * 1024;
pub(crate) const DEFAULT_IO_MB: u64 = 1000;

pub(crate) const HELP: &str = "\
culla-bench: the benchmark harness (docs/benchmarks.md). Read-only on the photo folder: it
writes nothing but stdout and stderr. Redirect stdout to keep a result.

usage:
  culla-bench scan <folder> [--exif] [--no-first-preview]                     B-01 to B-03
  culla-bench preview <folder> [--source jpeg|nef] [--limit N]                B-05, B-08 (miss)
  culla-bench swipe <folder> [--cadence steady|fast|burst|ramp|max]           B-09 to B-11, B-15
        [--preview-source jpeg|nef] [--readers N] [--workers N] [--ahead N] [--behind N]
        [--lru N] [--lru-mb N] [--steps N] [--back-percent P] [--seed N] [--budget-ms N]
  culla-bench io-baseline <folder> [--mb N]                                   B-16
  culla-bench report <result.json>...        medians and pooled quantiles of saved results
  culla-bench help | --version

options of every folder benchmark:
  --no-subfolders   scan the folder only
  --label TEXT      kept in the result (medium, run, cold or warm)
  --json            print the result as JSON (\"schema\": \"unstable\") instead of text

defaults (proposals, SPEC section 10): swipe --cadence steady, --preview-source jpeg,
--back-percent 10, --seed 1, --budget-ms 50, the pipeline's own defaults for the rest;
preview --source jpeg; io-baseline --mb 1000.

exit codes: 0 done, 1 usage error, 2 folder refused, 3 folder or file unreadable.
";

/// A usage error: the text says why.
pub(crate) type UsageError = String;

/// Options and positionals of one command.
struct Parsed {
    positionals: Vec<OsString>,
    flags: Vec<&'static str>,
    values: HashMap<&'static str, String>,
}

impl Parsed {
    fn flag(&self, f: &str) -> bool {
        self.flags.contains(&f)
    }

    fn value(&self, k: &str) -> Option<&str> {
        self.values.get(k).map(String::as_str)
    }

    fn number<T>(&self, k: &str, min: T) -> Result<Option<T>, UsageError>
    where
        T: std::str::FromStr + PartialOrd + std::fmt::Display + Copy,
    {
        let Some(v) = self.value(k) else {
            return Ok(None);
        };
        match v.parse::<T>() {
            Ok(n) if n >= min => Ok(Some(n)),
            _ => Err(format!(
                "{k} wants a whole number of at least {min}, not {v:?}"
            )),
        }
    }
}

fn quoted(s: &OsString) -> String {
    format!("{:?}", s.to_string_lossy())
}

fn parse_opts(
    rest: Vec<OsString>,
    flags: &[&'static str],
    values: &[&'static str],
) -> Result<Parsed, UsageError> {
    let mut p = Parsed {
        positionals: Vec::new(),
        flags: Vec::new(),
        values: HashMap::new(),
    };
    let mut done = false;
    let mut it = rest.into_iter();
    while let Some(arg) = it.next() {
        let s = arg.to_str().map(str::to_owned);
        match s.as_deref() {
            Some("--") if !done => done = true,
            Some(o) if !done && o.starts_with('-') => {
                let (name, inline) = match o.split_once('=') {
                    Some((n, v)) => (n, Some(v.to_owned())),
                    None => (o, None),
                };
                if let Some(f) = flags.iter().find(|f| **f == name) {
                    if inline.is_some() {
                        return Err(format!("{name} takes no value"));
                    }
                    if p.flags.contains(f) {
                        return Err(format!("{name} given twice"));
                    }
                    p.flags.push(f);
                } else if let Some(k) = values.iter().find(|k| **k == name) {
                    let v = match inline {
                        Some(v) => v,
                        None => it
                            .next()
                            .and_then(|v| v.into_string().ok())
                            .ok_or_else(|| format!("{name} needs a value"))?,
                    };
                    if p.values.insert(k, v).is_some() {
                        return Err(format!("{name} given twice"));
                    }
                } else {
                    return Err(format!("unknown option {}", quoted(&arg)));
                }
            }
            _ => p.positionals.push(arg),
        }
    }
    Ok(p)
}

const COMMON_FLAGS: [&str; 2] = ["--no-subfolders", "--json"];
const COMMON_VALUES: [&str; 1] = ["--label"];

fn common(p: &Parsed) -> Result<Common, UsageError> {
    let folder = match p.positionals.as_slice() {
        [] => return Err("no folder given".to_owned()),
        [f] if f.is_empty() => return Err("the folder is empty".to_owned()),
        [f] => PathBuf::from(f),
        [_, extra, ..] => return Err(format!("unexpected argument {}", quoted(extra))),
    };
    Ok(Common {
        folder,
        include_subfolders: !p.flag("--no-subfolders"),
        json: p.flag("--json"),
        label: p.value("--label").map(str::to_owned),
    })
}

fn with(base: &[&'static str], more: &[&'static str]) -> Vec<&'static str> {
    base.iter().chain(more).copied().collect()
}

fn source(p: &Parsed, key: &str) -> Result<SourceChoice, UsageError> {
    p.value(key).map_or(Ok(SourceChoice::Jpeg), |v| {
        SourceChoice::parse(v).ok_or_else(|| format!("{key} is jpeg or nef, not {v:?}"))
    })
}

/// Parses the arguments after the program name.
///
/// # Errors
///
/// A [`UsageError`] for no command, an unknown command or option, a value out of range, a
/// missing or extra argument.
pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, UsageError> {
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Err("no command given".to_owned());
    };
    let rest: Vec<OsString> = args.collect();
    if rest.iter().any(|a| a == "--help" || a == "-h") {
        return Ok(Command::Help);
    }
    match first.to_str() {
        Some("help" | "--help" | "-h") => Ok(Command::Help),
        Some("--version" | "-V" | "version") => Ok(Command::Version),
        Some("scan") => {
            let p = parse_opts(
                rest,
                &with(&COMMON_FLAGS, &["--exif", "--no-first-preview"]),
                &COMMON_VALUES,
            )?;
            Ok(Command::Scan(ScanArgs {
                common: common(&p)?,
                exif: p.flag("--exif"),
                first_preview: !p.flag("--no-first-preview"),
            }))
        }
        Some("preview") => {
            let p = parse_opts(
                rest,
                &COMMON_FLAGS,
                &with(&COMMON_VALUES, &["--source", "--limit"]),
            )?;
            Ok(Command::Preview(PreviewArgs {
                common: common(&p)?,
                source: source(&p, "--source")?,
                limit: p.number("--limit", 1)?,
            }))
        }
        Some("swipe") => {
            let p = parse_opts(
                rest,
                &COMMON_FLAGS,
                &with(
                    &COMMON_VALUES,
                    &[
                        "--cadence",
                        "--preview-source",
                        "--readers",
                        "--workers",
                        "--ahead",
                        "--behind",
                        "--lru",
                        "--lru-mb",
                        "--steps",
                        "--back-percent",
                        "--seed",
                        "--budget-ms",
                    ],
                ),
            )?;
            let cadence = p.value("--cadence").map_or(Ok(Cadence::Steady), |v| {
                Cadence::parse(v).ok_or_else(|| {
                    format!("--cadence is steady, fast, burst, ramp or max, not {v:?}")
                })
            })?;
            let back_percent = p
                .number("--back-percent", 0u8)?
                .unwrap_or(DEFAULT_BACK_PERCENT);
            if back_percent > 50 {
                return Err("--back-percent is at most 50".to_owned());
            }
            let budget = p.number("--budget-ms", 1u32)?;
            let lru_mb = p.number("--lru-mb", 1u64)?;
            if lru_mb.is_some_and(|m| m > MAX_LRU_MB) {
                return Err(format!("--lru-mb is at most {MAX_LRU_MB}"));
            }
            Ok(Command::Swipe(SwipeArgs {
                common: common(&p)?,
                cadence,
                source: source(&p, "--preview-source")?,
                readers: p.number("--readers", 1)?,
                workers: p.number("--workers", 1)?,
                ahead: p.number("--ahead", 0)?,
                behind: p.number("--behind", 0)?,
                lru: p.number("--lru", 1)?,
                lru_mb,
                steps: p.number("--steps", 1)?,
                back_percent,
                seed: p.number("--seed", 0)?.unwrap_or(DEFAULT_SEED),
                ramp: Ramp {
                    budget_ms: budget.map_or(Ramp::DEFAULT.budget_ms, f64::from),
                    ..Ramp::DEFAULT
                },
            }))
        }
        Some("io-baseline") => {
            let p = parse_opts(rest, &COMMON_FLAGS, &with(&COMMON_VALUES, &["--mb"]))?;
            Ok(Command::IoBaseline(IoArgs {
                common: common(&p)?,
                mb: p.number("--mb", 1)?.unwrap_or(DEFAULT_IO_MB),
            }))
        }
        Some("report") => {
            let p = parse_opts(rest, &[], &[])?;
            if p.positionals.is_empty() {
                return Err("no result file given".to_owned());
            }
            Ok(Command::Report(ReportArgs {
                files: p.positionals.into_iter().map(PathBuf::from).collect(),
            }))
        }
        _ => Err(format!("unknown command {}", quoted(&first))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Command, UsageError> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn scan_with_defaults_and_flags() {
        let Command::Scan(a) = p(&["scan", "E:\\DCIM"]).unwrap() else {
            panic!()
        };
        assert_eq!(a.common.folder, PathBuf::from("E:\\DCIM"));
        assert!(a.common.include_subfolders && !a.common.json && a.first_preview && !a.exif);
        let Command::Scan(a) = p(&[
            "scan",
            "--exif",
            "f",
            "--json",
            "--no-subfolders",
            "--label",
            "cold 1",
            "--no-first-preview",
        ])
        .unwrap() else {
            panic!()
        };
        assert!(a.exif && a.common.json && !a.common.include_subfolders && !a.first_preview);
        assert_eq!(a.common.label.as_deref(), Some("cold 1"));
    }

    #[test]
    fn swipe_options() {
        let Command::Swipe(a) = p(&["swipe", "f"]).unwrap() else {
            panic!()
        };
        assert_eq!(a.cadence, Cadence::Steady);
        assert_eq!(a.source, SourceChoice::Jpeg);
        assert_eq!((a.back_percent, a.seed, a.readers), (10, 1, None));
        assert!((a.ramp.budget_ms - 50.0).abs() < f64::EPSILON);
        let Command::Swipe(a) = p(&[
            "swipe",
            "f",
            "--cadence=ramp",
            "--preview-source",
            "nef",
            "--readers",
            "2",
            "--workers",
            "4",
            "--ahead",
            "6",
            "--behind",
            "0",
            "--lru",
            "50",
            "--lru-mb",
            "96",
            "--steps",
            "300",
            "--back-percent",
            "0",
            "--seed",
            "9",
            "--budget-ms",
            "40",
        ])
        .unwrap() else {
            panic!()
        };
        assert_eq!(a.cadence, Cadence::Ramp);
        assert_eq!(a.source, SourceChoice::Nef);
        assert_eq!(
            (
                a.readers, a.workers, a.ahead, a.behind, a.lru, a.lru_mb, a.steps
            ),
            (
                Some(2),
                Some(4),
                Some(6),
                Some(0),
                Some(50),
                Some(96),
                Some(300)
            )
        );
        assert_eq!((a.back_percent, a.seed), (0, 9));
        assert!((a.ramp.budget_ms - 40.0).abs() < f64::EPSILON);
    }

    #[test]
    fn other_commands() {
        let Command::Preview(a) =
            p(&["preview", "f", "--source", "nef", "--limit", "300"]).unwrap()
        else {
            panic!()
        };
        assert_eq!((a.source, a.limit), (SourceChoice::Nef, Some(300)));
        let Command::IoBaseline(a) = p(&["io-baseline", "f"]).unwrap() else {
            panic!()
        };
        assert_eq!(a.mb, 1000);
        let Command::Report(a) = p(&["report", "a.json", "b.json"]).unwrap() else {
            panic!()
        };
        assert_eq!(a.files.len(), 2);
        assert_eq!(p(&["help"]).unwrap(), Command::Help);
        assert_eq!(p(&["swipe", "f", "--help"]).unwrap(), Command::Help);
        assert_eq!(p(&["--version"]).unwrap(), Command::Version);
        // A folder named like an option, after `--`.
        let Command::Scan(a) = p(&["scan", "--", "--odd"]).unwrap() else {
            panic!()
        };
        assert_eq!(a.common.folder, PathBuf::from("--odd"));
    }

    #[test]
    fn usage_errors() {
        for (args, want) in [
            (&[][..], "no command"),
            (&["bake"][..], "unknown command"),
            (&["scan"][..], "no folder"),
            (&["scan", ""][..], "empty"),
            (&["scan", "a", "b"][..], "unexpected argument"),
            (&["scan", "a", "--exif", "--exif"][..], "twice"),
            (&["scan", "a", "--exif=1"][..], "takes no value"),
            (&["scan", "a", "--source", "nef"][..], "unknown option"),
            (&["swipe", "a", "--cadence", "slow"][..], "--cadence"),
            (&["swipe", "a", "--readers", "0"][..], "at least 1"),
            (&["swipe", "a", "--readers", "x"][..], "whole number"),
            (&["swipe", "a", "--back-percent", "60"][..], "at most 50"),
            (&["swipe", "a", "--lru-mb", "65537"][..], "at most 65536"),
            (
                &["swipe", "a", "--lru-mb", "18446744073709551615"][..],
                "at most",
            ),
            (&["swipe", "a", "--seed"][..], "needs a value"),
            (&["swipe", "a", "--lru", "1", "--lru", "2"][..], "twice"),
            (&["preview", "a", "--source", "raw"][..], "jpeg or nef"),
            (&["report"][..], "no result file"),
        ] {
            let e = p(args).unwrap_err();
            assert!(e.contains(want), "{args:?}: {e}");
        }
    }
}
