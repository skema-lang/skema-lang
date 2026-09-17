//! Reserved-word collisions and identifier length limits on *physical*
//! names – table, column, schema, database (`docs/11`, "Reserved words and
//! identifier length"; task 82). Only under `mode = strict` and a non-
//! `generic` dialect (decisions 2–3): `draft` and `generic` get no
//! diagnostic at all, matching task 01's precedent.

use skema_syntax::Span;

use super::{Mode, ProjectSettings};
use crate::catalog;
use crate::model;
use crate::{Diagnostic, FileId};

pub(crate) fn check(
    project: Option<&ProjectSettings>,
    tables: &[model::Table],
    diags: &mut Vec<Diagnostic>,
) {
    let Some(project) = project else { return };
    if project.mode != Mode::Strict {
        return;
    }
    let Some(engine) = project.dialect.engine_name() else {
        return;
    };
    let dialect = DialectLimits {
        engine,
        reserved: catalog::reserved_words(engine),
        limit: identifier_limit(project, engine),
    };

    for table in tables {
        check_name(
            table.file,
            table.span,
            IdentifierKind::Table,
            physical_table_name(table),
            &dialect,
            diags,
        );
        if let Some(schema) = &table.physical.schema {
            check_name(
                table.file,
                table.span,
                IdentifierKind::Schema,
                schema,
                &dialect,
                diags,
            );
        }
        if let Some(database) = &table.physical.database {
            check_name(
                table.file,
                table.span,
                IdentifierKind::Database,
                database,
                &dialect,
                diags,
            );
        }
        for field in &table.fields {
            check_name(
                field.file,
                field.span,
                IdentifierKind::Column,
                physical_column_name(field),
                &dialect,
                diags,
            );
        }
    }
}

/// The active dialect's reserved-word list and byte limit, bundled so
/// [`check_name`] doesn't drown under separate parameters for each.
struct DialectLimits<'a> {
    engine: &'a str,
    reserved: &'a [&'a str],
    limit: usize,
}

/// `table.physical.table`, falling back to the business name – same
/// "unset means the business name applies" rule `docs/11` documents for
/// every physical-name key.
fn physical_table_name(table: &model::Table) -> &str {
    table.physical.table.as_deref().unwrap_or(&table.name)
}

/// A field's `column="…"` flag, falling back to its declared name.
fn physical_column_name(field: &model::Field) -> &str {
    match field.flags.get("column") {
        Some(model::FlagValue::Text(name)) => name,
        _ => &field.name,
    }
}

/// Postgres and MySQL are fixed; Oracle's depends on the project's
/// `identifierLength` setting (decision 5). An engine outside
/// [`catalog::DIALECT_ENGINES`] has no decided limit – never flagged
/// (task 82, "skip those dialects silently").
fn identifier_limit(project: &ProjectSettings, engine: &str) -> usize {
    match engine {
        "postgres" => catalog::POSTGRES_IDENTIFIER_LIMIT,
        "mysql" => catalog::MYSQL_IDENTIFIER_LIMIT,
        "oracledb" => project.oracle_identifier_length.limit_bytes(),
        _ => usize::MAX,
    }
}

#[derive(Clone, Copy)]
enum IdentifierKind {
    Table,
    Column,
    Schema,
    Database,
}

impl IdentifierKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::Column => "column",
            Self::Schema => "schema",
            Self::Database => "database",
        }
    }
}

/// One diagnostic per violation (task 82): a name can be both reserved and
/// too long, and gets a diagnostic for each. Reserved-word matching is
/// case-insensitive (decision 4); the length limit is bytes, since that's
/// what every target engine itself counts against.
fn check_name(
    file: FileId,
    span: Span,
    kind: IdentifierKind,
    name: &str,
    dialect: &DialectLimits<'_>,
    diags: &mut Vec<Diagnostic>,
) {
    if dialect
        .reserved
        .iter()
        .any(|word| word.eq_ignore_ascii_case(name))
    {
        diags.push(Diagnostic::error(
            file,
            span,
            format!(
                "`{name}` is a reserved word in {} – rename this {} or give it a \
                 non-colliding physical name (docs/11)",
                dialect.engine,
                kind.label()
            ),
        ));
    }
    let byte_len = name.len();
    if byte_len > dialect.limit {
        diags.push(Diagnostic::error(
            file,
            span,
            format!(
                "`{name}` is {byte_len} bytes long – exceeds {}'s {}-byte identifier limit \
                 (docs/11)",
                dialect.engine, dialect.limit
            ),
        ));
    }
}
