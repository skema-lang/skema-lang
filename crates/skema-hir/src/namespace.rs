//! Visible namespaces: the file's own `Document` name plus imports/aliases
//! (`docs/02`).

use std::collections::HashMap;

use skema_syntax::Span;

use crate::symbols::FileFacts;
use crate::{Diagnostic, FileId};

/// An import as needed by the `unusedImport` lint: the name under which it
/// is visible, and where it comes from.
#[derive(Debug, Clone)]
pub(crate) struct ImportRecord {
    pub visible_as: String,
    pub span: Span,
}

#[derive(Debug, Default)]
pub(crate) struct Namespaces {
    /// Index = `FileId.0 as usize`. Name under which another file is
    /// visible → its `FileId`. Also contains the entry for the file's own
    /// `Document` name, because a qualified path within the same file looks
    /// syntactically no different from one crossing an import boundary.
    pub visible: Vec<HashMap<String, FileId>>,
    pub imports: Vec<Vec<ImportRecord>>,
}

impl Namespaces {
    pub fn resolve_namespace(&self, file: FileId, name: &str) -> Option<FileId> {
        self.visible.get(file.0 as usize)?.get(name).copied()
    }
}

/// Builds the namespace table of each file from its own `Document` name and
/// its imports. `docs/02`, "Namespace uniqueness": every name visible here
/// must be unique, otherwise it's an error.
pub(crate) fn build(facts: &[FileFacts], diags: &mut Vec<Diagnostic>) -> Namespaces {
    let mut visible = vec![HashMap::new(); facts.len()];
    let mut imports = vec![Vec::new(); facts.len()];

    for file in facts {
        let idx = file.id.0 as usize;

        if let Some((name, span, _)) = &file.document {
            insert_namespace(
                &mut visible[idx],
                name.to_string(),
                file.id,
                file.id,
                *span,
                diags,
            );
        }

        for (i, (item_span, import)) in file.imports.iter().enumerate() {
            let target = file.import_targets.get(i).copied().flatten();

            match (target, import.path()) {
                (None, Some(path)) => {
                    diags.push(Diagnostic::error(
                        file.id,
                        *item_span,
                        format!("import target not found: {path}"),
                    ));
                    continue;
                }
                (None, None) => continue, // Syntax error is already reported.
                _ => {}
            }
            let Some(target) = target else { continue };

            let (visible_as, name_span) = match import.alias() {
                Some(alias) => (alias.text().to_owned(), alias.span()),
                None => match facts
                    .iter()
                    .find(|f| f.id == target)
                    .and_then(|f| f.document.as_ref())
                {
                    Some((name, ..)) => (name.to_string(), *item_span),
                    // Target file has no `Document` block – already reported there.
                    None => continue,
                },
            };

            imports[idx].push(ImportRecord {
                visible_as: visible_as.clone(),
                span: *item_span,
            });
            insert_namespace(
                &mut visible[idx],
                visible_as,
                target,
                file.id,
                name_span,
                diags,
            );
        }
    }

    Namespaces { visible, imports }
}

fn insert_namespace(
    map: &mut HashMap<String, FileId>,
    name: String,
    target: FileId,
    reporting_file: FileId,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    if map.contains_key(&name) {
        diags.push(Diagnostic::error(
            reporting_file,
            span,
            format!(
                "namespace `{name}` is already taken in this file – use `as <other-name>` on the \
                 second occurrence (docs/02, \"Namespace uniqueness\")"
            ),
        ));
        return;
    }
    map.insert(name, target);
}
