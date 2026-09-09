//! Byte offset → human position, for the browser.
//!
//! Deliberately **not** a reuse of `skema-loader::report`'s `line_col`: that
//! one counts Unicode scalar values (chars), which is right for a terminal
//! but wrong here. JavaScript strings – and therefore editor column
//! positions in something like CodeMirror or Monaco – are UTF-16 code
//! units, so a character outside the Basic Multilingual Plane (an emoji,
//! say) would silently disagree by one between the two counting schemes.
//! `str::encode_utf16` already yields one `u16` per code unit, so it
//! naturally counts such a character as the 2 UTF-16 units (a surrogate
//! pair) that JavaScript itself would see.
//!
//! Line and column are 1-based, matching the existing convention in
//! `skema-loader`/`skema-cli`/`skema-py` (only the counting *method*
//! differs, not the numbering *base*) – `skema-lsp` is the one legitimate
//! 0-based exception, mandated by the LSP protocol itself.

use serde::Serialize;
use tsify_next::Tsify;

#[derive(Debug, Clone, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

fn utf16_line_col(src: &str, byte_offset: u32) -> (u32, u32) {
    let before = src.get(..byte_offset as usize).unwrap_or(src);
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line = before[..line_start].matches('\n').count() as u32 + 1;
    let column = before[line_start..].encode_utf16().count() as u32 + 1;
    (line, column)
}

pub fn locate(src: &str, filename: &str, span: skema_syntax::Span) -> Location {
    let (line, column) = utf16_line_col(src, span.start);
    let (end_line, end_column) = utf16_line_col(src, span.end);
    Location {
        file: filename.to_owned(),
        line,
        column,
        end_line,
        end_column,
    }
}

/// Fast byte-offset → flat UTF-16-offset conversion for **many** spans
/// against the same source text (a CM6-style editor position is one flat
/// offset, not a line/column pair).
///
/// [`utf16_line_col`] above recomputes `encode_utf16().count()` from byte 0
/// on every call – fine for the handful of spans in a diagnostics list, too
/// slow for the hundreds of token spans a highlighter re-touches on every
/// keystroke. This builds one sparse breakpoint table per source string in
/// a single pass, then converts each span endpoint in `O(log k)`, where `k`
/// is the number of multi-byte characters – zero, i.e. an identity mapping,
/// for the plain-ASCII source this DSL mostly consists of.
pub struct Utf16Offsets {
    /// `(byte_offset, cumulative_delta)`, sorted by `byte_offset`: from this
    /// byte offset onward (inclusive), a UTF-16 offset is the byte offset
    /// plus `cumulative_delta`. Only multi-byte characters (where UTF-8 and
    /// UTF-16 lengths differ) create an entry.
    breakpoints: Vec<(u32, i64)>,
}

impl Utf16Offsets {
    pub fn new(src: &str) -> Self {
        let mut breakpoints = Vec::new();
        let mut delta: i64 = 0;
        for (byte_start, ch) in src.char_indices() {
            let byte_len = i64::from(ch.len_utf8() as u32);
            let utf16_len = i64::from(ch.len_utf16() as u32);
            if byte_len != utf16_len {
                delta += utf16_len - byte_len;
                // The breakpoint sits at the character's *end*: everything
                // up to and including this character still counts at the
                // *previous* delta, since the shrinkage/growth is only
                // fully "behind" a query offset once the character itself
                // has been passed.
                breakpoints.push((byte_start as u32 + ch.len_utf8() as u32, delta));
            }
        }
        Self { breakpoints }
    }

    pub fn convert(&self, byte_offset: u32) -> u32 {
        let i = self.breakpoints.partition_point(|&(b, _)| b <= byte_offset);
        let delta = if i == 0 { 0 } else { self.breakpoints[i - 1].1 };
        (i64::from(byte_offset) + delta) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_positions() {
        let src = "line one\nline two\nline three";
        assert_eq!(utf16_line_col(src, 0), (1, 1));
        assert_eq!(utf16_line_col(src, 9), (2, 1));
        assert_eq!(utf16_line_col(src, 9 + 5), (2, 6));
    }

    #[test]
    fn astral_characters_count_as_two_utf16_units() {
        // "😀" (U+1F600) is outside the BMP: 4 bytes in UTF-8, a surrogate
        // pair (2 code units) in UTF-16 – exactly what a JS string sees.
        let src = "😀x";
        let x_byte_offset = "😀".len() as u32;
        assert_eq!(utf16_line_col(src, x_byte_offset), (1, 3));
    }

    #[test]
    fn utf16_offsets_identity_for_ascii() {
        let table = Utf16Offsets::new("plain ascii text");
        for b in 0..17u32 {
            assert_eq!(table.convert(b), b);
        }
    }

    #[test]
    fn utf16_offsets_shrink_across_a_multibyte_character() {
        // "é x": é is 2 UTF-8 bytes / 1 UTF-16 unit, so every offset from
        // right after é onward is exactly 1 less in UTF-16 than in UTF-8.
        let src = "é x";
        let table = Utf16Offsets::new(src);
        assert_eq!(table.convert(0), 0, "before é: unaffected");
        assert_eq!(table.convert(2), 1, "right after é: shrunk by 1");
        assert_eq!(table.convert(3), 2, "at 'x': still shrunk by 1");
        assert_eq!(table.convert(src.len() as u32), 3, "end of string");
    }

    #[test]
    fn utf16_offsets_grow_across_an_astral_character() {
        // "😀x": 4 UTF-8 bytes / 2 UTF-16 units (a surrogate pair) – every
        // offset from right after it onward is 2 *more* in UTF-16.
        let src = "😀x";
        let table = Utf16Offsets::new(src);
        assert_eq!(table.convert(0), 0);
        assert_eq!(table.convert(4), 2);
    }
}
