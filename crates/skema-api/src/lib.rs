//! What an editor needs to know about a model, over and above "is it
//! valid": the resolved model with a location for every entity, the exact
//! offsets of a `Meta` entry, the `Style` cascade, and a token stream for
//! highlighting.
//!
//! | Export | For |
//! |---|---|
//! | [`check`] / [`export_model`] | one in-memory file, no `import` resolution |
//! | [`analyze_project`] | several in-memory files, `import`s resolved against a virtual path set ([`project`]) |
//! | [`highlight_tokens`] / [`highlight_class_names`] | syntax highlighting, one merged token stream ([`highlight`]) |
//! | [`table_metas`] / [`enum_metas`] / [`group_metas`] | reading and precisely editing a `Meta` block ([`meta`]) |
//! | [`style_blocks`] | the `Style` cascade, unresolved ([`style`]) |
//! | [`builtins`] | the language's own key/flag/type catalogs, for completion |
//!
//! # Why this is its own crate
//!
//! Two front ends need exactly this and nothing else: `skema-wasm` serves it
//! to a browser, `skema-lsp` serves it to an editor over LSP. Everything
//! here is one analysis away from `skema-hir` but is not *of* it – a display
//! path, a UTF-16 offset and a highlight class are all concepts the core
//! deliberately does not have (`skema_syntax::Span` is byte offsets, "and
//! nothing else").
//!
//! Duplicating it per front end would mean two answers to questions like
//! "where exactly does this `Meta` entry's value start", and a diagram drawn
//! by the editor would slowly stop matching the one drawn in the browser.
//! Sharing it also means the two speak the same JSON: the TypeScript types
//! `skema-wasm` generates describe `skema-lsp`'s custom responses too, which
//! is what lets a client vendor those declarations without a second schema.
//!
//! # The `typescript` feature
//!
//! Off by default, and `skema-lsp` leaves it off: `tsify`/`wasm-bindgen` are
//! how a type crosses into JavaScript, not part of what the type *is*. With
//! the feature on, every DTO here additionally derives `Tsify` and the
//! boundary types gain their wasm ABI – which is all `skema-wasm` needs to
//! stay a thin wrapper rather than a second copy of these structures.
//!
//! Nothing here can fail (there is no I/O in this API), so nothing returns a
//! `Result` – consistent with the core's "errors are data, not control flow"
//! stance (`skema_syntax::parse`/`skema_hir::analyze` never return one
//! either).

pub mod dto;
pub mod highlight;
pub mod location;
pub mod meta;
pub mod project;
pub mod style;

use serde::Serialize;
use skema_hir::{FileId, InputFile, catalog, model};
#[cfg(feature = "typescript")]
use tsify_next::Tsify;

pub use dto::{
    CheckResult, Diagnostic, ExportResult, ExportedEnum, ExportedEnumEntry, ExportedField,
    ExportedFlag, ExportedGroup, ExportedGroupMember, ExportedIndex, ExportedModel,
    ExportedPhysical, ExportedRelation, ExportedTable, ExportedTypeRef, FileEntry, FlagValue,
    LocateCtx, RelationKind, Severity, Span,
};
pub use highlight::{highlight_class_names, highlight_tokens};
pub use location::Location;
pub use meta::{
    EnumMeta, EnumMetas, GroupMeta, GroupMetas, MetaEntry, TableMeta, TableMetas, Utf16Range,
    enum_metas, group_metas, table_metas,
};
pub use project::{ProjectFile, ProjectFileStatus, ProjectInput, ProjectResult, analyze_project};
pub use style::{StyleBlock, StyleBlocks, StyleScope, style_blocks};

pub const DEFAULT_FILENAME: &str = "input.skema";

/// The cheapest thing that answers "is *this* text valid": one file, no
/// `import` resolution, so an `import` in it surfaces as an ordinary "import
/// target not found" diagnostic. Anything with more than one file goes
/// through [`analyze_project`].
pub fn check(source: &str, filename: Option<&str>) -> CheckResult {
    let filename = filename.unwrap_or(DEFAULT_FILENAME);
    let (_model, diagnostics) = run_analysis(source, filename);
    CheckResult {
        ok: is_ok(&diagnostics),
        diagnostics,
    }
}

/// Like [`check`], but also returns the resolved model of that one file.
pub fn export_model(source: &str, filename: Option<&str>, with_locations: bool) -> ExportResult {
    let filename = filename.unwrap_or(DEFAULT_FILENAME);
    let (model, diagnostics) = run_analysis(source, filename);
    let ok = is_ok(&diagnostics);
    let entry = FileEntry {
        path: filename,
        src: source,
    };
    let ctx = LocateCtx {
        files: std::slice::from_ref(&entry),
        with_locations,
    };
    ExportResult {
        ok,
        model: dto::export_model(&model, &ctx),
        diagnostics,
    }
}

/// The language's own vocabulary, as the checking phase itself knows it
/// (`skema_hir::catalog`) – so an editor's completion offers exactly the
/// keys `skema-hir` will accept, instead of a hand-copied list in
/// TypeScript that drifts the first time the language grows a key.
pub fn builtins() -> Builtins {
    Builtins {
        meta_keys: owned(catalog::META_KEYS),
        lint_keys: owned(catalog::LINT_KEYS),
        database_keys: owned(catalog::DATABASE_KEYS),
        project_keys: owned(catalog::PROJECT_KEYS),
        field_block_keys: owned(catalog::FIELD_BLOCK_KEYS),
        param_body_keys: owned(catalog::PARAM_BODY_KEYS),
        entry_body_keys: owned(catalog::ENTRY_BODY_KEYS),
        column_body_keys: owned(catalog::COLUMN_BODY_KEYS),
        flag_keys: owned(catalog::FLAG_KEYS),
        index_flag_keys: owned(catalog::INDEX_FLAG_KEYS),
        builtin_types: owned(catalog::BUILTIN_TYPES),
        block_keywords: block_keywords(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[cfg_attr(feature = "typescript", tsify(into_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct Builtins {
    pub meta_keys: Vec<String>,
    pub lint_keys: Vec<String>,
    pub database_keys: Vec<String>,
    pub project_keys: Vec<String>,
    /// Keys inside a `Field { … }` description block (`docs/06`, `docs/08`).
    pub field_block_keys: Vec<String>,
    /// Keys in a `Datatype` parameter's body.
    pub param_body_keys: Vec<String>,
    /// Keys in an `Enum` entry's body.
    pub entry_body_keys: Vec<String>,
    /// Keys in a table column's `{ … }` body.
    pub column_body_keys: Vec<String>,
    /// Keys valid inside a field's `[ … ]` flags.
    pub flag_keys: Vec<String>,
    pub index_flag_keys: Vec<String>,
    pub builtin_types: Vec<String>,
    /// Every block keyword the parser recognizes, straight from
    /// `BlockKind` – the same list that decides whether a word opens a
    /// block at all (`grammar::parse_item_kind`).
    pub block_keywords: Vec<String>,
}

fn owned(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

/// Derived from `BlockKind` itself rather than written out again here, so
/// a new block keyword in the language shows up in completion without
/// anyone remembering to update a second list.
fn block_keywords() -> Vec<String> {
    use skema_syntax::ast::BlockKind::{
        Database, Datatype, Dialect, Document, Enum, Field, Group, Index, Lint, Meta, Partial,
        Project, Style, Table,
    };
    [
        Project, Document, Table, Partial, Enum, Group, Datatype, Meta, Style, Index, Database,
        Field, Dialect, Lint,
    ]
    .iter()
    .map(|k| k.as_str().to_owned())
    .collect()
}

fn run_analysis(source: &str, filename: &str) -> (model::Model, Vec<Diagnostic>) {
    let (ast, syntax_diagnostics) = skema_syntax::parse(source);
    let import_targets = vec![None; ast.imports().count()];
    let input = InputFile {
        id: FileId(0),
        display_path: filename.to_owned(),
        ast: &ast,
        import_targets,
    };
    let (model, hir_diagnostics) = skema_hir::analyze(std::slice::from_ref(&input));

    let mut diagnostics: Vec<Diagnostic> = syntax_diagnostics
        .iter()
        .map(|d| dto::syntax_diagnostic(source, filename, d))
        .collect();
    diagnostics.extend(
        hir_diagnostics
            .iter()
            .map(|d| dto::hir_diagnostic(source, filename, d)),
    );
    (model, diagnostics)
}

pub fn is_ok(diagnostics: &[Diagnostic]) -> bool {
    !diagnostics.iter().any(|d| d.severity == Severity::Error)
}
