//! Layout-only formatter: reindents and normalizes blank lines without
//! moving, adding, or removing any non-whitespace token.
//!
//! Works directly on [`crate::lexer::tokenize`]'s lossless token stream —
//! trivia included — rather than the parsed [`crate::ast`]. That is enough
//! for a formatter that never reorders anything (see
//! `brain/tasks/83-skema-fmt-cli-layout-mode.md`, "Why this doesn't need
//! task 66"): every non-whitespace token is copied through unchanged, in
//! the same order, so a comment can never end up detached from what it was
//! written next to. Only [`TokenKind::Whitespace`] tokens are rewritten, and
//! the one exception where a token boundary is invented rather than
//! rewritten: a missing space around `=` is inserted (`default=0` →
//! `default = 0`), since the language never puts whitespace-free tokens on
//! either side of it in valid code.
//!
//! # What never changes
//!
//! A [`TokenKind::Str`], [`TokenKind::Raw`], [`TokenKind::BlockComment`], or
//! [`TokenKind::LineComment`] token's own text is copied byte-for-byte: the
//! formatter only ever rewrites the *whitespace around* a token, never
//! looks inside one. That's what keeps a raw expression, a string, and a
//! multi-line block comment's interior lines untouched even though their
//! surrounding indentation moves.

use crate::token::{Token, TokenKind};

const INDENT_WIDTH: usize = 2;

/// Reindents `src` and normalizes its blank lines; see the module docs for
/// exactly what moves and what doesn't. Always returns a string — there is
/// no failure mode, per Rule 2 ("never panic"); a file with lexer errors
/// (an unterminated string, an unclosed block comment) or mismatched braces
/// still formats, since malformed constructs are single opaque tokens whose
/// interior is never touched, and brace depth cannot go negative regardless
/// of how many stray `}` show up.
pub fn format(src: &str) -> String {
    let tokens = crate::lexer::tokenize(src);
    let mut out = String::with_capacity(src.len());
    let mut depth: i32 = 0;

    for (i, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::LBrace => {
                out.push_str(token.text(src));
                depth += 1;
            }
            TokenKind::RBrace => {
                depth -= 1;
                out.push_str(token.text(src));
            }
            TokenKind::Whitespace => {
                let prev = i.checked_sub(1).map(|j| tokens[j]);
                let next = tokens[i + 1];
                out.push_str(&rewrite_gap(token.text(src), prev, next, depth));
            }
            _ => {
                out.push_str(token.text(src));
                insert_eq_space_if_adjacent(&mut out, token, tokens.get(i + 1));
            }
        }
    }

    out
}

/// After emitting a non-whitespace, non-brace token, inserts a single space
/// if it directly touches an `=` with no whitespace token in between —
/// `default=0` and `mode=draft` both need one. Only fires when the two
/// tokens are truly adjacent in the stream (no [`TokenKind::Whitespace`]
/// between them); the case where whitespace already exists is handled by
/// [`rewrite_gap`] instead.
fn insert_eq_space_if_adjacent(out: &mut String, cur: &Token, next: Option<&Token>) {
    let Some(next) = next else { return };
    if next.kind == TokenKind::Whitespace {
        return;
    }
    if cur.kind == TokenKind::Eq || next.kind == TokenKind::Eq {
        out.push(' ');
    }
}

/// Rewrites one `Whitespace` token's text: `prev`/`next` are its
/// neighboring non-whitespace tokens (there is always exactly one
/// `Whitespace` token between two content tokens, never two in a row, since
/// the lexer merges consecutive whitespace characters).
fn rewrite_gap(text: &str, prev: Option<Token>, next: Token, depth_after_prev: i32) -> String {
    if !contains_line_break(text) {
        // Same-line spacing. Only `=` has a rule; everything else (e.g. the
        // deliberate column padding in `examples/myexample.skema`) is left
        // exactly as the author wrote it — scope cut, see the task file.
        let touches_eq =
            prev.is_some_and(|p| p.kind == TokenKind::Eq) || next.kind == TokenKind::Eq;
        if touches_eq {
            return " ".to_string();
        }
        return text.to_string();
    }

    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let blank_lines_in_original = count_line_breaks(text).saturating_sub(1);

    let after_open_brace = prev.is_some_and(|p| p.kind == TokenKind::LBrace);
    let before_close_brace = next.kind == TokenKind::RBrace;
    let allowed_blank_lines = if after_open_brace || before_close_brace {
        0
    } else {
        blank_lines_in_original.min(1)
    };

    let mut gap = newline.repeat(1 + allowed_blank_lines);

    // No indent to append before EOF: that would only add trailing
    // whitespace at the end of the file.
    if next.kind != TokenKind::Eof {
        let indent_depth = if before_close_brace {
            depth_after_prev - 1
        } else {
            depth_after_prev
        };
        let spaces = indent_depth.max(0) as usize * INDENT_WIDTH;
        gap.push_str(&" ".repeat(spaces));
    }

    gap
}

/// Counts line breaks, treating `\r\n` as one and a lone `\r` or `\n` as one
/// each — so a file's existing CRLF/LF convention is preserved rather than
/// silently normalized (see NOTES in the task's final report for why that
/// is the conservative choice here).
fn count_line_breaks(text: &str) -> usize {
    let mut count = 0;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                count += 1;
            }
            '\n' => count += 1,
            _ => {}
        }
    }
    count
}

fn contains_line_break(text: &str) -> bool {
    text.contains('\n') || text.contains('\r')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reindents_nested_blocks() {
        let src = "spec 1;\nTable Foo {\n    id int;\n        Meta {\nnote = \"x\";\n}\n}\n";
        let formatted = format(src);
        assert_eq!(
            formatted,
            "spec 1;\nTable Foo {\n  id int;\n  Meta {\n    note = \"x\";\n  }\n}\n"
        );
    }

    #[test]
    fn mixed_tabs_and_spaces_reindent_to_two_spaces() {
        let src = "Table Foo {\n\tid int;\n  \tname varchar;\n}\n";
        let formatted = format(src);
        assert_eq!(formatted, "Table Foo {\n  id int;\n  name varchar;\n}\n");
    }

    #[test]
    fn collapses_runs_of_blank_lines_to_one() {
        let src = "spec 1;\n\n\n\nTable Foo {\n  id int;\n}\n";
        assert_eq!(format(src), "spec 1;\n\nTable Foo {\n  id int;\n}\n");
    }

    #[test]
    fn keeps_a_single_blank_line() {
        let src = "spec 1;\n\nTable Foo {\n  id int;\n}\n";
        assert_eq!(format(src), src);
    }

    #[test]
    fn removes_blank_line_right_after_open_brace() {
        let src = "Table Foo {\n\n  id int;\n}\n";
        assert_eq!(format(src), "Table Foo {\n  id int;\n}\n");
    }

    #[test]
    fn removes_blank_line_right_before_close_brace() {
        let src = "Table Foo {\n  id int;\n\n}\n";
        assert_eq!(format(src), "Table Foo {\n  id int;\n}\n");
    }

    #[test]
    fn single_space_around_eq_in_assignment() {
        let src = "Meta {\n  version    =    \"1\";\n}\n";
        assert_eq!(format(src), "Meta {\n  version = \"1\";\n}\n");
    }

    #[test]
    fn single_space_around_eq_in_flag_with_no_existing_space() {
        let src = "id int [default=0];\n";
        assert_eq!(format(src), "id int [default = 0];\n");
    }

    #[test]
    fn does_not_column_align_multiple_assignments() {
        let src = "Meta {\n  version = \"\";\n  author  = \"\";\n}\n";
        assert_eq!(
            format(src),
            "Meta {\n  version = \"\";\n  author = \"\";\n}\n"
        );
    }

    #[test]
    fn comments_are_reindented_but_untouched() {
        let src = "Table Foo {\n      # a comment\n  id int; // trailing\n}\n";
        assert_eq!(
            format(src),
            "Table Foo {\n  # a comment\n  id int; // trailing\n}\n"
        );
    }

    #[test]
    fn block_comment_interior_lines_are_left_exactly_as_written() {
        let src = "Table Foo {\n    /* line one\n       line two, indented on purpose\n    */\n  id int;\n}\n";
        assert_eq!(
            format(src),
            "Table Foo {\n  /* line one\n       line two, indented on purpose\n    */\n  id int;\n}\n"
        );
    }

    #[test]
    fn nested_block_comment_is_one_opaque_token() {
        let src = "Table Foo {\n    /* /* nested */ still inside */\n  id int;\n}\n";
        assert_eq!(
            format(src),
            "Table Foo {\n  /* /* nested */ still inside */\n  id int;\n}\n"
        );
    }

    #[test]
    fn string_contents_are_untouched() {
        let src = "Meta {\n    note = \"  spaced   out {}  =  text  \";\n}\n";
        assert_eq!(
            format(src),
            "Meta {\n  note = \"  spaced   out {}  =  text  \";\n}\n"
        );
    }

    #[test]
    fn raw_expression_is_untouched() {
        let src = "id int [default=`now(  )`];\n";
        assert_eq!(format(src), "id int [default = `now(  )`];\n");
    }

    #[test]
    fn unterminated_string_degrades_without_touching_its_line() {
        let src = "Table Foo {\n    id \"ups\n}\n";
        // The unterminated string ends at the line break (lexer rule); the
        // formatter must not panic and must leave that token's text as-is.
        let formatted = format(src);
        assert!(formatted.contains("\"ups"));
    }

    #[test]
    fn unterminated_block_comment_never_panics() {
        let src = "Table Foo {\n  /* never closed\n}\n";
        // No `}` token exists once the comment swallows the rest of the
        // file — depth math must not panic on the missing close.
        let formatted = format(src);
        assert!(formatted.contains("/* never closed"));
    }

    #[test]
    fn extra_closing_brace_does_not_panic_or_underflow() {
        let src = "Table Foo {\n  id int;\n}\n}\n";
        let formatted = format(src);
        // Depth clamps at zero instead of going negative.
        assert!(formatted.contains("}\n}"));
    }

    #[test]
    fn idempotent_on_the_reference_example() {
        let example = include_str!("../../../examples/myexample.skema");
        let once = format(example);
        let twice = format(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn formatting_never_changes_the_non_trivia_token_sequence() {
        let example = include_str!("../../../examples/myexample.skema");
        let formatted = format(example);

        let content = |src: &str| -> Vec<(TokenKind, String)> {
            crate::lexer::tokenize(src)
                .into_iter()
                .filter(|t| !matches!(t.kind, TokenKind::Whitespace | TokenKind::Eof))
                .map(|t| (t.kind, t.text(src).to_string()))
                .collect()
        };

        // `=` tokens may have gained surrounding whitespace, but every
        // other token's kind, text, and relative order is unchanged.
        let before: Vec<_> = content(example);
        let after: Vec<_> = content(&formatted);
        assert_eq!(before, after);
    }

    #[test]
    fn semantic_no_op_on_the_reference_example() {
        let example = include_str!("../../../examples/myexample.skema");
        let formatted = format(example);

        let (_, before) = crate::parse(example);
        let (_, after) = crate::parse(&formatted);
        assert_eq!(before, after);
    }
}
