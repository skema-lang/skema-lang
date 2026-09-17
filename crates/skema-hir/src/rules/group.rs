//! `Group`: member resolution, kind checking, uniqueness (`docs/12`).

use std::collections::HashMap;

use skema_syntax::Severity;
use skema_syntax::ast::{self, AstNode};

use super::Ctx;
use crate::model;
use crate::{Diagnostic, FileId};

pub(crate) fn check(
    ctx: &Ctx,
    dangling_ref_severity: Severity,
    links: &mut Vec<model::ReferenceLink>,
    diags: &mut Vec<Diagnostic>,
) -> Vec<model::Group> {
    // Every declaration may belong to at most one group – for that we keep
    // track of who claimed it first.
    let mut claimed: HashMap<(FileId, String), String> = HashMap::new();
    let mut out = Vec::new();

    for file in &ctx.facts {
        for (group_name, _name_span, item_span, block) in &file.groups {
            let mut members = Vec::new();

            for item in block.items() {
                let ast::ItemKind::Ref(r) = item.kind() else {
                    continue;
                };
                let path = r.path();

                let Some((owner, name, name_span, rest)) = ctx.resolve_decl_path(file.id, &path)
                else {
                    continue;
                };
                if !rest.is_empty() {
                    diags.push(Diagnostic::error(
                        file.id,
                        path.span(),
                        "a group reference names only the declaration, no further path after it",
                    ));
                    continue;
                }

                let Some(decl) = ctx.lookup_decl(owner, &name) else {
                    diags.push(Diagnostic::new(
                        file.id,
                        dangling_ref_severity,
                        name_span,
                        format!("unknown declaration `{name}`"),
                    ));
                    continue;
                };
                if decl.kind != r.kind() {
                    diags.push(Diagnostic::error(
                        file.id,
                        item.span(),
                        format!(
                            "`{name}` is a `{}`, expected `{}` (docs/12, \"The kind is checked\")",
                            decl.kind.as_str(),
                            r.kind().as_str()
                        ),
                    ));
                    continue;
                }
                links.push(model::ReferenceLink {
                    from_file: file.id,
                    from_span: name_span,
                    to_file: owner,
                    to_span: decl.name_span,
                });

                let key = (owner, name.to_string());
                if let Some(existing_group) = claimed.get(&key) {
                    diags.push(Diagnostic::error(
                        file.id,
                        name_span,
                        format!(
                            "`{name}` already belongs to group `{existing_group}` – a declaration \
                             belongs to at most one group (docs/12)"
                        ),
                    ));
                    continue;
                }
                claimed.insert(key, group_name.to_string());

                let namespace = ctx
                    .file(owner)
                    .and_then(|f| f.document.as_ref())
                    .map(|(n, ..)| n.to_string())
                    .unwrap_or_default();
                members.push(model::GroupMember {
                    kind: decl.kind.as_str().to_owned(),
                    namespace,
                    name: name.to_string(),
                    span: item.span(),
                });
            }

            out.push(model::Group {
                file: file.id,
                name: group_name.to_string(),
                members,
                span: *item_span,
            });
        }
    }

    out
}
