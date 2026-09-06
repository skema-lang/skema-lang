//! A minimal smoke test for the wasm-bindgen boundary itself – not a
//! reimplementation of `skema-hir`'s own test suite. Run via
//! `wasm-pack test --node` (no DOM/browser APIs are touched).
//!
//! `clippy.toml`'s `allow-expect-in-tests` recognizes plain `#[test]`, not
//! `#[wasm_bindgen_test]` – so `expect()` is allowed explicitly here for the
//! same reason it's allowed everywhere else under `tests/`: a panic here
//! *is* the failure message.
#![allow(clippy::expect_used)]

use wasm_bindgen_test::*;

const EXAMPLE: &str = include_str!("../../../examples/myexample.skema");

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
