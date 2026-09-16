//! The parser's toolkit: cursor, errors, recovery.
//!
//! This is **how** things are read; the grammar — **what** is read — lives
//! in the `grammar` module. This module stops changing once written, the
//! grammar moves with every language change.
//!
//! Recursive descent: one function per grammar rule, nesting in the
//! language becomes recursion in the code. That requires an LL(k) grammar —
//! the applicable rule must be decidable from the next token or two, which
//! is what [`Parser::nth`] is for.
//!
//! # The three rules that make a parser IDE-ready
//!
//! 1. **Never abort.** A syntax error yields a message and a placeholder
//!    node, not an `Err`; the rest of the file is still parsed.
//! 2. **Never get stuck.** Every loop consumes at least one token per
//!    iteration — see [`Parser::repeat_until`].
//! 3. **Never panic.** Broken input is the normal case.

use crate::diagnostic::Diagnostic;
use crate::span::Span;
use crate::token::{Token, TokenKind};
use crate::{ast, tokenize};

pub struct Parser<'a> {
    src: &'a str,
    /// **Without trivia**, filtered once so the grammar never skips it.
    /// Anything that needs trivia re-lexes with [`tokenize`].
    tokens: Vec<Token>,
    /// Index into `tokens`.
    pos: usize,
    diagnostics: Vec<Diagnostic>,
    /// Position of the most recently reported error, to stop message
    /// avalanches: without a token consumed in between, every further
    /// message just restates the first.
    last_error_pos: Option<usize>,
}

impl<'a> Parser<'a> {
    pub fn new(src: &'a str) -> Self {
        let all = tokenize(src);

        // Reported once here; afterwards the grammar treats them as
        // ordinary unexpected tokens.
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

    /// The collected messages, sorted by position — lexer messages arise
    /// up front, parser messages later, but the editor shows them in
    /// reading order.
    pub fn finish(mut self) -> Vec<Diagnostic> {
        self.diagnostics.sort_by_key(|d| (d.span.start, d.span.end));
        self.diagnostics
    }

    // -- Cursor ---------------------------------------------------------------

    /// The current token. Always `Eof` after the end — so never an `Option`.
    pub(crate) fn current(&self) -> Token {
        self.nth_token(0)
    }

    /// The kind of the `n`-th token from here (0 = current) — the grammar's
    /// lookahead for deciding which rule applies.
    pub(crate) fn nth(&self, n: usize) -> TokenKind {
        self.nth_token(n).kind
    }

    fn nth_token(&self, n: usize) -> Token {
        match self.tokens.get(self.pos + n) {
            Some(t) => *t,
            None => Token::new(TokenKind::Eof, Span::empty(self.src.len() as u32)),
        }
    }

    pub(crate) fn at(&self, kind: TokenKind) -> bool {
        self.current().kind == kind
    }

    /// The source text of the current token, e.g. to recognize `Table` as
    /// a block word.
    pub(crate) fn current_text(&self) -> &'a str {
        self.current().text(self.src)
    }

    /// The span of the most recently consumed token — the grammar builds a
    /// node's span as `start.cover(self.prev_span())`.
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
        // No token consumed since the last message means the same broken
        // spot is being described again.
        if self.last_error_pos == Some(self.pos) {
            return;
        }
        self.last_error_pos = Some(self.pos);
        self.diagnostics.push(Diagnostic::error(span, message));
    }

    // -- Recovery ---------------------------------------------------------------

    /// Skips the rest of a broken statement (panic-mode recovery).
    ///
    /// The language has two safe resume points: the `;` ending a statement
    /// and the `}` of the surrounding block. Bracket depth is tracked so a
    /// `}` from a skipped sub-block does not close the outer one; the `}`
    /// of the current level belongs to the caller and is left alone.
    ///
    /// # The line break as an emergency brake
    ///
    /// Meaningless to the grammar (see [`TokenKind`]), but the best
    /// recovery hint there is: the next line is probably a new statement.
    /// Without it, one missing `;` would swallow everything up to the next
    /// valid statement and a perfectly fine field would vanish from the
    /// tree. Inside brackets the rule is off — line breaks are normal there.
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

    /// Is there a line break between the last consumed token and the
    /// current one? Answerable only because the stream is lossless: what
    /// lies between two tokens is exactly the slice between their spans.
    pub(crate) fn line_break_before_current(&self) -> bool {
        let from = self.prev_span().end as usize;
        let to = self.current().span.start as usize;
        self.src
            .get(from..to)
            .is_some_and(|between| between.contains('\n'))
    }

    // -- Loop building blocks -----------------------------------------------

    /// Repeats `step` until `end` or end of file.
    ///
    /// # The progress guarantee
    ///
    /// An iteration that consumes no token loops forever, and one grammar
    /// rule that reports and returns on an unexpected token is enough to
    /// cause it. So progress is checked here and a token force-consumed if
    /// needed — a safety net, not the normal path.
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
    /// Covers `[pk, required]`, `(10, 2)` and `[highlight]`; they differ
    /// only in the element parser. A trailing separator (`[a, b,]`) is
    /// allowed — convenient when editing line by line, and free.
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
            // Keep going after a missing separator — a forgotten comma
            // shouldn't make the rest of the list unreadable.
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
