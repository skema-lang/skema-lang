//! Integration tests for `skema fmt`, run against the built binary —
//! follows `diff.rs`'s style: real files in a tempdir, real subprocess.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Badly indented (4 spaces) but otherwise valid.
const MISFORMATTED: &str = "spec 1;\nTable Order {\n    id int [pk];\n}\n";

/// The same model, already at the 2-space convention.
const FORMATTED: &str = "spec 1;\nTable Order {\n  id int [pk];\n}\n";

fn write_file(dir: &std::path::Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, content).expect("write fixture");
    path
}

fn skema_fmt(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_skema"))
        .arg("fmt")
        .args(args)
        .output()
        .expect("run skema fmt")
}

#[test]
fn rewrites_a_misformatted_file_in_place_by_default() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = write_file(dir.path(), "order.skema", MISFORMATTED);

    let output = skema_fmt(&[file.to_str().unwrap()]);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);

    let rewritten = fs::read_to_string(&file).expect("read back");
    assert_eq!(rewritten, FORMATTED);
}

#[test]
fn leaves_an_already_formatted_file_byte_identical() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = write_file(dir.path(), "order.skema", FORMATTED);

    let output = skema_fmt(&[file.to_str().unwrap()]);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);

    let rewritten = fs::read_to_string(&file).expect("read back");
    assert_eq!(rewritten, FORMATTED);
}

#[test]
fn check_on_a_misformatted_file_fails_and_writes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = write_file(dir.path(), "order.skema", MISFORMATTED);

    let output = skema_fmt(&["--check", file.to_str().unwrap()]);
    assert!(!output.status.success());

    let untouched = fs::read_to_string(&file).expect("read back");
    assert_eq!(untouched, MISFORMATTED, "--check must not write the file");
}

#[test]
fn check_on_an_already_formatted_file_succeeds_and_writes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = write_file(dir.path(), "order.skema", FORMATTED);

    let output = skema_fmt(&["--check", file.to_str().unwrap()]);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);

    let untouched = fs::read_to_string(&file).expect("read back");
    assert_eq!(untouched, FORMATTED);
}

#[test]
fn an_unreadable_file_fails_without_panicking() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("does-not-exist.skema");

    let output = skema_fmt(&[missing.to_str().unwrap()]);
    assert!(!output.status.success());
}
