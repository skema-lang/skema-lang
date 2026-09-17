//! Property-style checks for `format::format` over real `.skema` text: the
//! example corpus (also used by `korpus.rs`) plus every fenced `skema` code
//! block in `docs/src/`. Unit-level rules (blank-line collapse, `=`
//! spacing, comment placement, …) live as `#[cfg(test)]` cases in
//! `src/format.rs` itself; what belongs here is the invariants that only
//! mean something over a large, varied body of real text.

use skema_syntax::ast::BlockKind;
use skema_syntax::{TokenKind, format, parse, reorder, tokenize};

const EXAMPLE: &str = include_str!("../../../examples/myexample.skema");
const GENERIC_PROJECT: &str = include_str!("../../../examples/generic/project.skema");
const GENERIC_CATALOG: &str = include_str!("../../../examples/generic/catalog.skema");
const GENERIC_LENDING: &str = include_str!("../../../examples/generic/lending.skema");
const GENERIC_TYPES: &str = include_str!("../../../examples/generic/types.skema");
const DIFF_OLD: &str = include_str!("../../../examples/diff/old.skema");
const DIFF_NEW: &str = include_str!("../../../examples/diff/new.skema");

fn example_corpus() -> Vec<(&'static str, &'static str)> {
    vec![
        ("myexample.skema", EXAMPLE),
        ("generic/project.skema", GENERIC_PROJECT),
        ("generic/catalog.skema", GENERIC_CATALOG),
        ("generic/lending.skema", GENERIC_LENDING),
        ("generic/types.skema", GENERIC_TYPES),
        ("diff/old.skema", DIFF_OLD),
        ("diff/new.skema", DIFF_NEW),
    ]
}

/// Every fenced ` ```skema ` block across `docs/src/`, embedded at compile
/// time so the test needs no filesystem access at runtime. Most blocks are
/// deliberately partial (one construct at a time) — fine here, since the
/// properties under test never assume a complete, valid file.
fn doc_code_blocks() -> Vec<(&'static str, String)> {
    const DOC_FILES: &[(&str, &str)] = &[
        (
            "docs/src/README.md",
            include_str!("../../../docs/src/README.md"),
        ),
        (
            "docs/src/01-language-basics.md",
            include_str!("../../../docs/src/01-language-basics.md"),
        ),
        (
            "docs/src/02-project-and-documents.md",
            include_str!("../../../docs/src/02-project-and-documents.md"),
        ),
        (
            "docs/src/03-meta-and-style.md",
            include_str!("../../../docs/src/03-meta-and-style.md"),
        ),
        (
            "docs/src/04-linting.md",
            include_str!("../../../docs/src/04-linting.md"),
        ),
        (
            "docs/src/05-tables-and-fields.md",
            include_str!("../../../docs/src/05-tables-and-fields.md"),
        ),
        (
            "docs/src/06-data-types.md",
            include_str!("../../../docs/src/06-data-types.md"),
        ),
        (
            "docs/src/07-relationships.md",
            include_str!("../../../docs/src/07-relationships.md"),
        ),
        (
            "docs/src/08-enums.md",
            include_str!("../../../docs/src/08-enums.md"),
        ),
        (
            "docs/src/09-partials.md",
            include_str!("../../../docs/src/09-partials.md"),
        ),
        (
            "docs/src/10-indexes.md",
            include_str!("../../../docs/src/10-indexes.md"),
        ),
        (
            "docs/src/11-physical-mapping.md",
            include_str!("../../../docs/src/11-physical-mapping.md"),
        ),
        (
            "docs/src/12-groups.md",
            include_str!("../../../docs/src/12-groups.md"),
        ),
    ];

    let mut blocks = Vec::new();
    for (path, text) in DOC_FILES {
        for (index, block) in extract_skema_fences(text).into_iter().enumerate() {
            blocks.push((*path, block));
            let _ = index; // only used for the (unused) uniqueness of the tuple below
        }
    }
    blocks
}

/// Pulls the content of every ` ```skema ` … ` ``` ` fence out of a
/// markdown file, in order.
fn extract_skema_fences(markdown: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut lines = markdown.lines();
    while let Some(line) = lines.next() {
        if line.trim() != "```skema" {
            continue;
        }
        let mut block = String::new();
        for line in lines.by_ref() {
            if line.trim() == "```" {
                break;
            }
            block.push_str(line);
            block.push('\n');
        }
        blocks.push(block);
    }
    blocks
}

/// A token's kind plus text, ignoring whitespace and the synthetic `Eof`
/// marker — what "the same tokens, in the same order" means here.
fn non_trivial_tokens(src: &str) -> Vec<(TokenKind, &str)> {
    tokenize(src)
        .into_iter()
        .filter(|t| !matches!(t.kind, TokenKind::Whitespace | TokenKind::Eof))
        .map(|t| (t.kind, t.text(src)))
        .collect()
}

#[test]
fn corpus_formatting_is_idempotent() {
    for (name, src) in example_corpus() {
        let once = format(src);
        let twice = format(&once);
        assert_eq!(once, twice, "{name}: formatting twice differs");
    }
}

#[test]
fn corpus_formatting_preserves_the_non_trivia_token_sequence() {
    for (name, src) in example_corpus() {
        let formatted = format(src);
        assert_eq!(
            non_trivial_tokens(src),
            non_trivial_tokens(&formatted),
            "{name}: token sequence changed"
        );
    }
}

#[test]
fn corpus_formatting_is_a_semantic_no_op() {
    for (name, src) in example_corpus() {
        let formatted = format(src);
        let (_, before) = parse(src);
        let (_, after) = parse(&formatted);
        assert_eq!(
            before, after,
            "{name}: diagnostics changed after formatting"
        );
    }
}

// ---------------------------------------------------------------------------
// `--sort` (`reorder::reorder`)
// ---------------------------------------------------------------------------

/// Position of a declaration's `BlockKind` in the canonical order — mirrors
/// `reorder::SORTED_KINDS` plus `Project`/`Document`'s own earlier slots,
/// kept independently here so the test can't share a bug with the code it
/// checks. Any kind outside the seven declaration kinds sorts last instead
/// of panicking — the ordering assertion below still catches it, just as a
/// reported mismatch rather than a hard crash mid-suite.
fn canonical_slot(kind: BlockKind) -> usize {
    const ORDER: [BlockKind; 7] = [
        BlockKind::Project,
        BlockKind::Document,
        BlockKind::Partial,
        BlockKind::Table,
        BlockKind::Enum,
        BlockKind::Datatype,
        BlockKind::Group,
    ];
    ORDER.iter().position(|k| *k == kind).unwrap_or(usize::MAX)
}

/// `(BlockKind, name)` for every top-level declaration, in file order.
fn declaration_signature(src: &str) -> Vec<(BlockKind, Option<String>)> {
    parse(src)
        .0
        .blocks()
        .map(|b| {
            (
                b.kind(),
                b.name()
                    .and_then(|path| path.as_ident())
                    .map(|i| i.text().to_owned()),
            )
        })
        .collect()
}

#[test]
fn corpus_sort_succeeds_on_every_example() {
    for (name, src) in example_corpus() {
        reorder(src).unwrap_or_else(|e| panic!("{name}: expected sorting to succeed, got {e}"));
    }
}

#[test]
fn corpus_sort_orders_declarations_canonically() {
    for (name, src) in example_corpus() {
        let sorted = reorder(src).expect("sorts").text;
        let sequence = declaration_signature(&sorted);

        let mut previous: Option<(usize, &str)> = None;
        for (kind, decl_name) in &sequence {
            let slot = canonical_slot(*kind);
            let decl_name = decl_name.as_deref().unwrap_or("");
            if let Some((prev_slot, prev_name)) = previous {
                assert!(
                    (prev_slot, prev_name) <= (slot, decl_name),
                    "{name}: {:?} {decl_name:?} out of canonical order after slot {prev_slot} name {prev_name:?}",
                    kind
                );
            }
            previous = Some((slot, decl_name));
        }
    }
}

#[test]
fn corpus_sort_keeps_the_same_declarations() {
    for (name, src) in example_corpus() {
        let sorted = reorder(src).expect("sorts").text;

        let mut before = declaration_signature(src);
        let mut after = declaration_signature(&sorted);
        before.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
        after.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
        assert_eq!(before, after, "{name}: declaration set changed by sorting");
    }
}

#[test]
fn corpus_sort_is_a_semantic_no_op_beyond_comments() {
    for (name, src) in example_corpus() {
        let sorted = reorder(src).expect("sorts").text;
        let (_, before_diag) = parse(src);
        let (_, after_diag) = parse(&sorted);
        assert_eq!(
            before_diag.len(),
            after_diag.len(),
            "{name}: diagnostic count changed by sorting"
        );
    }
}

#[test]
fn corpus_sort_is_idempotent() {
    for (name, src) in example_corpus() {
        let once = reorder(src).expect("sorts").text;
        let twice = reorder(&once).expect("re-sorting the sorted output must also succeed");
        assert_eq!(once, twice.text, "{name}: sorting twice differs");
        assert_eq!(
            twice.comments_removed, 0,
            "{name}: a second sort still found comments to remove"
        );
    }
}

/// Doc snippets are illustrative, not real files — several are deliberately
/// partial constructs, and at least one (`01-language-basics.md`'s
/// "Anatomy of a declaration" diagram) uses leading spaces to visually
/// point at columns rather than to express nesting. Depth-based reindent
/// correctly overwrites that padding, which can shift a diagnostic's byte
/// span without changing its shape — so unlike the real corpus below, doc
/// blocks are checked for the two invariants that hold regardless of
/// whether the fragment is valid skema at all: idempotence, and that no
/// non-whitespace token was added, removed, reordered, or edited.
#[test]
fn doc_code_blocks_format_idempotently_and_keep_their_tokens() {
    let blocks = doc_code_blocks();
    assert!(!blocks.is_empty(), "expected at least one ```skema block");

    for (path, block) in &blocks {
        let once = format(block);
        let twice = format(&once);
        assert_eq!(once, twice, "{path}: formatting twice differs on {block:?}");
        assert_eq!(
            non_trivial_tokens(block),
            non_trivial_tokens(&once),
            "{path}: token sequence changed on {block:?}"
        );
    }
}
