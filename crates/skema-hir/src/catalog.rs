//! Public, stable catalogs of the language's own vocabulary – known key
//! names, flag names, built-in type names.
//!
//! `docs/01`, "known keys are validated and autocompleted": [`rules::keys`]
//! and [`rules::resolve`] check *against* these lists. This module exists
//! so something outside this crate (a browser editor via `skema-wasm`,
//! eventually `skema-lsp`'s own completion) can *read* them too, instead of
//! re-deriving or duplicating the language's own keyword catalog.
//!
//! [`rules::keys`]: crate::rules
//! [`rules::resolve`]: crate::rules

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
/// `docs/02` describes `mode = draft` as "simple types suffice: string,
/// int, float, bool" – the reference project `examples/generic/` (itself
/// `mode = draft`) also uses `uuid`, `timestamp`, `varchar(n)`, `text`,
/// `date` throughout. Read as "suffices, but doesn't forbid anything more
/// specific" rather than as a closed allowlist – that's the reading that
/// fits the reference project, and the one that applies here. `mode` thus
/// no longer decides the type catalog, only `Lint` (`docs/04`) and the
/// future catalog extension per `dialect` do.
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
