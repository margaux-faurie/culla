//! Culla core: domain types, ports and pure logic.
//!
//! No I/O and no Tauri. See `docs/architecture.md` and `docs/design/file-engine.md`.
#![forbid(unsafe_code)]

pub mod exif;
pub mod grouping;
pub mod ifd;
pub mod names;
pub mod volume;

/// Crate version, shown by the CLI, the bench and the app.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
