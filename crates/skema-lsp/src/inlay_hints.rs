//! `textDocument/inlayHint`: three small, easy-to-miss facts made visible
//! without opening the docs or scrolling to a `Partial`'s own declaration
//! (`docs/07`, `docs/09`, brain task 76).
//!
//! * Every field with a resolved `>`/`-` relation gets the physical column
//!   name it creates — the `[column="…"]` override, or (the detail readers
//!   miss, docs/07) the field's own name when there is none. A `<`
//!   back-reference creates no column at all and gets no hint.
//! * Every `use Partial;` directly in a `Table` gets the fields it brought
//!   in, in the table's flattened order — transitively, through any `use`
//!   nested inside that `Partial`. A `use` nested inside a `Partial` gets no
//!   hint of its own.
//! * Every field declared directly in a `Partial` gets the `Table`s
//!   (anywhere in the project) whose flattened field list contains it, in
//!   the same order `model::Table`s are already sorted — namespace, then
//!   name.
//!
//! All three read only `Analysis::model`/`references`/`project` — no second
//! name-resolution pass, and no `skema-hir`/`skema-api` change.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use skema_api::{ExportedField, FlagValue, RelationKind};
use skema_syntax::ast::{self, AstNode, BlockKind, ItemKind};

use crate::analysis::{Analysis, display_path};
use crate::convert;

pub fn inlay_hints(
    analysis: &Analysis,
    path: &Path,
    range: lsp_types::Range,
) -> Vec<lsp_types::InlayHint> {
    let Some(file) = analysis.file(path) else {
        return Vec::new();
    };
    let src = file.src.as_str();
    let from = display_path(file);

    let mut hints = relation_hints(analysis, from, src);
    hints.extend(use_line_hints(analysis, from, file, src));
    hints.extend(partial_field_hints(analysis, from, file, src));

    // The client-visible order, and what makes "only within `range`"
    // checkable without caring which of the three passes produced a hint.
    hints.sort_by_key(|h| (h.position.line, h.position.character));

    let start = convert::offset_of(src, range.start) as u32;
    let end = convert::offset_of(src, range.end) as u32;
    hints.retain(|h| {
        let offset = convert::offset_of(src, h.position) as u32;
        start <= offset && offset <= end
    });
    hints
}

// ===========================================================================
// The FK column name
// ===========================================================================

fn relation_hints(analysis: &Analysis, from: &str, src: &str) -> Vec<lsp_types::InlayHint> {
    // A field inherited via `use` appears once per `Table` it was flattened
    // into, but its declaration site — and so its hint — is one; keyed by
    // span to collapse the duplicates.
    let mut by_span: BTreeMap<(u32, u32), &ExportedField> = BTreeMap::new();
    for field in analysis
        .model
        .tables
        .iter()
        .flat_map(|t| &t.fields)
        .filter(|f| f.file == from)
    {
        let Some(relation) = &field.relation else {
            continue;
        };
        // A `<` back-reference creates no column (docs/07, "Not every
        // relationship creates a column") — nothing to show.
        if relation.kind == RelationKind::ManyToOne {
            continue;
        }
        by_span.insert((field.span.start, field.span.end), field);
    }

    by_span
        .into_values()
        .map(|f| end_of_line_hint(src, f.span.end, format!("column={}", column_name(f))))
        .collect()
}

/// The physical column name a relation field creates: the `column` flag's
/// text if set, the field's own name otherwise (docs/07, docs/11).
fn column_name(field: &ExportedField) -> String {
    field
        .flags
        .iter()
        .find(|flag| flag.name == "column")
        .and_then(|flag| match &flag.value {
            FlagValue::Text(text) => Some(text.clone()),
            FlagValue::Bool(_) => None,
        })
        .unwrap_or_else(|| field.name.clone())
}

// ===========================================================================
// `use` line (Table side)
// ===========================================================================

fn use_line_hints(
    analysis: &Analysis,
    from: &str,
    file: &skema_loader::LoadedFile,
    src: &str,
) -> Vec<lsp_types::InlayHint> {
    let mut hints = Vec::new();
    for item in file.ast.items() {
        let ItemKind::Block(table_block) = item.kind() else {
            continue;
        };
        if table_block.kind() != BlockKind::Table {
            continue;
        }
        let Some(table_name) = table_block.name().and_then(|p| p.as_ident()) else {
            continue;
        };
        let Some(table) = analysis
            .model
            .tables
            .iter()
            .find(|t| t.file == from && t.name == table_name.text())
        else {
            continue;
        };

        for use_item in table_block.items() {
            if !matches!(use_item.kind(), ItemKind::Use(_)) {
                continue;
            }
            let Some(hint) = use_hint(analysis, from, src, table, &use_item) else {
                continue;
            };
            hints.push(hint);
        }
    }
    hints
}

/// The hint for one `use` item directly in a `Table`'s body — `None` when
/// the `use` didn't resolve to a `Partial` (already diagnosed elsewhere) or
/// brought in no field the table kept.
fn use_hint(
    analysis: &Analysis,
    from: &str,
    src: &str,
    table: &skema_api::ExportedTable,
    use_item: &ast::Item,
) -> Option<lsp_types::InlayHint> {
    let link = analysis
        .references
        .iter()
        .find(|r| r.from_file == from && span_contains(use_item.span(), r.from_span))?;
    let target_file = analysis
        .project
        .files
        .iter()
        .find(|f| display_path(f) == link.to_file)?;
    let name = &src[link.from_span.start as usize..link.from_span.end as usize];
    let target_block = target_file.ast.declaration(BlockKind::Partial, name)?;

    let mut reachable = HashSet::new();
    let mut visited = HashSet::new();
    collect_partial_fields(
        analysis,
        target_file,
        &target_block,
        name,
        &mut visited,
        &mut reachable,
    );

    let field_names: Vec<&str> = table
        .fields
        .iter()
        .filter(|f| reachable.contains(&(f.file.clone(), f.span.start, f.span.end)))
        .map(|f| f.name.as_str())
        .collect();
    if field_names.is_empty() {
        return None;
    }

    Some(end_of_line_hint(
        src,
        use_item.span().end,
        format!("→ {}", field_names.join(", ")),
    ))
}

/// Every field reachable from a `Partial`'s own body — directly declared, or
/// through a `use` nested inside it, transitively. `(file, span)` pairs, to
/// match against `ExportedField::file`/`span` without caring about names.
///
/// `visited` guards the same cycle a nested `use` can form in the model
/// itself (`resolve.rs::flatten_partial`) — a cycle here just stops instead
/// of contributing more fields, same as there.
fn collect_partial_fields(
    analysis: &Analysis,
    owner: &skema_loader::LoadedFile,
    block: &ast::Block,
    name: &str,
    visited: &mut HashSet<(String, String)>,
    out: &mut HashSet<(String, u32, u32)>,
) {
    let owner_path = display_path(owner).to_owned();
    if !visited.insert((owner_path.clone(), name.to_owned())) {
        return;
    }
    for item in block.items() {
        match item.kind() {
            ItemKind::Field(_) => {
                let span = item.span();
                out.insert((owner_path.clone(), span.start, span.end));
            }
            ItemKind::Use(_) => {
                let Some(link) = analysis
                    .references
                    .iter()
                    .find(|r| r.from_file == owner_path && span_contains(item.span(), r.from_span))
                else {
                    continue;
                };
                let Some(target) = analysis
                    .project
                    .files
                    .iter()
                    .find(|f| display_path(f) == link.to_file)
                else {
                    continue;
                };
                let nested_name =
                    &owner.src[link.from_span.start as usize..link.from_span.end as usize];
                let Some(nested_block) = target.ast.declaration(BlockKind::Partial, nested_name)
                else {
                    continue;
                };
                collect_partial_fields(analysis, target, &nested_block, nested_name, visited, out);
            }
            _ => {}
        }
    }
}

// ===========================================================================
// Partial field (Partial side)
// ===========================================================================

fn partial_field_hints(
    analysis: &Analysis,
    from: &str,
    file: &skema_loader::LoadedFile,
    src: &str,
) -> Vec<lsp_types::InlayHint> {
    let mut hints = Vec::new();
    for item in file.ast.items() {
        let ItemKind::Block(block) = item.kind() else {
            continue;
        };
        if block.kind() != BlockKind::Partial {
            continue;
        }
        for field_item in block.items() {
            if !matches!(field_item.kind(), ItemKind::Field(_)) {
                continue;
            }
            let field_span = field_item.span();
            // `analysis.model.tables` is already sorted by (namespace,
            // name) — the deterministic order this hint lists tables in.
            let table_names: Vec<&str> = analysis
                .model
                .tables
                .iter()
                .filter(|t| {
                    t.fields.iter().any(|f| {
                        f.file == from
                            && f.span.start == field_span.start
                            && f.span.end == field_span.end
                    })
                })
                .map(|t| t.name.as_str())
                .collect();
            if table_names.is_empty() {
                continue;
            }
            hints.push(end_of_line_hint(
                src,
                field_span.end,
                format!("← {}", table_names.join(", ")),
            ));
        }
    }
    hints
}

// ===========================================================================
// Shared plumbing
// ===========================================================================

fn end_of_line_hint(src: &str, anchor: u32, label: String) -> lsp_types::InlayHint {
    lsp_types::InlayHint {
        position: convert::position_of(src, end_of_line(src, anchor) as usize),
        label: lsp_types::InlayHintLabel::String(label),
        kind: None,
        text_edits: None,
        tooltip: None,
        padding_left: Some(true),
        padding_right: None,
        data: None,
    }
}

/// The byte offset just before the next line break after `anchor` — where a
/// trailing hint on that line belongs.
fn end_of_line(src: &str, anchor: u32) -> u32 {
    let anchor = anchor as usize;
    match src.get(anchor..).and_then(|s| s.find('\n')) {
        Some(i) => (anchor + i) as u32,
        None => src.len() as u32,
    }
}

fn span_contains(outer: skema_syntax::Span, inner: skema_api::Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        for (name, contents) in files {
            fs::write(dir.path().join(name), contents).expect("write fixture");
        }
        let root = dir.path().join(files[0].0);
        (dir, root)
    }

    fn build(root: &Path) -> Analysis {
        Analysis::build(root, false, |p| fs::read_to_string(p)).expect("analysis")
    }

    fn full_range(src: &str) -> lsp_types::Range {
        lsp_types::Range::new(
            lsp_types::Position::new(0, 0),
            convert::position_of(src, src.len()),
        )
    }

    /// The position a hint anchored on `needle`'s line should render at —
    /// mirrors `end_of_line` without depending on it.
    fn end_of_line_position(src: &str, needle: &str) -> lsp_types::Position {
        let start = src.find(needle).expect("needle in source");
        let eol = src[start..].find('\n').map_or(src.len(), |i| start + i);
        convert::position_of(src, eol)
    }

    fn label_text(hint: &lsp_types::InlayHint) -> &str {
        match &hint.label {
            lsp_types::InlayHintLabel::String(s) => s.as_str(),
            lsp_types::InlayHintLabel::LabelParts(_) => panic!("expected a plain string label"),
        }
    }

    #[test]
    fn a_relation_with_no_override_hints_the_field_name_as_the_column() {
        let main = "spec 1;\nDocument shop {}\n\
                     Table Customer { id int [pk]; }\n\
                     Table Order { customer >Customer.id; }\n";
        let (_dir, root) = workspace(&[("main.skema", main)]);
        let analysis = build(&root);

        let hints = inlay_hints(&analysis, &root, full_range(main));
        assert_eq!(hints.len(), 1, "{hints:?}");
        assert_eq!(label_text(&hints[0]), "column=customer");
        assert_eq!(hints[0].position, end_of_line_position(main, "Table Order"));
    }

    #[test]
    fn a_column_override_is_reflected_instead_of_the_field_name() {
        let main = "spec 1;\nDocument shop {}\n\
                     Table Customer { id int [pk]; }\n\
                     Table Order { customer >Customer.id [column=\"cust_id\"]; }\n";
        let (_dir, root) = workspace(&[("main.skema", main)]);
        let analysis = build(&root);

        let hints = inlay_hints(&analysis, &root, full_range(main));
        assert_eq!(hints.len(), 1, "{hints:?}");
        assert_eq!(label_text(&hints[0]), "column=cust_id");
    }

    #[test]
    fn a_back_reference_creates_no_column_and_gets_no_hint() {
        let main = "spec 1;\nDocument shop {}\n\
                     Table Customer { id int [pk]; orders <Order.customer; }\n\
                     Table Order { customer >Customer.id; }\n";
        let (_dir, root) = workspace(&[("main.skema", main)]);
        let analysis = build(&root);

        let hints = inlay_hints(&analysis, &root, full_range(main));
        // Only the `>` side (`Order.customer`) gets a hint - `<` creates no
        // column (docs/07).
        assert_eq!(hints.len(), 1, "{hints:?}");
        assert_eq!(label_text(&hints[0]), "column=customer");
        assert_eq!(hints[0].position, end_of_line_position(main, "Table Order"));
    }

    #[test]
    fn a_plain_field_gets_no_hint() {
        let main = "spec 1;\nDocument shop {}\nTable T { id int [pk]; note varchar; }\n";
        let (_dir, root) = workspace(&[("main.skema", main)]);
        let analysis = build(&root);

        assert!(inlay_hints(&analysis, &root, full_range(main)).is_empty());
    }

    #[test]
    fn a_use_of_a_same_file_partial_lists_its_fields_on_the_use_line() {
        let main = "spec 1;\nDocument shop {}\n\
                     Partial Stamped { createdAt string; updatedAt string; }\n\
                     Table Order { id int [pk]; use Stamped; }\n";
        let (_dir, root) = workspace(&[("main.skema", main)]);
        let analysis = build(&root);

        let hints = inlay_hints(&analysis, &root, full_range(main));
        let use_hint = hints
            .iter()
            .find(|h| label_text(h).starts_with('→'))
            .expect("a use-line hint");
        assert_eq!(label_text(use_hint), "→ createdAt, updatedAt");
        assert_eq!(
            use_hint.position,
            end_of_line_position(main, "use Stamped;")
        );
    }

    #[test]
    fn a_field_declared_in_a_partial_lists_the_table_that_inherits_it() {
        let main = "spec 1;\nDocument shop {}\n\
                     Partial Stamped { createdAt string; updatedAt string; }\n\
                     Table Order { id int [pk]; use Stamped; }\n";
        let (_dir, root) = workspace(&[("main.skema", main)]);
        let analysis = build(&root);

        let hints = inlay_hints(&analysis, &root, full_range(main));
        let created_hint = hints
            .iter()
            .find(|h| h.position == end_of_line_position(main, "createdAt string;"))
            .expect("a hint on createdAt's line");
        assert_eq!(label_text(created_hint), "← Order");
    }

    #[test]
    fn a_use_of_a_partial_in_an_imported_file_still_resolves() {
        let common = "spec 1;\nDocument common {}\nPartial Stamped { createdAt string; }\n";
        // A prefix is mandatory for an imported declaration (docs/02,
        // "Referencing within vs. across files").
        let main = "spec 1;\nDocument shop {}\nimport \"./common.skema\";\n\
                     Table Order { id int [pk]; use common.Stamped; }\n";
        let (_dir, root) = workspace(&[("main.skema", main), ("common.skema", common)]);
        let analysis = build(&root);

        let hints = inlay_hints(&analysis, &root, full_range(main));
        let use_hint = hints
            .iter()
            .find(|h| label_text(h).starts_with('→'))
            .expect("a use-line hint");
        assert_eq!(label_text(use_hint), "→ createdAt");

        let common_path = root.with_file_name("common.skema");
        let common_hints = inlay_hints(&analysis, &common_path, full_range(common));
        assert_eq!(common_hints.len(), 1, "{common_hints:?}");
        assert_eq!(label_text(&common_hints[0]), "← Order");
    }

    #[test]
    fn a_partial_used_by_two_tables_in_different_files_lists_both() {
        let common = "spec 1;\nDocument common {}\nPartial Stamped { createdAt string; }\n";
        let order = "spec 1;\nDocument shop {}\nimport \"./common.skema\";\n\
                      Table Order { id int [pk]; use common.Stamped; }\n";
        let invoice = "spec 1;\nDocument shop {}\nimport \"./common.skema\";\n\
                        Table Invoice { id int [pk]; use common.Stamped; }\n";
        let main = "spec 1;\nDocument shop {}\n\
                     import \"./order.skema\";\nimport \"./invoice.skema\";\n";
        let (_dir, root) = workspace(&[
            ("main.skema", main),
            ("order.skema", order),
            ("invoice.skema", invoice),
            ("common.skema", common),
        ]);
        let analysis = build(&root);

        let common_path = root.with_file_name("common.skema");
        let hints = inlay_hints(&analysis, &common_path, full_range(common));
        assert_eq!(hints.len(), 1, "{hints:?}");
        // Sorted the same way `model.tables` already is: namespace, then
        // name - both tables share the `shop` namespace, so alphabetically
        // by name.
        assert_eq!(label_text(&hints[0]), "← Invoice, Order");
    }

    #[test]
    fn a_partial_reached_only_through_a_nested_use_flattens_transitively() {
        let main = "spec 1;\nDocument shop {}\n\
                     Partial Inner { createdAt string; }\n\
                     Partial Outer {\n  use Inner;\n  updatedAt string;\n}\n\
                     Table Order { id int [pk]; use Outer; }\n";
        let (_dir, root) = workspace(&[("main.skema", main)]);
        let analysis = build(&root);

        let hints = inlay_hints(&analysis, &root, full_range(main));

        // The table-side hint on `use Outer;` lists both of Outer's own
        // field and Inner's, transitively, in flattened order.
        let outer_use_hint = hints
            .iter()
            .find(|h| h.position == end_of_line_position(main, "use Outer;"))
            .expect("a hint on the `use Outer;` line");
        assert_eq!(label_text(outer_use_hint), "→ createdAt, updatedAt");

        // `use Inner;` sits inside a `Partial`, not a `Table` - out of
        // scope, no hint of its own.
        assert!(
            !hints
                .iter()
                .any(|h| h.position == end_of_line_position(main, "use Inner;"))
        );

        // Inner's own field lists the table that reached it transitively.
        let inner_field_hint = hints
            .iter()
            .find(|h| h.position == end_of_line_position(main, "createdAt string;"))
            .expect("a hint on Inner's own field");
        assert_eq!(label_text(inner_field_hint), "← Order");

        let outer_field_hint = hints
            .iter()
            .find(|h| h.position == end_of_line_position(main, "updatedAt string;"))
            .expect("a hint on Outer's own field");
        assert_eq!(label_text(outer_field_hint), "← Order");
    }

    #[test]
    fn a_partial_field_no_table_inherits_gets_no_hint() {
        let main = "spec 1;\nDocument shop {}\n\
                     Partial Unused { note string; }\n\
                     Table T { id int [pk]; }\n";
        let (_dir, root) = workspace(&[("main.skema", main)]);
        let analysis = build(&root);

        assert!(inlay_hints(&analysis, &root, full_range(main)).is_empty());
    }

    #[test]
    fn only_hints_within_the_requested_range_are_returned() {
        let main = "spec 1;\nDocument shop {}\n\
                     Partial Stamped { createdAt string; updatedAt string; }\n\
                     Table Order { id int [pk]; use Stamped; }\n";
        let (_dir, root) = workspace(&[("main.skema", main)]);
        let analysis = build(&root);

        assert_eq!(inlay_hints(&analysis, &root, full_range(main)).len(), 3);

        let partial_start = main.find("Partial Stamped").expect("Partial line") as u32;
        let partial_end = main.find("Table Order").expect("Table line") as u32;
        let narrow = convert::range_of(
            main,
            skema_syntax::Span {
                start: partial_start,
                end: partial_end,
            },
        );

        let hints = inlay_hints(&analysis, &root, narrow);
        assert_eq!(hints.len(), 2, "{hints:?}");
        assert!(hints.iter().all(|h| label_text(h).starts_with('←')));
    }
}
