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

// ---------------------------------------------------------------------------
// `--sort`
// ---------------------------------------------------------------------------

/// Declarations in reverse-canonical order, badly indented, with a comment
/// in a few positions.
const UNSORTED_WITH_COMMENTS: &str = "\
Datatype Money {
  Field { type = float; }
}

Enum Status {
    draft; # first status
}

/* the main table */
Table Zeta {
  id int [pk];
}

Table Alpha {
    id int [pk];
}

Partial base {
  id int [pk];
}

import \"./b.skema\";
import \"./a.skema\";
spec 1;
";

const SORTED_NO_COMMENTS: &str = "spec 1;\n\nimport \"./a.skema\";\nimport \"./b.skema\";\n\nPartial base {\n  id int [pk];\n}\n\nTable Alpha {\n  id int [pk];\n}\n\nTable Zeta {\n  id int [pk];\n}\n\nEnum Status {\n  draft;\n}\n\nDatatype Money {\n  Field { type = float; }\n}\n";

#[test]
fn sort_rewrites_in_canonical_order_and_warns_with_the_comment_count() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = write_file(dir.path(), "model.skema", UNSORTED_WITH_COMMENTS);

    let output = skema_fmt(&["--sort", file.to_str().unwrap()]);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);

    let rewritten = fs::read_to_string(&file).expect("read back");
    assert_eq!(rewritten, SORTED_NO_COMMENTS);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.trim(),
        format!(
            "warning: removed 2 comments while sorting {}",
            file.display()
        )
    );
}

#[test]
fn sort_check_fails_and_writes_nothing_on_an_unsorted_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = write_file(dir.path(), "model.skema", UNSORTED_WITH_COMMENTS);

    let output = skema_fmt(&["--sort", "--check", file.to_str().unwrap()]);
    assert!(!output.status.success());

    let untouched = fs::read_to_string(&file).expect("read back");
    assert_eq!(
        untouched, UNSORTED_WITH_COMMENTS,
        "--sort --check must not write the file"
    );
}

#[test]
fn sort_check_succeeds_on_an_already_sorted_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = write_file(dir.path(), "model.skema", SORTED_NO_COMMENTS);

    let output = skema_fmt(&["--sort", "--check", file.to_str().unwrap()]);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);

    let untouched = fs::read_to_string(&file).expect("read back");
    assert_eq!(untouched, SORTED_NO_COMMENTS);

    assert!(
        output.stderr.is_empty(),
        "no comments to remove means no warning: {:?}",
        output.stderr
    );
}

#[test]
fn sort_prints_no_warning_when_there_are_no_comments() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = write_file(dir.path(), "model.skema", SORTED_NO_COMMENTS);

    let output = skema_fmt(&["--sort", file.to_str().unwrap()]);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    assert!(
        output.stderr.is_empty(),
        "expected no warning, got: {:?}",
        output.stderr
    );
}

#[test]
fn sort_refuses_and_leaves_the_file_untouched_when_it_cannot_sort_safely() {
    // A top-level `Table` with no name has nowhere well-defined to sort to.
    let unnameable = "Table {\n  id int;\n}\n";
    let dir = tempfile::tempdir().expect("tempdir");
    let file = write_file(dir.path(), "model.skema", unnameable);

    let output = skema_fmt(&["--sort", file.to_str().unwrap()]);
    assert!(!output.status.success());

    let untouched = fs::read_to_string(&file).expect("read back");
    assert_eq!(
        untouched, unnameable,
        "a refused sort must not write the file"
    );
}
