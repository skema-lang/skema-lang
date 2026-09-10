//! Syntactic frontend of the `.skema` DSL: lexer, parser, AST.
//!
//! # The one rule for this crate
//!
//! **No dependency on the outside world.** No `std::fs`, no network, no
//! `tokio`, no environment variables, no time, no logging backend — so the
//! same frontend runs in the CLI, the language server, the browser (WASM,
//! no filesystem) and Python. Callers read files and pass in a `&str`.
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
//! # Parse permissively, validate strictly
//!
//! The parser accepts more than the language allows — a `Lint` block inside
//! a `Table`, an `import` mid-file, an unknown `Meta` key — and all of it
//! yields valid AST nodes. A file being edited is almost always invalid,
//! and a parser that bails out takes completion away exactly when it is
//! needed. So [`parse`] always returns a tree plus what is wrong with it.
//!
//! Meaning checks live in [`skema-hir`](https://docs.rs/skema-hir).
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

/// Reads a `.skema` file. An empty diagnostic list means no syntax errors.
///
/// # Why not `Result`
///
/// An IDE frontend needs *all* errors at once, not just the first, and a
/// tree *despite* them so hover and go-to-definition keep working. Errors
/// are data here, not control flow.
pub fn parse(src: &str) -> (SourceFile, Vec<Diagnostic>) {
    let mut parser = Parser::new(src);
    let file = parser.parse_source_file();
    (file, parser.finish())
}
