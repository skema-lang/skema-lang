//! Diagnostics across file boundaries.
//!
//! [`skema_syntax::Diagnostic`] suffices for the parser: one file, so a span
//! is unambiguous. Every rule from `docs/02` on spans several files, so a
//! diagnostic must also say **which** file its span belongs to.
//!
//! That file is always **the one whose text the span comes from**, not the
//! one a rule happens to be evaluating: `>cat.Copy.id` is reported where it
//! is written, not in `cat.skema`.

use serde::Serialize;

pub use skema_syntax::Severity;
use skema_syntax::Span;

use crate::FileId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub file: FileId,
    pub span: Span,
    pub severity: Severity,
    pub message: String,
}

impl Diagnostic {
    pub fn new(file: FileId, severity: Severity, span: Span, message: impl Into<String>) -> Self {
        Self {
            file,
            span,
            severity,
            message: message.into(),
        }
    }

    pub fn error(file: FileId, span: Span, message: impl Into<String>) -> Self {
        Self::new(file, Severity::Error, span, message)
    }

    pub fn warning(file: FileId, span: Span, message: impl Into<String>) -> Self {
        Self::new(file, Severity::Warning, span, message)
    }
}
