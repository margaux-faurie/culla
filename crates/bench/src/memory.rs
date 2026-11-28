//! The process's peak memory (B-15), std only.
//!
//! * Linux: `VmHWM` (peak resident set) from `/proc/self/status`.
//! * Windows: `PeakWorkingSet64` of this process, asked of Windows PowerShell's `Get-Process`
//!   once at the end of a run (a child process, so the measured process loads no extra code).
//!   PowerShell is started by its absolute path under `%SystemRoot%`, never looked up on
//!   `PATH` or in the current directory; without `SystemRoot` or that file, the peak is
//!   unavailable. The bench has no `windows-sys` dependency; the proposed long-term route is
//!   a `GetProcessMemoryInfo` lookup in `culla-adapters::winfs` (SPEC §10).
//! * Elsewhere, or when the lookup fails: unavailable, with the reason.
//!
//! The peak covers the whole process lifetime: the scan before a swipe included.

/// The peak in bytes, or why it is unavailable.
pub(crate) fn peak_bytes() -> Result<u64, String> {
    imp()
}

#[cfg(target_os = "linux")]
fn imp() -> Result<u64, String> {
    let status = std::fs::read_to_string("/proc/self/status")
        .map_err(|e| format!("cannot read /proc/self/status: {e}"))?;
    parse_vm_hwm(&status).ok_or_else(|| "no VmHWM line in /proc/self/status".to_owned())
}

#[cfg(windows)]
fn imp() -> Result<u64, String> {
    let root = std::env::var_os("SystemRoot").ok_or_else(|| "SystemRoot is not set".to_owned())?;
    let exe = powershell_path(std::path::Path::new(&root));
    if !exe.is_file() {
        return Err(format!("{} is missing", exe.display()));
    }
    let query = format!("(Get-Process -Id {}).PeakWorkingSet64", std::process::id());
    let out = std::process::Command::new(&exe)
        .args(["-NoProfile", "-NonInteractive", "-Command", query.as_str()])
        .output()
        .map_err(|e| format!("cannot run powershell.exe: {e}"))?;
    if !out.status.success() {
        return Err(format!("powershell.exe failed: {}", out.status));
    }
    parse_count(&String::from_utf8_lossy(&out.stdout))
        .ok_or_else(|| "powershell.exe did not print a byte count".to_owned())
}

#[cfg(not(any(target_os = "linux", windows)))]
fn imp() -> Result<u64, String> {
    Err("peak memory is read on Windows and Linux only".to_owned())
}

/// The `VmHWM` value of a `/proc/<pid>/status` text, in bytes.
#[cfg_attr(not(target_os = "linux"), allow(dead_code, reason = "used on Linux"))]
#[must_use]
pub(crate) fn parse_vm_hwm(status: &str) -> Option<u64> {
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let mut parts = line["VmHWM:".len()..].split_whitespace();
    let n: u64 = parts.next()?.parse().ok()?;
    match parts.next() {
        Some("kB") => n.checked_mul(1024),
        _ => None,
    }
}

/// Windows PowerShell under `system_root` (`%SystemRoot%`).
#[cfg_attr(not(windows), allow(dead_code, reason = "used on Windows"))]
#[must_use]
pub(crate) fn powershell_path(system_root: &std::path::Path) -> std::path::PathBuf {
    system_root
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe")
}

/// A plain decimal count on one line (PowerShell's output).
#[cfg_attr(not(windows), allow(dead_code, reason = "used on Windows"))]
#[must_use]
pub(crate) fn parse_count(s: &str) -> Option<u64> {
    s.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vm_hwm_in_bytes() {
        let status = "Name:\tculla-bench\nVmPeak:\t  999 kB\nVmHWM:\t   12345 kB\nVmRSS:\t 1 kB\n";
        assert_eq!(parse_vm_hwm(status), Some(12345 * 1024));
        assert_eq!(parse_vm_hwm("VmRSS:\t1 kB\n"), None);
        assert_eq!(parse_vm_hwm("VmHWM:\t12 MB\n"), None);
        assert_eq!(parse_vm_hwm("VmHWM:\tx kB\n"), None);
    }

    #[test]
    fn powershell_count() {
        assert_eq!(parse_count("123456789\r\n"), Some(123_456_789));
        assert_eq!(parse_count(""), None);
        assert_eq!(parse_count("1.5"), None);
    }

    #[test]
    fn powershell_by_absolute_path() {
        let root = std::path::Path::new("/win");
        let want: std::path::PathBuf = [
            "/win",
            "System32",
            "WindowsPowerShell",
            "v1.0",
            "powershell.exe",
        ]
        .iter()
        .collect();
        assert_eq!(powershell_path(root), want);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn reads_this_process() {
        assert!(peak_bytes().unwrap() > 0);
    }
}
