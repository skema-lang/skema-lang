//! What is being typed, and where.
//!
//! Completion needs to know whether the caret sits in a `[]` flag list, a
//! type position, after a relation arrow, or in a `Meta` body. The scan uses
//! the language's own token stream ([`crate::highlight`]), not regexes, so a
//! `{` inside a string or comment cannot throw the nesting off.
//!
//! A heuristic by design: it decides what to *offer*, never what is
//! *allowed*. Placement rules stay with `skema-hir`, which reports them as
//! diagnostics a moment later.
//!
//! It does not pick the words. That needs the catalogs
//! ([`crate::builtins`]) and the analyzed project, and presentation differs
//! per front end — so this returns the question and each client answers it.

use crate::highlight::{Class, classified};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextKind {
    /// Inside `[ … ]`: flag names.
    Flags,
    /// Inside `[ key=… ]`: the value of the flag named in
    /// [`CursorContext::qualifier`] - `cascade` for `onDelete`.
    FlagValue,
    /// A `key = value;` body - `Meta`, `Project`, `Lint`, `Database`, `Field`.
    SettingKey,
    /// After a field's name: its type comes next.
    TypePosition,
    /// Directly after `>`, `<` or `-`: the table a relation points at.
    RelationTarget,
    /// After a `.`: the members of [`CursorContext::qualifier`].
    MemberOf,
    /// The start of a fresh statement or declaration.
    StatementStart,
}

#[derive(Debug, Clone)]
pub struct CursorContext {
    pub kind: ContextKind,
    /// The block the caret is inside, e.g. `Table`, `Meta`, `Project`.
    /// `None` at the top level of a file.
    pub block_keyword: Option<String>,
    /// For [`ContextKind::MemberOf`]: the table or namespace whose members
    /// are being asked for. For [`ContextKind::FlagValue`]: the flag whose
    /// value is being typed.
    pub qualifier: Option<String>,
}

/// Classes that count as "a word is being typed here" - so a half-written
/// identifier is treated as the caret's own token rather than as a name that
/// has already been settled.
fn is_word(class: Class) -> bool {
    matches!(
        class,
        Class::Ident
            | Class::BlockKeyword
            | Class::Namespace
            | Class::DeclarationName
            | Class::FieldName
            | Class::TypeRef
            | Class::FlagName
            | Class::MetaKey
    )
}

/// Blocks whose bodies are `key = value;` pairs rather than declarations.
fn is_settings_block(keyword: Option<&str>) -> bool {
    matches!(
        keyword,
        Some("Meta" | "Database" | "Project" | "Lint" | "Field" | "Style" | "Dialect")
    )
}

/// `offset` is a **byte** offset into `source`, like every span in
/// `skema-syntax`. A caller holding an editor position converts once, at its
/// own boundary.
pub fn cursor_context(source: &str, offset: u32) -> CursorContext {
    let mut blocks: Vec<String> = Vec::new();
    let mut pending_keyword: Option<String> = None;
    let mut bracket_depth: u32 = 0;
    // Tokens since the last `;`, `{` or `}` - i.e. the statement being typed.
    let mut statement: Vec<(Class, &str, u32)> = Vec::new();

    for (class, span) in classified(source) {
        if span.start >= offset {
            break;
        }
        if class == Class::Comment {
            continue;
        }
        let text = source
            .get(span.start as usize..span.end as usize)
            .unwrap_or("");

        if class == Class::BlockKeyword {
            pending_keyword = Some(text.to_owned());
        }

        if class == Class::Punctuation {
            match text {
                "{" => {
                    blocks.push(pending_keyword.take().unwrap_or_default());
                    statement.clear();
                    continue;
                }
                "}" => {
                    blocks.pop();
                    statement.clear();
                    continue;
                }
                ";" => {
                    pending_keyword = None;
                    statement.clear();
                    continue;
                }
                "[" => bracket_depth += 1,
                "]" => bracket_depth = bracket_depth.saturating_sub(1),
                _ => {}
            }
        }

        statement.push((class, text, span.end));
    }

    let block_keyword = blocks.last().filter(|b| !b.is_empty()).cloned();

    // Only the tokens the caret is actually touching matter from here on: a
    // half-typed word is its own token and must not count as "already named".
    let touching_word = statement
        .last()
        .is_some_and(|&(class, _, end)| end >= offset && is_word(class));
    let settled = if touching_word {
        &statement[..statement.len() - 1]
    } else {
        &statement[..]
    };

    if bracket_depth > 0 {
        // `[onDelete=|]`: the flag settled just before the `=` decides which
        // values make sense - `cascade` for `onDelete`, nothing for `pk`.
        if let Some(&(Class::Punctuation, "=", _)) = settled.last() {
            let flag = settled
                .len()
                .checked_sub(2)
                .and_then(|i| settled.get(i))
                .map(|&(_, text, _)| text.to_owned());
            return CursorContext {
                kind: ContextKind::FlagValue,
                block_keyword,
                qualifier: flag,
            };
        }
        return CursorContext {
            kind: ContextKind::Flags,
            block_keyword,
            qualifier: None,
        };
    }

    let kind = match settled.last() {
        Some(&(Class::Punctuation, ".", _)) => {
            let qualifier = settled
                .len()
                .checked_sub(2)
                .and_then(|i| settled.get(i))
                .map(|&(_, text, _)| text.to_owned());
            return CursorContext {
                kind: ContextKind::MemberOf,
                block_keyword,
                qualifier,
            };
        }
        Some(&(Class::RelationOp, _, _)) => ContextKind::RelationTarget,
        // `email ` inside a table body: the name is written, the type is next.
        Some(&(class, _, _)) if settled.len() == 1 && is_word(class) => ContextKind::TypePosition,
        None if is_settings_block(block_keyword.as_deref()) => ContextKind::SettingKey,
        None => ContextKind::StatementStart,
        Some(_) => ContextKind::StatementStart,
    };

    CursorContext {
        kind,
        block_keyword,
        qualifier: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes the caret as `|` so a case reads as the text someone typed.
    fn context_at(source_with_caret: &str) -> CursorContext {
        let offset = source_with_caret.find('|').expect("a | marking the caret");
        let source = source_with_caret.replace('|', "");
        cursor_context(&source, offset as u32)
    }

    #[test]
    fn inside_a_flag_list() {
        let ctx = context_at("Table T { id int [p| ] }");
        assert_eq!(ctx.kind, ContextKind::Flags);
        assert_eq!(ctx.block_keyword.as_deref(), Some("Table"));
    }

    #[test]
    fn a_flag_value_reports_the_flag_name() {
        let ctx = context_at("Table O { c >B.id [onDelete=|] }");
        assert_eq!(ctx.kind, ContextKind::FlagValue);
        assert_eq!(ctx.qualifier.as_deref(), Some("onDelete"));
    }

    #[test]
    fn a_half_typed_flag_value_still_reports_the_flag_name() {
        let ctx = context_at("Table O { c >B.id [onDelete=casc|] }");
        assert_eq!(ctx.kind, ContextKind::FlagValue);
        assert_eq!(ctx.qualifier.as_deref(), Some("onDelete"));
    }

    #[test]
    fn after_a_flag_value_and_a_comma_a_flag_name_comes_next() {
        let ctx = context_at("Table O { c >B.id [onDelete=cascade, |] }");
        assert_eq!(ctx.kind, ContextKind::Flags);
    }

    #[test]
    fn an_index_flag_list_reports_its_own_block() {
        let ctx = context_at("Table T { Index { i (a) [u|] } }");
        assert_eq!(ctx.kind, ContextKind::Flags);
        assert_eq!(
            ctx.block_keyword.as_deref(),
            Some("Index"),
            "so a client can offer index flags rather than field flags"
        );
    }

    #[test]
    fn after_a_field_name_the_type_comes_next() {
        assert_eq!(
            context_at("Table T { email |").kind,
            ContextKind::TypePosition
        );
        assert_eq!(
            context_at("Table T { email va|").kind,
            ContextKind::TypePosition,
            "a half-typed type is still a type position"
        );
    }

    #[test]
    fn after_a_relation_arrow() {
        assert_eq!(
            context_at("Table O { c >|").kind,
            ContextKind::RelationTarget
        );
        assert_eq!(
            context_at("Table C { o <Ord|").kind,
            ContextKind::RelationTarget
        );
    }

    #[test]
    fn after_a_dot_the_qualifier_is_reported() {
        let ctx = context_at("Table O { c >Customer.|");
        assert_eq!(ctx.kind, ContextKind::MemberOf);
        assert_eq!(ctx.qualifier.as_deref(), Some("Customer"));

        let half = context_at("Table O { c >Customer.i|");
        assert_eq!(half.kind, ContextKind::MemberOf);
        assert_eq!(half.qualifier.as_deref(), Some("Customer"));
    }

    #[test]
    fn a_settings_block_body_asks_for_keys() {
        let ctx = context_at("Project { |");
        assert_eq!(ctx.kind, ContextKind::SettingKey);
        assert_eq!(ctx.block_keyword.as_deref(), Some("Project"));

        let meta = context_at("Table T { Meta { | } }");
        assert_eq!(meta.kind, ContextKind::SettingKey);
        assert_eq!(meta.block_keyword.as_deref(), Some("Meta"));
    }

    #[test]
    fn a_declaration_body_asks_for_statements() {
        let ctx = context_at("Table T { |");
        assert_eq!(ctx.kind, ContextKind::StatementStart);
        assert_eq!(ctx.block_keyword.as_deref(), Some("Table"));
    }

    #[test]
    fn the_top_level_is_a_statement_start_without_a_block() {
        let ctx = context_at("spec 1;\n|");
        assert_eq!(ctx.kind, ContextKind::StatementStart);
        assert_eq!(ctx.block_keyword, None);
    }

    #[test]
    fn a_closed_block_no_longer_counts_as_the_enclosing_one() {
        let ctx = context_at("Table T { Meta { note = \"x\"; } |");
        assert_eq!(ctx.block_keyword.as_deref(), Some("Table"));
    }

    /// The reason this reads tokens instead of scanning text: braces inside
    /// strings and comments are not nesting.
    #[test]
    fn braces_in_strings_and_comments_do_not_nest() {
        let ctx = context_at("Table T { note = \"{{{\"; # }}} \n|");
        assert_eq!(ctx.block_keyword.as_deref(), Some("Table"));
    }

    #[test]
    fn a_statement_already_under_way_is_not_a_fresh_start() {
        // Two settled words: neither a bare type position nor an empty line.
        let ctx = context_at("Table T { id int |");
        assert_eq!(ctx.kind, ContextKind::StatementStart);
    }
}
