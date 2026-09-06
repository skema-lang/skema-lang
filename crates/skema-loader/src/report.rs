//! Byte offset → line/column, for humans and for tools.
//!
//! The core (`skema-syntax`, `skema-hir`) works exclusively with byte
//! offsets (see `skema_syntax::span`); the conversion deliberately happens
//! only here, at the boundary to the consumers (`skema-cli`, `skema-py`),
//! which each build their own notion of "message" from it.

use std::path::PathBuf;

use skema_syntax::Severity;

use crate::Project;

#[derive(Debug)]
pub struct ReportEntry {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub severity: Severity,
    pub message: String,
}

/// Byte offset → 1-based line and column. The column counts characters, not
/// bytes, otherwise the caret would point to the wrong place on non-ASCII
/// characters.
fn line_col(src: &str, offset: usize) -> (usize, usize) {
    let before = src.get(..offset).unwrap_or(src);
    let line = before.matches('\n').count() + 1;
    let column = match before.rfind('\n') {
        Some(i) => before[i + 1..].chars().count() + 1,
        None => before.chars().count() + 1,
    };
    (line, column)
}

/// Combines parser and analysis messages into a single list, sorted by
/// file, line, and column – the reading order in a terminal.
pub fn build(project: &Project, hir_diagnostics: &[skema_hir::Diagnostic]) -> Vec<ReportEntry> {
    let mut reports = Vec::new();

    for file in &project.files {
        for d in &file.parse_diagnostics {
            let (line, column) = line_col(&file.src, d.span.start as usize);
            reports.push(ReportEntry {
                path: file.path.clone(),
                line,
                column,
                severity: d.severity,
                message: d.message.clone(),
            });
        }
    }

    for d in hir_diagnostics {
        let Some(file) = project.files.get(d.file.0 as usize) else {
            continue;
        };
        let (line, column) = line_col(&file.src, d.span.start as usize);
        reports.push(ReportEntry {
            path: file.path.clone(),
            line,
            column,
            severity: d.severity,
            message: d.message.clone(),
        });
    }

    reports.sort_by(|a, b| (&a.path, a.line, a.column).cmp(&(&b.path, b.line, b.column)));
    reports
}
