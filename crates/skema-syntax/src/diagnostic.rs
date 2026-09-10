//! Error and warning messages.
//!
//! Diagnostics are returned alongside the tree, not in a `Result` — see
//! [`crate::parse`] for why.

use serde::Serialize;

use crate::Span;

/// How bad is it?
///
/// Ordered like the LSP protocol (Error = 1 … Hint = 4), so the conversion
/// at the protocol boundary stays trivial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

/// A single message with a location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub span: Span,
    pub severity: Severity,
    pub message: String,
}

impl Diagnostic {
    pub fn new(severity: Severity, span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            severity,
            message: message.into(),
        }
    }

    pub fn error(span: Span, message: impl Into<String>) -> Self {
        Self::new(Severity::Error, span, message)
    }

    pub fn warning(span: Span, message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, span, message)
    }
}
