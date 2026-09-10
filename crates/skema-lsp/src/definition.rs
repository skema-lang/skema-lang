//! `textDocument/definition`: from a name to where it is declared.
//!
//! The browser editor does this by double-clicking a table in the diagram;
//! in an editor it is the name under the caret, which can be four different
//! things - a type, a relation target, a `use`d partial, an `import` path.
//!
//! # What this does and does not resolve
//!
//! Name resolution proper is `skema-hir`'s (`rules/resolve.rs`). This only
//! needs *where a name was written*, and answers from two facts the tree
//! carries directly: the file's `Document` name and its `import … as`
//! aliases. A wrong guess costs a jump that does not happen, never a wrong
//! diagnostic — so correctness stays with the checker.

use std::path::Path;

use skema_syntax::Span;
use skema_syntax::ast::{BlockKind, ItemKind};

use crate::analysis::Analysis;
use crate::convert;
use crate::symbols::{self, Decl};

/// Every declaration kind a name in a type or relation position can point at.
const TARGET_KINDS: &[BlockKind] = &[
    BlockKind::Table,
    BlockKind::Enum,
    BlockKind::Datatype,
    BlockKind::Partial,
    BlockKind::Group,
];

pub fn definition(analysis: &Analysis, path: &Path, offset: u32) -> Option<lsp_types::Location> {
    let file = analysis.file(path)?;

    // An `import "./types.skema";` jumps to the file itself - resolved by
    // the loader already, so this is a lookup rather than a path join.
    if let Some(target) = import_target(analysis, file, offset) {
        return Some(target);
    }

    let (segments, index) = path_at(&file.src, offset)?;
    let looked_up = &segments[..=index];
    let (target_path, decl) = resolve(analysis, file, looked_up)?;

    let target = analysis.file(&target_path)?;
    Some(lsp_types::Location {
        uri: convert::url_of(&target_path)?,
        range: convert::range_of(&target.src, decl.name_span),
    })
}

/// The declaration `segments` names, as (file, declaration).
fn resolve(
    analysis: &Analysis,
    from: &skema_loader::LoadedFile,
    segments: &[String],
) -> Option<(std::path::PathBuf, Decl)> {
    let own = symbols::declarations(&from.ast);

    match segments {
        [name] => {
            // Same file first: a name written unqualified most often means
            // the one next to it.
            if let Some(decl) = symbols::find_kind(&own, name, TARGET_KINDS) {
                return Some((from.path.clone(), decl.clone()));
            }
            // Then anything the file imports, then the rest of the project -
            // an unqualified name from another namespace is a `skema-hir`
            // error, but jumping to it is still the useful answer.
            imported_files(analysis, from)
                .into_iter()
                .chain(analysis.project.files.iter().map(|f| f.path.clone()))
                .find_map(|path| {
                    let file = analysis.file(&path)?;
                    let decls = symbols::declarations(&file.ast);
                    let decl = symbols::find_kind(&decls, name, TARGET_KINDS)?;
                    Some((path, decl.clone()))
                })
        }
        [first, second] => {
            // `types.Email` - a namespace and a declaration in it.
            if let Some(path) = namespace_file(analysis, from, first) {
                let file = analysis.file(&path)?;
                let decls = symbols::declarations(&file.ast);
                if let Some(decl) = symbols::find_kind(&decls, second, TARGET_KINDS) {
                    return Some((path, decl.clone()));
                }
            }
            // `Customer.id` - a declaration and one of its members.
            let (path, table) = resolve(analysis, from, std::slice::from_ref(first))?;
            let member = symbols::find(&table.children, second)?;
            Some((path, member.clone()))
        }
        [namespace, declaration, member] => {
            let path = namespace_file(analysis, from, namespace)?;
            let file = analysis.file(&path)?;
            let decls = symbols::declarations(&file.ast);
            let target = symbols::find_kind(&decls, declaration, TARGET_KINDS)?;
            let member = symbols::find(&target.children, member)?;
            Some((path, member.clone()))
        }
        _ => None,
    }
}

/// The file a namespace refers to: this file's own `Document` name, or an
/// `import`'s alias, or the `Document` name of anything it imports.
fn namespace_file(
    analysis: &Analysis,
    from: &skema_loader::LoadedFile,
    namespace: &str,
) -> Option<std::path::PathBuf> {
    if symbols::document_name(&from.ast).as_deref() == Some(namespace) {
        return Some(from.path.clone());
    }

    // `import_targets` is positionally aligned with `ast.imports()` - a
    // contract of `skema_loader`, and the only way back from an alias to a
    // file.
    for (import, target) in from.ast.imports().zip(&from.import_targets) {
        let Some(file) = target.and_then(|id| analysis.project.files.get(id.0 as usize)) else {
            continue;
        };
        let name = match &import.alias {
            Some(alias) => alias.text.clone(),
            None => symbols::document_name(&file.ast)?,
        };
        if name == namespace {
            return Some(file.path.clone());
        }
    }
    None
}

fn imported_files(analysis: &Analysis, from: &skema_loader::LoadedFile) -> Vec<std::path::PathBuf> {
    from.import_targets
        .iter()
        .filter_map(|target| {
            let id = (*target)?;
            Some(analysis.project.files.get(id.0 as usize)?.path.clone())
        })
        .collect()
}

/// When `offset` sits on an `import`'s path string, the file it names.
fn import_target(
    analysis: &Analysis,
    from: &skema_loader::LoadedFile,
    offset: u32,
) -> Option<lsp_types::Location> {
    let mut imports = 0;
    for item in &from.ast.items {
        if !matches!(item.kind, ItemKind::Import(_)) {
            continue;
        }
        let index = imports;
        imports += 1;
        if !contains(item.span, offset) {
            continue;
        }
        let id = (*from.import_targets.get(index)?)?;
        let target = analysis.project.files.get(id.0 as usize)?;
        return Some(lsp_types::Location {
            uri: convert::url_of(&target.path)?,
            // The top of the file: an import names a file, not a place in it.
            range: lsp_types::Range::default(),
        });
    }
    None
}

/// The dotted path the caret is inside, split into segments, plus which
/// segment that is - so `Customer.id` jumps to the table from its first half
/// and to the field from its second.
fn path_at(src: &str, offset: u32) -> Option<(Vec<String>, usize)> {
    use skema_syntax::TokenKind;

    let tokens: Vec<_> = skema_syntax::tokenize(src)
        .into_iter()
        .filter(|t| !matches!(t.kind, TokenKind::Whitespace))
        .collect();

    let hit = tokens
        .iter()
        .position(|t| t.kind == TokenKind::Ident && contains_inclusive(t.span, offset))?;

    // Walk out to both ends of the `a.b.c` chain the hit token belongs to.
    let mut start = hit;
    while start >= 2
        && tokens[start - 1].kind == TokenKind::Dot
        && tokens[start - 2].kind == TokenKind::Ident
    {
        start -= 2;
    }
    let mut end = hit;
    while end + 2 < tokens.len()
        && tokens[end + 1].kind == TokenKind::Dot
        && tokens[end + 2].kind == TokenKind::Ident
    {
        end += 2;
    }

    let segments: Vec<String> = (start..=end)
        .step_by(2)
        .filter_map(|i| text_of(src, tokens[i].span).map(str::to_owned))
        .collect();
    Some((segments, (hit - start) / 2))
}

fn text_of(src: &str, span: Span) -> Option<&str> {
    src.get(span.start as usize..span.end as usize)
}

const fn contains(span: Span, offset: u32) -> bool {
    span.start <= offset && offset < span.end
}

/// A caret sitting just past the last character of a name is still on it -
/// that is where it lands after double-clicking a word.
const fn contains_inclusive(span: Span, offset: u32) -> bool {
    span.start <= offset && offset <= span.end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_and_index(source_with_caret: &str) -> (Vec<String>, usize) {
        let offset = source_with_caret.find('|').expect("a caret");
        let source = source_with_caret.replace('|', "");
        path_at(&source, offset as u32).expect("a path under the caret")
    }

    #[test]
    fn a_bare_name_is_one_segment() {
        let (segments, index) = path_and_index("Table T { c >Cust|omer.id; }");
        assert_eq!(segments, vec!["Customer", "id"]);
        assert_eq!(index, 0, "the caret is on the table half");
    }

    #[test]
    fn the_caret_on_the_member_half_selects_it() {
        let (segments, index) = path_and_index("Table T { c >Customer.i|d; }");
        assert_eq!(segments, vec!["Customer", "id"]);
        assert_eq!(index, 1);
    }

    #[test]
    fn a_namespaced_path_keeps_all_three_segments() {
        let (segments, index) = path_and_index("Table T { c >cat.Co|py.id; }");
        assert_eq!(segments, vec!["cat", "Copy", "id"]);
        assert_eq!(index, 1);
    }

    #[test]
    fn a_caret_just_past_a_name_still_belongs_to_it() {
        let (segments, index) = path_and_index("Table T { email varchar|; }");
        assert_eq!(segments, vec!["varchar"]);
        assert_eq!(index, 0);
    }
}
