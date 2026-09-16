//! Builds the exportable model: `use` partials inlined, relationships and
//! types resolved, physical names cascaded (`docs/05`–`docs/11`).

use std::collections::{BTreeSet, HashMap, HashSet};

use skema_syntax::Span;
use skema_syntax::ast::{self, BlockKind, RelationOp, ValueKind};

use super::{Ctx, Dialect, Mode, ProjectSettings};
use crate::catalog::{
    self, BUILTIN_TYPE_PARAMS, BUILTIN_TYPES, DRAFT_TYPES, RELATION_ACTION_VALUES, TypeParamSlot,
    TypeParams,
};
use crate::model;
use crate::{Diagnostic, FileId};

/// Bundles what every resolution function needs, so the signatures don't
/// drown under the sheer parameter count.
struct Env<'c, 'a> {
    ctx: &'c Ctx<'a>,
    dialect: Dialect,
    mode: Mode,
}

/// The fields and indexes of a `Table` or a `Partial`, after every `use`
/// has been inlined at its spot (`docs/09`).
///
/// Every entry carries the file it's written in – diagnostics **and** name
/// resolution follow that file, not the table that ends up using it (via
/// `use`).
#[derive(Default)]
struct Flattened<'a> {
    fields: Vec<(FileId, Span, &'a ast::Field)>,
    indexes: Vec<(FileId, Span, &'a ast::IndexEntry)>,
    /// One entry per `Record` block directly inside the table's own
    /// `Example` block – `Example` is `Table`-only (`docs/05`), so unlike
    /// `fields`/`indexes` this never comes from an inlined `use` `Partial`.
    examples: Vec<(FileId, Span, &'a ast::Block)>,
}

pub(crate) fn run<'a>(
    ctx: &Ctx<'a>,
    project: Option<&ProjectSettings<'a>>,
    links: &mut Vec<model::ReferenceLink>,
    diags: &mut Vec<Diagnostic>,
) -> (Vec<model::Table>, Vec<model::Enum>) {
    let env = Env {
        ctx,
        dialect: project.map_or(Dialect::Generic, |p| p.dialect),
        mode: project.map_or(Mode::Draft, |p| p.mode),
    };
    let project_database = project.and_then(|p| p.database);

    let mut flattened: HashMap<(FileId, &'a str), Flattened<'a>> = HashMap::new();
    for file in &ctx.facts {
        for (&name, decl) in &file.declarations {
            if matches!(decl.kind, BlockKind::Table | BlockKind::Partial) {
                let mut stack = Vec::new();
                let flat = flatten_items(ctx, file.id, &decl.block.items, &mut stack, links, diags);
                flattened.insert((file.id, name), flat);
            }
        }
    }

    let mut tables = Vec::new();
    for file in &ctx.facts {
        for (&name, decl) in &file.declarations {
            if decl.kind != BlockKind::Table {
                continue;
            }
            let Some(flat) = flattened.get(&(file.id, name)) else {
                continue;
            };
            let fields = flat
                .fields
                .iter()
                .map(|&(owner, span, f)| {
                    resolve_field(&env, owner, span, f, &flattened, links, diags)
                })
                .collect();
            let indexes = build_indexes(flat, diags);
            let examples = build_examples(&env, flat, diags);
            let document_database = file
                .document
                .and_then(|(_, _, b)| b.child(BlockKind::Database));
            let table_database = decl.block.child(BlockKind::Database);
            let physical = resolve_physical(project_database, document_database, table_database);
            let namespace = file
                .document
                .map_or_else(String::new, |(n, ..)| n.to_owned());
            let moved_from = resolve_moved_from(file.id, decl.block.child(BlockKind::Meta), diags);
            tables.push(model::Table {
                file: file.id,
                namespace,
                name: name.to_owned(),
                physical,
                fields,
                indexes,
                examples,
                moved_from,
                span: decl.item_span,
            });
        }
    }
    tables.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));

    let mut enums = Vec::new();
    for file in &ctx.facts {
        for (&name, decl) in &file.declarations {
            if decl.kind != BlockKind::Enum {
                continue;
            }
            enums.push(resolve_enum(file, name, decl.block, decl.item_span, diags));
        }
    }
    enums.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));

    (tables, enums)
}

// ===========================================================================
// Inlining `use` partials
// ===========================================================================

fn flatten_partial<'a>(
    ctx: &Ctx<'a>,
    owner: FileId,
    name: &str,
    name_span: Span,
    stack: &mut Vec<(FileId, String)>,
    links: &mut Vec<model::ReferenceLink>,
    diags: &mut Vec<Diagnostic>,
) -> Flattened<'a> {
    let key = (owner, name.to_owned());
    if stack.contains(&key) {
        diags.push(Diagnostic::error(
            owner,
            name_span,
            format!("cycle in `use`: `{name}` uses itself, directly or indirectly"),
        ));
        return Flattened::default();
    }
    let Some(decl) = ctx.lookup_decl(owner, name) else {
        diags.push(Diagnostic::error(
            owner,
            name_span,
            format!("unknown `Partial` `{name}`"),
        ));
        return Flattened::default();
    };
    if decl.kind != BlockKind::Partial {
        diags.push(Diagnostic::error(
            owner,
            name_span,
            format!(
                "`use` expects a `Partial`, `{name}` is a `{}`",
                decl.kind.as_str()
            ),
        ));
        return Flattened::default();
    }

    stack.push(key);
    let result = flatten_items(ctx, owner, &decl.block.items, stack, links, diags);
    stack.pop();
    result
}

fn flatten_items<'a>(
    ctx: &Ctx<'a>,
    owner: FileId,
    items: &'a [ast::Item],
    stack: &mut Vec<(FileId, String)>,
    links: &mut Vec<model::ReferenceLink>,
    diags: &mut Vec<Diagnostic>,
) -> Flattened<'a> {
    let mut out = Flattened::default();
    for item in items {
        match &item.kind {
            ast::ItemKind::Use(u) => {
                let Some((target_owner, name, name_span, rest)) =
                    ctx.resolve_decl_path(owner, &u.target)
                else {
                    continue;
                };
                if !rest.is_empty() {
                    diags.push(Diagnostic::error(
                        owner,
                        u.target.span,
                        "`use` names only a `Partial`, no further path after it",
                    ));
                }
                if let Some(decl) = ctx.lookup_decl(target_owner, name) {
                    if decl.kind == BlockKind::Partial {
                        links.push(model::ReferenceLink {
                            from_file: owner,
                            from_span: name_span,
                            to_file: target_owner,
                            to_span: decl.name_span,
                        });
                    }
                }
                let inner =
                    flatten_partial(ctx, target_owner, name, name_span, stack, links, diags);
                for field in inner.fields {
                    push_field(&mut out, field, diags);
                }
                out.indexes.extend(inner.indexes);
            }
            ast::ItemKind::Field(f) => push_field(&mut out, (owner, item.span, f), diags),
            ast::ItemKind::Block(b) if b.kind == BlockKind::Index => {
                for sub in &b.items {
                    if let ast::ItemKind::IndexEntry(e) = &sub.kind {
                        out.indexes.push((owner, sub.span, e));
                    }
                }
            }
            ast::ItemKind::Block(b) if b.kind == BlockKind::Example => {
                for sub in &b.items {
                    if let ast::ItemKind::Block(record) = &sub.kind {
                        if record.kind == BlockKind::Record {
                            out.examples.push((owner, sub.span, record));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn push_field<'a>(
    out: &mut Flattened<'a>,
    field: (FileId, Span, &'a ast::Field),
    diags: &mut Vec<Diagnostic>,
) {
    let (owner, span, f) = field;
    if let Some((_, _, existing)) = out
        .fields
        .iter()
        .find(|(_, _, e)| e.name.text == f.name.text)
    {
        diags.push(Diagnostic::error(
            owner,
            f.name.span,
            format!(
                "field name `{}` is already taken (at {:?}) – from `use` or an earlier field (docs/09)",
                f.name.text, existing.name.span
            ),
        ));
        return;
    }
    out.fields.push((owner, span, f));
}

// ===========================================================================
// Indexes
// ===========================================================================

fn build_indexes(flat: &Flattened<'_>, diags: &mut Vec<Diagnostic>) -> Vec<model::Index> {
    flat.indexes
        .iter()
        .map(|&(owner, span, e)| {
            for c in &e.columns {
                if !flat.fields.iter().any(|(_, _, f)| f.name.text == c.text) {
                    diags.push(Diagnostic::error(
                        owner,
                        c.span,
                        format!("index refers to unknown column `{}`", c.text),
                    ));
                }
            }
            model::Index {
                file: owner,
                name: e.name.as_ref().map(|n| n.text.clone()),
                columns: e.columns.iter().map(|c| c.text.clone()).collect(),
                flags: build_flags(&e.flags),
                span,
            }
        })
        .collect()
}

// ===========================================================================
// Example rows
// ===========================================================================

/// Structural and value-type validation for `Table`'s `Example` block
/// (`docs/05`): unknown field names, missing `required`/`pk` fields,
/// `pk`/`unique` value collisions across a table's own rows, and (task 12)
/// each value's literal kind against its field's resolved type.
fn build_examples(
    env: &Env<'_, '_>,
    flat: &Flattened<'_>,
    diags: &mut Vec<Diagnostic>,
) -> Vec<model::ExampleRow> {
    let field_types: Vec<(&str, Option<&str>)> = flat
        .fields
        .iter()
        .map(|&(owner, _, f)| {
            (
                f.name.text.as_str(),
                example_field_terminal_type(env, owner, f),
            )
        })
        .collect();

    let rows: Vec<model::ExampleRow> = flat
        .examples
        .iter()
        .map(|&(owner, span, record)| {
            let attrs: Vec<ast::Attr> = record
                .items
                .iter()
                .filter_map(|item| match &item.kind {
                    ast::ItemKind::Setting(a) => Some(a.clone()),
                    _ => None,
                })
                .collect();
            for attr in &attrs {
                if !flat
                    .fields
                    .iter()
                    .any(|(_, _, f)| f.name.text == attr.key.text)
                {
                    diags.push(Diagnostic::error(
                        owner,
                        attr.key.span,
                        format!(
                            "`Example` row assigns unknown field `{}` (docs/05)",
                            attr.key.text
                        ),
                    ));
                    continue;
                }
                let Some(value) = &attr.value else { continue };
                let Some(expected_ty) = field_types
                    .iter()
                    .find(|(name, _)| *name == attr.key.text)
                    .and_then(|&(_, ty)| ty)
                else {
                    continue;
                };
                if !example_value_matches_type(value, expected_ty) {
                    diags.push(Diagnostic::error(
                        owner,
                        value.span,
                        format!(
                            "`Example` row's `{}` should be `{expected_ty}`, got `{}`",
                            attr.key.text,
                            value.to_text()
                        ),
                    ));
                }
            }
            model::ExampleRow {
                file: owner,
                values: build_flags(&attrs),
                span,
            }
        })
        .collect();

    let required: Vec<&str> = flat
        .fields
        .iter()
        .filter(|(_, _, f)| {
            f.flags
                .iter()
                .any(|a| matches!(a.key.text.as_str(), "required" | "pk"))
        })
        .map(|(_, _, f)| f.name.text.as_str())
        .collect();
    for (i, row) in rows.iter().enumerate() {
        let (owner, span, _) = flat.examples[i];
        for &name in &required {
            if !row.values.contains_key(name) {
                diags.push(Diagnostic::error(
                    owner,
                    span,
                    format!(
                        "record {} in `Example` is missing required field `{name}`",
                        i + 1
                    ),
                ));
            }
        }
    }

    let collision_checked: Vec<&str> = flat
        .fields
        .iter()
        .filter(|(_, _, f)| {
            f.flags
                .iter()
                .any(|a| matches!(a.key.text.as_str(), "pk" | "unique"))
        })
        .map(|(_, _, f)| f.name.text.as_str())
        .collect();
    for &name in &collision_checked {
        let mut seen: Vec<(usize, &model::FlagValue)> = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            let Some(value) = row.values.get(name) else {
                continue;
            };
            if let Some(&(first, _)) = seen.iter().find(|&&(_, v)| v == value) {
                let (owner, span, _) = flat.examples[i];
                diags.push(Diagnostic::error(
                    owner,
                    span,
                    format!(
                        "record {} in `Example` repeats record {}'s value for `{name}` – `pk`/`unique` fields must be distinct across a table's example rows",
                        i + 1,
                        first + 1
                    ),
                ));
            } else {
                seen.push((i, value));
            }
        }
    }

    rows
}

/// The built-in type name an `Example` value should be checked against, or
/// `None` if this field's type doesn't resolve to one – a relationship
/// field, an `Enum` (task 12 covers built-ins and `Datatype`s only), or a
/// name this rule can't resolve. Left unchecked rather than risking a false
/// positive (`docs/13` "Not enforced yet" already covers the gap this
/// leaves).
///
/// A `Datatype`-typed field resolves one level through to that `Datatype`'s
/// own `Field.type` (`docs/06`); a `Datatype` naming another custom type
/// there is not followed further – out of scope for this task.
fn example_field_terminal_type<'a>(
    env: &Env<'_, 'a>,
    owner: FileId,
    field: &'a ast::Field,
) -> Option<&'a str> {
    let ty = field.ty.as_ref()?;
    let ast::FieldTypeKind::Named { path, .. } = &ty.kind else {
        return None;
    };
    if let Some(bare) = path.as_ident() {
        if BUILTIN_TYPES.contains(&bare.text.as_str()) {
            return Some(bare.text.as_str());
        }
    }
    let (decl_owner, name, _, rest) = env.ctx.resolve_decl_path(owner, path)?;
    if !rest.is_empty() {
        return None;
    }
    let decl = env.ctx.lookup_decl(decl_owner, name)?;
    if decl.kind != BlockKind::Datatype {
        return None;
    }
    datatype_field_type(decl.block).filter(|word| BUILTIN_TYPES.contains(word))
}

/// Reads `Field { type = …; }` inside a `Datatype` declaration – the same
/// shape `resolve_enum` reads for `Enum { Field { type = …; } }`, but
/// without validating the word (an `Enum`'s storage is closed to
/// `string`/`int`; a `Datatype`'s is any type name, checked elsewhere).
fn datatype_field_type(datatype: &ast::Block) -> Option<&str> {
    let field_block = datatype.child(BlockKind::Field)?;
    field_block.items.iter().find_map(|item| match &item.kind {
        ast::ItemKind::Setting(a) if a.key.text == "type" => match &a.value.as_ref()?.kind {
            ValueKind::Path(p) => p.as_ident().map(|i| i.text.as_str()),
            _ => None,
        },
        _ => None,
    })
}

/// Whether an `Example` value's literal kind is compatible with
/// `expected_ty`, a resolved built-in name from [`BUILTIN_TYPES`] (task 12).
///
/// A raw expression (`` `…` ``) is accepted unconditionally for any type –
/// opaque SQL passed through untouched by design (`docs/01`), the same call
/// `check_arg_type` already makes for a `Datatype`/built-in argument.
fn example_value_matches_type(value: &ast::Value, expected_ty: &str) -> bool {
    match (&value.kind, expected_ty) {
        (ValueKind::Raw(_), _) => true,
        (ValueKind::Int(_), "int") => true,
        (ValueKind::Int(_) | ValueKind::Float(_), "float") => true,
        (ValueKind::Str(_), "string" | "varchar" | "text" | "uuid" | "date" | "timestamp") => true,
        (ValueKind::Path(p), "bool") => {
            matches!(
                p.as_ident().map(|i| i.text.as_str()),
                Some("true" | "false")
            )
        }
        _ => false,
    }
}

// ===========================================================================
// Fields, types, relationships
// ===========================================================================

fn resolve_field<'a>(
    env: &Env<'_, 'a>,
    owner: FileId,
    span: Span,
    f: &'a ast::Field,
    flattened: &HashMap<(FileId, &'a str), Flattened<'a>>,
    links: &mut Vec<model::ReferenceLink>,
    diags: &mut Vec<Diagnostic>,
) -> model::Field {
    let mut flags = build_flags(&f.flags);
    // `onDelete`/`onUpdate` are relation metadata, surfaced on `Relation`
    // below (via `resolve_relation`) instead – not one of a field's own
    // flags, regardless of whether this field turns out to be a relation.
    flags.remove("onDelete");
    flags.remove("onUpdate");
    // `pk` implies `required` and `unique` (docs/05).
    if flags.contains_key("pk") {
        flags.insert("required".to_owned(), model::FlagValue::Bool(true));
        flags.insert("unique".to_owned(), model::FlagValue::Bool(true));
    }

    let (ty, relation) = match &f.ty {
        Some(t) => match &t.kind {
            ast::FieldTypeKind::Relation { op, target } => {
                let site = RelationSite {
                    op: *op,
                    target,
                    flags: &f.flags,
                    span: t.span,
                };
                let relation = resolve_relation(env, owner, site, flattened, links, diags);
                let ty = match &relation {
                    Some(r) => model::TypeRef {
                        name: format!("{}.{}", r.target_table, r.target_field),
                        args: Vec::new(),
                    },
                    None => unknown_type(),
                };
                (ty, relation)
            }
            ast::FieldTypeKind::Named { path, args } => (
                resolve_named_type(env, owner, path, args, t.span, links, diags),
                None,
            ),
        },
        None => (unknown_type(), None),
    };

    let note = string_setting(&f.items, "note");
    let label = string_setting(&f.items, "label");

    model::Field {
        file: owner,
        name: f.name.text.clone(),
        ty,
        flags,
        relation,
        note,
        label,
        span,
    }
}

fn unknown_type() -> model::TypeRef {
    model::TypeRef {
        name: "unknown".to_owned(),
        args: Vec::new(),
    }
}

/// Bundles the parts of a `>`/`</`-` relation site that come straight off
/// the AST, so [`resolve_relation`] doesn't drown under the sheer parameter
/// count (the same reasoning as [`Env`]).
struct RelationSite<'a> {
    op: RelationOp,
    target: &'a ast::Path,
    flags: &'a [ast::Attr],
    /// The span of the relation's [`ast::FieldType`] itself (e.g.
    /// `>Customer.id`) – more precise than the owning field's span.
    span: Span,
}

fn resolve_relation<'a>(
    env: &Env<'_, 'a>,
    owner: FileId,
    site: RelationSite<'a>,
    flattened: &HashMap<(FileId, &'a str), Flattened<'a>>,
    links: &mut Vec<model::ReferenceLink>,
    diags: &mut Vec<Diagnostic>,
) -> Option<model::Relation> {
    let RelationSite {
        op,
        target,
        flags,
        span,
    } = site;
    let (table_owner, table_name, table_span, rest) = env.ctx.resolve_decl_path(owner, target)?;
    if rest.len() != 1 {
        diags.push(Diagnostic::error(
            owner,
            target.span,
            "a relationship target always names a table and a field, e.g. `Customer.id` (docs/07)",
        ));
        return None;
    }
    let field_name = rest[0].text.as_str();

    let Some(decl) = env.ctx.lookup_decl(table_owner, table_name) else {
        diags.push(Diagnostic::error(
            owner,
            table_span,
            format!("unknown table `{table_name}`"),
        ));
        return None;
    };
    if decl.kind != BlockKind::Table {
        diags.push(Diagnostic::error(
            owner,
            table_span,
            format!(
                "`{table_name}` is not a `Table`, but a `{}`",
                decl.kind.as_str()
            ),
        ));
        return None;
    }
    links.push(model::ReferenceLink {
        from_file: owner,
        from_span: table_span,
        to_file: table_owner,
        to_span: decl.name_span,
    });
    let field_decl = flattened.get(&(table_owner, table_name)).and_then(|flat| {
        flat.fields
            .iter()
            .find(|(_, _, f)| f.name.text == field_name)
    });
    let Some(&(field_owner, _, field)) = field_decl else {
        diags.push(Diagnostic::error(
            owner,
            rest[0].span,
            format!("`{table_name}` has no field `{field_name}`"),
        ));
        return None;
    };
    links.push(model::ReferenceLink {
        from_file: owner,
        from_span: rest[0].span,
        to_file: field_owner,
        to_span: field.name.span,
    });

    let mut on_delete = None;
    let mut on_update = None;
    for attr in flags {
        match attr.key.text.as_str() {
            "onDelete" | "onUpdate" if op != RelationOp::OneToMany => {
                diags.push(Diagnostic::error(
                    owner,
                    attr.key.span,
                    format!(
                        "`{}` is only allowed on the `>` side of a relationship (docs/07, \"Flags are written only on the > side\")",
                        attr.key.text
                    ),
                ));
            }
            "onDelete" => {
                on_delete = check_relation_action(owner, &attr.key.text, attr.value.as_ref(), diags)
            }
            "onUpdate" => {
                on_update = check_relation_action(owner, &attr.key.text, attr.value.as_ref(), diags)
            }
            _ => {}
        }
    }

    Some(model::Relation {
        kind: match op {
            RelationOp::OneToMany => model::RelationKind::OneToMany,
            RelationOp::ManyToOne => model::RelationKind::ManyToOne,
            RelationOp::OneToOne => model::RelationKind::OneToOne,
        },
        target_namespace: env
            .ctx
            .file(table_owner)
            .and_then(|f| f.document)
            .map_or_else(String::new, |(name, ..)| name.to_owned()),
        target_table: table_name.to_owned(),
        target_field: field_name.to_owned(),
        on_delete,
        on_update,
        span,
    })
}

/// Checks an `onDelete`/`onUpdate` value against `RELATION_ACTION_VALUES`
/// (`docs/07`, "Flags"). Stored as written either way – errors are data.
fn check_relation_action(
    owner: FileId,
    key: &str,
    value: Option<&ast::Value>,
    diags: &mut Vec<Diagnostic>,
) -> Option<String> {
    let value = value?;
    let text = value.to_text();
    if !RELATION_ACTION_VALUES.contains(&text.as_str()) {
        diags.push(Diagnostic::error(
            owner,
            value.span,
            format!(
                "unknown `{key}` value `{text}` – allowed: {}",
                RELATION_ACTION_VALUES.join(", ")
            ),
        ));
    }
    Some(text)
}

/// Resolves a `Table`'s `Meta.movedFrom` (`docs/03`) – `rules::keys` already
/// confirmed the key is only legal here; this checks its value is a string,
/// same as `author`/`note`/`version`.
fn resolve_moved_from(
    owner: FileId,
    meta: Option<&ast::Block>,
    diags: &mut Vec<Diagnostic>,
) -> Option<String> {
    let attr = meta?.items.iter().find_map(|item| match &item.kind {
        ast::ItemKind::Setting(a) if a.key.text == "movedFrom" => Some(a),
        _ => None,
    })?;
    match attr.value.as_ref().map(|v| &v.kind) {
        Some(ValueKind::Str(s)) => Some(s.clone()),
        _ => {
            diags.push(Diagnostic::error(
                owner,
                attr.span,
                "`movedFrom` must be a string, e.g. `movedFrom = \"OldName\";` (docs/03)",
            ));
            None
        }
    }
}

// ===========================================================================
// Circular required foreign keys
// ===========================================================================

/// Identifies a table project-wide – `name` alone is ambiguous (`docs/02`:
/// two files may each declare a `Table Copy`).
type TableKey = (String, String);

fn table_key(table: &model::Table) -> TableKey {
    (table.namespace.clone(), table.name.clone())
}

/// Rejects a cycle of `required`/`pk` foreign keys: every row in such a
/// cycle needs a not-yet-existing row to already exist first, so no target
/// database has a valid insert order – including the one-table self-loop
/// (`Employee.manager >Employee.id [required]`). Always an error, never
/// gated by `Lint` or `mode` – see `decisions.md`, "Lint catalog expansion
/// (tasks 66–70)". Runs on the whole-project table graph, so it needs
/// `resolve::run`'s output and cannot live inside `resolve_field` itself.
pub(crate) fn check_required_fk_cycles(tables: &[model::Table], diags: &mut Vec<Diagnostic>) {
    // An edge `from -> to` per `required`/`pk` `>`/`-` field – `<`
    // back-references create no column, so they create no edge (docs/07,
    // "Not every relationship creates a column").
    let mut adjacency: HashMap<TableKey, Vec<TableKey>> = HashMap::new();
    let mut edges: Vec<(TableKey, TableKey, &model::Field)> = Vec::new();
    for table in tables {
        let from = table_key(table);
        for field in &table.fields {
            let Some(relation) = &field.relation else {
                continue;
            };
            if relation.kind == model::RelationKind::ManyToOne {
                continue;
            }
            if !(field.flags.contains_key("required") || field.flags.contains_key("pk")) {
                continue; // optional – can be inserted NULL first, breaks any cycle
            }
            let to = (
                relation.target_namespace.clone(),
                relation.target_table.clone(),
            );
            adjacency.entry(from.clone()).or_default().push(to.clone());
            edges.push((from.clone(), to, field));
        }
    }

    let mut reach_cache: HashMap<TableKey, HashSet<TableKey>> = HashMap::new();
    for (from, to, field) in &edges {
        if !reach(to, &adjacency, &mut reach_cache).contains(from) {
            continue; // `to` never leads back to `from` – no cycle through this edge
        }
        let members = cycle_members(from, &adjacency, &mut reach_cache);
        let names: Vec<&str> = members.iter().map(|(_, name)| name.as_str()).collect();
        diags.push(Diagnostic::error(
            field.file,
            field.span,
            format!(
                "`required` foreign key `{}` closes a cycle among tables: {} \
                 – no row in the cycle could ever be inserted first (docs/07)",
                field.name,
                names.join(", ")
            ),
        ));
    }
}

/// Every node reachable from `start` by following one or more edges –
/// includes `start` itself exactly when some path leads back to it (a
/// cycle through `start`). Memoized because the same `to` is looked up
/// once per field that targets it.
fn reach(
    start: &TableKey,
    adjacency: &HashMap<TableKey, Vec<TableKey>>,
    cache: &mut HashMap<TableKey, HashSet<TableKey>>,
) -> HashSet<TableKey> {
    if let Some(set) = cache.get(start) {
        return set.clone();
    }
    let mut visited = HashSet::new();
    let mut stack: Vec<TableKey> = adjacency.get(start).cloned().unwrap_or_default();
    while let Some(node) = stack.pop() {
        if visited.insert(node.clone()) {
            if let Some(next) = adjacency.get(&node) {
                stack.extend(next.iter().cloned());
            }
        }
    }
    cache.insert(start.clone(), visited.clone());
    visited
}

/// The strongly-connected component `from` belongs to: `from` itself plus
/// every node it can reach that can also reach back to it. Used only to
/// name the cycle in a diagnostic – not full Tarjan/Kosaraju, since we only
/// ever need this for nodes already known to sit on a cycle.
fn cycle_members(
    from: &TableKey,
    adjacency: &HashMap<TableKey, Vec<TableKey>>,
    cache: &mut HashMap<TableKey, HashSet<TableKey>>,
) -> BTreeSet<TableKey> {
    let forward = reach(from, adjacency, cache);
    let mut members: BTreeSet<TableKey> = BTreeSet::new();
    members.insert(from.clone());
    for node in &forward {
        if reach(node, adjacency, cache).contains(from) {
            members.insert(node.clone());
        }
    }
    members
}

fn resolve_named_type(
    env: &Env<'_, '_>,
    owner: FileId,
    path: &ast::Path,
    args: &[ast::Value],
    span: Span,
    links: &mut Vec<model::ReferenceLink>,
    diags: &mut Vec<Diagnostic>,
) -> model::TypeRef {
    let dotted = path.to_dotted();
    let str_args: Vec<String> = args.iter().map(ast::Value::to_text).collect();

    if let Some(bare) = path.as_ident().map(|i| i.text.as_str()) {
        let dialect_entry = env
            .dialect
            .engine_name()
            .and_then(|engine| catalog::dialect_type(engine, bare));
        if BUILTIN_TYPES.contains(&bare) || dialect_entry.is_some() {
            let params = dialect_entry.map_or_else(|| builtin_type_params(bare), |e| e.params);
            check_type_args(owner, bare, &params, args, span, diags);
            if env.mode == Mode::Strict && DRAFT_TYPES.contains(&bare) {
                diags.push(Diagnostic::error(
                    owner,
                    span,
                    format!(
                        "`{bare}` is not precise enough under `mode = strict` – use a more precise built-in, an `Enum`, or a `Datatype` instead (docs/02, \"The mode switch\")"
                    ),
                ));
            }
            return model::TypeRef {
                name: bare.to_owned(),
                args: str_args,
            };
        }
    }

    let Some((decl_owner, name, name_span, rest)) = env.ctx.resolve_decl_path(owner, path) else {
        return model::TypeRef {
            name: dotted,
            args: str_args,
        };
    };
    if !rest.is_empty() {
        diags.push(Diagnostic::error(
            owner,
            span,
            "unexpected path after the type name",
        ));
        return model::TypeRef {
            name: dotted,
            args: str_args,
        };
    }

    let decl = env.ctx.lookup_decl(decl_owner, name);
    if let Some(d) = decl {
        if matches!(d.kind, BlockKind::Enum | BlockKind::Datatype) {
            links.push(model::ReferenceLink {
                from_file: owner,
                from_span: name_span,
                to_file: decl_owner,
                to_span: d.name_span,
            });
        }
    }

    match decl {
        Some(decl) if decl.kind == BlockKind::Enum => model::TypeRef {
            name: dotted,
            args: Vec::new(),
        },
        Some(decl) if decl.kind == BlockKind::Datatype => {
            check_datatype_args(owner, decl.block, args, span, diags);
            check_datatype_dialect_coverage(env, owner, &dotted, decl.block, span, diags);
            model::TypeRef {
                name: dotted,
                args: str_args,
            }
        }
        Some(decl) => {
            diags.push(Diagnostic::error(
                owner,
                name_span,
                format!("`{name}` is not a type, but a `{}`", decl.kind.as_str()),
            ));
            model::TypeRef {
                name: dotted,
                args: str_args,
            }
        }
        None if env.dialect == Dialect::Generic => {
            diags.push(Diagnostic::error(
                owner,
                span,
                format!(
                    "unknown type `{dotted}` – allowed are the built-in types, a custom \
                     `Enum`/`Datatype`, or (with `dialect`) an engine-specific type (docs/02, docs/06)"
                ),
            ));
            model::TypeRef {
                name: dotted,
                args: str_args,
            }
        }
        None => {
            let engine = env.dialect.engine_name().unwrap_or("this dialect");
            diags.push(Diagnostic::error(
                owner,
                span,
                format!(
                    "unknown type `{dotted}` – allowed are the built-in types, `{engine}`'s own \
                     type catalog (docs/06), a custom `Enum`/`Datatype`"
                ),
            ));
            model::TypeRef {
                name: dotted,
                args: str_args,
            }
        }
    }
}

/// `decimal("abc", 2)` is a type error at the model level (`docs/06`) –
/// checked here: argument count, then each supplied argument against its
/// parameter's declared type.
fn check_datatype_args(
    owner: FileId,
    datatype: &ast::Block,
    args: &[ast::Value],
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    let params = &datatype.params;
    if args.len() > params.len() {
        diags.push(Diagnostic::error(
            owner,
            span,
            format!(
                "`{}` expects at most {} argument(s), got: {}",
                datatype
                    .name
                    .as_ref()
                    .map(|n| n.to_dotted())
                    .unwrap_or_default(),
                params.len(),
                args.len()
            ),
        ));
        return;
    }
    for (arg, param) in args.iter().zip(params) {
        if let Some(ty) = param.ty.as_ref().and_then(ast::Path::as_ident) {
            check_arg_type(owner, arg, &ty.text, &param.name.text, diags);
        }
    }
    for param in &params[args.len()..] {
        if !param_has_default(datatype, &param.name.text) {
            diags.push(Diagnostic::error(
                owner,
                span,
                format!(
                    "argument for parameter `{}` is missing – no `default` set",
                    param.name.text
                ),
            ));
        }
    }
}

/// A `Datatype` used under a non-`generic` dialect with no matching
/// `Dialect @<engine>` block (`docs/06`, "Missing dialect"): a hint under
/// `draft` (a `Warning`-severity diagnostic – the same severity
/// `resolve_named_type`'s own `None` arm already uses for "not finalized
/// yet" cases), an error under `strict`.
fn check_datatype_dialect_coverage(
    env: &Env<'_, '_>,
    owner: FileId,
    datatype_name: &str,
    datatype: &ast::Block,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(engine) = env.dialect.engine_name() else {
        return;
    };
    let covered = datatype.items.iter().any(|item| match &item.kind {
        ast::ItemKind::Block(b) if b.kind == BlockKind::Dialect => {
            b.selector.as_ref().is_some_and(|s| s.text == engine)
        }
        _ => false,
    });
    if covered {
        return;
    }
    let message = format!(
        "`{datatype_name}` has no `Dialect @{engine}` block – the type is not defined for this target (docs/06, \"Missing dialect\")"
    );
    match env.mode {
        Mode::Draft => diags.push(Diagnostic::warning(owner, span, message)),
        Mode::Strict => diags.push(Diagnostic::error(owner, span, message)),
    }
}

/// The dialect-independent argument shape of a [`BUILTIN_TYPES`] name – a
/// name not in [`BUILTIN_TYPE_PARAMS`] takes no arguments.
fn builtin_type_params(name: &str) -> TypeParams {
    BUILTIN_TYPE_PARAMS
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(TypeParams::None, |(_, params)| *params)
}

/// Argument check for a built-in type against its catalog shape (`docs/06`,
/// "Built-in types"): a fixed/optional run of typed slots, a variadic
/// string-literal list, or no arguments at all.
fn check_type_args(
    owner: FileId,
    name: &str,
    params: &TypeParams,
    args: &[ast::Value],
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    match params {
        TypeParams::None => {
            if !args.is_empty() {
                diags.push(Diagnostic::error(
                    owner,
                    span,
                    format!("`{name}` expects 0 argument(s), got: {}", args.len()),
                ));
            }
        }
        TypeParams::StringList => {
            if args.is_empty() {
                diags.push(Diagnostic::error(
                    owner,
                    span,
                    format!("`{name}` expects one or more string arguments"),
                ));
                return;
            }
            for arg in args {
                if !matches!(arg.kind, ValueKind::Str(_) | ValueKind::Raw(_)) {
                    diags.push(Diagnostic::error(
                        owner,
                        arg.span,
                        format!(
                            "`{name}`'s arguments must be string literals, got `{}`",
                            arg.to_text()
                        ),
                    ));
                }
            }
        }
        TypeParams::Slots(slots) => {
            if args.len() > slots.len() {
                diags.push(Diagnostic::error(
                    owner,
                    span,
                    format!(
                        "`{name}` expects at most {} argument(s), got: {}",
                        slots.len(),
                        args.len()
                    ),
                ));
                return;
            }
            let required = slots
                .iter()
                .take_while(|s| matches!(s, TypeParamSlot::Int))
                .count();
            if args.len() < required {
                diags.push(Diagnostic::error(
                    owner,
                    span,
                    format!(
                        "`{name}` expects at least {required} argument(s), got: {}",
                        args.len()
                    ),
                ));
                return;
            }
            for (arg, slot) in args.iter().zip(*slots) {
                match slot {
                    TypeParamSlot::Int | TypeParamSlot::OptInt => {
                        check_arg_type(owner, arg, "int", name, diags);
                    }
                    TypeParamSlot::OptOneOf(words) => {
                        check_one_of_arg(owner, arg, words, name, diags);
                    }
                }
            }
        }
    }
}

/// Checks a bare-word argument against a closed set (`vector`'s `format`) –
/// a raw expression (`` `…` ``) is accepted unconditionally, the same
/// convention [`check_arg_type`] follows.
fn check_one_of_arg(
    owner: FileId,
    arg: &ast::Value,
    words: &[&str],
    name: &str,
    diags: &mut Vec<Diagnostic>,
) {
    let ok = match &arg.kind {
        ValueKind::Raw(_) => true,
        ValueKind::Path(p) => p
            .as_ident()
            .is_some_and(|i| words.contains(&i.text.as_str())),
        _ => false,
    };
    if !ok {
        diags.push(Diagnostic::error(
            owner,
            arg.span,
            format!(
                "`{name}`'s argument should be one of {}, got `{}`",
                words.join(", "),
                arg.to_text()
            ),
        ));
    }
}

/// Compares an argument's [`ast::Value`] against a parameter's declared type
/// name. A raw expression (`` `…` ``) is accepted for any type – it passes
/// through to the target database unvalidated by design (`docs/01`). A type
/// name this doesn't recognize (a custom `Enum`/`Datatype` used as a
/// parameter type) is left unchecked – only the four simple types have a
/// decided shape to check argument values against.
fn check_arg_type(
    owner: FileId,
    arg: &ast::Value,
    expected_ty: &str,
    param_name: &str,
    diags: &mut Vec<Diagnostic>,
) {
    let matches = match (&arg.kind, expected_ty) {
        (ValueKind::Raw(_), _) => true,
        (ValueKind::Int(_), "int") => true,
        (ValueKind::Int(_) | ValueKind::Float(_), "float") => true,
        (ValueKind::Str(_), "string") => true,
        (ValueKind::Path(p), "bool") => {
            matches!(
                p.as_ident().map(|i| i.text.as_str()),
                Some("true" | "false")
            )
        }
        (_, "int" | "float" | "string" | "bool") => false,
        _ => return,
    };
    if !matches {
        diags.push(Diagnostic::error(
            owner,
            arg.span,
            format!(
                "argument for parameter `{param_name}` should be `{expected_ty}`, got `{}`",
                arg.to_text()
            ),
        ));
    }
}

fn param_has_default(datatype: &ast::Block, param_name: &str) -> bool {
    datatype.items.iter().any(|item| match &item.kind {
        ast::ItemKind::Field(f) if f.name.text == param_name => f
            .items
            .iter()
            .any(|sub| matches!(&sub.kind, ast::ItemKind::Setting(a) if a.key.text == "default")),
        _ => false,
    })
}

// ===========================================================================
// Enums
// ===========================================================================

fn resolve_enum<'a>(
    file: &crate::symbols::FileFacts<'a>,
    name: &str,
    block: &'a ast::Block,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) -> model::Enum {
    let field_block = block.child(BlockKind::Field);
    let stored_type = field_block
        .and_then(|b| {
            b.items.iter().find_map(|i| match &i.kind {
                ast::ItemKind::Setting(a) if a.key.text == "type" => {
                    a.value.as_ref().and_then(|v| match &v.kind {
                        ValueKind::Path(p) => p.as_ident().map(|i| (i.text.clone(), v.span)),
                        _ => None,
                    })
                }
                _ => None,
            })
        })
        .map(|(word, span)| {
            if word == "string" || word == "int" {
                word
            } else {
                diags.push(Diagnostic::error(
                    file.id,
                    span,
                    format!("`Enum.Field.type` must be `string` or `int`, not `{word}` (docs/08)"),
                ));
                "string".to_owned()
            }
        })
        .unwrap_or_else(|| "string".to_owned());

    let entries = block
        .items
        .iter()
        .filter_map(|item| match &item.kind {
            ast::ItemKind::Field(f) => Some((item.span, f)),
            _ => None,
        })
        .map(|(entry_span, f)| model::EnumEntry {
            name: f.name.text.clone(),
            label: string_setting(&f.items, "label"),
            value: value_setting(&f.items, "value"),
            span: entry_span,
        })
        .collect();

    model::Enum {
        file: file.id,
        namespace: file
            .document
            .map_or_else(String::new, |(n, ..)| n.to_owned()),
        name: name.to_owned(),
        stored_type,
        entries,
        span,
    }
}

// ===========================================================================
// Physical names
// ===========================================================================

fn resolve_physical(
    project: Option<&ast::Block>,
    document: Option<&ast::Block>,
    table: Option<&ast::Block>,
) -> model::Physical {
    model::Physical {
        database: cascade_str([table, document, project], "database"),
        schema: cascade_str([table, document, project], "schema"),
        table: database_str(table, "table"),
    }
}

fn cascade_str(levels: [Option<&ast::Block>; 3], key: &str) -> Option<String> {
    levels.into_iter().find_map(|b| database_str(b, key))
}

fn database_str(database: Option<&ast::Block>, key: &str) -> Option<String> {
    string_setting(&database?.items, key)
}

// ===========================================================================
// Small helpers
// ===========================================================================

fn string_setting(items: &[ast::Item], key: &str) -> Option<String> {
    items.iter().find_map(|item| match &item.kind {
        ast::ItemKind::Setting(a) if a.key.text == key => {
            a.value.as_ref().and_then(|v| match &v.kind {
                ValueKind::Str(s) => Some(s.clone()),
                _ => None,
            })
        }
        _ => None,
    })
}

fn value_setting(items: &[ast::Item], key: &str) -> Option<String> {
    items.iter().find_map(|item| match &item.kind {
        ast::ItemKind::Setting(a) if a.key.text == key => a.value.as_ref().map(ast::Value::to_text),
        _ => None,
    })
}

/// Turns a field's or index's `[]`-flags into the export's flat
/// `key -> value` shape (`model::Flags`).
///
/// Checks against no whitelist — that is `rules::keys`' job — so a custom
/// `x-` flag reaches the export instead of being silently dropped.
fn build_flags(attrs: &[ast::Attr]) -> model::Flags {
    attrs
        .iter()
        .map(|attr| {
            let value = match &attr.value {
                Some(v) => model::FlagValue::Text(v.to_text()),
                None => model::FlagValue::Bool(true),
            };
            (attr.key.text.clone(), value)
        })
        .collect()
}
