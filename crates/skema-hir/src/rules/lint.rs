//! `Lint`: naming conventions and `unusedImport` – both only in effect under
//! `mode = strict` (`docs/04`).
//!
//! A rule only applies when it's **written in the `Lint` block** – "makes
//! conventions explicit … instead of leaving them as unwritten tool
//! defaults" (`docs/04`). An unwritten `fieldNaming` therefore means: no
//! check, not "some default style".

use std::collections::HashSet;

use skema_syntax::Severity;
use skema_syntax::ast::{self, BlockKind, ValueKind};

use super::project::read_word;
use super::{Ctx, Mode, ProjectSettings};
use crate::{Diagnostic, FileId};

pub(crate) fn check(
    ctx: &Ctx<'_>,
    project: Option<&ProjectSettings<'_>>,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(project) = project else { return };
    if project.mode != Mode::Strict {
        return;
    }
    let Some(lint) = project.lint else { return };

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
}

// ===========================================================================
// Naming conventions
// ===========================================================================

#[derive(Clone, Copy)]
enum Case {
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

fn matches_case(name: &str, case: Case) -> bool {
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

fn read_case(
    project_file: FileId,
    lint: &ast::Block,
    key: &str,
    diags: &mut Vec<Diagnostic>,
) -> Option<Case> {
    let (word, span) = read_word(lint, key)?;
    let case = parse_case(word);
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
    case
}

fn check_naming(
    ctx: &Ctx<'_>,
    project_file: FileId,
    lint: &ast::Block,
    key: &str,
    kind: BlockKind,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(case) = read_case(project_file, lint, key, diags) else {
        return;
    };
    for file in &ctx.facts {
        for (&name, decl) in &file.declarations {
            if decl.kind == kind && !matches_case(name, case) {
                diags.push(Diagnostic::warning(
                    file.id,
                    decl.name_span,
                    format!("`{name}` does not follow the configured `{key}` (Lint)"),
                ));
            }
        }
    }
}

fn check_field_naming(
    ctx: &Ctx<'_>,
    project_file: FileId,
    lint: &ast::Block,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(case) = read_case(project_file, lint, "fieldNaming", diags) else {
        return;
    };
    for file in &ctx.facts {
        for decl in file.declarations.values() {
            if !matches!(decl.kind, BlockKind::Table | BlockKind::Partial) {
                continue;
            }
            for item in &decl.block.items {
                let ast::ItemKind::Field(f) = &item.kind else {
                    continue;
                };
                if !matches_case(&f.name.text, case) {
                    diags.push(Diagnostic::warning(
                        file.id,
                        f.name.span,
                        format!(
                            "`{}` does not follow the configured `fieldNaming` (Lint)",
                            f.name.text
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
    ctx: &Ctx<'_>,
    project_file: FileId,
    lint: &ast::Block,
    diags: &mut Vec<Diagnostic>,
) {
    let Some((severity_word, severity_span)) = read_word(lint, "unusedImport") else {
        return;
    };
    let severity = match severity_word {
        "warn" => Severity::Warning,
        "error" => Severity::Error,
        other => {
            diags.push(Diagnostic::error(
                project_file,
                severity_span,
                format!("unknown severity `{other}` – allowed: warn, error"),
            ));
            return;
        }
    };

    for file in &ctx.facts {
        let used = collect_used_namespaces(file.ast);
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
fn collect_used_namespaces(file: &ast::SourceFile) -> HashSet<&str> {
    let mut used = HashSet::new();
    walk_items(&file.items, &mut used);
    used
}

fn note_path<'a>(path: &'a ast::Path, used: &mut HashSet<&'a str>) {
    if path.segments.len() >= 2 {
        used.insert(path.segments[0].text.as_str());
    }
}

fn note_value<'a>(value: &'a ast::Value, used: &mut HashSet<&'a str>) {
    match &value.kind {
        ValueKind::Path(p) => note_path(p, used),
        ValueKind::Type { path, args } => {
            note_path(path, used);
            args.iter().for_each(|a| note_value(a, used));
        }
        ValueKind::List(items) => items.iter().for_each(|v| note_value(v, used)),
        _ => {}
    }
}

fn walk_items<'a>(items: &'a [ast::Item], used: &mut HashSet<&'a str>) {
    for item in items {
        match &item.kind {
            ast::ItemKind::Use(u) => note_path(&u.target, used),
            ast::ItemKind::Ref(r) => note_path(&r.path, used),
            ast::ItemKind::Setting(a) => {
                if let Some(v) = &a.value {
                    note_value(v, used);
                }
            }
            ast::ItemKind::Block(b) => {
                if let Some(n) = &b.name {
                    note_path(n, used);
                }
                for param in &b.params {
                    if let Some(ty) = &param.ty {
                        note_path(ty, used);
                    }
                }
                walk_items(&b.items, used);
            }
            ast::ItemKind::Field(f) => {
                if let Some(ty) = &f.ty {
                    match &ty.kind {
                        ast::FieldTypeKind::Named { path, args } => {
                            note_path(path, used);
                            args.iter().for_each(|a| note_value(a, used));
                        }
                        ast::FieldTypeKind::Relation { target, .. } => note_path(target, used),
                    }
                }
                for flag in &f.flags {
                    if let Some(v) = &flag.value {
                        note_value(v, used);
                    }
                }
                walk_items(&f.items, used);
            }
            ast::ItemKind::IndexEntry(e) => {
                for flag in &e.flags {
                    if let Some(v) = &flag.value {
                        note_value(v, used);
                    }
                }
            }
            _ => {}
        }
    }
}
