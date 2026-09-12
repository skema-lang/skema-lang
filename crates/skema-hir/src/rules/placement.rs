//! Where a block is allowed to appear – a single, data-driven check for
//! `docs/13`, "Block quick reference", instead of a special rule per block
//! kind (Lint belongs in Project, Index in Table/Partial, Field in
//! Datatype/Enum, …).

use skema_syntax::ast::{self, BlockKind};

use crate::{Diagnostic, FileId};

/// Where an `Item` currently sits – the basis against which every block
/// kind checks its allowed parents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Parent {
    /// Top level of a file.
    Top,
    Block(BlockKind),
    /// The description body of a field (`{ note = "…"; }`) – **no**
    /// sub-block is allowed there, regardless of block kind.
    FieldBody,
}

/// `(block kind, allowed parents)`. If a block kind is missing here (e.g.
/// [`BlockKind::Other`] or a top-level declaration like `Table`, which via
/// [`BlockKind::is_declaration`] is only allowed at the root anyway), this
/// check leaves it untouched.
fn allowed_parents(kind: BlockKind) -> Option<&'static [Parent]> {
    use BlockKind::*;
    Some(match kind {
        Project | Document | Table | Partial | Enum | Group | Datatype => &[Parent::Top],
        Meta => &[
            Parent::Block(Project),
            Parent::Block(Document),
            Parent::Block(Table),
            Parent::Block(Partial),
            Parent::Block(Enum),
            Parent::Block(Group),
            Parent::Block(Datatype),
        ],
        Style => &[Parent::Block(Project), Parent::Block(Document)],
        Lint => &[Parent::Block(Project)],
        Database => &[
            Parent::Block(Project),
            Parent::Block(Document),
            Parent::Block(Table),
        ],
        Index => &[Parent::Block(Table), Parent::Block(Partial)],
        Dialect => &[Parent::Block(Datatype), Parent::Block(Database)],
        // The `Field` description of a type (`docs/06`, `docs/08`).
        Field => &[Parent::Block(Datatype), Parent::Block(Enum)],
        Example => &[Parent::Block(Table)],
        Record => &[Parent::Block(Example)],
        Other => return None,
    })
}

pub(crate) fn check(id: FileId, file: &ast::SourceFile, diags: &mut Vec<Diagnostic>) {
    walk(id, &file.items, Parent::Top, diags);
}

fn walk(id: FileId, items: &[ast::Item], parent: Parent, diags: &mut Vec<Diagnostic>) {
    for item in items {
        match &item.kind {
            ast::ItemKind::Block(b) => {
                if let Some(allowed) = allowed_parents(b.kind) {
                    if !allowed.contains(&parent) {
                        diags.push(Diagnostic::error(id, item.span, message(b.kind, allowed)));
                    }
                }
                walk(id, &b.items, Parent::Block(b.kind), diags);
            }
            ast::ItemKind::Field(f) => walk(id, &f.items, Parent::FieldBody, diags),
            _ => {}
        }
    }
}

fn message(kind: BlockKind, allowed: &[Parent]) -> String {
    let places: Vec<&str> = allowed
        .iter()
        .map(|p| match p {
            Parent::Top => "the top level of a file",
            Parent::Block(k) => k.as_str(),
            Parent::FieldBody => "a field body",
        })
        .collect();
    format!(
        "`{}` is not allowed here – only inside: {}",
        kind.as_str(),
        places.join(", ")
    )
}
