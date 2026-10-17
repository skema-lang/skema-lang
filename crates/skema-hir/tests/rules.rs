//! Checks that actual rule violations are recognized as such – the
//! counterpart to `tests/examples.rs`, which only shows the success case.

use skema_hir::model::Model;
use skema_hir::{Diagnostic, FileId, InputFile, analyze};

fn analyze_src(src: &str) -> (Model, Vec<Diagnostic>) {
    let (ast, parse_diags) = skema_syntax::parse(src);
    assert!(
        parse_diags.is_empty(),
        "source already contains parser diagnostics: {src}"
    );
    let import_count = ast.imports().count();
    let file = InputFile {
        id: FileId(0),
        display_path: "test.skema".to_owned(),
        ast: &ast,
        import_targets: vec![None; import_count],
    };
    let (model, _references, diags) = analyze(&[file]);
    (model, diags)
}

fn messages(diags: &[Diagnostic]) -> Vec<&str> {
    diags.iter().map(|d| d.message.as_str()).collect()
}

const HEADER: &str = r#"
spec 1;
Document shop { }
Project { mode = draft; dialect = generic; }
"#;

#[test]
fn missing_project_block_is_reported() {
    let (_, diags) = analyze_src("spec 1;\nDocument shop { }\n");
    assert!(
        messages(&diags).iter().any(|m| m.contains("Project")),
        "{diags:#?}"
    );
}

#[test]
fn second_project_block_is_reported() {
    let src = format!("{HEADER}\nProject {{ mode = draft; dialect = generic; }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("second `Project`")),
        "{diags:#?}"
    );
}

#[test]
fn lint_outside_project_is_rejected() {
    let src =
        format!("{HEADER}\nTable T {{ id int [pk]; Lint {{ fieldNaming = snake_case; }} }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("`Lint` is not allowed here")),
        "{diags:#?}"
    );
}

#[test]
fn unquoted_custom_key_is_rejected() {
    let src = format!("{HEADER}\nTable T {{ id int [pk]; Meta {{ athor = \"anna\"; }} }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("unknown key `athor`")),
        "{diags:#?}"
    );
}

#[test]
fn quoted_key_without_x_prefix_is_rejected() {
    let src = format!("{HEADER}\nTable T {{ id int [pk]; Meta {{ \"jira\" = \"SHOP-1\"; }} }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("must start with `x-`")),
        "{diags:#?}"
    );
}

#[test]
fn known_key_with_x_prefix_is_allowed() {
    let src = format!("{HEADER}\nTable T {{ id int [pk]; Meta {{ \"x-jira\" = \"SHOP-1\"; }} }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn moved_from_on_a_table_meta_is_accepted_and_resolved() {
    let src = format!("{HEADER}\nTable T {{ id int [pk]; Meta {{ movedFrom = \"OldName\"; }} }}\n");
    let (model, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
    assert_eq!(model.tables[0].moved_from.as_deref(), Some("OldName"));
}

#[test]
fn table_without_moved_from_resolves_to_none() {
    let src = format!("{HEADER}\nTable T {{ id int [pk]; }}\n");
    let (model, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
    assert_eq!(model.tables[0].moved_from, None);
}

fn extensions(pairs: &[(&str, &str)]) -> skema_hir::model::Extensions {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn x_keys_in_meta_reach_the_model_and_standard_keys_do_not() {
    let src = r#"
spec 1;
Document shop { Meta { author = "anna"; "x-source" = "erp"; } }
Project { mode = draft; dialect = generic; }
Table T { id int [pk]; Meta { note = "n"; "x-title" = "Things"; } }
Group G { Meta { author = "anna"; "x-color" = "blue"; } Table T; }
"#;
    let (model, diags) = analyze_src(src);
    assert!(diags.is_empty(), "{diags:#?}");
    assert_eq!(
        model.tables[0].extensions,
        extensions(&[("x-title", "Things")])
    );
    assert_eq!(
        model.groups[0].extensions,
        extensions(&[("x-color", "blue")])
    );
    assert_eq!(model.documents.len(), 1);
    assert_eq!(model.documents[0].namespace, "shop");
    assert_eq!(
        model.documents[0].extensions,
        extensions(&[("x-source", "erp")])
    );
}

#[test]
fn an_enums_field_block_wins_over_its_meta_on_the_same_x_key() {
    let src = format!(
        r#"{HEADER}
Enum Status {{
  Meta {{ note = "n"; "x-source" = "meta"; "x-icon" = "flag"; }}
  Field {{ type = string; note = "f"; "x-source" = "field"; }}
  active {{ label = "Active"; "x-color" = "green"; }}
  archived;
}}
"#
    );
    let (model, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
    let status = &model.enums[0];
    assert_eq!(
        status.extensions,
        extensions(&[("x-icon", "flag"), ("x-source", "field")])
    );
    assert_eq!(
        status.entries[0].extensions,
        extensions(&[("x-color", "green")])
    );
    assert!(status.entries[1].extensions.is_empty());
}

#[test]
fn an_x_flag_stays_in_flags_and_an_x_body_key_goes_to_extensions() {
    let src = format!(
        r#"{HEADER}
Table T {{
  id   int [pk];
  name text ["x-widget" = "input"] {{ note = "n"; "x-order" = 1; }}
}}
"#
    );
    let (model, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
    let name = &model.tables[0].fields[1];
    assert_eq!(name.extensions, extensions(&[("x-order", "1")]));
    assert!(name.flags.contains_key("x-widget"));
    assert!(model.tables[0].fields[0].extensions.is_empty());
    assert!(model.tables[0].extensions.is_empty());
}

#[test]
fn x_values_are_text_and_the_first_occurrence_in_a_block_wins() {
    let src = format!(
        "{HEADER}\nTable T {{ id int [pk]; Meta {{ \"x-a\" = true; \"x-b\" = [one, two]; \
         \"x-c\" = `now()`; \"x-a\" = false; }} }}\n"
    );
    let (model, _) = analyze_src(&src);
    assert_eq!(
        model.tables[0].extensions,
        extensions(&[("x-a", "true"), ("x-b", "one, two"), ("x-c", "now()")])
    );
}

#[test]
fn moved_from_with_a_non_string_value_is_rejected() {
    let src = format!("{HEADER}\nTable T {{ id int [pk]; Meta {{ movedFrom = [foo]; }} }}\n");
    let (model, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("`movedFrom` must be a string")),
        "{diags:#?}"
    );
    assert_eq!(model.tables[0].moved_from, None);
}

#[test]
fn moved_from_is_rejected_in_project_meta() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; Meta { movedFrom = \"OldName\"; } }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("unknown key `movedFrom`")),
        "{diags:#?}"
    );
}

#[test]
fn moved_from_is_rejected_in_document_meta() {
    let src = "spec 1;\nDocument shop { Meta { movedFrom = \"OldName\"; } }\nProject { mode = draft; dialect = generic; }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("unknown key `movedFrom`")),
        "{diags:#?}"
    );
}

#[test]
fn moved_from_is_rejected_in_partial_meta() {
    let src = format!(
        "{HEADER}\nPartial P {{ id int [pk]; Meta {{ movedFrom = \"OldName\"; }} }}\nTable T {{ use P; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("unknown key `movedFrom`")),
        "{diags:#?}"
    );
}

#[test]
fn moved_from_is_rejected_in_enum_meta() {
    let src = format!(
        "{HEADER}\nEnum Status {{ Field {{ type = string; }} Meta {{ movedFrom = \"OldName\"; }} draft; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("unknown key `movedFrom`")),
        "{diags:#?}"
    );
}

#[test]
fn moved_from_is_rejected_in_group_meta() {
    let src = format!(
        "{HEADER}\nTable Order {{ id int [pk]; }}\nGroup G {{ Meta {{ movedFrom = \"OldName\"; }} Table Order; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("unknown key `movedFrom`")),
        "{diags:#?}"
    );
}

#[test]
fn moved_from_is_rejected_in_datatype_meta() {
    let src = format!(
        "{HEADER}\nDatatype D(a int) {{ Field {{ type = int; }} Meta {{ movedFrom = \"OldName\"; }} }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("unknown key `movedFrom`")),
        "{diags:#?}"
    );
}

/// A field body needs no `[…]` flags of its own – `bar string { label =
/// "…"; };` is the whole statement (`docs/05`, the "any part may be
/// missing" rule).
#[test]
fn a_field_body_without_flags_is_allowed() {
    let src = format!("{HEADER}\nTable T {{ bar string {{ label = \"hallo\"; }} }}\n");
    let (model, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
    let field = &model.tables[0].fields[0];
    assert_eq!(field.label.as_deref(), Some("hallo"));
}

#[test]
fn group_with_wrong_kind_is_rejected() {
    let src = format!("{HEADER}\nTable Order {{ id int [pk]; }}\nGroup G {{ Enum Order; }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("is a `Table`, expected `Enum`")),
        "{diags:#?}"
    );
}

#[test]
fn duplicate_group_membership_is_rejected() {
    let src = format!(
        "{HEADER}\nTable Customer {{ id int [pk]; }}\nGroup A {{ Table Customer; }}\nGroup B {{ Table Customer; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("already belongs to group")),
        "{diags:#?}"
    );
}

/// `main` is `FileId(0)` and imports `imported` (`FileId(1)`, namespace `a`).
fn analyze_main_importing(main: &str, imported: &str) -> Vec<Diagnostic> {
    analyze_main_importing_model(main, imported).1
}

fn analyze_main_importing_model(main: &str, imported: &str) -> (Model, Vec<Diagnostic>) {
    let ast_main = skema_syntax::parse(main).0;
    let ast_imported = skema_syntax::parse(imported).0;
    let files = [
        InputFile {
            id: FileId(0),
            display_path: "main.skema".to_owned(),
            ast: &ast_main,
            import_targets: vec![Some(FileId(1))],
        },
        InputFile {
            id: FileId(1),
            display_path: "a.skema".to_owned(),
            ast: &ast_imported,
            import_targets: vec![],
        },
    ];
    let (model, _, diags) = analyze(&files);
    (model, diags)
}

fn diagnostics_containing<'a>(diags: &'a [Diagnostic], needle: &str) -> Vec<&'a Diagnostic> {
    diags
        .iter()
        .filter(|d| d.message.contains(needle))
        .collect()
}

#[test]
fn duplicate_group_membership_across_files_is_reported_in_the_later_file() {
    let main = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; }\nimport \"./a.skema\";\nGroup Billing { Table a.X; }\n";
    let imported =
        "spec 1;\nDocument a { }\nTable X { id int [pk]; }\nGroup MasterData { Table X; }\n";

    let diags = analyze_main_importing(main, imported);
    let belongs = diagnostics_containing(&diags, "already belongs to group `Billing`");
    assert_eq!(belongs.len(), 1, "{diags:#?}");
    assert_eq!(belongs[0].file, FileId(1));
}

#[test]
fn same_named_groups_in_two_files_are_different_groups() {
    let main = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; }\nimport \"./a.skema\";\nGroup A { Table a.X; }\n";
    let imported = "spec 1;\nDocument a { }\nTable X { id int [pk]; }\nGroup A { Table X; }\n";

    let diags = analyze_main_importing(main, imported);
    assert_eq!(
        diagnostics_containing(&diags, "already belongs to group `A`").len(),
        1,
        "{diags:#?}"
    );
    assert!(
        diagnostics_containing(&diags, "listed twice").is_empty(),
        "{diags:#?}"
    );
}

#[test]
fn a_member_listed_twice_in_one_group_is_reported_and_kept_once() {
    let src = format!("{HEADER}\nTable X {{ id int [pk]; }}\nGroup A {{ Table X; Table X; }}\n");
    let (model, diags) = analyze_src(&src);
    assert_eq!(
        diagnostics_containing(&diags, "`X` is listed twice in group `A`").len(),
        1,
        "{diags:#?}"
    );
    assert!(
        diagnostics_containing(&diags, "already belongs").is_empty(),
        "{diags:#?}"
    );
    let members = &model.groups[0].members;
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].name, "X");
}

#[test]
fn an_enum_in_two_groups_is_rejected() {
    let src = format!(
        "{HEADER}\nEnum Status {{ Field {{ type = string; }} draft; }}\nGroup A {{ Enum Status; }}\nGroup B {{ Enum Status; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert_eq!(
        diagnostics_containing(&diags, "`Status` already belongs to group `A`").len(),
        1,
        "{diags:#?}"
    );
}

#[test]
fn dangling_group_ref_is_an_error_by_default() {
    let src = format!("{HEADER}\nGroup G {{ Table Ghost; }}\n");
    let (_, diags) = analyze_src(&src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("unknown declaration"))
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Error);
}

#[test]
fn dangling_group_ref_is_a_warning_when_configured_under_strict() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { danglingRef = warn; } }\nGroup G { Table Ghost; }\n";
    let (_, diags) = analyze_src(src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("unknown declaration"))
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Warning);
}

#[test]
fn dangling_group_ref_stays_an_error_when_explicitly_configured() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { danglingRef = error; } }\nGroup G { Table Ghost; }\n";
    let (_, diags) = analyze_src(src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("unknown declaration"))
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Error);
}

#[test]
fn dangling_group_ref_ignores_configured_severity_under_draft() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; Lint { danglingRef = warn; } }\nGroup G { Table Ghost; }\n";
    let (_, diags) = analyze_src(src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("unknown declaration"))
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Error);
}

// ===========================================================================
// `Lint.danglingRef` on the other reference kinds `resolve.rs` resolves
// (task 90) – same `dangling_ref_severity` helper as `Group`'s tests above.
// ===========================================================================

#[test]
fn dangling_use_target_is_an_error_by_default() {
    let src = format!("{HEADER}\nTable T {{ use Ghost; id uuid [pk]; }}\n");
    let (_, diags) = analyze_src(&src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("unknown `Partial` `Ghost`"))
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Error);
}

#[test]
fn dangling_use_target_is_a_warning_when_configured_under_strict() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { danglingRef = warn; } }\nTable T { use Ghost; id uuid [pk]; }\n";
    let (_, diags) = analyze_src(src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("unknown `Partial` `Ghost`"))
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Warning);
}

#[test]
fn dangling_use_target_ignores_configured_severity_under_draft() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; Lint { danglingRef = warn; } }\nTable T { use Ghost; id uuid [pk]; }\n";
    let (_, diags) = analyze_src(src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("unknown `Partial` `Ghost`"))
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Error);
}

#[test]
fn dangling_relation_table_is_an_error_by_default() {
    let src = format!("{HEADER}\nTable T {{ id int [pk]; customer >Ghost.id; }}\n");
    let (_, diags) = analyze_src(&src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("unknown table `Ghost`"))
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Error);
}

#[test]
fn dangling_relation_table_is_a_warning_when_configured_under_strict() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { danglingRef = warn; } }\nTable T { id uuid [pk]; customer >Ghost.id; }\n";
    let (model, diags) = analyze_src(src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("unknown table `Ghost`"))
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Warning);

    // Owner-decided model behaviour (task 90): downgrading to `warn` keeps
    // the field, untyped and relation-less, and blocks nothing.
    let table = model.tables.iter().find(|t| t.name == "T").unwrap();
    let field = table.fields.iter().find(|f| f.name == "customer").unwrap();
    assert_eq!(field.ty.name, "unknown");
    assert!(field.relation.is_none());
    assert!(
        !diags
            .iter()
            .any(|d| d.severity == skema_hir::Severity::Error),
        "{diags:#?}"
    );
}

#[test]
fn dangling_relation_table_ignores_configured_severity_under_draft() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; Lint { danglingRef = warn; } }\nTable T { id int [pk]; customer >Ghost.id; }\n";
    let (_, diags) = analyze_src(src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("unknown table `Ghost`"))
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Error);
}

#[test]
fn dangling_relation_field_is_an_error_by_default() {
    let src = format!(
        "{HEADER}\nTable Customer {{ id int [pk]; }}\nTable Order {{ id int [pk]; customer >Customer.ghost; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("`Customer` has no field `ghost`"))
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Error);
}

#[test]
fn dangling_relation_field_is_a_warning_when_configured_under_strict() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { danglingRef = warn; } }\nTable Customer { id uuid [pk]; }\nTable Order { id uuid [pk]; customer >Customer.ghost; }\n";
    let (_, diags) = analyze_src(src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("`Customer` has no field `ghost`"))
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Warning);
}

#[test]
fn dangling_relation_field_ignores_configured_severity_under_draft() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; Lint { danglingRef = warn; } }\nTable Customer { id int [pk]; }\nTable Order { id int [pk]; customer >Customer.ghost; }\n";
    let (_, diags) = analyze_src(src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("`Customer` has no field `ghost`"))
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Error);
}

#[test]
fn dangling_type_name_is_an_error_by_default() {
    let src = format!("{HEADER}\nTable T {{ x ghosttype; }}\n");
    let (_, diags) = analyze_src(&src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("unknown type `ghosttype`"))
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Error);
}

#[test]
fn dangling_type_name_is_a_warning_when_configured_under_strict() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { danglingRef = warn; } }\nTable T { x ghosttype; }\n";
    let (_, diags) = analyze_src(src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("unknown type `ghosttype`"))
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Warning);
}

#[test]
fn dangling_type_name_ignores_configured_severity_under_draft() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; Lint { danglingRef = warn; } }\nTable T { x ghosttype; }\n";
    let (_, diags) = analyze_src(src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("unknown type `ghosttype`"))
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Error);
}

#[test]
fn dangling_type_name_under_a_dialect_is_also_governed() {
    // The dialect-specific "unknown type" message (`resolve_named_type`'s
    // other `None` arm) is the same governed diagnostic under a different
    // wording – covers both variants task 90 lists under kind 5.
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = postgres; Lint { danglingRef = warn; } }\nTable T { x ghosttype; }\n";
    let (_, diags) = analyze_src(src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("unknown type `ghosttype`"))
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Warning);
}

#[test]
fn dangling_use_wrong_kind_is_an_error_by_default() {
    let src = format!(
        "{HEADER}\nEnum Status {{ Field {{ type = string; }} draft; }}\nTable T {{ use Status; id int [pk]; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| {
            d.message
                .contains("expects a `Partial`, `Status` is a `Enum`")
        })
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Error);
}

#[test]
fn dangling_use_wrong_kind_is_a_warning_when_configured_under_strict() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { danglingRef = warn; } }\nEnum Status { Field { type = string; } draft; }\nTable T { use Status; id uuid [pk]; }\n";
    let (_, diags) = analyze_src(src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| {
            d.message
                .contains("expects a `Partial`, `Status` is a `Enum`")
        })
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Warning);
}

#[test]
fn dangling_use_wrong_kind_ignores_configured_severity_under_draft() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; Lint { danglingRef = warn; } }\nEnum Status { Field { type = string; } draft; }\nTable T { use Status; id int [pk]; }\n";
    let (_, diags) = analyze_src(src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| {
            d.message
                .contains("expects a `Partial`, `Status` is a `Enum`")
        })
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Error);
}

#[test]
fn dangling_relation_wrong_kind_is_an_error_by_default() {
    let src = format!(
        "{HEADER}\nEnum Status {{ Field {{ type = string; }} draft; }}\nTable T {{ id int [pk]; x >Status.draft; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| {
            d.message
                .contains("`Status` is not a `Table`, but a `Enum`")
        })
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Error);
}

#[test]
fn dangling_relation_wrong_kind_is_a_warning_when_configured_under_strict() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { danglingRef = warn; } }\nEnum Status { Field { type = string; } draft; }\nTable T { id uuid [pk]; x >Status.draft; }\n";
    let (_, diags) = analyze_src(src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| {
            d.message
                .contains("`Status` is not a `Table`, but a `Enum`")
        })
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Warning);
}

#[test]
fn dangling_relation_wrong_kind_ignores_configured_severity_under_draft() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; Lint { danglingRef = warn; } }\nEnum Status { Field { type = string; } draft; }\nTable T { id int [pk]; x >Status.draft; }\n";
    let (_, diags) = analyze_src(src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| {
            d.message
                .contains("`Status` is not a `Table`, but a `Enum`")
        })
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Error);
}

#[test]
fn use_of_a_namespace_without_a_partial_is_a_hard_error() {
    // `use ns;` where `ns` resolves to an imported namespace but no
    // `Partial` name follows it – previously silent (`Ctx::resolve_decl_path`
    // returning `None`), never governed by `danglingRef` (task 90).
    let imported = "spec 1;\nDocument ns { }\nPartial P { id uuid [pk]; }\n";
    let main = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; }\nimport \"./a.skema\";\nTable T { use ns; other uuid; }\n";

    let ast_imported = skema_syntax::parse(imported).0;
    let ast_main = skema_syntax::parse(main).0;
    let files = [
        InputFile {
            id: FileId(0),
            display_path: "main.skema".to_owned(),
            ast: &ast_main,
            import_targets: vec![Some(FileId(1))],
        },
        InputFile {
            id: FileId(1),
            display_path: "a.skema".to_owned(),
            ast: &ast_imported,
            import_targets: vec![],
        },
    ];

    let (_, _, diags) = analyze(&files);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| {
            d.message
                .contains("`use` expects a `Partial` after namespace `ns`")
        })
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Error);
}

#[test]
fn use_of_a_namespace_without_a_partial_stays_an_error_under_warn() {
    let imported = "spec 1;\nDocument ns { }\nPartial P { id uuid [pk]; }\n";
    let main = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { danglingRef = warn; } }\nimport \"./a.skema\";\nTable T { use ns; other uuid; }\n";

    let ast_imported = skema_syntax::parse(imported).0;
    let ast_main = skema_syntax::parse(main).0;
    let files = [
        InputFile {
            id: FileId(0),
            display_path: "main.skema".to_owned(),
            ast: &ast_main,
            import_targets: vec![Some(FileId(1))],
        },
        InputFile {
            id: FileId(1),
            display_path: "a.skema".to_owned(),
            ast: &ast_imported,
            import_targets: vec![],
        },
    ];

    let (_, _, diags) = analyze(&files);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| {
            d.message
                .contains("`use` expects a `Partial` after namespace `ns`")
        })
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Error);
}

#[test]
fn cycle_in_use_still_errors_under_warn() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { danglingRef = warn; } }\nPartial A { use B; }\nPartial B { use A; }\nTable T { use A; id uuid [pk]; }\n";
    let (_, diags) = analyze_src(src);
    let cycles: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("cycle in `use`"))
        .collect();
    assert!(!cycles.is_empty(), "{diags:#?}");
    assert!(
        cycles
            .iter()
            .all(|d| d.severity == skema_hir::Severity::Error),
        "{diags:#?}"
    );
}

#[test]
fn partial_field_collision_is_rejected() {
    let src =
        format!("{HEADER}\nPartial P {{ id int [pk]; }}\nTable T {{ use P; id uuid [pk]; }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("field name `id` is already taken")),
        "{diags:#?}"
    );
}

#[test]
fn cycle_in_use_is_detected_instead_of_hanging() {
    let src =
        format!("{HEADER}\nPartial A {{ use B; }}\nPartial B {{ use A; }}\nTable T {{ use A; }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("cycle in `use`")),
        "{diags:#?}"
    );
}

#[test]
fn relation_target_without_table_is_rejected() {
    let src = format!("{HEADER}\nTable Order {{ customer >Customer.id; }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("unknown table `Customer`")),
        "{diags:#?}"
    );
}

#[test]
fn a_table_named_like_the_document_resolves_to_itself() {
    // `shop` is both the file's own `Document` name and a `Table` in it –
    // the local declaration must win, not the (redundant) self-namespace
    // reading of the same name.
    let src =
        format!("{HEADER}\nTable shop {{ id int [pk]; }}\nTable Order {{ owner >shop.id; }}\n");
    let (model, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");

    let order = model.tables.iter().find(|t| t.name == "Order").unwrap();
    let relation = order.fields[0].relation.as_ref().unwrap();
    assert_eq!(relation.target_table, "shop");
    assert_eq!(relation.target_field, "id");
}

#[test]
fn on_delete_on_the_wrong_side_is_rejected() {
    let src = format!(
        "{HEADER}\nTable B {{ id int [pk]; }}\nTable A {{ id int [pk]; b <B.id [onDelete=cascade]; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("only allowed on the `>` side")),
        "{diags:#?}"
    );
}

#[test]
fn unknown_on_delete_value_is_rejected() {
    let src = format!(
        "{HEADER}\nTable B {{ id int [pk]; }}\nTable A {{ id int [pk]; b >B.id [onDelete=cascadee]; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("unknown `onDelete` value `cascadee`") && m.contains("cascade")),
        "{diags:#?}"
    );
}

#[test]
fn unknown_on_update_value_is_rejected() {
    let src = format!(
        "{HEADER}\nTable B {{ id int [pk]; }}\nTable A {{ id int [pk]; b >B.id [onUpdate=nope]; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("unknown `onUpdate` value `nope`")),
        "{diags:#?}"
    );
}

#[test]
fn known_relation_action_values_are_accepted() {
    let src = format!(
        "{HEADER}\nTable B {{ id int [pk]; }}\nTable A {{ id int [pk]; b >B.id [onDelete=cascade, onUpdate=setNull]; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn relation_target_of_a_namespace_without_a_table_is_rejected() {
    // A relation target naming only a namespace (`customer >ns;`) – the only
    // other `None` case from `Ctx::resolve_decl_path`, an empty path, cannot
    // occur here. This is an unconditional error, never governed by `danglingRef`.
    let imported = "spec 1;\nDocument ns { }\nTable Customer { id uuid [pk]; }\n";
    let main = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; }\nimport \"./a.skema\";\nTable Order { customer >ns; }\n";

    let ast_imported = skema_syntax::parse(imported).0;
    let ast_main = skema_syntax::parse(main).0;
    let files = [
        InputFile {
            id: FileId(0),
            display_path: "main.skema".to_owned(),
            ast: &ast_main,
            import_targets: vec![Some(FileId(1))],
        },
        InputFile {
            id: FileId(1),
            display_path: "a.skema".to_owned(),
            ast: &ast_imported,
            import_targets: vec![],
        },
    ];

    let (_, _, diags) = analyze(&files);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| {
            d.message.contains(
                "a relationship target names a table and a field, not only namespace `ns`",
            )
        })
        .collect();
    assert_eq!(errors.len(), 1, "{diags:#?}");
    assert_eq!(errors[0].severity, skema_hir::Severity::Error);
}

#[test]
fn relation_target_of_a_namespace_without_a_table_stays_an_error_under_warn() {
    let imported = "spec 1;\nDocument ns { }\nTable Customer { id uuid [pk]; }\n";
    let main = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { danglingRef = warn; } }\nimport \"./a.skema\";\nTable Order { customer >ns; }\n";

    let ast_imported = skema_syntax::parse(imported).0;
    let ast_main = skema_syntax::parse(main).0;
    let files = [
        InputFile {
            id: FileId(0),
            display_path: "main.skema".to_owned(),
            ast: &ast_main,
            import_targets: vec![Some(FileId(1))],
        },
        InputFile {
            id: FileId(1),
            display_path: "a.skema".to_owned(),
            ast: &ast_imported,
            import_targets: vec![],
        },
    ];

    let (_, _, diags) = analyze(&files);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| {
            d.message.contains(
                "a relationship target names a table and a field, not only namespace `ns`",
            )
        })
        .collect();
    assert_eq!(errors.len(), 1, "{diags:#?}");
    assert_eq!(errors[0].severity, skema_hir::Severity::Error);
}

#[test]
fn dialect_in_datatype_without_at_sign_is_rejected() {
    let src = format!(
        "{HEADER}\nDatatype D(a int) {{ Field {{ type = int; }} Dialect postgres {{ }} }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("needs an engine name")),
        "{diags:#?}"
    );
}

#[test]
fn dialect_in_datatype_with_unknown_engine_is_rejected() {
    let src = format!(
        "{HEADER}\nDatatype D(a int) {{ Field {{ type = int; }} Dialect @oracle {{ }} }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("unknown dialect engine `oracle`")),
        "{diags:#?}"
    );
}

#[test]
fn dialect_in_database_without_at_sign_is_rejected() {
    let src = format!(
        "{HEADER}\nTable T {{ id int [pk]; Database {{ table = \"t\"; Dialect mysql {{ }} }} }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("needs an engine name")),
        "{diags:#?}"
    );
}

#[test]
fn dialect_in_database_with_unknown_engine_is_rejected() {
    let src = format!(
        "{HEADER}\nTable T {{ id int [pk]; Database {{ table = \"t\"; Dialect @sqlite {{ }} }} }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("unknown dialect engine `sqlite`")),
        "{diags:#?}"
    );
}

#[test]
fn known_dialect_engines_are_accepted() {
    let src = format!(
        "{HEADER}\nDatatype D(a int) {{ Field {{ type = int; }} Dialect @postgres {{ }} }}\nTable T {{ id int [pk]; Database {{ table = \"t\"; Dialect @mysql {{ }} }} }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn a_datatype_covering_the_active_dialect_is_silent() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = postgres; }\nDatatype D { Field { type = int; } Dialect @postgres { sql = \"int\"; } }\nTable T { x D; }\n";
    let (_, diags) = analyze_src(src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn a_datatype_missing_the_active_dialect_is_a_hint_under_draft() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = mysql; }\nDatatype D { Field { type = int; } Dialect @postgres { sql = \"int\"; } }\nTable T { x D; }\n";
    let (_, diags) = analyze_src(src);
    let missing: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("no `Dialect @mysql` block"))
        .collect();
    assert_eq!(missing.len(), 1, "{diags:#?}");
    assert_eq!(missing[0].severity, skema_hir::Severity::Warning);
}

#[test]
fn a_datatype_missing_the_active_dialect_is_an_error_under_strict() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = mysql; }\nDatatype D { Field { type = int; } Dialect @postgres { sql = \"int\"; } }\nTable T { x D; }\n";
    let (_, diags) = analyze_src(src);
    let missing: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("no `Dialect @mysql` block"))
        .collect();
    assert_eq!(missing.len(), 1, "{diags:#?}");
    assert_eq!(missing[0].severity, skema_hir::Severity::Error);
}

#[test]
fn a_datatype_used_under_generic_dialect_needs_no_dialect_coverage() {
    let src = format!("{HEADER}\nDatatype D {{ Field {{ type = int; }} }}\nTable T {{ x D; }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn unknown_type_under_generic_is_an_error() {
    let src = format!("{HEADER}\nTable T {{ x nonexistenttype; }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("unknown type `nonexistenttype`")),
        "{diags:#?}"
    );
}

#[test]
fn draft_native_types_are_accepted_under_draft() {
    for ty in ["string", "int", "float", "bool"] {
        let src = format!("{HEADER}\nTable T {{ x {ty}; }}\n");
        let (_, diags) = analyze_src(&src);
        assert!(diags.is_empty(), "`{ty}` under draft: {diags:#?}");
    }
}

#[test]
fn draft_native_types_are_rejected_under_strict() {
    for ty in ["string", "int", "float", "bool"] {
        let src = format!(
            "spec 1;\nDocument shop {{ }}\nProject {{ mode = strict; dialect = generic; }}\nTable T {{ x {ty}; }}\n"
        );
        let (_, diags) = analyze_src(&src);
        assert!(
            messages(&diags).iter().any(|m| m.contains(&format!(
                "`{ty}` is not precise enough under `mode = strict`"
            ))),
            "`{ty}` under strict: {diags:#?}"
        );
    }
}

#[test]
fn precise_builtin_types_are_accepted_under_strict() {
    for ty in ["varchar(80)", "text", "uuid", "date", "timestamp"] {
        let src = format!(
            "spec 1;\nDocument shop {{ }}\nProject {{ mode = strict; dialect = generic; }}\nTable T {{ x {ty}; }}\n"
        );
        let (_, diags) = analyze_src(&src);
        assert!(diags.is_empty(), "`{ty}` under strict: {diags:#?}");
    }
}

#[test]
fn a_datatype_typed_field_is_accepted_under_strict() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; }\nDatatype Email { Field { type = string; } }\nTable T { x Email; }\n";
    let (_, diags) = analyze_src(src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn an_enum_typed_field_is_accepted_under_strict() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; }\nEnum Status { Field { type = string; } draft; }\nTable T { x Status; }\n";
    let (_, diags) = analyze_src(src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn unknown_type_under_postgres_is_an_error() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = postgres; }\nTable T { x vachar; }\n";
    let (_, diags) = analyze_src(src);
    let unknown: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("vachar"))
        .collect();
    assert_eq!(unknown.len(), 1, "{diags:#?}");
    assert_eq!(unknown[0].severity, skema_hir::Severity::Error);
}

#[test]
fn unknown_type_under_mysql_is_an_error() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = mysql; }\nTable T { x tinytxt; }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("tinytxt") && m.contains("unknown type")),
        "{diags:#?}"
    );
}

#[test]
fn unknown_type_under_oracledb_is_an_error() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = oracledb; }\nTable T { x varchar3; }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("varchar3") && m.contains("unknown type")),
        "{diags:#?}"
    );
}

#[test]
fn engine_specific_types_are_accepted_per_dialect() {
    let cases = [
        ("postgres", "jsonb"),
        ("postgres", "int8"),
        ("postgres", "doublePrecision"),
        ("mysql", "tinytext"),
        ("mysql", "mediumint"),
        ("oracledb", "varchar2(255)"),
        ("oracledb", "clob"),
    ];
    for (dialect, ty) in cases {
        let src = format!(
            "spec 1;\nDocument shop {{ }}\nProject {{ mode = strict; dialect = {dialect}; }}\nTable T {{ x {ty}; }}\n"
        );
        let (_, diags) = analyze_src(&src);
        assert!(diags.is_empty(), "`{ty}` under `{dialect}`: {diags:#?}");
    }
}

#[test]
fn a_type_from_the_wrong_dialects_catalog_is_rejected() {
    // `tinytext` is MySQL-only; used under `postgres` it must still be an
    // error, not silently accepted because some engine somewhere has it.
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = postgres; }\nTable T { x tinytext; }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("tinytext") && m.contains("unknown type")),
        "{diags:#?}"
    );
}

#[test]
fn dialect_type_argument_shapes_are_checked() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = mysql; }\nTable T { x enum(); }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("one or more string arguments")),
        "{diags:#?}"
    );

    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = oracledb; }\nTable T { x vector(3, \"nope\"); }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("should be one of")),
        "{diags:#?}"
    );
}

#[test]
fn oracledb_timestamp_accepts_its_own_optional_precision() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = oracledb; }\nTable T { x timestamp(6); }\n";
    let (_, diags) = analyze_src(src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn enum_field_type_must_be_string_or_int() {
    let src = format!("{HEADER}\nEnum E {{ Field {{ type = float; }} draft; }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("must be `string` or `int`")),
        "{diags:#?}"
    );
}

#[test]
fn an_enum_default_naming_an_entry_bare_is_exported_with_the_field_note() {
    let src = format!(
        "{HEADER}\nEnum E {{ Field {{ type = string; default = paid; note = \"States\"; }} draft; paid; }}\n"
    );
    let (model, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
    assert_eq!(model.enums[0].default.as_deref(), Some("paid"));
    assert_eq!(model.enums[0].note.as_deref(), Some("States"));
}

#[test]
fn an_enum_without_default_or_field_note_exports_none() {
    let src = format!(
        "{HEADER}\nEnum E {{ Field {{ type = string; }} Meta {{ note = \"meta only\"; }} draft; }}\nEnum F {{ draft; }}\n"
    );
    let (model, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
    for e in &model.enums {
        assert_eq!((e.default.as_deref(), e.note.as_deref()), (None, None));
    }
}

#[test]
fn an_enum_default_that_names_no_entry_is_rejected_and_exports_none() {
    for default in ["shipped", "\"paid\"", "1", "`paid`"] {
        let src = format!(
            "{HEADER}\nEnum E {{ Field {{ type = string; default = {default}; }} draft; paid; }}\n"
        );
        let (model, diags) = analyze_src(&src);
        assert_eq!(
            messages(&diags),
            ["`Enum.Field.default` must name one of this enum's entries, written bare (docs/08)"],
            "{default}"
        );
        assert_eq!(model.enums[0].default, None, "{default}");
    }
}

#[test]
fn a_qualified_enum_default_is_rejected_with_a_hint_to_write_it_bare() {
    let src = format!(
        "{HEADER}\nEnum OrderStatus {{ Field {{ type = string; default = OrderStatus.paid; }} draft; paid; }}\n"
    );
    let (model, diags) = analyze_src(&src);
    assert_eq!(
        messages(&diags),
        [
            "`Enum.Field.default` must name one of this enum's entries, written bare: `paid`, not `OrderStatus.paid` (docs/08)"
        ]
    );
    assert_eq!(model.enums[0].default, None);
}

#[test]
fn too_many_datatype_arguments_are_rejected() {
    let src = format!(
        "{HEADER}\nDatatype D(a int) {{ Field {{ type = int; }} }}\nTable T {{ x D(1, 2, 3); }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("expects at most 1 argument")),
        "{diags:#?}"
    );
}

#[test]
fn missing_datatype_argument_without_default_is_rejected() {
    let src = format!(
        "{HEADER}\nDatatype D(a int, b int) {{ Field {{ type = int; }} }}\nTable T {{ x D(1); }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("argument for parameter `b` is missing")),
        "{diags:#?}"
    );
}

#[test]
fn wrongly_typed_datatype_argument_is_rejected() {
    let src = format!(
        "{HEADER}\nDatatype D(precision int, scale int) {{ Field {{ type = float; }} }}\nTable T {{ x D(\"abc\", 2); }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("argument for parameter `precision` should be `int`")),
        "{diags:#?}"
    );
}

#[test]
fn a_raw_expression_is_accepted_for_any_datatype_parameter_type() {
    let src = format!(
        "{HEADER}\nDatatype D(a int) {{ Field {{ type = int; }} }}\nTable T {{ x D(`current_setting('x')`); }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn varchar_with_a_non_int_length_is_rejected() {
    let src = format!("{HEADER}\nTable T {{ x varchar(\"eighty\"); }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("argument for parameter `varchar` should be `int`")),
        "{diags:#?}"
    );
}

#[test]
fn varchar_with_too_many_arguments_is_rejected() {
    let src = format!("{HEADER}\nTable T {{ x varchar(80, 3); }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("`varchar` expects at most 1 argument(s), got: 2")),
        "{diags:#?}"
    );
}

#[test]
fn uuid_with_an_argument_is_rejected() {
    let src = format!("{HEADER}\nTable T {{ x uuid(7); }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("`uuid` expects 0 argument(s), got: 1")),
        "{diags:#?}"
    );
}

#[test]
fn varchar_with_a_valid_length_is_accepted() {
    let src = format!("{HEADER}\nTable T {{ x varchar(80); }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn index_on_unknown_column_is_rejected() {
    let src = format!("{HEADER}\nTable T {{ id int [pk]; Index {{ ix_x (nichtvorhanden); }} }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("refers to unknown column")),
        "{diags:#?}"
    );
}

// ===========================================================================
// Multi-file scenarios
// ===========================================================================

#[test]
fn namespace_collision_between_two_imports_is_rejected() {
    let a = "spec 1;\nDocument common { }\n";
    let b = "spec 1;\nDocument common { }\n";
    let main = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; }\nimport \"./a.skema\";\nimport \"./b.skema\";\n";

    let ast_a = skema_syntax::parse(a).0;
    let ast_b = skema_syntax::parse(b).0;
    let ast_main = skema_syntax::parse(main).0;

    let files = [
        InputFile {
            id: FileId(0),
            display_path: "main.skema".to_owned(),
            ast: &ast_main,
            import_targets: vec![Some(FileId(1)), Some(FileId(2))],
        },
        InputFile {
            id: FileId(1),
            display_path: "a.skema".to_owned(),
            ast: &ast_a,
            import_targets: vec![],
        },
        InputFile {
            id: FileId(2),
            display_path: "b.skema".to_owned(),
            ast: &ast_b,
            import_targets: vec![],
        },
    ];

    let (_, _, diags) = analyze(&files);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("namespace `common` is already taken in this file")),
        "{diags:#?}"
    );
}

#[test]
fn unused_import_is_reported_under_strict() {
    let base = "spec 1;\nDocument base { Datatype X { Field { type = int; } } }\n";
    let main = r#"
spec 1;
Document shop { }
Project {
  mode = strict;
  dialect = generic;
  Lint { unusedImport = warn; }
}
import "./base.skema";
Table T { id int [pk]; }
"#;

    let ast_base = skema_syntax::parse(base).0;
    let ast_main = skema_syntax::parse(main).0;

    let files = [
        InputFile {
            id: FileId(0),
            display_path: "main.skema".to_owned(),
            ast: &ast_main,
            import_targets: vec![Some(FileId(1))],
        },
        InputFile {
            id: FileId(1),
            display_path: "base.skema".to_owned(),
            ast: &ast_base,
            import_targets: vec![],
        },
    ];

    let (_, _, diags) = analyze(&files);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("is not used anywhere in this file")),
        "{diags:#?}"
    );
}

#[test]
fn field_naming_is_checked_under_strict() {
    let src = r#"
spec 1;
Document shop { }
Project {
  mode = strict;
  dialect = generic;
  Lint { fieldNaming = snake_case; }
}
Table T { CamelCaseName int; }
"#;
    let (_, diags) = analyze_src(src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("`CamelCaseName` does not follow the configured `fieldNaming`")),
        "{diags:#?}"
    );
}

#[test]
fn a_valid_multi_row_example_is_accepted() {
    let src = format!(
        "{HEADER}\nTable Post {{\n  id int [pk];\n  title varchar(200) [required];\n  body text;\n\n  Example {{\n    Record {{ id = 1; title = \"Hello World\"; body = \"My first post\"; }}\n    Record {{ id = 2; title = \"Second post\"; }}\n  }}\n}}\n"
    );
    let (model, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
    let post = model.tables.iter().find(|t| t.name == "Post").unwrap();
    assert_eq!(post.examples.len(), 2);
    assert_eq!(
        post.examples[0]
            .values
            .get("title")
            .map(|v| format!("{v:?}")),
        Some(r#"Text("Hello World")"#.to_owned())
    );
}

#[test]
fn example_row_with_unknown_field_is_rejected() {
    let src = format!(
        "{HEADER}\nTable T {{ id int [pk]; Example {{ Record {{ id = 1; nickname = \"x\"; }} }} }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("unknown field `nickname`")),
        "{diags:#?}"
    );
}

#[test]
fn example_row_missing_a_required_field_is_rejected() {
    let src = format!(
        "{HEADER}\nTable T {{ id int [pk]; name string [required]; Example {{ Record {{ id = 1; }} }} }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("record 1 in `Example` is missing required field `name`")),
        "{diags:#?}"
    );
}

#[test]
fn example_rows_with_a_duplicate_pk_value_are_rejected() {
    let src = format!(
        "{HEADER}\nTable T {{ id int [pk]; Example {{ Record {{ id = 1; }} Record {{ id = 1; }} }} }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("record 2 in `Example` repeats record 1's value for `id`")),
        "{diags:#?}"
    );
}

#[test]
fn example_rows_with_a_duplicate_unique_value_are_rejected() {
    let src = format!(
        "{HEADER}\nTable T {{ id int [pk]; slug string [unique]; Example {{ Record {{ id = 1; slug = \"a\"; }} Record {{ id = 2; slug = \"a\"; }} }} }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("record 2 in `Example` repeats record 1's value for `slug`")),
        "{diags:#?}"
    );
}

#[test]
fn record_outside_example_is_rejected() {
    let src = format!("{HEADER}\nTable T {{ id int [pk]; Record {{ id = 1; }} }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("`Record` is not allowed here")),
        "{diags:#?}"
    );
}

#[test]
fn example_outside_table_is_rejected() {
    let src = format!("{HEADER}\nExample {{ Record {{ id = 1; }} }}\nTable T {{ id int [pk]; }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("`Example` is not allowed here")),
        "{diags:#?}"
    );
}

#[test]
fn a_use_inlined_partials_fields_are_visible_to_an_example_row() {
    let src = format!(
        "{HEADER}\nPartial P {{ name string; }}\nTable T {{ use P; id int [pk]; Example {{ Record {{ id = 1; name = \"a\"; }} }} }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn example_values_are_checked_against_their_field_type_per_builtin() {
    // (skema type, a literal it accepts, one it doesn't)
    let cases: &[(&str, &str, &str)] = &[
        ("string", r#""hi""#, "42"),
        ("int", "42", r#""hi""#),
        ("float", "4.2", r#""hi""#),
        ("bool", "true", "42"),
        ("varchar(20)", r#""hi""#, "42"),
        ("text", r#""hi""#, "42"),
        ("uuid", r#""11111111-1111-1111-1111-111111111111""#, "42"),
        ("date", r#""2024-01-01""#, "42"),
        ("timestamp", r#""2024-01-01T00:00:00Z""#, "42"),
    ];
    for &(ty, valid, invalid) in cases {
        let accepted = format!(
            "{HEADER}\nTable T {{ id int [pk]; x {ty}; Example {{ Record {{ id = 1; x = {valid}; }} }} }}\n"
        );
        let (_, diags) = analyze_src(&accepted);
        assert!(diags.is_empty(), "`{ty}` accepting `{valid}`: {diags:#?}");

        let rejected = format!(
            "{HEADER}\nTable T {{ id int [pk]; x {ty}; Example {{ Record {{ id = 1; x = {invalid}; }} }} }}\n"
        );
        let (_, diags) = analyze_src(&rejected);
        assert!(
            messages(&diags).iter().any(|m| m.contains(&format!(
                "`x` should be `{}`",
                ty.split('(').next().unwrap()
            ))),
            "`{ty}` rejecting `{invalid}`: {diags:#?}"
        );
    }
}

#[test]
fn example_value_for_a_datatype_typed_field_is_checked_against_its_underlying_type() {
    let src = format!(
        "{HEADER}\nDatatype Email {{ Field {{ type = string; }} }}\nTable T {{ id int [pk]; email Email; Example {{ Record {{ id = 1; email = 42; }} }} }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("`email` should be `string`")),
        "{diags:#?}"
    );
}

#[test]
fn example_value_for_a_datatype_typed_field_accepts_a_matching_value() {
    let src = format!(
        "{HEADER}\nDatatype Email {{ Field {{ type = string; }} }}\nTable T {{ id int [pk]; email Email; Example {{ Record {{ id = 1; email = \"a@b.com\"; }} }} }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn a_raw_expression_is_accepted_for_any_example_value() {
    let src = format!(
        "{HEADER}\nTable T {{ id int [pk]; created timestamp; Example {{ Record {{ id = 1; created = `now()`; }} }} }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
}

// ===========================================================================
// Style property format validation
// ===========================================================================

#[test]
fn style_unquoted_snake_case_key_in_project_is_rejected() {
    let src = format!(
        "{HEADER}\nProject {{ Style @table {{ header_background_color = \"#cc0000\"; }} }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("is not camelCase")),
        "{diags:#?}"
    );
}

#[test]
fn style_unquoted_screaming_snake_case_key_in_document_is_rejected() {
    let src = format!(
        "{HEADER}\nDocument doc {{ Style @table {{ HEADER_BACKGROUND_COLOR = \"#cc0000\"; }} }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("is not camelCase")),
        "{diags:#?}"
    );
}

#[test]
fn style_unquoted_pascal_case_key_in_class_is_rejected() {
    let src = format!(
        "{HEADER}\nProject {{ Style MyClass {{ HeaderBackgroundColor = \"#cc0000\"; }} }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("is not camelCase")),
        "{diags:#?}"
    );
}

#[test]
fn style_unquoted_camel_case_key_in_project_selector_is_allowed() {
    let src = r##"
spec 1;
Document shop { }
Project {
  mode = draft;
  dialect = generic;
  Style @table {
    headerBackgroundColor = "#cc0000";
  }
}
"##;
    let (_, diags) = analyze_src(src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn style_unquoted_camel_case_key_in_class_is_allowed() {
    let src = r##"
spec 1;
Document shop { }
Project {
  mode = draft;
  dialect = generic;
  Style highlight {
    borderColor = "#cc0000";
  }
}
"##;
    let (_, diags) = analyze_src(src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn style_quoted_x_prefixed_key_is_allowed() {
    let src = r#"
spec 1;
Document shop { }
Project {
  mode = draft;
  dialect = generic;
  Style @table {
    "x-mermaidClass" = "important";
  }
}
"#;
    let (_, diags) = analyze_src(src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn style_quoted_key_without_x_prefix_is_rejected() {
    let src =
        format!("{HEADER}\nProject {{ Style @table {{ \"mermaidClass\" = \"important\"; }} }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("must start with `x-`")),
        "{diags:#?}"
    );
}

#[test]
fn style_integer_value_is_rejected() {
    let src = format!("{HEADER}\nProject {{ Style @table {{ borderWidth = 3; }} }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("must be strings, not integer")),
        "{diags:#?}"
    );
}

#[test]
fn style_float_value_is_rejected() {
    let src = format!("{HEADER}\nProject {{ Style @table {{ opacity = 0.5; }} }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("must be strings, not float")),
        "{diags:#?}"
    );
}

#[test]
fn style_path_value_is_rejected() {
    let src = format!("{HEADER}\nProject {{ Style @table {{ borderColor = someColor; }} }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("must be strings, not identifier")),
        "{diags:#?}"
    );
}

#[test]
fn style_raw_value_is_rejected() {
    let src =
        format!("{HEADER}\nProject {{ Style @table {{ borderColor = `rgb(255, 0, 0)`; }} }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("must be strings, not raw string")),
        "{diags:#?}"
    );
}

#[test]
fn style_list_value_is_rejected() {
    let src = format!("{HEADER}\nProject {{ Style @table {{ borderColor = [red, green]; }} }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("must be strings, not list")),
        "{diags:#?}"
    );
}

#[test]
fn style_type_value_is_rejected() {
    let src = format!("{HEADER}\nProject {{ Style @table {{ width = int(5); }} }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("must be strings, not type")),
        "{diags:#?}"
    );
}

#[test]
fn style_string_value_is_allowed() {
    let src = r##"
spec 1;
Document shop { }
Project {
  mode = draft;
  dialect = generic;
  Style @table {
    headerBackgroundColor = "#cc0000";
  }
}
"##;
    let (_, diags) = analyze_src(src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn style_multiple_properties_with_mixed_validity() {
    let src = format!(
        "{HEADER}\nProject {{ Style @table {{ headerBackgroundColor = \"#cc0000\"; header_text_color = 5; }} }}\n"
    );
    let (_, diags) = analyze_src(&src);
    let msgs = messages(&diags);
    assert!(
        msgs.iter().any(|m| m.contains("is not camelCase")),
        "{diags:#?}"
    );
    assert!(
        msgs.iter()
            .any(|m| m.contains("must be strings, not integer")),
        "{diags:#?}"
    );
}

#[test]
fn style_in_document_with_valid_properties() {
    let src = r##"
spec 1;
Document doc {
  Style @table {
    borderColor = "#123456";
  }
}
Project {
  mode = draft;
  dialect = generic;
}
"##;
    let (_, diags) = analyze_src(src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn style_class_with_invalid_key_is_rejected() {
    let src =
        format!("{HEADER}\nProject {{ Style highlight {{ BORDER_COLOR = \"#cc0000\"; }} }}\n");
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("is not camelCase")),
        "{diags:#?}"
    );
}

// ===========================================================================
// requirePrimaryKey / requireForeignKeyIndex / requireTableNote /
// requireFieldNote (task 68)
// ===========================================================================

/// One (Lint key, source triggering the gap, substring of the gap's
/// diagnostic) triple per new rule – shared by the omitted-key, draft-mode
/// and unknown-severity tests below, which behave identically across all
/// four keys.
const REQUIRE_LINT_CASES: &[(&str, &str, &str)] = &[
    (
        "requirePrimaryKey",
        "Table T { name string; }",
        "has no primary key",
    ),
    (
        "requireForeignKeyIndex",
        "Table Customer { id int [pk]; }\nTable Order { id int [pk]; customer >Customer.id; }",
        "has no covering index",
    ),
    (
        "requireTableNote",
        "Table T { id int [pk]; }",
        "has no `Meta.note`",
    ),
    (
        "requireFieldNote",
        "Table T { id int [pk] { note = \"pk\"; } name string; }",
        "has no `note`",
    ),
];

#[test]
fn require_lint_key_omitted_means_no_check() {
    for &(key, decls, gap) in REQUIRE_LINT_CASES {
        let src = format!(
            "spec 1;\nDocument shop {{ }}\nProject {{ mode = strict; dialect = generic; Lint {{ danglingRef = warn; }} }}\n{decls}\n"
        );
        let (_, diags) = analyze_src(&src);
        assert!(
            !messages(&diags).iter().any(|m| m.contains(gap)),
            "`{key}` fired although it was never written in `Lint`: {diags:#?}"
        );
    }
}

#[test]
fn require_lint_key_is_not_checked_under_draft() {
    for &(key, decls, gap) in REQUIRE_LINT_CASES {
        let src = format!(
            "spec 1;\nDocument shop {{ }}\nProject {{ mode = draft; dialect = generic; Lint {{ {key} = warn; }} }}\n{decls}\n"
        );
        let (_, diags) = analyze_src(&src);
        assert!(
            !messages(&diags).iter().any(|m| m.contains(gap)),
            "`{key}` fired under `mode = draft`: {diags:#?}"
        );
    }
}

#[test]
fn require_lint_key_unknown_severity_word_is_rejected() {
    for &(key, decls, gap) in REQUIRE_LINT_CASES {
        let src = format!(
            "spec 1;\nDocument shop {{ }}\nProject {{ mode = strict; dialect = generic; Lint {{ {key} = maybe; }} }}\n{decls}\n"
        );
        let (_, diags) = analyze_src(&src);
        assert!(
            messages(&diags)
                .iter()
                .any(|m| m.contains("unknown severity `maybe`")),
            "`{key}`: {diags:#?}"
        );
        assert!(
            !messages(&diags).iter().any(|m| m.contains(gap)),
            "`{key}` still ran its check after an invalid severity: {diags:#?}"
        );
    }
}

// --- requirePrimaryKey -----------------------------------------------------

#[test]
fn table_without_pk_is_flagged_under_strict() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requirePrimaryKey = warn; } }\nTable T { name string; }\n";
    let (_, diags) = analyze_src(src);
    let hits: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("has no primary key"))
        .collect();
    assert_eq!(hits.len(), 1, "{diags:#?}");
    assert_eq!(hits[0].severity, skema_hir::Severity::Warning);
}

#[test]
fn table_without_pk_severity_error_is_respected() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requirePrimaryKey = error; } }\nTable T { name string; }\n";
    let (_, diags) = analyze_src(src);
    let hits: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("has no primary key"))
        .collect();
    assert_eq!(hits.len(), 1, "{diags:#?}");
    assert_eq!(hits[0].severity, skema_hir::Severity::Error);
}

#[test]
fn table_with_a_pk_is_not_flagged() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requirePrimaryKey = warn; } }\nTable T { id uuid [pk]; }\n";
    let (_, diags) = analyze_src(src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn a_pk_inherited_via_use_partial_counts() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requirePrimaryKey = warn; } }\nPartial P { id int [pk]; }\nTable T { use P; name string; }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        !messages(&diags)
            .iter()
            .any(|m| m.contains("has no primary key")),
        "{diags:#?}"
    );
}

#[test]
fn a_partial_without_pk_is_never_flagged_itself() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requirePrimaryKey = warn; } }\nPartial P { name string; }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        !messages(&diags)
            .iter()
            .any(|m| m.contains("has no primary key")),
        "a `Partial` never becomes a table (docs/09): {diags:#?}"
    );
}

// --- requireForeignKeyIndex -------------------------------------------------

#[test]
fn uncovered_foreign_key_is_flagged_under_strict() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requireForeignKeyIndex = warn; } }\nTable Customer { id int [pk]; }\nTable Order { id int [pk]; customer >Customer.id; }\n";
    let (_, diags) = analyze_src(src);
    let hits: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("has no covering index"))
        .collect();
    assert_eq!(hits.len(), 1, "{diags:#?}");
    assert_eq!(hits[0].severity, skema_hir::Severity::Warning);
}

#[test]
fn uncovered_foreign_key_severity_error_is_respected() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requireForeignKeyIndex = error; } }\nTable Customer { id int [pk]; }\nTable Order { id int [pk]; customer >Customer.id; }\n";
    let (_, diags) = analyze_src(src);
    let hits: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("has no covering index"))
        .collect();
    assert_eq!(hits.len(), 1, "{diags:#?}");
    assert_eq!(hits[0].severity, skema_hir::Severity::Error);
}

#[test]
fn foreign_key_with_own_unique_flag_is_not_flagged() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requireForeignKeyIndex = warn; } }\nTable Customer { id int [pk]; }\nTable Order { id int [pk]; customer >Customer.id [unique]; }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        !messages(&diags)
            .iter()
            .any(|m| m.contains("has no covering index")),
        "{diags:#?}"
    );
}

#[test]
fn foreign_key_with_own_pk_flag_is_not_flagged() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requireForeignKeyIndex = warn; } }\nTable Customer { id int [pk]; }\nTable Order { customer >Customer.id [pk]; }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        !messages(&diags)
            .iter()
            .any(|m| m.contains("has no covering index")),
        "{diags:#?}"
    );
}

#[test]
fn foreign_key_as_the_leading_column_of_a_composite_index_is_not_flagged() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requireForeignKeyIndex = warn; } }\nTable Customer { id int [pk]; }\nTable Order { id int [pk]; customer >Customer.id; opened_on date; Index { (customer, opened_on); } }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        !messages(&diags)
            .iter()
            .any(|m| m.contains("has no covering index")),
        "{diags:#?}"
    );
}

#[test]
fn foreign_key_not_leading_a_composite_index_is_still_flagged() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requireForeignKeyIndex = warn; } }\nTable Customer { id int [pk]; }\nTable Order { id int [pk]; customer >Customer.id; opened_on date; Index { (opened_on, customer); } }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("has no covering index")),
        "{diags:#?}"
    );
}

#[test]
fn backreference_field_is_never_flagged() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requireForeignKeyIndex = warn; } }\nTable Customer { id int [pk]; orders <Order.customer; }\nTable Order { id int [pk]; customer >Customer.id; }\n";
    let (_, diags) = analyze_src(src);
    let msgs = messages(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("`customer` has no covering index")),
        "the `>` side should still be flagged: {diags:#?}"
    );
    assert!(
        !msgs
            .iter()
            .any(|m| m.contains("`orders` has no covering index")),
        "a `<` backreference creates no column (docs/07) and must be exempt: {diags:#?}"
    );
}

#[test]
fn one_to_one_relation_field_is_also_checked() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requireForeignKeyIndex = warn; } }\nTable User { id int [pk]; }\nTable Profile { id int [pk]; user -User.id; }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("`user` has no covering index")),
        "{diags:#?}"
    );
}

// --- requireTableNote --------------------------------------------------------

#[test]
fn table_without_meta_is_flagged_under_strict() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requireTableNote = warn; } }\nTable T { id int [pk]; }\n";
    let (_, diags) = analyze_src(src);
    let hits: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("has no `Meta.note`"))
        .collect();
    assert_eq!(hits.len(), 1, "{diags:#?}");
    assert_eq!(hits[0].severity, skema_hir::Severity::Warning);
}

#[test]
fn table_without_meta_note_severity_error_is_respected() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requireTableNote = error; } }\nTable T { id int [pk]; }\n";
    let (_, diags) = analyze_src(src);
    let hits: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("has no `Meta.note`"))
        .collect();
    assert_eq!(hits.len(), 1, "{diags:#?}");
    assert_eq!(hits[0].severity, skema_hir::Severity::Error);
}

#[test]
fn table_with_meta_but_no_note_key_is_still_flagged() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requireTableNote = warn; } }\nTable T { id int [pk]; Meta { author = \"anna\"; } }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("has no `Meta.note`")),
        "{diags:#?}"
    );
}

#[test]
fn table_with_meta_note_is_not_flagged() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requireTableNote = warn; } }\nTable T { id uuid [pk]; Meta { note = \"what this table is for\"; } }\n";
    let (_, diags) = analyze_src(src);
    assert!(diags.is_empty(), "{diags:#?}");
}

// --- requireFieldNote --------------------------------------------------------

#[test]
fn field_without_note_is_flagged_under_strict() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requireFieldNote = warn; } }\nTable T { id int [pk] { note = \"primary key\"; } name string; }\n";
    let (_, diags) = analyze_src(src);
    let msgs = messages(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("field `name` has no `note`")),
        "{diags:#?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("field `id` has no `note`")),
        "{diags:#?}"
    );
}

#[test]
fn field_without_note_severity_error_is_respected() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requireFieldNote = error; } }\nTable T { id int [pk] { note = \"primary key\"; } name string; }\n";
    let (_, diags) = analyze_src(src);
    let hits: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("field `name` has no `note`"))
        .collect();
    assert_eq!(hits.len(), 1, "{diags:#?}");
    assert_eq!(hits[0].severity, skema_hir::Severity::Error);
}

#[test]
fn field_with_note_is_not_flagged() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requireFieldNote = warn; } }\nTable T { id uuid [pk] { note = \"primary key\"; } name varchar(80) { note = \"the customer's name\"; } }\n";
    let (_, diags) = analyze_src(src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn partial_field_without_note_is_flagged_even_when_unused_by_any_table() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requireFieldNote = warn; } }\nPartial P { description string; }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("field `description` has no `note`")),
        "a `Partial`'s own fields are checked directly, not only through a \
         `Table` that `use`s it: {diags:#?}"
    );
}

#[test]
fn enum_is_unaffected_by_either_note_rule() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requireTableNote = warn; requireFieldNote = warn; } }\nEnum Status { Field { type = string; } draft; }\nTable T { id uuid [pk] { note = \"pk\"; } status Status { note = \"current status\"; } Meta { note = \"a table\"; } }\n";
    let (_, diags) = analyze_src(src);
    assert!(diags.is_empty(), "{diags:#?}");
}

// --- required-FK cycles (always-on, not `Lint`) -----------------------------

#[test]
fn two_table_required_cycle_is_rejected_under_draft_with_no_lint_block() {
    let src = format!(
        "{HEADER}\nTable A {{ id int [pk]; b >B.id [required]; }}\nTable B {{ id int [pk]; a >A.id [required]; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    let msgs = messages(&diags);
    let hits: Vec<&&str> = msgs
        .iter()
        .filter(|m| m.contains("closes a cycle among tables: A, B"))
        .collect();
    assert_eq!(
        hits.len(),
        2,
        "one diagnostic per field that closes the cycle, not gated by `Lint`/`mode`: {diags:#?}"
    );
}

#[test]
fn two_table_required_cycle_is_rejected_under_strict_with_lint_block() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { unusedImport = warn; } }\nTable A { id int [pk]; b >B.id [required]; }\nTable B { id int [pk]; a >A.id [required]; }\n";
    let (_, diags) = analyze_src(src);
    let hits = messages(&diags)
        .iter()
        .filter(|m| m.contains("closes a cycle among tables: A, B"))
        .count();
    assert_eq!(hits, 2, "{diags:#?}");
}

#[test]
fn three_table_required_cycle_is_rejected() {
    let src = format!(
        "{HEADER}\nTable A {{ id int [pk]; b >B.id [required]; }}\nTable B {{ id int [pk]; c >C.id [required]; }}\nTable C {{ id int [pk]; a >A.id [required]; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    let hits = messages(&diags)
        .iter()
        .filter(|m| m.contains("closes a cycle among tables: A, B, C"))
        .count();
    assert_eq!(hits, 3, "{diags:#?}");
}

#[test]
fn self_referencing_required_foreign_key_is_rejected() {
    let src =
        format!("{HEADER}\nTable Employee {{ id int [pk]; manager >Employee.id [required]; }}\n");
    let (_, diags) = analyze_src(&src);
    let hits = messages(&diags)
        .iter()
        .filter(|m| m.contains("closes a cycle among tables: Employee"))
        .count();
    assert_eq!(hits, 1, "{diags:#?}");
}

#[test]
fn pk_flag_foreign_key_counts_as_required() {
    // `b` carries `[pk]`, not `[required]` – `pk` implies `required` (docs/05,
    // "pk implies required and unique") and must close the cycle just the same.
    let src = format!(
        "{HEADER}\nTable A {{ id int [pk]; b >B.id [pk]; }}\nTable B {{ id int [pk]; a >A.id [required]; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    let msgs = messages(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("foreign key `b`") && m.contains("A, B")),
        "{diags:#?}"
    );
    assert!(
        msgs.iter()
            .any(|m| m.contains("foreign key `a`") && m.contains("A, B")),
        "{diags:#?}"
    );
}

#[test]
fn noncircular_required_chain_is_not_flagged() {
    let src = format!(
        "{HEADER}\nTable A {{ id int [pk]; b >B.id [required]; }}\nTable B {{ id int [pk]; c >C.id [required]; }}\nTable C {{ id int [pk]; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn optional_link_breaks_the_cycle() {
    let src = format!(
        "{HEADER}\nTable A {{ id int [pk]; b >B.id [required]; }}\nTable B {{ id int [pk]; a >A.id; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
}

#[test]
fn back_reference_does_not_create_a_cycle_edge() {
    let src = format!(
        "{HEADER}\nTable A {{ id int [pk]; b >B.id [required]; }}\nTable B {{ id int [pk]; bs <A.b; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
}

// --- Naming-rule severity override ------------------------------------------

/// `(Lint key, declaration with one violating name, message fragment)` – a
/// bad `CamelCaseName` against a `snake_case` rule, shared by every test
/// below that behaves identically across all three naming rules.
const NAMING_LINT_CASES: &[(&str, &str, &str)] = &[
    (
        "fieldNaming",
        "Table T { id uuid [pk]; CamelCaseName text; }",
        "`CamelCaseName` does not follow the configured `fieldNaming`",
    ),
    (
        "tableNaming",
        "Table CamelCaseName { id uuid [pk]; }",
        "`CamelCaseName` does not follow the configured `tableNaming`",
    ),
    (
        "enumNaming",
        "Enum CamelCaseName { Field { type = string; } draft; }",
        "`CamelCaseName` does not follow the configured `enumNaming`",
    ),
];

#[test]
fn naming_rule_violation_defaults_to_warning() {
    for &(key, decls, gap) in NAMING_LINT_CASES {
        let src = format!(
            "spec 1;\nDocument shop {{ }}\nProject {{ mode = strict; dialect = generic; Lint {{ {key} = snake_case; }} }}\n{decls}\n"
        );
        let (_, diags) = analyze_src(&src);
        let hits: Vec<_> = diags.iter().filter(|d| d.message.contains(gap)).collect();
        assert_eq!(hits.len(), 1, "`{key}`: {diags:#?}");
        assert_eq!(hits[0].severity, skema_hir::Severity::Warning);
    }
}

#[test]
fn naming_rule_explicit_warn_override_stays_a_warning() {
    for &(key, decls, gap) in NAMING_LINT_CASES {
        let src = format!(
            "spec 1;\nDocument shop {{ }}\nProject {{ mode = strict; dialect = generic; Lint {{ {key} = snake_case [warn]; }} }}\n{decls}\n"
        );
        let (_, diags) = analyze_src(&src);
        let hits: Vec<_> = diags.iter().filter(|d| d.message.contains(gap)).collect();
        assert_eq!(hits.len(), 1, "`{key}`: {diags:#?}");
        assert_eq!(hits[0].severity, skema_hir::Severity::Warning);
    }
}

#[test]
fn naming_rule_error_override_is_respected() {
    for &(key, decls, gap) in NAMING_LINT_CASES {
        let src = format!(
            "spec 1;\nDocument shop {{ }}\nProject {{ mode = strict; dialect = generic; Lint {{ {key} = snake_case [error]; }} }}\n{decls}\n"
        );
        let (_, diags) = analyze_src(&src);
        let hits: Vec<_> = diags.iter().filter(|d| d.message.contains(gap)).collect();
        assert_eq!(hits.len(), 1, "`{key}`: {diags:#?}");
        assert_eq!(hits[0].severity, skema_hir::Severity::Error);
    }
}

#[test]
fn naming_rule_unknown_severity_word_is_rejected() {
    for &(key, decls, gap) in NAMING_LINT_CASES {
        let src = format!(
            "spec 1;\nDocument shop {{ }}\nProject {{ mode = strict; dialect = generic; Lint {{ {key} = snake_case [oops]; }} }}\n{decls}\n"
        );
        let (_, diags) = analyze_src(&src);
        assert!(
            messages(&diags)
                .iter()
                .any(|m| m.contains("unknown severity `oops`")),
            "`{key}`: {diags:#?}"
        );
        assert!(
            !messages(&diags).iter().any(|m| m.contains(gap)),
            "`{key}` still ran its check after an invalid severity: {diags:#?}"
        );
        assert_eq!(
            diags.len(),
            1,
            "`{key}` should yield exactly one diagnostic: {diags:#?}"
        );
    }
}

#[test]
fn severity_override_on_unused_import_is_rejected() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { unusedImport = warn [error]; } }\nTable T { id uuid [pk]; }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        messages(&diags).iter().any(|m| m.contains(
            "severity override `[error]` is only valid on `fieldNaming`/`tableNaming`/`enumNaming`"
        )),
        "{diags:#?}"
    );
    assert_eq!(diags.len(), 1, "{diags:#?}");
}

#[test]
fn severity_override_on_require_lint_key_is_rejected() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; Lint { requirePrimaryKey = warn [error]; } }\nTable T { id uuid [pk]; }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        messages(&diags).iter().any(|m| m.contains(
            "severity override `[error]` is only valid on `fieldNaming`/`tableNaming`/`enumNaming`"
        )),
        "{diags:#?}"
    );
    assert_eq!(diags.len(), 1, "{diags:#?}");
}

#[test]
fn severity_override_on_meta_setting_is_rejected() {
    for mode in ["draft", "strict"] {
        let src = format!(
            "spec 1;\nDocument shop {{ }}\nProject {{ mode = {mode}; dialect = generic; }}\nTable T {{ id uuid [pk]; Meta {{ author = \"anna\" [error]; }} }}\n"
        );
        let (_, diags) = analyze_src(&src);
        assert!(
            messages(&diags).iter().any(|m| m.contains(
                "severity override `[error]` is only valid on `fieldNaming`/`tableNaming`/`enumNaming`"
            )),
            "mode={mode}: {diags:#?}"
        );
        assert_eq!(diags.len(), 1, "mode={mode}: {diags:#?}");
    }
}

#[test]
fn severity_override_on_database_setting_is_rejected() {
    for mode in ["draft", "strict"] {
        let src = format!(
            "spec 1;\nDocument shop {{ }}\nProject {{ mode = {mode}; dialect = generic; }}\nTable T {{ id uuid [pk]; Database {{ table = \"t\" [error]; }} }}\n"
        );
        let (_, diags) = analyze_src(&src);
        assert!(
            messages(&diags).iter().any(|m| m.contains(
                "severity override `[error]` is only valid on `fieldNaming`/`tableNaming`/`enumNaming`"
            )),
            "mode={mode}: {diags:#?}"
        );
        assert_eq!(diags.len(), 1, "mode={mode}: {diags:#?}");
    }
}

// ===========================================================================
// Reserved words & identifier length (task 82)
// ===========================================================================

#[test]
fn reserved_physical_table_name_is_flagged_under_strict() {
    // `order` is reserved in all three dialects (docs/05's own example).
    for dialect in ["postgres", "mysql", "oracledb"] {
        let src = format!(
            "spec 1;\nDocument shop {{ }}\nProject {{ mode = strict; dialect = {dialect}; }}\nTable Order {{ id uuid [pk]; Database {{ table = \"order\"; }} }}\n"
        );
        let (_, diags) = analyze_src(&src);
        assert!(
            messages(&diags)
                .iter()
                .any(|m| m.contains("`order` is a reserved word in")
                    && m.contains(dialect)
                    && m.contains("table")),
            "{dialect}: {diags:#?}"
        );
    }
}

#[test]
fn reserved_physical_table_name_is_silent_under_draft_and_generic() {
    let draft = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = postgres; }\nTable Order { id uuid [pk]; Database { table = \"order\"; } }\n";
    let (_, diags) = analyze_src(draft);
    assert!(
        !messages(&diags).iter().any(|m| m.contains("reserved word")),
        "draft: {diags:#?}"
    );

    let generic = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = generic; }\nTable Order { id uuid [pk]; Database { table = \"order\"; } }\n";
    let (_, diags) = analyze_src(generic);
    assert!(
        !messages(&diags).iter().any(|m| m.contains("reserved word")),
        "generic: {diags:#?}"
    );
}

#[test]
fn reserved_physical_column_name_is_flagged_under_strict() {
    // `select` is reserved in all three dialects.
    for dialect in ["postgres", "mysql", "oracledb"] {
        let src = format!(
            "spec 1;\nDocument shop {{ }}\nProject {{ mode = strict; dialect = {dialect}; }}\nTable T {{ id uuid [pk]; select varchar(80); }}\n"
        );
        let (_, diags) = analyze_src(&src);
        assert!(
            messages(&diags)
                .iter()
                .any(|m| m.contains("`select` is a reserved word in")
                    && m.contains(dialect)
                    && m.contains("column")),
            "{dialect}: {diags:#?}"
        );
    }
}

#[test]
fn too_long_physical_column_name_is_flagged_under_strict() {
    // One byte over each dialect's limit (oracledb defaults to `standard`, 30).
    for (dialect, len) in [("postgres", 64), ("mysql", 65), ("oracledb", 31)] {
        let name = "a".repeat(len);
        let src = format!(
            "spec 1;\nDocument shop {{ }}\nProject {{ mode = strict; dialect = {dialect}; }}\nTable T {{ id uuid [pk]; {name} varchar(80); }}\n"
        );
        let (_, diags) = analyze_src(&src);
        assert!(
            messages(&diags)
                .iter()
                .any(|m| m.contains("byte identifier limit")),
            "{dialect} at {len} bytes: {diags:#?}"
        );
    }
}

#[test]
fn physical_column_name_at_the_dialect_limit_is_not_flagged() {
    // Exactly at each dialect's limit – no diagnostic.
    for (dialect, len) in [("postgres", 63), ("mysql", 64), ("oracledb", 30)] {
        let name = "a".repeat(len);
        let src = format!(
            "spec 1;\nDocument shop {{ }}\nProject {{ mode = strict; dialect = {dialect}; }}\nTable T {{ id uuid [pk]; {name} varchar(80); }}\n"
        );
        let (_, diags) = analyze_src(&src);
        assert!(
            !messages(&diags)
                .iter()
                .any(|m| m.contains("byte identifier limit")),
            "{dialect} at {len} bytes: {diags:#?}"
        );
    }
}

#[test]
fn physical_table_name_check_uses_the_database_override_not_the_declared_name() {
    // Safe declared name, colliding override -> flagged (a safe business
    // name given a colliding override must not slip through).
    let collides = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = postgres; }\nTable Purchase { id uuid [pk]; Database { table = \"order\"; } }\n";
    let (_, diags) = analyze_src(collides);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("`order` is a reserved word")),
        "{diags:#?}"
    );

    // Colliding declared name, safe override -> not flagged (checking the
    // business name instead would false-positive here).
    let safe = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = postgres; }\nTable Order { id uuid [pk]; Database { table = \"purchase_orders\"; } }\n";
    let (_, diags) = analyze_src(safe);
    assert!(
        !messages(&diags).iter().any(|m| m.contains("reserved word")),
        "{diags:#?}"
    );
}

#[test]
fn physical_column_name_check_uses_the_column_flag_not_the_declared_name() {
    let collides = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = postgres; }\nTable T { id uuid [pk]; status varchar(80) [column=\"select\"]; }\n";
    let (_, diags) = analyze_src(collides);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("`select` is a reserved word")),
        "{diags:#?}"
    );

    let safe = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = postgres; }\nTable T { id uuid [pk]; select varchar(80) [column=\"status\"]; }\n";
    let (_, diags) = analyze_src(safe);
    assert!(
        !messages(&diags).iter().any(|m| m.contains("reserved word")),
        "{diags:#?}"
    );
}

#[test]
fn reserved_schema_name_is_flagged_under_strict() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = postgres; Database { schema = \"select\"; } }\nTable T { id uuid [pk]; }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("`select` is a reserved word in postgres") && m.contains("schema")),
        "{diags:#?}"
    );
}

#[test]
fn reserved_database_name_is_flagged_under_strict() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = postgres; Database { database = \"select\"; } }\nTable T { id uuid [pk]; }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        messages(&diags).iter().any(
            |m| m.contains("`select` is a reserved word in postgres") && m.contains("database")
        ),
        "{diags:#?}"
    );
}

#[test]
fn multibyte_physical_column_name_is_counted_in_bytes_not_characters() {
    // 32 characters, each `é` two UTF-8 bytes -> 64 bytes, one over
    // postgres's 63-byte limit despite being well under it in characters.
    let name = "é".repeat(32);
    let src = format!(
        "spec 1;\nDocument shop {{ }}\nProject {{ mode = strict; dialect = postgres; }}\nTable T {{ id uuid [pk]; note varchar(80) [column=\"{name}\"]; }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("64 bytes long") && m.contains("63-byte identifier limit")),
        "{diags:#?}"
    );
}

#[test]
fn oracledb_default_identifier_length_is_standard_30_bytes() {
    let name = "a".repeat(31);
    let src = format!(
        "spec 1;\nDocument shop {{ }}\nProject {{ mode = strict; dialect = oracledb; }}\nTable T {{ id uuid [pk]; {name} varchar(80); }}\n"
    );
    let (_, diags) = analyze_src(&src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("30-byte identifier limit")),
        "{diags:#?}"
    );
}

#[test]
fn identifier_length_extended_raises_oracledbs_limit_to_128_bytes() {
    let name_31 = "a".repeat(31);
    let under_limit = format!(
        "spec 1;\nDocument shop {{ }}\nProject {{ mode = strict; dialect = oracledb; Database {{ Dialect @oracledb {{ identifierLength = \"extended\"; }} }} }}\nTable T {{ id uuid [pk]; {name_31} varchar(80); }}\n"
    );
    let (_, diags) = analyze_src(&under_limit);
    assert!(
        !messages(&diags)
            .iter()
            .any(|m| m.contains("byte identifier limit")),
        "31 bytes under extended: {diags:#?}"
    );

    let name_129 = "a".repeat(129);
    let over_limit = format!(
        "spec 1;\nDocument shop {{ }}\nProject {{ mode = strict; dialect = oracledb; Database {{ Dialect @oracledb {{ identifierLength = \"extended\"; }} }} }}\nTable T {{ id uuid [pk]; {name_129} varchar(80); }}\n"
    );
    let (_, diags) = analyze_src(&over_limit);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("128-byte identifier limit")),
        "129 bytes under extended: {diags:#?}"
    );
}

#[test]
fn unknown_identifier_length_value_is_rejected() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = oracledb; Database { Dialect @oracledb { identifierLength = \"medium\"; } } }\nTable T { id uuid [pk]; }\n";
    let (_, diags) = analyze_src(src);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("unknown `identifierLength` value `medium`")),
        "{diags:#?}"
    );
}

#[test]
fn identifier_length_outside_projects_own_database_is_rejected() {
    let document_level = "spec 1;\nDocument shop { Database { Dialect @oracledb { identifierLength = \"extended\"; } } }\nProject { mode = strict; dialect = oracledb; }\nTable T { id uuid [pk]; }\n";
    let (_, diags) = analyze_src(document_level);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("only legal in `Project`'s own `Database` block")),
        "document-level: {diags:#?}"
    );

    let table_level = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = oracledb; }\nTable T { id uuid [pk]; Database { Dialect @oracledb { identifierLength = \"extended\"; } } }\n";
    let (_, diags) = analyze_src(table_level);
    assert!(
        messages(&diags)
            .iter()
            .any(|m| m.contains("only legal in `Project`'s own `Database` block")),
        "table-level: {diags:#?}"
    );
}

// ===========================================================================
// Datatype declarations and type targets (task 114)
// ===========================================================================

// clippy.toml's test allowance covers `#[test]` fns, not their helpers.
#[allow(clippy::panic)]
fn datatype<'m>(model: &'m Model, name: &str) -> &'m skema_hir::model::Datatype {
    model
        .datatypes
        .iter()
        .find(|d| d.name == name)
        .unwrap_or_else(|| panic!("no Datatype `{name}` in {:#?}", model.datatypes))
}

fn decl_ref(namespace: &str, name: &str) -> Option<skema_hir::model::DeclRef> {
    Some(skema_hir::model::DeclRef {
        namespace: namespace.to_owned(),
        name: name.to_owned(),
    })
}

fn texts(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[test]
fn a_datatype_on_a_builtin_exports_its_field_block_and_params_as_written() {
    let src = format!(
        r#"{HEADER}
Datatype regexstring(pattern string, flags string) {{
  pattern {{ note = "Pattern the value must match"; }}
  flags   {{ default = "i"; }}
  Field {{ type = string; regex = "^{{pattern}}$"; default = "none"; note = "Matched text"; }}
}}
"#
    );
    let (model, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
    let regexstring = datatype(&model, "regexstring");
    assert_eq!(regexstring.namespace, "shop");
    assert_eq!(regexstring.base_type.as_deref(), Some("string"));
    assert!(regexstring.base_type_args.is_empty());
    assert_eq!(regexstring.base_datatype, None);
    assert_eq!(regexstring.resolved_base_type.as_deref(), Some("string"));
    assert_eq!(regexstring.regex.as_deref(), Some("^{pattern}$"));
    assert_eq!(regexstring.default.as_deref(), Some("none"));
    assert_eq!(regexstring.note.as_deref(), Some("Matched text"));
    let params: Vec<_> = regexstring
        .params
        .iter()
        .map(|p| {
            (
                p.name.as_str(),
                p.ty.as_deref(),
                p.default.as_deref(),
                p.note.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        params,
        [
            (
                "pattern",
                Some("string"),
                None,
                Some("Pattern the value must match")
            ),
            ("flags", Some("string"), Some("i"), None),
        ]
    );
}

#[test]
fn a_datatype_on_an_engine_catalog_type_resolves_to_it_under_its_dialect() {
    let src = r#"spec 1;
Document shop { }
Project { mode = draft; dialect = postgres; }
Datatype Price { Field { type = numeric(10, 2); } Dialect @postgres { sql = "numeric(10, 2)"; } }
"#;
    let (model, diags) = analyze_src(src);
    assert!(diags.is_empty(), "{diags:#?}");
    let price = datatype(&model, "Price");
    assert_eq!(price.base_type.as_deref(), Some("numeric"));
    assert_eq!(price.base_type_args, texts(&["10", "2"]));
    assert_eq!(price.base_datatype, None);
    assert_eq!(price.resolved_base_type.as_deref(), Some("numeric"));
}

#[test]
fn a_layered_datatype_resolves_through_an_import_alias_to_its_terminal_type() {
    let imported = r#"spec 1;
Document types { }
Datatype decimal(precision int, scale int) {
  scale { default = 2; }
  Field { type = float; }
}
Datatype Cents { Field { type = decimal(12, 0); } }
"#;
    let main = r#"spec 1;
Document shop { }
Project { mode = draft; dialect = generic; }
import "./a.skema" as types;
Datatype Money { Field { type = types.decimal(10); } }
Datatype Fee   { Field { type = types.Cents; } }
Datatype Total { Field { type = Money; } }
"#;
    let (model, diags) = analyze_main_importing_model(main, imported);
    assert!(diags.is_empty(), "{diags:#?}");

    let money = datatype(&model, "Money");
    assert_eq!(money.base_type.as_deref(), Some("types.decimal"));
    assert_eq!(money.base_type_args, texts(&["10"]));
    assert_eq!(money.base_datatype, decl_ref("types", "decimal"));
    assert_eq!(money.resolved_base_type.as_deref(), Some("float"));

    // `Cents`' own `decimal` resolves in its own file, where it is local.
    let fee = datatype(&model, "Fee");
    assert_eq!(fee.base_datatype, decl_ref("types", "Cents"));
    assert_eq!(fee.resolved_base_type.as_deref(), Some("float"));

    let total = datatype(&model, "Total");
    assert_eq!(total.base_datatype, decl_ref("shop", "Money"));
    assert_eq!(total.resolved_base_type.as_deref(), Some("float"));

    let order: Vec<(&str, &str)> = model
        .datatypes
        .iter()
        .map(|d| (d.namespace.as_str(), d.name.as_str()))
        .collect();
    assert_eq!(
        order,
        [
            ("shop", "Fee"),
            ("shop", "Money"),
            ("shop", "Total"),
            ("types", "Cents"),
            ("types", "decimal"),
        ]
    );
}

#[test]
fn a_base_datatype_names_the_declaring_document_not_the_import_alias() {
    let imported =
        "spec 1;\nDocument numeric { }\nDatatype decimal(p int) { Field { type = float; } }\n";
    let main = "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; }\nimport \"./a.skema\" as num;\nDatatype Money { Field { type = num.decimal(10); } }\nTable T { id int [pk]; amount num.decimal(8); }\n";
    let (model, diags) = analyze_main_importing_model(main, imported);
    assert!(diags.is_empty(), "{diags:#?}");
    let money = datatype(&model, "Money");
    assert_eq!(money.base_type.as_deref(), Some("num.decimal"));
    assert_eq!(money.base_datatype, decl_ref("numeric", "decimal"));
    let amount = &model.tables[0].fields[1].ty;
    assert_eq!(amount.name, "num.decimal");
    assert_eq!(amount.datatype, decl_ref("numeric", "decimal"));
    assert_eq!(amount.enum_, None);
}

#[test]
fn an_unknown_or_enum_base_exports_a_null_resolved_type_without_a_diagnostic() {
    let src = format!(
        r#"{HEADER}
Enum Status {{ active; }}
Datatype Unknown {{ Field {{ type = nope; }} }}
Datatype OnEnum  {{ Field {{ type = Status; }} }}
Datatype NoField {{ Meta {{ note = "n"; }} }}
"#
    );
    let (model, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
    let unknown = datatype(&model, "Unknown");
    assert_eq!(unknown.base_type.as_deref(), Some("nope"));
    assert_eq!(unknown.base_datatype, None);
    assert_eq!(unknown.resolved_base_type, None);
    let on_enum = datatype(&model, "OnEnum");
    assert_eq!(on_enum.base_type.as_deref(), Some("Status"));
    assert_eq!(on_enum.base_datatype, None);
    assert_eq!(on_enum.resolved_base_type, None);
    let no_field = datatype(&model, "NoField");
    assert_eq!(no_field.base_type, None);
    assert_eq!(no_field.resolved_base_type, None);
}

#[test]
fn a_datatype_cycle_is_an_error_at_each_members_field_type() {
    let src = format!(
        r#"{HEADER}
Datatype A {{ Field {{ type = B; }} }}
Datatype B {{ Field {{ type = A(1); }} }}
Datatype C {{ Field {{ type = A; }} }}
Datatype Self {{ Field {{ type = shop.Self; }} }}
"#
    );
    let (model, diags) = analyze_src(&src);
    let cycles = diagnostics_containing(&diags, "cycle in `Field.type`");
    let at: Vec<&str> = cycles
        .iter()
        .map(|d| &src[d.span.start as usize..d.span.end as usize])
        .collect();
    assert_eq!(at, ["B", "A(1)", "shop.Self"], "{diags:#?}");
    assert!(
        cycles
            .iter()
            .all(|d| d.severity == skema_hir::Severity::Error)
    );
    for name in ["A", "B", "C", "Self"] {
        assert_eq!(datatype(&model, name).resolved_base_type, None, "{name}");
    }
    assert_eq!(datatype(&model, "C").base_datatype, decl_ref("shop", "A"));
}

#[test]
fn a_datatypes_field_block_wins_over_its_meta_on_the_same_x_key() {
    let src = format!(
        r#"{HEADER}
Datatype D(size int) {{
  size {{ note = "n"; "x-param" = "ignored"; }}
  Meta {{ note = "m"; "x-source" = "meta"; "x-icon" = "coin"; }}
  Field {{ type = int; "x-source" = "field"; }}
}}
"#
    );
    let (model, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
    assert_eq!(
        datatype(&model, "D").extensions,
        extensions(&[("x-icon", "coin"), ("x-source", "field")])
    );
}

#[test]
fn a_field_type_points_at_its_enum_or_datatype_and_other_types_at_neither() {
    let src = format!(
        r#"{HEADER}
Enum Status {{ active; }}
Datatype Email {{ Field {{ type = string; }} }}
Table Customer {{
  id     int [pk];
  email  Email;
  status shop.Status;
  owner  >Customer.id;
}}
"#
    );
    let (model, diags) = analyze_src(&src);
    assert!(diags.is_empty(), "{diags:#?}");
    let types: Vec<_> = model.tables[0]
        .fields
        .iter()
        .map(|f| (f.name.as_str(), f.ty.datatype.clone(), f.ty.enum_.clone()))
        .collect();
    assert_eq!(
        types,
        [
            ("id", None, None),
            ("email", decl_ref("shop", "Email"), None),
            ("status", None, decl_ref("shop", "Status")),
            ("owner", None, None),
        ]
    );
}

/// Pins today's behaviour (`resolve_named_type` checks the catalog first),
/// not a decision: postgres has `jsonb` in its catalog, but not `json`.
#[test]
fn a_bare_engine_catalog_name_shadows_a_same_named_datatype() {
    let src = r#"spec 1;
Document shop { }
Project { mode = draft; dialect = postgres; }
Datatype jsonb   { Field { type = text; } Dialect @postgres { sql = "jsonb"; } }
Datatype json    { Field { type = text; } Dialect @postgres { sql = "json"; } }
Datatype Payload { Field { type = jsonb; } Dialect @postgres { sql = "jsonb"; } }
Table T { id int [pk]; data jsonb; doc json; }
"#;
    let (model, diags) = analyze_src(src);
    assert!(diags.is_empty(), "{diags:#?}");
    let fields = &model.tables[0].fields;
    assert_eq!(fields[1].ty.datatype, None);
    assert_eq!(fields[2].ty.datatype, decl_ref("shop", "json"));
    let payload = datatype(&model, "Payload");
    assert_eq!(payload.base_datatype, None);
    assert_eq!(payload.resolved_base_type.as_deref(), Some("jsonb"));
}

#[test]
fn example_values_follow_layered_datatypes_to_their_builtin() {
    let src = format!(
        r#"{HEADER}
Datatype decimal(precision int) {{ Field {{ type = float; }} }}
Datatype Money {{ Field {{ type = decimal(10); }} }}
Datatype Price {{ Field {{ type = Money; }} }}
Table T {{ id int [pk]; price Price; Example {{ Record {{ id = 1; price = "cheap"; }} Record {{ id = 2; price = 9.5; }} }} }}
"#
    );
    let (_, diags) = analyze_src(&src);
    let wrong = diagnostics_containing(&diags, "`price` should be `float`");
    assert_eq!(wrong.len(), 1, "{diags:#?}");
    assert_eq!(diags.len(), 1, "{diags:#?}");
}
