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

use lsp_types::{CompletionItem, CompletionItemKind};
use skema_api::{Builtins, ContextKind, CursorContext, cursor_context};
use skema_syntax::ast::BlockKind;

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
            words(keys, CompletionItemKind::KEYWORD, None)
        }
        ContextKind::RelationTarget => table_names(analysis),
        ContextKind::MemberOf => match context.qualifier.as_deref() {
            Some(qualifier) => members_of(analysis, qualifier),
            None => Vec::new(),
        },
        ContextKind::TypePosition => type_names(analysis, &catalog),
        ContextKind::SettingKey => setting_keys(&context, &catalog),
        ContextKind::StatementStart => {
            let mut items = words(&catalog.block_keywords, CompletionItemKind::KEYWORD, None);
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
