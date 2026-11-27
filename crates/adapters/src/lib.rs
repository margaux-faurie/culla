//! Culla adapters: the only crate that touches the user's files.
//!
//! The audited removal module `winfs::remove_verified` lands in M2
//! (`docs/design/file-engine.md` §9.7). Until then this crate holds no file operations; it only
//! lists folders (`scan`) and reads files (`metadata`).

pub mod metadata;
pub mod scan;

pub use culla_core::VERSION;

#[cfg(test)]
mod tests {
    #[test]
    fn links_core() {
        assert_eq!(super::VERSION, culla_core::VERSION);
    }
}
