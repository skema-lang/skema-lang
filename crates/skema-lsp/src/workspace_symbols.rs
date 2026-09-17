//! `workspace/symbol`: every declaration across every analyzed root,
//! filtered by a case-insensitive substring match against the query.
//!
//! The same tree `document_symbols` already walks for one file
//! (`symbols::declarations`), just flattened across every project file of
//! every `Analysis` the server currently holds, instead of the one file the
//! editor happens to have open.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use lsp_types::{Location, SymbolInformation};

use crate::analysis::Analysis;
use crate::convert;
use crate::symbols::{self, Decl};

/// A declaration's location, keyed by what makes two results the same
/// entry: the same file, the same span, the same name. `Location` itself
/// carries no `Hash`, so the fields that matter are pulled out instead.
type DedupeKey = (String, u32, u32, u32, u32, String);

/// Every declaration across every analyzed root whose name contains `query`
/// (case-insensitive). A file reachable from two analyzed roots - a shared
/// `import` target - is walked once per root, so results are deduplicated
/// by location and name.
///
/// An empty query matches every name (`str::contains("")` is always true),
/// so it returns everything rather than nothing: VSCode's own "Go to Symbol
/// in Workspace" narrows the list further as the user types, and an empty
/// superset is nothing to narrow from.
pub fn workspace_symbols(
    analyses: &HashMap<PathBuf, Analysis>,
    query: &str,
) -> Vec<SymbolInformation> {
    let query = query.to_lowercase();
    let mut seen = HashSet::new();
    let mut symbols = Vec::new();

    for analysis in analyses.values() {
        for file in &analysis.project.files {
            for decl in symbols::declarations(&file.ast) {
                collect(
                    &decl,
                    &file.src,
                    &file.path,
                    &query,
                    &mut seen,
                    &mut symbols,
                );
            }
        }
    }
    symbols
}

#[allow(deprecated)] // `SymbolInformation::deprecated` is required by the struct.
fn collect(
    decl: &Decl,
    source: &str,
    path: &Path,
    query: &str,
    seen: &mut HashSet<DedupeKey>,
    out: &mut Vec<SymbolInformation>,
) {
    if let Some(uri) = convert::url_of(path).filter(|_| decl.name.to_lowercase().contains(query)) {
        let range = convert::range_of(source, decl.name_span);
        let key = (
            uri.to_string(),
            range.start.line,
            range.start.character,
            range.end.line,
            range.end.character,
            decl.name.clone(),
        );
        if seen.insert(key) {
            out.push(SymbolInformation {
                name: decl.name.clone(),
                kind: symbols::symbol_kind(decl.kind),
                tags: None,
                deprecated: None,
                location: Location { uri, range },
                container_name: None,
            });
        }
    }

    for child in &decl.children {
        collect(child, source, path, query, seen, out);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn write_files(dir: &Path, files: &[(&str, &str)]) {
        for (name, contents) in files {
            fs::write(dir.join(name), contents).expect("write fixture");
        }
    }

    fn build(root: &Path) -> Analysis {
        Analysis::build(root, false, |p| fs::read_to_string(p)).expect("analysis")
    }

    fn names(results: &[SymbolInformation]) -> Vec<&str> {
        results.iter().map(|s| s.name.as_str()).collect()
    }

    fn expected_range(src: &str, needle: &str) -> lsp_types::Range {
        let start = src.find(needle).expect("needle in source") as u32;
        let end = start + needle.len() as u32;
        convert::range_of(src, skema_syntax::Span { start, end })
    }

    #[test]
    fn a_partial_name_matches_across_two_files_of_one_root() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_files(
            dir.path(),
            &[
                (
                    "main.skema",
                    "spec 1;\nDocument shop {}\nimport \"./orders.skema\";\n\
                     Table Customer { id int [pk]; }\n",
                ),
                (
                    "orders.skema",
                    "spec 1;\nDocument orders {}\nTable Order { id int [pk]; }\n",
                ),
            ],
        );
        let root = dir.path().join("main.skema");
        let mut analyses = HashMap::new();
        analyses.insert(root.clone(), build(&root));

        assert_eq!(
            names(&workspace_symbols(&analyses, "tom")),
            vec!["Customer"]
        );
    }

    #[test]
    fn a_name_matches_across_two_separate_roots() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_files(
            dir.path(),
            &[
                (
                    "a.skema",
                    "spec 1;\nDocument a {}\nTable CustomerRecord { id int [pk]; }\n",
                ),
                (
                    "b.skema",
                    "spec 1;\nDocument b {}\nTable VendorRecord { id int [pk]; }\n",
                ),
            ],
        );
        let root_a = dir.path().join("a.skema");
        let root_b = dir.path().join("b.skema");
        let mut analyses = HashMap::new();
        analyses.insert(root_a.clone(), build(&root_a));
        analyses.insert(root_b.clone(), build(&root_b));

        let results = workspace_symbols(&analyses, "Record");
        let mut found = names(&results);
        found.sort_unstable();
        assert_eq!(found, vec!["CustomerRecord", "VendorRecord"]);
    }

    #[test]
    fn the_match_is_case_insensitive() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_files(
            dir.path(),
            &[(
                "main.skema",
                "spec 1;\nDocument shop {}\nTable Customer { id int [pk]; }\n",
            )],
        );
        let root = dir.path().join("main.skema");
        let mut analyses = HashMap::new();
        analyses.insert(root.clone(), build(&root));

        assert_eq!(
            names(&workspace_symbols(&analyses, "CUSTOMER")),
            vec!["Customer"]
        );
    }

    #[test]
    fn a_table_and_a_nested_field_report_their_own_kind_and_location() {
        let source = "spec 1;\nDocument shop {}\nTable Customer { id int [pk]; }\n";
        let dir = tempfile::tempdir().expect("temp dir");
        write_files(dir.path(), &[("main.skema", source)]);
        let root = dir.path().join("main.skema");
        let mut analyses = HashMap::new();
        analyses.insert(root.clone(), build(&root));

        let results = workspace_symbols(&analyses, "");

        let table = results
            .iter()
            .find(|s| s.name == "Customer")
            .expect("the table");
        assert_eq!(table.kind, lsp_types::SymbolKind::CLASS);
        assert!(table.location.uri.path().ends_with("main.skema"));
        assert_eq!(table.location.range, expected_range(source, "Customer"));

        let field = results.iter().find(|s| s.name == "id").expect("the field");
        assert_eq!(field.kind, lsp_types::SymbolKind::FIELD);
        assert_eq!(field.location.range, expected_range(source, "id"));
    }

    #[test]
    fn no_analyses_returns_an_empty_list() {
        let analyses = HashMap::new();
        assert!(workspace_symbols(&analyses, "anything").is_empty());
    }

    #[test]
    fn an_empty_query_returns_everything() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_files(
            dir.path(),
            &[(
                "main.skema",
                "spec 1;\nDocument shop {}\nTable Customer { id int [pk]; email varchar; }\n",
            )],
        );
        let root = dir.path().join("main.skema");
        let mut analyses = HashMap::new();
        analyses.insert(root.clone(), build(&root));

        let results = workspace_symbols(&analyses, "");
        let mut found = names(&results);
        found.sort_unstable();
        // Document, Table, and its two fields - every declaration the file makes.
        assert_eq!(found, vec!["Customer", "email", "id", "shop"]);
    }

    #[test]
    fn a_file_shared_by_two_roots_is_reported_once() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_files(
            dir.path(),
            &[
                (
                    "shared.skema",
                    "spec 1;\nDocument common {}\nTable Shared { id int [pk]; }\n",
                ),
                (
                    "a.skema",
                    "spec 1;\nDocument docA {}\nimport \"./shared.skema\";\n",
                ),
                (
                    "b.skema",
                    "spec 1;\nDocument docB {}\nimport \"./shared.skema\";\n",
                ),
            ],
        );
        let root_a = dir.path().join("a.skema");
        let root_b = dir.path().join("b.skema");
        let mut analyses = HashMap::new();
        analyses.insert(root_a.clone(), build(&root_a));
        analyses.insert(root_b.clone(), build(&root_b));

        assert_eq!(
            names(&workspace_symbols(&analyses, "Shared")),
            vec!["Shared"]
        );
    }
}
