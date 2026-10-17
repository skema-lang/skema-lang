//! Golden test of `skema export --json --locations` over a multi-file
//! fixture: the shape external generators code against (`decisions.md`,
//! "Export contract"). Regenerate after an intended change:
//! `SKEMA_UPDATE_SNAPSHOT=1`.

// Helpers outside `#[test]` fns are not covered by clippy.toml's test allowance.
#![allow(clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/export-contract"
);
const GOLDEN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/export-contract.json"
);

/// The export's stdout, with the machine-specific fixture directory cut
/// from every path.
fn export_fixture() -> String {
    let dir = Path::new(FIXTURE)
        .canonicalize()
        .expect("fixture directory");
    let output = Command::new(env!("CARGO_BIN_EXE_skema"))
        .args(["export", "--json", "--locations"])
        .arg(dir.join("project.skema"))
        .output()
        .expect("run skema export");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stderr.is_empty(),
        "stderr: {stderr}"
    );
    String::from_utf8(output.stdout)
        .expect("utf8 stdout")
        .replace(&format!("{}/", dir.display()), "")
}

fn find<'a>(list: &'a Value, name: &str) -> &'a Value {
    list.as_array()
        .and_then(|items| items.iter().find(|item| item["name"] == name))
        .unwrap_or_else(|| panic!("no `{name}` in {list}"))
}

#[test]
fn the_export_matches_the_golden_json() {
    let actual = export_fixture();
    if std::env::var_os("SKEMA_UPDATE_SNAPSHOT").is_some() {
        std::fs::write(GOLDEN, &actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(GOLDEN).expect("read golden");
    for (line, (a, e)) in actual.lines().zip(expected.lines()).enumerate() {
        assert_eq!(a, e, "golden line {}", line + 1);
    }
    assert_eq!(actual.lines().count(), expected.lines().count());
}

#[test]
fn only_x_keys_reach_extensions_as_text() {
    let model: Value = serde_json::from_str(&export_fixture()).expect("export is JSON");

    let customer = find(&model["tables"], "Customer");
    assert_eq!(
        customer["extensions"],
        json!({ "x-nocobase-title": "Customers" })
    );
    let created_at = find(&customer["fields"], "created_at");
    assert_eq!(created_at["file"], "shared.skema");
    assert_eq!(
        created_at["extensions"],
        json!({ "x-nocobase-interface": "createdAt" })
    );
    let name = find(&customer["fields"], "name");
    assert_eq!(name["extensions"], json!({ "x-ui-order": "1" }));
    assert!(find(&name["flags"], "x-ui-widget").is_object());
    assert_eq!(find(&customer["fields"], "id")["extensions"], json!({}));
    assert_eq!(find(&model["tables"], "Tag")["extensions"], json!({}));

    let status = find(&model["enums"], "Status");
    assert_eq!(
        status["extensions"],
        json!({ "x-icon": "flag", "x-source": "field" })
    );
    assert_eq!(
        find(&status["entries"], "active")["extensions"],
        json!({ "x-color": "green" })
    );
    assert_eq!(
        find(&status["entries"], "archived")["extensions"],
        json!({})
    );

    assert_eq!(
        find(&model["groups"], "Core")["extensions"],
        json!({ "x-color": "blue" })
    );

    let documents: Vec<(&Value, &Value, &Value)> = model["documents"]
        .as_array()
        .expect("documents is a list")
        .iter()
        .map(|d| (&d["file"], &d["namespace"], &d["extensions"]))
        .collect();
    assert_eq!(
        documents,
        [
            (
                &json!("project.skema"),
                &json!("shop"),
                &json!({ "x-nocobase-datasource": "main" })
            ),
            (&json!("shared.skema"), &json!("shared"), &json!({})),
        ]
    );
}

#[test]
fn an_enum_exports_its_default_entry_and_field_note() {
    let model: Value = serde_json::from_str(&export_fixture()).expect("export is JSON");

    let status = find(&model["enums"], "Status");
    assert_eq!(status["default"], "active");
    assert_eq!(status["note"], "Stored as text");
}

#[test]
fn a_field_type_names_its_declaration_by_document_namespace_not_alias() {
    let model: Value = serde_json::from_str(&export_fixture()).expect("export is JSON");

    let customer = find(&model["tables"], "Customer");
    let status = &find(&customer["fields"], "status")["ty"];
    assert_eq!(status["name"], "sh.Status");
    assert_eq!(
        status["enum"],
        json!({ "namespace": "shared", "name": "Status" })
    );
    assert_eq!(status["datatype"], Value::Null);
    let balance = &find(&customer["fields"], "balance")["ty"];
    assert_eq!(balance["name"], "sh.Money");
    assert_eq!(
        balance["datatype"],
        json!({ "namespace": "shared", "name": "Money" })
    );
    assert_eq!(balance["enum"], Value::Null);

    let tag = find(&model["tables"], "Tag");
    for builtin_or_relation in ["label", "customer"] {
        let ty = &find(&tag["fields"], builtin_or_relation)["ty"];
        assert_eq!(ty["datatype"], Value::Null, "{builtin_or_relation}");
        assert_eq!(ty["enum"], Value::Null, "{builtin_or_relation}");
    }
}

#[test]
fn every_datatype_is_exported_with_its_base_chain_resolved_by_name() {
    let model: Value = serde_json::from_str(&export_fixture()).expect("export is JSON");

    let names: Vec<(&Value, &Value)> = model["datatypes"]
        .as_array()
        .expect("datatypes is a list")
        .iter()
        .map(|d| (&d["namespace"], &d["name"]))
        .collect();
    assert_eq!(
        names,
        [
            (&json!("shared"), &json!("Money")),
            (&json!("shared"), &json!("decimal")),
            (&json!("shop"), &json!("Sku")),
        ]
    );

    let money = find(&model["datatypes"], "Money");
    assert_eq!(money["baseType"], "decimal");
    assert_eq!(money["baseTypeArgs"], json!(["10"]));
    assert_eq!(
        money["baseDatatype"],
        json!({ "namespace": "shared", "name": "decimal" })
    );
    assert_eq!(money["resolvedBaseType"], "float");

    let decimal = find(&model["datatypes"], "decimal");
    assert_eq!(
        decimal["params"][1],
        json!({ "name": "scale", "type": "int", "default": "2", "note": "Fixed decimal places" })
    );
    assert_eq!(decimal["default"], "0");
    assert_eq!(
        decimal["extensions"],
        json!({ "x-source": "field", "x-unit": "none" })
    );

    let sku = find(&model["datatypes"], "Sku");
    assert_eq!(sku["regex"], "^SKU-[A-Z0-9]{length}$");
    assert_eq!(sku["baseDatatype"], Value::Null);
    assert_eq!(sku["extensions"], json!({}));
}
