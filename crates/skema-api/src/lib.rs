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
//! Two front ends need exactly this: `skema-wasm` serves it to a browser,
//! `skema-lsp` to an editor. A display path, a UTF-16 offset and a highlight
//! class are concepts the core deliberately lacks — `skema_syntax::Span` is
//! byte offsets and nothing else — so they cannot live in `skema-hir`.
//!
//! One copy per front end would mean two answers to "where does this `Meta`
//! entry's value start", and the diagram drawn in an editor would drift from
//! the one drawn in the browser. Sharing also means both speak the same
//! JSON: the TypeScript types `skema-wasm` generates describe `skema-lsp`'s
//! custom responses too, so a client needs no second schema.
//!
//! # The `typescript` feature
//!
//! Off by default; `skema-lsp` leaves it off. `tsify`/`wasm-bindgen` are how
//! a type crosses into JavaScript, not part of what it *is*. On, every DTO
//! also derives `Tsify` and the boundary types gain their wasm ABI — all
//! `skema-wasm` needs to stay a thin wrapper.
//!
//! There is no I/O here, so nothing can fail and nothing returns a `Result`.

pub mod context;
pub mod diff;
pub mod dto;
pub mod highlight;
pub mod location;
pub mod meta;
pub mod project;
pub mod reference;
pub mod style;

use std::collections::BTreeMap;

use serde::Serialize;
use skema_hir::{FileId, InputFile, catalog, model};
#[cfg(feature = "typescript")]
use tsify_next::Tsify;

pub use context::{ContextKind, CursorContext, cursor_context};
pub use diff::{
    EnumChange, EnumDiff, IndexIdentity, SchemaDiff, TableChange, TableDiff, diff_models,
};
pub use dto::{
    CheckResult, DeclRef, Diagnostic, ExportResult, ExportedDatatype, ExportedDatatypeParam,
    ExportedDocument, ExportedEnum, ExportedEnumEntry, ExportedField, ExportedFlag, ExportedGroup,
    ExportedGroupMember, ExportedIndex, ExportedModel, ExportedPhysical, ExportedRelation,
    ExportedTable, ExportedTypeRef, FileEntry, FlagValue, LocateCtx, ReferenceLink, RelationKind,
    Severity, Span,
};
pub use highlight::{highlight_class_names, highlight_tokens};
pub use location::Location;
pub use meta::{
    EnumMeta, EnumMetas, GroupMeta, GroupMetas, MetaEntry, TableMeta, TableMetas, Utf16Range,
    enum_metas, group_metas, table_metas,
};
pub use project::{ProjectFile, ProjectFileStatus, ProjectInput, ProjectResult, analyze_project};
pub use reference::{block_reference_markdown, flag_reference};
pub use style::{StyleBlock, StyleBlocks, StyleScope, style_blocks};

pub const DEFAULT_FILENAME: &str = "input.skema";

/// Answers "is *this* text valid" for one file, without resolving `import` —
/// an `import` surfaces as an ordinary "import target not found" diagnostic.
/// More than one file goes through [`analyze_project`].
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

/// The language's vocabulary as the checking phase knows it
/// (`skema_hir::catalog`), so completion offers exactly the keys
/// `skema-hir` accepts and no hand-copied TypeScript list can drift.
pub fn builtins() -> Builtins {
    Builtins {
        meta_keys: owned(catalog::META_KEYS),
        lint_keys: owned(catalog::LINT_KEYS),
        database_keys: owned(catalog::DATABASE_KEYS),
        project_keys: owned(catalog::PROJECT_KEYS),
        datatype_field_block_keys: owned(catalog::DATATYPE_FIELD_BLOCK_KEYS),
        enum_field_block_keys: owned(catalog::ENUM_FIELD_BLOCK_KEYS),
        param_body_keys: owned(catalog::PARAM_BODY_KEYS),
        entry_body_keys: owned(catalog::ENTRY_BODY_KEYS),
        column_body_keys: owned(catalog::COLUMN_BODY_KEYS),
        flag_keys: owned(catalog::FLAG_KEYS),
        index_flag_keys: owned(catalog::INDEX_FLAG_KEYS),
        builtin_types: owned(catalog::BUILTIN_TYPES),
        dialect_types: dialect_types(),
        relation_action_values: owned(catalog::RELATION_ACTION_VALUES),
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
    /// Keys inside a `Datatype`'s `Field { … }` block (`docs/06`).
    pub datatype_field_block_keys: Vec<String>,
    /// Keys inside an `Enum`'s `Field { … }` block (`docs/08`).
    pub enum_field_block_keys: Vec<String>,
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
    /// Per-dialect built-in types beyond [`Self::builtin_types`]
    /// (`docs/06`, task 17's catalog), keyed by engine name
    /// (`skema_hir::catalog::DIALECT_ENGINES`). A caller offering
    /// completion for a project on one dialect merges in only that
    /// engine's entry; the whole small catalog is sent so a client never
    /// needs a second round trip to ask about a different one.
    pub dialect_types: BTreeMap<String, Vec<String>>,
    /// Values accepted by `onDelete`/`onUpdate` - `cascade`, `restrict`, …
    pub relation_action_values: Vec<String>,
    /// Every block keyword the parser recognizes, from `BlockKind` — the
    /// list that decides whether a word opens a block at all.
    pub block_keywords: Vec<String>,
}

fn owned(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

/// Every [`catalog::DIALECT_ENGINES`] engine's catalog, both a type's own
/// name and its alias where it has one – both are accepted spellings
/// (`DialectType::alias`), so both belong in completion.
fn dialect_types() -> BTreeMap<String, Vec<String>> {
    catalog::DIALECT_ENGINES
        .iter()
        .map(|&engine| {
            let names = catalog::dialect_types(engine)
                .iter()
                .flat_map(|t| std::iter::once(t.name).chain(t.alias))
                .map(str::to_owned)
                .collect();
            (engine.to_owned(), names)
        })
        .collect()
}

/// Derived from `BlockKind`, so a new block keyword reaches completion
/// without anyone updating a second list.
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
    // `check`/`export_model` analyze one in-memory file with no LSP session
    // behind it – nothing consumes go-to-definition data here.
    let (model, _references, hir_diagnostics) = skema_hir::analyze(std::slice::from_ref(&input));

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialect_types_covers_every_dialect_engine_with_no_empty_catalog() {
        let catalog = builtins();
        for &engine in catalog::DIALECT_ENGINES {
            let types = catalog
                .dialect_types
                .get(engine)
                .unwrap_or_else(|| panic!("no entry for {engine}"));
            assert!(!types.is_empty(), "{engine}'s catalog is empty");
        }
    }

    #[test]
    fn dialect_types_includes_both_a_type_and_its_alias() {
        let catalog = builtins();
        let postgres = &catalog.dialect_types["postgres"];
        assert!(postgres.contains(&"bigint".to_owned()));
        assert!(postgres.contains(&"int8".to_owned()));
    }
}
