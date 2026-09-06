//! Analyzes the project around a file and builds LSP `Diagnostic`s from it –
//! including the conversion from byte offsets to UTF-16 positions, as
//! required by the LSP protocol (unlike the terminal in `skema-cli`, which
//! counts characters).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range, Url};
use skema_syntax::{Severity, Span};

/// Loads the project starting at `entry` and returns the LSP diagnostics per
/// file – including files with zero messages, so old markers in the editor
/// disappear as soon as their error is fixed.
pub fn analyze(
    entry: &Path,
    read: impl FnMut(&Path) -> std::io::Result<String>,
) -> HashMap<PathBuf, Vec<Diagnostic>> {
    let project = match skema_loader::load_with(entry, read) {
        Ok(p) => p,
        Err(e) => {
            let mut by_file = HashMap::new();
            by_file.insert(
                entry.to_owned(),
                vec![Diagnostic {
                    range: Range::new(Position::new(0, 0), Position::new(0, 0)),
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: e.to_string(),
                    ..Diagnostic::default()
                }],
            );
            return by_file;
        }
    };

    let files = project.input_files();
    let (_model, hir_diagnostics) = skema_hir::analyze(&files);

    let mut by_file: HashMap<PathBuf, Vec<Diagnostic>> = HashMap::new();
    for file in &project.files {
        by_file.entry(file.path.clone()).or_default();
        for d in &file.parse_diagnostics {
            by_file
                .entry(file.path.clone())
                .or_default()
                .push(to_lsp(&file.src, d.span, d.severity, &d.message));
        }
    }
    for d in &hir_diagnostics {
        let Some(file) = project.files.get(d.file.0 as usize) else {
            continue;
        };
        by_file
            .entry(file.path.clone())
            .or_default()
            .push(to_lsp(&file.src, d.span, d.severity, &d.message));
    }

    by_file
}

pub fn file_path_url(path: &Path) -> Option<Url> {
    Url::from_file_path(path).ok()
}

fn to_lsp(src: &str, span: Span, severity: Severity, message: &str) -> Diagnostic {
    Diagnostic {
        range: Range::new(
            utf16_position(src, span.start as usize),
            utf16_position(src, span.end as usize),
        ),
        severity: Some(lsp_severity(severity)),
        message: message.to_owned(),
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

/// Byte offset → 0-based LSP `Position`. The column counts UTF-16 code
/// units, not bytes and not Unicode characters – the LSP protocol speaks
/// UTF-16 by default (see `skema_syntax::span`).
fn utf16_position(src: &str, byte_offset: usize) -> Position {
    let before = src.get(..byte_offset).unwrap_or(src);
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line = before[..line_start].matches('\n').count() as u32;
    let character = before[line_start..].encode_utf16().count() as u32;
    Position::new(line, character)
}
