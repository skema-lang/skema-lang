//! The abstract syntax tree (AST) of the `.skema` language.
//!
//! # Few nodes, lots of reuse
//!
//! Almost every line in the language has the same shape:
//! `Name [Type] [flags] { Body }`. Five node kinds cover all of it, instead
//! of one per keyword.
//!
//! | Node | Covers |
//! |---|---|
//! | [`Block`] | `Project`, `Table`, `Meta`, `Style @table`, `Dialect`, … |
//! | [`Field`] | table fields, enum values, `Dialect` sub-entries |
//! | [`Attr`] | `mode = draft;` **and** `[pk]` **and** `[default=12]` |
//! | [`Value`] | every value, wherever it appears |
//! | [`Ref`] | `Table shop.User;` inside a `Group` |
//!
//! # Form, not meaning
//!
//! The AST allows an `Index` block inside an `Enum`, or a named `Project`.
//! Deliberately: "`Lint` may only appear inside `Project`" is then a clear
//! message from `skema-hir` instead of a syntax error that drags down the
//! rest of the file. The tree stays complete, so completion and
//! go-to-definition keep working, and one body parser serves all blocks.

use crate::{Span, TextSize};

// ===========================================================================
// Building blocks
// ===========================================================================

/// A name in the source text.
///
/// The text is held **by value**, unlike in [`crate::Token`]: a `&'a str`
/// would tie the tree to the source buffer, and a language server keeps the
/// tree precisely when that buffer has already changed. Swappable for an
/// interned type (`SmolStr`) later without touching the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub text: String,
    pub span: Span,
}

/// A dot-separated name: `email`, `shop.OrderStatus`, `shop.Order.id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    pub segments: Vec<Ident>,
    pub span: Span,
}

impl Path {
    /// The name, if it is **not** qualified.
    ///
    /// Declarations (`Table Customer`) need a plain name, references
    /// (`Table shop.User`) may be qualified. One parser function reads both;
    /// the distinction is made here.
    pub fn as_ident(&self) -> Option<&Ident> {
        match self.segments.as_slice() {
            [only] => Some(only),
            _ => None,
        }
    }

    /// `shop.Order.id` — for error messages and debug output.
    pub fn to_dotted(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(".")
    }
}

/// The key of an assignment.
///
/// `quoted` distinguishes the two forms from `docs/01`, "Custom keys":
///
/// ```text
/// author   = "anna";       # known key, unquoted
/// "x-jira" = "SHOP-412";   # custom key, quoted and x- prefixed
/// ```
///
/// The parser records what was written; whether `athor` is a typo is the
/// checking phase's call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub text: String,
    pub quoted: bool,
    pub span: Span,
}

/// A value — the same type everywhere in the language.
#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    pub kind: ValueKind,
    pub span: Span,
}

impl Value {
    /// The value as plain text: a string without its quotes, a raw
    /// expression without its backticks, a list comma-joined.
    ///
    /// Not `dump::value`, which keeps quotes and backticks so a debug dump
    /// stays unambiguous. This is the content form, used by the model export
    /// (`skema-hir`) and by `Meta` readers (`skema-wasm`) alike.
    pub fn to_text(&self) -> String {
        match &self.kind {
            ValueKind::Path(p) => p.to_dotted(),
            ValueKind::Str(s) => s.clone(),
            ValueKind::Int(n) => n.to_string(),
            ValueKind::Float(f) => f.to_string(),
            ValueKind::Raw(r) => r.clone(),
            ValueKind::List(items) => items
                .iter()
                .map(Value::to_text)
                .collect::<Vec<_>>()
                .join(", "),
            ValueKind::Type { path, args } => format!(
                "{}({})",
                path.to_dotted(),
                args.iter()
                    .map(Value::to_text)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValueKind {
    /// `draft`, `CASCADE`, `shop.OrderStatus.paid`
    ///
    /// Whether `draft` is an enum value, a mode or a typo is name
    /// resolution's call, not the parser's.
    Path(Path),
    Str(String),
    Int(i64),
    Float(f64),
    /// Content between backticks, **without** the backticks. Passed through
    /// to the target database unchanged.
    Raw(String),
    /// `[highlight, important]`
    List(Vec<Value>),
    /// `decimal(10)` — a type with arguments, used as a value.
    ///
    /// One place only: `type = decimal(10);` in a `Datatype`'s `Field`
    /// block, where a type layers on another (`docs/06`). Structurally
    /// [`FieldTypeKind::Named`], read as a value instead of a field type.
    Type {
        path: Path,
        args: Vec<Value>,
    },
}

/// An assignment — in `{}` as well as in `[]`.
///
/// The language separates what the two *mean* (`[]` produces DDL, `{}`
/// documentation — `docs/01`, "Brackets and braces"), but their *shape* is
/// identical: key, optional `=`, value. One type, one parser function.
///
/// ```text
/// mode = draft;          Attr { key: "mode",     value: Some(draft) }
/// [pk]                   Attr { key: "pk",       value: None }
/// [default=`now()`]      Attr { key: "default",  value: Some(Raw) }
/// ```
///
/// `value: None` only makes sense in `[]`; in `{}` the checking phase
/// reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct Attr {
    pub key: Key,
    pub value: Option<Value>,
    /// `key = value [error]` — a per-setting severity override. Set only
    /// when this `Attr` came from a plain setting (`parse_setting`); a
    /// field's `[flags]`, an index entry's `[flags]`, and a `Datatype`
    /// parameter always leave it `None`, since a bracket already means
    /// something else there.
    pub severity_override: Option<Ident>,
    pub span: Span,
}

// ===========================================================================
// Blocks
// ===========================================================================

/// Generates [`BlockKind`] plus its name table from a single list.
///
/// Without the macro every keyword would appear in three places (variant,
/// `from_keyword`, `as_str`) and the third is easy to forget. Variants are
/// named exactly like the keywords, so `stringify!` yields the spelling.
macro_rules! block_kinds {
    ($($v:ident),* $(,)?) => {
        /// Which block is it?
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum BlockKind {
            $($v,)*
            /// A block name the language doesn't know.
            ///
            /// Not a parser error: `Dialect { postgres { … } }` has freely
            /// named sub-blocks by design. The checking phase decides
            /// whether the name is allowed there.
            Other,
        }

        impl BlockKind {
            /// PascalCase word → block kind. `None` for everything else.
            pub fn from_keyword(text: &str) -> Option<Self> {
                match text {
                    $(stringify!($v) => Some(Self::$v),)*
                    _ => None,
                }
            }

            /// The canonical spelling, for error messages.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$v => stringify!($v),)*
                    Self::Other => "<unknown>",
                }
            }
        }
    };
}

block_kinds!(
    // Declarations – appear at the top level and (mostly) have a name
    Project, Document, Table, Partial, Enum, Group, Datatype,
    // Settings blocks – only appear inside other blocks
    //
    // `Field` here is the settings block that says how a field of a type
    // behaves, in `Datatype` (`docs/06`) and `Enum` (`docs/08`). It is not
    // the [`Field`] AST node below — the language reuses the word.
    Meta, Style, Index, Database, Field, Dialect, Lint, Example, Record,
);

impl BlockKind {
    /// May this block appear at the top level?
    ///
    /// A property of the language, not a rule about a model — so it lives
    /// here, not in the checking phase.
    pub const fn is_declaration(self) -> bool {
        matches!(
            self,
            Self::Project
                | Self::Document
                | Self::Table
                | Self::Partial
                | Self::Enum
                | Self::Group
                | Self::Datatype
        )
    }
}

/// A block with a `{ … }` body.
///
/// Covers every level: `items` holds [`Item`]s, which may be `Block`s again.
///
/// ```text
/// Table Customer                    { … }   name = Customer
/// Project                           { … }   name = None
/// Style @table                      { … }   selector = table
/// Datatype decimal(precision int, scale int) { … }
///                                            name = decimal
///                                            params = [precision: int, scale: int]
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// The word as written — even if it's unknown (`postgres`).
    pub keyword: Ident,
    pub kind: BlockKind,
    /// `Style @table` — the built-in element type behind the `@`.
    pub selector: Option<Ident>,
    /// The name of the declaration. A `Path`, so `Table shop.User` takes
    /// the same route; the checking phase decides whether a dot is allowed.
    pub name: Option<Path>,
    /// The parameters of a `Datatype`: `decimal(precision int, scale int)`.
    pub params: Vec<Param>,
    /// The `{` that opens the body.
    ///
    /// Always present: without a body the same syntax parses as a [`Ref`]
    /// (`grammar::parse_block_or_ref`).
    ///
    /// Recorded because only the parser ever sees this token. [`Item::span`]
    /// covers keyword-through-`}` and says nothing about where the *body*
    /// starts, so anything inserting into a block (an editor adding a
    /// `Meta { … }`, a formatter, a code action) would have to re-lex.
    pub body_brace: Span,
    pub items: Vec<Item>,
}

impl Block {
    /// The first direct sub-block of a given kind, e.g. the `Meta` inside a
    /// `Table` or the `Lint` inside a `Project`.
    pub fn child(&self, kind: BlockKind) -> Option<&Block> {
        self.items.iter().find_map(|item| match &item.kind {
            ItemKind::Block(b) if b.kind == kind => Some(b),
            _ => None,
        })
    }

    /// Byte offset just past the opening `{` — where this block's body
    /// content begins, and thus where new content can be inserted.
    pub const fn body_start(&self) -> TextSize {
        self.body_brace.end
    }
}

/// A parameter of a `Datatype`: `precision int` in
/// `Datatype decimal(precision int, scale int) { … }`.
///
/// Parameters are declared like fields — name, then type (`docs/06`). `ty`
/// is optional because a half-typed line is the normal case in an editor.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: Ident,
    pub ty: Option<Path>,
    pub span: Span,
}

/// A reference without a body: `Table shop.User;` inside a `Group`.
///
/// Syntactically the start of a [`Block`], ending in `;` instead of `{` —
/// so one parser function reads both and branches at the end.
#[derive(Debug, Clone, PartialEq)]
pub struct Ref {
    pub keyword: Ident,
    pub kind: BlockKind,
    pub path: Path,
}

// ===========================================================================
// Fields
// ===========================================================================

/// A named entry in the body of a block.
///
/// Covers three things, because they share one shape:
///
/// ```text
/// email varchar [unique] { note = "…" };   table field
/// draft { label = "Draft" }                enum value (no type)
/// cancelled;                               enum value (no type, no body)
/// ```
///
/// A separate `Variant` node would be an exact copy with `ty: None`.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: Ident,
    pub ty: Option<FieldType>,
    /// `[pk, required, default=0]` — changes the generated schema.
    pub flags: Vec<Attr>,
    /// `{ note = "…" }` — descriptive only. Empty if no block was written.
    pub items: Vec<Item>,
}

/// What comes after the field name.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldType {
    pub kind: FieldTypeKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldTypeKind {
    /// `varchar`, `decimal(10, 2)`, `shop.OrderStatus`
    ///
    /// Data type or enum is not visible in the syntax — name resolution
    /// settles it.
    Named { path: Path, args: Vec<Value> },
    /// `>Customer.id`, `<>Tag.id`
    Relation { op: RelationOp, target: Path },
}

/// The cardinality of a relationship.
///
/// Three values; the language has no many-to-many sign (`docs/07`). SQL
/// needs a join table for that anyway, so it is written into the model
/// instead of hidden behind an operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelationOp {
    /// `>` — many `A`s belong to one `B`; the column is created on `A`.
    OneToMany,
    /// `<` — a back-reference, creates no column.
    ManyToOne,
    /// `-` — one-to-one; the column is created wherever the line is
    /// written, plus `UNIQUE`.
    OneToOne,
}

impl RelationOp {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OneToMany => ">",
            Self::ManyToOne => "<",
            Self::OneToOne => "-",
        }
    }
}

/// An entry in the `Index` block: `[name] (column, …) [flags];`
///
/// The name is optional (`docs/10`); an unnamed index gets a generated one
/// in the checking phase.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexEntry {
    pub name: Option<Ident>,
    /// The order matters: `(a, b)` is not `(b, a)`.
    pub columns: Vec<Ident>,
    pub flags: Vec<Attr>,
}

// ===========================================================================
// Directives and file level
// ===========================================================================

/// `spec 1;` — the language definition version the file is written against.
///
/// `version` is optional so a half-typed `spec ;` still yields a node and
/// the IDE knows a `spec` is here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub version: Option<i64>,
}

/// `import "./common.skema" as base;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub path: Option<String>,
    /// `as base` — overrides the source file's document name.
    pub alias: Option<Ident>,
}

/// `use common_base;` — pulls in a `Partial`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Use {
    pub target: Path,
}

/// A single statement — the kind of node every body is made of.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub kind: ItemKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    Spec(Spec),
    Import(Import),
    Use(Use),
    /// `mode = draft;`
    Setting(Attr),
    Field(Field),
    Block(Block),
    Ref(Ref),
    IndexEntry(IndexEntry),
    /// An unreadable section. The parser skipped it and moved on.
    ///
    /// A node instead of an abort, so the other declarations in the file
    /// stay usable while someone is typing.
    Error,
}

/// A whole file.
///
/// A flat list: `spec`, `import` and declarations are all [`Item`]s. That
/// `spec` comes first and `import` may not sit inside a `Table` is a rule,
/// not a shape, and rules belong to the checking phase.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceFile {
    pub items: Vec<Item>,
    pub span: Span,
}

impl SourceFile {
    /// All top-level blocks — the usual entry point for analysis.
    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.items.iter().filter_map(|i| match &i.kind {
            ItemKind::Block(b) => Some(b),
            _ => None,
        })
    }

    /// The first top-level block of `kind` declared under `name`, e.g. the
    /// `Table Customer` of a file.
    ///
    /// Purely syntactic — the name as written, no namespace resolution, no
    /// duplicate check. Those live in `skema_hir::symbols`. This is the
    /// cheap lookup for a caller that already knows what it wants.
    pub fn declaration(&self, kind: BlockKind, name: &str) -> Option<&Block> {
        self.blocks().find(|b| {
            b.kind == kind
                && b.name
                    .as_ref()
                    .and_then(Path::as_ident)
                    .is_some_and(|i| i.text == name)
        })
    }

    /// The first `spec` entry, if any.
    pub fn spec(&self) -> Option<&Spec> {
        self.items.iter().find_map(|i| match &i.kind {
            ItemKind::Spec(s) => Some(s),
            _ => None,
        })
    }

    pub fn imports(&self) -> impl Iterator<Item = &Import> {
        self.items.iter().filter_map(|i| match &i.kind {
            ItemKind::Import(im) => Some(im),
            _ => None,
        })
    }
}
