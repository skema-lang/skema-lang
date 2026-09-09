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
    ProjectFileStatus, ProjectInput, ProjectResult, RelationKind, Severity, Span, StyleBlock,
    StyleBlocks, StyleScope, TableMeta, TableMetas, Utf16Range,
};

/// Readable panic messages in the browser console instead of an opaque
/// "unreachable" trap – defense in depth, even though the core is designed
/// to never panic on any input (`docs/01`: "a parser must never panic").
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn check(source: &str, filename: Option<String>) -> CheckResult {
    skema_api::check(source, filename.as_deref())
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
    skema_api::export_model(source, filename.as_deref(), with_locations)
}

/// Several in-memory files, with `import` resolved against the virtual path
/// set in `files` – a browser has no filesystem for `skema-loader` to walk.
/// A path that no file in the set matches is not an exception here: it
/// degrades to that one import failing, reported as an ordinary diagnostic.
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

/// One call, one parse: lexes and parses `source` once each, merges the
/// lossless token stream with the AST-driven semantic classification, and
/// returns `[class, utf16Start, utf16End]` triples flattened into one array.
///
/// Needs no debounce on the caller's side: the hand-written parser is fast,
/// and `skema-lsp` already re-parses in full on every `didChange`.
#[wasm_bindgen(js_name = "highlightTokens")]
pub fn highlight_tokens(source: &str) -> Vec<u32> {
    skema_api::highlight_tokens(source)
}

/// Every `Table` in `source`, with its `Meta` entries and the UTF-16 offsets
/// to edit them at. Tables without a `Meta` block are included too (with
/// `metaBodyStart: undefined` and no entries) – the caller still needs
/// `tableBodyStart` to add one.
///
/// Every offset belongs to *this exact revision* of `source`: fetch them
/// immediately before building an edit, never reuse an older result.
#[wasm_bindgen(js_name = "tableMetas")]
pub fn table_metas(source: &str) -> TableMetas {
    skema_api::table_metas(source)
}

/// Every `Enum` in `source`, with its `Meta` entries – the same shape as
/// [`table_metas`], for the declaration kind that needs it next: an `Enum`
/// gets rendered in a diagram and can carry `Meta.styleClasses` (`docs/03`)
/// just like a `Table` can.
#[wasm_bindgen(js_name = "enumMetas")]
pub fn enum_metas(source: &str) -> EnumMetas {
    skema_api::enum_metas(source)
}

/// Every `Group` in `source`, with its `Meta` entries – the same shape as
/// [`table_metas`]. A `Group`'s own diagram position is just another
/// `x-editorPosition` entry here, exactly like a `Table`'s.
#[wasm_bindgen(js_name = "groupMetas")]
pub fn group_metas(source: &str) -> GroupMetas {
    skema_api::group_metas(source)
}

/// Every `Style` block in `source`, unresolved: the two cascade levels are
/// reported as they are written (`docs/03`), and applying them is the
/// renderer's job. Single-file, like `tableMetas` – a caller with several
/// files already loops over them one at a time.
#[wasm_bindgen(js_name = "styleBlocks")]
pub fn style_blocks(source: &str) -> StyleBlocks {
    skema_api::style_blocks(source)
}

/// The language's own vocabulary, as the checking phase itself knows it
/// (`skema_hir::catalog`) – so completion offers exactly the keys
/// `skema-hir` will accept, instead of a hand-copied list in TypeScript
/// that drifts the first time the language grows a key.
#[wasm_bindgen(js_name = "builtins")]
pub fn builtins_js() -> Builtins {
    skema_api::builtins()
}
