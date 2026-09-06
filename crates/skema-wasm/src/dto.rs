//! JS-facing DTOs mirroring `skema_hir::model` and the diagnostic types,
//! plus the plain conversion functions from the core's types to these.
//!
//! Deliberately a 1:1 mirror rather than re-exporting `skema_hir::model::*`
//! directly: `tsify` only emits a real (non-`any`) TypeScript type for a
//! field whose own type also derives it, and pulling `tsify`/`wasm-bindgen`
//! into `skema-hir` would cross that crate's explicit "stay platform-
//! neutral, minimal deps" boundary – it has no concept of a display
//! filename or a UTF-16 position at all. Conversion is done with plain
//! functions rather than `From` impls, since turning a `span` into an
//! optional human `location` needs more context (the source text, the
//! filename, whether locations were even requested) than a bare `&T` can
//! carry.

use serde::Serialize;
use skema_hir::model;
use tsify_next::Tsify;

use crate::location::{self, Location};

/// What every conversion function needs to turn a byte-offset `span` into
/// an optional human [`Location`].
pub struct LocateCtx<'a> {
    pub src: &'a str,
    pub filename: &'a str,
    pub with_locations: bool,
}

impl LocateCtx<'_> {
    fn locate(&self, span: skema_syntax::Span) -> Option<Location> {
        self.with_locations
            .then(|| location::locate(self.src, self.filename, span))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Tsify)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Tsify)]
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

/// Unlike the model entities below, a diagnostic always gets a `location`:
/// the list is always small, and a diagnostic without a human position is
/// nearly useless to an editor. `withLocations` only gates the (potentially
/// large) per-entity `location` field on the model.
#[derive(Debug, Clone, Serialize, Tsify)]
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

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct CheckResult {
    pub ok: bool,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct ExportResult {
    pub ok: bool,
    pub model: ExportedModel,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct ExportedModel {
    pub tables: Vec<ExportedTable>,
    pub enums: Vec<ExportedEnum>,
    pub groups: Vec<ExportedGroup>,
}

/// A field's or index's `[]`-flags, mirroring `model::Flags`: a valueless
/// flag (`[pk]`) maps to `true`, a valued one to its stringified value.
pub type Flags = std::collections::BTreeMap<String, FlagValue>;

#[derive(Debug, Clone, Serialize, Tsify)]
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

fn flags(flags: &model::Flags) -> Flags {
    flags.iter().map(|(k, v)| (k.clone(), v.into())).collect()
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct ExportedTable {
    pub namespace: String,
    pub name: String,
    pub physical: ExportedPhysical,
    pub fields: Vec<ExportedField>,
    pub indexes: Vec<ExportedIndex>,
    pub span: Span,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct ExportedField {
    pub name: String,
    pub ty: ExportedTypeRef,
    pub flags: Flags,
    pub relation: Option<ExportedRelation>,
    pub note: Option<String>,
    pub span: Span,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ExportedTypeRef {
    pub name: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct ExportedRelation {
    pub kind: RelationKind,
    pub target_table: String,
    pub target_field: String,
    pub on_delete: Option<String>,
    pub on_update: Option<String>,
    pub span: Span,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Tsify)]
pub enum RelationKind {
    OneToMany,
    ManyToOne,
    OneToOne,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ExportedIndex {
    pub name: Option<String>,
    pub columns: Vec<String>,
    pub flags: Flags,
    pub span: Span,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct ExportedEnum {
    pub namespace: String,
    pub name: String,
    pub stored_type: String,
    pub entries: Vec<ExportedEnumEntry>,
    pub span: Span,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ExportedEnumEntry {
    pub name: String,
    pub label: Option<String>,
    pub value: Option<String>,
    pub span: Span,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ExportedGroup {
    pub name: String,
    pub members: Vec<ExportedGroupMember>,
    pub span: Span,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ExportedGroupMember {
    pub kind: String,
    pub namespace: String,
    pub name: String,
    pub span: Span,
    pub location: Option<Location>,
}

/// Physical names after the `Project` → `Document` → `Table` cascade. No
/// span: it's a cascade of up to three independent source blocks, so there
/// is no single correct source location for it – inventing one would be
/// actively misleading.
#[derive(Debug, Clone, Serialize, Tsify)]
pub struct ExportedPhysical {
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table: Option<String>,
}

fn type_ref(t: &model::TypeRef) -> ExportedTypeRef {
    ExportedTypeRef {
        name: t.name.clone(),
        args: t.args.clone(),
    }
}

fn physical(p: &model::Physical) -> ExportedPhysical {
    ExportedPhysical {
        database: p.database.clone(),
        schema: p.schema.clone(),
        table: p.table.clone(),
    }
}

fn relation(r: &model::Relation, ctx: &LocateCtx<'_>) -> ExportedRelation {
    ExportedRelation {
        kind: match r.kind {
            model::RelationKind::OneToMany => RelationKind::OneToMany,
            model::RelationKind::ManyToOne => RelationKind::ManyToOne,
            model::RelationKind::OneToOne => RelationKind::OneToOne,
        },
        target_table: r.target_table.clone(),
        target_field: r.target_field.clone(),
        on_delete: r.on_delete.clone(),
        on_update: r.on_update.clone(),
        span: r.span.into(),
        location: ctx.locate(r.span),
    }
}

fn field(f: &model::Field, ctx: &LocateCtx<'_>) -> ExportedField {
    ExportedField {
        name: f.name.clone(),
        ty: type_ref(&f.ty),
        flags: flags(&f.flags),
        relation: f.relation.as_ref().map(|r| relation(r, ctx)),
        note: f.note.clone(),
        span: f.span.into(),
        location: ctx.locate(f.span),
    }
}

fn index(i: &model::Index, ctx: &LocateCtx<'_>) -> ExportedIndex {
    ExportedIndex {
        name: i.name.clone(),
        columns: i.columns.clone(),
        flags: flags(&i.flags),
        span: i.span.into(),
        location: ctx.locate(i.span),
    }
}

fn table(t: &model::Table, ctx: &LocateCtx<'_>) -> ExportedTable {
    ExportedTable {
        namespace: t.namespace.clone(),
        name: t.name.clone(),
        physical: physical(&t.physical),
        fields: t.fields.iter().map(|f| field(f, ctx)).collect(),
        indexes: t.indexes.iter().map(|i| index(i, ctx)).collect(),
        span: t.span.into(),
        location: ctx.locate(t.span),
    }
}

fn enum_entry(e: &model::EnumEntry, ctx: &LocateCtx<'_>) -> ExportedEnumEntry {
    ExportedEnumEntry {
        name: e.name.clone(),
        label: e.label.clone(),
        value: e.value.clone(),
        span: e.span.into(),
        location: ctx.locate(e.span),
    }
}

fn enum_(e: &model::Enum, ctx: &LocateCtx<'_>) -> ExportedEnum {
    ExportedEnum {
        namespace: e.namespace.clone(),
        name: e.name.clone(),
        stored_type: e.stored_type.clone(),
        entries: e.entries.iter().map(|en| enum_entry(en, ctx)).collect(),
        span: e.span.into(),
        location: ctx.locate(e.span),
    }
}

fn group_member(m: &model::GroupMember, ctx: &LocateCtx<'_>) -> ExportedGroupMember {
    ExportedGroupMember {
        kind: m.kind.clone(),
        namespace: m.namespace.clone(),
        name: m.name.clone(),
        span: m.span.into(),
        location: ctx.locate(m.span),
    }
}

fn group(g: &model::Group, ctx: &LocateCtx<'_>) -> ExportedGroup {
    ExportedGroup {
        name: g.name.clone(),
        members: g.members.iter().map(|m| group_member(m, ctx)).collect(),
        span: g.span.into(),
        location: ctx.locate(g.span),
    }
}

pub fn export_model(model: &model::Model, ctx: &LocateCtx<'_>) -> ExportedModel {
    ExportedModel {
        tables: model.tables.iter().map(|t| table(t, ctx)).collect(),
        enums: model.enums.iter().map(|e| enum_(e, ctx)).collect(),
        groups: model.groups.iter().map(|g| group(g, ctx)).collect(),
    }
}
