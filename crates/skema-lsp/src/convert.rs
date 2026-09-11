//! The three coordinate systems this server sits between, and nothing else.
//!
//! * `skema-syntax` speaks **byte offsets** ("positions – byte offsets, and
//!   nothing else").
//! * `skema-api` speaks **UTF-16 offsets**, because that is what an editor
//!   edits in.
//! * LSP speaks **(line, UTF-16 character)** pairs.
//!
//! Every conversion between them happens here, so a wrong one is a bug in
//! one file rather than a class of bugs spread over the features.

use std::path::{Path, PathBuf};

use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range, Url};
use skema_syntax::{Severity, Span};

/// Byte offset → LSP position. The column counts UTF-16 code units — the
/// protocol's default encoding — not bytes and not characters.
///
/// The counting is `skema_api::location`'s, which is 1-based for everything
/// human-facing. LSP is the one 0-based consumer, so this subtracts instead
/// of recounting: two implementations of "which line and column is this
/// byte in" agree until they don't.
pub fn position_of(src: &str, byte_offset: usize) -> Position {
    let (line, column) = skema_api::location::utf16_line_col(src, byte_offset as u32);
    Position::new(line - 1, column - 1)
}

/// LSP position → byte offset, saturating at the end of the line and of the
/// document: an editor may ask about a position one past the last character
/// (the caret at end of file), and that is not an error.
pub fn offset_of(src: &str, position: Position) -> usize {
    let mut line_start = 0;
    for _ in 0..position.line {
        match src[line_start..].find('\n') {
            Some(i) => line_start += i + 1,
            None => return src.len(),
        }
    }

    // Walk the line in UTF-16 units, because that is what `character` counts.
    let line = &src[line_start..];
    let mut units = 0;
    for (offset, ch) in line.char_indices() {
        if units >= position.character as usize || ch == '\n' {
            return line_start + offset;
        }
        units += ch.len_utf16();
    }
    src.len()
}

pub fn range_of(src: &str, span: Span) -> Range {
    Range::new(
        position_of(src, span.start as usize),
        position_of(src, span.end as usize),
    )
}

pub fn url_of(path: &Path) -> Option<Url> {
    Url::from_file_path(path).ok()
}

/// A `file:` URL → a path in the same shape `skema-loader` interns them in,
/// so the two can be compared at all.
///
/// A file that has never been saved does not exist on disk; the
/// uncanonicalized path is then the best available identifier, and losing
/// it would mean losing the buffer's diagnostics entirely.
pub fn path_of(uri: &Url) -> Option<PathBuf> {
    let path = uri.to_file_path().ok()?;
    Some(path.canonicalize().unwrap_or(path))
}

pub fn diagnostic(src: &str, span: Span, severity: Severity, message: &str) -> Diagnostic {
    Diagnostic {
        range: range_of(src, span),
        severity: Some(lsp_severity(severity)),
        message: message.to_owned(),
        source: Some("skema".to_owned()),
        ..Diagnostic::default()
    }
}

fn lsp_severity(severity: Severity) -> DiagnosticSeverity {
    match severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Info => DiagnosticSeverity::INFORMATION,
        Severity::Hint => DiagnosticSeverity::HINT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIXED: &str = "Table Ä {\n  note = \"🙂\";\n}\n";

    #[test]
    fn positions_count_utf16_units_not_bytes() {
        // `Ä` is two bytes but one UTF-16 unit.
        let byte_offset = MIXED.find('Ä').expect("Ä") + 'Ä'.len_utf8();
        assert_eq!(position_of(MIXED, byte_offset), Position::new(0, 7));
    }

    #[test]
    fn offsets_and_positions_are_inverse() {
        for (line, character) in [(0, 0), (0, 7), (1, 2), (2, 1)] {
            let position = Position::new(line, character);
            let offset = offset_of(MIXED, position);
            assert_eq!(position_of(MIXED, offset), position, "at {position:?}");
        }
    }

    #[test]
    fn a_position_past_the_end_saturates_instead_of_panicking() {
        assert_eq!(offset_of(MIXED, Position::new(99, 0)), MIXED.len());
        // Clamped to the end of line 0 - a byte offset, so the two-byte `Ä`
        // makes it one past the nine UTF-16 units that line contains.
        assert_eq!(
            offset_of(MIXED, Position::new(0, 999)),
            MIXED.find('\n').expect("a line break")
        );
    }
}
