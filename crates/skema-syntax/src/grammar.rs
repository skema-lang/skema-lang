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

use crate::ast::*;
use crate::parser::Parser;
use crate::span::Span;
use crate::token::Token;
use crate::token::TokenKind as T;

impl Parser<'_> {
    /// Entry point: reads a whole file.
    pub fn parse_source_file(&mut self) -> SourceFile {
        let mut items = Vec::new();
        self.repeat_until(T::Eof, |p| items.push(p.parse_item()));
        SourceFile {
            span: Span::new(0, self.prev_span().end),
            items,
        }
    }

    /// `{ item* }` — the body of every block.
    ///
    /// **One** function for all fourteen block kinds. That `Lint` may only
    /// appear inside `Project` is a rule about meaning and is checked
    /// later, not here.
    fn parse_body(&mut self) -> Vec<Item> {
        let mut items = Vec::new();
        if !self.expect(T::LBrace) {
            return items;
        }
        self.repeat_until(T::RBrace, |p| items.push(p.parse_item()));
        self.expect(T::RBrace);
        items
    }

    // -- Statements -------------------------------------------------------

    /// A single statement, including its span.
    fn parse_item(&mut self) -> Item {
        let start = self.current().span;
        let kind = self.parse_item_kind();
        Item {
            kind,
            span: start.cover(self.prev_span()),
        }
    }

    /// The dispatch. It looks at at most two tokens — the language doesn't
    /// need more.
    fn parse_item_kind(&mut self) -> ItemKind {
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
                ItemKind::Error
            }
        }
    }

    /// `spec 1;`
    fn parse_spec(&mut self) -> ItemKind {
        self.bump(); // `spec`
        let version = if self.at(T::Int) {
            let t = self.bump();
            self.int_value(t)
        } else {
            self.error_here("expected an integer, e.g. `spec 1;`");
            None
        };
        self.expect_terminator();
        ItemKind::Spec(Spec { version })
    }

    /// `import "./common.skema" as base;`
    fn parse_import(&mut self) -> ItemKind {
        self.bump(); // `import`
        let path = if self.at(T::Str) {
            let t = self.bump();
            Some(unquote(self.text_of(t)))
        } else {
            self.error_here("expected a quoted path");
            None
        };
        // `as` is optional: without an alias, the source file's document
        // name applies.
        let alias = if self.eat(T::KwAs) {
            self.ident()
        } else {
            None
        };
        self.expect_terminator();
        ItemKind::Import(Import { path, alias })
    }

    /// `use common_base;`
    fn parse_use(&mut self) -> ItemKind {
        self.bump(); // `use`
        let Some(target) = self.parse_path() else {
            self.recover_item();
            return ItemKind::Error;
        };
        self.expect_terminator();
        ItemKind::Use(Use { target })
    }

    /// `mode = draft;`, `fieldNaming = snake_case [error];`
    fn parse_setting(&mut self) -> ItemKind {
        let Some(mut attr) = self.parse_attr() else {
            self.recover_item();
            return ItemKind::Error;
        };
        if attr.value.is_none() {
            self.error(attr.span, "a value is missing here: `key = value;`");
        }
        if self.at(T::LBracket) {
            attr.severity_override = self.parse_severity_override();
        }
        self.expect_terminator();
        ItemKind::Setting(attr)
    }

    /// `[error]` on a setting — a severity override, not a flag list: one
    /// bare word, no comma (task 67 gives it meaning; here it's grammar
    /// only, legal after any setting in any block, `docs/13`).
    fn parse_severity_override(&mut self) -> Option<Ident> {
        let start = self.current().span;
        self.bump(); // `[`
        if self.at(T::Ident) && self.nth(1) == T::RBracket {
            let word = self.ident();
            self.bump(); // `]`
            return word;
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
        None
    }

    /// `Table Customer { … }`, `Style @table { … }`, `Table shop.User;`
    ///
    /// Block and reference share everything but the last character —
    /// that's why they share a function too.
    fn parse_block_or_ref(&mut self) -> ItemKind {
        let Some(keyword) = self.ident() else {
            self.recover_item();
            return ItemKind::Error;
        };
        let kind = BlockKind::from_keyword(&keyword.text).unwrap_or(BlockKind::Other);

        // `@` = built-in element type, no `@` = self-chosen name.
        let selector = if self.eat(T::At) { self.ident() } else { None };

        let name = if self.at(T::Ident) {
            self.parse_path()
        } else {
            None
        };

        // Only `Datatype` has parameters: `Datatype decimal(precision int, scale int)`.
        let params = if self.at(T::LParen) {
            self.parse_params()
        } else {
            Vec::new()
        };

        if self.at(T::LBrace) {
            let body_brace = self.current().span;
            let items = self.parse_body();
            self.eat(T::Semi); // `};` is tolerated, the formatter cleans it up
            ItemKind::Block(Block {
                keyword,
                kind,
                selector,
                name,
                params,
                body_brace,
                items,
            })
        } else if let Some(path) = name {
            // No body, but a name: a reference like in `Group`.
            self.expect_terminator();
            ItemKind::Ref(Ref {
                keyword,
                kind,
                path,
            })
        } else {
            self.error_here("expected `{` or a name");
            self.recover_item();
            ItemKind::Error
        }
    }

    /// `email varchar [unique] { note = "…" };`
    fn parse_field(&mut self) -> ItemKind {
        let Some(name) = self.ident() else {
            self.recover_item();
            return ItemKind::Error;
        };
        let ty = self.parse_field_type();
        let flags = if self.at(T::LBracket) {
            self.parse_flags()
        } else {
            Vec::new()
        };

        // If the statement ends with `}`, the `;` is superfluous (docs/01, 1.4).
        let items = if self.at(T::LBrace) {
            let items = self.parse_body();
            self.eat(T::Semi);
            items
        } else {
            self.expect_terminator();
            Vec::new()
        };

        ItemKind::Field(Field {
            name,
            ty,
            flags,
            items,
        })
    }

    /// `[name] (email, date) [unique];` – the name is optional (`docs/10`).
    fn parse_index_entry(&mut self) -> ItemKind {
        let name = if self.at(T::Ident) {
            self.ident()
        } else {
            None
        };
        let columns = self.parse_ident_list();
        if columns.is_empty() {
            self.error_here("an index needs at least one column");
        }
        let flags = if self.at(T::LBracket) {
            self.parse_flags()
        } else {
            Vec::new()
        };
        self.expect_terminator();
        ItemKind::IndexEntry(IndexEntry {
            name,
            columns,
            flags,
        })
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
    fn parse_field_type(&mut self) -> Option<FieldType> {
        let start = self.current().span;

        // `customer >Customer.id`
        if self.current().kind.is_relation_op() {
            let op = match self.bump().kind {
                T::Gt => RelationOp::OneToMany,
                T::Lt => RelationOp::ManyToOne,
                _ => RelationOp::OneToOne,
            };
            let target = self.parse_path()?;
            return Some(FieldType {
                kind: FieldTypeKind::Relation { op, target },
                span: start.cover(self.prev_span()),
            });
        }

        // `varchar`, `decimal(10, 2)`, `shop.OrderStatus`
        if self.at(T::Ident) {
            let path = self.parse_path()?;
            let args = if self.at(T::LParen) {
                self.delimited_list(T::LParen, T::RParen, T::Comma, |p| p.parse_value())
            } else {
                Vec::new()
            };
            return Some(FieldType {
                kind: FieldTypeKind::Named { path, args },
                span: start.cover(self.prev_span()),
            });
        }

        None
    }

    /// `[pk, required, default=0]`
    fn parse_flags(&mut self) -> Vec<Attr> {
        self.delimited_list(T::LBracket, T::RBracket, T::Comma, |p| p.parse_attr())
    }

    /// `(a, b)` — for `Index` columns. `Datatype` parameters additionally
    /// have a type and are therefore read by [`Self::parse_params`].
    fn parse_ident_list(&mut self) -> Vec<Ident> {
        self.delimited_list(T::LParen, T::RParen, T::Comma, |p| p.ident())
    }

    /// `(precision int, scale int)` — the parameter list of a `Datatype`.
    fn parse_params(&mut self) -> Vec<Param> {
        self.delimited_list(T::LParen, T::RParen, T::Comma, |p| p.parse_param())
    }

    /// A single parameter: "declared like a field — name, then type"
    /// (`docs/06`).
    fn parse_param(&mut self) -> Option<Param> {
        let name = self.ident()?;
        let ty = if self.at(T::Ident) {
            self.parse_path()
        } else {
            None
        };
        Some(Param {
            span: name.span.cover(self.prev_span()),
            name,
            ty,
        })
    }

    /// `key` or `key = value`.
    fn parse_attr(&mut self) -> Option<Attr> {
        let key = self.parse_key()?;
        let start = key.span;
        let value = if self.eat_assign() {
            self.parse_value()
        } else {
            None
        };
        Some(Attr {
            key,
            value,
            severity_override: None,
            span: start.cover(self.prev_span()),
        })
    }

    /// `author` or `"x-jira"`.
    fn parse_key(&mut self) -> Option<Key> {
        match self.current().kind {
            T::Ident => {
                let t = self.bump();
                Some(Key {
                    text: self.text_of(t).to_owned(),
                    quoted: false,
                    span: t.span,
                })
            }
            T::Str => {
                let t = self.bump();
                Some(Key {
                    text: unquote(self.text_of(t)),
                    quoted: true,
                    span: t.span,
                })
            }
            _ => {
                let found = self.current().kind.describe();
                self.error_here(format!("expected a key, found {found}"));
                None
            }
        }
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
    fn parse_value(&mut self) -> Option<Value> {
        let start = self.current().span;

        let kind = match self.current().kind {
            T::Str => {
                let t = self.bump();
                ValueKind::Str(unquote(self.text_of(t)))
            }
            // Strip the backticks: the content is what gets passed through.
            T::Raw => {
                let t = self.bump();
                let text = self.text_of(t);
                ValueKind::Raw(text[1..text.len().saturating_sub(1)].to_owned())
            }
            T::Int => {
                let t = self.bump();
                ValueKind::Int(self.int_value(t).unwrap_or(0))
            }
            T::Float => {
                let t = self.bump();
                ValueKind::Float(self.float_value(t))
            }
            // `default=-1` – negative numbers are the only "arithmetic" the
            // language has; no expression parser is needed for that.
            T::Minus if matches!(self.nth(1), T::Int | T::Float) => {
                self.bump();
                let t = self.bump();
                if t.kind == T::Int {
                    ValueKind::Int(-self.int_value(t).unwrap_or(0))
                } else {
                    ValueKind::Float(-self.float_value(t))
                }
            }
            T::LBracket => {
                let values =
                    self.delimited_list(T::LBracket, T::RBracket, T::Comma, |p| p.parse_value());
                ValueKind::List(values)
            }
            // `shop.OrderStatus.paid` or, with arguments, `decimal(10)`
            // (`docs/06`, "Layering types" – a type used as the value of
            // `type = …;`).
            T::Ident => {
                let path = self.parse_path()?;
                if self.at(T::LParen) {
                    let args =
                        self.delimited_list(T::LParen, T::RParen, T::Comma, |p| p.parse_value());
                    ValueKind::Type { path, args }
                } else {
                    ValueKind::Path(path)
                }
            }
            _ => {
                let found = self.current().kind.describe();
                self.error_here(format!("expected a value, found {found}"));
                return None;
            }
        };

        Some(Value {
            kind,
            span: start.cover(self.prev_span()),
        })
    }

    /// `shop.OrderStatus.paid`
    fn parse_path(&mut self) -> Option<Path> {
        let first = self.ident()?;
        let start = first.span;
        let mut segments = vec![first];
        while self.eat(T::Dot) {
            match self.ident() {
                Some(i) => segments.push(i),
                // The name is missing after a dot — the normal case while
                // typing in an editor. The path stays valid, just shorter.
                None => break,
            }
        }
        Some(Path {
            span: start.cover(self.prev_span()),
            segments,
        })
    }

    // -- Numbers --------------------------------------------------------------

    /// `1_000` → `1000`. `None` on overflow, with a message.
    fn int_value(&mut self, token: Token) -> Option<i64> {
        let text = self.text_of(token).replace('_', "");
        match text.parse::<i64>() {
            Ok(n) => Some(n),
            Err(_) => {
                self.error(token.span, "number is too large for 64 bits");
                None
            }
        }
    }

    /// Can't fail: the lexer only lets `digits.digits` through. For
    /// absurdly large values, Rust yields `inf` instead of an error.
    fn float_value(&mut self, token: Token) -> f64 {
        self.text_of(token).replace('_', "").parse().unwrap_or(0.0)
    }
}

/// Strips the quotes and resolves escape sequences.
///
/// Unknown escapes (`\q`) are left as-is instead of producing an error —
/// in a configuration language, that's the friendlier interpretation.
fn unquote(raw: &str) -> String {
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
    use crate::parse;

    /// Parses and asserts that there were no messages.
    fn ok(src: &str) -> SourceFile {
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
    fn block(src: &str) -> Block {
        let file = ok(src);
        file.blocks().next().cloned().expect("a block")
    }

    // -- Structure ------------------------------------------------------------

    #[test]
    fn empty_file_is_valid() {
        assert_eq!(ok("").items.len(), 0);
        assert_eq!(ok("# nur ein Kommentar\n").items.len(), 0);
    }

    #[test]
    fn file_header() {
        let file = ok(r#"
            spec 1;
            import "./common.skema";
            import "./de/common.skema" as de;
        "#);
        assert_eq!(file.spec().and_then(|s| s.version), Some(1));

        let imports: Vec<_> = file
            .imports()
            .map(|i| (i.path.clone(), i.alias.as_ref().map(|a| a.text.clone())))
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
        assert_eq!(project.kind, BlockKind::Project);
        assert!(project.name.is_none(), "Project has no name");

        let inner: Vec<_> = project
            .items
            .iter()
            .filter_map(|i| match &i.kind {
                ItemKind::Block(b) => Some((
                    b.kind,
                    b.selector.as_ref().map(|s| s.text.clone()),
                    b.name.as_ref().map(|n| n.to_dotted()),
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
            .items
            .iter()
            .find_map(|i| match &i.kind {
                ItemKind::Block(b) if b.kind == BlockKind::Example => Some(b),
                _ => None,
            })
            .expect("Table has an Example block");

        let records: Vec<_> = example
            .items
            .iter()
            .filter_map(|i| match &i.kind {
                ItemKind::Block(b) if b.kind == BlockKind::Record => Some(b),
                _ => None,
            })
            .collect();
        assert_eq!(records.len(), 2);

        let keys: Vec<_> = records[0]
            .items
            .iter()
            .filter_map(|i| match &i.kind {
                ItemKind::Setting(a) => Some(a.key.text.as_str()),
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
            assert_eq!(b.kind, BlockKind::Table);
            assert_eq!(b.name.map(|n| n.to_dotted()).as_deref(), Some(name));
        }
    }

    #[test]
    fn unknown_block_words_are_allowed() {
        // `Dialect { postgres { … } }` – the sub-blocks are named after
        // the dialect, not after a keyword of the language.
        let d = block("Dialect { postgres { tablespace = \"fast_ssd\"; } }");
        assert_eq!(d.kind, BlockKind::Dialect);
        assert!(matches!(&d.items[0].kind, ItemKind::Field(f) if f.name.text == "postgres"));
    }

    /// `docs/06`: "Parameters are declared like fields — name, then type."
    #[test]
    fn datatype_parameters_carry_a_type() {
        let dt = block("Datatype decimal(precision int, scale int) { Field { type = float; } }");
        let names: Vec<_> = dt.params.iter().map(|p| p.name.text.as_str()).collect();
        assert_eq!(names, ["precision", "scale"]);
        let types: Vec<_> = dt
            .params
            .iter()
            .map(|p| p.ty.as_ref().map(|t| t.to_dotted()))
            .collect();
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
        let ItemKind::IndexEntry(e) = &t.items.last().expect("index entry").kind else {
            panic!("expected an index entry");
        };
        assert_eq!(
            e.name.as_ref().map(|n| n.text.as_str()),
            Some("ix_customer_order_date")
        );
        let columns: Vec<_> = e.columns.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(columns, ["customer", "order_date"]);

        // The unnamed form stays valid.
        let t2 = block("Table T { email varchar; (email) [unique]; }");
        let ItemKind::IndexEntry(e2) = &t2.items.last().expect("index entry").kind else {
            panic!("expected an index entry");
        };
        assert!(e2.name.is_none());
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

        let names: Vec<_> = t
            .items
            .iter()
            .filter_map(|i| match &i.kind {
                ItemKind::Field(f) => Some(f.name.text.as_str()),
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
        assert!(matches!(&t.items[0].kind, ItemKind::Use(_)));
        // So is the index entry.
        let ItemKind::IndexEntry(idx) = &t.items.last().expect("index").kind else {
            panic!("expected an index entry");
        };
        let columns: Vec<_> = idx.columns.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(columns, ["email", "angelegt"]);
        assert_eq!(idx.flags[0].key.text, "unique");
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
            .items
            .iter()
            .filter_map(|i| match &i.kind {
                ItemKind::Field(f) => f.ty.as_ref(),
                _ => None,
            })
            .filter_map(|ty| match &ty.kind {
                FieldTypeKind::Relation { op, .. } => Some(*op),
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
            .items
            .iter()
            .filter_map(|i| match &i.kind {
                ItemKind::Field(f) => Some((f.name.text.as_str(), f.ty.is_some(), f.items.len())),
                _ => None,
            })
            .collect();
        assert_eq!(
            values,
            [
                ("draft", false, 1),
                ("paid", false, 1),
                ("cancelled", false, 0)
            ]
        );
    }

    /// `docs/06`, "Layering types": a type can build on another one as a
    /// value, e.g. `type = decimal(10);` in the `Field` block of a
    /// `Datatype`.
    #[test]
    fn type_with_arguments_as_a_value() {
        let m = block("Datatype Money { Field { type = decimal(10); } }");
        let ItemKind::Block(field) = &m.items[0].kind else {
            panic!("expected a Field block");
        };
        let ItemKind::Setting(attr) = &field.items[0].kind else {
            panic!("expected an assignment");
        };
        let Some(Value {
            kind: ValueKind::Type { path, args },
            ..
        }) = &attr.value
        else {
            panic!("expected a type with arguments, found {:?}", attr.value);
        };
        assert_eq!(path.to_dotted(), "decimal");
        assert_eq!(args.len(), 1);
        assert_eq!(args[0].kind, ValueKind::Int(10));
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
            .items
            .iter()
            .filter_map(|i| match &i.kind {
                ItemKind::Setting(a) => Some((a.key.text.clone(), a.key.quoted, a.value.clone())),
                _ => None,
            })
            .map(|(k, q, v)| (k, q, v.map(|v| v.kind)))
            .collect();

        assert_eq!(
            values[0],
            ("text".into(), false, Some(ValueKind::Str("hallo".into())))
        );
        assert_eq!(values[1], ("zahl".into(), false, Some(ValueKind::Int(12))));
        assert_eq!(
            values[2],
            ("komma".into(), false, Some(ValueKind::Float(2.5)))
        );
        assert_eq!(
            values[3],
            ("negativ".into(), false, Some(ValueKind::Int(-1)))
        );
        assert!(matches!(values[4].2, Some(ValueKind::Path(_))));
        assert_eq!(
            values[5],
            ("roh".into(), false, Some(ValueKind::Raw("now()".into())))
        );
        assert!(matches!(&values[6].2, Some(ValueKind::List(l)) if l.len() == 2));
        // Custom keys must be quoted — the parser only records that; only
        // the checking phase evaluates it.
        assert_eq!(values[7].0, "x-jira");
        assert!(values[7].1, "quoted");
    }

    #[test]
    fn escapes_in_strings() {
        let m = block(r#"Meta { note = "Zeile\nmit \"Zitat\""; }"#);
        let ItemKind::Setting(a) = &m.items[0].kind else {
            panic!("expected an assignment");
        };
        assert_eq!(
            a.value.as_ref().map(|v| &v.kind),
            Some(&ValueKind::Str("Zeile\nmit \"Zitat\"".to_owned()))
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
            let ItemKind::Setting(a) = &b.items[0].kind else {
                panic!("expected an assignment");
            };
            a.severity_override.as_ref().map(|i| i.text.clone())
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
        let ItemKind::Setting(a) = &b.items[0].kind else {
            panic!("expected an assignment");
        };
        assert!(a.severity_override.is_none());
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
        let ItemKind::Field(f) = &t.items[0].kind else {
            panic!("expected a field");
        };
        assert!(f.flags[0].severity_override.is_none());
        assert_eq!(f.flags[0].key.text, "unique");

        let ItemKind::IndexEntry(idx) = &t.items[1].kind else {
            panic!("expected an index entry");
        };
        assert!(idx.flags[0].severity_override.is_none());
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
            let ItemKind::Setting(a) = &lint.items[0].kind else {
                panic!("expected an assignment in {src:?}");
            };
            assert!(
                a.severity_override.is_none(),
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
        let keys: Vec<_> = lint
            .items
            .iter()
            .filter_map(|i| match &i.kind {
                ItemKind::Setting(a) => Some(a.key.text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(keys, ["fieldNaming", "tableNaming"]);
        let ItemKind::Setting(second) = &lint.items[1].kind else {
            panic!("expected the second setting");
        };
        assert!(second.value.is_some(), "tableNaming must keep its value");
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
            .filter_map(|b| b.name.as_ref().map(|n| n.to_dotted()))
            .collect();
        assert_eq!(names, ["Customer", "Order"]);

        // … and so are both fields of the first one.
        let customer = file.blocks().next().expect("Customer");
        let fields: Vec<_> = customer
            .items
            .iter()
            .filter_map(|i| match &i.kind {
                ItemKind::Field(f) => Some(f.name.text.as_str()),
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
        assert_eq!(a.items.len(), 1);
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

        assert_eq!(file.items[0].span.text(src), src);
        assert_eq!(t.items[0].span.text(src), "email varchar;");
        let ItemKind::Field(f) = &t.items[0].kind else {
            panic!("expected a field");
        };
        assert_eq!(f.name.span.text(src), "email");
        assert_eq!(f.ty.as_ref().map(|ty| ty.span.text(src)), Some("varchar"));
    }
}
