//! Terminal representation of a message: `path:line:column: severity: text`.
//!
//! Everything *behind* that line – the analysis, the byte-offset conversion,
//! the merge of parser and checker messages, the sort order – is
//! `skema_loader::analyze`'s, shared with `skema-py` and `skema-lsp`. What
//! stays here is only the format, because a terminal line is the one thing
//! about a diagnostic that is genuinely this crate's business: `skema-api`
//! serves editors and has no reason to know what a shell prints.

use std::fmt;

use skema_api::Diagnostic;

/// Wraps a diagnostic in the one-line form a compiler is expected to print,
/// so `println!("{}", report::line(&d))` reads like the rest of the CLI.
pub fn line(diagnostic: &Diagnostic) -> impl fmt::Display + '_ {
    Line(diagnostic)
}

struct Line<'a>(&'a Diagnostic);

impl fmt::Display for Line<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d = self.0;
        write!(
            f,
            "{}:{}:{}: {}: {}",
            d.location.file,
            d.location.line,
            d.location.column,
            d.severity.as_str(),
            d.message
        )
    }
}
