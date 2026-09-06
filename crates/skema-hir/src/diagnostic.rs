//! Diagnostics across file boundaries.
//!
//! [`skema_syntax::Diagnostic`] is enough for the parser: one file, one
//! source text, a span is unambiguous. As soon as multiple files are in play
//! (every rule from `docs/02` onward is), a diagnostic must additionally say
//! **in which** file its span applies – hence a dedicated type instead of
//! reusing the parser's type.
//!
//! Rule of thumb for the `file` a diagnostic carries: **the file whose text
//! the span comes from** – not the file a rule happens to be evaluating. A
//! relationship target `>cat.Copy.id` is reported where `cat.Copy.id` is
//! written, not in `cat.skema` itself.

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
