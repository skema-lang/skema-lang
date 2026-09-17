//! `textDocument/foldingRange`: one range per brace pair in the syntax tree.
//!
//! Purely syntactic, like `tokens::semantic_tokens` — so folding works for a
//! buffer that isn't part of any project, and for one with a parse error:
//! whatever parsed still gets its gutter arrows.
//!
//! Broader than `symbols::declarations`, which only surfaces named
//! declarations and does not look inside a [`Field`]'s own body: a
//! `Field { … }` settings block (`Datatype`, `Enum`) and anything nested
//! under it would otherwise have no fold.

use lsp_types::{FoldingRange, FoldingRangeKind};
use skema_syntax::Span;
use skema_syntax::ast::{AstNode, Item, ItemKind, SourceFile};

use crate::convert;

pub fn folding_ranges(source: &str, file: &SourceFile) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();
    collect(file.items(), source, &mut ranges);
    ranges
}

fn collect(items: impl Iterator<Item = Item>, source: &str, ranges: &mut Vec<FoldingRange>) {
    for item in items {
        match item.kind() {
            ItemKind::Block(block) => {
                push(source, item.span(), ranges);
                collect(block.items(), source, ranges);
            }
            // A field's own `{ … }` body is not a `Block` node - `Field`
            // holds its body items directly - but is exactly as foldable.
            ItemKind::Field(field) if field.items().next().is_some() => {
                push(source, item.span(), ranges);
                collect(field.items(), source, ranges);
            }
            _ => {}
        }
    }
}

/// One block's span, keyword through closing brace - the same span
/// `symbols::Decl::span` uses. A block that never leaves its opening line
/// has nothing to fold.
fn push(source: &str, span: Span, ranges: &mut Vec<FoldingRange>) {
    let start_line = convert::position_of(source, span.start as usize).line;
    let end_line = convert::position_of(source, span.end as usize).line;
    if start_line == end_line {
        return;
    }
    ranges.push(FoldingRange {
        start_line,
        start_character: None,
        end_line,
        end_character: None,
        kind: Some(FoldingRangeKind::Region),
        collapsed_text: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges(source: &str) -> Vec<(u32, u32)> {
        let (file, _) = skema_syntax::parse(source);
        folding_ranges(source, &file)
            .iter()
            .map(|r| (r.start_line, r.end_line))
            .collect()
    }

    #[test]
    fn every_multiline_block_folds_and_nested_blocks_get_their_own() {
        let source = [
            "spec 1;",
            "Document shop {}",
            "",
            "Table Post {",
            "  id int [pk];",
            "  title varchar(200) [required];",
            "",
            "  Example {",
            "    Record {",
            "      id = 1;",
            "      title = \"Hello\";",
            "    }",
            "  }",
            "}",
            "",
            "Datatype money(precision int, scale int) {",
            "  precision { note = \"digits\"; }",
            "",
            "  Field {",
            "    type = float;",
            "  }",
            "",
            "  Dialect @postgres { sql = \"numeric\"; }",
            "}",
        ]
        .join("\n");

        let mut found = ranges(&source);
        found.sort_unstable();
        let mut expected = vec![
            (3, 13),  // Table Post
            (7, 12),  // Example
            (8, 11),  // Record
            (15, 23), // Datatype money
            (18, 20), // Field
        ];
        expected.sort_unstable();
        assert_eq!(found, expected);
    }

    #[test]
    fn a_single_line_block_has_no_fold() {
        assert_eq!(ranges("Table Empty {}"), Vec::new());
        assert_eq!(
            ranges("Table T { Meta { note = \"x\"; } }"),
            Vec::new(),
            "the whole statement sits on one line, nested block included"
        );
    }

    #[test]
    fn a_field_bodys_own_brace_pair_folds_independently_of_its_table() {
        let source = "Table T {\n  id int {\n    note = \"x\";\n  }\n}";
        assert_eq!(ranges(source), vec![(0, 4), (1, 3)]);
    }

    #[test]
    fn a_file_with_a_parse_error_still_folds_what_parsed() {
        // The stray `=` inside `Meta` is a parse error (`cascading_errors_are_suppressed`
        // in skema-syntax), but its own braces stay balanced, so the
        // enclosing `Table` is still a clean multi-line block.
        let source = "Table Bad {\n  Meta { = ; }\n  id int;\n}";
        let (file, messages) = skema_syntax::parse(source);
        assert!(!messages.is_empty(), "the fixture must contain an error");
        assert_eq!(folding_ranges(source, &file).len(), 1);
        assert_eq!(ranges(source), vec![(0, 3)]);
    }
}
