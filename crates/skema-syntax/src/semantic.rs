//! Classifies AST identifier spans by *role* – block keyword, declaration
//! name, field name, type reference, flag name, `{}`-setting key – for
//! syntax highlighting that goes beyond what the lexer alone can tell.
//!
//! The lexer cannot tell any of this apart: block keywords are ordinary
//! `Ident` tokens to it (`docs/01`, "Case conventions and keywords"), and a
//! field name, a type name and a flag name differ only by *where* they sit
//! in the tree. This module is that lookup, built once per parse.
//!
//! Like `dump.rs` (whose traversal it mirrors) it walks in source order, so
//! the result is ascending by span. Both consumers rely on that:
//! `skema_api::highlight` merges it against the token stream in one pass,
//! and `skema-lsp` serves it as semantic tokens.
//!
//! Deliberately conservative. Anything needing real name resolution — is
//! `paid` in `default = paid;` an enum value, a bare setting word or a
//! typo? — is left unclassified: mislabeling text mid-edit is worse than
//! not labeling it.

use crate::Span;
use crate::legacy_ast::*;

/// A role an identifier-ish span plays in the source, independent of what
/// the lexer alone can determine from the characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticKind {
    /// `Table`, `Meta`, `Style`, … – the word that opens (or references) a
    /// recognized block.
    BlockKeyword,
    /// A qualifying path segment before the last one: `shop` in
    /// `shop.OrderStatus`, or `cat` in a relation target `cat.Copy.id`.
    ///
    /// Not always a true cross-file namespace — `Customer` in `Customer.id`
    /// is a table — but "the qualifying part before the leaf" is the
    /// distinction a reader wants, and separating the two exactly needs
    /// name resolution the tree alone does not have.
    Namespace,
    /// The name of a declaration: `Table Customer`, `Enum OrderStatus`, an
    /// index's own name, a `Group` member reference (`Table shop.User;`).
    DeclarationName,
    /// A field/column/parameter/enum-entry name, or a name referenced as
    /// one (an index's column list).
    FieldName,
    /// A type or relationship-target reference: `varchar`, `decimal(10,2)`,
    /// `>Customer.id`.
    TypeRef,
    /// A key inside `[]` – changes the generated schema (`docs/01`, 1.7).
    FlagName,
    /// A key inside `{}` – documentation only, never DDL (`docs/01`, 1.7).
    MetaKey,
}

/// Classifies every identifier-ish span in `file`, in ascending source
/// order (the same order [`crate::tokenize`] yields tokens in – callers
/// that merge the two streams rely on this).
pub fn classify(file: &SourceFile) -> Vec<(Span, SemanticKind)> {
    let mut out = Vec::new();
    for item in &file.items {
        classify_item(item, &mut out);
    }
    out
}

fn classify_item(item: &Item, out: &mut Vec<(Span, SemanticKind)>) {
    match &item.kind {
        ItemKind::Block(b) => classify_block(b, out),
        ItemKind::Field(f) => classify_field(f, out),
        ItemKind::Ref(r) => {
            if r.kind != BlockKind::Other {
                out.push((r.keyword.span, SemanticKind::BlockKeyword));
            }
            classify_path(&r.path, SemanticKind::DeclarationName, out);
        }
        ItemKind::IndexEntry(e) => classify_index_entry(e, out),
        ItemKind::Setting(a) => classify_attr(a, SemanticKind::MetaKey, out),
        ItemKind::Spec(_) | ItemKind::Import(_) | ItemKind::Use(_) | ItemKind::Error => {}
    }
}

fn classify_block(b: &Block, out: &mut Vec<(Span, SemanticKind)>) {
    if b.kind != BlockKind::Other {
        out.push((b.keyword.span, SemanticKind::BlockKeyword));
    }
    if let Some(name) = &b.name {
        classify_path(name, SemanticKind::DeclarationName, out);
    }
    for param in &b.params {
        out.push((param.name.span, SemanticKind::FieldName));
        if let Some(ty) = &param.ty {
            classify_path(ty, SemanticKind::TypeRef, out);
        }
    }
    for item in &b.items {
        classify_item(item, out);
    }
}

fn classify_field(f: &Field, out: &mut Vec<(Span, SemanticKind)>) {
    out.push((f.name.span, SemanticKind::FieldName));
    if let Some(ty) = &f.ty {
        match &ty.kind {
            FieldTypeKind::Named { path, .. } => classify_path(path, SemanticKind::TypeRef, out),
            FieldTypeKind::Relation { target, .. } => {
                classify_path(target, SemanticKind::TypeRef, out);
            }
        }
    }
    for flag in &f.flags {
        classify_attr(flag, SemanticKind::FlagName, out);
    }
    for item in &f.items {
        classify_item(item, out);
    }
}

fn classify_index_entry(e: &IndexEntry, out: &mut Vec<(Span, SemanticKind)>) {
    if let Some(name) = &e.name {
        out.push((name.span, SemanticKind::DeclarationName));
    }
    for column in &e.columns {
        out.push((column.span, SemanticKind::FieldName));
    }
    for flag in &e.flags {
        classify_attr(flag, SemanticKind::FlagName, out);
    }
}

/// `key_kind` is `FlagName` for a `[]` flag and `MetaKey` for a `{}`
/// setting – same `Attr` node, different meaning depending on which
/// bracket it was written in (`docs/01`, 1.7), so the caller decides.
fn classify_attr(a: &Attr, key_kind: SemanticKind, out: &mut Vec<(Span, SemanticKind)>) {
    out.push((a.key.span, key_kind));
    // `type = decimal(10);` (docs/06, "Layering types"): unlike a bare
    // `ValueKind::Path` value (deliberately left unclassified, see the
    // module doc), a `Type` value is unambiguously a type reference by
    // construction, no name resolution needed to know that much.
    if let Some(Value {
        kind: ValueKind::Type { path, .. },
        ..
    }) = &a.value
    {
        classify_path(path, SemanticKind::TypeRef, out);
    }
}

/// Pushes `Namespace` for every segment but the last, and `leaf_kind` for
/// the last one. A single-segment path is just `leaf_kind`.
fn classify_path(path: &Path, leaf_kind: SemanticKind, out: &mut Vec<(Span, SemanticKind)>) {
    let Some((last, prefix)) = path.segments.split_last() else {
        return;
    };
    for segment in prefix {
        out.push((segment.span, SemanticKind::Namespace));
    }
    out.push((last.span, leaf_kind));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn classify_src(src: &str) -> Vec<(Span, SemanticKind)> {
        let (file, diags) = parse(src);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        classify(&file)
    }

    fn kinds_at(out: &[(Span, SemanticKind)], src: &str, needle: &str) -> Vec<SemanticKind> {
        let start = src.find(needle).expect("needle not found") as u32;
        let end = start + needle.len() as u32;
        out.iter()
            .filter(|(span, _)| span.start >= start && span.end <= end)
            .map(|(_, k)| *k)
            .collect()
    }

    #[test]
    fn spans_are_ascending() {
        let src = r#"
            spec 1;
            Table Customer {
              id    int     [pk];
              email varchar [unique];
            }
        "#;
        let out = classify_src(src);
        for w in out.windows(2) {
            assert!(w[0].0.start <= w[1].0.start, "not ascending: {out:?}");
        }
    }

    #[test]
    fn classifies_block_keyword_and_declaration_name() {
        let out = classify_src("Table Customer { id int [pk]; }");
        assert_eq!(
            kinds_at(&out, "Table Customer { id int [pk]; }", "Table"),
            vec![SemanticKind::BlockKeyword]
        );
        assert_eq!(
            kinds_at(&out, "Table Customer { id int [pk]; }", "Customer"),
            vec![SemanticKind::DeclarationName]
        );
    }

    #[test]
    fn does_not_classify_the_keyword_of_an_unknown_block() {
        // `BlockKind::Other` is only reachable via the `@selector` form
        // with an unrecognized leading word (`grammar.rs::parse_item_kind`
        // only dispatches to block parsing when the word is a known
        // keyword or is followed by `@`) – a bare unrecognized word like
        // `postgres { … }` parses as a `Field`, not a `Block`, and so gets
        // (correctly) classified as `FieldName` instead, exercised by
        // `meta_key_vs_flag_key`'s sibling cases elsewhere in this file.
        let src = "Foo @bar { x = 1; }";
        let out = classify_src(src);
        assert!(kinds_at(&out, src, "Foo").is_empty());
    }

    #[test]
    fn relation_target_is_type_ref_with_namespace_prefix() {
        let src = "Table Order { customer >cat.Customer.id [onDelete=cascade]; }";
        let out = classify_src(src);
        assert_eq!(
            kinds_at(&out, src, "customer"),
            vec![SemanticKind::FieldName]
        );
        assert_eq!(kinds_at(&out, src, "cat"), vec![SemanticKind::Namespace]);
        assert_eq!(
            kinds_at(&out, src, "onDelete"),
            vec![SemanticKind::FlagName]
        );
    }

    #[test]
    fn meta_key_vs_flag_key() {
        let src = r#"Table T { x int [pk] { note = "n"; } }"#;
        let out = classify_src(src);
        assert_eq!(kinds_at(&out, src, "pk"), vec![SemanticKind::FlagName]);
        assert_eq!(kinds_at(&out, src, "note"), vec![SemanticKind::MetaKey]);
    }
}
