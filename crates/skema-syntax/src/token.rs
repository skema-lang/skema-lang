//! The `.skema` language's token kinds, and the token type itself.

use crate::Span;

/// A single token: *what* it is and *where* it is.
///
/// Notably absent here: the text. A token is only 12 bytes. You fetch the
/// text on demand via [`Token::text`] from the source — a `String` per
/// token would mean thousands of heap allocations per file, and in the
/// language server, re-lexing happens after every keystroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub const fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// This token's source text. `src` must be the same text the token
    /// was lexed from.
    pub fn text<'a>(&self, src: &'a str) -> &'a str {
        self.span.text(src)
    }
}

/// All token kinds of the language.
///
/// # Why trivia gets its own variants
///
/// Many lexers throw away whitespace and comments. We don't:
///
/// * A **formatter** (`skema fmt`) must preserve comments.
/// * The token stream stays **lossless**: concatenating the text of all
///   tokens yields exactly the original source text. That's exactly what
///   `tokens_are_lossless` checks. This property saves you on every
///   refactor, because almost every lexer bug violates it.
///
/// The parser skips trivia itself — but it's still there if you need it.
///
/// # Why there's no `Newline` token
///
/// In `.skema`, `;` ends a statement and `}` ends a block. The line break
/// therefore carries **no** meaning and is ordinary whitespace. That's a
/// noticeable simplification: the parser needs no rules of the form "a
/// line break is allowed here, not there", and an expression may be
/// spread across lines however you like.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    // ---- Trivia: invisible to the parser, but preserved --------------------
    /// Spaces, tabs, line breaks.
    Whitespace,
    /// `# …` or `// …` — both forms are equivalent per the language spec.
    LineComment,
    /// `/* … */`, nestable.
    BlockComment,

    // ---- Literals and names ------------------------------------------------
    /// `Customer`, `created_at`, `draft`
    Ident,
    /// `42`
    Int,
    /// `3.14`
    Float,
    /// `"#cc0000"`
    Str,
    /// `` `now()` `` — opaque to skema, only passed through.
    Raw,

    // ---- Reserved keywords ---------------------------------------------
    //
    // Only the four lowercase directives are reserved here. The PascalCase
    // block names (`Table`, `Meta`, …) are deliberately NOT keywords — see
    // `keyword_from_str`.
    /// `spec`
    KwSpec,
    /// `import`
    KwImport,
    /// `as`
    KwAs,
    /// `use`
    KwUse,

    // ---- Punctuation ---------------------------------------------------
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `,`
    Comma,
    /// `;`
    Semi,
    /// `.`
    Dot,
    /// `=`
    Eq,
    /// `@` — introduces a style selector: `Style @table`.
    At,
    /// `:` — **not** a character of the language. Lexed anyway, so the
    /// parser can report "write `=` instead of `:`".
    Colon,

    // ---- Relationship operators ---------------------------------------------
    //
    // Only three — the language deliberately has no `<>` for many-to-many
    // (`docs/07`, "Many-to-many has no arrow"): SQL needs a join table for
    // that anyway, so it gets written into the model instead of being
    // hidden behind an operator.
    /// `>` — one-to-many
    Gt,
    /// `<` — many-to-one (back-reference, creates no column)
    Lt,
    /// `-` — one-to-one
    Minus,

    // ---- Errors --------------------------------------------------------
    //
    // The lexer never returns a `Result` and never panics. Errors become
    // tokens. A language server almost always receives unfinished text —
    // if the lexer bails out there, syntax highlighting stops while
    // you're typing.
    /// A character that doesn't occur in the language, e.g. `§`.
    Unknown,
    /// A string literal without a closing `"`.
    UnterminatedStr,
    /// A raw expression without a closing `` ` ``.
    UnterminatedRaw,
    /// A block comment without `*/`.
    UnterminatedComment,

    /// End of the file. Always the last token, with an empty span.
    ///
    /// An explicit EOF token is an old compiler trick: the parser never
    /// has to deal with `Option<Token>` anywhere and can always look at
    /// "the current token". That saves a surprising number of case
    /// distinctions.
    Eof,
}

impl TokenKind {
    /// Does the parser skip this token?
    pub const fn is_trivia(self) -> bool {
        matches!(
            self,
            Self::Whitespace | Self::LineComment | Self::BlockComment | Self::UnterminatedComment
        )
    }

    pub const fn is_keyword(self) -> bool {
        matches!(
            self,
            Self::KwSpec | Self::KwImport | Self::KwAs | Self::KwUse
        )
    }

    /// Does this token indicate a lexer error?
    pub const fn is_error(self) -> bool {
        matches!(
            self,
            Self::Unknown
                | Self::UnterminatedStr
                | Self::UnterminatedRaw
                | Self::UnterminatedComment
        )
    }

    /// Is this a relationship operator (`>`, `<`, `-`)?
    pub const fn is_relation_op(self) -> bool {
        matches!(self, Self::Gt | Self::Lt | Self::Minus)
    }

    /// A human-readable name for error messages:
    /// "expected `}`, found identifier".
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Whitespace => "whitespace",
            Self::LineComment | Self::BlockComment => "comment",
            Self::Ident => "identifier",
            Self::Int => "integer",
            Self::Float => "decimal number",
            Self::Str => "string",
            Self::Raw => "raw expression",
            Self::KwSpec => "`spec`",
            Self::KwImport => "`import`",
            Self::KwAs => "`as`",
            Self::KwUse => "`use`",
            Self::LBrace => "`{`",
            Self::RBrace => "`}`",
            Self::LParen => "`(`",
            Self::RParen => "`)`",
            Self::LBracket => "`[`",
            Self::RBracket => "`]`",
            Self::Comma => "`,`",
            Self::Semi => "`;`",
            Self::Dot => "`.`",
            Self::Eq => "`=`",
            Self::At => "`@`",
            Self::Colon => "`:`",
            Self::Gt => "`>`",
            Self::Lt => "`<`",
            Self::Minus => "`-`",
            Self::Unknown => "unknown character",
            Self::UnterminatedStr => "unterminated string",
            Self::UnterminatedRaw => "unterminated raw expression",
            Self::UnterminatedComment => "unterminated comment",
            Self::Eof => "end of file",
        }
    }
}

/// Is `text` a reserved directive?
///
/// # Why only four words are here
///
/// The language has two classes of keywords (see `docs/01`, section 1.6):
///
/// | Class | Spelling | Examples |
/// |---|---|---|
/// | Directive | `lowercase` | `spec`, `import`, `as`, `use` |
/// | Block | `PascalCase` | `Table`, `Meta`, `Group`, `Index`, `File` … |
///
/// Only the directives are **reserved**. The block names remain ordinary
/// [`TokenKind::Ident`] and are only recognized as keywords by the parser
/// based on their *position* — this is called **contextual keywords**.
///
/// The reason is practical, not aesthetic: in an ERP model, `Group`,
/// `File`, `Index`, and `Document` are perfectly ordinary table names. If
/// they were reserved, nobody could write `Table Group { … }`. The price
/// is a small ambiguity that the parser resolves — see `grammar.rs`,
/// `parse_item`.
pub const fn keyword_from_str(text: &str) -> Option<TokenKind> {
    // `match` on `&str` isn't allowed in a const fn, but on bytes it is.
    match text.as_bytes() {
        b"spec" => Some(TokenKind::KwSpec),
        b"import" => Some(TokenKind::KwImport),
        b"as" => Some(TokenKind::KwAs),
        b"use" => Some(TokenKind::KwUse),
        _ => None,
    }
}
