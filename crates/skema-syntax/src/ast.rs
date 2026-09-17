//! The syntax tree of the `.skema` language: typed views over a lossless
//! `rowan` tree.
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
//! # Views, not data
//!
//! Every type here is a handle on a [`SyntaxNode`] or [`SyntaxToken`]; the
//! tree underneath keeps every token, whitespace and comments included, so
//! `file.syntax().to_string()` is the source text. Accessors return
//! `Option` where the parser may have left a part out, and a node's
//! [`AstNode::span`] covers its significant tokens only — never the
//! comments attached to it.
//!
//! # Form, not meaning
//!
//! The tree allows an `Index` block inside an `Enum`, or a named `Project`.
//! Deliberately: "`Lint` may only appear inside `Project`" is then a clear
//! message from `skema-hir` instead of a syntax error that drags down the
//! rest of the file.

use rowan::{GreenNode, TextRange};

use crate::Span;
use crate::TextSize;
use crate::grammar::unquote;
use crate::syntax_kind::{SyntaxKind, SyntaxNode, SyntaxToken};

// ===========================================================================
// Plumbing
// ===========================================================================

pub trait AstNode: Sized {
    fn cast(node: SyntaxNode) -> Option<Self>;
    fn syntax(&self) -> &SyntaxNode;

    /// First to last significant token — the span the owned AST recorded.
    fn span(&self) -> Span {
        significant_span(self.syntax())
    }
}

macro_rules! ast_node {
    ($name:ident, $kind:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(SyntaxNode);

        impl AstNode for $name {
            fn cast(node: SyntaxNode) -> Option<Self> {
                (node.kind() == SyntaxKind::$kind).then_some(Self(node))
            }

            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }
    };
}

fn to_span(range: TextRange) -> Span {
    Span::new(range.start().into(), range.end().into())
}

fn significant_span(node: &SyntaxNode) -> Span {
    let mut tokens = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia());
    let Some(first) = tokens.next() else {
        return Span::empty(node.text_range().start().into());
    };
    let first = to_span(first.text_range());
    match tokens.last() {
        Some(last) => first.cover(to_span(last.text_range())),
        None => first,
    }
}

fn child<N: AstNode>(parent: &SyntaxNode) -> Option<N> {
    parent.children().find_map(N::cast)
}

fn children<N: AstNode>(parent: &SyntaxNode) -> impl Iterator<Item = N> + use<N> {
    parent.children().filter_map(N::cast)
}

/// Direct significant tokens, in order.
fn tokens(parent: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> + use<> {
    parent
        .children_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
}

fn token(parent: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    tokens(parent).find(|token| token.kind() == kind)
}

/// The token directly after the first `after`, if it is a `kind`.
fn token_after(parent: &SyntaxNode, after: SyntaxKind, kind: SyntaxKind) -> Option<SyntaxToken> {
    let mut tokens = tokens(parent).skip_while(|token| token.kind() != after);
    tokens.next()?;
    tokens.next().filter(|token| token.kind() == kind)
}

fn body_items(parent: &SyntaxNode) -> impl Iterator<Item = Item> + use<> {
    child::<Body>(parent)
        .into_iter()
        .flat_map(|body| children::<Item>(&body.0))
}

// ===========================================================================
// Building blocks
// ===========================================================================

/// A name in the source text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ident(SyntaxToken);

impl Ident {
    fn cast(token: SyntaxToken) -> Option<Self> {
        (token.kind() == SyntaxKind::Ident).then_some(Self(token))
    }

    pub fn text(&self) -> &str {
        self.0.text()
    }

    pub fn span(&self) -> Span {
        to_span(self.0.text_range())
    }

    pub fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

ast_node!(Path, Path);

impl Path {
    pub fn segments(&self) -> impl Iterator<Item = Ident> + use<> {
        tokens(&self.0).filter_map(Ident::cast)
    }

    /// The name, if it is **not** qualified.
    pub fn as_ident(&self) -> Option<Ident> {
        let mut segments = self.segments();
        let only = segments.next()?;
        segments.next().is_none().then_some(only)
    }

    /// `shop.Order.id` — for error messages and debug output.
    pub fn to_dotted(&self) -> String {
        self.segments()
            .map(|segment| segment.text().to_owned())
            .collect::<Vec<_>>()
            .join(".")
    }
}

/// The key of an assignment: `author` or the quoted custom form `"x-jira"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key(SyntaxToken);

impl Key {
    fn cast(token: SyntaxToken) -> Option<Self> {
        matches!(token.kind(), SyntaxKind::Ident | SyntaxKind::Str).then_some(Self(token))
    }

    pub fn text(&self) -> String {
        if self.quoted() {
            unquote(self.0.text())
        } else {
            self.0.text().to_owned()
        }
    }

    pub fn quoted(&self) -> bool {
        self.0.kind() == SyntaxKind::Str
    }

    pub fn span(&self) -> Span {
        to_span(self.0.text_range())
    }
}

ast_node!(Value, Value);

impl Value {
    /// `None` only for a node the parser could not fill.
    pub fn kind(&self) -> Option<ValueKind> {
        let first = self
            .0
            .children_with_tokens()
            .find(|element| !element.kind().is_trivia())?;
        let first = match first {
            rowan::NodeOrToken::Node(node) => return node_value_kind(node),
            rowan::NodeOrToken::Token(token) => token,
        };
        Some(match first.kind() {
            SyntaxKind::Str => ValueKind::Str(unquote(first.text())),
            SyntaxKind::Raw => {
                let text = first.text();
                ValueKind::Raw(
                    text.get(1..text.len().saturating_sub(1))
                        .unwrap_or_default()
                        .to_owned(),
                )
            }
            SyntaxKind::Int => ValueKind::Int(parse_int(first.text()).unwrap_or(0)),
            SyntaxKind::Float => ValueKind::Float(parse_float(first.text())),
            SyntaxKind::Minus => {
                let number = tokens(&self.0).nth(1)?;
                match number.kind() {
                    SyntaxKind::Int => ValueKind::Int(-parse_int(number.text()).unwrap_or(0)),
                    _ => ValueKind::Float(-parse_float(number.text())),
                }
            }
            _ => return None,
        })
    }

    /// The value as plain text: a string without its quotes, a raw
    /// expression without its backticks, a list comma-joined.
    pub fn to_text(&self) -> String {
        match self.kind() {
            None => String::new(),
            Some(ValueKind::Path(p)) => p.to_dotted(),
            Some(ValueKind::Str(s) | ValueKind::Raw(s)) => s,
            Some(ValueKind::Int(n)) => n.to_string(),
            Some(ValueKind::Float(f)) => f.to_string(),
            Some(ValueKind::List(items)) => join_texts(&items),
            Some(ValueKind::Type { path, args }) => {
                format!("{}({})", path.to_dotted(), join_texts(&args))
            }
        }
    }
}

fn join_texts(values: &[Value]) -> String {
    values
        .iter()
        .map(Value::to_text)
        .collect::<Vec<_>>()
        .join(", ")
}

fn node_value_kind(node: SyntaxNode) -> Option<ValueKind> {
    match node.kind() {
        SyntaxKind::List => Some(ValueKind::List(values(&node))),
        SyntaxKind::Path => {
            let parent = node.parent()?;
            let path = Path(node);
            Some(match child::<Args>(&parent) {
                Some(args) => ValueKind::Type {
                    path,
                    args: values(&args.0),
                },
                None => ValueKind::Path(path),
            })
        }
        _ => None,
    }
}

/// The filled values among a node's children.
fn values(parent: &SyntaxNode) -> Vec<Value> {
    children::<Value>(parent)
        .filter(|value| value.kind().is_some())
        .collect()
}

pub(crate) fn parse_int(text: &str) -> Option<i64> {
    text.replace('_', "").parse().ok()
}

fn parse_float(text: &str) -> f64 {
    text.replace('_', "").parse().unwrap_or(0.0)
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValueKind {
    Path(Path),
    Str(String),
    Int(i64),
    Float(f64),
    Raw(String),
    List(Vec<Value>),
    Type { path: Path, args: Vec<Value> },
}

ast_node!(Attr, Attr);

impl Attr {
    pub fn key(&self) -> Option<Key> {
        tokens(&self.0).next().and_then(Key::cast)
    }

    pub fn value(&self) -> Option<Value> {
        children::<Value>(&self.0).find(|value| value.kind().is_some())
    }

    /// `[error]` after a setting; `None` in `[flags]` and when malformed.
    pub fn severity_override(&self) -> Option<Ident> {
        let setting = self
            .0
            .parent()
            .filter(|p| p.kind() == SyntaxKind::Setting)?;
        let bracket = setting
            .children()
            .find(|node| node.kind() == SyntaxKind::SeverityOverride)?;
        let significant: Vec<_> = tokens(&bracket).collect();
        match significant.as_slice() {
            [open, word, close]
                if open.kind() == SyntaxKind::LBracket && close.kind() == SyntaxKind::RBracket =>
            {
                Ident::cast(word.clone())
            }
            _ => None,
        }
    }
}

ast_node!(Body, Body);
ast_node!(Args, Args);
ast_node!(Flags, Flags);

fn flags(parent: &SyntaxNode) -> impl Iterator<Item = Attr> + use<> {
    child::<Flags>(parent)
        .into_iter()
        .flat_map(|flags| children::<Attr>(&flags.0))
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

ast_node!(Block, Block);

impl Block {
    /// The word as written — even if it's unknown (`postgres`).
    pub fn keyword(&self) -> Option<Ident> {
        tokens(&self.0).next().and_then(Ident::cast)
    }

    pub fn kind(&self) -> BlockKind {
        block_kind(self.keyword())
    }

    /// `Style @table` — the built-in element type behind the `@`.
    pub fn selector(&self) -> Option<Ident> {
        token_after(&self.0, SyntaxKind::At, SyntaxKind::Ident).and_then(Ident::cast)
    }

    pub fn name(&self) -> Option<Path> {
        child(&self.0)
    }

    pub fn params(&self) -> impl Iterator<Item = Param> + use<> {
        child::<Params>(&self.0)
            .into_iter()
            .flat_map(|params| children::<Param>(&params.0))
    }

    /// The `{` that opens the body.
    pub fn body_brace(&self) -> Option<Span> {
        let body = child::<Body>(&self.0)?;
        token(&body.0, SyntaxKind::LBrace).map(|brace| to_span(brace.text_range()))
    }

    /// Byte offset just past the opening `{`.
    pub fn body_start(&self) -> Option<TextSize> {
        self.body_brace().map(|brace| brace.end)
    }

    pub fn items(&self) -> impl Iterator<Item = Item> + use<> {
        body_items(&self.0)
    }

    /// The first direct sub-block of a given kind.
    pub fn child(&self, kind: BlockKind) -> Option<Block> {
        self.items().find_map(|item| match item.kind() {
            ItemKind::Block(b) if b.kind() == kind => Some(b),
            _ => None,
        })
    }
}

fn block_kind(keyword: Option<Ident>) -> BlockKind {
    keyword
        .and_then(|keyword| BlockKind::from_keyword(keyword.text()))
        .unwrap_or(BlockKind::Other)
}

ast_node!(Params, Params);
ast_node!(Param, Param);

impl Param {
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0).next().and_then(Ident::cast)
    }

    pub fn ty(&self) -> Option<Path> {
        child(&self.0)
    }
}

ast_node!(Ref, Ref);

impl Ref {
    pub fn keyword(&self) -> Option<Ident> {
        tokens(&self.0).next().and_then(Ident::cast)
    }

    pub fn kind(&self) -> BlockKind {
        block_kind(self.keyword())
    }

    pub fn path(&self) -> Option<Path> {
        child(&self.0)
    }
}

// ===========================================================================
// Fields
// ===========================================================================

ast_node!(Field, Field);

impl Field {
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0).next().and_then(Ident::cast)
    }

    /// `None` also for a half-written relation (`>` without a target).
    pub fn ty(&self) -> Option<FieldType> {
        children::<FieldType>(&self.0).find(|ty| ty.kind().is_some())
    }

    pub fn flags(&self) -> impl Iterator<Item = Attr> + use<> {
        flags(&self.0)
    }

    pub fn items(&self) -> impl Iterator<Item = Item> + use<> {
        body_items(&self.0)
    }
}

ast_node!(FieldType, FieldType);

impl FieldType {
    pub fn kind(&self) -> Option<FieldTypeKind> {
        let path = child::<Path>(&self.0)?;
        let op = tokens(&self.0).next().and_then(|token| match token.kind() {
            SyntaxKind::Gt => Some(RelationOp::OneToMany),
            SyntaxKind::Lt => Some(RelationOp::ManyToOne),
            SyntaxKind::Minus => Some(RelationOp::OneToOne),
            _ => None,
        });
        Some(match op {
            Some(op) => FieldTypeKind::Relation { op, target: path },
            None => FieldTypeKind::Named {
                path,
                args: child::<Args>(&self.0)
                    .map(|args| values(&args.0))
                    .unwrap_or_default(),
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldTypeKind {
    Named { path: Path, args: Vec<Value> },
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

ast_node!(IndexEntry, IndexEntry);

impl IndexEntry {
    pub fn name(&self) -> Option<Ident> {
        tokens(&self.0).next().and_then(Ident::cast)
    }

    pub fn columns(&self) -> impl Iterator<Item = Ident> + use<> {
        child::<Columns>(&self.0)
            .into_iter()
            .flat_map(|columns| tokens(&columns.0).filter_map(Ident::cast))
    }

    pub fn flags(&self) -> impl Iterator<Item = Attr> + use<> {
        flags(&self.0)
    }
}

ast_node!(Columns, Columns);

// ===========================================================================
// Directives and file level
// ===========================================================================

ast_node!(Spec, Spec);

impl Spec {
    pub fn version(&self) -> Option<i64> {
        token(&self.0, SyntaxKind::Int).and_then(|int| parse_int(int.text()))
    }
}

ast_node!(Import, Import);

impl Import {
    pub fn path(&self) -> Option<String> {
        token(&self.0, SyntaxKind::Str).map(|path| unquote(path.text()))
    }

    pub fn alias(&self) -> Option<Ident> {
        token_after(&self.0, SyntaxKind::KwAs, SyntaxKind::Ident).and_then(Ident::cast)
    }
}

ast_node!(Use, Use);

impl Use {
    pub fn target(&self) -> Option<Path> {
        child(&self.0)
    }
}

ast_node!(Setting, Setting);

/// A single statement — the kind of node every body is made of.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Item(SyntaxNode);

impl AstNode for Item {
    fn cast(node: SyntaxNode) -> Option<Self> {
        node.kind().is_item().then_some(Self(node))
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }
}

impl Item {
    pub fn kind(&self) -> ItemKind {
        let node = self.0.clone();
        match node.kind() {
            SyntaxKind::Spec => ItemKind::Spec(Spec(node)),
            SyntaxKind::Import => ItemKind::Import(Import(node)),
            SyntaxKind::Use => ItemKind::Use(Use(node)),
            SyntaxKind::Setting => child(&node).map_or(ItemKind::Error, ItemKind::Setting),
            SyntaxKind::Field => ItemKind::Field(Field(node)),
            SyntaxKind::Block => ItemKind::Block(Block(node)),
            SyntaxKind::Ref => ItemKind::Ref(Ref(node)),
            SyntaxKind::IndexEntry => ItemKind::IndexEntry(IndexEntry(node)),
            _ => ItemKind::Error,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    Error,
}

/// A whole file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceFile(SyntaxNode);

impl AstNode for SourceFile {
    fn cast(node: SyntaxNode) -> Option<Self> {
        (node.kind() == SyntaxKind::SourceFile).then_some(Self(node))
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.0
    }

    /// From offset 0 — a file starts at its start, not at its first token.
    fn span(&self) -> Span {
        Span::new(0, significant_span(&self.0).end)
    }
}

impl SourceFile {
    pub fn new_root(green: GreenNode) -> Self {
        Self(SyntaxNode::new_root(green))
    }

    pub fn items(&self) -> impl Iterator<Item = Item> + use<> {
        children::<Item>(&self.0)
    }

    /// All top-level blocks — the usual entry point for analysis.
    pub fn blocks(&self) -> impl Iterator<Item = Block> + use<> {
        self.items().filter_map(|item| match item.kind() {
            ItemKind::Block(b) => Some(b),
            _ => None,
        })
    }

    /// The first top-level block of `kind` declared under `name`.
    pub fn declaration(&self, kind: BlockKind, name: &str) -> Option<Block> {
        self.blocks().find(|b| {
            b.kind() == kind
                && b.name()
                    .and_then(|path| path.as_ident())
                    .is_some_and(|ident| ident.text() == name)
        })
    }

    pub fn spec(&self) -> Option<Spec> {
        self.items().find_map(|item| match item.kind() {
            ItemKind::Spec(s) => Some(s),
            _ => None,
        })
    }

    pub fn imports(&self) -> impl Iterator<Item = Import> + use<> {
        self.items().filter_map(|item| match item.kind() {
            ItemKind::Import(import) => Some(import),
            _ => None,
        })
    }
}
