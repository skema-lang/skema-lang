//! The kinds of a `rowan` tree: every [`TokenKind`] plus the grammar's
//! node kinds, and the [`rowan::Language`] tying them together.

use crate::token::TokenKind;

macro_rules! syntax_kinds {
    (tokens: [$($token:ident),* $(,)?], nodes: [$($node:ident),* $(,)?]) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(u16)]
        pub enum SyntaxKind {
            $($token,)*
            $($node,)*
        }

        impl SyntaxKind {
            const ALL: &[Self] = &[$(Self::$token,)* $(Self::$node,)*];
        }

        impl From<TokenKind> for SyntaxKind {
            fn from(kind: TokenKind) -> Self {
                match kind {
                    $(TokenKind::$token => Self::$token,)*
                }
            }
        }
    };
}

syntax_kinds!(
    tokens: [
        Whitespace, LineComment, BlockComment,
        Ident, Int, Float, Str, Raw,
        KwSpec, KwImport, KwAs, KwUse,
        LBrace, RBrace, LParen, RParen, LBracket, RBracket,
        Comma, Semi, Dot, Eq, At, Colon, Gt, Lt, Minus,
        Unknown, UnterminatedStr, UnterminatedRaw, UnterminatedComment,
        Eof,
    ],
    nodes: [
        SourceFile,
        // Items
        Spec, Import, Use, Setting, Field, Block, Ref, IndexEntry, Error,
        // Components
        Body, Path, FieldType, Attr, Value, List, Params, Param, Flags, Args, Columns,
        SeverityOverride,
    ]
);

impl SyntaxKind {
    pub const fn is_trivia(self) -> bool {
        matches!(
            self,
            Self::Whitespace | Self::LineComment | Self::BlockComment | Self::UnterminatedComment
        )
    }

    pub const fn is_item(self) -> bool {
        matches!(
            self,
            Self::Spec
                | Self::Import
                | Self::Use
                | Self::Setting
                | Self::Field
                | Self::Block
                | Self::Ref
                | Self::IndexEntry
                | Self::Error
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SkemaLanguage {}

impl rowan::Language for SkemaLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        SyntaxKind::ALL
            .get(usize::from(raw.0))
            .copied()
            .unwrap_or(SyntaxKind::Error)
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

pub type SyntaxNode = rowan::SyntaxNode<SkemaLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<SkemaLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<SkemaLanguage>;

#[cfg(test)]
mod tests {
    use super::*;
    use rowan::Language;

    #[test]
    fn raw_kinds_round_trip() {
        for &kind in SyntaxKind::ALL {
            assert_eq!(
                SkemaLanguage::kind_from_raw(SkemaLanguage::kind_to_raw(kind)),
                kind
            );
        }
    }
}
