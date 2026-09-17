//! Declaration collection: a purely file-local pass that extracts from a
//! file's AST what every other rule needs – without itself resolving
//! anything across file boundaries yet.

use std::collections::HashMap;

use skema_syntax::Span;
use skema_syntax::ast::{self, AstNode, BlockKind};
use smol_str::SmolStr;

use crate::{Diagnostic, FileId};

/// A top-level declaration: `Table`, `Partial`, `Enum`, or `Datatype`.
///
/// The four share a single namespace – `docs/12` ("a Table and an Enum can
/// never share a name in the same namespace") – hence one shared table
/// instead of four separate ones.
#[derive(Debug)]
pub(crate) struct Decl {
    pub kind: BlockKind,
    pub name_span: Span,
    /// The span of the whole declaration item (keyword through closing
    /// `}`/`;`), for pointing a diagram/editor tool at "the definition" as
    /// opposed to just the name token.
    pub item_span: Span,
    pub block: ast::Block,
}

/// Everything that can be said about a file without looking at other files.
#[derive(Debug)]
pub(crate) struct FileFacts {
    pub id: FileId,
    pub ast: ast::SourceFile,
    pub import_targets: Vec<Option<FileId>>,
    /// Name, span of the name, and the `Document` block itself (for its own
    /// `Database` sub-block, `docs/11`).
    pub document: Option<(SmolStr, Span, ast::Block)>,
    pub declarations: HashMap<SmolStr, Decl>,
    /// Name, span of the name, span of the whole `Group { … }` item, and the
    /// block itself.
    pub groups: Vec<(SmolStr, Span, Span, ast::Block)>,
    /// Every `import`, with the span of the whole statement (for
    /// diagnostics).
    pub imports: Vec<(Span, ast::Import)>,
    pub projects: Vec<(Span, ast::Block)>,
}

/// The effective namespace of a declaration in its own file, when no other
/// file overrides it: the declaration's own name.
pub(crate) fn collect_file_facts(
    id: FileId,
    file: ast::SourceFile,
    import_targets: Vec<Option<FileId>>,
    diags: &mut Vec<Diagnostic>,
) -> FileFacts {
    let mut document: Option<(SmolStr, Span, ast::Block)> = None;
    let mut declarations: HashMap<SmolStr, Decl> = HashMap::new();
    let mut groups: Vec<(SmolStr, Span, Span, ast::Block)> = Vec::new();
    let mut imports = Vec::new();
    let mut projects = Vec::new();

    for item in file.items() {
        let item_span = item.span();
        match item.kind() {
            ast::ItemKind::Import(im) => imports.push((item_span, im)),
            ast::ItemKind::Block(b) if b.kind() == BlockKind::Project => {
                projects.push((item_span, b));
            }
            ast::ItemKind::Block(b) if b.kind() == BlockKind::Document => {
                let Some(name) = b.name().and_then(|p| p.as_ident()) else {
                    diags.push(Diagnostic::error(
                        id,
                        item_span,
                        "`Document` needs a simple name, e.g. `Document shop { }`",
                    ));
                    continue;
                };
                if let Some((_, first_span, _)) = &document {
                    diags.push(Diagnostic::error(
                        id,
                        name.span(),
                        format!(
                            "second `Document` block in this file – the first is at {first_span:?}"
                        ),
                    ));
                } else {
                    document = Some((SmolStr::new(name.text()), name.span(), b));
                }
            }
            ast::ItemKind::Block(b) if b.kind() == BlockKind::Group => {
                let Some(name) = b.name().and_then(|p| p.as_ident()) else {
                    diags.push(Diagnostic::error(
                        id,
                        item_span,
                        "`Group` needs a simple name",
                    ));
                    continue;
                };
                let clash = groups
                    .iter()
                    .find(|(n, ..)| n == name.text())
                    .map(|(_, span, ..)| *span);
                match clash {
                    Some(existing_span) => diags.push(Diagnostic::error(
                        id,
                        name.span(),
                        format!(
                            "second group named `{}` – the first is at {existing_span:?}",
                            name.text()
                        ),
                    )),
                    None => groups.push((SmolStr::new(name.text()), name.span(), item_span, b)),
                }
            }
            ast::ItemKind::Block(b) if b.kind().is_declaration() => {
                let kind = b.kind();
                let Some(name) = b.name().and_then(|p| p.as_ident()) else {
                    diags.push(Diagnostic::error(
                        id,
                        item_span,
                        format!("`{}` needs a simple name", kind.as_str()),
                    ));
                    continue;
                };
                match declarations.get(name.text()) {
                    Some(prev) => diags.push(Diagnostic::error(
                        id,
                        name.span(),
                        format!(
                            "`{}` is already declared in this file as `{}` (at {:?})",
                            name.text(),
                            prev.kind.as_str(),
                            prev.name_span
                        ),
                    )),
                    None => {
                        declarations.insert(
                            SmolStr::new(name.text()),
                            Decl {
                                kind,
                                name_span: name.span(),
                                item_span,
                                block: b,
                            },
                        );
                    }
                }
            }
            _ => {}
        }
    }

    if document.is_none() {
        diags.push(Diagnostic::error(
            id,
            file.span(),
            "every file needs exactly one `Document` block (docs/02)",
        ));
    }

    FileFacts {
        id,
        ast: file,
        import_targets,
        document,
        declarations,
        groups,
        imports,
        projects,
    }
}
