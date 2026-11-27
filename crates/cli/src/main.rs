//! `culla`: headless Culla (ADR 0020). Wiring only: every read goes through
//! `culla-adapters`, every rule is in `culla-core`. Nothing is written anywhere but stdout and
//! stderr, except the previews `culla preview --out <dir>` creates in a new or empty folder
//! outside the scanned one; `plan`, `apply` and `resume` come with M2.
//!
//! Exit codes (provisional, SPEC §10): 0 done, 1 usage error, 2 the folder is refused, 3 the
//! folder cannot be read, 4 the output folder is refused or cannot be written.

mod args;
mod json;
mod labels;
mod preview;
mod scan;
mod text;

use std::io::{self, Write};
use std::process::ExitCode;

use args::{Command, help, parse};
use scan::{Failure, Progress};

/// Exit code: done.
const EXIT_OK: u8 = 0;
/// Exit code: the command line is not valid.
const EXIT_USAGE: u8 = 1;
/// Exit code: the folder is refused (link, placeholder, not a folder, too large, unsupported
/// path or volume).
const EXIT_REFUSED: u8 = 2;
/// Exit code: the folder cannot be read.
const EXIT_IO: u8 = 3;
/// Exit code: the output folder of `culla preview --out` is refused or cannot be written.
const EXIT_OUT: u8 = 4;

fn main() -> ExitCode {
    ExitCode::from(run())
}

fn run() -> u8 {
    let command = match parse(std::env::args_os().skip(1)) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(io::stderr(), "culla: {e}\n\n{}", help(e.topic).trim_end());
            return EXIT_USAGE;
        }
    };
    match command {
        Command::Help(topic) => out(help(topic)),
        Command::Version => out(&format!("culla {}\n", culla_adapters::VERSION)),
        Command::Scan(a) => {
            let shown = text::shown(&a.folder.to_string_lossy()).into_owned();
            match scan::run(&a, &mut Progress::stderr()) {
                Ok(report) => {
                    if a.json {
                        let mut s = scan::to_json(&report).to_pretty();
                        s.push('\n');
                        out(&s)
                    } else {
                        out(&text::render(&report))
                    }
                }
                Err(Failure::Refused(why)) => {
                    let _ = writeln!(io::stderr(), "culla: {shown}: {why}");
                    EXIT_REFUSED
                }
                Err(Failure::Io(why)) => {
                    let _ = writeln!(io::stderr(), "culla: {shown}: {why}");
                    EXIT_IO
                }
            }
        }
        Command::Preview(a) => {
            let shown = text::shown(&a.folder.to_string_lossy()).into_owned();
            match preview::run(&a, &mut Progress::stderr()) {
                Ok(report) => out(&preview::render(&report)),
                Err(preview::PreviewFailure::Scan(Failure::Refused(why))) => {
                    let _ = writeln!(io::stderr(), "culla: {shown}: {why}");
                    EXIT_REFUSED
                }
                Err(preview::PreviewFailure::Scan(Failure::Io(why))) => {
                    let _ = writeln!(io::stderr(), "culla: {shown}: {why}");
                    EXIT_IO
                }
                Err(preview::PreviewFailure::Out(why)) => {
                    let dir = a.out.as_deref().map_or_else(String::new, |d| {
                        text::shown(&d.to_string_lossy()).into_owned()
                    });
                    let _ = writeln!(io::stderr(), "culla: {dir}: {why}");
                    EXIT_OUT
                }
            }
        }
    }
}

/// Prints `s` to stdout. A closed stdout (`culla scan … | head`) is not an error of the scan.
fn out(s: &str) -> u8 {
    let mut stdout = io::stdout().lock();
    match stdout.write_all(s.as_bytes()).and_then(|()| stdout.flush()) {
        Ok(()) => EXIT_OK,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => EXIT_OK,
        Err(e) => {
            let _ = writeln!(io::stderr(), "culla: cannot write the output: {e}");
            EXIT_IO
        }
    }
}
