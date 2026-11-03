//! What the page's code took from JavaScript and Rust spells otherwise: its
//! `trim`, its `.` in a regular expression, its string lengths. And slicing
//! that never panics: a panic's message is logged, and Rust's for a cut
//! inside a character quotes the text, which may be a value.

/// What `String.prototype.trim` takes away: JavaScript's white space and line
/// terminators, BOM included, U+0085 not.
pub fn is_space(c: char) -> bool {
    matches!(
        c,
        '\u{9}'..='\u{d}'
            | ' '
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}

pub fn trim(text: &str) -> &str {
    text.trim_matches(is_space)
}

/// What `.` in a regular expression does not match.
pub fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `text.length`: UTF-16 code units.
pub fn length(text: &str) -> usize {
    text.encode_utf16().count()
}

/// `text.slice(from, to)` by bytes, or nothing when a bound is not a
/// character's: the parsers only cut where they found an ASCII byte, so the
/// empty answer never happens, and a mistake never becomes a panic.
pub fn cut(text: &str, from: usize, to: usize) -> &str {
    text.get(from..to.min(text.len())).unwrap_or("")
}

/// `text.slice(from)` by bytes, as [`cut`].
pub fn from(text: &str, at: usize) -> &str {
    text.get(at.min(text.len())..).unwrap_or("")
}

/// The first `n` characters (`Array.from(text).slice(0, n)`).
pub fn first_chars(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

/// After the last `/`.
pub fn base_name(path: &str) -> &str {
    match path.rfind('/') {
        Some(slash) => from(path, slash + 1),
        None => path,
    }
}

/// Up to and with the last `/`; empty at the root.
pub fn dir_name(path: &str) -> &str {
    match path.rfind('/') {
        Some(slash) => cut(path, 0, slash + 1),
        None => "",
    }
}

/// Where a 1-based line and column are, in UTF-16 units as the page counted
/// them, of byte `at` of `text`.
pub fn line_of(text: &[u8], at: usize) -> (usize, usize) {
    let limit = at.min(text.len());
    let mut line = 1;
    let mut start = 0;
    for (index, byte) in text[..limit].iter().enumerate() {
        if *byte == b'\n' {
            line += 1;
            start = index + 1;
        }
    }
    let units: usize = text[start..limit]
        .iter()
        .map(|byte| match byte {
            0x80..=0xbf => 0, // inside a character
            0xf0..=0xff => 2, // a character outside the BMP: a surrogate pair
            _ => 1,
        })
        .sum();
    (line, units + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_as_javascript_does() {
        assert_eq!(trim("\u{feff} a \u{a0}\n"), "a");
        assert_eq!(trim("\u{85}a"), "\u{85}a");
    }

    #[test]
    fn cuts_never_panic() {
        assert_eq!(cut("é", 0, 1), "");
        assert_eq!(cut("abc", 1, 9), "bc");
        assert_eq!(from("abc", 9), "");
    }

    #[test]
    fn lines_and_columns_in_utf16_units() {
        assert_eq!(line_of(b"ab\ncd", 4), (2, 2));
        assert_eq!(line_of("é😀x".as_bytes(), 6), (1, 4));
    }
}
