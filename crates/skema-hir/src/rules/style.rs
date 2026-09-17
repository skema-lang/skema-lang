//! `Style` block property format validation: unquoted keys must be camelCase,
//! values must be strings.

use skema_syntax::legacy_ast::{self as ast, BlockKind, ValueKind};

use super::lint::{Case, matches_case};
use crate::{Diagnostic, FileId};

pub(crate) fn check(id: FileId, file: &ast::SourceFile, diags: &mut Vec<Diagnostic>) {
    walk(id, &file.items, diags);
}

fn walk(id: FileId, items: &[ast::Item], diags: &mut Vec<Diagnostic>) {
    for item in items {
        match &item.kind {
            ast::ItemKind::Block(b) if b.kind == BlockKind::Style => {
                check_style_block(id, b, diags);
                walk(id, &b.items, diags);
            }
            ast::ItemKind::Block(b) => walk(id, &b.items, diags),
            ast::ItemKind::Field(f) => walk(id, &f.items, diags),
            _ => {}
        }
    }
}

fn check_style_block(id: FileId, block: &ast::Block, diags: &mut Vec<Diagnostic>) {
    for item in &block.items {
        if let ast::ItemKind::Setting(attr) = &item.kind {
            check_style_property(id, attr, diags);
        }
    }
}

fn check_style_property(id: FileId, attr: &ast::Attr, diags: &mut Vec<Diagnostic>) {
    if !attr.key.quoted && !matches_case(&attr.key.text, Case::Camel) {
        diags.push(Diagnostic::error(
            id,
            attr.key.span,
            format!(
                "`{}` is not camelCase – Style property keys must be camelCase (docs/03)",
                attr.key.text
            ),
        ));
    }

    if let Some(value) = &attr.value {
        if !matches!(value.kind, ValueKind::Str(_)) {
            let kind_name = match &value.kind {
                ValueKind::Str(_) => unreachable!(),
                ValueKind::Path(_) => "identifier",
                ValueKind::Int(_) => "integer",
                ValueKind::Float(_) => "float",
                ValueKind::Raw(_) => "raw string",
                ValueKind::List(_) => "list",
                ValueKind::Type { .. } => "type",
            };
            diags.push(Diagnostic::error(
                id,
                value.span,
                format!(
                    "Style property values must be strings, not {} (docs/03)",
                    kind_name
                ),
            ));
        }
    }
}
