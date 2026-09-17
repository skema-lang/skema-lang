//! The one `Project` block: existence, uniqueness, `mode`/`dialect`
//! (`docs/02`), and the Oracle-only `identifierLength` setting (`docs/11`).

use skema_syntax::Span;
use skema_syntax::ast::{self, AstNode, BlockKind, ValueKind};
use smol_str::SmolStr;

use super::{Dialect, Mode, OracleIdentifierLength, ProjectSettings};
use crate::catalog::{DIALECT_ENGINES, IDENTIFIER_LENGTH_VALUES};
use crate::symbols::FileFacts;
use crate::{Diagnostic, FileId};

pub(crate) fn check(facts: &[FileFacts], diags: &mut Vec<Diagnostic>) -> Option<ProjectSettings> {
    let all: Vec<(FileId, Span, &ast::Block)> = facts
        .iter()
        .flat_map(|f| f.projects.iter().map(move |(span, b)| (f.id, *span, b)))
        .collect();

    let (owner, first_span, block) = match all.first() {
        Some(&x) => x,
        None => {
            let anchor_file = facts.first().map(|f| f.id).unwrap_or(FileId(0));
            let anchor_span = facts.first().map(|f| f.ast.span()).unwrap_or_default();
            diags.push(Diagnostic::error(
                anchor_file,
                anchor_span,
                "no `Project` block found – exactly one is needed per project (docs/02)",
            ));
            return None;
        }
    };

    for &(file, span, _) in &all[1..] {
        diags.push(Diagnostic::error(
            file,
            span,
            format!("second `Project` block – the first is at {first_span:?} (docs/02)"),
        ));
    }

    let mode = read_word(block, "mode").and_then(|(word, span)| match word.as_str() {
        "draft" => Some(Mode::Draft),
        "strict" => Some(Mode::Strict),
        other => {
            diags.push(Diagnostic::error(
                owner,
                span,
                format!("unknown `mode` value `{other}` – allowed: draft, strict"),
            ));
            None
        }
    });
    let dialect = read_word(block, "dialect").and_then(|(word, span)| match word.as_str() {
        "generic" => Some(Dialect::Generic),
        "postgres" => Some(Dialect::Postgres),
        "mysql" => Some(Dialect::Mysql),
        "oracledb" => Some(Dialect::Oracledb),
        other => {
            diags.push(Diagnostic::error(
                owner,
                span,
                format!(
                    "unknown `dialect` value `{other}` – allowed: generic, {}",
                    DIALECT_ENGINES.join(", ")
                ),
            ));
            None
        }
    });

    if mode.is_none() && read_word(block, "mode").is_none() {
        diags.push(Diagnostic::error(
            owner,
            first_span,
            "`Project.mode` is missing – draft or strict (docs/02)",
        ));
    }
    if dialect.is_none() && read_word(block, "dialect").is_none() {
        diags.push(Diagnostic::error(
            owner,
            first_span,
            "`Project.dialect` is missing – generic, postgres, mysql, or oracledb (docs/02)",
        ));
    }

    let lint = block.child(BlockKind::Lint);
    let database = block.child(BlockKind::Database);
    let oracle_identifier_length = read_identifier_length(owner, database.as_ref(), diags);

    check_identifier_length_placement(facts, diags);

    Some(ProjectSettings {
        file: owner,
        mode: mode.unwrap_or(Mode::Draft),
        dialect: dialect.unwrap_or(Dialect::Generic),
        lint,
        database,
        oracle_identifier_length,
    })
}

/// Reads `Database { Dialect @oracledb { identifierLength = "…"; } }` off
/// the *Project*'s own `Database` block – a closed `"standard"`/`"extended"`
/// value (decision 5, task 82). A quoted string, not a bare word like
/// `mode`/`dialect`: every other setting inside a `Dialect` block is
/// (`tablespace`, `engine`, `charset`, `docs/11` "Engine-specific
/// settings"), and this one follows that convention.
fn read_identifier_length(
    owner: FileId,
    database: Option<&ast::Block>,
    diags: &mut Vec<Diagnostic>,
) -> OracleIdentifierLength {
    let Some(oracledb) = database.and_then(|b| dialect_child(b, "oracledb")) else {
        return OracleIdentifierLength::Standard;
    };
    let Some((value, span)) = read_string(&oracledb, "identifierLength") else {
        return OracleIdentifierLength::Standard;
    };
    match value.as_str() {
        "standard" => OracleIdentifierLength::Standard,
        "extended" => OracleIdentifierLength::Extended,
        other => {
            diags.push(Diagnostic::error(
                owner,
                span,
                format!(
                    "unknown `identifierLength` value `{other}` – allowed: {}",
                    IDENTIFIER_LENGTH_VALUES.join(", ")
                ),
            ));
            OracleIdentifierLength::Standard
        }
    }
}

/// `identifierLength` is a database-instance-wide fact, unlike
/// `tablespace`/`schema` – legal only directly inside *Project*'s own
/// `Database` block, never a `Document`'s or a `Table`'s (decision 5, task
/// 82). Walks every file's whole tree, tracking the nearest enclosing
/// top-level declaration the way `rules::keys` does.
fn check_identifier_length_placement(facts: &[FileFacts], diags: &mut Vec<Diagnostic>) {
    for file in facts {
        walk_declarations(file.id, file.ast.items(), None, diags);
    }
}

fn walk_declarations(
    id: FileId,
    items: impl Iterator<Item = ast::Item>,
    declaration: Option<BlockKind>,
    diags: &mut Vec<Diagnostic>,
) {
    for item in items {
        let ast::ItemKind::Block(b) = item.kind() else {
            continue;
        };
        let kind = b.kind();
        if kind == BlockKind::Database && declaration != Some(BlockKind::Project) {
            reject_identifier_length(id, &b, diags);
        }
        let next_declaration = if kind.is_declaration() {
            Some(kind)
        } else {
            declaration
        };
        walk_declarations(id, b.items(), next_declaration, diags);
    }
}

fn reject_identifier_length(id: FileId, database: &ast::Block, diags: &mut Vec<Diagnostic>) {
    let Some(oracledb) = dialect_child(database, "oracledb") else {
        return;
    };
    if let Some((_, span)) = read_string(&oracledb, "identifierLength") {
        diags.push(Diagnostic::error(
            id,
            span,
            "`identifierLength` is only legal in `Project`'s own `Database` block (docs/11)",
        ));
    }
}

/// The direct `Dialect @<engine>` sub-block of `block`, if any – `Block::child`
/// doesn't filter by selector, and a `Database` block can carry more than
/// one engine's settings side by side (`docs/11`, "Engine-specific settings").
fn dialect_child(block: &ast::Block, engine: &str) -> Option<ast::Block> {
    block.items().find_map(|item| match item.kind() {
        ast::ItemKind::Block(b)
            if b.kind() == BlockKind::Dialect
                && b.selector().is_some_and(|s| s.text() == engine) =>
        {
            Some(b)
        }
        _ => None,
    })
}

/// Reads `key = "…";` directly in the body of `block` – the form every
/// other setting inside a `Database`/`Dialect` block uses (`database`,
/// `schema`, `table`, and the engine-specific settings in `docs/11`).
fn read_string(block: &ast::Block, key: &str) -> Option<(String, Span)> {
    block.items().find_map(|item| match item.kind() {
        ast::ItemKind::Setting(a) if a.key().text() == key => {
            a.value().and_then(|v| match v.kind() {
                Some(ValueKind::Str(s)) => Some((s, v.span())),
                _ => None,
            })
        }
        _ => None,
    })
}

/// Reads `key = word;` directly in the body of `block` – the form in which
/// `mode`, `dialect`, and the rules in `Lint` are written.
pub(crate) fn read_word(block: &ast::Block, key: &str) -> Option<(SmolStr, Span)> {
    block.items().find_map(|item| match item.kind() {
        ast::ItemKind::Setting(a) if a.key().text() == key => {
            let value = a.value()?;
            match value.kind() {
                Some(ValueKind::Path(p)) => {
                    p.as_ident().map(|i| (SmolStr::new(i.text()), value.span()))
                }
                _ => None,
            }
        }
        _ => None,
    })
}
