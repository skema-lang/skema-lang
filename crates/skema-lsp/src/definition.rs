//! `textDocument/definition`: from a reference to where it is declared.
//!
//! The browser editor does this by double-clicking a table in the diagram;
//! in an editor it is the name under the caret, which can be several
//! different things - a type, a relation target, a `use`d partial, a
//! `Group` member, an `import` path.
//!
//! # Where the answer comes from
//!
//! Name resolution proper is `skema-hir`'s (`rules/resolve.rs`). It already
//! computes, for every reference it resolves, both that reference's own
//! span and its target's - and keeps the pairing instead of discarding it
//! (`skema_hir::model::ReferenceLink`, brain task 09). This module is
//! therefore a lookup against [`Analysis::references`], not a second
//! resolution pass: given an offset, find the one link whose `from_span`
//! contains it.
//!
//! `import "./types.skema";` is the one case that isn't a `ReferenceLink` -
//! it is resolved already at load time by `skema-loader`, so
//! [`import_target`] answers it directly from `import_targets`.

use std::path::Path;

use skema_syntax::ast::{AstNode, Item, ItemKind};

use crate::analysis::{Analysis, display_path};
use crate::convert;

pub fn definition(analysis: &Analysis, path: &Path, offset: u32) -> Option<lsp_types::Location> {
    let file = analysis.file(path)?;

    if let Some(target) = import_target(analysis, file, offset) {
        return Some(target);
    }

    let from = display_path(file);
    let link = analysis
        .references
        .iter()
        .find(|r| r.from_file == from && contains_inclusive(r.from_span, offset))?;

    let target = analysis
        .project
        .files
        .iter()
        .find(|f| display_path(f) == link.to_file)?;
    Some(lsp_types::Location {
        uri: convert::url_of(&target.path)?,
        range: convert::range_of(&target.src, to_syntax_span(link.to_span)),
    })
}

/// Every `import` item in `from`, paired with its resolved target file -
/// `None` for one that didn't resolve. The one walk of
/// `from.ast.items`/`from.import_targets` that [`import_target`] and
/// `links::document_links` both need (brain task 77).
pub(crate) fn imports<'a>(
    analysis: &'a Analysis,
    from: &'a skema_loader::LoadedFile,
) -> impl Iterator<Item = (Item, Option<&'a skema_loader::LoadedFile>)> {
    let mut resolved = from.import_targets.iter();
    from.ast.items().filter_map(move |item| {
        if !matches!(item.kind(), ItemKind::Import(_)) {
            return None;
        }
        let id = (*resolved.next()?)?;
        let target = analysis.project.files.get(id.0 as usize);
        Some((item, target))
    })
}

/// When `offset` sits on an `import`'s path string, the file it names.
fn import_target(
    analysis: &Analysis,
    from: &skema_loader::LoadedFile,
    offset: u32,
) -> Option<lsp_types::Location> {
    let (_, target) = imports(analysis, from).find(|(item, _)| {
        let span = item.span();
        span.start <= offset && offset < span.end
    })?;
    Some(lsp_types::Location {
        uri: convert::url_of(&target?.path)?,
        // The top of the file: an import names a file, not a place in it.
        range: lsp_types::Range::default(),
    })
}

fn to_syntax_span(span: skema_api::Span) -> skema_syntax::Span {
    skema_syntax::Span {
        start: span.start,
        end: span.end,
    }
}

/// A caret sitting just past the last character of a name is still on it -
/// that is where it lands after double-clicking a word.
const fn contains_inclusive(span: skema_api::Span, offset: u32) -> bool {
    span.start <= offset && offset <= span.end
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

    /// Strips one `|` caret out of `source_with_caret`, returning the plain
    /// source and the caret's byte offset in it.
    fn parse_caret(source_with_caret: &str) -> (String, u32) {
        let offset = source_with_caret.find('|').expect("a caret");
        (source_with_caret.replace('|', ""), offset as u32)
    }

    /// The `lsp_types::Range` `definition` should report for `needle` - the
    /// same conversion `convert::range_of` does, so a test only has to name
    /// the expected text, not reason about UTF-16 columns.
    fn expected_range(src: &str, needle: &str) -> lsp_types::Range {
        let start = src.find(needle).expect("needle in source") as u32;
        let end = start + needle.len() as u32;
        convert::range_of(src, skema_syntax::Span { start, end })
    }

    #[test]
    fn a_field_type_jumps_to_the_datatype() {
        let (source, offset) = parse_caret(
            "spec 1;\nDocument shop {}\nDatatype Money { Field { type = int; } }\n\
             Table Order { total Mon|ey; }\n",
        );
        let (_dir, root) = workspace(&[("main.skema", &source)]);
        let analysis = build(&root);

        let location = definition(&analysis, &root, offset).expect("a definition");
        assert!(location.uri.path().ends_with("main.skema"));
        assert_eq!(location.range, expected_range(&source, "Money"));
    }

    #[test]
    fn a_relation_jumps_the_table_half_to_the_table() {
        let (source, offset) = parse_caret(
            "spec 1;\nDocument shop {}\nTable Customer { id int [pk]; }\n\
             Table Order { customer >Cust|omer.id; }\n",
        );
        let (_dir, root) = workspace(&[("main.skema", &source)]);
        let analysis = build(&root);

        let location = definition(&analysis, &root, offset).expect("a definition");
        assert_eq!(location.range, expected_range(&source, "Customer"));
    }

    #[test]
    fn a_relation_jumps_the_field_half_to_the_column() {
        let (source, offset) = parse_caret(
            "spec 1;\nDocument shop {}\nTable Customer { id int [pk]; }\n\
             Table Order { customer >Customer.i|d; }\n",
        );
        let (_dir, root) = workspace(&[("main.skema", &source)]);
        let analysis = build(&root);

        let location = definition(&analysis, &root, offset).expect("a definition");
        // The `id` inside `Customer`, not the `id` written at the relation
        // site - `expected_range` would find the first occurrence either
        // way, so this pins the range down explicitly instead.
        let inside_customer = source.find("id int").expect("Customer's own field") as u32;
        assert_eq!(
            location.range,
            convert::range_of(
                &source,
                skema_syntax::Span {
                    start: inside_customer,
                    end: inside_customer + 2,
                }
            )
        );
    }

    #[test]
    fn a_use_target_jumps_to_the_partial() {
        let (source, offset) = parse_caret(
            "spec 1;\nDocument shop {}\nPartial Stamped { createdAt string; }\n\
             Table Order { use Sta|mped; }\n",
        );
        let (_dir, root) = workspace(&[("main.skema", &source)]);
        let analysis = build(&root);

        let location = definition(&analysis, &root, offset).expect("a definition");
        assert_eq!(location.range, expected_range(&source, "Stamped"));
    }

    #[test]
    fn a_group_member_jumps_to_the_declaration() {
        let (source, offset) = parse_caret(
            "spec 1;\nDocument shop {}\nTable Customer { id int [pk]; }\n\
             Group Core { Table Cust|omer; }\n",
        );
        let (_dir, root) = workspace(&[("main.skema", &source)]);
        let analysis = build(&root);

        let location = definition(&analysis, &root, offset).expect("a definition");
        assert_eq!(location.range, expected_range(&source, "Customer"));
    }

    #[test]
    fn an_import_jumps_to_the_top_of_the_target_file() {
        let (main, offset) = parse_caret("spec 1;\nDocument shop {}\nimport \"./ty|pes.skema\";\n");
        let types = "spec 1;\nDocument types {}\n";
        let (_dir, root) = workspace(&[("main.skema", &main), ("types.skema", types)]);
        let analysis = build(&root);

        let location = definition(&analysis, &root, offset).expect("a definition");
        assert!(location.uri.path().ends_with("types.skema"));
        assert_eq!(location.range, lsp_types::Range::default());
    }

    #[test]
    fn no_definition_for_a_builtin_type() {
        let (source, offset) =
            parse_caret("spec 1;\nDocument shop {}\nTable T { id in|t [pk]; }\n");
        let (_dir, root) = workspace(&[("main.skema", &source)]);
        let analysis = build(&root);

        assert!(definition(&analysis, &root, offset).is_none());
    }
}
