//! WASM bindings for skema: the browser-facing API behind an editor and
//! ER-diagram ("map") tool.
//!
//! Every answer here comes from [`skema_api`], which holds the actual
//! editor-facing services; this crate is only the boundary – the JS names,
//! the wasm ABI, and the panic hook. `skema-lsp` serves the same structures
//! over LSP from that same crate, so a browser and an editor never disagree
//! about what a model looks like.
//!
//! | Export | For |
//! |---|---|
//! | `check` / `exportModel` | one in-memory file, no `import` resolution |
//! | `analyzeProject` | several in-memory files, `import`s resolved against a virtual path set |
//! | `highlightTokens` / `highlightClassNames` | syntax highlighting, one merged token stream |
//! | `tableMetas` / `enumMetas` / `groupMetas` | reading and precisely editing a `Meta` block |
//! | `styleBlocks` | the `Style` cascade, unresolved |
//! | `builtins` | the language's own key/flag/type catalogs, for completion |
//!
//! Nothing here can fail (there is no I/O in this API), so nothing returns
//! a JS exception – consistent with the core's "errors are data, not
//! control flow" stance.

use wasm_bindgen::prelude::*;

// Re-exported so the generated `.d.ts` keeps naming these types, and so a
// Rust caller of this crate needs only one import.
pub use skema_api::{
    Builtins, CheckResult, Diagnostic, EnumMeta, EnumMetas, ExportResult, ExportedEnum,
    ExportedEnumEntry, ExportedField, ExportedFlag, ExportedGroup, ExportedGroupMember,
    ExportedIndex, ExportedModel, ExportedPhysical, ExportedRelation, ExportedTable,
    ExportedTypeRef, FlagValue, GroupMeta, GroupMetas, Location, MetaEntry, ProjectFile,
    ProjectFileStatus, ProjectInput, ProjectResult, ReferenceLink, RelationKind, Severity, Span,
    StyleBlock, StyleBlocks, StyleScope, TableMeta, TableMetas, Utf16Range,
};

/// Readable panic messages in the browser console instead of an opaque
/// "unreachable" trap. Defense in depth: the core is built never to panic
/// on any input (see `clippy.toml`), but a trap would be unreadable.
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn check(source: &str, filename: Option<String>) -> CheckResult {
    skema_api::check(source, filename.as_deref())
}

/// Named `exportModel`, not `export`: `export` is reserved in
/// JavaScript/TypeScript, so `import { export } from …` is a syntax error.
/// wasm-bindgen would silently rename the binding to `_export`, which is an
/// undocumented surprise for anyone reading the `.d.ts`.
#[wasm_bindgen(js_name = "exportModel")]
pub fn export_model_js(
    source: &str,
    filename: Option<String>,
    with_locations: bool,
) -> ExportResult {
    skema_api::export_model(source, filename.as_deref(), with_locations)
}

/// Several in-memory files, `import` resolved against the virtual path set
/// in `files` — a browser has no filesystem for `skema-loader` to walk. An
/// unmatched path degrades to that one import failing, reported as an
/// ordinary diagnostic.
#[wasm_bindgen(js_name = "analyzeProject")]
pub fn analyze_project(input: ProjectInput) -> ProjectResult {
    skema_api::analyze_project(input)
}

/// `class` code ↔ name, index-matched: `highlightClassNames()[class]` is the
/// name for that code. A plain list, not a map – the class codes in
/// [`highlight_tokens`]'s flat output *are* indexes into this same list.
#[wasm_bindgen(js_name = "highlightClassNames")]
pub fn highlight_class_names() -> Vec<String> {
    skema_api::highlight_class_names()
}

/// One call, one parse: merges the lossless token stream with the
/// AST-driven classification and returns `[class, utf16Start, utf16End]`
/// triples flattened into one array.
///
/// Needs no debounce — the hand-written parser is fast, and `skema-lsp`
/// re-parses in full on every `didChange` already.
#[wasm_bindgen(js_name = "highlightTokens")]
pub fn highlight_tokens(source: &str) -> Vec<u32> {
    skema_api::highlight_tokens(source)
}

/// Every `Table` in `source`, with its `Meta` entries and the UTF-16 offsets
/// to edit them at. Tables without a `Meta` block are included, with
/// `metaBodyStart: undefined` — the caller needs `tableBodyStart` to add one.
///
/// Offsets belong to *this exact revision* of `source`: fetch them right
/// before building an edit, never reuse an older result.
#[wasm_bindgen(js_name = "tableMetas")]
pub fn table_metas(source: &str) -> TableMetas {
    skema_api::table_metas(source)
}

/// Every `Enum` in `source`, with its `Meta` entries — same shape as
/// [`table_metas`]. An `Enum` is drawn in the diagram and can carry
/// `Meta.styleClasses` (`docs/03`) just like a `Table`.
#[wasm_bindgen(js_name = "enumMetas")]
pub fn enum_metas(source: &str) -> EnumMetas {
    skema_api::enum_metas(source)
}

/// Every `Group` in `source`, with its `Meta` entries – the same shape as
/// [`table_metas`].
#[wasm_bindgen(js_name = "groupMetas")]
pub fn group_metas(source: &str) -> GroupMetas {
    skema_api::group_metas(source)
}

/// Every `Style` block in `source`, unresolved: the two cascade levels as
/// written (`docs/03`); applying them is the renderer's job. Single-file
/// like `tableMetas`, since a multi-file caller already loops.
#[wasm_bindgen(js_name = "styleBlocks")]
pub fn style_blocks(source: &str) -> StyleBlocks {
    skema_api::style_blocks(source)
}

/// The language's vocabulary as the checking phase knows it
/// (`skema_hir::catalog`), so completion offers exactly the keys
/// `skema-hir` accepts and no hand-copied TypeScript list can drift.
#[wasm_bindgen(js_name = "builtins")]
pub fn builtins_js() -> Builtins {
    skema_api::builtins()
}
