//! `textDocument/references`: every place that names a declaration.
//!
//! The mirror of [`definition`](crate::definition), which reads a reference
//! and answers "what does this point to". Finding references answers the
//! opposite question, "what points here" - and needs no new data:
//! `skema_hir::model::ReferenceLink` already pairs each reference's span
//! with its target's (brain task 09), so this collects every link whose
//! target matches instead of resolving one link's target.
//!
//! # Two ways to be on a target
//!
//! The caret can land on either side of a link:
//! - on a reference itself (a field type, a relation half, a `use` target,
//!   a `Group` member) - the target is that link's own `(to_file, to_span)`.
//! - on the declaration's own name, where there is no link to read a target
//!   *from* - [`symbols::find_at`] walks the file's declaration tree instead.
//!
//! Either way the answer is a `(file, span)` identity, and every link whose
//! target equals it names one use.

use std::path::Path;

use crate::analysis::{Analysis, display_path};
use crate::convert;
use crate::definition::{contains_inclusive, to_syntax_span};
use crate::symbols;

/// Every use of the declaration under `(path, offset)`, or `None` when the
/// offset names neither a reference nor a declaration (a builtin keyword,
/// whitespace).
pub fn references(
    analysis: &Analysis,
    path: &Path,
    offset: u32,
    include_declaration: bool,
) -> Option<Vec<lsp_types::Location>> {
    let file = analysis.file(path)?;
    let from = display_path(file);

    let (target_file, target_span) = target_at(analysis, file, from, offset)?;

    let mut locations: Vec<lsp_types::Location> = analysis
        .references
        .iter()
        .filter(|link| link.to_file == target_file && same_span(link.to_span, target_span))
        .filter_map(|link| location_of(analysis, &link.from_file, link.from_span))
        .collect();

    if include_declaration {
        locations.extend(location_of(analysis, &target_file, target_span));
    }

    Some(locations)
}

/// The declaration `offset` is asking about, as a `(file, span)` identity -
/// what every reference to it must match.
fn target_at(
    analysis: &Analysis,
    file: &skema_loader::LoadedFile,
    from: &str,
    offset: u32,
) -> Option<(String, skema_api::Span)> {
    if let Some(link) = analysis
        .references
        .iter()
        .find(|r| r.from_file == from && contains_inclusive(r.from_span, offset))
    {
        return Some((link.to_file.clone(), link.to_span));
    }

    let decls = symbols::declarations(&file.ast);
    let decl = symbols::find_at(&decls, offset)?;
    Some((from.to_owned(), decl.name_span.into()))
}

fn same_span(a: skema_api::Span, b: skema_api::Span) -> bool {
    a.start == b.start && a.end == b.end
}

fn location_of(
    analysis: &Analysis,
    file_path: &str,
    span: skema_api::Span,
) -> Option<lsp_types::Location> {
    let file = analysis
        .project
        .files
        .iter()
        .find(|f| display_path(f) == file_path)?;
    Some(lsp_types::Location {
        uri: convert::url_of(&file.path)?,
        range: convert::range_of(&file.src, to_syntax_span(span)),
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

    /// Strips one `|` caret out of `source_with_caret`, returning the plain
    /// source and the caret's byte offset in it.
    fn parse_caret(source_with_caret: &str) -> (String, u32) {
        let offset = source_with_caret.find('|').expect("a caret");
        (source_with_caret.replace('|', ""), offset as u32)
    }

    /// The `lsp_types::Range` of the `n`th (0-based) occurrence of `needle`.
    fn nth_range(src: &str, needle: &str, n: usize) -> lsp_types::Range {
        let mut search_from = 0;
        for _ in 0..n {
            let found = src[search_from..].find(needle).expect("needle in source");
            search_from += found + needle.len();
        }
        let start = search_from + src[search_from..].find(needle).expect("needle in source");
        convert::range_of(
            src,
            skema_syntax::Span {
                start: start as u32,
                end: (start + needle.len()) as u32,
            },
        )
    }

    /// `references`' locations, as ranges sorted by position - the order
    /// links are resolved in is an implementation detail, not part of the
    /// contract.
    fn ranges_of(locations: &[lsp_types::Location]) -> Vec<lsp_types::Range> {
        let mut ranges: Vec<lsp_types::Range> = locations.iter().map(|l| l.range).collect();
        ranges.sort_by_key(|r| (r.start.line, r.start.character));
        ranges
    }

    #[test]
    fn a_field_type_reference_lists_every_use() {
        let (source, offset) = parse_caret(
            "spec 1;\nDocument shop {}\nDatatype Money { Field { type = int; } }\n\
             Table Order { total Mon|ey; }\nTable Invoice { amount Money; }\n",
        );
        let (_dir, root) = workspace(&[("main.skema", &source)]);
        let analysis = build(&root);

        let locations = references(&analysis, &root, offset, false).expect("references");
        assert_eq!(
            ranges_of(&locations),
            vec![
                nth_range(&source, "Money", 1),
                nth_range(&source, "Money", 2)
            ]
        );

        let with_decl = references(&analysis, &root, offset, true).expect("references");
        assert_eq!(
            ranges_of(&with_decl),
            vec![
                nth_range(&source, "Money", 0),
                nth_range(&source, "Money", 1),
                nth_range(&source, "Money", 2),
            ]
        );
    }

    #[test]
    fn a_relation_table_half_lists_all_references_to_the_table() {
        let (source, offset) = parse_caret(
            "spec 1;\nDocument shop {}\nTable Customer { id int [pk]; }\n\
             Table Order { customer >Cust|omer.id; }\nGroup Core { Table Customer; }\n",
        );
        let (_dir, root) = workspace(&[("main.skema", &source)]);
        let analysis = build(&root);

        let locations = references(&analysis, &root, offset, false).expect("references");
        assert_eq!(
            ranges_of(&locations),
            vec![
                nth_range(&source, "Customer", 1),
                nth_range(&source, "Customer", 2),
            ]
        );

        let with_decl = references(&analysis, &root, offset, true).expect("references");
        assert_eq!(
            ranges_of(&with_decl),
            vec![
                nth_range(&source, "Customer", 0),
                nth_range(&source, "Customer", 1),
                nth_range(&source, "Customer", 2),
            ]
        );
    }

    #[test]
    fn a_relation_column_half_lists_references_to_the_column() {
        let (source, offset) = parse_caret(
            "spec 1;\nDocument shop {}\nTable Customer { id int [pk]; }\n\
             Table Order { customer >Customer.i|d; }\n",
        );
        let (_dir, root) = workspace(&[("main.skema", &source)]);
        let analysis = build(&root);

        let locations = references(&analysis, &root, offset, false).expect("references");
        assert_eq!(ranges_of(&locations), vec![nth_range(&source, "id", 1)]);

        let with_decl = references(&analysis, &root, offset, true).expect("references");
        assert_eq!(
            ranges_of(&with_decl),
            vec![nth_range(&source, "id", 0), nth_range(&source, "id", 1)]
        );
    }

    #[test]
    fn a_use_target_lists_every_partial_use() {
        let (source, offset) = parse_caret(
            "spec 1;\nDocument shop {}\nPartial Stamped { createdAt string; }\n\
             Table Order { use Stam|ped; }\nTable Invoice { use Stamped; }\n",
        );
        let (_dir, root) = workspace(&[("main.skema", &source)]);
        let analysis = build(&root);

        let locations = references(&analysis, &root, offset, false).expect("references");
        assert_eq!(
            ranges_of(&locations),
            vec![
                nth_range(&source, "Stamped", 1),
                nth_range(&source, "Stamped", 2),
            ]
        );

        let with_decl = references(&analysis, &root, offset, true).expect("references");
        assert_eq!(
            ranges_of(&with_decl),
            vec![
                nth_range(&source, "Stamped", 0),
                nth_range(&source, "Stamped", 1),
                nth_range(&source, "Stamped", 2),
            ]
        );
    }

    #[test]
    fn a_group_member_reference_lists_all_uses_of_the_table() {
        let (source, offset) = parse_caret(
            "spec 1;\nDocument shop {}\nTable Customer { id int [pk]; }\n\
             Table Order { customer >Customer.id; }\nGroup Core { Table Cust|omer; }\n",
        );
        let (_dir, root) = workspace(&[("main.skema", &source)]);
        let analysis = build(&root);

        let locations = references(&analysis, &root, offset, false).expect("references");
        assert_eq!(
            ranges_of(&locations),
            vec![
                nth_range(&source, "Customer", 1),
                nth_range(&source, "Customer", 2),
            ]
        );
    }

    #[test]
    fn a_declaration_name_lists_itself_plus_all_references() {
        let (source, offset) = parse_caret(
            "spec 1;\nDocument shop {}\nTable Cust|omer { id int [pk]; }\n\
             Table Order { customer >Customer.id; }\n",
        );
        let (_dir, root) = workspace(&[("main.skema", &source)]);
        let analysis = build(&root);

        let locations = references(&analysis, &root, offset, false).expect("references");
        assert_eq!(
            ranges_of(&locations),
            vec![nth_range(&source, "Customer", 1)]
        );

        let with_decl = references(&analysis, &root, offset, true).expect("references");
        assert_eq!(
            ranges_of(&with_decl),
            vec![
                nth_range(&source, "Customer", 0),
                nth_range(&source, "Customer", 1),
            ]
        );
    }

    #[test]
    fn no_references_for_a_builtin_type() {
        let (source, offset) =
            parse_caret("spec 1;\nDocument shop {}\nTable T { id in|t [pk]; }\n");
        let (_dir, root) = workspace(&[("main.skema", &source)]);
        let analysis = build(&root);

        assert!(references(&analysis, &root, offset, false).is_none());
    }
}
