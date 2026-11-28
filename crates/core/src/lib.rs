//! Culla core: domain types, ports and pure logic.
//!
//! No I/O and no Tauri. See `docs/architecture.md` and `docs/design/file-engine.md`.
#![forbid(unsafe_code)]

pub mod applylog;
pub mod decisions;
pub mod exif;
pub mod grouping;
pub mod ifd;
pub mod journal;
pub mod names;
pub mod plan;
pub mod prefetch;
pub mod preflight;
pub mod preview;
pub mod text;
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
