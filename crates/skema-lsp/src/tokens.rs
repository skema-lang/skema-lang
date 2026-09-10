//! `textDocument/semanticTokens`: colouring from the language's own lexer.
//!
//! # Why not a TextMate grammar
//!
//! A `.tmLanguage.json` would be a second description of skema's syntax, in
//! a notation that has no parser and so cannot tell a type reference from a
//! field name — and it would drift the first time the language grows a
//! keyword. `skema_api::highlight_tokens` already merges the lexer's
//! lossless output with the AST-driven classification, and is what the
//! browser editor colours from. Serving it here means both colour
//! identically, from one implementation.
//!
//! The cost is one round trip before colouring arrives. Not perceptible for
//! a file of a few hundred lines, and this server re-parses per keystroke
//! for diagnostics anyway.

use lsp_types::{SemanticToken, SemanticTokenType, SemanticTokens};

/// Index into this list *is* the `token_type` a token carries, so the order
/// here and the mapping below must stay in step - which is why they sit next
/// to each other.
pub fn legend() -> Vec<SemanticTokenType> {
    vec![
        SemanticTokenType::COMMENT,
        SemanticTokenType::STRING,
        SemanticTokenType::NUMBER,
        SemanticTokenType::OPERATOR,
        SemanticTokenType::KEYWORD,
        SemanticTokenType::NAMESPACE,
        SemanticTokenType::CLASS,
        SemanticTokenType::PROPERTY,
        SemanticTokenType::TYPE,
        SemanticTokenType::MODIFIER,
        SemanticTokenType::VARIABLE,
    ]
}

/// The class names `skema-api` hands out, mapped to indexes into [`legend`].
///
/// Matched by name rather than by the class's numeric code: the codes are an
/// implementation detail of the flat token stream, while the names are the
/// published mapping (`highlightClassNames`). A class this server does not
/// know yet simply goes uncoloured instead of mis-coloured.
fn token_type(class: &str) -> Option<u32> {
    Some(match class {
        "comment" => 0,
        "string" | "raw" => 1,
        "number" => 2,
        "relationOp" => 3,
        "directive" | "blockKeyword" => 4,
        "namespace" => 5,
        "declarationName" => 6,
        "fieldName" | "metaKey" => 7,
        "typeRef" => 8,
        "flagName" => 9,
        "ident" => 10,
        // Punctuation is left to the editor's own tokenizer, and an `error`
        // class is already carrying a squiggle from the diagnostics.
        _ => return None,
    })
}

/// The flat `[class, utf16Start, utf16End]` triples, re-encoded as LSP's
/// delta-of-a-delta form: each token is relative to the previous one.
pub fn semantic_tokens(source: &str) -> SemanticTokens {
    let class_names = skema_api::highlight_class_names();
    let lines = Utf16Lines::new(source);

    let mut data: Vec<SemanticToken> = Vec::new();
    let mut previous = (0u32, 0u32);

    for triple in skema_api::highlight_tokens(source).chunks_exact(3) {
        let (class, start, end) = (triple[0], triple[1], triple[2]);
        let Some(token_type) = class_names
            .get(class as usize)
            .and_then(|name| token_type(name))
        else {
            continue;
        };
        let (line, character) = lines.position(start);
        // A token that spans a line break cannot be expressed in this
        // encoding at all; the language has none (strings and comments are
        // the only multi-line tokens, and both are skipped or single-line
        // in practice), so clamping to the line's end is the safe answer.
        let length = end
            .saturating_sub(start)
            .min(lines.rest_of_line(line, character));
        if length == 0 {
            continue;
        }

        let delta_line = line - previous.0;
        data.push(SemanticToken {
            delta_line,
            delta_start: if delta_line == 0 {
                character - previous.1
            } else {
                character
            },
            length,
            token_type,
            token_modifiers_bitset: 0,
        });
        previous = (line, character);
    }

    SemanticTokens {
        result_id: None,
        data,
    }
}

/// Where each line starts, counted in UTF-16 units - so a token's offset
/// becomes a (line, character) pair with a binary search instead of a walk
/// from the top of the file per token.
struct Utf16Lines {
    /// Cumulative UTF-16 length at the start of each line.
    starts: Vec<u32>,
    /// UTF-16 length of each line, without its newline.
    lengths: Vec<u32>,
}

impl Utf16Lines {
    fn new(source: &str) -> Self {
        let mut starts = Vec::new();
        let mut lengths = Vec::new();
        let mut total = 0u32;
        for line in source.split_inclusive('\n') {
            starts.push(total);
            let units = line.encode_utf16().count() as u32;
            lengths.push(units - line.ends_with('\n') as u32 - line.ends_with("\r\n") as u32);
            total += units;
        }
        if starts.is_empty() {
            starts.push(0);
            lengths.push(0);
        }
        Self { starts, lengths }
    }

    fn position(&self, offset: u32) -> (u32, u32) {
        let line = match self.starts.binary_search(&offset) {
            Ok(exact) => exact,
            Err(next) => next.saturating_sub(1),
        };
        (line as u32, offset - self.starts[line])
    }

    fn rest_of_line(&self, line: u32, character: u32) -> u32 {
        self.lengths
            .get(line as usize)
            .copied()
            .unwrap_or(0)
            .saturating_sub(character)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_encoded_relative_to_their_predecessor() {
        let tokens = semantic_tokens("Table Customer {\n  id int;\n}").data;
        assert!(!tokens.is_empty());
        // `Table` is the first token: absolute position, i.e. both deltas 0.
        assert_eq!((tokens[0].delta_line, tokens[0].delta_start), (0, 0));
        assert_eq!(tokens[0].length, 5);
        // `Customer` follows on the same line, six characters along.
        assert_eq!((tokens[1].delta_line, tokens[1].delta_start), (0, 6));
    }

    #[test]
    fn a_token_after_a_newline_restarts_its_character_count() {
        let tokens = semantic_tokens("Table T {\n  id int;\n}").data;
        let second_line = tokens
            .iter()
            .find(|t| t.delta_line > 0)
            .expect("a token on a later line");
        assert_eq!(
            second_line.delta_start, 2,
            "after a line break the column is absolute, not a delta"
        );
    }

    #[test]
    fn multibyte_text_does_not_shift_later_tokens() {
        // `ü` is two bytes but one UTF-16 unit: a byte-based count would put
        // every following token one column to the right.
        let tokens = semantic_tokens("Table T { note = \"ü\"; id int; }").data;
        let source = "Table T { note = \"ü\"; id int; }";
        let expected = source
            .find("id")
            .map(|i| source[..i].encode_utf16().count() as u32)
            .expect("id");
        let mut character = 0;
        for token in &tokens {
            character = if token.delta_line == 0 {
                character + token.delta_start
            } else {
                token.delta_start
            };
            if token.length == 2 && character == expected {
                return;
            }
        }
        panic!("no token found at the UTF-16 column of `id`");
    }

    #[test]
    fn punctuation_is_left_to_the_editor() {
        let tokens = semantic_tokens("Table T { }").data;
        // `Table`, `T` - and no entries for the braces.
        assert_eq!(tokens.len(), 2);
    }
}
