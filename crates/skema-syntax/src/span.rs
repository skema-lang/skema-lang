//! Positions in the source text.
//!
//! # The one rule
//!
//! The core computes **exclusively with UTF-8 byte offsets** — never
//! line/column, never character indices. Rust strings are UTF-8, so
//! `&src[start..end]` needs bytes; line/column is a display form that would
//! have to be recomputed on every edit; and the LSP protocol counts UTF-16
//! code units, which agrees with neither.
//!
//! So: [`Span`] everywhere, and exactly one place converts —
//! `skema_api::location`, at the front-end boundary.

use std::fmt;
use std::ops::Range;

use serde::Serialize;

/// A byte offset in the source text.
///
/// `u32`, not `usize`: every node in the tree carries a span, so this halves
/// the footprint on 64-bit. The 4 GiB per file ceiling is no limit for a
/// DSL; rustc and rust-analyzer do the same.
pub type TextSize = u32;

/// A half-open byte range `[start, end)` in the source text.
///
/// Same convention as Rust's `Range`: `end - start` is the length, and an
/// empty span (`5..5`) is representable — needed for "a token is missing
/// here".
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize)]
pub struct Span {
    pub start: TextSize,
    pub end: TextSize,
}

impl Span {
    /// Does not panic on `start > end`, but the result is nonsensical;
    /// tests catch that via [`Span::is_valid`].
    pub const fn new(start: TextSize, end: TextSize) -> Self {
        Self { start, end }
    }

    /// An empty span at `at`, for errors like "a `}` was expected here" —
    /// the editor shows a caret instead of an underline.
    pub const fn empty(at: TextSize) -> Self {
        Self::new(at, at)
    }

    pub const fn len(self) -> TextSize {
        self.end.saturating_sub(self.start)
    }

    pub const fn is_empty(self) -> bool {
        self.start >= self.end
    }

    pub const fn is_valid(self) -> bool {
        self.start <= self.end
    }

    /// Does the span contain the offset `at`? (`end` itself doesn't belong to it.)
    pub const fn contains(self, at: TextSize) -> bool {
        self.start <= at && at < self.end
    }

    /// The smallest span covering both — how the parser builds a node's
    /// span from its children.
    pub fn cover(self, other: Self) -> Self {
        Self::new(self.start.min(other.start), self.end.max(other.end))
    }

    /// The source-text slice.
    ///
    /// # Panics
    /// If `src` is not the text this span came from, or the offsets miss a
    /// UTF-8 character boundary. Lexer-produced spans satisfy both by
    /// construction.
    pub fn text(self, src: &str) -> &str {
        &src[self.start as usize..self.end as usize]
    }
}

/// `1..5` instead of `Span { start: 1, end: 5 }` — makes test output readable.
impl fmt::Debug for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

impl From<Range<TextSize>> for Span {
    fn from(r: Range<TextSize>) -> Self {
        Self::new(r.start, r.end)
    }
}

impl From<Span> for Range<usize> {
    fn from(s: Span) -> Self {
        s.start as usize..s.end as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics() {
        let s = Span::new(2, 5);
        assert_eq!(s.len(), 3);
        assert!(!s.is_empty());
        assert!(s.contains(2));
        assert!(s.contains(4));
        assert!(!s.contains(5), "end does not belong to the span");
        assert_eq!(s.text("abcdefg"), "cde");
        assert_eq!(format!("{s:?}"), "2..5");
    }

    #[test]
    fn cover_unites_both() {
        assert_eq!(Span::new(2, 4).cover(Span::new(8, 9)), Span::new(2, 9));
        assert_eq!(Span::new(8, 9).cover(Span::new(2, 4)), Span::new(2, 9));
    }

    #[test]
    fn empty_span() {
        let s = Span::empty(3);
        assert!(s.is_empty());
        assert_eq!(s.text("abcdefg"), "");
    }
}
