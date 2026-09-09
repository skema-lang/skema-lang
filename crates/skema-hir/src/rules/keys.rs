//! The "known key or `x-`" rule from `docs/01`:
//!
//! > Known keys are validated and autocompleted. Custom keys must start with
//! > `x-` and be quoted. Anything else is an error.
//!
//! The rule applies everywhere the language has key/value pairs: in `Meta`,
//! in field bodies, in `[]` flags, in `Index` flags. What counts as a
//! "known" key depends on the surrounding block – hence a small table
//! instead of a single constant.
//!
//! `Style` and `Dialect` are deliberately missing from the tables: `docs/13`,
//! "Known limitations", lists both word lists as not yet finalized. There,
//! only the context-independent half of the rule is checked – a quoted key
//! without `x-` is always wrong, regardless of whether the list of
//! unquoted keys is already settled.

use skema_syntax::ast::{self, BlockKind};

use crate::catalog::{
    COLUMN_BODY_KEYS, DATABASE_KEYS, ENTRY_BODY_KEYS, FIELD_BLOCK_KEYS, FLAG_KEYS, INDEX_FLAG_KEYS,
    LINT_KEYS, META_KEYS, PARAM_BODY_KEYS, PROJECT_KEYS,
};
use crate::{Diagnostic, FileId};

/// Known keys for the *direct* settings of a block, e.g. `mode`/`dialect`
/// in `Project`.
fn known_settings_for(kind: BlockKind) -> Option<&'static [&'static str]> {
    match kind {
        BlockKind::Meta => Some(META_KEYS),
        BlockKind::Lint => Some(LINT_KEYS),
        BlockKind::Database => Some(DATABASE_KEYS),
        BlockKind::Project => Some(PROJECT_KEYS),
        _ => None,
    }
}

/// Known keys for the body of an [`ast::Field`] node – depending on whether
/// it's a table column, a `Datatype` parameter, or an `Enum` entry (all
/// three share the node type, see `skema_syntax::ast`).
fn known_field_body_for(enclosing: BlockKind) -> Option<&'static [&'static str]> {
    match enclosing {
        BlockKind::Datatype => Some(PARAM_BODY_KEYS),
        BlockKind::Enum => Some(ENTRY_BODY_KEYS),
        BlockKind::Table | BlockKind::Partial => Some(COLUMN_BODY_KEYS),
        _ => None,
    }
}

pub(crate) fn check(id: FileId, file: &ast::SourceFile, diags: &mut Vec<Diagnostic>) {
    walk(id, &file.items, None, diags);
}

fn walk(
    id: FileId,
    items: &[ast::Item],
    enclosing: Option<BlockKind>,
    diags: &mut Vec<Diagnostic>,
) {
    for item in items {
        match &item.kind {
            ast::ItemKind::Setting(a) => {
                check_attr(id, a, enclosing.and_then(known_settings_for), diags);
            }
            ast::ItemKind::Block(b) if b.kind == BlockKind::Field => {
                for sub in &b.items {
                    if let ast::ItemKind::Setting(a) = &sub.kind {
                        check_attr(id, a, Some(FIELD_BLOCK_KEYS), diags);
                    }
                }
            }
            ast::ItemKind::Block(b) => walk(id, &b.items, Some(b.kind), diags),
            ast::ItemKind::Field(f) => {
                let known = enclosing.and_then(known_field_body_for);
                for sub in &f.items {
                    if let ast::ItemKind::Setting(a) = &sub.kind {
                        check_attr(id, a, known, diags);
                    }
                }
                for flag in &f.flags {
                    check_attr(id, flag, Some(FLAG_KEYS), diags);
                }
            }
            ast::ItemKind::IndexEntry(e) => {
                for flag in &e.flags {
                    check_attr(id, flag, Some(INDEX_FLAG_KEYS), diags);
                }
            }
            _ => {}
        }
    }
}

fn check_attr(id: FileId, attr: &ast::Attr, known: Option<&[&str]>, diags: &mut Vec<Diagnostic>) {
    if attr.key.quoted {
        if !attr.key.text.starts_with("x-") {
            diags.push(Diagnostic::error(
                id,
                attr.key.span,
                format!(
                    "custom keys must start with `x-`, e.g. `\"x-{}\"` (docs/01)",
                    attr.key.text
                ),
            ));
        }
        return;
    }
    if let Some(known) = known {
        if !known.contains(&attr.key.text.as_str()) {
            diags.push(Diagnostic::error(
                id,
                attr.key.span,
                format!(
                    "unknown key `{}` – allowed here: {} (custom keys: `\"x-…\"`)",
                    attr.key.text,
                    known.join(", ")
                ),
            ));
        }
    }
}
