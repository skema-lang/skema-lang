//! The declarations a file makes, read off the syntax tree.
//!
//! Two features want this list: the outline
//! (`textDocument/documentSymbol`) and go-to-definition, which needs
//! somewhere to jump *to*.
//!
//! Read from the AST, not the resolved model. The model answers "what does
//! this table consist of" better — partials applied, types resolved — but
//! it does not contain what an outline is about: a `Datatype`, a `Partial`,
//! the `Project` block. Those are declarations, and they live in the tree.

use skema_syntax::Span;
use skema_syntax::ast::{Block, BlockKind, Item, ItemKind, Path, SourceFile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclKind {
    Block(BlockKind),
    Field,
    Index,
    /// A `Group`'s member: a reference to a declaration elsewhere.
    Reference,
}

#[derive(Debug, Clone)]
pub struct Decl {
    pub kind: DeclKind,
    pub name: String,
    /// Just the name, for a jump that lands on the word rather than on the
    /// whole declaration.
    pub name_span: Span,
    /// Keyword through the closing brace - the outline's range.
    pub span: Span,
    /// The type of a field, the stored type of an enum: what the outline
    /// shows greyed out behind the name.
    pub detail: Option<String>,
    pub children: Vec<Decl>,
}

/// Every declaration in the file, nested as written.
pub fn declarations(file: &SourceFile) -> Vec<Decl> {
    file.items.iter().filter_map(decl_of).collect()
}

/// The file's own namespace: the name of its `Document` block (`docs/02`).
pub fn document_name(file: &SourceFile) -> Option<String> {
    file.blocks()
        .find(|b| b.kind == BlockKind::Document)
        .and_then(|b| ident_of(b.name.as_ref()))
        .map(|(text, _)| text)
}

/// Finds a declaration by name anywhere in `decls`, depth-first.
pub fn find<'a>(decls: &'a [Decl], name: &str) -> Option<&'a Decl> {
    for decl in decls {
        if decl.name == name {
            return Some(decl);
        }
        if let Some(found) = find(&decl.children, name) {
            return Some(found);
        }
    }
    None
}

/// Finds a top-level declaration of one kind - the lookup go-to-definition
/// wants, where a field called `Customer` must not shadow the table.
pub fn find_kind<'a>(decls: &'a [Decl], name: &str, kinds: &[BlockKind]) -> Option<&'a Decl> {
    decls
        .iter()
        .find(|d| d.name == name && matches!(d.kind, DeclKind::Block(k) if kinds.contains(&k)))
}

fn decl_of(item: &Item) -> Option<Decl> {
    match &item.kind {
        ItemKind::Block(block) => Some(block_decl(block, item.span)),
        ItemKind::Field(field) => Some(Decl {
            kind: DeclKind::Field,
            name: field.name.text.clone(),
            name_span: field.name.span,
            span: item.span,
            detail: field.ty.as_ref().map(type_text),
            children: Vec::new(),
        }),
        ItemKind::IndexEntry(entry) => {
            let name = entry.name.as_ref();
            Some(Decl {
                kind: DeclKind::Index,
                // An unnamed index has no name to show: the naming scheme is
                // an open language question (`docs/13`), so the checker
                // leaves it empty. The columns identify it well enough.
                name: name.map_or_else(|| format!("({})", column_list(entry)), |i| i.text.clone()),
                name_span: name.map_or(item.span, |i| i.span),
                span: item.span,
                detail: name.map(|_| column_list(entry)),
                children: Vec::new(),
            })
        }
        // `Table shop.User;` inside a `Group`: syntactically the start of a
        // block that ends in `;` instead of `{`, and the only thing a group
        // consists of - so an outline that skipped it would show every group
        // as empty.
        ItemKind::Ref(reference) => Some(Decl {
            kind: DeclKind::Reference,
            name: reference.path.to_dotted(),
            name_span: reference.path.span,
            span: item.span,
            detail: Some(reference.keyword.text.clone()),
            children: Vec::new(),
        }),
        _ => None,
    }
}

fn block_decl(block: &Block, span: Span) -> Decl {
    let named = ident_of(block.name.as_ref());
    // `Project { … }` and `Meta { … }` carry no name; the keyword is what
    // they are called, and the keyword is where a jump should land.
    let (name, name_span) = named.unwrap_or_else(|| {
        (
            block
                .selector
                .as_ref()
                .map_or_else(|| block.keyword.text.clone(), |s| format!("@{}", s.text)),
            block.keyword.span,
        )
    });

    Decl {
        kind: DeclKind::Block(block.kind),
        name,
        name_span,
        span,
        detail: (block.kind == BlockKind::Datatype && !block.params.is_empty())
            .then(|| params_text(block)),
        children: block.items.iter().filter_map(decl_of).collect(),
    }
}

fn ident_of(path: Option<&Path>) -> Option<(String, Span)> {
    let path = path?;
    path.as_ident()
        .map(|i| (i.text.clone(), i.span))
        // `Table shop.User` is a dotted name; whether that is allowed is the
        // checking phase's call, and an outline still has to show it.
        .or_else(|| Some((path.to_dotted(), path.span)))
}

fn type_text(ty: &skema_syntax::ast::FieldType) -> String {
    use skema_syntax::ast::FieldTypeKind::{Named, Relation};
    match &ty.kind {
        Named { path, args } if args.is_empty() => path.to_dotted(),
        Named { path, args } => format!(
            "{}({})",
            path.to_dotted(),
            args.iter()
                .map(|a| a.to_text())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Relation { op, target } => format!("{}{}", op.as_str(), target.to_dotted()),
    }
}

fn params_text(block: &Block) -> String {
    format!(
        "({})",
        block
            .params
            .iter()
            .map(|p| match &p.ty {
                Some(ty) => format!("{} {}", p.name.text, ty.to_dotted()),
                None => p.name.text.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn column_list(entry: &skema_syntax::ast::IndexEntry) -> String {
    entry
        .columns
        .iter()
        .map(|c| c.text.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> SourceFile {
        skema_syntax::parse(source).0
    }

    #[test]
    fn a_table_lists_its_fields_as_children() {
        let file = parse("Table Customer {\n  id int [pk];\n  email varchar;\n}");
        let decls = declarations(&file);
        let table = decls.first().expect("the table");
        assert_eq!(table.name, "Customer");
        assert_eq!(table.kind, DeclKind::Block(BlockKind::Table));
        let names: Vec<&str> = table.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["id", "email"]);
        assert_eq!(table.children[0].detail.as_deref(), Some("int"));
    }

    #[test]
    fn a_relation_field_shows_the_arrow_in_its_detail() {
        let file = parse("Table Order { customer >Customer.id; }");
        let decls = declarations(&file);
        let field = &decls[0].children[0];
        assert_eq!(field.detail.as_deref(), Some(">Customer.id"));
    }

    #[test]
    fn the_name_span_covers_the_name_alone() {
        let source = "Table Customer { id int; }";
        let file = parse(source);
        let decl = &declarations(&file)[0];
        assert_eq!(
            &source[decl.name_span.start as usize..decl.name_span.end as usize],
            "Customer",
            "a jump should land on the name, not on the keyword"
        );
    }

    #[test]
    fn an_unnamed_block_is_named_after_its_keyword() {
        let file = parse("Project { mode = draft; }");
        let decls = declarations(&file);
        assert_eq!(decls[0].name, "Project");
        assert_eq!(decls[0].kind, DeclKind::Block(BlockKind::Project));
    }

    #[test]
    fn a_datatype_shows_its_parameters() {
        let file = parse("Datatype money(precision int, scale int) { Field { type = decimal; } }");
        let decls = declarations(&file);
        assert_eq!(decls[0].name, "money");
        assert_eq!(
            decls[0].detail.as_deref(),
            Some("(precision int, scale int)")
        );
    }

    #[test]
    fn the_document_block_names_the_files_namespace() {
        let file = parse("spec 1;\nDocument shop {}\nTable T {}");
        assert_eq!(document_name(&file).as_deref(), Some("shop"));
    }

    #[test]
    fn find_kind_does_not_confuse_a_field_with_a_declaration() {
        let file = parse("Table Customer { Customer int; }");
        let decls = declarations(&file);
        let found = find_kind(&decls, "Customer", &[BlockKind::Table]).expect("the table");
        assert_eq!(found.kind, DeclKind::Block(BlockKind::Table));
    }

    #[test]
    fn a_groups_members_are_listed_as_its_children() {
        let file = parse("Group Core { Table Customer; Enum cat.Status; }");
        let decls = declarations(&file);
        let names: Vec<&str> = decls[0].children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["Customer", "cat.Status"]);
        assert_eq!(decls[0].children[0].kind, DeclKind::Reference);
    }

    #[test]
    fn an_unnamed_index_is_listed_by_its_columns() {
        let file = parse("Table T { Index { (a, b); } }");
        let decls = declarations(&file);
        let index_block = &decls[0].children[0];
        assert_eq!(index_block.children[0].name, "(a, b)");
    }
}
