//! Builds the exportable model: `use` partials inlined, relationships and
//! types resolved, physical names cascaded (`docs/05`–`docs/11`).

use std::collections::HashMap;

use skema_syntax::Span;
use skema_syntax::ast::{self, BlockKind, RelationOp, ValueKind};

use super::{Ctx, Dialect, ProjectSettings};
use crate::catalog::BUILTIN_TYPES;
use crate::model;
use crate::{Diagnostic, FileId};

/// Bundles what every resolution function needs, so the signatures don't
/// drown under the sheer parameter count.
struct Env<'c, 'a> {
    ctx: &'c Ctx<'a>,
    dialect: Dialect,
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
}

pub(crate) fn run<'a>(
    ctx: &Ctx<'a>,
    project: Option<&ProjectSettings<'a>>,
    diags: &mut Vec<Diagnostic>,
) -> (Vec<model::Table>, Vec<model::Enum>) {
    let env = Env {
        ctx,
        dialect: project.map_or(Dialect::Generic, |p| p.dialect),
    };
    let project_database = project.and_then(|p| p.database);

    let mut flattened: HashMap<(FileId, &'a str), Flattened<'a>> = HashMap::new();
    for file in &ctx.facts {
        for (&name, decl) in &file.declarations {
            if matches!(decl.kind, BlockKind::Table | BlockKind::Partial) {
                let mut stack = Vec::new();
                let flat = flatten_items(ctx, file.id, &decl.block.items, &mut stack, diags);
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
                .map(|&(owner, span, f)| resolve_field(&env, owner, span, f, &flattened, diags))
                .collect();
            let indexes = build_indexes(flat, diags);
            let document_database = file
                .document
                .and_then(|(_, _, b)| b.child(BlockKind::Database));
            let table_database = decl.block.child(BlockKind::Database);
            let physical = resolve_physical(project_database, document_database, table_database);
            let namespace = file
                .document
                .map_or_else(String::new, |(n, ..)| n.to_owned());
            tables.push(model::Table {
                file: file.id,
                namespace,
                name: name.to_owned(),
                physical,
                fields,
                indexes,
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
    let result = flatten_items(ctx, owner, &decl.block.items, stack, diags);
    stack.pop();
    result
}

fn flatten_items<'a>(
    ctx: &Ctx<'a>,
    owner: FileId,
    items: &'a [ast::Item],
    stack: &mut Vec<(FileId, String)>,
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
                let inner = flatten_partial(ctx, target_owner, name, name_span, stack, diags);
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
// Fields, types, relationships
// ===========================================================================

fn resolve_field<'a>(
    env: &Env<'_, 'a>,
    owner: FileId,
    span: Span,
    f: &'a ast::Field,
    flattened: &HashMap<(FileId, &'a str), Flattened<'a>>,
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
                let relation = resolve_relation(env, owner, site, flattened, diags);
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
                resolve_named_type(env, owner, path, args, t.span, diags),
                None,
            ),
        },
        None => (unknown_type(), None),
    };

    let note = string_setting(&f.items, "note");

    model::Field {
        file: owner,
        name: f.name.text.clone(),
        ty,
        flags,
        relation,
        note,
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
    let has_field = flattened
        .get(&(table_owner, table_name))
        .is_some_and(|flat| {
            flat.fields
                .iter()
                .any(|(_, _, f)| f.name.text == field_name)
        });
    if !has_field {
        diags.push(Diagnostic::error(
            owner,
            rest[0].span,
            format!("`{table_name}` has no field `{field_name}`"),
        ));
        return None;
    }

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
            "onDelete" => on_delete = attr.value.as_ref().map(ast::Value::to_text),
            "onUpdate" => on_update = attr.value.as_ref().map(ast::Value::to_text),
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

fn resolve_named_type(
    env: &Env<'_, '_>,
    owner: FileId,
    path: &ast::Path,
    args: &[ast::Value],
    span: Span,
    diags: &mut Vec<Diagnostic>,
) -> model::TypeRef {
    let dotted = path.to_dotted();
    let str_args: Vec<String> = args.iter().map(ast::Value::to_text).collect();

    if let Some(bare) = path.as_ident().map(|i| i.text.as_str()) {
        if BUILTIN_TYPES.contains(&bare) {
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

    match env.ctx.lookup_decl(decl_owner, name) {
        Some(decl) if decl.kind == BlockKind::Enum => model::TypeRef {
            name: dotted,
            args: Vec::new(),
        },
        Some(decl) if decl.kind == BlockKind::Datatype => {
            check_datatype_args(owner, decl.block, args, span, diags);
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
            // Under postgres/mysql/oracledb, the type catalog is "not fully
            // finalized" per docs/13 – only a warning instead of an error.
            diags.push(Diagnostic::warning(
                owner,
                span,
                format!(
                    "unknown type `{dotted}` – the built-in type catalog for this `dialect` is not \
                     yet fully finalized per docs/13; carried over unchanged"
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
/// only the **number** of arguments is checked here, not their type. A full
/// per-argument type check is a sensible next step, but not (yet) part of
/// this analysis.
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
