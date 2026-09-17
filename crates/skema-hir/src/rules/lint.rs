//! `Lint`: naming conventions and the opt-in modeling-quality rules
//! (`unusedImport`, `requirePrimaryKey`, `requireForeignKeyIndex`,
//! `requireTableNote`, `requireFieldNote`) – all only in effect under
//! `mode = strict` (`docs/04`).
//!
//! A rule only applies when it's **written in the `Lint` block** – "makes
//! conventions explicit … instead of leaving them as unwritten tool
//! defaults" (`docs/04`). An unwritten `fieldNaming` therefore means: no
//! check, not "some default style".

use std::collections::HashSet;

use skema_syntax::Severity;
use skema_syntax::ast::{self, BlockKind, ValueKind};
use smol_str::SmolStr;

use super::project::read_word;
use super::{Ctx, Mode, ProjectSettings};
use crate::model;
use crate::{Diagnostic, FileId};

pub(crate) fn check(
    ctx: &Ctx,
    project: Option<&ProjectSettings>,
    tables: &[model::Table],
    diags: &mut Vec<Diagnostic>,
) {
    let Some(project) = project else { return };
    if project.mode != Mode::Strict {
        return;
    }
    let Some(lint) = &project.lint else { return };

    check_naming(
        ctx,
        project.file,
        lint,
        "tableNaming",
        BlockKind::Table,
        diags,
    );
    check_naming(
        ctx,
        project.file,
        lint,
        "enumNaming",
        BlockKind::Enum,
        diags,
    );
    check_field_naming(ctx, project.file, lint, diags);
    check_unused_imports(ctx, project.file, lint, diags);
    check_require_primary_key(project.file, lint, tables, diags);
    check_require_foreign_key_index(project.file, lint, tables, diags);
    check_require_table_note(ctx, project.file, lint, diags);
    check_require_field_note(ctx, project.file, lint, diags);
}

/// Reads a `key = warn|error;` severity setting, rejecting any other word
/// the same way `unusedImport` already does – shared by every opt-in
/// boolean `Lint` rule (`unusedImport`, `danglingRef`, and the
/// `require*`/note rules below).
fn read_severity(
    project_file: FileId,
    lint: &ast::Block,
    key: &str,
    diags: &mut Vec<Diagnostic>,
) -> Option<Severity> {
    let (word, span) = read_word(lint, key)?;
    let severity = parse_severity(&word);
    if severity.is_none() {
        diags.push(Diagnostic::error(
            project_file,
            span,
            format!("unknown severity `{word}` – allowed: warn, error"),
        ));
    }
    severity
}

fn parse_severity(word: &str) -> Option<Severity> {
    match word {
        "warn" => Some(Severity::Warning),
        "error" => Some(Severity::Error),
        _ => None,
    }
}

/// The `[word]` after `key = value` in `lint`, if written – `rules::keys`
/// already confirmed placement (`fieldNaming`/`tableNaming`/`enumNaming`
/// only), so here it's just read and parsed.
fn read_severity_override(lint: &ast::Block, key: &str) -> Option<ast::Ident> {
    lint.items().find_map(|item| match item.kind() {
        ast::ItemKind::Setting(a) if a.key().text() == key => a.severity_override(),
        _ => None,
    })
}

// ===========================================================================
// Naming conventions
// ===========================================================================

#[derive(Clone, Copy)]
pub(crate) enum Case {
    Snake,
    Camel,
    Pascal,
    ScreamingSnake,
    Any,
}

fn parse_case(word: &str) -> Option<Case> {
    match word {
        "snake_case" => Some(Case::Snake),
        "camelCase" => Some(Case::Camel),
        "PascalCase" => Some(Case::Pascal),
        "SCREAMING_SNAKE_CASE" => Some(Case::ScreamingSnake),
        "any" => Some(Case::Any),
        _ => None,
    }
}

pub(crate) fn matches_case(name: &str, case: Case) -> bool {
    let Some(first) = name.chars().next() else {
        return true;
    };
    match case {
        Case::Any => true,
        Case::Snake => name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
        Case::ScreamingSnake => name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
        Case::Pascal => first.is_ascii_uppercase() && !name.contains('_'),
        Case::Camel => first.is_ascii_lowercase() && !name.contains('_'),
    }
}

/// Reads `key = case_style [warn|error];` – the style, and the severity to
/// report a violation at (`Warning` when no bracket is written, unchanged
/// from before the override existed).
fn read_case(
    project_file: FileId,
    lint: &ast::Block,
    key: &str,
    diags: &mut Vec<Diagnostic>,
) -> Option<(Case, Severity)> {
    let (word, span) = read_word(lint, key)?;
    let case = parse_case(&word);
    if case.is_none() {
        diags.push(Diagnostic::error(
            project_file,
            span,
            format!(
                "unknown style `{word}` – allowed: snake_case, camelCase, PascalCase, \
                 SCREAMING_SNAKE_CASE, any (docs/04)"
            ),
        ));
    }
    let case = case?;

    let severity = match read_severity_override(lint, key) {
        Some(ident) => match parse_severity(ident.text()) {
            Some(severity) => severity,
            None => {
                diags.push(Diagnostic::error(
                    project_file,
                    ident.span(),
                    format!("unknown severity `{}` – allowed: warn, error", ident.text()),
                ));
                return None;
            }
        },
        None => Severity::Warning,
    };
    Some((case, severity))
}

fn check_naming(
    ctx: &Ctx,
    project_file: FileId,
    lint: &ast::Block,
    key: &str,
    kind: BlockKind,
    diags: &mut Vec<Diagnostic>,
) {
    let Some((case, severity)) = read_case(project_file, lint, key, diags) else {
        return;
    };
    for file in &ctx.facts {
        for (name, decl) in &file.declarations {
            if decl.kind == kind && !matches_case(name, case) {
                diags.push(Diagnostic::new(
                    file.id,
                    severity,
                    decl.name_span,
                    format!("`{name}` does not follow the configured `{key}` (Lint)"),
                ));
            }
        }
    }
}

fn check_field_naming(
    ctx: &Ctx,
    project_file: FileId,
    lint: &ast::Block,
    diags: &mut Vec<Diagnostic>,
) {
    let Some((case, severity)) = read_case(project_file, lint, "fieldNaming", diags) else {
        return;
    };
    for file in &ctx.facts {
        for decl in file.declarations.values() {
            if !matches!(decl.kind, BlockKind::Table | BlockKind::Partial) {
                continue;
            }
            for item in decl.block.items() {
                let ast::ItemKind::Field(f) = item.kind() else {
                    continue;
                };
                let name = f.name();
                if !matches_case(name.text(), case) {
                    diags.push(Diagnostic::new(
                        file.id,
                        severity,
                        name.span(),
                        format!(
                            "`{}` does not follow the configured `fieldNaming` (Lint)",
                            name.text()
                        ),
                    ));
                }
            }
        }
    }
}

// ===========================================================================
// unusedImport
// ===========================================================================

fn check_unused_imports(
    ctx: &Ctx,
    project_file: FileId,
    lint: &ast::Block,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(severity) = read_severity(project_file, lint, "unusedImport", diags) else {
        return;
    };

    for file in &ctx.facts {
        let used = collect_used_namespaces(&file.ast);
        for record in &ctx.namespaces.imports[file.id.0 as usize] {
            if !used.contains(record.visible_as.as_str()) {
                diags.push(Diagnostic::new(
                    file.id,
                    severity,
                    record.span,
                    format!(
                        "import `{}` is not used anywhere in this file (Lint.unusedImport)",
                        record.visible_as
                    ),
                ));
            }
        }
    }
}

/// All namespace prefixes actually used anywhere in the file – a plain text
/// search isn't enough for `unusedImport`, because a prefix appears in many
/// shapes: field type, relationship target, `use`, `Group` reference, any
/// value (even nested inside lists).
fn collect_used_namespaces(file: &ast::SourceFile) -> HashSet<SmolStr> {
    let mut used = HashSet::new();
    walk_items(file.items(), &mut used);
    used
}

fn note_path(path: &ast::Path, used: &mut HashSet<SmolStr>) {
    let mut segments = path.segments();
    if let (Some(first), Some(_)) = (segments.next(), segments.next()) {
        used.insert(SmolStr::new(first.text()));
    }
}

fn note_value(value: &ast::Value, used: &mut HashSet<SmolStr>) {
    match value.kind() {
        Some(ValueKind::Path(p)) => note_path(&p, used),
        Some(ValueKind::Type { path, args }) => {
            note_path(&path, used);
            args.iter().for_each(|a| note_value(a, used));
        }
        Some(ValueKind::List(items)) => items.iter().for_each(|v| note_value(v, used)),
        _ => {}
    }
}

fn note_flags(flags: impl Iterator<Item = ast::Attr>, used: &mut HashSet<SmolStr>) {
    for flag in flags {
        if let Some(v) = flag.value() {
            note_value(&v, used);
        }
    }
}

fn walk_items(items: impl Iterator<Item = ast::Item>, used: &mut HashSet<SmolStr>) {
    for item in items {
        match item.kind() {
            ast::ItemKind::Use(u) => note_path(&u.target(), used),
            ast::ItemKind::Ref(r) => note_path(&r.path(), used),
            ast::ItemKind::Setting(a) => {
                if let Some(v) = a.value() {
                    note_value(&v, used);
                }
            }
            ast::ItemKind::Block(b) => {
                if let Some(n) = b.name() {
                    note_path(&n, used);
                }
                for ty in b.params().filter_map(|param| param.ty()) {
                    note_path(&ty, used);
                }
                walk_items(b.items(), used);
            }
            ast::ItemKind::Field(f) => {
                match f.ty().and_then(|ty| ty.kind()) {
                    Some(ast::FieldTypeKind::Named { path, args }) => {
                        note_path(&path, used);
                        args.iter().for_each(|a| note_value(a, used));
                    }
                    Some(ast::FieldTypeKind::Relation { target, .. }) => note_path(&target, used),
                    None => {}
                }
                note_flags(f.flags(), used);
                walk_items(f.items(), used);
            }
            ast::ItemKind::IndexEntry(e) => note_flags(e.flags(), used),
            _ => {}
        }
    }
}

// ===========================================================================
// requirePrimaryKey / requireForeignKeyIndex / requireTableNote / requireFieldNote
// ===========================================================================

fn check_require_primary_key(
    project_file: FileId,
    lint: &ast::Block,
    tables: &[model::Table],
    diags: &mut Vec<Diagnostic>,
) {
    let Some(severity) = read_severity(project_file, lint, "requirePrimaryKey", diags) else {
        return;
    };
    for table in tables {
        // `table.fields` already has `use Partial` inlined, so an inherited
        // `pk` counts.
        if table.fields.iter().any(|f| f.flags.contains_key("pk")) {
            continue;
        }
        diags.push(Diagnostic::new(
            table.file,
            severity,
            table.span,
            format!(
                "table `{}` has no primary key (Lint.requirePrimaryKey)",
                table.name
            ),
        ));
    }
}

/// Every relation op except `<` (`ManyToOne`, the backreference side)
/// creates a column (`docs/07`, "Not every relationship creates a column").
fn check_require_foreign_key_index(
    project_file: FileId,
    lint: &ast::Block,
    tables: &[model::Table],
    diags: &mut Vec<Diagnostic>,
) {
    let Some(severity) = read_severity(project_file, lint, "requireForeignKeyIndex", diags) else {
        return;
    };
    for table in tables {
        for field in &table.fields {
            let Some(relation) = &field.relation else {
                continue;
            };
            if relation.kind == model::RelationKind::ManyToOne {
                continue;
            }
            // Covering index: the field's own flag, or the leading column
            // of some `Index` entry – column order is significant (`docs/10`).
            let covered = field.flags.contains_key("unique")
                || field.flags.contains_key("pk")
                || table
                    .indexes
                    .iter()
                    .any(|index| index.columns.first().is_some_and(|c| c == &field.name));
            if covered {
                continue;
            }
            diags.push(Diagnostic::new(
                field.file,
                severity,
                field.span,
                format!(
                    "foreign key `{}` has no covering index – add `[unique]`/`[pk]` or make it \
                     the leading column of an `Index` (Lint.requireForeignKeyIndex)",
                    field.name
                ),
            ));
        }
    }
}

fn check_require_table_note(
    ctx: &Ctx,
    project_file: FileId,
    lint: &ast::Block,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(severity) = read_severity(project_file, lint, "requireTableNote", diags) else {
        return;
    };
    for file in &ctx.facts {
        for decl in file.declarations.values() {
            if decl.kind != BlockKind::Table {
                continue;
            }
            let has_note = decl
                .block
                .child(BlockKind::Meta)
                .is_some_and(|meta| has_note_setting(meta.items()));
            if has_note {
                continue;
            }
            diags.push(Diagnostic::new(
                file.id,
                severity,
                decl.item_span,
                "table has no `Meta.note` (Lint.requireTableNote)",
            ));
        }
    }
}

fn check_require_field_note(
    ctx: &Ctx,
    project_file: FileId,
    lint: &ast::Block,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(severity) = read_severity(project_file, lint, "requireFieldNote", diags) else {
        return;
    };
    for file in &ctx.facts {
        for decl in file.declarations.values() {
            if !matches!(decl.kind, BlockKind::Table | BlockKind::Partial) {
                continue;
            }
            for item in decl.block.items() {
                let ast::ItemKind::Field(f) = item.kind() else {
                    continue;
                };
                if has_note_setting(f.items()) {
                    continue;
                }
                let name = f.name();
                diags.push(Diagnostic::new(
                    file.id,
                    severity,
                    name.span(),
                    format!(
                        "field `{}` has no `note` (Lint.requireFieldNote)",
                        name.text()
                    ),
                ));
            }
        }
    }
}

/// A `note = "…";` setting directly inside `items` – shared by
/// `requireTableNote` (a `Meta` child's items) and `requireFieldNote` (a
/// field's own body).
fn has_note_setting(mut items: impl Iterator<Item = ast::Item>) -> bool {
    items.any(|item| {
        matches!(
            item.kind(),
            ast::ItemKind::Setting(a)
                if a.key().text() == "note"
                    && matches!(a.value().and_then(|v| v.kind()), Some(ValueKind::Str(_)))
        )
    })
}
