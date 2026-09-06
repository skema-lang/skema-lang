//! Positions in the source text.
//!
//! # The most important rule of the whole project
//!
//! Internally, we compute **exclusively with byte offsets in UTF-8**. Never
//! with line/column, never with character indices.
//!
//! That sounds like a detail, but it's the decision on which most
//! home-grown language servers founder:
//!
//! * Rust strings are UTF-8. `&src[start..end]` expects byte offsets.
//! * The LSP protocol speaks in **UTF-16 code units** by default —
//!   an `é` counts as 1 there, an `😀` counts as 2, whereas in UTF-8
//!   they're 2 and 4 bytes respectively.
//! * Line/column is a purely *display* form. Whoever passes it around
//!   inside the compiler has to recompute everything on every change.
//!
//! That's why: [`Span`] everywhere, and the conversion to line/column
//! happens at exactly **one** place — later, in the `skema-lsp` crate,
//! right at the protocol boundary.

use std::fmt;
use std::ops::Range;

use serde::Serialize;

/// A byte offset in the source text.
///
/// `u32` instead of `usize`, because a syntax tree consists of very many
/// nodes and each of them carries a span. `u32` halves the memory
/// footprint compared to `usize` on 64-bit systems. The 4 GiB-per-source-
/// file ceiling is not a serious limitation for a DSL — rustc and
/// rust-analyzer do it the same way.
pub type TextSize = u32;

/// A half-open byte range `[start, end)` in the source text.
///
/// "Half-open" means: `end` points past the last byte. The span `0..3` for
/// `"abc"` thus covers exactly `abc`. This is the same convention as
/// Rust's `Range`, and it has the pleasant effect that `end - start` is
/// directly the length, and empty spans (`5..5`) are naturally
/// representable — needed, e.g., for "a token is missing here".
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize)]
pub struct Span {
    pub start: TextSize,
    pub end: TextSize,
}

impl Span {
    /// # Panics
    /// Never — but `start > end` yields a nonsensical span. In a debug
    /// build, this shows up via [`Span::is_valid`] in tests.
    pub const fn new(start: TextSize, end: TextSize) -> Self {
        Self { start, end }
    }

    /// An empty span at position `at`. Useful for errors like "a `}` was
    /// expected here" — the editor then shows a cursor blink instead of an
    /// underline.
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

    /// The smallest span that covers both. This is how the parser
    /// assembles a node's span from its children's spans.
    pub fn cover(self, other: Self) -> Self {
        Self::new(self.start.min(other.start), self.end.max(other.end))
    }

    /// The source-text slice.
    ///
    /// # Panics
    /// If `src` is not the text this span originated from, or the offsets
    /// don't fall on UTF-8 character boundaries. All spans produced by the
    /// lexer satisfy this by construction.
    ///
    /// A Rust detail on the side: there's **no** explicit lifetime here,
    /// even though the return value is one. Because `self` is passed by
    /// value (`Span` is `Copy`), there's only a single reference in the
    /// signature — and *lifetime elision* automatically ties the output to
    /// `src`. In [`crate::Token::text`], on the other hand, the lifetime is
    /// spelled out: there, with `&self`, there are two references, and
    /// the elision rule would pick the wrong one.
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
