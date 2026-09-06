//! The lexer (also: scanner or tokenizer).
//!
//! # What a lexer actually does
//!
//! It turns a flat character sequence into a flat token sequence:
//!
//! ```text
//! "Table Customer {"   ──►   [Ident 0..5]  [Whitespace 5..6]
//!                            [Ident 6..14] [Whitespace 14..15]
//!                            [LBrace 15..16] [Eof 16..16]
//! ```
//!
//! Nothing more. The lexer knows **no structure**. It doesn't know that a
//! name must follow `Table`, and it doesn't care whether a bracket is ever
//! closed. This division of labor is the whole trick: the parser never has
//! to deal with individual characters, escape sequences, or comment
//! nesting — only with a handful of token kinds.
//!
//! # The blueprint — applies to practically every hand-written lexer
//!
//! 1. Keep a **position** (here: a byte offset `pos`) in the source text.
//! 2. Remember `start = pos`.
//! 3. Read **one** character and decide via `match` what kind of token it
//!    is. In almost all cases this one character is enough (plus,
//!    occasionally, a look at the next one — "lookahead 1").
//! 4. Devour any further characters that still belong to it.
//! 5. Return `Token { kind, span: start..pos }` and start over.
//!
//! That's all. No state machine to write out by hand, no regular grammar,
//! no generators. That's exactly how rustc, TypeScript, Go, and
//! rust-analyzer do it too — for two reasons: better error messages and
//! full control over the spans.
//!
//! # The two rules that matter in an LSP context
//!
//! * **The lexer never panics and never returns a `Result`.** Errors become
//!   tokens ([`TokenKind::Unknown`] etc.).
//! * **The token stream is lossless.** Every byte of the source text lives
//!   in exactly one token. See the `tokens_are_lossless` test.

use crate::span::{Span, TextSize};
use crate::token::{Token, TokenKind, keyword_from_str};

/// Splits the complete source text into tokens.
///
/// The last element is always [`TokenKind::Eof`].
pub fn tokenize(src: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(src);
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token();
        let done = token.kind == TokenKind::Eof;
        tokens.push(token);
        if done {
            return tokens;
        }
    }
}

/// Runs once through the source text from left to right.
///
/// The `'a` is a **lifetime**: it tells the compiler that the `Lexer` may
/// not live longer than the text it points to. You don't need to do
/// anything for that beyond writing it down — the compiler checks the
/// rest. Nothing gets copied; `&'a str` is just a pointer + length.
#[derive(Debug, Clone)]
pub struct Lexer<'a> {
    src: &'a str,
    /// Byte offset of the next not-yet-read position.
    ///
    /// **Invariant:** `pos` always sits on a UTF-8 character boundary. This
    /// is guaranteed because we only ever advance in `len_utf8()` steps.
    /// That's why `bump`/`peek` are allowed to slice without checking.
    pos: usize,
}

impl<'a> Lexer<'a> {
    /// # Panics
    /// For source texts over 4 GiB — see [`TextSize`].
    #[allow(clippy::panic)]
    pub fn new(src: &'a str) -> Self {
        if src.len() > TextSize::MAX as usize {
            panic!("source files over 4 GiB are not supported");
        }
        Self { src, pos: 0 }
    }

    /// Reads the next token. After the end of the file, the method returns
    /// [`TokenKind::Eof`] any number of times.
    pub fn next_token(&mut self) -> Token {
        let start = self.pos;

        // `let … else` – the compact way to write "if there's nothing left,
        // get out of here". The else branch must diverge (return/panic).
        let Some(c) = self.bump() else {
            return Token::new(TokenKind::Eof, Span::empty(self.offset()));
        };

        let kind = match c {
            // --- Comments -----------------------------------------------
            // `#` is unambiguous, `/` needs lookahead: on its own it's not
            // a valid character of the language, but `//` and `/*` are.
            '#' => self.line_comment(),
            '/' => match self.peek() {
                Some('/') => {
                    self.bump();
                    self.line_comment()
                }
                Some('*') => self.block_comment(),
                _ => TokenKind::Unknown,
            },

            // --- Literals with delimiters ---------------------------------
            '"' => self.string(),
            '`' => self.raw(),

            // --- Punctuation ------------------------------------------------
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ',' => TokenKind::Comma,
            ';' => TokenKind::Semi,
            '.' => TokenKind::Dot,
            '=' => TokenKind::Eq,
            '@' => TokenKind::At,
            ':' => TokenKind::Colon,
            '>' => TokenKind::Gt,
            '-' => TokenKind::Minus,
            '<' => TokenKind::Lt,

            // --- Multi-character tokens --------------------------------------
            // These branches are deliberately at the end: `match` checks
            // from top to bottom, and a guard (`c if …`) is more expensive
            // than a literal.
            c if is_ident_start(c) => {
                self.eat_while(is_ident_continue);
                // Read the whole word first, THEN look up whether it's a
                // keyword. The other way around ("starts with `use`, so
                // maybe `use`?") would wrongly split `user_id` into a
                // keyword plus a remainder. This mistake is called a
                // *maximal munch violation* and is a classic.
                let word = &self.src[start..self.pos];
                keyword_from_str(word).unwrap_or(TokenKind::Ident)
            }
            c if c.is_ascii_digit() => self.number(),
            c if c.is_whitespace() => {
                self.eat_while(char::is_whitespace);
                TokenKind::Whitespace
            }

            // Everything else: unknown. One character per error token — so
            // the error highlighting in the editor stays tight.
            _ => TokenKind::Unknown,
        };

        Token::new(kind, Span::new(start as TextSize, self.offset()))
    }

    // -- The individual token kinds -----------------------------------------

    /// Rest of the line. The caller has already consumed `#` or `//`.
    ///
    /// The line break itself doesn't belong to it — it's ordinary
    /// whitespace and gets picked up by the next `next_token()`.
    fn line_comment(&mut self) -> TokenKind {
        self.eat_while(|c| c != '\n' && c != '\r');
        TokenKind::LineComment
    }

    /// `/* … */`, nestable — `/` is already consumed.
    ///
    /// Nesting is intentional (see `docs/01`, 1.2): without it, commenting
    /// out a region that already contains a comment would silently end in
    /// the wrong place. Rust does it this way, C doesn't.
    fn block_comment(&mut self) -> TokenKind {
        self.bump(); // '*'
        let mut depth = 1_u32;
        while let Some(c) = self.bump() {
            match c {
                '/' if self.peek() == Some('*') => {
                    self.bump();
                    depth += 1;
                }
                '*' if self.peek() == Some('/') => {
                    self.bump();
                    depth -= 1;
                    if depth == 0 {
                        return TokenKind::BlockComment;
                    }
                }
                _ => {}
            }
        }
        // File ended, but `*/` is missing. No panic — an error token.
        TokenKind::UnterminatedComment
    }

    /// `"…"` — the opening `"` is already consumed.
    ///
    /// Strings may not span multiple lines. This restriction is
    /// intentional: if someone forgets a `"`, the damage ends at the line
    /// instead of swallowing the rest of the file.
    fn string(&mut self) -> TokenKind {
        self.delimited('"', true, TokenKind::Str, TokenKind::UnterminatedStr)
    }

    /// `` `…` `` — the opening backtick is already consumed.
    ///
    /// The content is opaque to skema: not parsed, not validated, only
    /// passed through. That's also why there are no escape sequences here
    /// — a backtick ends the expression, period.
    fn raw(&mut self) -> TokenKind {
        self.delimited('`', false, TokenKind::Raw, TokenKind::UnterminatedRaw)
    }

    /// Shared body of [`Self::string`] and [`Self::raw`].
    ///
    /// `escapes` switches handling of `\` on or off. Splitting this into a
    /// helper method doesn't just save lines: it ensures both literal
    /// kinds share the same end-of-line rule, even if later someone only
    /// touches one of them.
    fn delimited(
        &mut self,
        close: char,
        escapes: bool,
        ok: TokenKind,
        unterminated: TokenKind,
    ) -> TokenKind {
        while let Some(c) = self.peek() {
            match c {
                _ if c == close => {
                    self.bump();
                    return ok;
                }
                '\n' | '\r' => break,
                '\\' if escapes => {
                    self.bump(); // the backslash
                    // A `\` right before the line end doesn't escape anything.
                    if !matches!(self.peek(), Some('\n' | '\r') | None) {
                        self.bump(); // the escaped character
                    }
                }
                _ => {
                    self.bump();
                }
            }
        }
        unterminated
    }

    /// `42` or `3.14` — the first digit is already consumed.
    fn number(&mut self) -> TokenKind {
        self.eat_while(is_digit_continue);

        // A `.` only belongs to the number if a digit follows. Otherwise
        // `shop.OrderStatus` after a number would break. For this we need
        // two characters of lookahead, exceptionally.
        if self.peek() == Some('.') && self.peek_nth(1).is_some_and(|c| c.is_ascii_digit()) {
            self.bump(); // '.'
            self.eat_while(is_digit_continue);
            return TokenKind::Float;
        }
        TokenKind::Int
    }

    // -- Cursor primitives ----------------------------------------------------
    //
    // These four methods are the complete toolbox. Once you understand
    // them, you can read any lexer.

    /// The next character, without consuming it.
    ///
    /// `self.src[self.pos..]` is O(1) — a `str` slice is just pointer +
    /// length. `.chars().next()` decodes exactly one UTF-8 character.
    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    /// The `n`-th character from the current position (0 = next).
    ///
    /// O(n) in the character distance — so only use it for small `n`.
    fn peek_nth(&self, n: usize) -> Option<char> {
        self.src[self.pos..].chars().nth(n)
    }

    /// Consumes a character and returns it.
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        // `len_utf8()` instead of `+= 1`: an `é` is 2 bytes, an `😀` is 4.
        // Only this way does `pos` stay on a character boundary.
        self.pos += c.len_utf8();
        Some(c)
    }

    /// Consumes characters as long as `pred` holds.
    fn eat_while(&mut self, mut pred: impl FnMut(char) -> bool) {
        while let Some(c) = self.peek() {
            if !pred(c) {
                break;
            }
            self.pos += c.len_utf8();
        }
    }

    fn offset(&self) -> TextSize {
        self.pos as TextSize
    }
}

/// What an identifier is allowed to start with.
///
/// `is_alphabetic()` allows Unicode — `café` and `naïve` are therefore
/// valid names. For a DSL that's also written by non-programmers, that's
/// the friendlier choice. Anyone who wants strict adherence to the
/// Unicode standard (UAX #31) can later switch to the `unicode-ident`
/// crate.
fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

/// What an identifier is allowed to continue with.
fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// `_` as a thousands separator: `1_000_000`.
fn is_digit_continue(c: char) -> bool {
    c.is_ascii_digit() || c == '_'
}

// ===========================================================================
// Tests
// ===========================================================================
//
// In Rust, unit tests conventionally live in the same file as the code.
// `#[cfg(test)]` ensures the module is only compiled while testing — it
// doesn't end up in the library. `use super::*` pulls in everything from
// the surrounding module.

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the token kinds, without trivia — the way the parser sees the
    /// world.
    fn kinds(src: &str) -> Vec<TokenKind> {
        tokenize(src)
            .into_iter()
            .map(|t| t.kind)
            .filter(|k| !k.is_trivia() && *k != TokenKind::Eof)
            .collect()
    }

    /// Token kind + text, for tests where the boundaries matter.
    fn pairs(src: &str) -> Vec<(TokenKind, &str)> {
        tokenize(src)
            .into_iter()
            .filter(|t| t.kind != TokenKind::Eof)
            .map(|t| (t.kind, t.text(src)))
            .collect()
    }

    // -- The most important invariants ---------------------------------------

    /// **The most valuable test in this file.**
    ///
    /// It checks two things at once: that the spans connect to each other
    /// without gaps, and that the original text comes out exactly at the
    /// end. Almost every lexer bug — a forgotten `bump()`, a wrongly set
    /// `start` — violates one of the two conditions and shows up here.
    #[test]
    fn tokens_are_lossless() {
        let sources = [
            "",
            "\n",
            "Table Customer { id int [pk]; }",
            "# comment\nTable A {}\n",
            "// also a comment\n",
            "/* /* nested */ */ x",
            "\"unterminated",
            "`unterminated",
            "§§§ garbage",
            "café é 😀 \"emoji: 😀\"",
            "buyer >Customer.id;\r\n",
            "3.14 42 1_000 x.1",
            "created_at timestamp [default=`now()`];",
        ];

        for src in sources {
            let tokens = tokenize(src);

            let mut expected = 0;
            for t in &tokens {
                assert!(t.span.is_valid(), "invalid span {:?} in {src:?}", t.span);
                assert_eq!(
                    t.span.start as usize, expected,
                    "gap or overlap before {:?} in {src:?}",
                    t.kind
                );
                expected = t.span.end as usize;
            }
            assert_eq!(expected, src.len(), "text not fully lexed: {src:?}");

            let reconstructed: String = tokens.iter().map(|t| t.text(src)).collect();
            assert_eq!(reconstructed, src, "reconstruction differs");
        }
    }

    #[test]
    fn empty_input_yields_only_eof() {
        let tokens = tokenize("");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Eof);
        assert_eq!(tokens[0].span, Span::new(0, 0));
    }

    #[test]
    fn eof_always_comes_last() {
        let tokens = tokenize("Table A");
        assert_eq!(tokens.last().map(|t| t.kind), Some(TokenKind::Eof));
        assert_eq!(tokens.last().map(|t| t.span), Some(Span::new(7, 7)));
    }

    // -- Individual token kinds ----------------------------------------------

    #[test]
    fn only_directives_are_reserved() {
        use TokenKind::*;
        assert_eq!(kinds("spec import as use"), [KwSpec, KwImport, KwAs, KwUse]);
        // PascalCase block names remain identifiers — that's why
        // `Table Group { … }` is writable.
        assert_eq!(
            kinds("Table Group Meta Index"),
            [Ident, Ident, Ident, Ident]
        );
        // Starts with "use", but isn't one — the keyword here is maximal munch.
        assert_eq!(kinds("user_id usage"), [Ident, Ident]);
    }

    #[test]
    fn identifiers_may_contain_unicode() {
        assert_eq!(
            pairs("café naïve_2"),
            [
                (TokenKind::Ident, "café"),
                (TokenKind::Whitespace, " "),
                (TokenKind::Ident, "naïve_2"),
            ]
        );
    }

    #[test]
    fn line_breaks_are_ordinary_whitespace() {
        // A single whitespace token, no matter how many line breaks it contains.
        assert_eq!(
            pairs("a \t\n\r\n b"),
            [
                (TokenKind::Ident, "a"),
                (TokenKind::Whitespace, " \t\n\r\n "),
                (TokenKind::Ident, "b"),
            ]
        );
    }

    #[test]
    fn relation_operators() {
        use TokenKind::*;
        assert_eq!(kinds("> < -"), [Gt, Lt, Minus]);
        assert_eq!(
            kinds("buyer >Customer.id"),
            [Ident, Gt, Ident, Dot, Ident],
            "`>` sticks to its target and is still its own token"
        );
        // No `<>` operator: the language has no many-to-many sign
        // (`docs/07`). `<` followed by `>` are two separate tokens.
        assert_eq!(kinds("< >"), [Lt, Gt]);
    }

    #[test]
    fn numbers() {
        use TokenKind::*;
        assert_eq!(pairs("42"), [(Int, "42")]);
        assert_eq!(pairs("3.14"), [(Float, "3.14")]);
        assert_eq!(pairs("1_000"), [(Int, "1_000")]);
        // The dot does NOT belong to the number if no digit follows:
        assert_eq!(kinds("1.foo"), [Int, Dot, Ident]);
        assert_eq!(
            kinds("decimal(10, 2)"),
            [Ident, LParen, Int, Comma, Int, RParen]
        );
    }

    #[test]
    fn comments() {
        use TokenKind::*;
        assert_eq!(
            pairs("# hallo\n"),
            [(LineComment, "# hallo"), (Whitespace, "\n")]
        );
        assert_eq!(
            pairs("// hallo\n"),
            [(LineComment, "// hallo"), (Whitespace, "\n")]
        );
        assert_eq!(pairs("/* a */"), [(BlockComment, "/* a */")]);
        assert_eq!(
            pairs("/* /* tief */ */"),
            [(BlockComment, "/* /* tief */ */")]
        );
        // Comment at the end of the file, without a line break:
        assert_eq!(pairs("# ende"), [(LineComment, "# ende")]);
    }

    #[test]
    fn hash_in_string_is_not_a_comment() {
        // The normal case for this language: color values.
        // `r##"…"##` instead of `r#"…"#`, because the content itself
        // contains `"#`.
        assert_eq!(
            pairs(r##""#cc0000""##),
            [(TokenKind::Str, r##""#cc0000""##)],
            "`#` inside a string must not open a comment"
        );
    }

    #[test]
    fn strings_and_raw_expressions() {
        use TokenKind::*;
        assert_eq!(pairs(r#""./common.skema""#), [(Str, r#""./common.skema""#)]);
        assert_eq!(pairs(r#""mit \" drin""#), [(Str, r#""mit \" drin""#)]);
        assert_eq!(pairs(r#""""#), [(Str, r#""""#)]);
        assert_eq!(pairs("`now()`"), [(Raw, "`now()`")]);
        // In a raw expression, `\` doesn't escape anything — the backtick
        // ends it.
        assert_eq!(pairs(r"`a\`"), [(Raw, r"`a\`")]);
    }

    // -- Error cases: the lexer ALWAYS returns something usable ----------

    #[test]
    fn unknown_characters_become_error_tokens() {
        use TokenKind::*;
        assert_eq!(
            pairs("a § b"),
            [
                (Ident, "a"),
                (Whitespace, " "),
                (Unknown, "§"),
                (Whitespace, " "),
                (Ident, "b"),
            ]
        );
        // One token per character — tight highlighting in the editor.
        assert_eq!(kinds("§§"), [Unknown, Unknown]);
        // A lone `/` is not a valid character.
        assert_eq!(kinds("a / b"), [Ident, Unknown, Ident]);
    }

    #[test]
    fn colon_is_lexed_even_though_the_language_does_not_use_it() {
        // Intentional: this way the parser can report "write `=` instead of `:`".
        assert_eq!(
            kinds("author: \"anna\""),
            [TokenKind::Ident, TokenKind::Colon, TokenKind::Str]
        );
    }

    #[test]
    fn unterminated_literals_end_at_the_line() {
        use TokenKind::*;
        assert_eq!(
            pairs("\"ups\nTable A"),
            [
                (UnterminatedStr, "\"ups"),
                (Whitespace, "\n"),
                // Important: the next line is lexed completely normally.
                (Ident, "Table"),
                (Whitespace, " "),
                (Ident, "A"),
            ]
        );
        assert_eq!(pairs("\"ups"), [(UnterminatedStr, "\"ups")]);
        assert_eq!(pairs("`ups"), [(UnterminatedRaw, "`ups")]);
    }

    #[test]
    fn unterminated_block_comment() {
        assert_eq!(
            pairs("/* ups"),
            [(TokenKind::UnterminatedComment, "/* ups")]
        );
        assert_eq!(
            pairs("/* /* nur einmal zu */"),
            [(TokenKind::UnterminatedComment, "/* /* nur einmal zu */")]
        );
    }

    /// No matter what comes in — the lexer terminates and doesn't panic.
    #[test]
    fn robust_against_arbitrary_input() {
        let garbage = [
            "\0\0\0",
            "\\\\\\",
            "@@@@",
            "\"\\",
            "/*",
            "/",
            "`",
            "😀😀😀",
            "<><><",
            "..........",
        ];
        for src in garbage {
            let tokens = tokenize(src);
            assert!(tokens.last().is_some_and(|t| t.kind == TokenKind::Eof));
        }
    }
}
