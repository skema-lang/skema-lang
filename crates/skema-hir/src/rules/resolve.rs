//! Builds the exportable model: `use` partials inlined, relationships and
//! types resolved, physical names cascaded (`docs/05`–`docs/11`).

use std::collections::{BTreeSet, HashMap, HashSet};

use skema_syntax::Severity;
use skema_syntax::Span;
use skema_syntax::ast::{self, AstNode, BlockKind, RelationOp, ValueKind};
use smol_str::SmolStr;

use super::{Ctx, Dialect, Mode, ProjectSettings};
use crate::catalog::{
    self, BUILTIN_TYPE_PARAMS, BUILTIN_TYPES, DRAFT_TYPES, RELATION_ACTION_VALUES, TypeParamSlot,
    TypeParams,
};
use crate::model;
use crate::symbols::{Decl, FileFacts};
use crate::{Diagnostic, FileId};

/// Bundles what every resolution function needs, so the signatures don't
/// drown under the sheer parameter count.
struct Env<'c> {
    ctx: &'c Ctx,
    dialect: Dialect,
    mode: Mode,
    /// `Lint.danglingRef`'s configured severity, computed once for the whole
    /// analysis (`rules::dangling_ref_severity`) – shared with `group`.
    dangling_ref_severity: Severity,
}

/// The fields and indexes of a `Table` or a `Partial`, after every `use`
/// has been inlined at its spot (`docs/09`).
///
/// Every entry carries the file it's written in – diagnostics **and** name
/// resolution follow that file, not the table that ends up using it (via
/// `use`).
#[derive(Default)]
struct Flattened {
    fields: Vec<FlatField>,
    indexes: Vec<(FileId, Span, ast::IndexEntry)>,
    /// One entry per `Record` block directly inside the table's own
    /// `Example` block – `Example` is `Table`-only (`docs/05`), so unlike
    /// `fields`/`indexes` this never comes from an inlined `use` `Partial`.
    examples: Vec<(FileId, Span, ast::Block)>,
}

struct FlatField {
    owner: FileId,
    span: Span,
    field: ast::Field,
    /// The innermost `Partial` declaring the field; `None` when written in
    /// the table itself.
    partial: Option<(FileId, SmolStr)>,
}

/// A `Partial` reports its diagnostics and links on its first flattening only.
/// It is not memoized: a cycle member's fields depend on where the cycle is entered.
#[derive(Default)]
struct Partials {
    reported: HashSet<(FileId, SmolStr)>,
    in_progress: Vec<(FileId, SmolStr)>,
}

pub(crate) fn run(
    ctx: &Ctx,
    project: Option<&ProjectSettings>,
    dangling_ref_severity: Severity,
    links: &mut Vec<model::ReferenceLink>,
    diags: &mut Vec<Diagnostic>,
) -> (Vec<model::Table>, Vec<model::Enum>, Vec<model::Datatype>) {
    let env = Env {
        ctx,
        dialect: project.map_or(Dialect::Generic, |p| p.dialect),
        mode: project.map_or(Mode::Draft, |p| p.mode),
        dangling_ref_severity,
    };
    let project_database = project.and_then(|p| p.database.as_ref());

    let mut partials = Partials::default();
    for file in &ctx.facts {
        let mut partial_decls: Vec<_> = file
            .declarations
            .iter()
            .filter(|(_, decl)| decl.kind == BlockKind::Partial)
            .collect();
        // Source order, so a cycle is always reported at the same `use`.
        partial_decls.sort_by_key(|(_, decl)| decl.item_span.start);
        for (name, decl) in partial_decls {
            let key = (file.id, name.clone());
            if !partials.reported.contains(&key) {
                flatten_partial(&env, key, &decl.block, &mut partials, links, diags);
            }
        }
    }

    let mut flattened: HashMap<(FileId, SmolStr), Flattened> = HashMap::new();
    for file in &ctx.facts {
        for (name, decl) in &file.declarations {
            if decl.kind == BlockKind::Table {
                let flat = flatten_items(
                    &env,
                    file.id,
                    decl.block.items(),
                    &mut partials,
                    links,
                    diags,
                );
                flattened.insert((file.id, name.clone()), flat);
            }
        }
    }

    let mut tables = Vec::new();
    for file in &ctx.facts {
        for (name, decl) in &file.declarations {
            if decl.kind != BlockKind::Table {
                continue;
            }
            let Some(flat) = flattened.get(&(file.id, name.clone())) else {
                continue;
            };
            let fields = flat
                .fields
                .iter()
                .map(|field| resolve_field(&env, field, &flattened, links, diags))
                .collect();
            let indexes = build_indexes(flat, diags);
            let examples = build_examples(&env, flat, diags);
            let document_database = file
                .document
                .as_ref()
                .and_then(|(_, _, b)| b.child(BlockKind::Database));
            let table_database = decl.block.child(BlockKind::Database);
            let physical = resolve_physical(
                project_database,
                document_database.as_ref(),
                table_database.as_ref(),
            );
            let namespace = document_namespace(file);
            let moved_from =
                resolve_moved_from(file.id, decl.block.child(BlockKind::Meta).as_ref(), diags);
            tables.push(model::Table {
                file: file.id,
                namespace,
                name: name.to_string(),
                physical,
                fields,
                indexes,
                examples,
                moved_from,
                extensions: meta_custom_keys(&decl.block),
                span: decl.item_span,
            });
        }
    }
    tables.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));

    let mut enums = Vec::new();
    for file in &ctx.facts {
        for (name, decl) in &file.declarations {
            if decl.kind != BlockKind::Enum {
                continue;
            }
            enums.push(resolve_enum(file, name, &decl.block, decl.item_span, diags));
        }
    }
    enums.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));

    let mut datatypes = Vec::new();
    for file in &ctx.facts {
        for (name, decl) in &file.declarations {
            if decl.kind == BlockKind::Datatype {
                datatypes.push(resolve_datatype(&env, file, name, decl, diags));
            }
        }
    }
    datatypes.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));

    (tables, enums, datatypes)
}

fn document_namespace(file: &FileFacts) -> String {
    file.document
        .as_ref()
        .map_or_else(String::new, |(name, ..)| name.to_string())
}

fn decl_ref(ctx: &Ctx, owner: FileId, name: &str) -> model::DeclRef {
    model::DeclRef {
        namespace: ctx.file(owner).map_or_else(String::new, document_namespace),
        name: name.to_owned(),
    }
}

// ===========================================================================
// Inlining `use` partials
// ===========================================================================

fn flatten_partial(
    env: &Env<'_>,
    key: (FileId, SmolStr),
    block: &ast::Block,
    partials: &mut Partials,
    links: &mut Vec<model::ReferenceLink>,
    diags: &mut Vec<Diagnostic>,
) -> Flattened {
    let (mut silent_links, mut silent_diags) = (Vec::new(), Vec::new());
    let (links, diags) = if partials.reported.insert(key.clone()) {
        (links, diags)
    } else {
        (&mut silent_links, &mut silent_diags)
    };
    partials.in_progress.push(key.clone());
    let mut result = flatten_items(env, key.0, block.items(), partials, links, diags);
    partials.in_progress.pop();
    for field in &mut result.fields {
        field.partial.get_or_insert_with(|| key.clone());
    }
    result
}

/// Every diagnostic here belongs to `owner`, the file holding the `use` –
/// not to the file declaring its target.
fn flatten_use(
    env: &Env<'_>,
    owner: FileId,
    target: &ast::Path,
    partials: &mut Partials,
    links: &mut Vec<model::ReferenceLink>,
    diags: &mut Vec<Diagnostic>,
) -> Flattened {
    let ctx = env.ctx;
    let Some((target_owner, name, name_span, rest)) = ctx.resolve_decl_path(owner, target) else {
        // A namespace with nothing after it (`use ns;`) – the only
        // other `None` case, an empty path, cannot occur here.
        let namespace = first_segment(target);
        diags.push(Diagnostic::error(
            owner,
            target.span(),
            format!("`use` expects a `Partial` after namespace `{namespace}`"),
        ));
        return Flattened::default();
    };
    if !rest.is_empty() {
        diags.push(Diagnostic::error(
            owner,
            target.span(),
            "`use` names only a `Partial`, no further path after it",
        ));
    }
    let Some(decl) = ctx.lookup_decl(target_owner, &name) else {
        diags.push(Diagnostic::new(
            owner,
            env.dangling_ref_severity,
            name_span,
            format!("unknown `Partial` `{name}`"),
        ));
        return Flattened::default();
    };
    if decl.kind != BlockKind::Partial {
        diags.push(Diagnostic::new(
            owner,
            env.dangling_ref_severity,
            name_span,
            format!(
                "`use` expects a `Partial`, `{name}` is a `{}`",
                decl.kind.as_str()
            ),
        ));
        return Flattened::default();
    }
    links.push(model::ReferenceLink {
        from_file: owner,
        from_span: name_span,
        to_file: target_owner,
        to_span: decl.name_span,
    });
    let key = (target_owner, name);
    if partials.in_progress.contains(&key) {
        diags.push(Diagnostic::error(
            owner,
            name_span,
            format!(
                "cycle in `use`: `{}` uses itself, directly or indirectly",
                key.1
            ),
        ));
        return Flattened::default();
    }
    flatten_partial(env, key, &decl.block, partials, links, diags)
}

fn flatten_items(
    env: &Env<'_>,
    owner: FileId,
    items: impl Iterator<Item = ast::Item>,
    partials: &mut Partials,
    links: &mut Vec<model::ReferenceLink>,
    diags: &mut Vec<Diagnostic>,
) -> Flattened {
    let mut out = Flattened::default();
    for item in items {
        match item.kind() {
            ast::ItemKind::Use(u) => {
                let inner = flatten_use(env, owner, &u.target(), partials, links, diags);
                for field in inner.fields {
                    push_field(&mut out, field, diags);
                }
                out.indexes.extend(inner.indexes);
            }
            ast::ItemKind::Field(field) => push_field(
                &mut out,
                FlatField {
                    owner,
                    span: item.span(),
                    field,
                    partial: None,
                },
                diags,
            ),
            ast::ItemKind::Block(b) if b.kind() == BlockKind::Index => {
                for sub in b.items() {
                    if let ast::ItemKind::IndexEntry(e) = sub.kind() {
                        out.indexes.push((owner, sub.span(), e));
                    }
                }
            }
            ast::ItemKind::Block(b) if b.kind() == BlockKind::Example => {
                for sub in b.items() {
                    if let ast::ItemKind::Block(record) = sub.kind() {
                        if record.kind() == BlockKind::Record {
                            out.examples.push((owner, sub.span(), record));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// The first segment's text – `""` for an empty path.
fn first_segment(path: &ast::Path) -> String {
    path.segments()
        .next()
        .map(|segment| segment.text().to_owned())
        .unwrap_or_default()
}

fn push_field(out: &mut Flattened, field: FlatField, diags: &mut Vec<Diagnostic>) {
    let name = field.field.name();
    if let Some(existing) = out
        .fields
        .iter()
        .find(|e| e.field.name().text() == name.text())
    {
        diags.push(Diagnostic::error(
            field.owner,
            name.span(),
            format!(
                "field name `{}` is already taken (at {:?}) – from `use` or an earlier field (docs/09)",
                name.text(),
                existing.field.name().span()
            ),
        ));
        return;
    }
    out.fields.push(field);
}

// ===========================================================================
// Indexes
// ===========================================================================

fn build_indexes(flat: &Flattened, diags: &mut Vec<Diagnostic>) -> Vec<model::Index> {
    flat.indexes
        .iter()
        .map(|&(owner, span, ref e)| {
            let columns: Vec<ast::Ident> = e.columns().collect();
            for c in &columns {
                if !has_field(flat, c.text()) {
                    diags.push(Diagnostic::error(
                        owner,
                        c.span(),
                        format!("index refers to unknown column `{}`", c.text()),
                    ));
                }
            }
            model::Index {
                file: owner,
                name: e.name().map(|n| n.text().to_owned()),
                columns: columns.iter().map(|c| c.text().to_owned()).collect(),
                flags: build_flags(e.flags()),
                span,
            }
        })
        .collect()
}

fn has_field(flat: &Flattened, name: &str) -> bool {
    flat.fields.iter().any(|f| f.field.name().text() == name)
}

// ===========================================================================
// Example rows
// ===========================================================================

/// Structural and value-type validation for `Table`'s `Example` block
/// (`docs/05`): unknown field names, missing `required`/`pk` fields,
/// `pk`/`unique` value collisions across a table's own rows, and (task 12)
/// each value's literal kind against its field's resolved type.
fn build_examples(
    env: &Env<'_>,
    flat: &Flattened,
    diags: &mut Vec<Diagnostic>,
) -> Vec<model::ExampleRow> {
    let field_types: Vec<(ast::Ident, Option<String>)> = flat
        .fields
        .iter()
        .map(|f| {
            (
                f.field.name(),
                example_field_terminal_type(env, f.owner, &f.field),
            )
        })
        .collect();

    let rows: Vec<model::ExampleRow> = flat
        .examples
        .iter()
        .map(|&(owner, span, ref record)| {
            let attrs: Vec<ast::Attr> = record
                .items()
                .filter_map(|item| match item.kind() {
                    ast::ItemKind::Setting(a) => Some(a),
                    _ => None,
                })
                .collect();
            for attr in &attrs {
                let key = attr.key();
                let key_text = key.text();
                if !has_field(flat, &key_text) {
                    diags.push(Diagnostic::error(
                        owner,
                        key.span(),
                        format!("`Example` row assigns unknown field `{key_text}` (docs/05)"),
                    ));
                    continue;
                }
                let Some(value) = attr.value() else { continue };
                let Some(expected_ty) = field_types
                    .iter()
                    .find(|(name, _)| name.text() == key_text)
                    .and_then(|(_, ty)| ty.as_deref())
                else {
                    continue;
                };
                if !example_value_matches_type(&value, expected_ty) {
                    diags.push(Diagnostic::error(
                        owner,
                        value.span(),
                        format!(
                            "`Example` row's `{key_text}` should be `{expected_ty}`, got `{}`",
                            value.to_text()
                        ),
                    ));
                }
            }
            model::ExampleRow {
                file: owner,
                values: build_flags(attrs.into_iter()),
                span,
            }
        })
        .collect();

    let required = fields_flagged(flat, &["required", "pk"]);
    for (i, row) in rows.iter().enumerate() {
        let (owner, span, _) = flat.examples[i];
        for name in &required {
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

    let collision_checked = fields_flagged(flat, &["pk", "unique"]);
    for name in &collision_checked {
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

/// Names of the flattened fields carrying any of `keys` as a flag, in order.
fn fields_flagged(flat: &Flattened, keys: &[&str]) -> Vec<String> {
    flat.fields
        .iter()
        .filter(|f| {
            f.field
                .flags()
                .any(|a| keys.contains(&a.key().text().as_str()))
        })
        .map(|f| f.field.name().text().to_owned())
        .collect()
}

/// The built-in type name an `Example` value should be checked against, or
/// `None` if this field's type doesn't resolve to one – a relationship
/// field, an `Enum` (task 12 covers built-ins and `Datatype`s only), an
/// engine-catalog type, or a name this rule can't resolve. Left unchecked
/// rather than risking a false positive (`docs/13` "Not enforced yet").
fn example_field_terminal_type(env: &Env<'_>, owner: FileId, field: &ast::Field) -> Option<String> {
    let ast::FieldTypeKind::Named { path, .. } = field.ty()?.kind()? else {
        return None;
    };
    let terminal = match type_target(env, owner, &path) {
        TypeTarget::Catalog(name) => name,
        TypeTarget::Datatype(datatype) => match base_chain_end(env, datatype) {
            BaseChainEnd::Type(name) => name,
            BaseChainEnd::Unresolved | BaseChainEnd::Cycle => return None,
        },
        TypeTarget::Other => return None,
    };
    BUILTIN_TYPES
        .contains(&terminal.as_str())
        .then_some(terminal)
}

/// Whether an `Example` value's literal kind is compatible with
/// `expected_ty`, a resolved built-in name from [`BUILTIN_TYPES`] (task 12).
///
/// A raw expression (`` `…` ``) is accepted unconditionally for any type –
/// opaque SQL passed through untouched by design (`docs/01`), the same call
/// `check_arg_type` already makes for a `Datatype`/built-in argument.
fn example_value_matches_type(value: &ast::Value, expected_ty: &str) -> bool {
    let Some(kind) = value.kind() else {
        return false;
    };
    match (kind, expected_ty) {
        (ValueKind::Raw(_), _) => true,
        (ValueKind::Int(_), "int") => true,
        (ValueKind::Int(_) | ValueKind::Float(_), "float") => true,
        (ValueKind::Str(_), "string" | "varchar" | "text" | "uuid" | "date" | "timestamp") => true,
        (ValueKind::Path(p), "bool") => is_bool_literal(&p),
        _ => false,
    }
}

fn is_bool_literal(path: &ast::Path) -> bool {
    path.as_ident()
        .is_some_and(|i| matches!(i.text(), "true" | "false"))
}

// ===========================================================================
// Fields, types, relationships
// ===========================================================================

fn resolve_field(
    env: &Env<'_>,
    flat_field: &FlatField,
    flattened: &HashMap<(FileId, SmolStr), Flattened>,
    links: &mut Vec<model::ReferenceLink>,
    diags: &mut Vec<Diagnostic>,
) -> model::Field {
    let FlatField {
        owner,
        span,
        field: ref f,
        ref partial,
    } = *flat_field;
    let mut flags = build_flags(f.flags());
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

    let field_type = f.ty().and_then(|t| Some((t.span(), t.kind()?)));
    let (ty, relation) = match field_type {
        Some((type_span, kind)) => match kind {
            ast::FieldTypeKind::Relation { op, target } => {
                let site = RelationSite {
                    op,
                    target,
                    flags: f.flags().collect(),
                    span: type_span,
                };
                let relation = resolve_relation(env, owner, site, flattened, links, diags);
                let ty = match &relation {
                    Some(r) => {
                        plain_type(format!("{}.{}", r.target_table, r.target_field), Vec::new())
                    }
                    None => unknown_type(),
                };
                (ty, relation)
            }
            ast::FieldTypeKind::Named { path, args } => (
                resolve_named_type(env, owner, &path, &args, type_span, links, diags),
                None,
            ),
        },
        None => (unknown_type(), None),
    };

    let note = string_setting(f.items(), "note");
    let label = string_setting(f.items(), "label");

    model::Field {
        file: owner,
        name: f.name().text().to_owned(),
        ty,
        flags,
        relation,
        note,
        label,
        extensions: custom_keys(f.items()),
        from_partial: partial
            .as_ref()
            .map(|(partial_owner, name)| decl_ref(env.ctx, *partial_owner, name)),
        span,
    }
}

fn unknown_type() -> model::TypeRef {
    plain_type("unknown".to_owned(), Vec::new())
}

/// A type that is neither an `Enum` nor a `Datatype`.
fn plain_type(name: String, args: Vec<String>) -> model::TypeRef {
    model::TypeRef {
        name,
        args,
        datatype: None,
        enum_: None,
    }
}

/// Bundles the parts of a `>`/`</`-` relation site that come straight off
/// the AST, so [`resolve_relation`] doesn't drown under the sheer parameter
/// count (the same reasoning as [`Env`]).
struct RelationSite {
    op: RelationOp,
    target: ast::Path,
    flags: Vec<ast::Attr>,
    /// The span of the relation's [`ast::FieldType`] itself (e.g.
    /// `>Customer.id`) – more precise than the owning field's span.
    span: Span,
}

fn resolve_relation(
    env: &Env<'_>,
    owner: FileId,
    site: RelationSite,
    flattened: &HashMap<(FileId, SmolStr), Flattened>,
    links: &mut Vec<model::ReferenceLink>,
    diags: &mut Vec<Diagnostic>,
) -> Option<model::Relation> {
    let RelationSite {
        op,
        target,
        flags,
        span,
    } = site;
    let Some((table_owner, table_name, table_span, rest)) =
        env.ctx.resolve_decl_path(owner, &target)
    else {
        // A namespace with nothing after it (`customer >ns;`) – the only
        // other `None` case, an empty path, cannot occur here.
        let namespace = first_segment(&target);
        diags.push(Diagnostic::error(
            owner,
            target.span(),
            format!(
                "a relationship target names a table and a field, not only namespace `{namespace}`"
            ),
        ));
        return None;
    };
    let [field_ident] = rest.as_slice() else {
        diags.push(Diagnostic::error(
            owner,
            target.span(),
            "a relationship target always names a table and a field, e.g. `Customer.id` (docs/07)",
        ));
        return None;
    };
    let field_name = field_ident.text();

    let Some(decl) = env.ctx.lookup_decl(table_owner, &table_name) else {
        diags.push(Diagnostic::new(
            owner,
            env.dangling_ref_severity,
            table_span,
            format!("unknown table `{table_name}`"),
        ));
        return None;
    };
    if decl.kind != BlockKind::Table {
        diags.push(Diagnostic::new(
            owner,
            env.dangling_ref_severity,
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
    let field_decl = flattened
        .get(&(table_owner, table_name.clone()))
        .and_then(|flat| {
            flat.fields
                .iter()
                .find(|f| f.field.name().text() == field_name)
        });
    let Some(FlatField {
        owner: field_owner,
        field,
        ..
    }) = field_decl
    else {
        diags.push(Diagnostic::new(
            owner,
            env.dangling_ref_severity,
            field_ident.span(),
            format!("`{table_name}` has no field `{field_name}`"),
        ));
        return None;
    };
    links.push(model::ReferenceLink {
        from_file: owner,
        from_span: field_ident.span(),
        to_file: *field_owner,
        to_span: field.name().span(),
    });

    let mut on_delete = None;
    let mut on_update = None;
    for attr in &flags {
        let key = attr.key();
        let key_text = key.text();
        match key_text.as_str() {
            "onDelete" | "onUpdate" if op != RelationOp::OneToMany => {
                diags.push(Diagnostic::error(
                    owner,
                    key.span(),
                    format!(
                        "`{key_text}` is only allowed on the `>` side of a relationship (docs/07, \"Flags are written only on the > side\")"
                    ),
                ));
            }
            "onDelete" => {
                on_delete = check_relation_action(owner, &key_text, attr.value().as_ref(), diags)
            }
            "onUpdate" => {
                on_update = check_relation_action(owner, &key_text, attr.value().as_ref(), diags)
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
            .map_or_else(String::new, document_namespace),
        target_table: table_name.to_string(),
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
            value.span(),
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
    let attr = meta?.items().find_map(|item| match item.kind() {
        ast::ItemKind::Setting(a) if a.key().text() == "movedFrom" => Some(a),
        _ => None,
    })?;
    match attr.value().and_then(|v| v.kind()) {
        Some(ValueKind::Str(s)) => Some(s),
        _ => {
            diags.push(Diagnostic::error(
                owner,
                attr.span(),
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
    env: &Env<'_>,
    owner: FileId,
    path: &ast::Path,
    args: &[ast::Value],
    span: Span,
    links: &mut Vec<model::ReferenceLink>,
    diags: &mut Vec<Diagnostic>,
) -> model::TypeRef {
    let dotted = path.to_dotted();
    let str_args: Vec<String> = args.iter().map(ast::Value::to_text).collect();

    if let Some(bare_ident) = path.as_ident() {
        let bare = bare_ident.text();
        if let Some(params) = catalog_type_params(env.dialect, bare) {
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
            return plain_type(bare.to_owned(), str_args);
        }
    }

    let Some((decl_owner, name, name_span, rest)) = env.ctx.resolve_decl_path(owner, path) else {
        return plain_type(dotted, str_args);
    };
    if !rest.is_empty() {
        diags.push(Diagnostic::error(
            owner,
            span,
            "unexpected path after the type name",
        ));
        return plain_type(dotted, str_args);
    }

    let decl = env.ctx.lookup_decl(decl_owner, &name);
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
            enum_: Some(decl_ref(env.ctx, decl_owner, &name)),
            ..plain_type(dotted, Vec::new())
        },
        Some(decl) if decl.kind == BlockKind::Datatype => {
            check_datatype_args(owner, &decl.block, args, span, diags);
            check_datatype_dialect_coverage(env, owner, &dotted, &decl.block, span, diags);
            model::TypeRef {
                datatype: Some(decl_ref(env.ctx, decl_owner, &name)),
                ..plain_type(dotted, str_args)
            }
        }
        Some(decl) => {
            diags.push(Diagnostic::error(
                owner,
                name_span,
                format!("`{name}` is not a type, but a `{}`", decl.kind.as_str()),
            ));
            plain_type(dotted, str_args)
        }
        None if env.dialect == Dialect::Generic => {
            diags.push(Diagnostic::new(
                owner,
                env.dangling_ref_severity,
                span,
                format!(
                    "unknown type `{dotted}` – allowed are the built-in types, a custom \
                     `Enum`/`Datatype`, or (with `dialect`) an engine-specific type (docs/02, docs/06)"
                ),
            ));
            plain_type(dotted, str_args)
        }
        None => {
            let engine = env.dialect.engine_name().unwrap_or("this dialect");
            diags.push(Diagnostic::new(
                owner,
                env.dangling_ref_severity,
                span,
                format!(
                    "unknown type `{dotted}` – allowed are the built-in types, `{engine}`'s own \
                     type catalog (docs/06), a custom `Enum`/`Datatype`"
                ),
            ));
            plain_type(dotted, str_args)
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
    let params: Vec<ast::Param> = datatype.params().collect();
    if args.len() > params.len() {
        diags.push(Diagnostic::error(
            owner,
            span,
            format!(
                "`{}` expects at most {} argument(s), got: {}",
                datatype.name().map(|n| n.to_dotted()).unwrap_or_default(),
                params.len(),
                args.len()
            ),
        ));
        return;
    }
    for (arg, param) in args.iter().zip(&params) {
        if let Some(ty) = param.ty().as_ref().and_then(ast::Path::as_ident) {
            check_arg_type(owner, arg, ty.text(), &param_name(param), diags);
        }
    }
    for param in &params[args.len()..] {
        let name = param_name(param);
        if !param_has_default(datatype, &name) {
            diags.push(Diagnostic::error(
                owner,
                span,
                format!("argument for parameter `{name}` is missing – no `default` set"),
            ));
        }
    }
}

/// `""` only for a parameter the parser could not name.
fn param_name(param: &ast::Param) -> String {
    param
        .name()
        .map(|name| name.text().to_owned())
        .unwrap_or_default()
}

/// A `Datatype` used under a non-`generic` dialect with no matching
/// `Dialect @<engine>` block (`docs/06`, "Missing dialect"): a hint under
/// `draft` (a `Warning`-severity diagnostic – the same severity
/// `resolve_named_type`'s own `None` arm already uses for "not finalized
/// yet" cases), an error under `strict`.
fn check_datatype_dialect_coverage(
    env: &Env<'_>,
    owner: FileId,
    datatype_name: &str,
    datatype: &ast::Block,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(engine) = env.dialect.engine_name() else {
        return;
    };
    let covered = datatype.items().any(|item| match item.kind() {
        ast::ItemKind::Block(b) if b.kind() == BlockKind::Dialect => {
            b.selector().is_some_and(|s| s.text() == engine)
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

/// The argument shape of a built-in or, under `dialect`, engine-catalog
/// type – `None` for any other name. Checked before declarations, so the
/// catalog shadows a same-named `Datatype` referenced bare.
fn catalog_type_params(dialect: Dialect, name: &str) -> Option<TypeParams> {
    let dialect_entry = dialect
        .engine_name()
        .and_then(|engine| catalog::dialect_type(engine, name));
    match dialect_entry {
        Some(entry) => Some(entry.params),
        None => BUILTIN_TYPES
            .contains(&name)
            .then(|| builtin_type_params(name)),
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
                if !matches!(arg.kind(), Some(ValueKind::Str(_) | ValueKind::Raw(_))) {
                    diags.push(Diagnostic::error(
                        owner,
                        arg.span(),
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
    let ok = match arg.kind() {
        Some(ValueKind::Raw(_)) => true,
        Some(ValueKind::Path(p)) => p.as_ident().is_some_and(|i| words.contains(&i.text())),
        _ => false,
    };
    if !ok {
        diags.push(Diagnostic::error(
            owner,
            arg.span(),
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
    let matches = match (arg.kind(), expected_ty) {
        (Some(ValueKind::Raw(_)), _) => true,
        (Some(ValueKind::Int(_)), "int") => true,
        (Some(ValueKind::Int(_) | ValueKind::Float(_)), "float") => true,
        (Some(ValueKind::Str(_)), "string") => true,
        (Some(ValueKind::Path(p)), "bool") => is_bool_literal(&p),
        (_, "int" | "float" | "string" | "bool") => false,
        _ => return,
    };
    if !matches {
        diags.push(Diagnostic::error(
            owner,
            arg.span(),
            format!(
                "argument for parameter `{param_name}` should be `{expected_ty}`, got `{}`",
                arg.to_text()
            ),
        ));
    }
}

fn param_has_default(datatype: &ast::Block, param_name: &str) -> bool {
    param_body(datatype, param_name).is_some_and(|body| {
        body.items().any(
            |sub| matches!(sub.kind(), ast::ItemKind::Setting(a) if a.key().text() == "default"),
        )
    })
}

/// A parameter's own body inside its `Datatype` (`scale { default = 2; }`).
fn param_body(datatype: &ast::Block, param_name: &str) -> Option<ast::Field> {
    datatype.items().find_map(|item| match item.kind() {
        ast::ItemKind::Field(f) if f.name().text() == param_name => Some(f),
        _ => None,
    })
}

// ===========================================================================
// Datatypes
// ===========================================================================

/// What a type name written in `owner` refers to, the catalog first as in
/// [`resolve_named_type`].
enum TypeTarget {
    Catalog(String),
    Datatype((FileId, SmolStr)),
    Other,
}

fn type_target(env: &Env<'_>, owner: FileId, path: &ast::Path) -> TypeTarget {
    if let Some(bare) = path.as_ident() {
        if catalog_type_params(env.dialect, bare.text()).is_some() {
            return TypeTarget::Catalog(bare.text().to_owned());
        }
    }
    let Some((decl_owner, name, _, rest)) = env.ctx.resolve_decl_path(owner, path) else {
        return TypeTarget::Other;
    };
    match env.ctx.lookup_decl(decl_owner, &name) {
        Some(decl) if rest.is_empty() && decl.kind == BlockKind::Datatype => {
            TypeTarget::Datatype((decl_owner, name))
        }
        _ => TypeTarget::Other,
    }
}

/// `Field { type = …; }` of a `Datatype`.
struct DatatypeBase {
    path: ast::Path,
    args: Vec<ast::Value>,
    /// The whole value, arguments included.
    span: Span,
}

fn datatype_base(datatype: &ast::Block) -> Option<DatatypeBase> {
    let field_block = datatype.child(BlockKind::Field)?;
    let value = field_block.items().find_map(|item| match item.kind() {
        ast::ItemKind::Setting(a) if a.key().text() == "type" => a.value(),
        _ => None,
    })?;
    let (path, args) = match value.kind()? {
        ValueKind::Path(path) => (path, Vec::new()),
        ValueKind::Type { path, args } => (path, args),
        _ => return None,
    };
    Some(DatatypeBase {
        path,
        args,
        span: value.span(),
    })
}

enum BaseChainEnd {
    /// A built-in or engine-catalog type name.
    Type(String),
    /// An unknown name, an `Enum`, or a cycle `start` only leads into.
    Unresolved,
    /// The chain leads back to `start`.
    Cycle,
}

/// Follows `Field.type` from the `Datatype` `start` through layered
/// `Datatype`s (`docs/06`, "Layering types"), each step resolved in the file
/// it is written in. Names only: no argument substitution, no `Dialect` `sql`.
fn base_chain_end(env: &Env<'_>, start: (FileId, SmolStr)) -> BaseChainEnd {
    let mut visited = vec![start.clone()];
    let mut current = start.clone();
    loop {
        let Some(base) = env
            .ctx
            .lookup_decl(current.0, &current.1)
            .and_then(|decl| datatype_base(&decl.block))
        else {
            return BaseChainEnd::Unresolved;
        };
        match type_target(env, current.0, &base.path) {
            TypeTarget::Catalog(name) => return BaseChainEnd::Type(name),
            TypeTarget::Other => return BaseChainEnd::Unresolved,
            TypeTarget::Datatype(next) if next == start => return BaseChainEnd::Cycle,
            TypeTarget::Datatype(next) if visited.contains(&next) => {
                return BaseChainEnd::Unresolved;
            }
            TypeTarget::Datatype(next) => {
                visited.push(next.clone());
                current = next;
            }
        }
    }
}

fn resolve_datatype(
    env: &Env<'_>,
    file: &FileFacts,
    name: &SmolStr,
    decl: &Decl,
    diags: &mut Vec<Diagnostic>,
) -> model::Datatype {
    let base = datatype_base(&decl.block);
    let base_datatype =
        base.as_ref()
            .and_then(|base| match type_target(env, file.id, &base.path) {
                TypeTarget::Datatype((owner, name)) => Some(decl_ref(env.ctx, owner, &name)),
                TypeTarget::Catalog(_) | TypeTarget::Other => None,
            });
    let resolved_base_type = resolved_base_type(env, file.id, name, base.as_ref(), diags);
    let field_block = decl.block.child(BlockKind::Field);
    let field_setting = |key: &str| {
        field_block
            .as_ref()
            .and_then(|b| value_setting(b.items(), key))
    };

    model::Datatype {
        file: file.id,
        namespace: document_namespace(file),
        name: name.to_string(),
        params: datatype_params(&decl.block),
        base_type: base.as_ref().map(|base| base.path.to_dotted()),
        base_type_args: base
            .as_ref()
            .map(|base| base.args.iter().map(ast::Value::to_text).collect())
            .unwrap_or_default(),
        base_datatype,
        resolved_base_type,
        default: field_setting("default"),
        regex: field_setting("regex"),
        note: field_setting("note"),
        extensions: meta_and_field_custom_keys(&decl.block),
        span: decl.item_span,
    }
}

/// A cycle is reported once per member, at its own `Field.type`.
fn resolved_base_type(
    env: &Env<'_>,
    owner: FileId,
    name: &SmolStr,
    base: Option<&DatatypeBase>,
    diags: &mut Vec<Diagnostic>,
) -> Option<String> {
    match base_chain_end(env, (owner, name.clone())) {
        BaseChainEnd::Type(resolved) => Some(resolved),
        BaseChainEnd::Unresolved => None,
        BaseChainEnd::Cycle => {
            if let Some(base) = base {
                diags.push(Diagnostic::error(
                    owner,
                    base.span,
                    format!(
                        "cycle in `Field.type`: `{name}` builds on itself, directly or indirectly"
                    ),
                ));
            }
            None
        }
    }
}

fn datatype_params(datatype: &ast::Block) -> Vec<model::DatatypeParam> {
    datatype
        .params()
        .map(|param| {
            let name = param_name(&param);
            let body = param_body(datatype, &name);
            let body_setting =
                |key: &str| body.as_ref().and_then(|b| value_setting(b.items(), key));
            model::DatatypeParam {
                ty: param.ty().map(|path| path.to_dotted()),
                default: body_setting("default"),
                note: body_setting("note"),
                name,
            }
        })
        .collect()
}

// ===========================================================================
// Enums
// ===========================================================================

fn resolve_enum(
    file: &FileFacts,
    name: &str,
    block: &ast::Block,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) -> model::Enum {
    let field_block = block.child(BlockKind::Field);
    let extensions = meta_and_field_custom_keys(block);
    let stored_type = field_block
        .as_ref()
        .and_then(|b| {
            b.items().find_map(|i| match i.kind() {
                ast::ItemKind::Setting(a) if a.key().text() == "type" => {
                    a.value().and_then(|v| match v.kind() {
                        Some(ValueKind::Path(p)) => {
                            p.as_ident().map(|i| (i.text().to_owned(), v.span()))
                        }
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

    let entries: Vec<model::EnumEntry> = block
        .items()
        .filter_map(|item| match item.kind() {
            ast::ItemKind::Field(f) => Some((item.span(), f)),
            _ => None,
        })
        .map(|(entry_span, f)| model::EnumEntry {
            name: f.name().text().to_owned(),
            label: string_setting(f.items(), "label"),
            value: value_setting(f.items(), "value"),
            extensions: custom_keys(f.items()),
            span: entry_span,
        })
        .collect();

    model::Enum {
        file: file.id,
        namespace: document_namespace(file),
        name: name.to_owned(),
        stored_type,
        default: field_block
            .as_ref()
            .and_then(|b| enum_default(file.id, b, &entries, diags)),
        note: field_block
            .as_ref()
            .and_then(|b| string_setting(b.items(), "note")),
        entries,
        extensions,
        span,
    }
}

/// Written bare: inside the enum there is no literal-or-member ambiguity
/// to qualify away (`docs/08`, "Using an enum").
fn enum_default(
    owner: FileId,
    field_block: &ast::Block,
    entries: &[model::EnumEntry],
    diags: &mut Vec<Diagnostic>,
) -> Option<String> {
    let value = field_block.items().find_map(|item| match item.kind() {
        ast::ItemKind::Setting(a) if a.key().text() == "default" => a.value(),
        _ => None,
    })?;
    let is_entry = |name: &str| entries.iter().any(|entry| entry.name == name);
    let path = match value.kind() {
        Some(ValueKind::Path(path)) => Some(path),
        _ => None,
    };
    if let Some(name) = path.as_ref().and_then(ast::Path::as_ident)
        && is_entry(name.text())
    {
        return Some(name.text().to_owned());
    }
    let bare_hint = path
        .and_then(|path| path.segments().last())
        .filter(|last| is_entry(last.text()))
        .map(|last| format!(": `{}`, not `{}`", last.text(), value.to_text()))
        .unwrap_or_default();
    diags.push(Diagnostic::error(
        owner,
        value.span(),
        format!(
            "`Enum.Field.default` must name one of this enum's entries, written bare{bare_hint} (docs/08)"
        ),
    ));
    None
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
    string_setting(database?.items(), key)
}

// ===========================================================================
// Small helpers
// ===========================================================================

fn string_setting(mut items: impl Iterator<Item = ast::Item>, key: &str) -> Option<String> {
    items.find_map(|item| match item.kind() {
        ast::ItemKind::Setting(a) if a.key().text() == key => {
            a.value().and_then(|v| match v.kind() {
                Some(ValueKind::Str(s)) => Some(s),
                _ => None,
            })
        }
        _ => None,
    })
}

fn value_setting(mut items: impl Iterator<Item = ast::Item>, key: &str) -> Option<String> {
    items.find_map(|item| match item.kind() {
        ast::ItemKind::Setting(a) if a.key().text() == key => a.value().map(|v| v.to_text()),
        _ => None,
    })
}

/// The quoted `x-` keys among `items`, values as text; the first occurrence
/// of a key wins, as in [`string_setting`].
pub(crate) fn custom_keys(items: impl Iterator<Item = ast::Item>) -> model::Extensions {
    let mut keys = model::Extensions::new();
    for item in items {
        let ast::ItemKind::Setting(attr) = item.kind() else {
            continue;
        };
        let key = attr.key();
        let name = key.text();
        if !key.quoted() || !name.starts_with("x-") {
            continue;
        }
        if let Some(value) = attr.value() {
            keys.entry(name).or_insert_with(|| value.to_text());
        }
    }
    keys
}

/// [`custom_keys`] of a declaration's own `Meta` block.
pub(crate) fn meta_custom_keys(declaration: &ast::Block) -> model::Extensions {
    declaration
        .child(BlockKind::Meta)
        .map(|meta| custom_keys(meta.items()))
        .unwrap_or_default()
}

/// [`meta_custom_keys`] of an `Enum` or `Datatype` merged with its `Field`
/// block's; the `Field` block wins a clash.
fn meta_and_field_custom_keys(declaration: &ast::Block) -> model::Extensions {
    let mut keys = meta_custom_keys(declaration);
    if let Some(field_block) = declaration.child(BlockKind::Field) {
        keys.extend(custom_keys(field_block.items()));
    }
    keys
}

/// Turns a field's or index's `[]`-flags into the export's flat
/// `key -> value` shape (`model::Flags`).
///
/// Checks against no whitelist — that is `rules::keys`' job — so a custom
/// `x-` flag reaches the export instead of being silently dropped.
fn build_flags(attrs: impl Iterator<Item = ast::Attr>) -> model::Flags {
    attrs
        .map(|attr| {
            let value = match attr.value() {
                Some(v) => model::FlagValue::Text(v.to_text()),
                None => model::FlagValue::Bool(true),
            };
            (attr.key().text(), value)
        })
        .collect()
}
