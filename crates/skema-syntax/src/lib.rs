//! Syntactic frontend of the `.skema` DSL: lexer, parser, AST.
//!
//! # The one rule for this crate
//!
//! **No dependency on the outside world.** No `std::fs`, no network, no
//! `tokio`, no environment variables, no time, no logging backend.
//!
//! The reason isn't purism, it's reusability. This crate is meant to
//! eventually run:
//!
//! * in the CLI (`skema check`)
//! * in the language server
//! * in the browser via WASM (there's no filesystem there)
//! * in Python via `pyo3`
//!
//! Anyone who wants to read files does so in the calling crate and passes
//! in a `&str`. Retrofitting this separation later is expensive — keeping
//! it from the start costs almost nothing.
//!
//! # Structure
//!
//! ```text
//! &str ──[lexer]──► Vec<Token> ──[parser + grammar]──► SourceFile
//!                                                    + Vec<Diagnostic>
//! ```
//!
//! | Module | Responsibility |
//! |---|---|
//! | `span` | positions — byte offsets, nothing else |
//! | `token` | the language's token kinds |
//! | `lexer` | characters → tokens, lossless and panic-free |
//! | [`ast`] | the syntax tree's nodes |
//! | `parser` | cursor, error handling, recovery (*how*) |
//! | `grammar` | the language's rules (*what*) |
//! | `diagnostic` | messages with a location |
//! | `dump` | tree as text, for inspection |
//!
//! # The guiding idea: parse permissively, validate strictly
//!
//! The parser deliberately accepts **more** than the language allows: a
//! `Lint` block inside a `Table`, an `import` in the middle of a file, an
//! unknown key in `Meta`. All of that yields valid AST nodes.
//!
//! The reason is practical. Someone typing in an IDE almost always has an
//! invalid file in front of them. A parser that bails out at the first
//! inconsistency takes away autocompletion at exactly the moment it's
//! needed. That's why [`parse`] **always** returns a tree, plus a list of
//! what's wrong with it.
//!
//! Meaning checks ("`Lint` belongs inside `Project`", "`athor` is not a
//! known key") are a separate phase and live in the
//! [`skema-hir`](https://docs.rs/skema-hir) crate, which builds on this
//! AST.
//!
//! # Example
//!
//! ```
//! use skema_syntax::{ast::BlockKind, parse};
//!
//! let (file, diagnostics) = parse(r#"
//!     spec 1;
//!     Table Customer {
//!       id    int     [pk];
//!       email varchar [unique];
//!     }
//! "#);
//!
//! assert!(diagnostics.is_empty());
//!
//! let table = file.blocks().next().expect("a declaration");
//! assert_eq!(table.kind, BlockKind::Table);
//! assert_eq!(table.items.len(), 2);
//! ```

pub mod ast;
mod diagnostic;
mod dump;
mod grammar;
mod lexer;
mod parser;
mod semantic;
mod span;
mod token;

pub use diagnostic::{Diagnostic, Severity};
pub use dump::dump;
pub use lexer::{Lexer, tokenize};
pub use parser::Parser;
pub use semantic::{SemanticKind, classify};
pub use span::{Span, TextSize};
pub use token::{Token, TokenKind, keyword_from_str};

use ast::SourceFile;

/// Reads a `.skema` file.
///
/// # Why not `Result`
///
/// The obvious Rust reflex would be `-> Result<SourceFile, Error>`. For an
/// IDE frontend, that's the wrong shape:
///
/// * You want **all** errors at once, not just the first — otherwise the
///   user hops from message to message.
/// * You want a tree **despite** errors, so hover and go-to-definition
///   keep working.
///
/// So errors here are data, not control flow. If the list is empty, the
/// file was syntactically error-free.
pub fn parse(src: &str) -> (SourceFile, Vec<Diagnostic>) {
    let mut parser = Parser::new(src);
    let file = parser.parse_source_file();
    (file, parser.finish())
}
