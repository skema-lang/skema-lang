//! The abstract syntax tree (AST) of the `.skema` language.
//!
//! # The design idea: few nodes, lots of reuse
//!
//! Looking at `examples/myexample.skema`, you'll notice that almost every
//! line follows the same shape:
//!
//! ```text
//! Name  [Type]  [flags]  { Body }
//! ```
//!
//! That holds for fields (`email varchar [unique] { note = "…" }`), for
//! enum values (`draft { label="Draft" }`), and for blocks (`Table Customer { … }`).
//! The AST takes advantage of this: there are **five** load-bearing node
//! kinds instead of one per keyword.
//!
//! | Node | Covers |
//! |---|---|
//! | [`Block`] | `Project`, `Table`, `Meta`, `Style @table`, `Dialect`, … |
//! | [`Field`] | table fields, enum values, `Dialect` sub-entries |
//! | [`Attr`] | `mode = draft;` **and** `[pk]` **and** `[default=12]` |
//! | [`Value`] | every value, wherever it appears |
//! | [`Ref`] | `Table shop.User;` inside a `Group` |
//!
//! # Why the parser allows more than the language knows
//!
//! The AST captures **form**, not **meaning**. It allows an `Index` block
//! to appear inside an `Enum`, or a `Project` to have a name. That's
//! intentional and the central trick of this frontend:
//!
//! * An error like "`Lint` may only appear inside `Project`" is a **clear
//!   message** from a checking phase — not a "syntax error at line 12" where
//!   the parser no longer understands the rest of the file.
//! * The tree stays complete even with invalid content, so autocompletion
//!   and go-to-definition keep working.
//! * The grammar stays small: **one** body-parsing function for all blocks
//!   instead of fourteen nearly identical ones.
//!
//! This division of labor is called "parse permissively, validate strictly"
//! and is the reason modern IDE frontends work at all. The checking phase
//! belongs later in the `skema-hir` crate.

use crate::Span;

// ===========================================================================
// Building blocks
// ===========================================================================

/// A name in the source text.
///
/// Unlike [`crate::Token`], the AST holds the text **by value** (`String`).
/// The reason is ergonomics, not laziness: a `&'a str` would tie the whole
/// tree to the source-text buffer, and a language server wants to keep the
/// tree around precisely when the buffer has already changed. A few hundred
/// small `String`s per file is a cheap price for that.
/// (Anyone who wants to optimize can later swap this for an interned type
/// like `SmolStr` — the rest of the code stays the same.)
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
    /// Declarations (`Table Customer`) need a plain name; references
    /// (`Table shop.User`) may be qualified. Both are read by the same
    /// parser function; the distinction is made here.
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
/// `quoted` distinguishes the two forms from `docs/01`, section 1.11:
///
/// ```text
/// author   = "anna";       # known key, unquoted
/// "x-jira" = "SHOP-412";   # custom key, quoted and with an x- prefix
/// ```
///
/// The parser does **not** check the rule — it only records what was
/// written. Whether `athor` is a typo is something only the checking phase
/// knows.
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

#[derive(Debug, Clone, PartialEq)]
pub enum ValueKind {
    /// `draft`, `CASCADE`, `shop.OrderStatus.paid`
    ///
    /// Identifiers and qualified names are deliberately **not**
    /// distinguished here: whether `draft` is an enum value, a mode, or a
    /// typo is decided by name resolution, not by the parser.
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
    /// Occurs in exactly one place: `type = decimal(10);` in the `Field`
    /// block of a `Datatype` (`docs/06`, "Layering types") — a type builds
    /// on another one there. Structurally the same as
    /// [`FieldTypeKind::Named`], just read as a value instead of a field
    /// type.
    Type {
        path: Path,
        args: Vec<Value>,
    },
}

/// An assignment — in `{}` as well as in `[]`.
///
/// # Why one type for both
///
/// The language strictly separates what `[]` and `{}` *mean* (`[]` produces
/// DDL, `{}` produces documentation — `docs/01`, 1.7). Their *shape*,
/// however, is identical: key, optional `=`, value. Two structs for this
/// would mean two parser functions that would have to be maintained in
/// lockstep.
///
/// ```text
/// mode = draft;          Attr { key: "mode",     value: Some(draft) }
/// [pk]                   Attr { key: "pk",       value: None }
/// [default=`now()`]      Attr { key: "default",  value: Some(Raw) }
/// ```
///
/// `value: None` only makes sense in `[]`; in `{}` it's an error — reported
/// by the checking phase, not the parser.
#[derive(Debug, Clone, PartialEq)]
pub struct Attr {
    pub key: Key,
    pub value: Option<Value>,
    pub span: Span,
}

// ===========================================================================
// Blocks
// ===========================================================================

/// Generates [`BlockKind`] together with a name table from a single list.
///
/// A declarative macro is exactly right here: without it, every keyword
/// would have to appear in three places (variant, `from_keyword`, `as_str`),
/// and the third one is easy to forget. `stringify!($v)` turns the
/// identifier `Table` into the string `"Table"` — that's why the variants
/// are named exactly like the keywords.
macro_rules! block_kinds {
    ($($v:ident),* $(,)?) => {
        /// Which block is it?
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum BlockKind {
            $($v,)*
            /// A block name the language doesn't know.
            ///
            /// This is not a parser error: `Dialect { postgres { … } }`
            /// deliberately contains freely named sub-blocks. Whether the
            /// name is allowed in that position is decided by the checking
            /// phase.
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
    // `Field` describes, independent of any database, how a field of this
    // type behaves — the same in `Datatype` (`docs/06`) and `Enum`
    // (`docs/08`). Despite the name, it's a perfectly ordinary settings
    // block like `Meta` or `Dialect`, not a field in the sense of [`Field`]
    // (the AST node further below) — the shared name is a coincidence of
    // the language.
    Meta, Style, Index, Database, Field, Dialect, Lint,
);

impl BlockKind {
    /// May this block appear at the top level?
    ///
    /// Lives here rather than in the checking phase because it's a property
    /// of the language, not a rule about a specific model.
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
/// Covers every level — the nesting arises because `items` in turn contains
/// [`Item`]s, which may themselves be `Block`s.
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
    /// The name of the declaration. `Path`, so `Table shop.User` takes the
    /// same route; whether a dot is allowed here is checked by the checking
    /// phase.
    pub name: Option<Path>,
    /// The parameters of a `Datatype`: `decimal(precision int, scale int)`.
    pub params: Vec<Param>,
    pub items: Vec<Item>,
}

/// A parameter of a `Datatype`: `precision int` in
/// `Datatype decimal(precision int, scale int) { … }`.
///
/// "Parameters are declared like fields — name, then type" (`docs/06`).
/// `ty` is `Option` because an incomplete line while typing is the normal
/// case; the node stays usable regardless.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: Ident,
    pub ty: Option<Path>,
    pub span: Span,
}

/// A reference without a body: `Table shop.User;` inside a `Group`.
///
/// Syntactically the start of a [`Block`] — only the line ends with `;`
/// instead of `{`. That's why both are read by the same parser function,
/// which only branches at the very end.
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
/// Despite the name, `Field` covers three things because they share the
/// same shape:
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
    /// Whether `shop.OrderStatus` is a data type or an enum can't be told
    /// from the syntax — name resolution settles that.
    Named { path: Path, args: Vec<Value> },
    /// `>Customer.id`, `<>Tag.id`
    Relation { op: RelationOp, target: Path },
}

/// The cardinality of a relationship.
///
/// Only three values — the language deliberately has no many-to-many sign.
/// `docs/07`, "Many-to-many has no arrow": SQL needs a join table for that
/// anyway, so it gets written into the model instead of being hidden behind
/// an operator.
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
/// The name is optional, like every named thing in the language
/// (`docs/10`). An unnamed index later gets a generated name — decided by
/// the checking phase, not the parser.
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
/// `Option` because an incomplete line (`spec ;`) is the normal case while
/// editing. The node stays in the tree regardless, so the IDE knows a
/// `spec` is here.
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
    /// An error as a node instead of an abort: only this way do the *other*
    /// declarations in the file stay usable while someone is typing.
    Error,
}

/// A whole file.
///
/// Deliberately just a flat list: `spec`, `import`, and declarations are
/// all [`Item`]s. That `spec` must come first and `import` may not appear
/// inside a `Table` is a **rule**, not a **shape** — and rules are checked
/// by the checking phase.
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
