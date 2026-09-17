//! Tests `skema-hir` against the example projects in `examples/`.
//!
//! File loading is local test code: `skema-hir` stays platform-neutral (see
//! `src/lib.rs`), and `skema-loader` does this for real. Rebuilt here in
//! miniature to exercise the whole pipeline — loading, parsing, name
//! resolution, rules, model — against a real multi-file project.
//!
//! `unwrap`/`expect`/`panic` are fine here. Helpers like `load` are only
//! called from `#[test]` functions, but clippy's `*_used` lints recognize
//! that for the test function alone, not for its helpers (see
//! `clippy.toml`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use skema_hir::{Diagnostic, FileId, InputFile, Severity, analyze};
use skema_syntax::ast::SourceFile;

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = crates/skema-hir
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_owned()
}

/// Reads the root file and follows every `import` recursively – a rebuild
/// of the loading logic that will later live in `skema-cli`.
fn load(entry: &Path) -> Vec<(PathBuf, SourceFile)> {
    let mut loaded: Vec<(PathBuf, SourceFile)> = Vec::new();
    let mut queue = vec![entry.canonicalize().expect("root file exists")];

    while let Some(path) = queue.pop() {
        if loaded.iter().any(|(p, _)| p == &path) {
            continue;
        }
        let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        let (ast, diags) = skema_syntax::parse(&src);
        assert!(
            diags.is_empty(),
            "parser diagnostics in {path:?}: {diags:#?}"
        );
        for import in ast.imports() {
            let Some(rel) = import.path() else { continue };
            let target = path
                .parent()
                .expect("file has a parent directory")
                .join(&rel)
                .canonicalize()
                .unwrap_or_else(|e| panic!("import {rel:?} from {path:?}: {e}"));
            queue.push(target);
        }
        loaded.push((path, ast));
    }

    loaded
}

/// Builds the `InputFile` list that `skema_hir::analyze` expects from the
/// loaded files.
fn analyze_project(entry_relative: &str) -> (skema_hir::model::Model, Vec<Diagnostic>) {
    let entry = workspace_root().join(entry_relative);
    let loaded = load(&entry);

    let ids: HashMap<PathBuf, FileId> = loaded
        .iter()
        .enumerate()
        .map(|(i, (path, _))| (path.clone(), FileId(i as u32)))
        .collect();

    let files: Vec<InputFile<'_>> = loaded
        .iter()
        .enumerate()
        .map(|(i, (path, ast))| {
            let import_targets = ast
                .imports()
                .map(|import| {
                    let rel = import.path()?;
                    let target = path.parent()?.join(rel).canonicalize().ok()?;
                    ids.get(&target).copied()
                })
                .collect();
            InputFile {
                id: FileId(i as u32),
                display_path: path.display().to_string(),
                ast,
                import_targets,
            }
        })
        .collect();

    let (model, _references, diags) = analyze(&files);
    (model, diags)
}

fn errors(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect()
}

#[test]
fn generic_example_project_analyzes_without_errors() {
    let (model, diags) = analyze_project("examples/generic/project.skema");
    let errs = errors(&diags);
    assert!(errs.is_empty(), "unexpected errors:\n{errs:#?}");

    // `resolve::run` sorts by (namespace, name) – a deterministic order
    // for the JSON export.
    let names: Vec<_> = model
        .tables
        .iter()
        .map(|t| (t.namespace.as_str(), t.name.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("catalog", "Author"),
            ("catalog", "Book"),
            ("catalog", "Copy"),
            ("lending", "Loan"),
            ("lending", "LoanArchive"),
            ("lending", "Member"),
        ]
    );
}

#[test]
fn loan_gets_its_fields_from_two_partials_and_both_relations() {
    let (model, diags) = analyze_project("examples/generic/project.skema");
    assert!(errors(&diags).is_empty(), "{:#?}", errors(&diags));

    let loan = model
        .tables
        .iter()
        .find(|t| t.name == "Loan")
        .expect("Table Loan");

    let field_names: Vec<_> = loan.fields.iter().map(|f| f.name.as_str()).collect();
    // `use types.Base` brings in id/public_id/created_at/updated_at, then
    // Loan's own fields (docs/09: order matters).
    assert_eq!(
        field_names,
        [
            "id",
            "public_id",
            "created_at",
            "updated_at",
            "member",
            "copy",
            "lent_on",
            "due_on",
            "returned_at",
        ]
    );

    let member = loan.fields.iter().find(|f| f.name == "member").unwrap();
    let relation = member.relation.as_ref().expect("member is a relation");
    assert_eq!(relation.target_table, "Member");
    assert_eq!(relation.target_field, "id");
    assert_eq!(relation.on_delete.as_deref(), Some("restrict"));

    // `copy >cat.Copy.id` – the target sits in another file, reached via
    // the namespace alias `cat` (docs/02, "Referencing within vs. across
    // files").
    let copy = loan.fields.iter().find(|f| f.name == "copy").unwrap();
    let relation = copy.relation.as_ref().expect("copy is a relation");
    assert_eq!(relation.target_table, "Copy");
    assert_eq!(relation.target_field, "id");
}

#[test]
fn catalog_group_contains_all_four_members() {
    let (model, diags) = analyze_project("examples/generic/project.skema");
    assert!(errors(&diags).is_empty(), "{:#?}", errors(&diags));

    let group = model
        .groups
        .iter()
        .find(|g| g.name == "Catalog")
        .expect("Group Catalog");
    let members: Vec<_> = group
        .members
        .iter()
        .map(|m| (m.kind.as_str(), m.name.as_str()))
        .collect();
    assert_eq!(
        members,
        [
            ("Table", "Author"),
            ("Table", "Book"),
            ("Table", "Copy"),
            ("Enum", "CopyState"),
        ]
    );
}

#[test]
fn physical_names_cascade_project_document_table() {
    let (model, diags) = analyze_project("examples/generic/project.skema");
    assert!(errors(&diags).is_empty(), "{:#?}", errors(&diags));

    // Project.Database.database = "library" applies to all tables.
    // Document(catalog).Database.schema = "catalog" overrides the value
    // from Project for every table in catalog.skema – except Copy, which
    // sets its own `table = "book_copy"` (docs/11).
    let book = model.tables.iter().find(|t| t.name == "Book").unwrap();
    assert_eq!(book.physical.database.as_deref(), Some("library"));
    assert_eq!(book.physical.schema.as_deref(), Some("catalog"));
    assert_eq!(book.physical.table, None);

    let copy = model.tables.iter().find(|t| t.name == "Copy").unwrap();
    assert_eq!(copy.physical.schema.as_deref(), Some("catalog"));
    assert_eq!(copy.physical.table.as_deref(), Some("book_copy"));

    let member = model.tables.iter().find(|t| t.name == "Member").unwrap();
    assert_eq!(member.physical.schema.as_deref(), Some("lending"));
}

#[test]
fn myexample_runs_standalone_and_analyzes_without_errors() {
    let (model, diags) = analyze_project("examples/myexample.skema");
    let errs = errors(&diags);
    assert!(errs.is_empty(), "unexpected errors:\n{errs:#?}");

    let order_status = model
        .enums
        .iter()
        .find(|e| e.name == "OrderStatus")
        .expect("Enum OrderStatus");
    assert_eq!(order_status.stored_type, "string");
    let entries: Vec<_> = order_status
        .entries
        .iter()
        .map(|e| (e.name.as_str(), e.label.as_deref(), e.value.as_deref()))
        .collect();
    assert_eq!(
        entries,
        [
            ("draft", Some("Draft"), None),
            ("paid", Some("Paid"), None),
            ("shipped", Some("Shipped"), Some("S")),
            ("cancelled", None, None),
        ]
    );

    let order = model.tables.iter().find(|t| t.name == "Order").unwrap();
    let customer = order.fields.iter().find(|f| f.name == "customer").unwrap();
    assert!(customer.flags.contains_key("required"));
    let relation = customer.relation.as_ref().unwrap();
    assert_eq!(relation.on_delete.as_deref(), Some("cascade"));
}
