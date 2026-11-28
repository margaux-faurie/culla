//! Escaping for untrusted text shown to a person: file names, volume labels and EXIF strings
//! come from the card, so they may carry terminal escape sequences, bidi overrides or invisible
//! characters that change what a terminal or a text editor shows (threat model T6b). The CLI's
//! text output and the apply log ([`crate::applylog`]) escape the same set.

use std::borrow::Cow;
use std::fmt::Write;

/// Whether `c` is escaped by [`shown`]: control characters, the soft hyphen, the Arabic letter
/// mark and the Mongolian vowel separator, zero-width characters, bidi controls and isolates,
/// the line and paragraph separators, word joiners and invisible operators, the BOM,
/// interlinear annotation controls and the tag characters. Not escaped: variation selectors and
/// Hangul fillers.
#[must_use]
pub fn needs_escape(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{00AD}'
                | '\u{061C}'
                | '\u{180E}'
                | '\u{200B}'..='\u{200F}'
                | '\u{2028}'..='\u{202E}'
                | '\u{2060}'..='\u{206F}'
                | '\u{FEFF}'
                | '\u{FFF9}'..='\u{FFFB}'
                | '\u{E0000}'..='\u{E007F}'
        )
}

/// `s` with every character of [`needs_escape`] written as `\u{..}` (lowercase hex). Borrowed
/// when nothing needs escaping.
#[must_use]
pub fn shown(s: &str) -> Cow<'_, str> {
    escaped(s, |_| false)
}

/// `s` with every character of [`needs_escape`], and every character `also` selects, written as
/// `\u{..}`. The apply log uses it to escape the quote that delimits a path, and the path
/// separator inside a name, so a name can neither end its quotes nor fake a folder.
#[must_use]
pub fn escaped(s: &str, also: impl Fn(char) -> bool) -> Cow<'_, str> {
    if !s.chars().any(|c| needs_escape(c) || also(c)) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        if needs_escape(c) || also(c) {
            let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_controls_are_escaped() {
        assert!(matches!(shown("DSC_0001.JPG"), Cow::Borrowed(_)));
        assert_eq!(shown("é 📷 ok"), "é 📷 ok");
        assert_eq!(shown("a\u{1b}[31mb"), "a\\u{1b}[31mb");
        assert_eq!(shown("x\r\ny\t"), "x\\u{d}\\u{a}y\\u{9}");
        assert_eq!(shown("\u{9b}"), "\\u{9b}");
        assert_eq!(shown("gpj.\u{202E}EXE"), "gpj.\\u{202e}EXE");
        assert_eq!(
            shown("a\u{200B}\u{2066}\u{FEFF}\u{2028}"),
            "a\\u{200b}\\u{2066}\\u{feff}\\u{2028}"
        );
    }

    #[test]
    fn invisible_format_characters_are_escaped() {
        assert_eq!(
            shown("a\u{AD}b\u{61C}c\u{180E}d"),
            "a\\u{ad}b\\u{61c}c\\u{180e}d"
        );
        assert_eq!(
            shown("x\u{E0000}\u{E0041}\u{E007F}y"),
            "x\\u{e0000}\\u{e0041}\\u{e007f}y"
        );
        for c in ['\u{AD}', '\u{61C}', '\u{180E}', '\u{E0000}', '\u{E007F}'] {
            assert!(needs_escape(c), "{c:?}");
        }
        for c in ['a', 'é', '\u{AC}', '\u{AE}', '\u{E0080}', '\u{DFFFF}', '📷'] {
            assert!(!needs_escape(c), "{c:?}");
        }
    }

    #[test]
    fn extra_characters_are_escaped_too() {
        assert_eq!(escaped("a\"b\\c", |c| c == '"'), "a\\u{22}b\\c");
        assert_eq!(
            escaped("a\"b\\c\n", |c| matches!(c, '"' | '\\')),
            "a\\u{22}b\\u{5c}c\\u{a}"
        );
        assert!(matches!(escaped("plain", |c| c == '"'), Cow::Borrowed(_)));
    }
}
