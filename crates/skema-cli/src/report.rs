//! Terminal representation of a message: `path:line:column: severity: text`.
//!
//! Everything *behind* that line – the analysis, the byte-offset conversion,
//! the merge of parser and checker messages, the sort order – is
//! `skema_loader::analyze`'s, shared with `skema-py` and `skema-lsp`. What
//! stays here is only the format, because a terminal line is the one thing
//! about a diagnostic that is genuinely this crate's business: `skema-api`
//! serves editors and has no reason to know what a shell prints.

use std::fmt;

use skema_api::{Diagnostic, EnumChange, EnumDiff, IndexIdentity, TableChange, TableDiff};

/// Wraps a diagnostic in the one-line form a compiler is expected to print,
/// so `println!("{}", report::line(&d))` reads like the rest of the CLI.
pub fn line(diagnostic: &Diagnostic) -> impl fmt::Display + '_ {
    Line(diagnostic)
}

struct Line<'a>(&'a Diagnostic);

impl fmt::Display for Line<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d = self.0;
        write!(
            f,
            "{}:{}:{}: {}: {}",
            d.location.file,
            d.location.line,
            d.location.column,
            d.severity.as_str(),
            d.message
        )
    }
}

/// Wraps one `skema diff` table entry in its one-line report form, e.g.
/// `Table Order: column customer_id dropped [BREAKING]`.
pub fn table_diff_line(entry: &TableDiff) -> impl fmt::Display + '_ {
    DiffLine {
        kind: "Table",
        name: &entry.name,
        description: table_change_description(&entry.change),
        breaking: entry.breaking,
    }
}

/// Wraps one `skema diff` enum entry in its one-line report form, e.g.
/// `Enum Status: entry paid added`.
pub fn enum_diff_line(entry: &EnumDiff) -> impl fmt::Display + '_ {
    DiffLine {
        kind: "Enum",
        name: &entry.name,
        description: enum_change_description(&entry.change),
        breaking: entry.breaking,
    }
}

struct DiffLine<'a> {
    kind: &'static str,
    name: &'a str,
    description: String,
    breaking: bool,
}

impl fmt::Display for DiffLine<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}: {}", self.kind, self.name, self.description)?;
        if self.breaking {
            write!(f, " [BREAKING]")?;
        }
        Ok(())
    }
}

fn table_change_description(change: &TableChange) -> String {
    match change {
        TableChange::Added => "table added".to_string(),
        TableChange::Dropped => "table dropped".to_string(),
        TableChange::Renamed { from } => format!("renamed from {from}"),
        TableChange::ColumnAdded { column } => format!("column {column} added"),
        TableChange::ColumnDropped { column } => format!("column {column} dropped"),
        TableChange::ColumnTypeChanged {
            column,
            old_type,
            new_type,
        } => format!("column {column} type changed from {old_type} to {new_type}"),
        TableChange::ColumnNewlyRequired { column } => format!("column {column} newly required"),
        TableChange::ColumnNewlyUniqueOrPk { column } => {
            format!("column {column} newly unique")
        }
        TableChange::RelationAdded { column } => format!("relation {column} added"),
        TableChange::RelationRemoved { column } => format!("relation {column} removed"),
        TableChange::IndexAdded { index } => format!("index {} added", index_name(index)),
        TableChange::IndexRemoved { index } => format!("index {} removed", index_name(index)),
    }
}

fn enum_change_description(change: &EnumChange) -> String {
    match change {
        EnumChange::Added => "enum added".to_string(),
        EnumChange::Dropped => "enum dropped".to_string(),
        EnumChange::EntryAdded { entry } => format!("entry {entry} added"),
        EnumChange::EntryRemoved { entry } => format!("entry {entry} removed"),
    }
}

fn index_name(index: &IndexIdentity) -> String {
    index
        .name
        .clone()
        .unwrap_or_else(|| format!("({})", index.columns.join(", ")))
}
