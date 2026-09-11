//! `textDocument/hover`: the detail a line of DSL leaves out.
//!
//! `email varchar(120) [required, unique]` is all the text says. That the
//! relation cascades on delete, that the field was inlined from a `Partial`
//! in another file, that its type resolves through two `import`s — only the
//! resolved model carries that. The browser editor shows it in a card next
//! to the diagram; this is the same content where an editor puts it.
//!
//! Mostly reads the model, not the tree: a resolved fact beats a
//! description of the syntax. The exception is a block keyword itself
//! (`Document`, `Meta`, `Project`, …) - several of those carry no schema
//! meaning and so never appear in the resolved model at all, and hovering
//! one instead answers "what is this for, what goes inside it" from the
//! language's own reference (`keyword_at`, [`skema_api::block_reference_markdown`]).

use skema_api::{
    ExportedField, ExportedFlag, ExportedModel, ExportedTable, FlagValue, RelationKind, Span,
    block_reference_markdown,
};
use skema_syntax::ast::{BlockKind, Item, ItemKind};

use crate::analysis::Analysis;
use crate::convert;

/// The hover for `offset` in `file`, or `None` when the caret is on
/// something neither the model nor the language reference has anything to
/// add to (a brace, whitespace, a name already explained by its context).
pub fn hover(analysis: &Analysis, display_path: &str, offset: u32) -> Option<lsp_types::Hover> {
    let file = analysis
        .project
        .files
        .iter()
        .find(|f| crate::analysis::display_path(f) == display_path)?;

    let model_hit = describe(&analysis.model, display_path, offset);
    let keyword_hit = keyword_at(&file.ast.items, offset).map(|(kind, span)| {
        (
            block_reference_markdown(kind, &skema_api::builtins()),
            Span::from(span),
        )
    });

    // A resolved fact and a keyword reference can cover the same offset - a
    // `Table`'s span covers everything inside it, its own keyword included.
    // The narrower one is what the caret is actually on (same rule as
    // `describe`'s "innermost match wins").
    let (markdown, span) = match (model_hit, keyword_hit) {
        (Some(a), Some(b)) if width(b.1) < width(a.1) => b,
        (Some(a), _) => a,
        (None, Some(b)) => b,
        (None, None) => return None,
    };

    Some(lsp_types::Hover {
        contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
            kind: lsp_types::MarkupKind::Markdown,
            value: markdown,
        }),
        range: Some(convert::range_of(
            &file.src,
            skema_syntax::Span {
                start: span.start,
                end: span.end,
            },
        )),
    })
}

/// The innermost block keyword whose own word contains `offset`, walked
/// from the syntax tree rather than the model: `Document`, `Meta`,
/// `Project` and the other settings blocks carry no schema meaning of their
/// own, so they never appear in [`ExportedModel`] at all.
fn keyword_at(items: &[Item], offset: u32) -> Option<(BlockKind, skema_syntax::Span)> {
    for item in items {
        let ItemKind::Block(block) = &item.kind else {
            continue;
        };
        // `Other` is a name the language doesn't recognize (`postgres`
        // inside `Dialect`) - nothing in the reference to say about it, so
        // it is not a hit, but its children may still contain real ones.
        if block.kind != BlockKind::Other && syntax_contains(block.keyword.span, offset) {
            return Some((block.kind, block.keyword.span));
        }
        if let Some(found) = keyword_at(&block.items, offset) {
            return Some(found);
        }
    }
    None
}

const fn syntax_contains(span: skema_syntax::Span, offset: u32) -> bool {
    span.start <= offset && offset < span.end
}

/// Innermost match wins: a field's span sits inside its table's, and the
/// caret is on the field.
fn describe(model: &ExportedModel, file: &str, offset: u32) -> Option<(String, Span)> {
    let mut best: Option<(String, Span)> = None;
    let mut consider = |text: String, span: Span| {
        if !contains(span, offset) {
            return;
        }
        let better = best
            .as_ref()
            .is_none_or(|(_, current)| width(span) < width(*current));
        if better {
            best = Some((text, span));
        }
    };

    for table in &model.tables {
        if table.file == file {
            consider(table_markdown(table), table.span);
        }
        for field in &table.fields {
            if field.file == file {
                consider(field_markdown(field, table), field.span);
            }
        }
        for index in &table.indexes {
            if index.file == file {
                consider(index_markdown(index), index.span);
            }
        }
    }

    for enumeration in &model.enums {
        if enumeration.file != file {
            continue;
        }
        consider(enum_markdown(enumeration), enumeration.span);
        for entry in &enumeration.entries {
            let label = entry
                .label
                .as_deref()
                .map(|l| format!("\n\n{l}"))
                .unwrap_or_default();
            let value = entry
                .value
                .as_deref()
                .map(|v| format!("\n\nStored as `{v}`"))
                .unwrap_or_default();
            consider(
                format!(
                    "**{}** · entry of `{}`{label}{value}",
                    entry.name, enumeration.name
                ),
                entry.span,
            );
        }
    }

    for group in &model.groups {
        if group.file == file {
            consider(group_markdown(group), group.span);
        }
    }

    best
}

fn field_markdown(field: &ExportedField, table: &ExportedTable) -> String {
    let mut out = format!("**{}** `{}`", field.name, type_label(field));

    if let Some(label) = &field.label {
        out.push_str(&format!("\n\n{label}"));
    }
    if !field.flags.is_empty() {
        let flags: Vec<String> = field.flags.iter().map(flag_label).collect();
        out.push_str(&format!("\n\n{}", code_list(&flags)));
    }
    if let Some(note) = &field.note {
        out.push_str(&format!("\n\n{note}"));
    }

    let mut facts: Vec<String> = Vec::new();
    if let Some(relation) = &field.relation {
        facts.push(format!(
            "Kind: {}",
            match relation.kind {
                RelationKind::OneToMany => "many to one - this table holds the key",
                RelationKind::OneToOne => "one to one - this table holds the key, unique",
                RelationKind::ManyToOne => "back-reference - no column here",
            }
        ));
        if let Some(on_delete) = &relation.on_delete {
            facts.push(format!("On delete: {on_delete}"));
        }
        if let Some(on_update) = &relation.on_update {
            facts.push(format!("On update: {on_update}"));
        }
    }
    // Only worth saying when it differs from the table's own file, which is
    // exactly the `use`d-partial-from-another-file case.
    if field.file != table.file {
        facts.push(format!("From: {}", field.file));
    }
    append_facts(&mut out, &facts);
    out
}

fn table_markdown(table: &ExportedTable) -> String {
    let mut out = format!("**{}**", qualified(&table.namespace, &table.name));

    let columns = table
        .fields
        .iter()
        .filter(|f| {
            f.relation
                .as_ref()
                .is_none_or(|r| r.kind != RelationKind::ManyToOne)
        })
        .count();
    let mut facts = vec![format!("Columns: {columns}")];

    let physical: Vec<&str> = [
        table.physical.database.as_deref(),
        table.physical.schema.as_deref(),
        table.physical.table.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !physical.is_empty() {
        facts.push(format!("Physical: {}", physical.join(".")));
    }

    if !table.indexes.is_empty() {
        let indexes: Vec<String> = table
            .indexes
            .iter()
            .map(|index| {
                format!(
                    "{} ({})",
                    index.name.as_deref().unwrap_or("(unnamed)"),
                    index.columns.join(", ")
                )
            })
            .collect();
        facts.push(format!("Indexes: {}", indexes.join(", ")));
    }

    let back_references: Vec<String> = table
        .fields
        .iter()
        .filter_map(|f| {
            let relation = f.relation.as_ref()?;
            (relation.kind == RelationKind::ManyToOne)
                .then(|| format!("{} ({})", relation.target_table, f.name))
        })
        .collect();
    if !back_references.is_empty() {
        facts.push(format!("Has many: {}", back_references.join(", ")));
    }

    append_facts(&mut out, &facts);
    out
}

fn enum_markdown(enumeration: &skema_api::ExportedEnum) -> String {
    let mut out = format!(
        "**{}** · enum",
        qualified(&enumeration.namespace, &enumeration.name)
    );
    append_facts(
        &mut out,
        &[
            format!("Stored as: {}", enumeration.stored_type),
            format!("Entries: {}", enumeration.entries.len()),
        ],
    );
    out
}

fn group_markdown(group: &skema_api::ExportedGroup) -> String {
    let members: Vec<String> = group
        .members
        .iter()
        .map(|m| qualified(&m.namespace, &m.name))
        .collect();
    let mut out = format!("**{}** · group", group.name);
    append_facts(&mut out, &[format!("Members: {}", members.join(", "))]);
    out
}

fn index_markdown(index: &skema_api::ExportedIndex) -> String {
    let mut out = format!(
        "**{}** · index",
        index.name.as_deref().unwrap_or("(unnamed)")
    );
    let mut facts = vec![format!("Columns: {}", index.columns.join(", "))];
    if !index.flags.is_empty() {
        let flags: Vec<String> = index.flags.iter().map(flag_label).collect();
        facts.push(format!("Flags: {}", flags.join(", ")));
    }
    append_facts(&mut out, &facts);
    out
}

/// `varchar(120)`, or the relationship arrow as it is written.
fn type_label(field: &ExportedField) -> String {
    if let Some(relation) = &field.relation {
        let arrow = match relation.kind {
            RelationKind::ManyToOne => "<",
            RelationKind::OneToOne => "-",
            RelationKind::OneToMany => ">",
        };
        return format!(
            "{arrow}{}.{}",
            qualified(&relation.target_namespace, &relation.target_table),
            relation.target_field
        );
    }
    if field.ty.args.is_empty() {
        field.ty.name.clone()
    } else {
        format!("{}({})", field.ty.name, field.ty.args.join(", "))
    }
}

fn flag_label(flag: &ExportedFlag) -> String {
    match &flag.value {
        FlagValue::Bool(true) => flag.name.clone(),
        FlagValue::Bool(false) => format!("{}=false", flag.name),
        FlagValue::Text(text) => format!("{}={text}", flag.name),
    }
}

fn code_list(items: &[String]) -> String {
    items
        .iter()
        .map(|i| format!("`{i}`"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn append_facts(out: &mut String, facts: &[String]) {
    if facts.is_empty() {
        return;
    }
    out.push_str("\n\n");
    out.push_str(
        &facts
            .iter()
            .map(|f| format!("- {f}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

fn qualified(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_owned()
    } else {
        format!("{namespace}.{name}")
    }
}

const fn contains(span: Span, offset: u32) -> bool {
    span.start <= offset && offset < span.end
}

const fn width(span: Span) -> u32 {
    span.end.saturating_sub(span.start)
}
