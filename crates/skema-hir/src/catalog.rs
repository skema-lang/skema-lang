//! Public, stable catalogs of the language's own vocabulary – known key
//! names, flag names, built-in type names.
//!
//! `docs/01`, "Custom keys": `rules::keys` and `rules::resolve` check
//! *against* these lists. Public so the completion in `skema-wasm` and
//! `skema-lsp` can read the same catalog instead of duplicating it.

pub const META_KEYS: &[&str] = &["author", "note", "version", "styleClasses"];
pub const LINT_KEYS: &[&str] = &[
    "fieldNaming",
    "tableNaming",
    "enumNaming",
    "unusedImport",
    "danglingRef",
];
pub const DATABASE_KEYS: &[&str] = &["database", "schema", "table"];
pub const PROJECT_KEYS: &[&str] = &["mode", "dialect"];
pub const FIELD_BLOCK_KEYS: &[&str] = &["type", "default", "regex", "note"];
pub const PARAM_BODY_KEYS: &[&str] = &["note", "default"];
pub const ENTRY_BODY_KEYS: &[&str] = &["label", "value"];
pub const COLUMN_BODY_KEYS: &[&str] = &["note", "exampleValue"];
pub const FLAG_KEYS: &[&str] = &[
    "pk", "unique", "required", "optional", "default", "column", "onDelete", "onUpdate",
];
pub const INDEX_FLAG_KEYS: &[&str] = &["unique"];

/// The built-in, dialect-independent types (`docs/02`, `docs/06`).
///
/// `docs/02` says `mode = draft` gets by on `string`/`int`/`float`/`bool`,
/// but the reference project `examples/generic/` is `mode = draft` and uses
/// `uuid`, `timestamp`, `varchar(n)`, `text` and `date` throughout. Read
/// here as "suffices, but forbids nothing more specific": `mode` does not
/// decide the type catalog. The full catalog is an open language question
/// (`docs/13`, "Built-in type catalog").
pub const BUILTIN_TYPES: &[&str] = &[
    "string",
    "int",
    "float",
    "bool",
    "varchar",
    "text",
    "uuid",
    "date",
    "timestamp",
];
