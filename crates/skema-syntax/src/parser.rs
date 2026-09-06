//! The parser's toolkit: cursor, errors, recovery.
//!
//! This is **how** things are read. The grammar — **what** is read — lives
//! in the `grammar` module. The split pays off because this module is done
//! once it's written, while the grammar keeps moving with every language
//! change.
//!
//! # What "recursive descent" means
//!
//! For every grammar rule there is exactly one function, and nesting in
//! the language turns into recursion in the code:
//!
//! ```text
//! Grammar                        Rust
//! ─────────────────────────────  ──────────────────────────
//! block  := Name "{" item* "}"   fn parse_block()
//! item   := field | block | …    fn parse_item()   ← calls parse_block()
//! ```
//!
//! You don't need more theory than that. The price is that the grammar
//! must be "LL(k)": which rule applies must be decidable from the next one
//! or two tokens. That's exactly what [`Parser::nth`] is for.
//!
//! # The three rules that make a parser IDE-ready
//!
//! 1. **Never abort.** A syntax error produces a message and a placeholder
//!    node, not an `Err`. The rest of the file still gets parsed.
//! 2. **Never get stuck.** Every loop must consume at least one token per
//!    iteration — see [`Parser::repeat_until`]. That's *the* classic bug
//!    in hand-written parsers.
//! 3. **Never panic.** Broken input is the normal case, not the exception.

use crate::diagnostic::Diagnostic;
use crate::span::Span;
use crate::token::{Token, TokenKind};
use crate::{ast, tokenize};

pub struct Parser<'a> {
    src: &'a str,
    /// **Without trivia.** Whitespace and comments are filtered out once,
    /// so the grammar doesn't have to skip them everywhere. Whoever needs
    /// them (a formatter) re-lexes the text with [`tokenize`] — that costs
    /// nothing and keeps the grammar readable.
    tokens: Vec<Token>,
    /// Index into `tokens`.
    pos: usize,
    diagnostics: Vec<Diagnostic>,
    /// Position of the most recently reported error.
    ///
    /// Prevents message avalanches: as long as no token has been
    /// consumed, every further message is just a consequence of the first
    /// and helps no one.
    last_error_pos: Option<usize>,
}

impl<'a> Parser<'a> {
    pub fn new(src: &'a str) -> Self {
        let all = tokenize(src);

        // Report lexer errors once, centrally. After that, the grammar
        // treats them like ordinary unexpected tokens, without reporting
        // them again.
        let diagnostics = all
            .iter()
            .filter(|t| t.kind.is_error())
            .map(|t| Diagnostic::error(t.span, t.kind.describe()))
            .collect();

        Self {
            src,
            tokens: all.into_iter().filter(|t| !t.kind.is_trivia()).collect(),
            pos: 0,
            diagnostics,
            last_error_pos: None,
        }
    }

    /// Returns the collected messages, sorted by position.
    ///
    /// Sorted because lexer messages arise up front and parser messages
    /// later — in the editor, they should still appear in reading order.
    pub fn finish(mut self) -> Vec<Diagnostic> {
        self.diagnostics.sort_by_key(|d| (d.span.start, d.span.end));
        self.diagnostics
    }

    // -- Cursor ---------------------------------------------------------------

    /// The current token. Always `Eof` after the end — so never an `Option`.
    pub(crate) fn current(&self) -> Token {
        self.nth_token(0)
    }

    /// The kind of the `n`-th token from here (0 = current). This is the
    /// "lookahead" the grammar uses to decide which rule applies.
    pub(crate) fn nth(&self, n: usize) -> TokenKind {
        self.nth_token(n).kind
    }

    fn nth_token(&self, n: usize) -> Token {
        // Past the end, we return the Eof token instead of panicking.
        match self.tokens.get(self.pos + n) {
            Some(t) => *t,
            None => Token::new(TokenKind::Eof, Span::empty(self.src.len() as u32)),
        }
    }

    pub(crate) fn at(&self, kind: TokenKind) -> bool {
        self.current().kind == kind
    }

    /// The source text of the current token — e.g. to recognize `Table`
    /// as a block word.
    pub(crate) fn current_text(&self) -> &'a str {
        self.current().text(self.src)
    }

    /// The span of the most recently consumed token. This is how the
    /// grammar assembles a node's span: `start.cover(self.prev_span())`.
    pub(crate) fn prev_span(&self) -> Span {
        match self.pos.checked_sub(1).and_then(|i| self.tokens.get(i)) {
            Some(t) => t.span,
            None => Span::empty(0),
        }
    }

    /// Consumes the current token and returns it.
    pub(crate) fn bump(&mut self) -> Token {
        let t = self.current();
        if t.kind != TokenKind::Eof {
            self.pos += 1;
        }
        t
    }

    /// Consumes the token, if it matches.
    pub(crate) fn eat(&mut self, kind: TokenKind) -> bool {
        let matched = self.at(kind);
        if matched {
            self.bump();
        }
        matched
    }

    /// Like [`Self::eat`], but reports an error if it doesn't match.
    ///
    /// Consumes **nothing** on failure — the caller decides whether to
    /// continue or recover.
    pub(crate) fn expect(&mut self, kind: TokenKind) -> bool {
        if self.eat(kind) {
            return true;
        }
        let found = self.current().kind.describe();
        self.error_here(format!("expected {}, found {found}", kind.describe()));
        false
    }

    // -- Messages ---------------------------------------------------------------

    /// Reports an error at the current position.
    pub(crate) fn error_here(&mut self, message: impl Into<String>) {
        let span = self.current().span;
        self.error(span, message);
    }

    pub(crate) fn error(&mut self, span: Span, message: impl Into<String>) {
        // Suppress cascading errors: if no token has been consumed since
        // the last message, the new message describes the same broken spot.
        if self.last_error_pos == Some(self.pos) {
            return;
        }
        self.last_error_pos = Some(self.pos);
        self.diagnostics.push(Diagnostic::error(span, message));
    }

    // -- Recovery ---------------------------------------------------------------

    /// Skips the rest of a broken statement.
    ///
    /// This is called **panic-mode recovery**: the parser no longer knows
    /// where it is and looks for a point where it can safely resume. In
    /// this language, there are exactly two such points — the `;` at the
    /// end of a statement, and the `}` of the surrounding block.
    ///
    /// Bracket depth is tracked so that a `}` from a skipped sub-block
    /// doesn't prematurely close the outer block. The `}` of the current
    /// level is left alone — it belongs to the caller.
    ///
    /// # The line break as an emergency brake
    ///
    /// For the *grammar*, a line break is meaningless (see [`TokenKind`]).
    /// For *error recovery*, it's the best hint there is: whatever's on
    /// the next line is, with high probability, a new statement.
    ///
    /// Without this brake, a typo missing a `;` would swallow everything
    /// up to the next valid statement — the user would see a perfectly
    /// fine field disappear. Inside brackets the rule doesn't apply: line
    /// breaks are common there and mean nothing.
    pub(crate) fn recover_item(&mut self) {
        let mut depth = 0_u32;
        loop {
            if depth == 0 && self.line_break_before_current() {
                return;
            }
            match self.current().kind {
                TokenKind::Eof => return,
                TokenKind::RBrace if depth == 0 => return,
                TokenKind::Semi if depth == 0 => {
                    self.bump();
                    return;
                }
                TokenKind::LBrace | TokenKind::LBracket | TokenKind::LParen => {
                    depth += 1;
                    self.bump();
                }
                TokenKind::RBrace | TokenKind::RBracket | TokenKind::RParen => {
                    depth = depth.saturating_sub(1);
                    self.bump();
                }
                _ => {
                    self.bump();
                }
            }
        }
    }

    /// Is there a line break between the most recently consumed token and
    /// the current one?
    ///
    /// This is only possible because the token stream is lossless: what
    /// lies between two tokens is exactly the slice between their spans.
    fn line_break_before_current(&self) -> bool {
        let from = self.prev_span().end as usize;
        let to = self.current().span.start as usize;
        self.src
            .get(from..to)
            .is_some_and(|between| between.contains('\n'))
    }

    // -- Loop building blocks -----------------------------------------------

    /// Repeats `step` until `end` or the end of the file is reached.
    ///
    /// # The progress guarantee
    ///
    /// If one iteration consumes **no** token, the loop runs forever. That
    /// happens faster than you'd think: it only takes one grammar rule
    /// that, on an unexpected token, just reports and returns. That's why
    /// this function checks progress and, if necessary, forcibly consumes
    /// a token.
    ///
    /// This branch is a safety net, not the normal path: the grammar
    /// itself should ensure it makes progress.
    pub(crate) fn repeat_until(&mut self, end: TokenKind, mut step: impl FnMut(&mut Self)) {
        while !self.at(end) && !self.at(TokenKind::Eof) {
            let before = self.pos;
            step(self);
            if self.pos == before {
                let t = self.bump();
                self.error(t.span, format!("unexpected: {}", t.kind.describe()));
            }
        }
    }

    /// Reads `open element sep element … close`.
    ///
    /// Covers `[pk, required]`, `(10, 2)`, and `[highlight]` with a single
    /// function — the three differ only in the element parser. A trailing
    /// separator (`[a, b,]`) is allowed because it's convenient when
    /// editing line by line and costs nothing.
    pub(crate) fn delimited_list<T>(
        &mut self,
        open: TokenKind,
        close: TokenKind,
        sep: TokenKind,
        mut element: impl FnMut(&mut Self) -> Option<T>,
    ) -> Vec<T> {
        let mut out = Vec::new();
        if !self.expect(open) {
            return out;
        }
        self.repeat_until(close, |p| {
            if let Some(x) = element(p) {
                out.push(x);
            }
            // No separator and not at the end either? Report it, but keep
            // going — a forgotten comma shouldn't make the rest of the
            // list unreadable.
            if !p.eat(sep) && !p.at(close) && !p.at(TokenKind::Eof) {
                p.error_here(format!(
                    "expected {} or {}",
                    sep.describe(),
                    close.describe()
                ));
            }
        });
        self.expect(close);
        out
    }

    // -- Small helpers for the grammar ---------------------------------------

    /// Reads an identifier. Reports and returns `None` if there isn't one.
    pub(crate) fn ident(&mut self) -> Option<ast::Ident> {
        if !self.at(TokenKind::Ident) {
            let found = self.current().kind.describe();
            self.error_here(format!("expected an identifier, found {found}"));
            return None;
        }
        let t = self.bump();
        Some(ast::Ident {
            text: t.text(self.src).to_owned(),
            span: t.span,
        })
    }

    /// The text of an arbitrary token.
    pub(crate) fn text_of(&self, token: Token) -> &'a str {
        token.text(self.src)
    }
}

// `Debug` by hand, because `tokens` in the output would only get in the way.
impl std::fmt::Debug for Parser<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parser")
            .field("pos", &self.pos)
            .field("current", &self.current().kind)
            .field("diagnostics", &self.diagnostics.len())
            .finish()
    }
}
