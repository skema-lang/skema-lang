//! Integration tests for `skema diff`, run against the built binary –
//! `skema-cli` has no prior test precedent, so this is the first (`check`
//! and `export` are exercised manually per `README.md`).
//!
//! `clippy.toml`'s `allow-expect-in-tests`/`allow-unwrap-in-tests` only
//! recognize calls inside a function itself annotated `#[test]`, not in the
//! helpers those tests call – allowed explicitly here for the same reason
//! it's allowed everywhere else under `tests/`: a panic here *is* the
//! failure message.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const OLD: &str = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; }\n\
Table Order { id int [pk]; customer_id int [required]; }\n";

/// Drops `customer_id` (breaking) and adds `Shipment` (non-breaking).
const NEW_BREAKING: &str = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; }\n\
Table Order { id int [pk]; }\nTable Shipment { id int [pk]; }\n";

/// Only adds `Shipment` – no breaking change.
const NEW_NON_BREAKING: &str = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; }\n\
Table Order { id int [pk]; customer_id int [required]; }\nTable Shipment { id int [pk]; }\n";

const SYNTAX_ERROR: &str = "this is not a valid .skema file {{{";

/// Writes `content` to `<dir>/<name>` and returns its path.
fn write_project(dir: &std::path::Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, content).expect("write fixture");
    path
}

fn skema_diff(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_skema"))
        .arg("diff")
        .args(args)
        .output()
        .expect("run skema diff")
}

#[test]
fn text_report_lists_changes_and_marks_breaking_ones() {
    let dir = tempfile::tempdir().expect("tempdir");
    let old = write_project(dir.path(), "old.skema", OLD);
    let new = write_project(dir.path(), "new.skema", NEW_BREAKING);

    let output = skema_diff(&[old.to_str().unwrap(), new.to_str().unwrap()]);
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");

    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    assert!(
        stdout.contains("column customer_id dropped [BREAKING]"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("Table Shipment: table added"),
        "stdout: {stdout}"
    );
    // The added table is non-breaking, so its line carries no marker.
    assert!(
        !stdout
            .lines()
            .any(|l| l.contains("Shipment") && l.contains("[BREAKING]")),
        "stdout: {stdout}"
    );
}

#[test]
fn json_flag_produces_parseable_json_with_the_breaking_summary() {
    let dir = tempfile::tempdir().expect("tempdir");
    let old = write_project(dir.path(), "old.skema", OLD);
    let new = write_project(dir.path(), "new.skema", NEW_BREAKING);

    let output = skema_diff(&["--json", old.to_str().unwrap(), new.to_str().unwrap()]);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);

    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout is valid JSON");
    assert_eq!(value["hasBreakingChanges"], true);
    assert!(value["tables"].as_array().expect("tables array").len() >= 2);
}

#[test]
fn fail_on_breaking_flips_the_exit_code_only_when_set() {
    let dir = tempfile::tempdir().expect("tempdir");
    let old = write_project(dir.path(), "old.skema", OLD);
    let new_breaking = write_project(dir.path(), "new_breaking.skema", NEW_BREAKING);
    let new_safe = write_project(dir.path(), "new_safe.skema", NEW_NON_BREAKING);

    let breaking_with_flag = skema_diff(&[
        "--fail-on-breaking",
        old.to_str().unwrap(),
        new_breaking.to_str().unwrap(),
    ]);
    assert!(!breaking_with_flag.status.success());

    let breaking_without_flag =
        skema_diff(&[old.to_str().unwrap(), new_breaking.to_str().unwrap()]);
    assert!(breaking_without_flag.status.success());

    let safe_with_flag = skema_diff(&[
        "--fail-on-breaking",
        old.to_str().unwrap(),
        new_safe.to_str().unwrap(),
    ]);
    assert!(safe_with_flag.status.success());
}

#[test]
fn a_syntax_error_on_either_side_fails_without_attempting_a_diff() {
    let dir = tempfile::tempdir().expect("tempdir");
    let old = write_project(dir.path(), "old.skema", OLD);
    let bad_new = write_project(dir.path(), "bad_new.skema", SYNTAX_ERROR);

    let output = skema_diff(&[old.to_str().unwrap(), bad_new.to_str().unwrap()]);
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");

    assert!(!output.status.success());
    assert!(
        !stdout.contains("Table") && !stdout.contains("added") && !stdout.contains("dropped"),
        "expected no diff output, got stdout: {stdout}"
    );
}
