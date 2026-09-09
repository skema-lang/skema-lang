//! `styleBlocks`: every `Style` block inside a `Project` or `Document`, read
//! off the raw AST for the same reason as [`crate::meta`] – `skema-hir`
//! never resolves `Style` into the model (`docs/13`: the property set isn't
//! finalized), so the syntax tree is the only place it survives.
//!
//! Deliberately **no catalog of known keys and no validation**: which
//! property names mean something, and how they render, is the caller's
//! decision, not the language's.

use serde::Serialize;
use skema_syntax::ast::{self, BlockKind};
#[cfg(feature = "typescript")]
use tsify_next::Tsify;

use crate::location::Utf16Offsets;
use crate::meta::{MetaEntry, entries};

/// The two cascade levels from `docs/03`: a `Project`-level block is the
/// project-wide default, a `Document`-level one is that file's override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(rename_all = "lowercase")]
pub enum StyleScope {
    Project,
    Document,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct StyleBlock {
    pub scope: StyleScope,
    /// `Style @table { … }` → `Some("table")`.
    pub selector: Option<String>,
    /// `Style highlight { … }` → `Some("highlight")`. Exactly one of
    /// `selector`/`class_name` is ever set.
    pub class_name: Option<String>,
    pub entries: Vec<MetaEntry>,
}

/// A named result type at the boundary, like [`crate::meta::TableMetas`] –
/// `tsify` implements the wasm ABI for a struct, not for a bare `Vec`.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[cfg_attr(feature = "typescript", tsify(into_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct StyleBlocks {
    pub blocks: Vec<StyleBlock>,
}

/// Single-file, like `tableMetas`: a caller with several files already loops
/// over them one at a time.
pub fn style_blocks(source: &str) -> StyleBlocks {
    let (file, _) = skema_syntax::parse(source);
    let offsets = Utf16Offsets::new(source);

    let mut blocks = Vec::new();
    for owner in file.blocks() {
        let scope = match owner.kind {
            BlockKind::Project => StyleScope::Project,
            BlockKind::Document => StyleScope::Document,
            _ => continue,
        };
        // `Block::child` only returns the *first* match, but a `Project` or
        // `Document` legally holds several `Style` blocks as siblings.
        for item in &owner.items {
            let ast::ItemKind::Block(block) = &item.kind else {
                continue;
            };
            if block.kind != BlockKind::Style {
                continue;
            }
            blocks.push(StyleBlock {
                scope,
                selector: block.selector.as_ref().map(|s| s.text.clone()),
                class_name: block
                    .name
                    .as_ref()
                    .and_then(ast::Path::as_ident)
                    .map(|i| i.text.clone()),
                entries: entries(block, &offsets),
            });
        }
    }

    StyleBlocks { blocks }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_form_is_read() {
        let src = r##"Project { Style @table { headerBackgroundColor = "#cc0000"; } }"##;
        let blocks = style_blocks(src).blocks;
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].scope, StyleScope::Project);
        assert_eq!(blocks[0].selector.as_deref(), Some("table"));
        assert!(blocks[0].class_name.is_none());
        assert_eq!(blocks[0].entries[0].key, "headerBackgroundColor");
        assert_eq!(blocks[0].entries[0].value.as_deref(), Some("#cc0000"));
    }

    #[test]
    fn class_form_is_read_and_scoped_to_document() {
        let src = r##"Document { Style highlight { borderColor = "#cc0000"; } }"##;
        let blocks = style_blocks(src).blocks;
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].scope, StyleScope::Document);
        assert!(blocks[0].selector.is_none());
        assert_eq!(blocks[0].class_name.as_deref(), Some("highlight"));
    }

    /// The case `Block::child` would silently get wrong: a `Project` (or
    /// `Document`) legally holds several `Style` blocks as siblings, and
    /// every one of them must come back, not just the first.
    #[test]
    fn multiple_style_blocks_in_one_project_are_all_returned() {
        let src = r##"
            Project {
              Style @project  { fontSize = "1.1em"; }
              Style @table    { headerBackgroundColor = "#1f4e79"; }
              Style @relation { lineColor = "#888888"; }
              Style core       { borderColor = "#1f4e79"; }
              Style historical { backgroundColor = "#f0f0f0"; }
            }
        "##;
        let blocks = style_blocks(src).blocks;
        assert_eq!(blocks.len(), 5);
        let selectors: Vec<_> = blocks
            .iter()
            .filter_map(|b| b.selector.as_deref())
            .collect();
        assert_eq!(selectors, ["project", "table", "relation"]);
        let classes: Vec<_> = blocks
            .iter()
            .filter_map(|b| b.class_name.as_deref())
            .collect();
        assert_eq!(classes, ["core", "historical"]);
    }

    #[test]
    fn a_file_without_project_or_document_returns_no_blocks() {
        let src = "Table Customer { id int [pk]; }";
        assert!(style_blocks(src).blocks.is_empty());
    }
}
