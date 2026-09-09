//! The resolved model – the JSON export for other tools.
//!
//! Unlike [`skema_syntax::ast`], this type no longer represents the
//! *syntax* but the *meaning*: `use` partials are inlined, relationship
//! targets are resolved, physical names are cascaded. A field here exists
//! only if the checking phase understood it – faulty parts are missing
//! rather than the whole model being refused (the same stance as the
//! parser, see `skema_syntax`).

use std::collections::BTreeMap;

use serde::Serialize;
use skema_syntax::Span;

use crate::FileId;

#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct Model {
    pub tables: Vec<Table>,
    pub enums: Vec<Enum>,
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    /// The file this `Table` is declared in. A multi-file caller (several
    /// `InputFile`s in one `analyze` call) needs this to know which source
    /// text `span` refers to – a bare byte offset is ambiguous on its own
    /// the moment more than one file is in play.
    pub file: FileId,
    /// The `Document` namespace of the file that declares this table.
    pub namespace: String,
    pub name: String,
    pub physical: Physical,
    pub fields: Vec<Field>,
    pub indexes: Vec<Index>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    /// The file this field's own text lives in – **not necessarily** the
    /// owning `Table`'s file: a field inlined via `use` from a `Partial`
    /// declared in another (imported) file keeps that file here, so `span`
    /// stays unambiguous.
    pub file: FileId,
    pub name: String,
    pub ty: TypeRef,
    pub flags: Flags,
    pub relation: Option<Relation>,
    pub note: Option<String>,
    pub span: Span,
}

/// A type reference as it would ultimately be generated: base name plus
/// arguments. `varchar(80)` → `{ name: "varchar", args: ["80"] }`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TypeRef {
    pub name: String,
    pub args: Vec<String>,
}

/// A field's or index's `[]`-flags, keyed by their keyword (`pk`, `unique`,
/// `default`, a custom `x-`-flag, …). A valueless flag (`[pk]`) maps to
/// `true`; a valued flag (`` [default=`now()`] ``) maps to its stringified
/// value. Unrecognized keys are carried through unchanged rather than
/// dropped – the known/custom-key validation (`docs/01`) is a separate
/// concern handled by `rules::keys`, not by this export shape.
pub type Flags = BTreeMap<String, FlagValue>;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum FlagValue {
    Bool(bool),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Relation {
    pub kind: RelationKind,
    /// `Document` namespace of the file declaring the target table.
    ///
    /// Without it, `target_table` alone is ambiguous: two files may each
    /// declare a `Table Copy`, and a tool drawing the relationship would
    /// have to guess which one is meant. Namespace plus name is the pair
    /// that identifies a table project-wide (`docs/02`).
    pub target_namespace: String,
    pub target_table: String,
    pub target_field: String,
    pub on_delete: Option<String>,
    pub on_update: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RelationKind {
    OneToMany,
    ManyToOne,
    OneToOne,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Index {
    /// Same "not necessarily the owning Table's file" caveat as [`Field::file`].
    pub file: FileId,
    pub name: Option<String>,
    pub columns: Vec<String>,
    pub flags: Flags,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Enum {
    pub file: FileId,
    pub namespace: String,
    pub name: String,
    /// `string` or `int` – from `Field { type = …; }` (`docs/08`).
    pub stored_type: String,
    pub entries: Vec<EnumEntry>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EnumEntry {
    pub name: String,
    pub label: Option<String>,
    pub value: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Group {
    /// `Group` members are plain references (`Table Customer;`), never
    /// inlined content, so – unlike [`Field`]/[`Index`] – one file suffices
    /// for the whole group.
    pub file: FileId,
    pub name: String,
    pub members: Vec<GroupMember>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupMember {
    pub kind: String,
    pub namespace: String,
    pub name: String,
    pub span: Span,
}

/// Physical names after the `Project` → `Document` → `Table` cascade
/// (`docs/11`). `None` means: not set, the logical name applies.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct Physical {
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table: Option<String>,
}
