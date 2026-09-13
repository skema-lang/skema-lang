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
pub const COLUMN_BODY_KEYS: &[&str] = &["note", "exampleValue", "label"];
pub const FLAG_KEYS: &[&str] = &[
    "pk", "unique", "required", "optional", "default", "column", "onDelete", "onUpdate",
];
pub const INDEX_FLAG_KEYS: &[&str] = &["unique"];

/// Values accepted by `onDelete`/`onUpdate` (`docs/13`, "Flag reference").
/// Enforced in `rules::resolve::check_relation_action`.
pub const RELATION_ACTION_VALUES: &[&str] =
    &["cascade", "restrict", "setNull", "setDefault", "noAction"];

/// Known `Dialect @engine` names (`docs/06`, "Dialect @engine blocks";
/// `docs/11`, "Engine-specific settings"). Also the set `Project.dialect`
/// accepts, minus `generic` – that value selects no engine at all.
pub const DIALECT_ENGINES: &[&str] = &["postgres", "mysql", "oracledb"];

/// The built-in, dialect-independent types (`docs/02`, `docs/06`).
///
/// Includes both [`DRAFT_TYPES`] and the precise types beyond it. The full
/// precise catalog is still growing – an open language question
/// (`docs/13`, "Known limitations") – but the draft/precise split itself is
/// decided (`docs/02`, "The mode switch").
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

/// The four types `mode = draft` gets by on (`docs/02`, "The mode switch") –
/// a subset of [`BUILTIN_TYPES`]. `draft` merely doesn't *require* more than
/// these; it still accepts every other built-in, an `Enum`, or a
/// `Datatype`. `strict` requires one of those more precise alternatives –
/// enforced in `rules::resolve::resolve_named_type`.
pub const DRAFT_TYPES: &[&str] = &["string", "int", "float", "bool"];

/// Parameter shape of every parameterized built-in in [`BUILTIN_TYPES`]
/// (`docs/06`, "Built-in types"): `varchar(n)` takes one length. Anything in
/// `BUILTIN_TYPES` not listed here takes no arguments at all.
pub const BUILTIN_TYPE_PARAMS: &[(&str, &[&str])] = &[("varchar", &["int"])];
