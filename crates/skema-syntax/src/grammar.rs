//! The grammar of the `.skema` language.
//!
//! This is **what** gets read; the tools for that live in the `parser`
//! module.
//!
//! # The grammar at a glance
//!
//! ```text
//! file      := item*
//!
//! item      := spec | import | use | setting | field | block | ref | index
//!
//! spec      := "spec" Int ";"
//! import    := "import" Str ("as" Ident)? ";"
//! use       := "use" path ";"
//! setting   := key "=" value ";"
//! field     := Ident type? flags? (body | ";")
//! block     := Word ("@" Ident)? path? params? body
//! ref       := Word path ";"
//! index     := "(" Ident ("," Ident)* ")" flags? ";"
//!
//! type      := relop path | path ("(" value ("," value)* ")")?
//! relop     := ">" | "<" | "-"
//! flags     := "[" attr ("," attr)* "]"
//! attr      := key ("=" value)?
//! key       := Ident | Str
//! value     := Str | Int | Float | Raw | path | "[" value ("," value)* "]"
//! path      := Ident ("." Ident)*
//! body      := "{" item* "}"
//! ```
//!
//! # The one ambiguity
//!
//! Three lines look identical to the parser — all are `Ident … "{"`:
//!
//! ```text
//! Table Customer { … }      block
//! sum   float    { … }      field with a description block
//! draft          { … }      enum value
//! ```
//!
//! Resolved by word list: a first word in [`BlockKind`] opens a block,
//! anything else is a field (contextual keywords). The price is that a
//! field named `Index` is read wrong; the payoff is that `Table Group { … }`
//! stays writable, and in an ERP model that is an ordinary table name.

use crate::ast::{BlockKind, SourceFile};
use crate::parser::Parser;
use crate::span::Span;
use crate::syntax_kind::SyntaxKind as S;
use crate::token::Token;
use crate::token::TokenKind as T;

/// What [`Parser::parse_setting`] needs to know about an attribute it read.
struct ParsedAttr {
    span: Span,
    has_value: bool,
}

impl Parser<'_> {
    /// Entry point: reads a whole file.
    pub fn parse_source_file(&mut self) -> SourceFile {
        self.start_root();
        self.repeat_until(T::Eof, Self::parse_item);
        SourceFile::new_root(self.finish_root())
    }

    /// `{ item* }` — the body of every block.
    ///
    /// **One** function for all fourteen block kinds. That `Lint` may only
    /// appear inside `Project` is a rule about meaning and is checked
    /// later, not here.
    fn parse_body(&mut self) {
        self.in_node(S::Body, |p| {
            if !p.expect(T::LBrace) {
                return;
            }
            p.repeat_until(T::RBrace, Self::parse_item);
            p.expect(T::RBrace);
        });
    }

    // -- Statements -------------------------------------------------------

    /// A single statement.
    fn parse_item(&mut self) {
        let checkpoint = self.start_item();
        let kind = self.parse_item_kind();
        self.finish_item(checkpoint, kind);
    }

    /// The dispatch. It looks at at most two tokens — the language doesn't
    /// need more.
    fn parse_item_kind(&mut self) -> S {
        match self.current().kind {
            T::KwSpec => self.parse_spec(),
            T::KwImport => self.parse_import(),
            T::KwUse => self.parse_use(),

            // `(email) [unique];` – the unnamed form of an index entry.
            T::LParen => self.parse_index_entry(),

            // `"x-jira" = "SHOP-412";`
            T::Str if matches!(self.nth(1), T::Eq | T::Colon) => self.parse_setting(),

            T::Ident => {
                if matches!(self.nth(1), T::Eq | T::Colon) {
                    // `mode = draft;`
                    self.parse_setting()
                } else if self.nth(1) == T::At
                    || BlockKind::from_keyword(self.current_text()).is_some()
                {
                    // `Style @table { … }` or `Table Customer { … }`
                    self.parse_block_or_ref()
                } else if self.nth(1) == T::LParen {
                    // `ix_customer_date (customer, date) [unique];` – the
                    // named form. Unambiguous from an `Ident` **directly**
                    // followed by `(`: a typed field always has a second
                    // `Ident` in between (`gross decimal(10, 2)`).
                    self.parse_index_entry()
                } else {
                    // `email varchar;`, `draft { … }`, `cancelled;`
                    self.parse_field()
                }
            }

            _ => {
                let t = self.bump(); // progress: otherwise the loop spins
                // Lexer errors have already been reported while scanning.
                if !t.kind.is_error() {
                    self.error(t.span, format!("unexpected: {}", t.kind.describe()));
                }
                // After a stray bracket, don't also swallow the rest —
                // it presumably only closes too much.
                if !matches!(t.kind, T::RBrace | T::RBracket | T::RParen) {
                    self.recover_item();
                }
                S::Error
            }
        }
    }

    /// `spec 1;`
    fn parse_spec(&mut self) -> S {
        self.bump(); // `spec`
        if self.at(T::Int) {
            let t = self.bump();
            self.check_int(t);
        } else {
            self.error_here("expected an integer, e.g. `spec 1;`");
        }
        self.expect_terminator();
        S::Spec
    }

    /// `import "./common.skema" as base;`
    fn parse_import(&mut self) -> S {
        self.bump(); // `import`
        if !self.eat(T::Str) {
            self.error_here("expected a quoted path");
        }
        // `as` is optional: without an alias, the source file's document
        // name applies.
        if self.eat(T::KwAs) {
            self.ident();
        }
        self.expect_terminator();
        S::Import
    }

    /// `use common_base;`
    fn parse_use(&mut self) -> S {
        self.bump(); // `use`
        if !self.parse_path() {
            self.recover_item();
            return S::Error;
        }
        self.expect_terminator();
        S::Use
    }

    /// `mode = draft;`, `fieldNaming = snake_case [error];`
    fn parse_setting(&mut self) -> S {
        let Some(attr) = self.parse_attr() else {
            self.recover_item();
            return S::Error;
        };
        if !attr.has_value {
            self.error(attr.span, "a value is missing here: `key = value;`");
        }
        if self.at(T::LBracket) {
            self.parse_severity_override();
        }
        self.expect_terminator();
        S::Setting
    }

    /// `[error]` on a setting — a severity override, not a flag list: one
    /// bare word, no comma (task 67 gives it meaning; here it's grammar
    /// only, legal after any setting in any block, `docs/13`).
    fn parse_severity_override(&mut self) {
        self.in_node(S::SeverityOverride, Self::parse_severity_override_inner);
    }

    fn parse_severity_override_inner(&mut self) {
        let start = self.current().span;
        self.bump(); // `[`
        if self.at(T::Ident) && self.nth(1) == T::RBracket {
            self.ident();
            self.bump(); // `]`
            return;
        }
        // Not exactly one bare word: skip to `]`, a safe stopping point, or
        // a fresh line — the same emergency brake as `recover_item`.
        while !matches!(
            self.current().kind,
            T::RBracket | T::Semi | T::RBrace | T::Eof
        ) && !self.line_break_before_current()
        {
            self.bump();
        }
        self.error(
            start.cover(self.prev_span()),
            "expected exactly one identifier, e.g. `[error]`",
        );
        self.eat(T::RBracket);
    }

    /// `Table Customer { … }`, `Style @table { … }`, `Table shop.User;`
    ///
    /// Block and reference share everything but the last character —
    /// that's why they share a function too.
    fn parse_block_or_ref(&mut self) -> S {
        if !self.ident() {
            self.recover_item();
            return S::Error;
        }

        // `@` = built-in element type, no `@` = self-chosen name.
        if self.eat(T::At) {
            self.ident();
        }

        let named = self.at(T::Ident) && self.parse_path();

        // Only `Datatype` has parameters: `Datatype decimal(precision int, scale int)`.
        if self.at(T::LParen) {
            self.parse_params();
        }

        if self.at(T::LBrace) {
            self.parse_body();
            self.eat(T::Semi); // `};` is tolerated, the formatter cleans it up
            S::Block
        } else if named {
            // No body, but a name: a reference like in `Group`.
            self.expect_terminator();
            S::Ref
        } else {
            self.error_here("expected `{` or a name");
            self.recover_item();
            S::Error
        }
    }

    /// `email varchar [unique] { note = "…" };`
    fn parse_field(&mut self) -> S {
        if !self.ident() {
            self.recover_item();
            return S::Error;
        }
        self.parse_field_type();
        if self.at(T::LBracket) {
            self.parse_flags();
        }

        // If the statement ends with `}`, the `;` is superfluous (docs/01, 1.4).
        if self.at(T::LBrace) {
            self.parse_body();
            self.eat(T::Semi);
        } else {
            self.expect_terminator();
        }
        S::Field
    }

    /// `[name] (email, date) [unique];` – the name is optional (`docs/10`).
    fn parse_index_entry(&mut self) -> S {
        if self.at(T::Ident) {
            self.ident();
        }
        if self.parse_ident_list() == 0 {
            self.error_here("an index needs at least one column");
        }
        if self.at(T::LBracket) {
            self.parse_flags();
        }
        self.expect_terminator();
        S::IndexEntry
    }

    /// Ends a statement.
    ///
    /// `;` **separates** statements, so before a block's `}` there is
    /// nothing left to separate and it is optional there (`docs/01`,
    /// "Statement termination"):
    ///
    /// ```text
    /// Enum OrderStatus {
    ///   draft { label = "Draft" }    ← no `;` needed
    ///   paid;                        ← needed, something else follows
    /// }
    /// ```
    ///
    /// Between two statements it stays mandatory.
    fn expect_terminator(&mut self) {
        if self.at(T::RBrace) {
            return;
        }
        self.expect(T::Semi);
    }

    // -- Components ---------------------------------------------------------

    /// What's allowed to follow a field name — or nothing (`cancelled;`).
    fn parse_field_type(&mut self) {
        if !self.current().kind.is_relation_op() && !self.at(T::Ident) {
            return;
        }
        self.in_node(S::FieldType, Self::parse_field_type_inner);
    }

    fn parse_field_type_inner(&mut self) {
        // `customer >Customer.id`
        if self.current().kind.is_relation_op() {
            self.bump();
            self.parse_path();
            return;
        }

        // `varchar`, `decimal(10, 2)`, `shop.OrderStatus`
        if self.at(T::Ident) && self.parse_path() && self.at(T::LParen) {
            self.parse_args();
        }
    }

    /// `(10, 2)` after a type name.
    fn parse_args(&mut self) {
        self.in_node(S::Args, |p| {
            p.delimited_list(T::LParen, T::RParen, T::Comma, Self::parse_value)
        });
    }

    /// `[pk, required, default=0]`
    fn parse_flags(&mut self) {
        self.in_node(S::Flags, |p| {
            p.delimited_list(T::LBracket, T::RBracket, T::Comma, |p| {
                p.parse_attr().is_some()
            })
        });
    }

    /// `(a, b)` — for `Index` columns. `Datatype` parameters additionally
    /// have a type and are therefore read by [`Self::parse_params`].
    /// Returns how many columns were read.
    fn parse_ident_list(&mut self) -> usize {
        self.in_node(S::Columns, |p| {
            p.delimited_list(T::LParen, T::RParen, T::Comma, Self::ident)
        })
    }

    /// `(precision int, scale int)` — the parameter list of a `Datatype`.
    fn parse_params(&mut self) {
        self.in_node(S::Params, |p| {
            p.delimited_list(T::LParen, T::RParen, T::Comma, Self::parse_param)
        });
    }

    /// A single parameter: "declared like a field — name, then type"
    /// (`docs/06`).
    fn parse_param(&mut self) -> bool {
        if !self.at(T::Ident) {
            self.ident();
            return false;
        }
        self.in_node(S::Param, |p| {
            p.ident();
            if p.at(T::Ident) {
                p.parse_path();
            }
        });
        true
    }

    /// `key` or `key = value`.
    fn parse_attr(&mut self) -> Option<ParsedAttr> {
        if !matches!(self.current().kind, T::Ident | T::Str) {
            self.parse_key();
            return None;
        }
        self.in_node(S::Attr, Self::parse_attr_inner)
    }

    fn parse_attr_inner(&mut self) -> Option<ParsedAttr> {
        let start = self.parse_key()?;
        let has_value = self.eat_assign() && self.parse_value();
        Some(ParsedAttr {
            span: start.cover(self.prev_span()),
            has_value,
        })
    }

    /// `author` or `"x-jira"`.
    fn parse_key(&mut self) -> Option<Span> {
        if matches!(self.current().kind, T::Ident | T::Str) {
            return Some(self.bump().span);
        }
        let found = self.current().kind.describe();
        self.error_here(format!("expected a key, found {found}"));
        None
    }

    /// Consumes `=`.
    ///
    /// `:` is recognized as a common mix-up and waved through with a
    /// helpful message — the user should learn what the language wants,
    /// instead of reading "unexpected character". (`docs/01`, 1.5)
    fn eat_assign(&mut self) -> bool {
        if self.at(T::Colon) {
            let span = self.current().span;
            self.error(span, "assignment here uses `=`, not `:`");
            self.bump();
            return true;
        }
        self.eat(T::Eq)
    }

    /// Any value in the language.
    fn parse_value(&mut self) -> bool {
        let starts_value = match self.current().kind {
            T::Str | T::Raw | T::Int | T::Float | T::LBracket | T::Ident => true,
            T::Minus => matches!(self.nth(1), T::Int | T::Float),
            _ => false,
        };
        if !starts_value {
            let found = self.current().kind.describe();
            self.error_here(format!("expected a value, found {found}"));
            return false;
        }
        self.in_node(S::Value, Self::parse_value_inner);
        true
    }

    fn parse_value_inner(&mut self) {
        match self.current().kind {
            T::Str | T::Raw | T::Float => {
                self.bump();
            }
            T::Int => {
                let t = self.bump();
                self.check_int(t);
            }
            // `default=-1` – negative numbers are the only "arithmetic" the
            // language has; no expression parser is needed for that.
            T::Minus if matches!(self.nth(1), T::Int | T::Float) => {
                self.bump();
                let t = self.bump();
                if t.kind == T::Int {
                    self.check_int(t);
                }
            }
            T::LBracket => {
                self.in_node(S::List, |p| {
                    p.delimited_list(T::LBracket, T::RBracket, T::Comma, Self::parse_value)
                });
            }
            // `shop.OrderStatus.paid` or, with arguments, `decimal(10)`
            // (`docs/06`, "Layering types" – a type used as the value of
            // `type = …;`).
            T::Ident => {
                if self.parse_path() && self.at(T::LParen) {
                    self.parse_args();
                }
            }
            _ => {
                let found = self.current().kind.describe();
                self.error_here(format!("expected a value, found {found}"));
            }
        }
    }

    /// `shop.OrderStatus.paid`
    fn parse_path(&mut self) -> bool {
        if !self.at(T::Ident) {
            return self.ident();
        }
        self.in_node(S::Path, |p| {
            p.ident();
            // A name missing after a dot is the normal case while typing in
            // an editor. The path stays valid, just shorter.
            while p.eat(T::Dot) && p.ident() {}
        });
        true
    }

    // -- Numbers --------------------------------------------------------------

    /// `1_000` must fit in 64 bits; the value itself is read by the view.
    fn check_int(&mut self, token: Token) {
        let text = self.text_of(token).replace('_', "");
        if text.parse::<i64>().is_err() {
            self.error(token.span, "number is too large for 64 bits");
        }
    }
}

/// Strips the quotes and resolves escape sequences.
///
/// Unknown escapes (`\q`) are left as-is instead of producing an error —
/// in a configuration language, that's the friendlier interpretation.
pub(crate) fn unquote(raw: &str) -> String {
    // Two steps instead of a chain, so an unterminated string (`"oops`)
    // still yields something sensible.
    let inner = raw.strip_prefix('"').unwrap_or(raw);
    let inner = inner.strip_suffix('"').unwrap_or(inner);

    if !inner.contains('\\') {
        return inner.to_owned();
    }

    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{self, AstNode, RelationOp};
    use crate::parse;

    /// Parses and asserts that there were no messages.
    fn ok(src: &str) -> ast::SourceFile {
        let (file, messages) = parse(src);
        let text: Vec<_> = messages.iter().map(|d| d.message.clone()).collect();
        assert!(text.is_empty(), "unexpected messages in {src:?}: {text:#?}");
        file
    }

    /// Parses and returns the message texts.
    fn messages(src: &str) -> Vec<String> {
        parse(src).1.into_iter().map(|d| d.message).collect()
    }

    /// The first block of the file.
    fn block(src: &str) -> ast::Block {
        let file = ok(src);
        file.blocks().next().expect("a block")
    }

    // -- Structure ------------------------------------------------------------

    #[test]
    fn empty_file_is_valid() {
        assert_eq!(ok("").items().count(), 0);
        assert_eq!(ok("# nur ein Kommentar\n").items().count(), 0);
    }

    #[test]
    fn file_header() {
        let file = ok(r#"
            spec 1;
            import "./common.skema";
            import "./de/common.skema" as de;
        "#);
        assert_eq!(file.spec().and_then(|s| s.version()), Some(1));

        let imports: Vec<_> = file
            .imports()
            .map(|i| (i.path(), i.alias().map(|a| a.text().to_owned())))
            .collect();
        assert_eq!(
            imports,
            [
                (Some("./common.skema".to_owned()), None),
                (Some("./de/common.skema".to_owned()), Some("de".to_owned())),
            ]
        );
    }

    #[test]
    fn blocks_nest() {
        let project = block(
            r#"
            Project {
              mode = draft;
              Meta { author = "anna"; }
              Style @table { headerTextColor = "white"; }
              Style highlight { borderWidth = "3px"; }
            }
        "#,
        );
        assert_eq!(project.kind(), BlockKind::Project);
        assert!(project.name().is_none(), "Project has no name");

        let inner: Vec<_> = project
            .items()
            .filter_map(|item| match item.kind() {
                ast::ItemKind::Block(b) => Some((
                    b.kind(),
                    b.selector().map(|s| s.text().to_owned()),
                    b.name().map(|n| n.to_dotted()),
                )),
                _ => None,
            })
            .collect();

        assert_eq!(
            inner,
            [
                (BlockKind::Meta, None, None),
                // `@table` = built-in element type
                (BlockKind::Style, Some("table".to_owned()), None),
                // no `@` = self-chosen style class
                (BlockKind::Style, None, Some("highlight".to_owned())),
            ]
        );
    }

    /// `docs/05`: `Example`/`Record` need no new AST node – a `Record` body
    /// is the same flat `key = value;` shape `Meta`/`Database` already
    /// parse, so this only checks the nesting, not a new node kind.
    #[test]
    fn example_block_holds_record_blocks() {
        let table = block(
            r#"
            Table Post {
              id int [pk];
              Example {
                Record { id = 1; title = "Hello World"; }
                Record { id = 2; title = "Second post"; }
              }
            }
        "#,
        );
        let example = table
            .items()
            .find_map(|item| match item.kind() {
                ast::ItemKind::Block(b) if b.kind() == BlockKind::Example => Some(b),
                _ => None,
            })
            .expect("Table has an Example block");

        let records: Vec<_> = example
            .items()
            .filter_map(|item| match item.kind() {
                ast::ItemKind::Block(b) if b.kind() == BlockKind::Record => Some(b),
                _ => None,
            })
            .collect();
        assert_eq!(records.len(), 2);

        let keys: Vec<_> = records[0]
            .items()
            .filter_map(|item| match item.kind() {
                ast::ItemKind::Setting(a) => Some(a.key().text()),
                _ => None,
            })
            .collect();
        assert_eq!(keys, ["id", "title"]);
    }

    /// The reason for contextual keywords: in an ERP model, `Group`,
    /// `File`, and `Document` are ordinary table names.
    #[test]
    fn block_words_may_serve_as_names() {
        for name in ["Group", "File", "Document", "Index", "Enum"] {
            let b = block(&format!("Table {name} {{ id int [pk]; }}"));
            assert_eq!(b.kind(), BlockKind::Table);
            assert_eq!(b.name().map(|n| n.to_dotted()).as_deref(), Some(name));
        }
    }

    #[test]
    fn unknown_block_words_are_allowed() {
        // `Dialect { postgres { … } }` – the sub-blocks are named after
        // the dialect, not after a keyword of the language.
        let d = block("Dialect { postgres { tablespace = \"fast_ssd\"; } }");
        assert_eq!(d.kind(), BlockKind::Dialect);
        let first = d.items().next().expect("an item");
        assert!(matches!(first.kind(), ast::ItemKind::Field(f) if f.name().text() == "postgres"));
    }

    /// `docs/06`: "Parameters are declared like fields — name, then type."
    #[test]
    fn datatype_parameters_carry_a_type() {
        let dt = block("Datatype decimal(precision int, scale int) { Field { type = float; } }");
        let names: Vec<_> = dt
            .params()
            .map(|p| p.name().map(|n| n.text().to_owned()).unwrap_or_default())
            .collect();
        assert_eq!(names, ["precision", "scale"]);
        let types: Vec<_> = dt.params().map(|p| p.ty().map(|t| t.to_dotted())).collect();
        assert_eq!(types, [Some("int".to_owned()), Some("int".to_owned())]);
    }

    /// `docs/10`: "`[name] (column, …) [flags];`" — the name is optional.
    #[test]
    fn named_index_entry() {
        let t = block(
            r#"
            Table Customer {
              email varchar;
              customer int;
              order_date date;
              ix_customer_order_date (customer, order_date) [unique];
            }
        "#,
        );
        let items: Vec<_> = t.items().collect();
        let ast::ItemKind::IndexEntry(e) = items.last().expect("index entry").kind() else {
            panic!("expected an index entry");
        };
        assert_eq!(
            e.name().map(|n| n.text().to_owned()),
            Some("ix_customer_order_date".to_owned())
        );
        let columns: Vec<_> = e.columns().map(|c| c.text().to_owned()).collect();
        assert_eq!(columns, ["customer", "order_date"]);

        // The unnamed form stays valid.
        let t2 = block("Table T { email varchar; (email) [unique]; }");
        let items2: Vec<_> = t2.items().collect();
        let ast::ItemKind::IndexEntry(e2) = items2.last().expect("index entry").kind() else {
            panic!("expected an index entry");
        };
        assert!(e2.name().is_none());
    }

    // -- Fields ---------------------------------------------------------------

    #[test]
    fn field_kinds() {
        let t = block(
            r#"
            Table Order {
              use common_base;
              email    varchar;
              gross    decimal(10, 2);
              status   shop.OrderStatus [default=shop.OrderStatus.paid];
              customer >Customer.id     [onDelete=CASCADE];
              profil   -Profile.id;
              chef     <Employee.id;
              angelegt timestamp        [default=`now()`];
              summe    float [required] { note = "Die volle Summe"; }
              (email, angelegt) [unique];
            }
        "#,
        );

        let items: Vec<_> = t.items().collect();
        let names: Vec<_> = items
            .iter()
            .filter_map(|item| match item.kind() {
                ast::ItemKind::Field(f) => Some(f.name().text().to_owned()),
                _ => None,
            })
            .collect();
        assert_eq!(
            names,
            [
                "email", "gross", "status", "customer", "profil", "chef", "angelegt", "summe"
            ]
        );

        // `use` is its own statement, not a field.
        assert!(matches!(items[0].kind(), ast::ItemKind::Use(_)));
        // So is the index entry.
        let ast::ItemKind::IndexEntry(idx) = items.last().expect("index").kind() else {
            panic!("expected an index entry");
        };
        let columns: Vec<_> = idx.columns().map(|c| c.text().to_owned()).collect();
        assert_eq!(columns, ["email", "angelegt"]);
        assert_eq!(idx.flags().next().expect("a flag").key().text(), "unique");
    }

    #[test]
    fn relation_operators_are_distinguished() {
        let t = block(
            r#"
            Table A {
              a >B.id;
              b <B.id;
              c -B.id;
            }
        "#,
        );
        let ops: Vec<_> = t
            .items()
            .filter_map(|item| match item.kind() {
                ast::ItemKind::Field(f) => f.ty(),
                _ => None,
            })
            .filter_map(|ty| match ty.kind() {
                Some(ast::FieldTypeKind::Relation { op, .. }) => Some(op),
                _ => None,
            })
            .collect();
        assert_eq!(
            ops,
            [
                RelationOp::OneToMany,
                RelationOp::ManyToOne,
                RelationOp::OneToOne,
            ]
        );
    }

    /// No `<>` operator: `<` followed by `>` is not a many-to-many
    /// relationship, but a back-reference (`<`) followed by a broken
    /// second field.
    #[test]
    fn no_many_to_many_operator() {
        let m = messages("Table A { tags <>Tag.id; }");
        assert!(!m.is_empty(), "`<>` must be flagged as an error: {m:#?}");
    }

    /// Enum values are fields without a type — which is why there's no
    /// separate node for them.
    #[test]
    fn enum_values_are_fields_without_a_type() {
        let e = block(
            r#"
            Enum OrderStatus {
              draft { label = "Entwurf" }
              paid  { label = "Bezahlt" }
              cancelled;
            }
        "#,
        );
        let values: Vec<_> = e
            .items()
            .filter_map(|item| match item.kind() {
                ast::ItemKind::Field(f) => Some((
                    f.name().text().to_owned(),
                    f.ty().is_some(),
                    f.items().count(),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            values,
            [
                ("draft".to_owned(), false, 1),
                ("paid".to_owned(), false, 1),
                ("cancelled".to_owned(), false, 0)
            ]
        );
    }

    /// `docs/06`, "Layering types": a type can build on another one as a
    /// value, e.g. `type = decimal(10);` in the `Field` block of a
    /// `Datatype`.
    #[test]
    fn type_with_arguments_as_a_value() {
        let m = block("Datatype Money { Field { type = decimal(10); } }");
        let ast::ItemKind::Block(field) = m.items().next().expect("an item").kind() else {
            panic!("expected a Field block");
        };
        let ast::ItemKind::Setting(attr) = field.items().next().expect("an item").kind() else {
            panic!("expected an assignment");
        };
        let Some(ast::ValueKind::Type { path, args }) = attr.value().and_then(|v| v.kind()) else {
            panic!(
                "expected a type with arguments, found {:?}",
                attr.value().and_then(|v| v.kind())
            );
        };
        assert_eq!(path.to_dotted(), "decimal");
        assert_eq!(args.len(), 1);
        assert_eq!(args[0].kind(), Some(ast::ValueKind::Int(10)));
    }

    // -- Values -----------------------------------------------------------------

    #[test]
    fn all_value_kinds() {
        let m = block(
            r#"
            Meta {
              text     = "hallo";
              zahl     = 12;
              komma    = 2.5;
              negativ  = -1;
              pfad     = shop.OrderStatus.paid;
              roh      = `now()`;
              liste    = [highlight, wichtig];
              "x-jira" = "SHOP-412";
            }
        "#,
        );

        let values: Vec<_> = m
            .items()
            .filter_map(|item| match item.kind() {
                ast::ItemKind::Setting(a) => Some((a.key().text(), a.key().quoted(), a.value())),
                _ => None,
            })
            .map(|(k, q, v)| (k, q, v.and_then(|v| v.kind())))
            .collect();

        assert_eq!(
            values[0],
            (
                "text".into(),
                false,
                Some(ast::ValueKind::Str("hallo".into()))
            )
        );
        assert_eq!(
            values[1],
            ("zahl".into(), false, Some(ast::ValueKind::Int(12)))
        );
        assert_eq!(
            values[2],
            ("komma".into(), false, Some(ast::ValueKind::Float(2.5)))
        );
        assert_eq!(
            values[3],
            ("negativ".into(), false, Some(ast::ValueKind::Int(-1)))
        );
        assert!(matches!(values[4].2, Some(ast::ValueKind::Path(_))));
        assert_eq!(
            values[5],
            (
                "roh".into(),
                false,
                Some(ast::ValueKind::Raw("now()".into()))
            )
        );
        assert!(matches!(&values[6].2, Some(ast::ValueKind::List(l)) if l.len() == 2));
        // Custom keys must be quoted — the parser only records that; only
        // the checking phase evaluates it.
        assert_eq!(values[7].0, "x-jira");
        assert!(values[7].1, "quoted");
    }

    #[test]
    fn escapes_in_strings() {
        let m = block(r#"Meta { note = "Zeile\nmit \"Zitat\""; }"#);
        let ast::ItemKind::Setting(a) = m.items().next().expect("an item").kind() else {
            panic!("expected an assignment");
        };
        assert_eq!(
            a.value().and_then(|v| v.kind()),
            Some(ast::ValueKind::Str("Zeile\nmit \"Zitat\"".to_owned()))
        );
    }

    #[test]
    fn semicolon_before_closing_brace_is_optional() {
        ok(r#"Enum E { a { label = "A" } }"#);
        ok("Meta { author = \"anna\" }");
        // Between two statements it stays mandatory:
        assert_eq!(
            messages("Meta { author = \"a\" note = \"b\" }").len(),
            1,
            "a missing `;` between two assignments must be reported"
        );
    }

    // -- Severity override -----------------------------------------------------

    /// `docs/04`, "Naming rules": grammar-legal after any setting, in any
    /// block — task 67 restricts where it's *meaningful*.
    #[test]
    fn severity_override_captured_on_a_setting() {
        let text = |src: &str| {
            let b = block(src);
            let ast::ItemKind::Setting(a) = b.items().next().expect("an item").kind() else {
                panic!("expected an assignment");
            };
            a.severity_override().map(|i| i.text().to_owned())
        };
        assert_eq!(
            text("Lint { fieldNaming = snake_case [error]; }"),
            Some("error".to_owned())
        );
        assert_eq!(
            text(r#"Meta { author = "anna" [warn]; }"#),
            Some("warn".to_owned())
        );
    }

    #[test]
    fn severity_override_absent_without_brackets() {
        let b = block("Meta { author = \"anna\"; }");
        let ast::ItemKind::Setting(a) = b.items().next().expect("an item").kind() else {
            panic!("expected an assignment");
        };
        assert!(a.severity_override().is_none());
    }

    /// A field's `[flags]` and an index entry's `[flags]` are a different
    /// grammar position — a bracket there already means something else and
    /// must stay untouched by this feature.
    #[test]
    fn field_and_index_flags_leave_severity_override_none() {
        let t = block(
            r#"
            Table Customer {
              email varchar [unique];
              (email) [unique];
            }
        "#,
        );
        let items: Vec<_> = t.items().collect();
        let ast::ItemKind::Field(f) = items[0].kind() else {
            panic!("expected a field");
        };
        assert!(
            f.flags()
                .next()
                .expect("a flag")
                .severity_override()
                .is_none()
        );
        assert_eq!(f.flags().next().expect("a flag").key().text(), "unique");

        let ast::ItemKind::IndexEntry(idx) = items[1].kind() else {
            panic!("expected an index entry");
        };
        assert!(
            idx.flags()
                .next()
                .expect("a flag")
                .severity_override()
                .is_none()
        );
    }

    #[test]
    fn malformed_severity_override_recovers() {
        for src in [
            "Lint { fieldNaming = snake_case []; }",
            "Lint { fieldNaming = snake_case [a, b]; }",
            "Lint { fieldNaming = snake_case [1]; }",
        ] {
            let (file, messages) = parse(src);
            assert!(!messages.is_empty(), "{src:?} should be reported");
            let lint = file.blocks().next().expect("Lint block");
            let ast::ItemKind::Setting(a) = lint.items().next().expect("an item").kind() else {
                panic!("expected an assignment in {src:?}");
            };
            assert!(
                a.severity_override().is_none(),
                "malformed override must not be captured in {src:?}"
            );
        }
    }

    /// An unterminated `[` must not swallow the next, well-formed setting.
    #[test]
    fn unterminated_severity_override_does_not_swallow_the_next_setting() {
        let (file, messages) = parse(
            r#"
            Lint {
              fieldNaming = snake_case [
              tableNaming = PascalCase;
            }
        "#,
        );
        assert!(
            !messages.is_empty(),
            "the unterminated `[` must be reported"
        );
        let lint = file.blocks().next().expect("Lint block");
        let items: Vec<_> = lint.items().collect();
        let keys: Vec<_> = items
            .iter()
            .filter_map(|item| match item.kind() {
                ast::ItemKind::Setting(a) => Some(a.key().text()),
                _ => None,
            })
            .collect();
        assert_eq!(keys, ["fieldNaming", "tableNaming"]);
        let ast::ItemKind::Setting(second) = items[1].kind() else {
            panic!("expected the second setting");
        };
        assert!(second.value().is_some(), "tableNaming must keep its value");
    }

    // -- Error handling -----------------------------------------------------

    /// The parser's actual promise: a broken field doesn't take its
    /// neighbors down with it.
    #[test]
    fn a_broken_field_stays_local() {
        let (file, messages) = parse(
            r#"
            Table Customer {
              id    int [pk];
              ???   ???
              email varchar [unique];
            }
            Table Order { id int [pk]; }
        "#,
        );
        assert!(!messages.is_empty(), "the garbage must be reported");

        // Both tables are still there …
        let names: Vec<_> = file
            .blocks()
            .filter_map(|b| b.name().map(|n| n.to_dotted()))
            .collect();
        assert_eq!(names, ["Customer", "Order"]);

        // … and so are both fields of the first one.
        let customer = file.blocks().next().expect("Customer");
        let fields: Vec<_> = customer
            .items()
            .filter_map(|item| match item.kind() {
                ast::ItemKind::Field(f) => Some(f.name().text().to_owned()),
                _ => None,
            })
            .collect();
        assert_eq!(fields, ["id", "email"]);
    }

    #[test]
    fn colon_gets_a_helpful_message() {
        let m = messages(r#"Meta { author: "anna"; }"#);
        assert_eq!(m.len(), 1);
        assert!(m[0].contains('='), "message should suggest `=`: {m:?}");
    }

    #[test]
    fn missing_semicolon_between_fields_is_reported() {
        let m = messages("Table A { id int email varchar; }");
        assert_eq!(m.len(), 1, "{m:#?}");
        assert!(m[0].contains("`;`"), "{m:?}");
        // Right before the `}`, on the other hand, it's dispensable:
        ok("Table A { id int }");
    }

    #[test]
    fn missing_brace_at_end_of_file() {
        // The normal case while typing: the block isn't closed yet.
        let (file, messages) = parse("Table A { id int [pk];");
        assert_eq!(messages.len(), 1);
        // The tree is still usable — the basis for autocompletion.
        let a = file.blocks().next().expect("Table A");
        assert_eq!(a.items().count(), 1);
    }

    #[test]
    fn cascading_errors_are_suppressed() {
        // A single broken spot must not produce three messages.
        let m = messages("Meta { = ; }");
        assert!(m.len() <= 2, "too many cascading errors: {m:#?}");
    }

    /// A simple fuzzer: every combination of three tokens from a small set
    /// of characters. Only one thing is checked — that the parser
    /// **returns**.
    ///
    /// That's exactly the property you can't guarantee by reasoning about
    /// it: infinite loops arise from combinations nobody thinks of while
    /// writing the code.
    #[test]
    fn parser_terminates_on_arbitrary_garbage() {
        let pieces = [
            "Table", "{", "}", "[", "]", "(", ")", ";", "=", ",", ".", "@", ">", "<>", "\"a\"",
            "`r`", "1", "x", "use", "spec", "import", ":",
        ];
        for a in pieces {
            for b in pieces {
                for c in pieces {
                    let src = format!("{a} {b} {c}");
                    let (_, _) = parse(&src); // must neither hang nor panic
                }
            }
        }
    }

    // -- Spans --------------------------------------------------------------

    #[test]
    fn spans_point_at_the_right_text() {
        let src = "Table Customer { email varchar; }";
        let file = ok(src);
        let t = file.blocks().next().expect("Table");

        let file_items: Vec<_> = file.items().collect();
        assert_eq!(file_items[0].span().text(src), src);
        let t_items: Vec<_> = t.items().collect();
        assert_eq!(t_items[0].span().text(src), "email varchar;");
        let ast::ItemKind::Field(f) = t_items[0].kind() else {
            panic!("expected a field");
        };
        assert_eq!(f.name().span().text(src), "email");
        assert_eq!(f.ty().map(|ty| ty.span().text(src)), Some("varchar"));
    }
}
