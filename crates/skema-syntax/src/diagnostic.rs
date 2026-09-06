//! Error and warning messages.
//!
//! # Why not `Result<T, Error>`?
//!
//! The obvious Rust reflex would be `fn parse(src: &str) -> Result<Ast, Error>`.
//! For an IDE frontend, that's the wrong shape:
//!
//! * You want **all** errors at once, not just the first. Otherwise the
//!   user hops from error to error.
//! * You want a tree **despite** errors, so autocompletion and go-to-
//!   definition keep working while typing.
//!
//! That's why the signature is later `fn parse(src: &str) -> (Ast, Vec<Diagnostic>)`.
//! Errors are data, not control flow. That's the same approach taken by
//! rustc, TypeScript, and rust-analyzer.

use serde::Serialize;

use crate::Span;

/// How bad is it?
///
/// The order matches the LSP protocol (Error = 1 … Hint = 4), which makes
/// the conversion at the protocol boundary trivial.
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
