//! `textDocument/completion`, built from two real sources and no
//! hand-copied word lists:
//!
//! - [`skema_api::builtins`] - the very catalogs `skema-hir` validates
//!   against, so a suggested key is by construction a key the checker
//!   accepts;
//! - the current analysis - actual table, field, enum and datatype names
//!   from this project, including across `import` boundaries.
//!
//! Where the caret is comes from [`skema_api::cursor_context`], which reads
//! the language's own token stream. What it does *not* do is decide whether
//! a suggestion is allowed in this exact spot - that is the checker's job,
//! and it says so a moment later as a diagnostic.

use std::path::Path;

use lsp_types::{CompletionItem, CompletionItemKind, Documentation, MarkupContent, MarkupKind};
use skema_api::{
    Builtins, ContextKind, CursorContext, block_reference_markdown, cursor_context, flag_reference,
};
use skema_syntax::legacy_ast::BlockKind;

use crate::analysis::Analysis;
use crate::symbols::{self, DeclKind};

pub fn completion(analysis: &Analysis, path: &Path, offset: u32) -> Vec<CompletionItem> {
    let Some(file) = analysis.file(path) else {
        return Vec::new();
    };
    let context = cursor_context(&file.src, offset);
    let catalog = skema_api::builtins();

    match context.kind {
        ContextKind::Flags => {
            let keys = if context.block_keyword.as_deref() == Some("Index") {
                &catalog.index_flag_keys
            } else {
                &catalog.flag_keys
            };
            with_flag_docs(words(keys, CompletionItemKind::KEYWORD, None))
        }
        ContextKind::FlagValue => flag_values(&context, &catalog),
        ContextKind::RelationTarget => table_names(analysis),
        ContextKind::MemberOf => match context.qualifier.as_deref() {
            Some(qualifier) => members_of(analysis, qualifier),
            None => Vec::new(),
        },
        ContextKind::TypePosition => type_names(analysis, &catalog),
        ContextKind::SettingKey => setting_keys(&context, &catalog),
        ContextKind::StatementStart => {
            let mut items = with_block_docs(
                words(&catalog.block_keywords, CompletionItemKind::KEYWORD, None),
                &catalog,
            );
            items.extend(words(
                &["use".to_owned(), "import".to_owned(), "spec".to_owned()],
                CompletionItemKind::KEYWORD,
                None,
            ));
            // A settings block also accepts declarations, and a declaration
            // body sometimes accepts keys; offering both beats guessing.
            items.extend(setting_keys(&context, &catalog));
            items
        }
    }
}

/// `[onDelete=|]`: the fixed set of actions - nothing for a flag with no
/// documented value set (`default`, `column`, a plain boolean flag).
fn flag_values(context: &CursorContext, catalog: &Builtins) -> Vec<CompletionItem> {
    match context.qualifier.as_deref() {
        Some("onDelete" | "onUpdate") => words(
            &catalog.relation_action_values,
            CompletionItemKind::ENUM_MEMBER,
            None,
        ),
        _ => Vec::new(),
    }
}

/// Attaches the language reference's one-liner to a flag suggestion, where
/// one is documented - `docs/13`, "Flag reference".
fn with_flag_docs(mut items: Vec<CompletionItem>) -> Vec<CompletionItem> {
    for item in &mut items {
        if let Some(doc) = flag_reference(&item.label) {
            item.documentation = Some(markdown_doc(doc));
        }
    }
    items
}

/// Attaches the block quick reference to a block-keyword suggestion, so the
/// list is self-explanatory without a separate hover.
fn with_block_docs(mut items: Vec<CompletionItem>, catalog: &Builtins) -> Vec<CompletionItem> {
    for item in &mut items {
        if let Some(kind) = BlockKind::from_keyword(&item.label) {
            item.documentation = Some(markdown_doc(block_reference_markdown(kind, catalog)));
        }
    }
    items
}

fn markdown_doc(value: impl Into<String>) -> Documentation {
    Documentation::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value: value.into(),
    })
}

fn setting_keys(context: &CursorContext, catalog: &Builtins) -> Vec<CompletionItem> {
    let keys = match context.block_keyword.as_deref() {
        Some("Meta") => &catalog.meta_keys,
        Some("Database") => &catalog.database_keys,
        Some("Project") => &catalog.project_keys,
        Some("Lint") => &catalog.lint_keys,
        Some("Field") => &catalog.field_block_keys,
        // `Style`/`Dialect` word lists are explicitly unfinished in
        // `docs/13`, so there is nothing trustworthy to offer there yet.
        _ => return Vec::new(),
    };
    words(keys, CompletionItemKind::PROPERTY, None)
}

fn type_names(analysis: &Analysis, catalog: &Builtins) -> Vec<CompletionItem> {
    let mut items = words(
        &catalog.builtin_types,
        CompletionItemKind::TYPE_PARAMETER,
        Some("built-in"),
    );

    // `None` covers both `generic` and "no elected `Project`" - neither has
    // a dialect-specific catalog beyond the dialect-independent one above.
    if let Some(engine) = analysis.model.dialect.as_deref() {
        if let Some(dialect_types) = catalog.dialect_types.get(engine) {
            items.extend(words(
                dialect_types,
                CompletionItemKind::TYPE_PARAMETER,
                Some(engine),
            ));
        }
    }

    items.extend(analysis.model.enums.iter().map(|e| CompletionItem {
        label: qualified(&e.namespace, &e.name),
        kind: Some(CompletionItemKind::ENUM),
        detail: Some(format!("enum · {}", e.stored_type)),
        ..CompletionItem::default()
    }));

    // Datatypes are declarations, not model entities, so they come from the
    // trees rather than from the model - which is also why the browser
    // editor cannot offer them yet.
    for file in &analysis.project.files {
        let namespace = symbols::document_name(&file.ast).unwrap_or_default();
        for decl in symbols::declarations(&file.ast) {
            if decl.kind != DeclKind::Block(BlockKind::Datatype) {
                continue;
            }
            items.push(CompletionItem {
                label: qualified(&namespace, &decl.name),
                kind: Some(CompletionItemKind::STRUCT),
                detail: Some(format!("datatype{}", decl.detail.unwrap_or_default())),
                ..CompletionItem::default()
            });
        }
    }
    items
}

fn table_names(analysis: &Analysis) -> Vec<CompletionItem> {
    analysis
        .model
        .tables
        .iter()
        .map(|t| CompletionItem {
            label: qualified(&t.namespace, &t.name),
            kind: Some(CompletionItemKind::CLASS),
            detail: Some(t.file.clone()),
            ..CompletionItem::default()
        })
        .collect()
}

/// After `>Customer.`: that table's fields. After `>cat.`: the tables in
/// that namespace - a qualifier is either, and which one it is is decided by
/// what actually matches.
fn members_of(analysis: &Analysis, qualifier: &str) -> Vec<CompletionItem> {
    if let Some(table) = analysis
        .model
        .tables
        .iter()
        .find(|t| t.name == qualifier || qualified(&t.namespace, &t.name) == qualifier)
    {
        return table
            .fields
            .iter()
            .map(|f| CompletionItem {
                label: f.name.clone(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail: Some(f.ty.name.clone()),
                ..CompletionItem::default()
            })
            .collect();
    }

    analysis
        .model
        .tables
        .iter()
        .filter(|t| t.namespace == qualifier)
        .map(|t| CompletionItem {
            label: t.name.clone(),
            kind: Some(CompletionItemKind::CLASS),
            detail: Some(t.file.clone()),
            ..CompletionItem::default()
        })
        .collect()
}

fn words(items: &[String], kind: CompletionItemKind, detail: Option<&str>) -> Vec<CompletionItem> {
    items
        .iter()
        .map(|label| CompletionItem {
            label: label.clone(),
            kind: Some(kind),
            detail: detail.map(str::to_owned),
            ..CompletionItem::default()
        })
        .collect()
}

fn qualified(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_owned()
    } else {
        format!("{namespace}.{name}")
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

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

    #[test]
    fn a_postgres_project_offers_its_dialect_types_alongside_the_built_ins() {
        let field = "data ";
        let src = format!(
            "spec 1;\nDocument shop {{ }}\nProject {{ mode = draft; dialect = postgres; }}\nTable T {{ id int [pk]; {field}}}\n"
        );
        let (_dir, root) = workspace(&[("main.skema", &src)]);
        let analysis = build(&root);
        let offset = src.find(field).expect("field in source") as u32 + field.len() as u32;

        let items = completion(&analysis, &root, offset);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

        assert!(labels.contains(&"jsonb"), "{labels:?}");
        assert!(
            labels.contains(&"int8"),
            "postgres' alias for bigint: {labels:?}"
        );
        // Still offered alongside the dialect-independent catalog - this is
        // additive, not a replacement.
        assert!(labels.contains(&"int"), "{labels:?}");
        // A different dialect's types stay absent.
        assert!(!labels.contains(&"tinytext"), "mysql-only: {labels:?}");
    }

    #[test]
    fn a_generic_project_offers_only_the_dialect_independent_catalog() {
        let field = "data ";
        let src = format!(
            "spec 1;\nDocument shop {{ }}\nProject {{ mode = draft; dialect = generic; }}\nTable T {{ id int [pk]; {field}}}\n"
        );
        let (_dir, root) = workspace(&[("main.skema", &src)]);
        let analysis = build(&root);
        let offset = src.find(field).expect("field in source") as u32 + field.len() as u32;

        let items = completion(&analysis, &root, offset);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

        assert!(labels.contains(&"int"), "{labels:?}");
        assert!(!labels.contains(&"jsonb"), "postgres-only: {labels:?}");
    }
}
