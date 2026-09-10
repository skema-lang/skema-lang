//! Name resolution and semantic checking for the `.skema` language.
//!
//! # Division of labor with `skema-syntax`
//!
//! `skema-syntax` parses permissively — a `Lint` inside a `Table`, an
//! unknown `Meta` key, a `use` of a missing `Partial` all yield a valid
//! tree. This crate is the strict counterpart: it takes several files'
//! trees, resolves names across file and namespace boundaries, and checks
//! the rules from `docs/02` through `docs/13`.
//!
//! # The one rule for this crate
//!
//! **Platform-neutral**, like `skema-syntax`: no `std::fs`, no network.
//! Which files belong to a project, and how `import "./x.skema"` becomes a
//! [`FileId`], is the caller's decision (`skema-loader`). Only the result
//! arrives here: a set of already-parsed [`InputFile`]s.
//!
//! # Structure
//!
//! ```text
//! Vec<InputFile>  ──[analyze]──►  Model + Vec<Diagnostic>
//! ```
//!
//! | Module | Purpose |
//! |---|---|
//! | `ids` | [`FileId`] – identity of a file within an analysis |
//! | `namespace` | Namespaces visible per file: own name + imports/aliases |
//! | `symbols` | Declaration collection and path resolution across file boundaries |
//! | [`model`] | The resolved model – exportable to JSON |
//! | `rules` | The individual checks from `docs/02`–`docs/13` |
//!
//! As with the parser, **an analysis always produces a model**, even with
//! diagnostics. Faulty parts are missing or incomplete; the rest is usable.

pub mod catalog;
mod diagnostic;
mod ids;
pub mod model;
mod namespace;
mod rules;
mod symbols;

pub use diagnostic::{Diagnostic, Severity};
pub use ids::FileId;

use skema_syntax::ast;

/// An already-read and parsed file, as handed to this crate.
#[derive(Debug)]
pub struct InputFile<'a> {
    pub id: FileId,
    /// For diagnostics and the model – usually a path relative to the project.
    pub display_path: String,
    pub ast: &'a ast::SourceFile,
    /// The targets of `ast`'s `import` statements, in `ast.imports()` order.
    /// `None` where the path resolved to no file in this analysis.
    pub import_targets: Vec<Option<FileId>>,
}

/// Analyzes an already-loaded project: name resolution plus all rules from
/// `docs/02`–`docs/13`.
///
/// `files` must be the **complete** set of files in the project — normally a
/// breadth-first walk of the import graph from the root file (`docs/02`,
/// project membership is reachability-defined). A file missing here is not
/// part of the project, even if it sits in the same directory.
pub fn analyze(files: &[InputFile<'_>]) -> (model::Model, Vec<Diagnostic>) {
    rules::run(files)
}
