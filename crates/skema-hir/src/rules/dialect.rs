//! `Dialect @engine { … }` spelling (`docs/06`, "Dialect @engine blocks";
//! `docs/11`, "Engine-specific settings"): the block must carry a selector,
//! and that selector must name a known engine.
//!
//! `placement::check` already restricts where a `Dialect` block may sit
//! (inside `Datatype` or `Database`); this rule is orthogonal to that one –
//! it only looks at the block's own selector, wherever it occurs.

use skema_syntax::ast::{self, BlockKind};

use crate::catalog::DIALECT_ENGINES;
use crate::{Diagnostic, FileId};

pub(crate) fn check(id: FileId, file: &ast::SourceFile, diags: &mut Vec<Diagnostic>) {
    walk(id, file.items(), diags);
}

fn walk(id: FileId, items: impl Iterator<Item = ast::Item>, diags: &mut Vec<Diagnostic>) {
    for item in items {
        let ast::ItemKind::Block(b) = item.kind() else {
            continue;
        };
        if b.kind() == BlockKind::Dialect {
            check_selector(id, &b, diags);
        }
        walk(id, b.items(), diags);
    }
}

fn check_selector(id: FileId, block: &ast::Block, diags: &mut Vec<Diagnostic>) {
    let Some(selector) = block.selector() else {
        let Some(keyword) = block.keyword() else {
            return;
        };
        diags.push(Diagnostic::error(
            id,
            keyword.span(),
            format!(
                "`Dialect` needs an engine name, e.g. `Dialect @postgres {{ … }}` – allowed: {}",
                DIALECT_ENGINES.join(", ")
            ),
        ));
        return;
    };
    if !DIALECT_ENGINES.contains(&selector.text()) {
        diags.push(Diagnostic::error(
            id,
            selector.span(),
            format!(
                "unknown dialect engine `{}` – allowed: {}",
                selector.text(),
                DIALECT_ENGINES.join(", ")
            ),
        ));
    }
}
