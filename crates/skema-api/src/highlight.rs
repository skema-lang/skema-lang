//! `highlightTokens`: one merged, UTF-16-offset token stream for a CM6-style
//! editor. It combines the lexer's lossless coverage (comments, strings and
//! numbers stay colored mid syntax-error) with the AST-driven
//! classification from `skema_syntax::classify` (block keywords,
//! field/type/flag names, which the lexer cannot tell apart).
//!
//! A *flat* `Vec<u32>` of `[class, utf16Start, utf16End]` triples, not a
//! `Vec` of structs: this runs on every keystroke, and one fewer allocation
//! and JS object per token keeps it cheap. [`highlight_class_names`]
//! supplies the `class` ↔ name mapping so the codes are never duplicated in
//! TypeScript.

use skema_syntax::{SemanticKind, Span, TokenKind};

use crate::location::Utf16Offsets;

/// Declares the highlight classes once, as both the internal enum used for
/// matching and the `CLASS_NAMES` table `highlightClassNames()` hands to
/// JS – so the two can never drift apart into mismatched indices.
macro_rules! highlight_classes {
    ($($name:ident = $text:literal),* $(,)?) => {
        #[repr(u32)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum Class {
            $($name,)*
        }

        const CLASS_NAMES: &[&str] = &[$($text),*];
    };
}

highlight_classes! {
    Comment = "comment",
    String = "string",
    Number = "number",
    Raw = "raw",
    Punctuation = "punctuation",
    RelationOp = "relationOp",
    Directive = "directive",
    Ident = "ident",
    Error = "error",
    BlockKeyword = "blockKeyword",
    Namespace = "namespace",
    DeclarationName = "declarationName",
    FieldName = "fieldName",
    TypeRef = "typeRef",
    FlagName = "flagName",
    MetaKey = "metaKey",
}

/// `class` code ↔ name, index-matched: `CLASS_NAMES[class]` is the name
/// for that code. A plain list, not a map – the class codes in
/// [`highlight_tokens`]'s flat output *are* indexes into this same list, by
/// construction (see the `highlight_classes!` macro above).
pub fn highlight_class_names() -> Vec<String> {
    CLASS_NAMES.iter().map(|s| (*s).to_owned()).collect()
}

/// Trivia and `Eof` carry no highlight of their own – `None` skips them
/// entirely rather than emitting an empty-range entry.
fn class_of_token(kind: TokenKind) -> Option<Class> {
    use TokenKind::{
        At, BlockComment, Colon, Comma, Dot, Eof, Eq, Float, Gt, Ident, Int, KwAs, KwImport,
        KwSpec, KwUse, LBrace, LBracket, LParen, LineComment, Lt, Minus, RBrace, RBracket, RParen,
        Raw, Semi, Str, Unknown, UnterminatedComment, UnterminatedRaw, UnterminatedStr, Whitespace,
    };
    Some(match kind {
        Whitespace | Eof => return None,
        LineComment | BlockComment => Class::Comment,
        Str | UnterminatedStr => Class::String,
        Int | Float => Class::Number,
        Raw | UnterminatedRaw => Class::Raw,
        UnterminatedComment | Unknown => Class::Error,
        KwSpec | KwImport | KwAs | KwUse => Class::Directive,
        Gt | Lt | Minus => Class::RelationOp,
        LBrace | RBrace | LParen | RParen | LBracket | RBracket | Comma | Semi | Dot | Eq | At
        | Colon => Class::Punctuation,
        Ident => Class::Ident,
    })
}

fn class_of_semantic(kind: SemanticKind) -> Class {
    match kind {
        SemanticKind::BlockKeyword => Class::BlockKeyword,
        SemanticKind::Namespace => Class::Namespace,
        SemanticKind::DeclarationName => Class::DeclarationName,
        SemanticKind::FieldName => Class::FieldName,
        SemanticKind::TypeRef => Class::TypeRef,
        SemanticKind::FlagName => Class::FlagName,
        SemanticKind::MetaKey => Class::MetaKey,
    }
}

/// One call, one parse: lexes and parses `source` once each, merges the
/// lossless token stream with the AST-driven semantic classification, and
/// returns `[class, utf16Start, utf16End]` triples flattened into one
/// array – see the module doc for why this shape.
///
/// No debounce needed on the caller's side: the hand-written parser is
/// fast and `skema-lsp` already re-parses in full on every `didChange`
/// (`TextDocumentSyncKind::FULL`) – the same precedent applies here.
pub fn highlight_tokens(source: &str) -> Vec<u32> {
    let offsets = Utf16Offsets::new(source);
    let classified = classified(source);

    let mut out = Vec::with_capacity(classified.len() * 3);
    for (class, span) in classified {
        push_span(&mut out, &offsets, span, class as u32);
    }
    out
}

/// The merged classification itself, in **byte** spans – what
/// [`highlight_tokens`] turns into UTF-16 triples for an editor, and what
/// [`crate::context`] reads to tell where the caret is.
///
/// Shared rather than done twice: the merge below is the only place that
/// knows a semantic entry wins over a lexical one, and completion asking
/// the question a second way is how the two would come to disagree about
/// what a token is.
pub(crate) fn classified(source: &str) -> Vec<(Class, Span)> {
    let tokens = skema_syntax::tokenize(source);
    let (parsed, _) = skema_syntax::parse(source);
    let semantic = skema_syntax::classify(&parsed);

    let mut out = Vec::with_capacity(tokens.len());
    let mut semantic = semantic.into_iter().peekable();
    for tok in tokens {
        let Some(fallback) = class_of_token(tok.kind) else {
            continue;
        };
        // `next_if` consumes the semantic entry only when it belongs to
        // *this* token, so an entry that hasn't been reached yet stays put
        // for a later token – that is the whole two-pointer merge.
        let class = match semantic.next_if(|(span, _)| spans_match(*span, tok.span)) {
            Some((_, kind)) => class_of_semantic(kind),
            None => fallback,
        };
        out.push((class, tok.span));
    }
    out
}

/// Semantic spans classify existing `Ident`/`Str` tokens; they never
/// invent new ranges (`skema_syntax::semantic`'s module doc), so an
/// exact-boundary match is the correct – and only needed – test here, no
/// partial-overlap handling.
fn spans_match(a: Span, b: Span) -> bool {
    a.start == b.start && a.end == b.end
}

fn push_span(out: &mut Vec<u32>, offsets: &Utf16Offsets, span: Span, class: u32) {
    let start = offsets.convert(span.start);
    let end = offsets.convert(span.end);
    if start == end {
        return; // nothing to underline/color
    }
    out.push(class);
    out.push(start);
    out.push(end);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes_in_order(source: &str) -> Vec<&'static str> {
        let flat = highlight_tokens(source);
        flat.chunks_exact(3)
            .map(|c| CLASS_NAMES[c[0] as usize])
            .collect()
    }

    #[test]
    fn class_names_and_flat_output_share_indexing() {
        let names = highlight_class_names();
        assert_eq!(names.len(), CLASS_NAMES.len());
        assert_eq!(names[Class::BlockKeyword as usize], "blockKeyword");
    }

    #[test]
    fn simple_table_gets_expected_classes_in_order() {
        let classes = classes_in_order("Table Customer { id int [pk]; }");
        assert_eq!(
            classes,
            vec![
                "blockKeyword",    // Table
                "declarationName", // Customer
                "punctuation",     // {
                "fieldName",       // id
                "typeRef",         // int – a type by position, builtin or not
                "punctuation",     // [
                "flagName",        // pk
                "punctuation",     // ]
                "punctuation",     // ;
                "punctuation",     // }
            ]
        );
    }

    #[test]
    fn broken_syntax_still_highlights_comments_and_strings() {
        // A dangling `{` (no closing brace) is a real parse error, but the
        // lossless tokenizer keeps working regardless (`docs/01`: "a parser
        // must never panic", the same stance extends to the lexer).
        let classes = classes_in_order("Table T { # a comment\n note = \"x\" {{{");
        assert!(classes.contains(&"comment"));
        assert!(classes.contains(&"string"));
    }

    #[test]
    fn relation_operator_gets_its_own_class() {
        let classes = classes_in_order("Table O { c >Customer.id; }");
        assert!(classes.contains(&"relationOp"));
        assert!(classes.contains(&"typeRef"));
    }
}
