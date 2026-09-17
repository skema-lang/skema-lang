//! Reorders a file's top-level declarations into one canonical order and
//! drops every comment (`brain/tasks/84-skema-fmt-cli-sort-mode.md`).
//!
//! Canonical order: `spec`, `import`s (sorted by path), `Project`,
//! `Document`, then `Partial`/`Table`/`Enum`/`Datatype`/`Group`, each
//! alphabetized by name within its own kind. Comments are dropped
//! unconditionally, top-level or inside a block body: no AST node owns a
//! comment today ([task 66](../../../brain/tasks/66-lossless-syntax-tree-spike.md)),
//! so there is no way to say which moved declaration a comment belongs to,
//! and this pass sidesteps that question rather than guess.
//!
//! # Why refuse instead of guess
//!
//! A file with syntax errors, more than one `spec`/`Project`/`Document`, an
//! unnamed declaration, or a top-level item outside spec/import/the seven
//! declaration kinds all leave at least one open question about where
//! something belongs. Answering it silently risks losing or misplacing
//! real content, so [`reorder`] returns `Err` and leaves the input
//! conceptually untouched (the caller never even sees a rewritten string)
//! rather than take a guess.

use crate::ast::{self, AstNode, BlockKind};
use crate::token::{Token, TokenKind};
use crate::{Span, parse};

/// The five declaration kinds alphabetized within their own bucket, in
/// canonical order. `Project` and `Document` are declarations too
/// ([`BlockKind::is_declaration`]) but get their own slot ahead of these
/// instead, since at most one of each may exist.
const SORTED_KINDS: [BlockKind; 5] = [
    BlockKind::Partial,
    BlockKind::Table,
    BlockKind::Enum,
    BlockKind::Datatype,
    BlockKind::Group,
];

/// The result of a successful [`reorder`].
#[derive(Debug)]
pub struct Reordered {
    pub text: String,
    pub comments_removed: usize,
}

/// Reorders `src` into canonical order and drops every comment, then pipes
/// the result through [`crate::format`] so `--sort` output is indented
/// too. See the module docs for what makes this refuse rather than guess.
pub fn reorder(src: &str) -> Result<Reordered, String> {
    let (file, diagnostics) = parse(src);
    if !diagnostics.is_empty() {
        return Err(format!(
            "{} syntax error(s) — fix them first (`skema check`) before sorting",
            diagnostics.len()
        ));
    }

    let all_tokens = crate::lexer::tokenize(src);
    let comments_removed = all_tokens
        .iter()
        .filter(|t| matches!(t.kind, TokenKind::LineComment | TokenKind::BlockComment))
        .count();

    let mut spec: Option<String> = None;
    let mut imports: Vec<(String, String)> = Vec::new();
    let mut project: Option<String> = None;
    let mut document: Option<String> = None;
    let mut declarations: Vec<(usize, String, String)> = Vec::new();

    for item in file.items() {
        match item.kind() {
            ast::ItemKind::Spec(_) => {
                if spec.is_some() {
                    return Err("more than one `spec` directive".to_string());
                }
                spec = Some(strip_comments(src, &all_tokens, item.span()));
            }
            ast::ItemKind::Import(import) => {
                let key = import.path().unwrap_or_default();
                imports.push((key, strip_comments(src, &all_tokens, item.span())));
            }
            ast::ItemKind::Block(block) if block.kind() == BlockKind::Project => {
                if project.is_some() {
                    return Err("more than one `Project` block".to_string());
                }
                project = Some(strip_comments(src, &all_tokens, item.span()));
            }
            ast::ItemKind::Block(block) if block.kind() == BlockKind::Document => {
                if document.is_some() {
                    return Err("more than one `Document` block".to_string());
                }
                document = Some(strip_comments(src, &all_tokens, item.span()));
            }
            ast::ItemKind::Block(block) if block.kind().is_declaration() => {
                let Some(order) = SORTED_KINDS.iter().position(|k| *k == block.kind()) else {
                    return Err(format!(
                        "internal error: {} has no canonical slot",
                        block.kind().as_str()
                    ));
                };
                let Some(name) = block.name().and_then(|path| path.as_ident()) else {
                    return Err(format!(
                        "a top-level {} has no plain name to sort by",
                        block.kind().as_str()
                    ));
                };
                declarations.push((
                    order,
                    name.text().to_owned(),
                    strip_comments(src, &all_tokens, item.span()),
                ));
            }
            _ => {
                return Err(format!(
                    "a top-level item at {:?} is neither spec, import, nor a declaration",
                    item.span()
                ));
            }
        }
    }

    imports.sort_by(|a, b| a.0.cmp(&b.0));
    // Stable: two declarations sharing a name keep their original relative
    // order instead of erroring — no content is lost either way.
    declarations.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut sections: Vec<String> = Vec::new();
    if let Some(spec) = spec {
        sections.push(spec);
    }
    if !imports.is_empty() {
        sections.push(
            imports
                .into_iter()
                .map(|(_, text)| text)
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    sections.extend(project);
    sections.extend(document);
    sections.extend(declarations.into_iter().map(|(_, _, text)| text));

    let mut raw = sections.join("\n\n");
    if !raw.is_empty() {
        raw.push('\n');
    }

    Ok(Reordered {
        text: crate::format(&raw),
        comments_removed,
    })
}

/// `span`'s source text, minus every comment token inside it — the only
/// token kind this pass ever removes from a block's own body.
fn strip_comments(src: &str, tokens: &[Token], span: Span) -> String {
    let mut out = String::new();
    for token in tokens {
        if token.span.start < span.start || token.span.end > span.end {
            continue;
        }
        if matches!(token.kind, TokenKind::LineComment | TokenKind::BlockComment) {
            continue;
        }
        out.push_str(token.text(src));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(src: &str) -> Reordered {
        reorder(src).expect("expected sorting to succeed")
    }

    #[test]
    fn reorders_into_canonical_order() {
        let src = "\
Datatype Money { Field { type = float; } }
Enum Status { draft; }
Table Zeta { id int; }
Partial base { id int; }
Document shop { Meta { note = \"\"; } }
Project { mode = draft; }
import \"./b.skema\";
spec 1;
";
        let out = sorted(src).text;
        let (file, diagnostics) = parse(&out);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");

        let kinds: Vec<_> = file
            .items()
            .filter_map(|item| match item.kind() {
                ast::ItemKind::Spec(_) => Some("spec".to_string()),
                ast::ItemKind::Import(_) => Some("import".to_string()),
                ast::ItemKind::Block(b) => Some(b.kind().as_str().to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                "spec", "import", "Project", "Document", "Partial", "Table", "Enum", "Datatype"
            ]
        );
    }

    #[test]
    fn alphabetizes_within_each_bucket() {
        let src = "\
Table Zeta { id int; }
Table Alpha { id int; }
Table Mid { id int; }
Partial zzz { id int; }
Partial aaa { id int; }
import \"./z.skema\";
import \"./a.skema\";
";
        let out = sorted(src).text;
        let (file, _) = parse(&out);
        let names: Vec<_> = file
            .items()
            .filter_map(|item| match item.kind() {
                ast::ItemKind::Import(im) => im.path(),
                ast::ItemKind::Block(b) => b
                    .name()
                    .and_then(|path| path.as_ident())
                    .map(|i| i.text().to_owned()),
                _ => None,
            })
            .collect();
        assert_eq!(
            names,
            vec![
                "./a.skema",
                "./z.skema",
                "aaa",
                "zzz",
                "Alpha",
                "Mid",
                "Zeta"
            ]
        );
    }

    #[test]
    fn field_order_inside_a_table_is_byte_for_byte_unchanged() {
        let src = "spec 1;\nPartial base {\n  id int [pk];\n}\n\nTable Zeta {\n  use base;\n\n  z_col int;\n  a_col int;\n}\n";
        let out = sorted(src).text;
        assert!(out.contains("use base;\n\n  z_col int;\n  a_col int;"));
    }

    #[test]
    fn drops_comments_and_reports_the_count() {
        let src = "# header\nspec 1;\n\nTable Foo {\n  # a field\n  id int; // trailing\n}\n\n/* block */\nTable Bar {\n  id int;\n}\n";
        let result = sorted(src);
        assert_eq!(result.comments_removed, 4);
        assert!(!result.text.contains('#'));
        assert!(!result.text.contains("//"));
        assert!(!result.text.contains("/*"));
    }

    #[test]
    fn zero_comments_reports_zero() {
        let src = "spec 1;\nTable Foo {\n  id int;\n}\n";
        assert_eq!(sorted(src).comments_removed, 0);
    }

    #[test]
    fn sorting_twice_is_idempotent() {
        let src = "Table Zeta { id int; }\nTable Alpha { id int; }\nspec 1;\n# gone\n";
        let once = sorted(src);
        let twice = sorted(&once.text);
        assert_eq!(once.text, twice.text);
        assert_eq!(twice.comments_removed, 0);
    }

    #[test]
    fn duplicate_names_in_the_same_bucket_keep_original_relative_order() {
        let src = "Table Dup { id int [pk]; }\nTable Dup { name varchar; }\n";
        let out = sorted(src).text;
        let first_pk = out.find("[pk]").expect("first Dup body");
        let second_name = out.find("name varchar").expect("second Dup body");
        assert!(first_pk < second_name, "original relative order kept");
    }

    #[test]
    fn refuses_a_file_with_syntax_errors() {
        assert!(reorder("Table {").is_err());
    }

    #[test]
    fn refuses_an_unnamed_declaration() {
        assert!(reorder("Table { id int; }").is_err());
    }

    #[test]
    fn refuses_more_than_one_spec() {
        assert!(reorder("spec 1;\nspec 1;\n").is_err());
    }

    #[test]
    fn refuses_more_than_one_project() {
        assert!(reorder("Project { mode = draft; }\nProject { mode = draft; }\n").is_err());
    }

    #[test]
    fn refuses_more_than_one_document() {
        assert!(reorder("Document a { }\nDocument b { }\n").is_err());
    }

    #[test]
    fn refuses_a_top_level_item_outside_the_known_set() {
        // `email varchar;` at the top level parses as a plain `Field`, not
        // a declaration — nowhere in the canonical order for it to go.
        assert!(reorder("email varchar;\n").is_err());
    }

    #[test]
    fn semantic_no_op_beyond_comments() {
        let example = include_str!("../../../examples/myexample.skema");
        let out = sorted(example).text;

        let (before, before_diag) = parse(example);
        let (after, after_diag) = parse(&out);
        assert_eq!(before_diag.len(), after_diag.len());

        let mut before_decls: Vec<_> = before
            .blocks()
            .map(|b| {
                (
                    b.kind(),
                    b.name()
                        .and_then(|path| path.as_ident())
                        .map(|i| i.text().to_owned()),
                )
            })
            .collect();
        let mut after_decls: Vec<_> = after
            .blocks()
            .map(|b| {
                (
                    b.kind(),
                    b.name()
                        .and_then(|path| path.as_ident())
                        .map(|i| i.text().to_owned()),
                )
            })
            .collect();
        before_decls.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
        after_decls.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
        assert_eq!(before_decls, after_decls);
    }
}
