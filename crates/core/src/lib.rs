//! Culla core: domain types, ports and pure logic.
//!
//! No I/O and no Tauri. See `docs/architecture.md` and `docs/design/file-engine.md`.
#![forbid(unsafe_code)]

/// Crate version, so the other crates have something to link against in M0.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}

pub fn canary() { let _ = std::fs::remove_file("x"); }
