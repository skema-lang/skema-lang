//! Writing a diagram position back into the model's own source.
//!
//! A position lives in the declaration it belongs to, as a custom key
//! (`docs/01`, "Custom keys": quoted, `x-` prefixed, passed through
//! unchanged):
//!
//! ```skema
//! Table Customer {
//!   Meta { "x-editorPosition" = "120,340"; }
//! }
//! ```
//!
//! So a layout travels with the file, shows up in a diff, and survives being
//! opened in a different tool — unlike one kept in `localStorage` or an
//! editor's workspace state.
//!
//! # Why this is here and not in the front ends
//!
//! It is the same edit everywhere: a browser editor dragging a table, an LSP
//! client dragging it in a webview. Both must produce the *smallest* edit —
//! replace a value, add an entry, or add a whole `Meta` block — from the
//! same reader ([`crate::meta`]) that produced the offsets. Two copies would
//! disagree the first time the language grows another way to write a `Meta`
//! entry.
//!
//! # The one rule for callers
//!
//! `source` must be the file's **current** text. The returned UTF-16 range
//! belongs to exactly that revision and one keystroke before it invalidates
//! it. Read, build the edit, dispatch — without awaiting in between.

use serde::{Deserialize, Serialize};
#[cfg(feature = "typescript")]
use tsify_next::Tsify;

use crate::meta::{MetaEntry, Utf16Range, enum_metas, group_metas, table_metas};

/// The `x-` key a position is stored under. Tool metadata by the language's
/// own mechanism, not an extension to it.
pub const POSITION_KEY: &str = "x-editorPosition";

/// Which kind of declaration a position belongs to. A `Table`, an `Enum` and
/// a `Group` all carry theirs the same way; only the reader differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[cfg_attr(feature = "typescript", tsify(from_wasm_abi))]
#[serde(rename_all = "lowercase")]
pub enum DeclKind {
    Table,
    Enum,
    Group,
}

/// A replacement of one UTF-16 range with one string - the whole vocabulary
/// needed to record a position, in the coordinates an editor uses.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[cfg_attr(feature = "typescript", tsify(into_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub range: Utf16Range,
    pub insert: String,
}

/// Reads a stored position back, if the declaration carries one.
///
/// A malformed value is `None` rather than an error: `x-` keys are passed
/// through unchanged by the language, so anything at all may be written
/// there, and a diagram's answer to "where does this table sit" is simply
/// "nowhere stored yet".
pub fn read_position(entries: &[MetaEntry]) -> Option<(f64, f64)> {
    let value = entries
        .iter()
        .find(|e| e.key == POSITION_KEY)?
        .value
        .as_deref()?;
    let (x, y) = value.split_once(',')?;
    let x: f64 = x.trim().parse().ok()?;
    let y: f64 = y.trim().parse().ok()?;
    (x.is_finite() && y.is_finite()).then_some((x, y))
}

/// The smallest edit that records `(x, y)` for `name`, or `None` when the
/// file declares no such thing.
pub fn position_edit(source: &str, kind: DeclKind, name: &str, x: f64, y: f64) -> Option<TextEdit> {
    let target = match kind {
        DeclKind::Table => table_metas(source)
            .tables
            .into_iter()
            .find(|m| m.name == name)
            .map(|m| Target {
                body_start: m.table_body_start,
                meta_body_start: m.meta_body_start,
                entries: m.entries,
            }),
        DeclKind::Enum => enum_metas(source)
            .enums
            .into_iter()
            .find(|m| m.name == name)
            .map(|m| Target {
                body_start: m.enum_body_start,
                meta_body_start: m.meta_body_start,
                entries: m.entries,
            }),
        DeclKind::Group => group_metas(source)
            .groups
            .into_iter()
            .find(|m| m.name == name)
            .map(|m| Target {
                body_start: m.group_body_start,
                meta_body_start: m.meta_body_start,
                entries: m.entries,
            }),
    }?;

    Some(target.edit(x, y))
}

/// What the edit needs from a declaration, whichever kind it is.
struct Target {
    /// Right past the declaration's own `{`.
    body_start: u32,
    meta_body_start: Option<u32>,
    entries: Vec<MetaEntry>,
}

impl Target {
    fn edit(&self, x: f64, y: f64) -> TextEdit {
        let value = format!("\"{},{}\"", x.round() as i64, y.round() as i64);
        let existing = self.entries.iter().find(|e| e.key == POSITION_KEY);

        // Three cases, cheapest first: replace a value, add an entry to the
        // `Meta` block that is already there, or add the block itself.
        if let Some(entry) = existing {
            if let Some(range) = entry.value_range {
                return TextEdit {
                    range,
                    insert: value,
                };
            }
            // An entry with no value (`"x-editorPosition" = ;`) is invalid,
            // and is exactly what a half-typed one looks like the moment
            // after the `=`. Replacing the whole entry writes a valid one;
            // replacing a value range that does not exist could not.
            return TextEdit {
                range: entry.entry_range,
                insert: format!("\"{POSITION_KEY}\" = {value}"),
            };
        }

        if let Some(start) = self.meta_body_start {
            return TextEdit {
                range: Utf16Range { start, end: start },
                insert: format!("\n    \"{POSITION_KEY}\" = {value};"),
            };
        }

        TextEdit {
            range: Utf16Range {
                start: self.body_start,
                end: self.body_start,
            },
            insert: format!("\n  Meta {{ \"{POSITION_KEY}\" = {value}; }}\n"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies the edit the way a client does, so a test asserts on the
    /// resulting text rather than on offsets - which is what actually has to
    /// be right, and stays readable when the reader's offsets change.
    fn apply(source: &str, kind: DeclKind, name: &str, x: f64, y: f64) -> String {
        let edit = position_edit(source, kind, name, x, y).expect("an edit for a declared name");
        let units: Vec<u16> = source.encode_utf16().collect();
        let head = String::from_utf16_lossy(&units[..edit.range.start as usize]);
        let tail = String::from_utf16_lossy(&units[edit.range.end as usize..]);
        format!("{head}{}{tail}", edit.insert)
    }

    #[test]
    fn replaces_only_the_value_of_an_existing_entry() {
        let source = r#"Table Customer { Meta { "x-editorPosition" = "1,2"; note = "keep"; } }"#;
        assert_eq!(
            apply(source, DeclKind::Table, "Customer", 120.0, 340.0),
            r#"Table Customer { Meta { "x-editorPosition" = "120,340"; note = "keep"; } }"#
        );
    }

    #[test]
    fn adds_an_entry_to_an_existing_meta_block() {
        let source = "Table Customer {\n  Meta {\n    note = \"hi\";\n  }\n}";
        let after = apply(source, DeclKind::Table, "Customer", 10.0, 20.0);
        assert!(
            after.contains(r#""x-editorPosition" = "10,20";"#),
            "got: {after}"
        );
        assert!(after.contains(r#"note = "hi";"#), "the note survives");
    }

    #[test]
    fn adds_a_meta_block_to_a_declaration_without_one() {
        let source = "Table Customer {\n  id int [pk];\n}";
        let after = apply(source, DeclKind::Table, "Customer", 10.0, 20.0);
        assert!(
            after.contains(r#"Meta { "x-editorPosition" = "10,20"; }"#),
            "got: {after}"
        );
        assert!(after.contains("id int [pk];"), "the column survives");
    }

    /// Dragging a table in the moment after `=` was typed and before a value
    /// was: the entry exists, carries no value, and has no value range to
    /// replace. The whole entry is rewritten, so the file comes out valid
    /// rather than with a second entry beside a broken one.
    ///
    /// The entry range stops at the `=`, so whatever sat between it and the
    /// `;` stays - one space, here. Valid skema, and not worth a special
    /// case: the next drag replaces the value range alone and it is gone.
    #[test]
    fn an_entry_without_a_value_is_replaced_whole_rather_than_left_broken() {
        let source = r#"Table Customer { Meta { "x-editorPosition" = ; } }"#;
        assert_eq!(
            apply(source, DeclKind::Table, "Customer", 3.0, 4.0),
            r#"Table Customer { Meta { "x-editorPosition" = "3,4" ; } }"#
        );
    }

    /// The neighbouring case, which takes a *different* branch: a key with no
    /// `=` at all is not reported as a `Meta` entry, so nothing is found to
    /// replace and a fresh entry is added beside it. The file was already
    /// invalid and stays that way - a drag is not the place to guess at
    /// repairing someone's half-written line.
    #[test]
    fn a_key_written_without_an_equals_is_not_an_entry_to_replace() {
        let source = r#"Table Customer { Meta { "x-editorPosition"; } }"#;
        let after = apply(source, DeclKind::Table, "Customer", 3.0, 4.0);
        assert!(
            after.contains(r#""x-editorPosition" = "3,4";"#),
            "got: {after}"
        );
        assert!(
            after.contains(r#""x-editorPosition";"#),
            "the broken line is left alone, not silently rewritten: {after}"
        );
    }

    #[test]
    fn enums_and_groups_store_theirs_the_same_way() {
        let source = "Enum Status { draft; }\nGroup Core { Table Customer; }";
        assert!(
            apply(source, DeclKind::Enum, "Status", 1.0, 2.0)
                .contains(r#"Meta { "x-editorPosition" = "1,2"; }"#)
        );
        assert!(
            apply(source, DeclKind::Group, "Core", 5.0, 6.0)
                .contains(r#"Meta { "x-editorPosition" = "5,6"; }"#)
        );
    }

    #[test]
    fn an_undeclared_name_has_no_edit() {
        assert!(position_edit("Table A {}", DeclKind::Table, "B", 0.0, 0.0).is_none());
    }

    #[test]
    fn positions_round_to_whole_units() {
        let source = "Table A {}";
        assert!(apply(source, DeclKind::Table, "A", 10.4, -3.6).contains(r#""10,-4""#));
    }

    #[test]
    fn a_stored_position_reads_back() {
        let source = r#"Table A { Meta { "x-editorPosition" = " 12 , 34 "; } }"#;
        let metas = table_metas(source).tables;
        let entries = &metas.first().expect("A").entries;
        assert_eq!(read_position(entries), Some((12.0, 34.0)));
    }

    #[test]
    fn a_malformed_position_reads_as_none() {
        for value in ["nope", "1", "1,x"] {
            let source = format!(r#"Table A {{ Meta {{ "x-editorPosition" = "{value}"; }} }}"#);
            let metas = table_metas(&source).tables;
            let entries = &metas.first().expect("A").entries;
            assert_eq!(read_position(entries), None, "value: {value}");
        }
    }

    /// Offsets are UTF-16, so a multi-byte character before the edit must not
    /// shift it - the bug this whole coordinate choice exists to prevent.
    #[test]
    fn offsets_survive_multibyte_text_before_the_edit() {
        let source = r#"Table A { Meta { note = "üü🙂"; "x-editorPosition" = "1,2"; } }"#;
        assert_eq!(
            apply(source, DeclKind::Table, "A", 7.0, 8.0),
            r#"Table A { Meta { note = "üü🙂"; "x-editorPosition" = "7,8"; } }"#
        );
    }
}
