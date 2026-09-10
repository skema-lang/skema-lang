//! `tableMetas`: every `Table`'s own `Meta` block, read straight off the
//! raw AST, with the offsets an editor needs to change one entry without
//! touching anything else in the file.
//!
//! **Why the raw AST, not the resolved model**: `skema-hir` validates `Meta`
//! keys but never copies them into `model::Table`, and the custom `x-…` keys
//! this is mostly about (`docs/01`, "Custom keys") are by definition outside
//! the checked model. The syntax tree is where they survive.
//!
//! **Why UTF-16 offsets**, unlike the model's byte `span`s: this drives
//! *edits* in a browser editor, so it speaks the editor's coordinates. The
//! model's spans describe the model; these describe the text about to
//! change. [`Utf16Range`] names the difference so call sites cannot confuse
//! them.
//!
//! One parse covers every table in the file — reading N tables' positions on
//! load should not cost N parses.

use serde::Serialize;
use skema_syntax::ast::{self, BlockKind};
#[cfg(feature = "typescript")]
use tsify_next::Tsify;

use crate::location::Utf16Offsets;

/// A half-open range in **UTF-16 code units** – directly usable as a
/// CodeMirror/Monaco document position pair.
#[derive(Debug, Clone, Copy, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
pub struct Utf16Range {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct MetaEntry {
    /// The key as written, without the quotes a custom `"x-…"` key carries.
    pub key: String,
    /// The value's content (`ast::Value::to_text`) – a string without its
    /// quotes, a raw expression without its backticks. `None` for a
    /// valueless entry, which is a `[]`-flag shape and an error inside
    /// `{}` – reported by `skema-hir`, not by this reader.
    pub value: Option<String>,
    pub key_range: Utf16Range,
    /// The value **as written**, quotes included: replacing exactly this
    /// range (with a freshly quoted string) changes the value and nothing
    /// else – no reformatting, no touched neighbours.
    pub value_range: Option<Utf16Range>,
    /// The whole `key = value` pair, without the terminating `;`.
    pub entry_range: Utf16Range,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct TableMeta {
    pub name: String,
    /// Right past the `Table`'s own `{` – where a whole new `Meta { … }`
    /// block goes when the table has none yet.
    pub table_body_start: u32,
    /// Right past the `Meta` block's `{`, when the table already has one –
    /// where a new key goes into the existing block.
    pub meta_body_start: Option<u32>,
    pub entries: Vec<MetaEntry>,
}

/// A named result type at the boundary, like `CheckResult`/`ExportResult`
/// elsewhere in this crate – `tsify` implements the wasm ABI for a struct,
/// not for a bare `Vec` of one.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[cfg_attr(feature = "typescript", tsify(into_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct TableMetas {
    pub tables: Vec<TableMeta>,
}

/// Every `Table` in `source`, with its `Meta` entries and the offsets to
/// edit them at. Tables without a `Meta` block are included too (with
/// `metaBodyStart: undefined` and no entries) – the caller still needs
/// `tableBodyStart` to add one.
pub fn table_metas(source: &str) -> TableMetas {
    let (file, _) = skema_syntax::parse(source);
    let offsets = Utf16Offsets::new(source);
    let tables = decl_metas(&file, &offsets, BlockKind::Table)
        .into_iter()
        .map(|d| TableMeta {
            name: d.name,
            table_body_start: d.body_start,
            meta_body_start: d.meta_body_start,
            entries: d.entries,
        })
        .collect();
    TableMetas { tables }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct EnumMeta {
    pub name: String,
    /// Right past the `Enum`'s own `{` – see [`TableMeta::table_body_start`].
    pub enum_body_start: u32,
    pub meta_body_start: Option<u32>,
    pub entries: Vec<MetaEntry>,
}

/// See [`TableMetas`].
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[cfg_attr(feature = "typescript", tsify(into_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct EnumMetas {
    pub enums: Vec<EnumMeta>,
}

/// Every `Enum` in `source`, with its `Meta` entries – the same shape as
/// [`table_metas`], for the declaration kind that needs it next: an `Enum`
/// gets rendered in a diagram and can carry `Meta.styleClasses` (`docs/03`)
/// just like a `Table` can.
pub fn enum_metas(source: &str) -> EnumMetas {
    let (file, _) = skema_syntax::parse(source);
    let offsets = Utf16Offsets::new(source);
    let enums = decl_metas(&file, &offsets, BlockKind::Enum)
        .into_iter()
        .map(|d| EnumMeta {
            name: d.name,
            enum_body_start: d.body_start,
            meta_body_start: d.meta_body_start,
            entries: d.entries,
        })
        .collect();
    EnumMetas { enums }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct GroupMeta {
    pub name: String,
    /// Right past the `Group`'s own `{` – see [`TableMeta::table_body_start`].
    pub group_body_start: u32,
    pub meta_body_start: Option<u32>,
    pub entries: Vec<MetaEntry>,
}

/// See [`TableMetas`].
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(Tsify))]
#[cfg_attr(feature = "typescript", tsify(into_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct GroupMetas {
    pub groups: Vec<GroupMeta>,
}

/// Every `Group` in `source`, with its `Meta` entries – the same shape as
/// [`table_metas`]. A `Group`'s own diagram position (once it gets one, as a
/// draggable frame) is just another `x-editorPosition` entry here, exactly
/// like a `Table`'s – no separate mechanism needed.
pub fn group_metas(source: &str) -> GroupMetas {
    let (file, _) = skema_syntax::parse(source);
    let offsets = Utf16Offsets::new(source);
    let groups = decl_metas(&file, &offsets, BlockKind::Group)
        .into_iter()
        .map(|d| GroupMeta {
            name: d.name,
            group_body_start: d.body_start,
            meta_body_start: d.meta_body_start,
            entries: d.entries,
        })
        .collect();
    GroupMetas { groups }
}

/// What every `*_metas` function above needs per declaration – factored out
/// because `table_metas`/`enum_metas`/`group_metas` are otherwise the same
/// loop three times over, differing only in which `BlockKind` they filter
/// for and what the caller wants the offset field called.
struct RawDeclMeta {
    name: String,
    body_start: u32,
    meta_body_start: Option<u32>,
    entries: Vec<MetaEntry>,
}

fn decl_metas(file: &ast::SourceFile, offsets: &Utf16Offsets, kind: BlockKind) -> Vec<RawDeclMeta> {
    file.blocks()
        .filter(|b| b.kind == kind)
        .filter_map(|decl| {
            let name = decl.name.as_ref()?.as_ident()?.text.clone();
            let meta = decl.child(BlockKind::Meta);
            Some(RawDeclMeta {
                name,
                body_start: offsets.convert(decl.body_start()),
                meta_body_start: meta.map(|m| offsets.convert(m.body_start())),
                entries: meta.map(|m| entries(m, offsets)).unwrap_or_default(),
            })
        })
        .collect()
}

pub(crate) fn entries(meta: &ast::Block, offsets: &Utf16Offsets) -> Vec<MetaEntry> {
    meta.items
        .iter()
        .filter_map(|item| match &item.kind {
            ast::ItemKind::Setting(a) => Some(MetaEntry {
                key: a.key.text.clone(),
                value: a.value.as_ref().map(ast::Value::to_text),
                key_range: range(offsets, a.key.span),
                value_range: a.value.as_ref().map(|v| range(offsets, v.span)),
                entry_range: range(offsets, a.span),
            }),
            _ => None,
        })
        .collect()
}

pub(crate) fn range(offsets: &Utf16Offsets, span: skema_syntax::Span) -> Utf16Range {
    Utf16Range {
        start: offsets.convert(span.start),
        end: offsets.convert(span.end),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_without_meta_still_reports_where_one_would_go() {
        let src = "Table Customer { id int [pk]; }";
        let metas = table_metas(src).tables;
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].name, "Customer");
        assert!(metas[0].meta_body_start.is_none());
        assert!(metas[0].entries.is_empty());
        // Right past `Table Customer {`.
        assert_eq!(
            &src[metas[0].table_body_start as usize..],
            " id int [pk]; }"
        );
    }

    #[test]
    fn existing_custom_key_is_readable_and_replaceable_in_place() {
        let src = r#"Table Customer { Meta { "x-editorPosition" = "120,340"; } }"#;
        let metas = table_metas(src).tables;
        let entry = &metas[0].entries[0];
        assert_eq!(entry.key, "x-editorPosition");
        assert_eq!(entry.value.as_deref(), Some("120,340"));

        // The value range covers the quotes, so swapping in a freshly
        // quoted string is the whole edit.
        let value_range = entry.value_range.expect("a value was written");
        assert_eq!(
            &src[value_range.start as usize..value_range.end as usize],
            r#""120,340""#
        );
        let edited = format!(
            "{}{}{}",
            &src[..value_range.start as usize],
            r#""200,400""#,
            &src[value_range.end as usize..]
        );
        assert_eq!(
            edited,
            r#"Table Customer { Meta { "x-editorPosition" = "200,400"; } }"#
        );
    }

    #[test]
    fn meta_body_start_is_the_insertion_point_for_a_new_key() {
        let src = r#"Table Customer { Meta { author = "anna"; } }"#;
        let metas = table_metas(src).tables;
        let at = metas[0].meta_body_start.expect("table has a Meta block") as usize;
        let edited = format!(
            r#"{} "x-editorPosition" = "1,2";{}"#,
            &src[..at],
            &src[at..]
        );
        assert_eq!(
            edited,
            r#"Table Customer { Meta { "x-editorPosition" = "1,2"; author = "anna"; } }"#
        );
    }

    #[test]
    fn offsets_stay_correct_after_a_multibyte_character() {
        // The note's "ü" is 2 UTF-8 bytes but 1 UTF-16 unit: an editor
        // offset must count it once, or every later edit lands one
        // character off.
        let src = r#"Table T { Meta { note = "grün"; "x-editorPosition" = "1,2"; } }"#;
        let metas = table_metas(src).tables;
        let position = metas[0]
            .entries
            .iter()
            .find(|e| e.key == "x-editorPosition")
            .expect("the position entry");
        let utf16: Vec<u16> = src.encode_utf16().collect();
        let range = position.value_range.expect("a value was written");
        let slice = String::from_utf16_lossy(&utf16[range.start as usize..range.end as usize]);
        assert_eq!(slice, r#""1,2""#);
    }

    #[test]
    fn enum_without_meta_still_reports_where_one_would_go() {
        let src = "Enum Status { draft; paid; }";
        let metas = enum_metas(src).enums;
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].name, "Status");
        assert!(metas[0].meta_body_start.is_none());
        assert!(metas[0].entries.is_empty());
        assert_eq!(&src[metas[0].enum_body_start as usize..], " draft; paid; }");
    }

    #[test]
    fn enum_meta_entries_are_readable() {
        let src = r#"Enum Status { Meta { styleClasses = [highlight]; } draft; }"#;
        let metas = enum_metas(src).enums;
        assert_eq!(metas[0].entries[0].key, "styleClasses");
        assert_eq!(metas[0].entries[0].value.as_deref(), Some("highlight"));
    }

    #[test]
    fn group_without_meta_still_reports_where_one_would_go() {
        let src = "Group Core { Table Customer; }";
        let metas = group_metas(src).groups;
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].name, "Core");
        assert!(metas[0].meta_body_start.is_none());
        assert!(metas[0].entries.is_empty());
        assert_eq!(
            &src[metas[0].group_body_start as usize..],
            " Table Customer; }"
        );
    }

    #[test]
    fn group_meta_entries_are_readable() {
        let src = r#"Group Core { Meta { "x-editorPosition" = "10,20"; } }"#;
        let metas = group_metas(src).groups;
        assert_eq!(metas[0].entries[0].key, "x-editorPosition");
        assert_eq!(metas[0].entries[0].value.as_deref(), Some("10,20"));
    }
}
