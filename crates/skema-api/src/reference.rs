//! Static reference text for the language's own vocabulary: what hover
//! shows for a block keyword the resolved model has nothing to say about
//! (`Document`, `Meta`, `Project`, ...), and what a completion item's
//! `documentation` carries for a block keyword or a flag.
//!
//! The key/flag *lists* here are never hand-copied - they come from
//! [`crate::builtins`]. Only the prose - what a block is for, which blocks
//! typically nest inside it, what a flag means - is authored here,
//! condensed from `docs/src/13-reference.md` ("Block quick reference",
//! "Flag reference").

use skema_syntax::legacy_ast::BlockKind;

use crate::Builtins;

/// The hover/completion card for a block keyword: purpose, where it
/// typically appears, what it typically contains, and - for a settings
/// block - the exact keys `skema-hir` accepts there.
pub fn block_reference_markdown(kind: BlockKind, builtins: &Builtins) -> String {
    let (summary, parents, children) = block_reference(kind);
    let mut out = format!("**{}** · block keyword\n\n{summary}", kind.as_str());

    let mut facts = Vec::new();
    if !parents.is_empty() {
        facts.push(format!("Inside: {}", parents.join(", ")));
    }
    if !children.is_empty() {
        facts.push(format!("Contains: {}", children.join(", ")));
    }
    if let Some(keys) = block_keys(kind, builtins).filter(|k| !k.is_empty()) {
        facts.push(format!("Keys: {}", code_list(keys)));
    }
    append_facts(&mut out, &facts);
    out
}

/// One line for a flag documented in the language reference - `None` for a
/// custom or as-yet-undocumented one, which a caller simply omits rather
/// than showing an empty tooltip.
pub fn flag_reference(name: &str) -> Option<&'static str> {
    Some(match name {
        "pk" => "Primary key. Implies `required` and `unique`.",
        "unique" => "Uniqueness constraint.",
        "required" => "`NOT NULL`.",
        "optional" => "`NULL` allowed - the default, written explicitly.",
        "default" => "Default value; accepts a raw expression (`` `now()` ``).",
        "column" => "Physical column name.",
        "onDelete" => {
            "What happens to this row when the referenced row is deleted. Relationship field, `>` side only."
        }
        "onUpdate" => {
            "What happens to this row when the referenced row's key changes. Relationship field, `>` side only."
        }
        _ => return None,
    })
}

fn code_list(items: &[String]) -> String {
    items
        .iter()
        .map(|i| format!("`{i}`"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn append_facts(out: &mut String, facts: &[String]) {
    if facts.is_empty() {
        return;
    }
    out.push_str("\n\n");
    out.push_str(
        &facts
            .iter()
            .map(|f| format!("- {f}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

/// The catalog list that applies inside a block's own body, if its body is
/// `key = value;` settings rather than declarations.
fn block_keys(kind: BlockKind, builtins: &Builtins) -> Option<&[String]> {
    match kind {
        BlockKind::Meta => Some(&builtins.meta_keys),
        BlockKind::Project => Some(&builtins.project_keys),
        BlockKind::Database => Some(&builtins.database_keys),
        BlockKind::Lint => Some(&builtins.lint_keys),
        BlockKind::Field => Some(&builtins.field_block_keys),
        _ => None,
    }
}

/// Summary, typical parents, typical children - condensed from
/// `docs/src/13-reference.md`'s "Block quick reference".
fn block_reference(
    kind: BlockKind,
) -> (
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
) {
    use BlockKind::{
        Database, Datatype, Dialect, Document, Enum, Example, Field, Group, Index, Lint, Meta,
        Partial, Project, Record, Style, Table,
    };
    match kind {
        Project => (
            "Top-level settings: `mode`, `dialect`, and project-wide `Meta`/`Style`/`Lint`/`Database`. Exactly one per project, in the root file.",
            &[],
            &["Meta", "Style", "Lint", "Database"],
        ),
        Document => (
            "A file's namespace declaration. Exactly one per file.",
            &[],
            &["Meta", "Style"],
        ),
        Table => (
            "A table declaration: fields plus optional `Meta`, `Database`, `Index`, `Example`.",
            &[],
            &["Meta", "Database", "Index", "Example"],
        ),
        Partial => (
            "A reusable group of fields, inserted elsewhere with `use`. Never becomes a table of its own.",
            &[],
            &["Meta", "Index"],
        ),
        Enum => (
            "A closed, ordered set of named values with a `Field` block describing storage.",
            &[],
            &["Field", "Meta"],
        ),
        Group => (
            "A named, cross-file collection of `Table`/`Enum` references, for diagrams and documentation. No effect on the generated schema.",
            &[],
            &["Meta"],
        ),
        Datatype => (
            "A user-defined type: parameters, a `Field` block, and per-engine `Dialect` blocks.",
            &[],
            &["Field", "Dialect"],
        ),
        Meta => (
            "Descriptive metadata - author, note, version, style classes, and (Table only) movedFrom for the planned schema diff. Never affects the generated schema.",
            &[
                "Project", "Document", "Table", "Partial", "Enum", "Group", "Datatype",
            ],
            &[],
        ),
        Style => (
            "Visual settings. `@name` selects a built-in element type; a bare name defines a reusable class.",
            &["Project", "Document"],
            &[],
        ),
        Index => (
            "A table's or partial's list of indexes.",
            &["Table", "Partial"],
            &[],
        ),
        Database => (
            "Physical names: `database`, `schema`, `table`.",
            &["Project", "Document", "Table"],
            &["Dialect"],
        ),
        Field => (
            "Inside `Datatype`/`Enum`: describes how a field of that type behaves at the model level.",
            &["Datatype", "Enum"],
            &[],
        ),
        Dialect => (
            "Engine-specific settings, named `@postgres`/`@mysql`/etc.",
            &["Database", "Datatype"],
            &[],
        ),
        Lint => (
            "Project-wide naming/consistency rules, enforced only under `mode = strict`.",
            &["Project"],
            &[],
        ),
        Example => (
            "A table's sample-data container: any number of `Record` blocks. Descriptive only - never affects generated DDL/DML.",
            &["Table"],
            &["Record"],
        ),
        Record => (
            "One sample row inside `Example`: `field = value;` assignments, checked against the table's own field names.",
            &["Example"],
            &[],
        ),
        // Callers are expected to guard against `Other` themselves - see
        // `skema-lsp`'s `keyword_at`, which never returns it as a hit - but
        // the match still has to be exhaustive.
        BlockKind::Other => ("An unrecognized block name.", &[], &[]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settings_block_lists_its_keys() {
        let builtins = crate::builtins();
        let markdown = block_reference_markdown(BlockKind::Meta, &builtins);
        assert!(markdown.contains("`author`"), "{markdown}");
        assert!(markdown.contains("Inside: Project"), "{markdown}");
    }

    #[test]
    fn a_declaration_block_lists_its_children_not_keys() {
        let builtins = crate::builtins();
        let markdown = block_reference_markdown(BlockKind::Table, &builtins);
        assert!(
            markdown.contains("Contains: Meta, Database, Index"),
            "{markdown}"
        );
        assert!(!markdown.contains("Keys:"), "{markdown}");
    }

    #[test]
    fn every_block_kind_has_a_reference_entry() {
        // A future `BlockKind` variant not matched here is a compile error
        // in `block_reference`'s exhaustive match, not a silent gap - this
        // just exercises every arm once.
        for kind in [
            BlockKind::Project,
            BlockKind::Document,
            BlockKind::Table,
            BlockKind::Partial,
            BlockKind::Enum,
            BlockKind::Group,
            BlockKind::Datatype,
            BlockKind::Meta,
            BlockKind::Style,
            BlockKind::Index,
            BlockKind::Database,
            BlockKind::Field,
            BlockKind::Dialect,
            BlockKind::Lint,
            BlockKind::Example,
            BlockKind::Record,
        ] {
            assert!(!block_reference_markdown(kind, &crate::builtins()).is_empty());
        }
    }

    #[test]
    fn a_documented_flag_has_a_one_liner() {
        assert!(flag_reference("pk").is_some());
        assert!(flag_reference("x-custom").is_none());
    }
}
