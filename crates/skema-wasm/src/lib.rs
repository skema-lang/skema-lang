//! WASM bindings for skema: the browser-facing API behind an editor and
//! ER-diagram ("map") tool.
//!
//! | Export | For |
//! |---|---|
//! | `check` / `exportModel` | one in-memory file, no `import` resolution |
//! | `analyzeProject` | several in-memory files, `import`s resolved against a virtual path set ([`project`]) |
//! | `highlightTokens` / `highlightClassNames` | syntax highlighting, one merged token stream ([`highlight`]) |
//! | `tableMetas` | reading and precisely editing a `Table`'s `Meta` block ([`meta`]) |
//! | `builtins` | the language's own key/flag/type catalogs, for completion |
//!
//! `check`/`exportModel` stay deliberately single-file: they are the
//! cheapest thing that answers "is *this* text valid" (a scratch buffer, a
//! paste, a test), and an `import` in them surfaces as an ordinary "import
//! target not found" diagnostic. Anything with more than one file goes
//! through `analyzeProject`.
//!
//! Nothing here can fail (there is no I/O in this API), so nothing returns
//! a JS exception – consistent with the core's "errors are data, not
//! control flow" stance (`skema_syntax::parse`/`skema_hir::analyze` never
//! return `Result` either).

mod dto;
mod highlight;
mod location;
mod meta;
mod project;
mod style;

use serde::Serialize;
use skema_hir::{FileId, InputFile, catalog, model};
use tsify_next::Tsify;
use wasm_bindgen::prelude::*;

pub use dto::{
    CheckResult, Diagnostic, ExportResult, ExportedEnum, ExportedEnumEntry, ExportedField,
    ExportedFlag, ExportedGroup, ExportedGroupMember, ExportedIndex, ExportedModel,
    ExportedPhysical, ExportedRelation, ExportedTable, ExportedTypeRef, FlagValue, RelationKind,
    Severity, Span,
};
pub use highlight::{highlight_class_names, highlight_tokens};
pub use location::Location;
pub use meta::{
    EnumMeta, EnumMetas, GroupMeta, GroupMetas, MetaEntry, TableMeta, TableMetas, Utf16Range,
    enum_metas, group_metas, table_metas,
};
pub use project::{ProjectFile, ProjectFileStatus, ProjectInput, ProjectResult, analyze_project};
pub use style::{StyleBlock, StyleBlocks, StyleScope, style_blocks};

const DEFAULT_FILENAME: &str = "input.skema";

/// Readable panic messages in the browser console instead of an opaque
/// "unreachable" trap – defense in depth, even though the core is designed
/// to never panic on any input (`docs/01`: "a parser must never panic").
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn check(source: &str, filename: Option<String>) -> CheckResult {
    let filename = filename.unwrap_or_else(|| DEFAULT_FILENAME.to_owned());
    let (_model, diagnostics) = run_analysis(source, &filename);
    CheckResult {
        ok: is_ok(&diagnostics),
        diagnostics,
    }
}

/// Named `exportModel` rather than `export` on the JS side: `export` is a
/// reserved word in JavaScript/TypeScript, so a function literally named
/// `export` cannot be imported as a named binding (`import { export } from
/// …` is a syntax error) – wasm-bindgen itself would silently rename the
/// generated binding to `_export` to route around this, which is a
/// confusing, undocumented surprise for anyone reading the `.d.ts`.
#[wasm_bindgen(js_name = "exportModel")]
pub fn export_model_js(
    source: &str,
    filename: Option<String>,
    with_locations: bool,
) -> ExportResult {
    let filename = filename.unwrap_or_else(|| DEFAULT_FILENAME.to_owned());
    let (model, diagnostics) = run_analysis(source, &filename);
    let ok = is_ok(&diagnostics);
    let entry = dto::FileEntry {
        path: &filename,
        src: source,
    };
    let ctx = dto::LocateCtx {
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
#[wasm_bindgen(js_name = "builtins")]
pub fn builtins_js() -> Builtins {
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

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
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

fn is_ok(diagnostics: &[Diagnostic]) -> bool {
    !diagnostics.iter().any(|d| d.severity == Severity::Error)
}
