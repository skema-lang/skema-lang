//! A minimal smoke test for the wasm-bindgen boundary itself – not a
//! reimplementation of `skema-hir`'s own test suite. Run via
//! `wasm-pack test --node` (no DOM/browser APIs are touched).
//!
//! `clippy.toml`'s `allow-expect-in-tests` recognizes plain `#[test]`, not
//! `#[wasm_bindgen_test]` – so `expect()` is allowed explicitly here for the
//! same reason it's allowed everywhere else under `tests/`: a panic here
//! *is* the failure message.
#![allow(clippy::expect_used)]

use skema_wasm::{ProjectFile, ProjectInput};
use wasm_bindgen_test::*;

const EXAMPLE: &str = include_str!("../../../examples/myexample.skema");

/// The reference project from `examples/generic/` – the same corpus
/// `skema-hir`'s own tests load from disk, here as a virtual file set, so
/// the browser path is exercised against real multi-file input rather than
/// a toy fixture.
fn reference_project() -> Vec<ProjectFile> {
    [
        (
            "/project.skema",
            include_str!("../../../examples/generic/project.skema"),
        ),
        (
            "/types.skema",
            include_str!("../../../examples/generic/types.skema"),
        ),
        (
            "/catalog.skema",
            include_str!("../../../examples/generic/catalog.skema"),
        ),
        (
            "/lending.skema",
            include_str!("../../../examples/generic/lending.skema"),
        ),
    ]
    .into_iter()
    .map(|(path, source)| ProjectFile {
        path: path.to_owned(),
        source: source.to_owned(),
    })
    .collect()
}

#[wasm_bindgen_test]
fn export_reports_the_expected_declarations_and_locations() {
    let result = skema_wasm::export_model_js(EXAMPLE, None, true);
    assert!(result.ok, "diagnostics: {:?}", result.diagnostics);

    let table_names: Vec<&str> = result
        .model
        .tables
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    assert!(table_names.contains(&"Customer"));
    assert!(table_names.contains(&"Order"));

    let enum_names: Vec<&str> = result.model.enums.iter().map(|e| e.name.as_str()).collect();
    assert!(enum_names.contains(&"OrderStatus"));

    // `Table Customer {` starts at the very beginning of its line.
    let customer = result
        .model
        .tables
        .iter()
        .find(|t| t.name == "Customer")
        .expect("Customer table");
    let location = customer.location.as_ref().expect("withLocations was true");
    assert_eq!(location.file, "input.skema");
    assert_eq!(location.line, 73);
    assert_eq!(location.column, 1);
}

#[wasm_bindgen_test]
fn export_without_locations_omits_them() {
    let result = skema_wasm::export_model_js(EXAMPLE, None, false);
    assert!(result.ok, "diagnostics: {:?}", result.diagnostics);

    let customer = result
        .model
        .tables
        .iter()
        .find(|t| t.name == "Customer")
        .expect("Customer table");
    assert!(customer.location.is_none());
    // The raw byte span is always present, regardless of the flag.
    assert!(customer.span.end > customer.span.start);
}

#[wasm_bindgen_test]
fn check_reports_no_errors_for_the_example() {
    let result = skema_wasm::check(EXAMPLE, None);
    assert!(result.ok, "diagnostics: {:?}", result.diagnostics);
}

#[wasm_bindgen_test]
fn a_broken_file_still_returns_a_result_with_diagnostics() {
    let result = skema_wasm::check("Table {", None);
    assert!(!result.ok);
    assert!(!result.diagnostics.is_empty());
}

#[wasm_bindgen_test]
fn the_reference_project_analyzes_across_its_import_graph() {
    let result = skema_wasm::analyze_project(ProjectInput {
        files: reference_project(),
        root_path: "/project.skema".to_owned(),
        with_locations: true,
    });

    assert!(result.root_found);
    assert!(result.ok, "diagnostics: {:?}", result.diagnostics);
    assert!(
        result.files.iter().all(|f| f.in_project),
        "every reference file is reachable from the root: {:?}",
        result.files
    );

    // `Loan` lives in lending.skema and pulls fields from partials, while
    // its relation targets live in other files – i.e. this only resolves if
    // the import graph really was followed.
    let loan = result
        .model
        .tables
        .iter()
        .find(|t| t.name == "Loan")
        .expect("Loan table");
    assert_eq!(loan.file, "/lending.skema");
    assert!(
        loan.fields.iter().any(|f| f.relation.is_some()),
        "Loan should carry resolved relations"
    );
}

#[wasm_bindgen_test]
fn a_backreference_and_its_owning_side_are_two_model_fields() {
    // Documents the behavior a diagram has to account for (`docs/07`,
    // "tooling recognizes them by the column pair and draws one edge, not
    // two"): the `<` side is a real `Field` with a real `Relation` in the
    // model, even though it creates no column – so a renderer must filter
    // it out of both its edge list and its column list, rather than
    // expecting the model to have dropped it.
    let source = r#"spec 1;
        Document shop {}
        Project { mode = draft; dialect = generic; }
        Table Customer {
          id     int [pk];
          orders <Order.customer;
        }
        Table Order {
          id       int [pk];
          customer >Customer.id [onDelete=cascade];
        }"#;
    let result = skema_wasm::export_model_js(source, None, false);
    assert!(result.ok, "diagnostics: {:?}", result.diagnostics);

    let kinds: Vec<_> = result
        .model
        .tables
        .iter()
        .flat_map(|t| t.fields.iter())
        .filter_map(|f| f.relation.as_ref().map(|r| r.kind))
        .collect();
    assert_eq!(kinds.len(), 2, "both sides are present: {kinds:?}");
    assert!(kinds.contains(&skema_wasm::RelationKind::OneToMany));
    assert!(kinds.contains(&skema_wasm::RelationKind::ManyToOne));
}

#[wasm_bindgen_test]
fn a_relation_target_names_its_namespace_too() {
    // `Loan.copy` points at `cat.Copy.id` – across an import and behind an
    // alias. A diagram resolves an edge by namespace + name, because a bare
    // `Copy` could exist in more than one file.
    let result = skema_wasm::analyze_project(ProjectInput {
        files: reference_project(),
        root_path: "/project.skema".to_owned(),
        with_locations: false,
    });
    let loan = result
        .model
        .tables
        .iter()
        .find(|t| t.name == "Loan")
        .expect("Loan table");
    let copy = loan
        .fields
        .iter()
        .find(|f| f.name == "copy")
        .and_then(|f| f.relation.as_ref())
        .expect("Loan.copy is a relation");

    assert_eq!(copy.target_table, "Copy");
    assert_eq!(
        copy.target_namespace, "catalog",
        "the alias `cat` resolves to the target file's own Document name"
    );
}

#[wasm_bindgen_test]
fn highlight_tokens_cover_the_document_in_ascending_order() {
    let flat = skema_wasm::highlight_tokens(EXAMPLE);
    assert!(!flat.is_empty());
    assert_eq!(flat.len() % 3, 0, "flat [class, start, end] triples");

    let names = skema_wasm::highlight_class_names();
    let end_of_document = EXAMPLE.encode_utf16().count() as u32;
    let mut previous_start = 0;
    for triple in flat.chunks_exact(3) {
        let (class, start, end) = (triple[0], triple[1], triple[2]);
        assert!(
            (class as usize) < names.len(),
            "class code is a valid index"
        );
        assert!(start < end, "no empty ranges are emitted");
        assert!(end <= end_of_document, "ranges stay inside the document");
        assert!(start >= previous_start, "ranges are ascending");
        previous_start = start;
    }
}

#[wasm_bindgen_test]
fn table_metas_reach_the_boundary_intact() {
    let source = r#"Table Customer { Meta { "x-editorPosition" = "120,340"; } }"#;
    let metas = skema_wasm::table_metas(source).tables;
    let customer = metas.first().expect("Customer");
    assert_eq!(customer.name, "Customer");
    assert!(customer.meta_body_start.is_some());
    assert_eq!(
        customer
            .entries
            .iter()
            .find(|e| e.key == "x-editorPosition")
            .and_then(|e| e.value.as_deref()),
        Some("120,340")
    );
}

#[wasm_bindgen_test]
fn enum_and_group_metas_reach_the_boundary_intact() {
    let source = r#"
        Enum Status { Meta { styleClasses = [highlight]; } draft; }
        Group Core { Meta { "x-editorPosition" = "10,20"; } Table Customer; }
    "#;

    let enums = skema_wasm::enum_metas(source).enums;
    let status = enums.first().expect("Status");
    assert_eq!(status.name, "Status");
    assert_eq!(
        status
            .entries
            .iter()
            .find(|e| e.key == "styleClasses")
            .and_then(|e| e.value.as_deref()),
        Some("highlight")
    );

    let groups = skema_wasm::group_metas(source).groups;
    let core = groups.first().expect("Core");
    assert_eq!(core.name, "Core");
    assert_eq!(
        core.entries
            .iter()
            .find(|e| e.key == "x-editorPosition")
            .and_then(|e| e.value.as_deref()),
        Some("10,20")
    );
}

#[wasm_bindgen_test]
fn style_blocks_reach_the_boundary_intact() {
    let source = r##"
        Project {
          Style @table { headerBackgroundColor = "#1f4e79"; }
          Style highlight { borderColor = "#cc0000"; }
        }
    "##;
    let blocks = skema_wasm::style_blocks(source).blocks;
    assert_eq!(blocks.len(), 2);

    let table_style = blocks
        .iter()
        .find(|b| b.selector.as_deref() == Some("table"))
        .expect("@table selector block");
    assert_eq!(table_style.scope, skema_wasm::StyleScope::Project);
    assert_eq!(
        table_style
            .entries
            .iter()
            .find(|e| e.key == "headerBackgroundColor")
            .and_then(|e| e.value.as_deref()),
        Some("#1f4e79")
    );

    let class = blocks
        .iter()
        .find(|b| b.class_name.as_deref() == Some("highlight"))
        .expect("highlight class block");
    assert!(class.selector.is_none());
}

#[wasm_bindgen_test]
fn builtins_expose_the_checkers_own_catalogs() {
    let builtins = skema_wasm::builtins_js();
    assert!(builtins.flag_keys.iter().any(|k| k == "pk"));
    assert!(builtins.flag_keys.iter().any(|k| k == "onDelete"));
    assert!(builtins.builtin_types.iter().any(|t| t == "varchar"));
    assert!(builtins.meta_keys.iter().any(|k| k == "styleClasses"));
    assert!(builtins.block_keywords.iter().any(|k| k == "Table"));
}
