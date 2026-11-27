//! `culla-bench`: the benchmark harness of the M1 campaign (docs/benchmarks.md, ADR 0020).
//!
//! Read-only on the photo folder: it lists and reads through `culla-adapters` and opens files
//! for reading only. It writes nothing anywhere but stdout and stderr (ADR 0020 lets the bench
//! write only scratch data in temporary directories; the M1 benchmarks need none), so a result
//! is kept by redirecting stdout to a file outside the card. The removal and rename lint stays
//! on in this crate: nothing here needs it off.
//!
//! Exit codes (as `culla`, provisional, SPEC §10): 0 done, 1 usage error, 2 the folder is
//! refused, 3 the folder or a file cannot be read.

mod args;
mod cadence;
mod folder;
mod io_baseline;
mod json;
mod memory;
mod preview;
mod report;
mod run;
mod scan;
mod stats;
mod swipe;

use std::io::{self, Write};
use std::process::ExitCode;

use args::{Command, HELP, parse};
use folder::Failure;
use run::{Run, shown};

const EXIT_OK: u8 = 0;
const EXIT_USAGE: u8 = 1;
const EXIT_REFUSED: u8 = 2;
const EXIT_IO: u8 = 3;

fn main() -> ExitCode {
    ExitCode::from(run_cli())
}

fn run_cli() -> u8 {
    let command = match parse(std::env::args_os().skip(1)) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(io::stderr(), "culla-bench: {e}\n\n{}", HELP.trim_end());
            return EXIT_USAGE;
        }
    };
    if cfg!(debug_assertions)
        && !matches!(
            command,
            Command::Help | Command::Version | Command::Report(_)
        )
    {
        let _ = writeln!(
            io::stderr(),
            "culla-bench: warning: a debug build; build with --release for timings that mean something"
        );
    }
    let (folder, json, result) = match &command {
        Command::Help => return out(HELP),
        Command::Version => return out(&format!("culla-bench {}\n", culla_core::VERSION)),
        Command::Report(a) => {
            let mut loaded = Vec::new();
            for f in &a.files {
                match report::load(f) {
                    Ok(Some(l)) => loaded.push(l),
                    Ok(None) => {
                        let _ = writeln!(
                            io::stderr(),
                            "culla-bench: {}: empty, skipped (a run that failed?)",
                            shown(&f.to_string_lossy())
                        );
                    }
                    Err(e) => {
                        let _ = writeln!(io::stderr(), "culla-bench: {e}");
                        return EXIT_IO;
                    }
                }
            }
            return out(&report::render(&loaded));
        }
        Command::Scan(a) => (&a.common.folder, a.common.json, scan::run(a)),
        Command::Preview(a) => (&a.common.folder, a.common.json, preview::run(a)),
        Command::Swipe(a) => (&a.common.folder, a.common.json, swipe::run(a)),
        Command::IoBaseline(a) => (&a.common.folder, a.common.json, io_baseline::run(a)),
    };
    let shown_folder = shown(&folder.to_string_lossy());
    match result {
        Ok(r) => print_run(&r, json),
        Err(Failure::Refused(why)) => {
            let _ = writeln!(io::stderr(), "culla-bench: {shown_folder}: {why}");
            EXIT_REFUSED
        }
        Err(Failure::Io(why)) => {
            let _ = writeln!(io::stderr(), "culla-bench: {shown_folder}: {why}");
            EXIT_IO
        }
    }
}

fn print_run(r: &Run, json: bool) -> u8 {
    if json {
        let mut s = run::to_json(r).to_pretty();
        s.push('\n');
        out(&s)
    } else {
        out(&run::render(r))
    }
}

/// Prints `s` to stdout. A closed stdout (`| head`) is not an error.
fn out(s: &str) -> u8 {
    let mut stdout = io::stdout().lock();
    match stdout.write_all(s.as_bytes()).and_then(|()| stdout.flush()) {
        Ok(()) => EXIT_OK,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => EXIT_OK,
        Err(e) => {
            let _ = writeln!(io::stderr(), "culla-bench: cannot write the output: {e}");
            EXIT_IO
        }
    }
}
