//! Terminal representation of a message: `path:line:column: severity: text`.
//!
//! The actual byte-offset conversion lives in `skema_loader::report`, shared
//! with `skema-py`. This module only adds the text representation a CLI
//! needs (lowercase severity, `Display`, JSON serialization).

use std::fmt;

use serde::Serialize;
use skema_loader::Project;
use skema_syntax::Severity;

#[derive(Serialize)]
pub struct Report {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub severity: &'static str,
    pub message: String,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}: {}",
            self.path, self.line, self.column, self.severity, self.message
        )
    }
}

fn severity_word(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Hint => "hint",
    }
}

pub fn build(project: &Project, hir_diagnostics: &[skema_hir::Diagnostic]) -> Vec<Report> {
    skema_loader::report::build(project, hir_diagnostics)
        .into_iter()
        .map(|e| Report {
            path: e.path.display().to_string(),
            line: e.line,
            column: e.column,
            severity: severity_word(e.severity),
            message: e.message,
        })
        .collect()
}
