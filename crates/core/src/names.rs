//! Name validation and case folding (design/file-engine.md §3.4).
//!
//! Grouping, keys and in-plan collision checks compare names with [`fold`], which
//! approximates the NTFS upcase table. Extensions compare ASCII case-insensitively.
//! [`validate_component`] is what keeps `..`, device names and characters Windows refuses out
//! of a `RelPath` once `\\?\` turns off Win32 normalization (threat model T3).

use std::fmt;

/// Longest valid name component, in UTF-16 code units (NTFS, exFAT and FAT32 LFN).
pub const MAX_COMPONENT_UTF16: usize = 255;

/// Folds a name for comparison: each character becomes its simple uppercase when that is a
/// single character, otherwise it is kept (`ß` stays `ß`, as on NTFS).
#[must_use]
pub fn fold(s: &str) -> String {
    s.chars().map(fold_char).collect()
}

fn fold_char(c: char) -> char {
    let mut up = c.to_uppercase();
    match (up.next(), up.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

/// True when `a` and `b` are the same name under [`fold`].
#[must_use]
pub fn eq_folded(a: &str, b: &str) -> bool {
    a.chars().map(fold_char).eq(b.chars().map(fold_char))
}

/// Why a name component is not valid (design/file-engine.md §3.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameError {
    /// The empty string.
    Empty,
    /// `.` or `..`.
    DotOrDotDot,
    /// One of `\ / : * ? " < > |`.
    ReservedChar(char),
    /// A control character.
    ControlChar,
    /// Ends in a space or a dot, which Win32 would strip.
    TrailingSpaceOrDot,
    /// A DOS device name (`CON`, `NUL`, `COM1`…), with or without an extension.
    DeviceName,
    /// Longer than [`MAX_COMPONENT_UTF16`] UTF-16 code units.
    TooLong,
    /// The file system gave a name that is not valid Unicode (unpaired surrogate).
    NotUnicode,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("empty name"),
            Self::DotOrDotDot => f.write_str("name is . or .."),
            Self::ReservedChar(c) => write!(f, "name contains {c:?}"),
            Self::ControlChar => f.write_str("name contains a control character"),
            Self::TrailingSpaceOrDot => f.write_str("name ends in a space or a dot"),
            Self::DeviceName => f.write_str("name is a device name"),
            Self::TooLong => write!(f, "name is longer than {MAX_COMPONENT_UTF16} UTF-16 units"),
            Self::NotUnicode => f.write_str("name is not valid Unicode"),
        }
    }
}

impl std::error::Error for NameError {}

/// Device names Win32 reserves in every folder. `COM¹`–`COM³` and `LPT¹`–`LPT³` are
/// reserved too (Windows 11 still maps them).
const DEVICE_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "COM0", "COM1", "COM2", "COM3", "COM4",
    "COM5", "COM6", "COM7", "COM8", "COM9", "COM¹", "COM²", "COM³", "LPT0", "LPT1", "LPT2", "LPT3",
    "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9", "LPT¹", "LPT²", "LPT³",
];

/// Checks one path component against the rules of design/file-engine.md §3.4.
///
/// # Errors
///
/// The first rule the name breaks.
pub fn validate_component(name: &str) -> Result<(), NameError> {
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name == "." || name == ".." {
        return Err(NameError::DotOrDotDot);
    }
    for c in name.chars() {
        if matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            return Err(NameError::ReservedChar(c));
        }
        if c.is_control() {
            return Err(NameError::ControlChar);
        }
    }
    if name.ends_with([' ', '.']) {
        return Err(NameError::TrailingSpaceOrDot);
    }
    // Win32 maps `NUL`, `nul.txt` and `NUL .txt` to the device: compare the part before the
    // first dot, without trailing spaces.
    let base = name.split('.').next().unwrap_or(name).trim_end_matches(' ');
    if DEVICE_NAMES.iter().any(|d| eq_folded(d, base)) {
        return Err(NameError::DeviceName);
    }
    if name.encode_utf16().count() > MAX_COMPONENT_UTF16 {
        return Err(NameError::TooLong);
    }
    Ok(())
}

/// Splits a file name at its last dot: `("DSC_0001.edit", Some("JPG"))` for
/// `DSC_0001.edit.JPG`. A name without a dot, or whose only dot is the first character, has no
/// extension.
#[must_use]
pub fn split_ext(name: &str) -> (&str, Option<&str>) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], Some(&name[i + 1..])),
        _ => (name, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_uppercases_single_chars_and_keeps_expansions() {
        assert_eq!(fold("dsc_0001.nef"), "DSC_0001.NEF");
        assert_eq!(fold("été"), "ÉTÉ");
        assert_eq!(fold("straße"), "STRAßE");
        assert!(eq_folded("Dsc_0001.Jpg", "DSC_0001.JPG"));
        assert!(!eq_folded("DSC_0001", "DSC_0001 "));
    }

    #[test]
    fn valid_camera_names_pass() {
        for n in [
            "DSC_0001.JPG",
            "_DSC0001.NEF",
            "100NIKON",
            "NKSC_PARAM",
            "DSC_0001.NEF.nksc",
            "été 2026",
            "$RECYCLE.BIN",
            "CONSOLE.txt",
            "COM10",
            "x",
        ] {
            assert_eq!(validate_component(n), Ok(()), "{n}");
        }
    }

    #[test]
    fn invalid_names_are_refused() {
        let cases: &[(&str, NameError)] = &[
            ("", NameError::Empty),
            (".", NameError::DotOrDotDot),
            ("..", NameError::DotOrDotDot),
            ("a\\b", NameError::ReservedChar('\\')),
            ("a/b", NameError::ReservedChar('/')),
            ("C:x", NameError::ReservedChar(':')),
            ("a*", NameError::ReservedChar('*')),
            ("a?", NameError::ReservedChar('?')),
            ("a\"", NameError::ReservedChar('"')),
            ("a<", NameError::ReservedChar('<')),
            ("a>", NameError::ReservedChar('>')),
            ("a|", NameError::ReservedChar('|')),
            ("a\u{1}b", NameError::ControlChar),
            ("a\u{7f}", NameError::ControlChar),
            ("DSC_0001.JPG ", NameError::TrailingSpaceOrDot),
            ("DSC_0001.", NameError::TrailingSpaceOrDot),
            ("CON", NameError::DeviceName),
            ("nul.txt", NameError::DeviceName),
            ("Aux .JPG", NameError::DeviceName),
            ("com1.NEF", NameError::DeviceName),
            ("LPT9", NameError::DeviceName),
            ("COM²", NameError::DeviceName),
            ("conout$", NameError::DeviceName),
        ];
        for (n, e) in cases {
            assert_eq!(validate_component(n), Err(*e), "{n:?}");
        }
    }

    #[test]
    fn length_counts_utf16_units() {
        let ok = "a".repeat(MAX_COMPONENT_UTF16);
        assert_eq!(validate_component(&ok), Ok(()));
        let long = "a".repeat(MAX_COMPONENT_UTF16 + 1);
        assert_eq!(validate_component(&long), Err(NameError::TooLong));
        // U+1F600 is two UTF-16 units: 128 of them are 256 units.
        let emoji = "\u{1F600}".repeat(128);
        assert_eq!(validate_component(&emoji), Err(NameError::TooLong));
        assert_eq!(validate_component(&"\u{1F600}".repeat(127)), Ok(()));
    }

    #[test]
    fn split_ext_uses_the_last_dot() {
        assert_eq!(
            split_ext("DSC_0001.edit.JPG"),
            ("DSC_0001.edit", Some("JPG"))
        );
        assert_eq!(split_ext("DSC_0001.NEF.xmp"), ("DSC_0001.NEF", Some("xmp")));
        assert_eq!(split_ext("README"), ("README", None));
        assert_eq!(split_ext(".hidden"), (".hidden", None));
        assert_eq!(split_ext("a."), ("a", Some("")));
    }
}
