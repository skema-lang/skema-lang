//! `textDocument/documentLink`: every `import "…"` in a file at once, so an
//! editor can underline each without the caret ever landing on it - the
//! same per-import resolution [`definition::import_target`] does for one
//! offset ([`definition::imports`]).

use std::path::Path;

use skema_syntax::ast::AstNode;
use skema_syntax::{Span, TokenKind, tokenize};

use crate::analysis::Analysis;
use crate::convert;
use crate::definition;

pub fn document_links(analysis: &Analysis, path: &Path) -> Vec<lsp_types::DocumentLink> {
    let Some(file) = analysis.file(path) else {
        return Vec::new();
    };
    definition::imports(analysis, file)
        .filter_map(|(item, target)| {
            let target = target?;
            let path_span = path_literal_span(&file.src, item.span())?;
            Some(lsp_types::DocumentLink {
                range: convert::range_of(&file.src, path_span),
                target: convert::url_of(&target.path),
                tooltip: None,
                data: None,
            })
        })
        .collect()
}

/// The quoted path literal's own span within an `import` item - the AST
/// keeps only the whole item's span, so this re-lexes just that slice to
/// find the one string token in it (quotes included, as the lexer emits it).
fn path_literal_span(src: &str, item_span: Span) -> Option<Span> {
    let slice = src.get(item_span.start as usize..item_span.end as usize)?;
    let token = tokenize(slice)
        .into_iter()
        .find(|t| t.kind == TokenKind::Str)?;
    Some(Span {
        start: item_span.start + token.span.start,
        end: item_span.start + token.span.end,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        for (name, contents) in files {
            fs::write(dir.path().join(name), contents).expect("write fixture");
        }
        let root = dir.path().join(files[0].0);
        (dir, root)
    }

    fn build(root: &Path) -> Analysis {
        Analysis::build(root, false, |p| fs::read_to_string(p)).expect("analysis")
    }

    fn expected_range(src: &str, needle: &str) -> lsp_types::Range {
        let start = src.find(needle).expect("needle in source") as u32;
        let end = start + needle.len() as u32;
        convert::range_of(src, Span { start, end })
    }

    #[test]
    fn two_resolvable_imports_each_get_a_link() {
        let types = "spec 1;\nDocument types {}\n";
        let extra = "spec 1;\nDocument extra {}\n";
        let main = "spec 1;\nDocument shop {}\n\
                     import \"./types.skema\";\nimport \"./extra.skema\";\n";
        let (_dir, root) = workspace(&[
            ("main.skema", main),
            ("types.skema", types),
            ("extra.skema", extra),
        ]);
        let analysis = build(&root);

        let links = document_links(&analysis, &root);
        assert_eq!(links.len(), 2);

        assert_eq!(links[0].range, expected_range(main, "\"./types.skema\""));
        assert!(
            links[0]
                .target
                .as_ref()
                .unwrap()
                .path()
                .ends_with("types.skema")
        );

        assert_eq!(links[1].range, expected_range(main, "\"./extra.skema\""));
        assert!(
            links[1]
                .target
                .as_ref()
                .unwrap()
                .path()
                .ends_with("extra.skema")
        );
    }

    #[test]
    fn a_broken_import_gets_no_link_but_its_sibling_still_does() {
        let types = "spec 1;\nDocument types {}\n";
        let main = "spec 1;\nDocument shop {}\n\
                     import \"./missing.skema\";\nimport \"./types.skema\";\n";
        let (_dir, root) = workspace(&[("main.skema", main), ("types.skema", types)]);
        let analysis = build(&root);

        let links = document_links(&analysis, &root);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].range, expected_range(main, "\"./types.skema\""));
    }

    #[test]
    fn a_file_with_no_imports_has_no_links() {
        let main = "spec 1;\nDocument shop {}\nTable T { id int [pk]; }\n";
        let (_dir, root) = workspace(&[("main.skema", main)]);
        let analysis = build(&root);

        assert!(document_links(&analysis, &root).is_empty());
    }
}
