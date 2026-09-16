//! Structural diff between two resolved [`ExportedModel`]s – the input for
//! a breaking-change report ([task 73]) and, eventually, migration SQL
//! ([task 75]). Platform-neutral like the rest of `skema-api` (`CLAUDE.md`
//! rule 1): takes two already-exported models, does no loading of its own.
//!
//! [task 73]: ../../../../brain/tasks/73-breaking-change-classification.md
//! [task 75]: ../../../../brain/tasks/75-migration-sql-emission-blocked.md
//!
//! **Deliberately out of scope for v1** (see `brain/decisions.md`, "Schema
//! diff & migrations"):
//! - A relation that stays present on both sides but retargets (different
//!   `targetTable`/`targetField`) produces **no** diff entry – only
//!   presence (`Some`/`None`) is compared.
//! - A column that changes name reports as one drop + one add; there is no
//!   column-rename marker (unlike tables' `movedFrom`).
//! - A whole `Enum` added or dropped produces no entry of its own – only
//!   entries *within* an enum matched on both sides are compared, mirroring
//!   how entries are matched by name only, with no `movedFrom` equivalent.
//! - A relation field's `ExportedTypeRef` is a synthetic
//!   `TargetTable.targetField` reference, not a real column type
//!   (`skema-hir`'s `resolve_field`) – so `ColumnTypeChanged` is never
//!   compared for a field that is a relation on either side, which is also
//!   what keeps a retargeted relation producing no entry.

use std::collections::HashMap;

use serde::Serialize;
#[cfg(feature = "typescript")]
use tsify_next::Tsify;

use crate::dto::{ExportedEnum, ExportedField, ExportedIndex, ExportedModel, ExportedTable};

#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiff {
    pub tables: Vec<TableDiff>,
    pub enums: Vec<EnumDiff>,
    /// True iff any entry below is `breaking` – computed once here so
    /// callers (task 74's `--fail-on-breaking`) never re-derive it by
    /// scanning entries themselves.
    pub has_breaking_changes: bool,
}

/// One change affecting one table – either the table itself (added /
/// dropped / renamed) or something inside it (a column, its relation, or
/// an index), scoped by `namespace`/`name` the same way [`ExportedTable`]
/// identifies a table (`docs/02`: a bare name is ambiguous across files).
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct TableDiff {
    pub namespace: String,
    pub name: String,
    pub change: TableChange,
    /// Per the taxonomy in [task 73](../../../../brain/tasks/73-breaking-change-classification.md).
    pub breaking: bool,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TableChange {
    Added,
    Dropped,
    /// `name`/`namespace` above are the table's *new* identity; `from` is
    /// its old name (same namespace) per `Meta.movedFrom`.
    Renamed {
        from: String,
    },
    ColumnAdded {
        column: String,
    },
    ColumnDropped {
        column: String,
    },
    /// Full resolved type signature (name + args), rendered the same way on
    /// both sides – e.g. `varchar(50)` vs `varchar(100)`. Exact match only;
    /// no "is this narrower or wider" semantics (that's task 73's job).
    ColumnTypeChanged {
        column: String,
        old_type: String,
        new_type: String,
    },
    /// Gained `required` or `pk`, has no `default`, and had neither before.
    ColumnNewlyRequired {
        column: String,
    },
    /// An *existing* column (present on both sides) gained `unique` or
    /// `pk` – distinct from a brand-new column carrying the same flags,
    /// which is reported as `ColumnAdded` only.
    ColumnNewlyUniqueOrPk {
        column: String,
    },
    RelationAdded {
        column: String,
    },
    RelationRemoved {
        column: String,
    },
    IndexAdded {
        index: IndexIdentity,
    },
    IndexRemoved {
        index: IndexIdentity,
    },
}

/// Enough to name an index in a human-readable line: its declared name, or
/// – for an unnamed index – the ordered column set it was matched by.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct IndexIdentity {
    pub name: Option<String>,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct EnumDiff {
    pub namespace: String,
    pub name: String,
    pub change: EnumChange,
    pub breaking: bool,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EnumChange {
    EntryAdded { entry: String },
    EntryRemoved { entry: String },
}

/// A table's identity project-wide – a bare name is ambiguous (two files
/// may each declare a same-named table, `docs/02`), matching the
/// `(namespace, name)` key `skema-hir` itself already uses internally
/// (`rules/resolve.rs::table_key`).
type TableKey = (String, String);

fn table_key(t: &ExportedTable) -> TableKey {
    (t.namespace.clone(), t.name.clone())
}

pub fn diff_models(old: &ExportedModel, new: &ExportedModel) -> SchemaDiff {
    let tables = diff_tables(&old.tables, &new.tables);
    let enums = diff_enums(&old.enums, &new.enums);
    let has_breaking_changes =
        tables.iter().any(|t| t.breaking) || enums.iter().any(|e| e.breaking);
    SchemaDiff {
        tables,
        enums,
        has_breaking_changes,
    }
}

/// Breaking-change taxonomy (owner-confirmed, `brain/decisions.md` "Schema
/// diff & migrations"; task 73). `ColumnAdded` is the only variant that
/// needs the new side's field flags, since a nullable or defaulted column
/// added is safe but a required-with-no-default one is not.
fn is_table_change_breaking(
    change: &TableChange,
    new_fields: &HashMap<&str, &ExportedField>,
) -> bool {
    match change {
        TableChange::Dropped
        | TableChange::ColumnDropped { .. }
        | TableChange::ColumnTypeChanged { .. }
        | TableChange::ColumnNewlyRequired { .. }
        | TableChange::ColumnNewlyUniqueOrPk { .. }
        | TableChange::RelationRemoved { .. } => true,
        TableChange::Added
        | TableChange::Renamed { .. }
        | TableChange::RelationAdded { .. }
        | TableChange::IndexAdded { .. }
        | TableChange::IndexRemoved { .. } => false,
        TableChange::ColumnAdded { column } => new_fields.get(column.as_str()).is_some_and(|f| {
            (has_flag(f, "required") || has_flag(f, "pk")) && !has_flag(f, "default")
        }),
    }
}

fn is_enum_change_breaking(change: &EnumChange) -> bool {
    matches!(change, EnumChange::EntryRemoved { .. })
}

fn diff_tables(old: &[ExportedTable], new: &[ExportedTable]) -> Vec<TableDiff> {
    let old_by_key: HashMap<TableKey, &ExportedTable> =
        old.iter().map(|t| (table_key(t), t)).collect();
    let new_by_key: HashMap<TableKey, &ExportedTable> =
        new.iter().map(|t| (table_key(t), t)).collect();

    let mut entries = Vec::new();
    let mut consumed_old: Vec<TableKey> = Vec::new();

    for new_table in new {
        let key = table_key(new_table);
        if let Some(old_table) = old_by_key.get(&key) {
            consumed_old.push(key);
            entries.extend(diff_matched_table(old_table, new_table));
            continue;
        }

        // Rename: `new_table`'s own name is unmatched, but its `movedFrom`
        // names an old table that has itself disappeared from the new side
        // – if that old name still exists on the new side, it isn't a
        // rename (the old table lives on as itself), so `new_table` is a
        // plain add instead.
        if let Some(from_name) = &new_table.moved_from {
            let from_key = (new_table.namespace.clone(), from_name.clone());
            if let Some(old_table) = old_by_key.get(&from_key) {
                if !new_by_key.contains_key(&from_key) {
                    consumed_old.push(from_key);
                    entries.push(TableDiff {
                        namespace: new_table.namespace.clone(),
                        name: new_table.name.clone(),
                        change: TableChange::Renamed {
                            from: from_name.clone(),
                        },
                        breaking: false,
                    });
                    entries.extend(diff_matched_table(old_table, new_table));
                    continue;
                }
            }
        }

        entries.push(TableDiff {
            namespace: new_table.namespace.clone(),
            name: new_table.name.clone(),
            change: TableChange::Added,
            breaking: false,
        });
    }

    for old_table in old {
        let key = table_key(old_table);
        if consumed_old.contains(&key) {
            continue;
        }
        entries.push(TableDiff {
            namespace: old_table.namespace.clone(),
            name: old_table.name.clone(),
            change: TableChange::Dropped,
            breaking: true,
        });
    }

    entries.sort_by_key(sort_key);
    entries
}

/// Column, relation and index changes between a matched pair of tables –
/// same table identity on both sides, whether matched by equal name or by
/// `movedFrom`.
fn diff_matched_table(old: &ExportedTable, new: &ExportedTable) -> Vec<TableDiff> {
    let namespace = new.namespace.clone();
    let name = new.name.clone();
    let new_by_name: HashMap<&str, &ExportedField> =
        new.fields.iter().map(|f| (f.name.as_str(), f)).collect();
    diff_columns_and_relations(&old.fields, &new.fields)
        .into_iter()
        .chain(diff_indexes(&old.indexes, &new.indexes))
        .map(|change| {
            let breaking = is_table_change_breaking(&change, &new_by_name);
            TableDiff {
                namespace: namespace.clone(),
                name: name.clone(),
                change,
                breaking,
            }
        })
        .collect()
}

fn has_flag(field: &ExportedField, name: &str) -> bool {
    field.flags.iter().any(|f| f.name == name)
}

/// The full resolved type signature (name + args) as one comparable,
/// human-readable string – `varchar(50)`, or just `int` with no args.
fn type_signature(field: &ExportedField) -> String {
    if field.ty.args.is_empty() {
        field.ty.name.clone()
    } else {
        format!("{}({})", field.ty.name, field.ty.args.join(", "))
    }
}

fn diff_columns_and_relations(
    old_fields: &[ExportedField],
    new_fields: &[ExportedField],
) -> Vec<TableChange> {
    let old_by_name: HashMap<&str, &ExportedField> =
        old_fields.iter().map(|f| (f.name.as_str(), f)).collect();
    let new_by_name: HashMap<&str, &ExportedField> =
        new_fields.iter().map(|f| (f.name.as_str(), f)).collect();

    let mut changes = Vec::new();

    for new_field in new_fields {
        if !old_by_name.contains_key(new_field.name.as_str()) {
            changes.push(TableChange::ColumnAdded {
                column: new_field.name.clone(),
            });
        }
    }
    for old_field in old_fields {
        if !new_by_name.contains_key(old_field.name.as_str()) {
            changes.push(TableChange::ColumnDropped {
                column: old_field.name.clone(),
            });
        }
    }

    for old_field in old_fields {
        let Some(new_field) = new_by_name.get(old_field.name.as_str()) else {
            continue;
        };
        let column = old_field.name.clone();

        // A relation field's `ty` is a synthetic `TargetTable.targetField`
        // reference (`skema-hir::resolve_field`), not a real column type –
        // comparing it would report every retarget as a type change, which
        // is explicitly out of scope (see the module doc comment). Only
        // compare when the field is a plain column on both sides.
        if old_field.relation.is_none() && new_field.relation.is_none() {
            let old_type = type_signature(old_field);
            let new_type = type_signature(new_field);
            if old_type != new_type {
                changes.push(TableChange::ColumnTypeChanged {
                    column: column.clone(),
                    old_type,
                    new_type,
                });
            }
        }

        let old_required_or_pk = has_flag(old_field, "required") || has_flag(old_field, "pk");
        let new_required_or_pk = has_flag(new_field, "required") || has_flag(new_field, "pk");
        let new_has_default = has_flag(new_field, "default");
        if !old_required_or_pk && new_required_or_pk && !new_has_default {
            changes.push(TableChange::ColumnNewlyRequired {
                column: column.clone(),
            });
        }

        let old_unique_or_pk = has_flag(old_field, "unique") || has_flag(old_field, "pk");
        let new_unique_or_pk = has_flag(new_field, "unique") || has_flag(new_field, "pk");
        if !old_unique_or_pk && new_unique_or_pk {
            changes.push(TableChange::ColumnNewlyUniqueOrPk {
                column: column.clone(),
            });
        }

        match (old_field.relation.is_some(), new_field.relation.is_some()) {
            (false, true) => changes.push(TableChange::RelationAdded {
                column: column.clone(),
            }),
            (true, false) => changes.push(TableChange::RelationRemoved { column }),
            _ => {}
        }
    }

    changes
}

/// Matched by name where the index is named, otherwise by its ordered
/// column set – two different indexes that happen to share neither never
/// match, and are reported as one removed + one added.
#[derive(PartialEq, Eq, Hash, Clone)]
enum IndexKey {
    Named(String),
    Columns(Vec<String>),
}

fn index_key(i: &ExportedIndex) -> IndexKey {
    match &i.name {
        Some(name) => IndexKey::Named(name.clone()),
        None => IndexKey::Columns(i.columns.clone()),
    }
}

fn index_identity(i: &ExportedIndex) -> IndexIdentity {
    IndexIdentity {
        name: i.name.clone(),
        columns: i.columns.clone(),
    }
}

fn diff_indexes(old: &[ExportedIndex], new: &[ExportedIndex]) -> Vec<TableChange> {
    let old_by_key: HashMap<IndexKey, &ExportedIndex> =
        old.iter().map(|i| (index_key(i), i)).collect();
    let new_by_key: HashMap<IndexKey, &ExportedIndex> =
        new.iter().map(|i| (index_key(i), i)).collect();

    let mut changes = Vec::new();
    for new_index in new {
        if !old_by_key.contains_key(&index_key(new_index)) {
            changes.push(TableChange::IndexAdded {
                index: index_identity(new_index),
            });
        }
    }
    for old_index in old {
        if !new_by_key.contains_key(&index_key(old_index)) {
            changes.push(TableChange::IndexRemoved {
                index: index_identity(old_index),
            });
        }
    }
    changes
}

/// An enum's identity project-wide, same reasoning as [`TableKey`] – enums
/// are sorted by `(namespace, name)` in `skema-hir` for the same reason
/// (`rules/resolve.rs`).
type EnumKey = (String, String);

fn enum_key(e: &ExportedEnum) -> EnumKey {
    (e.namespace.clone(), e.name.clone())
}

fn diff_enums(old: &[ExportedEnum], new: &[ExportedEnum]) -> Vec<EnumDiff> {
    let old_by_key: HashMap<EnumKey, &ExportedEnum> =
        old.iter().map(|e| (enum_key(e), e)).collect();

    let mut entries = Vec::new();
    for new_enum in new {
        let Some(old_enum) = old_by_key.get(&enum_key(new_enum)) else {
            continue; // whole-enum add: out of scope, see module doc
        };

        let old_names: Vec<&str> = old_enum.entries.iter().map(|e| e.name.as_str()).collect();
        let new_names: Vec<&str> = new_enum.entries.iter().map(|e| e.name.as_str()).collect();

        for entry in &new_enum.entries {
            if !old_names.contains(&entry.name.as_str()) {
                let change = EnumChange::EntryAdded {
                    entry: entry.name.clone(),
                };
                let breaking = is_enum_change_breaking(&change);
                entries.push(EnumDiff {
                    namespace: new_enum.namespace.clone(),
                    name: new_enum.name.clone(),
                    change,
                    breaking,
                });
            }
        }
        for entry in &old_enum.entries {
            if !new_names.contains(&entry.name.as_str()) {
                let change = EnumChange::EntryRemoved {
                    entry: entry.name.clone(),
                };
                let breaking = is_enum_change_breaking(&change);
                entries.push(EnumDiff {
                    namespace: new_enum.namespace.clone(),
                    name: new_enum.name.clone(),
                    change,
                    breaking,
                });
            }
        }
    }

    entries.sort_by(|a, b| {
        (
            &a.namespace,
            &a.name,
            change_rank(&EitherChange::Enum(&a.change)),
            enum_change_subject(&a.change),
        )
            .cmp(&(
                &b.namespace,
                &b.name,
                change_rank(&EitherChange::Enum(&b.change)),
                enum_change_subject(&b.change),
            ))
    });
    entries
}

/// Shared ranking so both `TableDiff` and `EnumDiff` sort deterministically
/// without duplicating the same match twice.
enum EitherChange<'a> {
    Table(&'a TableChange),
    Enum(&'a EnumChange),
}

fn change_rank(change: &EitherChange<'_>) -> u8 {
    match change {
        EitherChange::Table(TableChange::Added) => 0,
        EitherChange::Table(TableChange::Dropped) => 1,
        EitherChange::Table(TableChange::Renamed { .. }) => 2,
        EitherChange::Table(TableChange::ColumnAdded { .. }) => 3,
        EitherChange::Table(TableChange::ColumnDropped { .. }) => 4,
        EitherChange::Table(TableChange::ColumnTypeChanged { .. }) => 5,
        EitherChange::Table(TableChange::ColumnNewlyRequired { .. }) => 6,
        EitherChange::Table(TableChange::ColumnNewlyUniqueOrPk { .. }) => 7,
        EitherChange::Table(TableChange::RelationAdded { .. }) => 8,
        EitherChange::Table(TableChange::RelationRemoved { .. }) => 9,
        EitherChange::Table(TableChange::IndexAdded { .. }) => 10,
        EitherChange::Table(TableChange::IndexRemoved { .. }) => 11,
        EitherChange::Enum(EnumChange::EntryAdded { .. }) => 0,
        EitherChange::Enum(EnumChange::EntryRemoved { .. }) => 1,
    }
}

fn table_change_subject(change: &TableChange) -> String {
    match change {
        TableChange::Added | TableChange::Dropped => String::new(),
        TableChange::Renamed { from } => from.clone(),
        TableChange::ColumnAdded { column }
        | TableChange::ColumnDropped { column }
        | TableChange::ColumnTypeChanged { column, .. }
        | TableChange::ColumnNewlyRequired { column }
        | TableChange::ColumnNewlyUniqueOrPk { column }
        | TableChange::RelationAdded { column }
        | TableChange::RelationRemoved { column } => column.clone(),
        TableChange::IndexAdded { index } | TableChange::IndexRemoved { index } => index
            .name
            .clone()
            .unwrap_or_else(|| index.columns.join(",")),
    }
}

fn enum_change_subject(change: &EnumChange) -> String {
    match change {
        EnumChange::EntryAdded { entry } | EnumChange::EntryRemoved { entry } => entry.clone(),
    }
}

fn sort_key(d: &TableDiff) -> (String, String, u8, String) {
    (
        d.namespace.clone(),
        d.name.clone(),
        change_rank(&EitherChange::Table(&d.change)),
        table_change_subject(&d.change),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str =
        "spec 1;\nDocument shop { }\nProject { mode = draft; dialect = generic; }\n";

    fn model(body: &str) -> ExportedModel {
        let src = format!("{HEADER}{body}");
        let result = crate::export_model(&src, None, false);
        assert!(result.ok, "{:#?}", result.diagnostics);
        result.model
    }

    fn diff(old_body: &str, new_body: &str) -> SchemaDiff {
        diff_models(&model(old_body), &model(new_body))
    }

    #[test]
    fn identical_models_produce_an_empty_diff() {
        let body = "Table T { id int [pk]; name varchar(50); }\n";
        let d = diff(body, body);
        assert!(d.tables.is_empty());
        assert!(d.enums.is_empty());
        assert!(!d.has_breaking_changes);
    }

    #[test]
    fn a_new_table_is_added() {
        let d = diff(
            "Table T { id int [pk]; }\n",
            "Table T { id int [pk]; }\nTable U { id int [pk]; }\n",
        );
        assert_eq!(d.tables.len(), 1);
        assert_eq!(d.tables[0].name, "U");
        assert!(matches!(d.tables[0].change, TableChange::Added));
        assert!(!d.tables[0].breaking);
        assert!(!d.has_breaking_changes);
    }

    #[test]
    fn a_removed_table_is_dropped() {
        let d = diff(
            "Table T { id int [pk]; }\nTable U { id int [pk]; }\n",
            "Table T { id int [pk]; }\n",
        );
        assert_eq!(d.tables.len(), 1);
        assert_eq!(d.tables[0].name, "U");
        assert!(matches!(d.tables[0].change, TableChange::Dropped));
        assert!(d.tables[0].breaking);
        assert!(d.has_breaking_changes);
    }

    #[test]
    fn a_table_with_a_moved_from_match_is_a_rename() {
        let d = diff(
            "Table T { id int [pk]; }\n",
            "Table U { id int [pk]; Meta { movedFrom = \"T\"; } }\n",
        );
        let renamed = d
            .tables
            .iter()
            .find(|t| matches!(t.change, TableChange::Renamed { .. }))
            .expect("a rename entry");
        assert_eq!(renamed.name, "U");
        assert!(matches!(&renamed.change, TableChange::Renamed { from } if from == "T"));
        // No separate add/drop alongside the rename.
        assert_eq!(d.tables.len(), 1);
        assert!(!renamed.breaking);
        assert!(!d.has_breaking_changes);
    }

    #[test]
    fn a_moved_from_naming_a_table_that_still_exists_is_not_a_rename() {
        // "T" survives on the new side under its own name, so "U"'s
        // `movedFrom = "T"` cannot be a rename – it's a plain add.
        let d = diff(
            "Table T { id int [pk]; }\n",
            "Table T { id int [pk]; }\nTable U { id int [pk]; Meta { movedFrom = \"T\"; } }\n",
        );
        assert_eq!(d.tables.len(), 1);
        assert_eq!(d.tables[0].name, "U");
        assert!(matches!(d.tables[0].change, TableChange::Added));
        assert!(!d.tables[0].breaking);
    }

    #[test]
    fn a_table_rename_with_no_moved_from_match_reports_as_a_breaking_drop_plus_a_nonbreaking_add() {
        // No `Meta.movedFrom` link at all: "T" simply disappears and "U"
        // simply appears, same as an unrelated drop and add.
        let d = diff("Table T { id int [pk]; }\n", "Table U { id int [pk]; }\n");
        assert_eq!(d.tables.len(), 2);
        let dropped = d
            .tables
            .iter()
            .find(|t| matches!(t.change, TableChange::Dropped))
            .expect("a dropped entry for T");
        let added = d
            .tables
            .iter()
            .find(|t| matches!(t.change, TableChange::Added))
            .expect("an added entry for U");
        assert!(dropped.breaking);
        assert!(!added.breaking);
        assert!(d.has_breaking_changes);
    }

    #[test]
    fn a_new_nullable_column_is_added_and_non_breaking() {
        let d = diff(
            "Table T { id int [pk]; }\n",
            "Table T { id int [pk]; name varchar(50); }\n",
        );
        assert!(matches!(
            &d.tables[0].change,
            TableChange::ColumnAdded { column } if column == "name"
        ));
        assert!(!d.tables[0].breaking);
        assert!(!d.has_breaking_changes);
    }

    #[test]
    fn a_new_column_with_a_default_is_added_and_non_breaking() {
        let d = diff(
            "Table T { id int [pk]; }\n",
            "Table T { id int [pk]; name varchar(50) [default=\"x\"]; }\n",
        );
        assert!(matches!(
            &d.tables[0].change,
            TableChange::ColumnAdded { column } if column == "name"
        ));
        assert!(!d.tables[0].breaking);
    }

    #[test]
    fn a_new_required_column_with_no_default_is_added_and_breaking() {
        let d = diff(
            "Table T { id int [pk]; }\n",
            "Table T { id int [pk]; name varchar(50) [required]; }\n",
        );
        assert!(matches!(
            &d.tables[0].change,
            TableChange::ColumnAdded { column } if column == "name"
        ));
        assert!(d.tables[0].breaking);
        assert!(d.has_breaking_changes);
    }

    #[test]
    fn a_removed_column_is_dropped_and_breaking() {
        let d = diff(
            "Table T { id int [pk]; name varchar(50); }\n",
            "Table T { id int [pk]; }\n",
        );
        assert!(matches!(
            &d.tables[0].change,
            TableChange::ColumnDropped { column } if column == "name"
        ));
        assert!(d.tables[0].breaking);
    }

    #[test]
    fn a_column_rename_reports_as_a_breaking_drop_plus_a_nonbreaking_add() {
        // No column-rename marker exists in v1 (decisions.md) – always
        // drop + add, and classified the same as any other drop + add.
        let d = diff(
            "Table T { id int [pk]; old_name varchar(50); }\n",
            "Table T { id int [pk]; new_name varchar(50); }\n",
        );
        let dropped = d
            .tables
            .iter()
            .find(|t| matches!(&t.change, TableChange::ColumnDropped { column } if column == "old_name"))
            .expect("a dropped entry for old_name");
        let added = d
            .tables
            .iter()
            .find(|t| matches!(&t.change, TableChange::ColumnAdded { column } if column == "new_name"))
            .expect("an added entry for new_name");
        assert!(dropped.breaking);
        assert!(!added.breaking);
    }

    #[test]
    fn a_columns_type_signature_change_is_reported_and_breaking() {
        let d = diff(
            "Table T { id int [pk]; name varchar(50); }\n",
            "Table T { id int [pk]; name varchar(100); }\n",
        );
        assert!(matches!(
            &d.tables[0].change,
            TableChange::ColumnTypeChanged { column, old_type, new_type }
                if column == "name" && old_type == "varchar(50)" && new_type == "varchar(100)"
        ));
        assert!(d.tables[0].breaking);
    }

    #[test]
    fn a_column_newly_required_with_no_default_is_reported_and_breaking() {
        let d = diff(
            "Table T { id int [pk]; name varchar(50); }\n",
            "Table T { id int [pk]; name varchar(50) [required]; }\n",
        );
        assert!(matches!(
            &d.tables[0].change,
            TableChange::ColumnNewlyRequired { column } if column == "name"
        ));
        assert!(d.tables[0].breaking);
    }

    #[test]
    fn a_column_newly_required_but_with_a_default_produces_no_entry() {
        let d = diff(
            "Table T { id int [pk]; name varchar(50); }\n",
            "Table T { id int [pk]; name varchar(50) [required, default=\"x\"]; }\n",
        );
        assert!(d.tables.is_empty());
        assert!(!d.has_breaking_changes);
    }

    #[test]
    fn an_existing_column_newly_unique_is_reported_and_breaking() {
        let d = diff(
            "Table T { id int [pk]; name varchar(50); }\n",
            "Table T { id int [pk]; name varchar(50) [unique]; }\n",
        );
        assert!(matches!(
            &d.tables[0].change,
            TableChange::ColumnNewlyUniqueOrPk { column } if column == "name"
        ));
        assert!(d.tables[0].breaking);
    }

    #[test]
    fn a_brand_new_column_that_is_unique_is_only_reported_as_added() {
        let d = diff(
            "Table T { id int [pk]; }\n",
            "Table T { id int [pk]; name varchar(50) [unique]; }\n",
        );
        assert_eq!(d.tables.len(), 1);
        assert!(matches!(
            &d.tables[0].change,
            TableChange::ColumnAdded { column } if column == "name"
        ));
        // Brand new, still nullable (unique alone doesn't imply required) –
        // non-breaking.
        assert!(!d.tables[0].breaking);
    }

    #[test]
    fn a_relation_added_is_reported_and_non_breaking() {
        let d = diff(
            "Table Customer { id int [pk]; }\nTable T { id int [pk]; customer_id int; }\n",
            "Table Customer { id int [pk]; }\nTable T { id int [pk]; customer_id >Customer.id; }\n",
        );
        let entry = d.tables.iter().find(|t| t.name == "T").expect("Table T");
        assert!(matches!(
            &entry.change,
            TableChange::RelationAdded { column } if column == "customer_id"
        ));
        assert!(!entry.breaking);
    }

    #[test]
    fn a_relation_removed_is_reported_and_breaking() {
        let d = diff(
            "Table Customer { id int [pk]; }\nTable T { id int [pk]; customer_id >Customer.id; }\n",
            "Table Customer { id int [pk]; }\nTable T { id int [pk]; customer_id int; }\n",
        );
        let entry = d.tables.iter().find(|t| t.name == "T").expect("Table T");
        assert!(matches!(
            &entry.change,
            TableChange::RelationRemoved { column } if column == "customer_id"
        ));
        assert!(entry.breaking);
    }

    #[test]
    fn a_retargeted_relation_produces_no_entry() {
        // Presence-only: same field, still a relation on both sides, just
        // pointed at a different table – explicitly out of scope for v1.
        let d = diff(
            "Table A { id int [pk]; }\nTable B { id int [pk]; }\nTable T { id int [pk]; ref_id >A.id; }\n",
            "Table A { id int [pk]; }\nTable B { id int [pk]; }\nTable T { id int [pk]; ref_id >B.id; }\n",
        );
        let entry = d.tables.iter().find(|t| t.name == "T");
        assert!(entry.is_none(), "{d:#?}");
    }

    #[test]
    fn an_index_added_is_reported_and_non_breaking() {
        let d = diff(
            "Table T { id int [pk]; name varchar(50); }\n",
            "Table T { id int [pk]; name varchar(50); Index { ix_name (name); } }\n",
        );
        assert!(matches!(
            &d.tables[0].change,
            TableChange::IndexAdded { index } if index.name.as_deref() == Some("ix_name")
        ));
        assert!(!d.tables[0].breaking);
        assert!(!d.has_breaking_changes);
    }

    #[test]
    fn an_index_removed_is_reported_and_non_breaking() {
        let d = diff(
            "Table T { id int [pk]; name varchar(50); Index { ix_name (name); } }\n",
            "Table T { id int [pk]; name varchar(50); }\n",
        );
        assert!(matches!(
            &d.tables[0].change,
            TableChange::IndexRemoved { index } if index.name.as_deref() == Some("ix_name")
        ));
        assert!(!d.tables[0].breaking);
        assert!(!d.has_breaking_changes);
    }

    #[test]
    fn an_unnamed_index_is_matched_by_its_ordered_column_set() {
        let unchanged = "Table T { id int [pk]; a int; b int; Index { (a, b); } }\n";
        let d = diff(unchanged, unchanged);
        assert!(d.tables.is_empty());

        // Same columns, different order: a different index identity.
        let d = diff(
            "Table T { id int [pk]; a int; b int; Index { (a, b); } }\n",
            "Table T { id int [pk]; a int; b int; Index { (b, a); } }\n",
        );
        assert_eq!(d.tables.len(), 2);
        assert!(
            d.tables
                .iter()
                .any(|t| matches!(&t.change, TableChange::IndexAdded { .. }))
        );
        assert!(
            d.tables
                .iter()
                .any(|t| matches!(&t.change, TableChange::IndexRemoved { .. }))
        );
    }

    const ENUM_HEADER: &str = "Enum Status {\n  Field { type = string; }\n";

    #[test]
    fn an_enum_entry_added_is_reported_and_non_breaking() {
        let d = diff(
            &format!("{ENUM_HEADER}  draft;\n}}\n"),
            &format!("{ENUM_HEADER}  draft;\n  paid;\n}}\n"),
        );
        assert_eq!(d.enums.len(), 1);
        assert!(matches!(
            &d.enums[0].change,
            EnumChange::EntryAdded { entry } if entry == "paid"
        ));
        assert!(!d.enums[0].breaking);
        assert!(!d.has_breaking_changes);
    }

    #[test]
    fn an_enum_entry_removed_is_reported_and_breaking() {
        let d = diff(
            &format!("{ENUM_HEADER}  draft;\n  paid;\n}}\n"),
            &format!("{ENUM_HEADER}  draft;\n}}\n"),
        );
        assert_eq!(d.enums.len(), 1);
        assert!(matches!(
            &d.enums[0].change,
            EnumChange::EntryRemoved { entry } if entry == "paid"
        ));
        assert!(d.enums[0].breaking);
        assert!(d.has_breaking_changes);
    }

    #[test]
    fn the_summary_flag_is_true_when_any_entry_is_breaking() {
        // One breaking table entry (dropped) and one non-breaking enum
        // entry (added) – the flag reflects the table alone.
        let d = diff(
            &format!("Table T {{ id int [pk]; }}\n{ENUM_HEADER}  draft;\n}}\n"),
            &format!("{ENUM_HEADER}  draft;\n  paid;\n}}\n"),
        );
        assert!(d.tables.iter().any(|t| t.breaking));
        assert!(d.enums.iter().all(|e| !e.breaking));
        assert!(d.has_breaking_changes);
    }

    #[test]
    fn the_summary_flag_is_false_when_every_entry_is_non_breaking() {
        let d = diff(
            "Table T { id int [pk]; }\n",
            "Table T { id int [pk]; }\nTable U { id int [pk]; }\n",
        );
        assert!(!d.tables.is_empty());
        assert!(d.tables.iter().all(|t| !t.breaking));
        assert!(!d.has_breaking_changes);
    }

    #[test]
    fn the_summary_flag_is_false_for_an_empty_diff() {
        let body = "Table T { id int [pk]; }\n";
        let d = diff(body, body);
        assert!(d.tables.is_empty());
        assert!(d.enums.is_empty());
        assert!(!d.has_breaking_changes);
    }
}
