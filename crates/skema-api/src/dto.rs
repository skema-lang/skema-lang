//! JS-facing DTOs mirroring `skema_hir::model` and the diagnostic types,
//! plus the plain conversion functions from the core's types to these.
//!
//! A 1:1 mirror rather than a re-export of `skema_hir::model`: `tsify` only
//! emits a real (non-`any`) TypeScript type for a field whose own type also
//! derives it, and pulling `tsify`/`wasm-bindgen` into `skema-hir` would
//! cross its platform-neutral boundary.
//!
//! Plain conversion functions, not `From` impls — turning a `span` into an
//! optional `location` needs more context (source text, filename, whether
//! locations were requested) than a bare `&T` carries.

use std::collections::BTreeMap;

use serde::Serialize;
use skema_hir::FileId;
use skema_hir::model;
#[cfg(feature = "typescript")]
use tsify_next::Tsify;

use crate::location::{self, Location};

/// One file's text and display path, as needed to resolve a `model` entity's
/// `file: FileId` back to source text and a human-readable path.
#[derive(Debug)]
pub struct FileEntry<'a> {
    pub path: &'a str,
    pub src: &'a str,
}

/// What every conversion function needs to turn a `(FileId, span)` pair
/// into a display path and an optional human [`Location`].
///
/// `files` is indexed by `FileId.0`, the same contiguity guarantee
/// `skema_hir::analyze` requires of its own callers – for the existing
/// single-file `exportModel` this is a one-element slice; `analyzeProject`
/// passes the whole resolved multi-file set.
#[derive(Debug)]
pub struct LocateCtx<'a> {
    pub files: &'a [FileEntry<'a>],
    pub with_locations: bool,
}

impl LocateCtx<'_> {
    fn file(&self, id: FileId) -> &FileEntry<'_> {
        &self.files[id.0 as usize]
    }

    fn path(&self, id: FileId) -> String {
        self.file(id).path.to_owned()
    }

    fn locate(&self, id: FileId, span: skema_syntax::Span) -> Option<Location> {
        self.with_locations.then(|| {
            let file = self.file(id);
            location::locate(file.src, file.path, span)
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl From<skema_syntax::Span> for Span {
    fn from(s: skema_syntax::Span) -> Self {
        Self {
            start: s.start,
            end: s.end,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

impl From<skema_syntax::Severity> for Severity {
    fn from(s: skema_syntax::Severity) -> Self {
        match s {
            skema_syntax::Severity::Error => Self::Error,
            skema_syntax::Severity::Warning => Self::Warning,
            skema_syntax::Severity::Info => Self::Info,
            skema_syntax::Severity::Hint => Self::Hint,
        }
    }
}

impl Severity {
    /// The same word `serde` writes for this variant (see the
    /// `rename_all` above), for the places that render a diagnostic as
    /// text rather than as JSON – `skema-cli`'s terminal line. Written
    /// once here so the printed word and the serialized one cannot drift.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Hint => "hint",
        }
    }
}

/// Unlike the model entities below, a diagnostic always gets a `location`:
/// the list is always small, and a diagnostic without a human position is
/// nearly useless to an editor. `withLocations` only gates the (potentially
/// large) per-entity `location` field on the model.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub span: Span,
    pub location: Location,
    pub severity: Severity,
    pub message: String,
}

fn diagnostic(
    src: &str,
    filename: &str,
    span: skema_syntax::Span,
    severity: Severity,
    message: &str,
) -> Diagnostic {
    Diagnostic {
        span: span.into(),
        location: location::locate(src, filename, span),
        severity,
        message: message.to_owned(),
    }
}

pub fn syntax_diagnostic(src: &str, filename: &str, d: &skema_syntax::Diagnostic) -> Diagnostic {
    diagnostic(src, filename, d.span, d.severity.into(), &d.message)
}

pub fn hir_diagnostic(src: &str, filename: &str, d: &skema_hir::Diagnostic) -> Diagnostic {
    diagnostic(src, filename, d.span, d.severity.into(), &d.message)
}

/// One written reference and the declaration it resolves to – `skema-hir`'s
/// [`model::ReferenceLink`], with each [`FileId`] turned into the display
/// path a caller can actually look a file up by. Raw byte spans, like
/// [`Diagnostic::span`]: go-to-definition needs to test "does this span
/// contain the cursor's offset", not a human line/column.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct ReferenceLink {
    pub from_file: String,
    pub from_span: Span,
    pub to_file: String,
    pub to_span: Span,
}

pub fn reference_link(ctx: &LocateCtx<'_>, link: &model::ReferenceLink) -> ReferenceLink {
    ReferenceLink {
        from_file: ctx.path(link.from_file),
        from_span: link.from_span.into(),
        to_file: ctx.path(link.to_file),
        to_span: link.to_span.into(),
    }
}

/// A diagnostic about a file rather than about a place in one – "this file
/// could not be read", which has no span because nothing was parsed.
///
/// Points at `1:1` with an empty span: a reader looks at the top of the
/// file first, and an empty range is the honest answer to "which characters
/// is this about". Kept here rather than hand-built by each caller so that
/// such a message is the same `Diagnostic` as every other one, and no front
/// end grows a second error shape alongside its diagnostics list.
pub fn synthetic_diagnostic(filename: &str, severity: Severity, message: &str) -> Diagnostic {
    Diagnostic {
        span: Span { start: 0, end: 0 },
        location: Location {
            file: filename.to_owned(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 1,
        },
        severity,
        message: message.to_owned(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[cfg_attr(feature = "typescript", tsify(into_wasm_abi))]
pub struct CheckResult {
    pub ok: bool,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[cfg_attr(feature = "typescript", tsify(into_wasm_abi, hashmap_as_object))]
pub struct ExportResult {
    pub ok: bool,
    pub model: ExportedModel,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct ExportedModel {
    pub tables: Vec<ExportedTable>,
    pub enums: Vec<ExportedEnum>,
    pub groups: Vec<ExportedGroup>,
    /// One per file that declares a `Document`, root file first.
    pub documents: Vec<ExportedDocument>,
    /// Every declared `Datatype`, used or not, sorted by namespace and name.
    pub datatypes: Vec<ExportedDatatype>,
    /// The project's `dialect` engine name (`docs/02`) – `None` for
    /// `generic`, or when no `Project` block was found.
    pub dialect: Option<String>,
}

/// One `[]`-flag as written: `[pk]` carries `true`, `[default=0]` its
/// stringified value.
///
/// A list of named entries, not a map. `model::Flags` is a `BTreeMap`,
/// which crosses the wasm boundary as a JS `Map` unless the top-level type
/// sets `hashmap_as_object`, and a bare `pub type` alias emits no
/// TypeScript declaration at all — the generated `.d.ts` then referenced an
/// undefined `Flags` and consumers silently got `any`. A declared struct
/// says exactly what arrives: an array in `BTreeMap` order.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct ExportedFlag {
    pub name: String,
    pub value: FlagValue,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(untagged)]
pub enum FlagValue {
    Bool(bool),
    Text(String),
}

impl From<&model::FlagValue> for FlagValue {
    fn from(v: &model::FlagValue) -> Self {
        match v {
            model::FlagValue::Bool(b) => Self::Bool(*b),
            model::FlagValue::Text(t) => Self::Text(t.clone()),
        }
    }
}

fn flags(flags: &model::Flags) -> Vec<ExportedFlag> {
    flags
        .iter()
        .map(|(name, value)| ExportedFlag {
            name: name.clone(),
            value: value.into(),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[cfg_attr(feature = "typescript", tsify(hashmap_as_object))]
#[serde(rename_all = "camelCase")]
pub struct ExportedTable {
    /// Display path of the file this `Table` is declared in – the file a
    /// position-editing tool must open/edit to change this table's own
    /// `Meta`, independent of which file(s) its fields come from.
    pub file: String,
    pub namespace: String,
    pub name: String,
    pub physical: ExportedPhysical,
    pub fields: Vec<ExportedField>,
    pub indexes: Vec<ExportedIndex>,
    pub examples: Vec<ExportedExampleRow>,
    /// `Meta.movedFrom` – see `model::Table::moved_from`.
    pub moved_from: Option<String>,
    pub extensions: BTreeMap<String, String>,
    pub span: Span,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[cfg_attr(feature = "typescript", tsify(hashmap_as_object))]
#[serde(rename_all = "camelCase")]
pub struct ExportedField {
    /// Display path of the file this field's own text lives in – **not
    /// necessarily** the owning table's `file`: a field inlined via `use`
    /// from a `Partial` declared in another (imported) file keeps that
    /// file here, so `span`/`location` stay unambiguous.
    pub file: String,
    pub name: String,
    pub ty: ExportedTypeRef,
    pub flags: Vec<ExportedFlag>,
    pub relation: Option<ExportedRelation>,
    pub note: Option<String>,
    pub label: Option<String>,
    pub extensions: BTreeMap<String, String>,
    pub span: Span,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
pub struct ExportedTypeRef {
    /// As written in the field's own file, import alias included.
    pub name: String,
    pub args: Vec<String>,
    pub datatype: Option<DeclRef>,
    #[serde(rename = "enum")]
    pub enum_: Option<DeclRef>,
}

/// A declaration identified project-wide. `namespace` is the declaring
/// file's `Document` name, never an import alias.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
pub struct DeclRef {
    pub namespace: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct ExportedRelation {
    pub kind: RelationKind,
    /// Namespace of the target table – with `targetTable`, the pair that
    /// identifies it project-wide (a bare name can occur in several files).
    pub target_namespace: String,
    pub target_table: String,
    pub target_field: String,
    pub on_delete: Option<String>,
    pub on_update: Option<String>,
    pub span: Span,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
pub enum RelationKind {
    OneToMany,
    ManyToOne,
    OneToOne,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct ExportedIndex {
    /// Same "not necessarily the owning table's file" caveat as
    /// [`ExportedField::file`].
    pub file: String,
    pub name: Option<String>,
    pub columns: Vec<String>,
    pub flags: Vec<ExportedFlag>,
    pub span: Span,
    pub location: Option<Location>,
}

/// One `Record` inside a `Table`'s `Example` block (`docs/05`) – sample
/// data, descriptive only. Unnamed by design: identified only by position
/// in [`ExportedTable::examples`] (see `brain/decisions.md`).
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct ExportedExampleRow {
    /// Same "not necessarily the owning table's file" caveat as
    /// [`ExportedField::file`].
    pub file: String,
    pub values: Vec<ExportedFlag>,
    pub span: Span,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[cfg_attr(feature = "typescript", tsify(hashmap_as_object))]
#[serde(rename_all = "camelCase")]
pub struct ExportedEnum {
    pub file: String,
    pub namespace: String,
    pub name: String,
    pub stored_type: String,
    pub default: Option<String>,
    pub note: Option<String>,
    pub entries: Vec<ExportedEnumEntry>,
    pub extensions: BTreeMap<String, String>,
    pub span: Span,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[cfg_attr(feature = "typescript", tsify(hashmap_as_object))]
pub struct ExportedEnumEntry {
    pub name: String,
    pub label: Option<String>,
    pub value: Option<String>,
    pub extensions: BTreeMap<String, String>,
    pub span: Span,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[cfg_attr(feature = "typescript", tsify(hashmap_as_object))]
#[serde(rename_all = "camelCase")]
pub struct ExportedGroup {
    /// `Group` members are plain references, never inlined content, so –
    /// unlike [`ExportedField`]/[`ExportedIndex`] – one file suffices for
    /// the whole group.
    pub file: String,
    pub name: String,
    pub members: Vec<ExportedGroupMember>,
    pub extensions: BTreeMap<String, String>,
    pub span: Span,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
pub struct ExportedGroupMember {
    pub kind: String,
    pub namespace: String,
    pub name: String,
    pub span: Span,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[cfg_attr(feature = "typescript", tsify(hashmap_as_object))]
pub struct ExportedDocument {
    pub file: String,
    pub namespace: String,
    pub extensions: BTreeMap<String, String>,
    pub span: Span,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[cfg_attr(feature = "typescript", tsify(hashmap_as_object))]
#[serde(rename_all = "camelCase")]
pub struct ExportedDatatype {
    pub file: String,
    pub namespace: String,
    pub name: String,
    pub params: Vec<ExportedDatatypeParam>,
    /// `Field.type` as written, without arguments.
    pub base_type: Option<String>,
    pub base_type_args: Vec<String>,
    /// Set when `baseType` names another `Datatype`.
    pub base_datatype: Option<DeclRef>,
    /// See `model::Datatype::resolved_base_type`.
    pub resolved_base_type: Option<String>,
    pub default: Option<String>,
    /// Unsubstituted: `{param}` placeholders stay as written.
    pub regex: Option<String>,
    pub note: Option<String>,
    pub extensions: BTreeMap<String, String>,
    pub span: Span,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
pub struct ExportedDatatypeParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Option<String>,
    pub default: Option<String>,
    pub note: Option<String>,
}

/// Physical names after the `Project` → `Document` → `Table` cascade. No
/// span: it's a cascade of up to three independent source blocks, so there
/// is no single correct source location for it – inventing one would be
/// actively misleading.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
pub struct ExportedPhysical {
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table: Option<String>,
}

fn type_ref(t: &model::TypeRef) -> ExportedTypeRef {
    ExportedTypeRef {
        name: t.name.clone(),
        args: t.args.clone(),
        datatype: t.datatype.as_ref().map(decl_ref),
        enum_: t.enum_.as_ref().map(decl_ref),
    }
}

fn decl_ref(d: &model::DeclRef) -> DeclRef {
    DeclRef {
        namespace: d.namespace.clone(),
        name: d.name.clone(),
    }
}

fn physical(p: &model::Physical) -> ExportedPhysical {
    ExportedPhysical {
        database: p.database.clone(),
        schema: p.schema.clone(),
        table: p.table.clone(),
    }
}

/// `relation.span` is the field's own `>`/`<`/`-` type-position text
/// (`resolve.rs`'s `RelationSite.span`), always written in the same file as
/// the owning field itself – hence the `owner_file` parameter rather than a
/// `file` field of its own on `model::Relation`.
fn relation(r: &model::Relation, owner_file: FileId, ctx: &LocateCtx<'_>) -> ExportedRelation {
    ExportedRelation {
        kind: match r.kind {
            model::RelationKind::OneToMany => RelationKind::OneToMany,
            model::RelationKind::ManyToOne => RelationKind::ManyToOne,
            model::RelationKind::OneToOne => RelationKind::OneToOne,
        },
        target_namespace: r.target_namespace.clone(),
        target_table: r.target_table.clone(),
        target_field: r.target_field.clone(),
        on_delete: r.on_delete.clone(),
        on_update: r.on_update.clone(),
        span: r.span.into(),
        location: ctx.locate(owner_file, r.span),
    }
}

fn field(f: &model::Field, ctx: &LocateCtx<'_>) -> ExportedField {
    ExportedField {
        file: ctx.path(f.file),
        name: f.name.clone(),
        ty: type_ref(&f.ty),
        flags: flags(&f.flags),
        relation: f.relation.as_ref().map(|r| relation(r, f.file, ctx)),
        note: f.note.clone(),
        label: f.label.clone(),
        extensions: f.extensions.clone(),
        span: f.span.into(),
        location: ctx.locate(f.file, f.span),
    }
}

fn index(i: &model::Index, ctx: &LocateCtx<'_>) -> ExportedIndex {
    ExportedIndex {
        file: ctx.path(i.file),
        name: i.name.clone(),
        columns: i.columns.clone(),
        flags: flags(&i.flags),
        span: i.span.into(),
        location: ctx.locate(i.file, i.span),
    }
}

fn example_row(e: &model::ExampleRow, ctx: &LocateCtx<'_>) -> ExportedExampleRow {
    ExportedExampleRow {
        file: ctx.path(e.file),
        values: flags(&e.values),
        span: e.span.into(),
        location: ctx.locate(e.file, e.span),
    }
}

fn table(t: &model::Table, ctx: &LocateCtx<'_>) -> ExportedTable {
    ExportedTable {
        file: ctx.path(t.file),
        namespace: t.namespace.clone(),
        name: t.name.clone(),
        physical: physical(&t.physical),
        fields: t.fields.iter().map(|f| field(f, ctx)).collect(),
        indexes: t.indexes.iter().map(|i| index(i, ctx)).collect(),
        examples: t.examples.iter().map(|e| example_row(e, ctx)).collect(),
        moved_from: t.moved_from.clone(),
        extensions: t.extensions.clone(),
        span: t.span.into(),
        location: ctx.locate(t.file, t.span),
    }
}

/// An enum entry's span is always in its owning `Enum`'s own file – entries
/// are declared directly in the `Enum` block, never inlined from elsewhere
/// the way `use`d `Partial` fields are.
fn enum_entry(e: &model::EnumEntry, owner_file: FileId, ctx: &LocateCtx<'_>) -> ExportedEnumEntry {
    ExportedEnumEntry {
        name: e.name.clone(),
        label: e.label.clone(),
        value: e.value.clone(),
        extensions: e.extensions.clone(),
        span: e.span.into(),
        location: ctx.locate(owner_file, e.span),
    }
}

fn enum_(e: &model::Enum, ctx: &LocateCtx<'_>) -> ExportedEnum {
    ExportedEnum {
        file: ctx.path(e.file),
        namespace: e.namespace.clone(),
        name: e.name.clone(),
        stored_type: e.stored_type.clone(),
        default: e.default.clone(),
        note: e.note.clone(),
        entries: e
            .entries
            .iter()
            .map(|en| enum_entry(en, e.file, ctx))
            .collect(),
        extensions: e.extensions.clone(),
        span: e.span.into(),
        location: ctx.locate(e.file, e.span),
    }
}

/// Same reasoning as [`enum_entry`]: a `Group` member is a plain reference
/// (`Table Customer;`), always written in the group's own file.
fn group_member(
    m: &model::GroupMember,
    owner_file: FileId,
    ctx: &LocateCtx<'_>,
) -> ExportedGroupMember {
    ExportedGroupMember {
        kind: m.kind.clone(),
        namespace: m.namespace.clone(),
        name: m.name.clone(),
        span: m.span.into(),
        location: ctx.locate(owner_file, m.span),
    }
}

fn group(g: &model::Group, ctx: &LocateCtx<'_>) -> ExportedGroup {
    ExportedGroup {
        file: ctx.path(g.file),
        name: g.name.clone(),
        members: g
            .members
            .iter()
            .map(|m| group_member(m, g.file, ctx))
            .collect(),
        extensions: g.extensions.clone(),
        span: g.span.into(),
        location: ctx.locate(g.file, g.span),
    }
}

fn document(d: &model::Document, ctx: &LocateCtx<'_>) -> ExportedDocument {
    ExportedDocument {
        file: ctx.path(d.file),
        namespace: d.namespace.clone(),
        extensions: d.extensions.clone(),
        span: d.span.into(),
        location: ctx.locate(d.file, d.span),
    }
}

fn datatype_param(p: &model::DatatypeParam) -> ExportedDatatypeParam {
    ExportedDatatypeParam {
        name: p.name.clone(),
        ty: p.ty.clone(),
        default: p.default.clone(),
        note: p.note.clone(),
    }
}

fn datatype(d: &model::Datatype, ctx: &LocateCtx<'_>) -> ExportedDatatype {
    ExportedDatatype {
        file: ctx.path(d.file),
        namespace: d.namespace.clone(),
        name: d.name.clone(),
        params: d.params.iter().map(datatype_param).collect(),
        base_type: d.base_type.clone(),
        base_type_args: d.base_type_args.clone(),
        base_datatype: d.base_datatype.as_ref().map(decl_ref),
        resolved_base_type: d.resolved_base_type.clone(),
        default: d.default.clone(),
        regex: d.regex.clone(),
        note: d.note.clone(),
        extensions: d.extensions.clone(),
        span: d.span.into(),
        location: ctx.locate(d.file, d.span),
    }
}

/// For the one case where there is no analysis to export at all (no root
/// file), so the result shape stays the same instead of the model field
/// becoming nullable for every caller.
pub fn empty_model() -> ExportedModel {
    ExportedModel {
        tables: Vec::new(),
        enums: Vec::new(),
        groups: Vec::new(),
        documents: Vec::new(),
        datatypes: Vec::new(),
        dialect: None,
    }
}

pub fn export_model(model: &model::Model, ctx: &LocateCtx<'_>) -> ExportedModel {
    ExportedModel {
        tables: model.tables.iter().map(|t| table(t, ctx)).collect(),
        enums: model.enums.iter().map(|e| enum_(e, ctx)).collect(),
        groups: model.groups.iter().map(|g| group(g, ctx)).collect(),
        documents: model.documents.iter().map(|d| document(d, ctx)).collect(),
        datatypes: model.datatypes.iter().map(|d| datatype(d, ctx)).collect(),
        dialect: model.dialect.clone(),
    }
}

#[cfg(test)]
mod tests {
    const SRC: &str = r#"
spec 1;
Document shop { }
Project { mode = draft; dialect = generic; }

Table Post {
  id    int [pk];
  title varchar(200) [required];

  Index { (title) [unique]; }

  Example {
    Record { id = 1; title = "Hello World"; }
    Record { id = 2; title = "Second post"; }
  }
}
"#;

    #[test]
    fn a_tables_example_rows_round_trip_through_export() {
        let result = crate::export_model(SRC, None, false);
        assert!(result.ok, "{:#?}", result.diagnostics);

        let post = result
            .model
            .tables
            .iter()
            .find(|t| t.name == "Post")
            .expect("Table Post");
        assert_eq!(post.examples.len(), 2);

        let first = &post.examples[0];
        let title = first
            .values
            .iter()
            .find(|f| f.name == "title")
            .expect("first row has a title");
        assert!(matches!(&title.value, crate::FlagValue::Text(t) if t == "Hello World"));
    }

    #[test]
    fn a_table_without_an_example_block_exports_an_empty_list() {
        let src = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; }\nTable T { id int [pk]; }\n";
        let result = crate::export_model(src, None, false);
        assert!(result.ok, "{:#?}", result.diagnostics);
        assert!(result.model.tables[0].examples.is_empty());
    }

    #[test]
    fn a_generic_project_exports_no_dialect() {
        let result = crate::export_model(SRC, None, false);
        assert!(result.ok, "{:#?}", result.diagnostics);
        assert_eq!(result.model.dialect, None);
    }

    #[test]
    fn a_postgres_project_exports_its_engine_name_as_the_dialect() {
        let src = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = postgres; }\nTable T { id int [pk]; data jsonb; }\n";
        let result = crate::export_model(src, None, false);
        assert!(result.ok, "{:#?}", result.diagnostics);
        assert_eq!(result.model.dialect.as_deref(), Some("postgres"));
    }

    #[test]
    fn a_tables_moved_from_round_trips_through_export() {
        let src = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; }\nTable T { id int [pk]; Meta { movedFrom = \"OldName\"; } }\n";
        let result = crate::export_model(src, None, false);
        assert!(result.ok, "{:#?}", result.diagnostics);
        assert_eq!(
            result.model.tables[0].moved_from.as_deref(),
            Some("OldName")
        );
    }

    #[test]
    fn a_table_without_moved_from_exports_none() {
        let result = crate::export_model(SRC, None, false);
        assert!(result.ok, "{:#?}", result.diagnostics);
        assert_eq!(result.model.tables[0].moved_from, None);
    }
}
