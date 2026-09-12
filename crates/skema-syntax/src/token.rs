//! The `.skema` language's token kinds, and the token type itself.

use crate::Span;

/// A single token: *what* it is and *where* it is.
///
/// It carries no text — that comes from the source via [`Token::text`]. A
/// `String` per token would be thousands of allocations per file, and the
/// language server re-lexes on every keystroke.
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
/// Whitespace and comments are kept, not discarded: a future formatter
/// must preserve comments, and it keeps the stream **lossless** —
/// concatenating all token texts reproduces the source exactly, which is
/// what `tokens_are_lossless` asserts. Almost every lexer bug breaks that
/// property. The parser skips trivia itself.
///
/// # Why there is no `Newline` token
///
/// `;` ends a statement and `}` ends a block, so a line break carries no
/// meaning and is ordinary whitespace. No "a line break is allowed here,
/// not there" rules, and a statement may wrap freely.
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
    // Only these four lowercase directives are reserved; PascalCase block
    // names stay `Ident` — see `keyword_from_str`.
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
    // Three only; no `<>` for many-to-many (`docs/07`). SQL needs a join
    // table anyway, so it is written into the model.
    /// `>` — one-to-many
    Gt,
    /// `<` — many-to-one (back-reference, creates no column)
    Lt,
    /// `-` — one-to-one
    Minus,

    // ---- Errors --------------------------------------------------------
    //
    // Errors become tokens; the lexer never returns a `Result` and never
    // panics. Unfinished text is the normal case, and a lexer that bails
    // out stops syntax highlighting mid-keystroke.
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
    /// Explicit, so the parser never handles an `Option<Token>` and can
    /// always ask for "the current token".
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
/// # Why only four words
///
/// Of the keyword classes in `docs/01`, "Case conventions and keywords",
/// only the lowercase directives (`spec`, `import`, `as`, `use`) are
/// reserved. PascalCase block names (`Table`, `Meta`, `Group`, …) stay
/// [`TokenKind::Ident`] and are recognized by *position* — contextual
/// keywords — and camelCase keys and flags are plain identifiers.
///
/// In an ERP model `Group`, `Index` and `Document` are ordinary table
/// names, so `Table Group { … }` has to stay writable. The price is one
/// ambiguity, resolved in `grammar::parse_item`.
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
