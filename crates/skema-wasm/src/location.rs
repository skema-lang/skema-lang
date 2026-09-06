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
}
