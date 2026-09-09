//! Name resolution and semantic checking for the `.skema` language.
//!
//! # Division of labor with `skema-syntax`
//!
//! `skema-syntax` parses **permissively**: a `Lint` block inside a `Table`,
//! an unknown key in `Meta`, a `use` referring to a `Partial` that doesn't
//! exist – all of that produces a valid tree. This crate is the **strict**
//! counterpart: it takes the trees of multiple files, resolves names across
//! file and namespace boundaries, and checks the rules from `docs/02`
//! through `docs/13`.
//!
//! # The one rule for this crate
//!
//! Like `skema-syntax`, this crate stays **platform-neutral**: no
//! `std::fs`, no network. Which files belong to a project and how an
//! `import "./x.skema"` becomes a concrete [`FileId`] is decided by the
//! calling side (`skema-cli`, `skema-lsp`, `skema-py`) – it reads files,
//! follows the import graph, and hands over only the result here: a set of
//! already-parsed [`InputFile`]s.
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
//! As with the parser: **an analysis always produces a model**, even if
//! diagnostics were reported. Faulty parts are missing from the model or
//! incomplete; the rest stays usable.

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
///
/// Reading files and following `import` paths is up to the calling side
/// (see the module documentation) – only the result arrives here.
#[derive(Debug)]
pub struct InputFile<'a> {
    pub id: FileId,
    /// For diagnostics and the model – usually a path relative to the project.
    pub display_path: String,
    pub ast: &'a ast::SourceFile,
    /// The targets of the `import` statements from `ast`, in the same order
    /// as `ast.imports()`. `None` if the path could not be resolved to a
    /// file in this analysis – the I/O behind that is not this crate's
    /// concern, but the result is.
    pub import_targets: Vec<Option<FileId>>,
}

/// Analyzes an already-loaded project: name resolution plus all rules from
/// `docs/02`–`docs/13`.
///
/// `files` is the **complete** set of files belonging to the project –
/// usually the result of a breadth-first traversal of the import graph
/// starting from the root file (`docs/02`, "Project membership is
/// reachability-defined"). A file that doesn't show up here is not part of
/// the project, even if it lives in the same directory.
pub fn analyze(files: &[InputFile<'_>]) -> (model::Model, Vec<Diagnostic>) {
    rules::run(files)
}
