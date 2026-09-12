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
    analyze(&[file])
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
fn unknown_type_under_postgres_is_only_a_warning() {
    let src = "spec 1;\nDocument shop { }\nProject { mode = strict; dialect = postgres; }\nTable T { x jsonb; }\n";
    let (_, diags) = analyze_src(src);
    let jsonb: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("jsonb"))
        .collect();
    assert_eq!(jsonb.len(), 1, "{diags:#?}");
    assert_eq!(jsonb[0].severity, skema_hir::Severity::Warning);
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
            .any(|m| m.contains("`varchar` expects 1 argument(s), got: 2")),
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

    let (_, diags) = analyze(&files);
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

    let (_, diags) = analyze(&files);
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
