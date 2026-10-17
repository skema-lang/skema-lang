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
