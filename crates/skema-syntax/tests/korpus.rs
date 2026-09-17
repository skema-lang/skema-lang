//! Test corpus: checks the example model against the lexer and parser.
//!
//! Files in `tests/` are **integration tests**: they're compiled as a
//! separate crate and only see the public API of `skema-syntax` — unlike
//! the `#[cfg(test)]` modules in the source, which can also reach private
//! items. That's a useful side effect: if something isn't reachable here,
//! a `pub use` is missing in `lib.rs`.

use skema_syntax::ast::{self, BlockKind, FieldTypeKind, RelationOp};
use skema_syntax::{parse, tokenize};

/// `include_str!` reads the file at **compile time** and embeds it as a
/// `&'static str`. No `std::fs`, no runtime path that can break — and the
/// test works even if the binary ends up somewhere else.
const EXAMPLE: &str = include_str!("../../../examples/myexample.skema");

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

#[test]
fn example_model_has_no_lexer_errors() {
    let errors: Vec<_> = tokenize(EXAMPLE)
        .into_iter()
        .filter(|t| t.kind.is_error())
        .map(|t| format!("{:?} at {:?}: {:?}", t.kind, t.span, t.text(EXAMPLE)))
        .collect();

    assert!(
        errors.is_empty(),
        "lexer errors in the example:\n{errors:#?}"
    );
}

#[test]
fn example_model_is_lexed_losslessly() {
    let tokens = tokenize(EXAMPLE);
    let reconstructed: String = tokens.iter().map(|t| t.text(EXAMPLE)).collect();
    assert_eq!(reconstructed, EXAMPLE);
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// The most important test in the corpus: the reference example must
/// parse cleanly.
#[test]
fn example_model_parses_without_messages() {
    let (_, messages) = parse(EXAMPLE);
    let text: Vec<_> = messages
        .iter()
        .map(|d| {
            format!(
                "{:?} at {:?}: {}  →  {:?}",
                d.severity,
                d.span,
                d.message,
                d.span.text(EXAMPLE)
            )
        })
        .collect();
    assert!(
        text.is_empty(),
        "parser messages in the example:\n{text:#?}"
    );
}

#[test]
fn example_model_contains_the_expected_declarations() {
    let (file, _) = parse(EXAMPLE);

    let count = |k: BlockKind| file.blocks().filter(|b| b.kind() == k).count();
    assert_eq!(count(BlockKind::Project), 1);
    assert_eq!(count(BlockKind::Document), 1);
    assert_eq!(count(BlockKind::Partial), 1);
    assert_eq!(count(BlockKind::Enum), 1);
    assert_eq!(count(BlockKind::Table), 2);
    assert_eq!(count(BlockKind::Group), 1);
    // Email and decimal
    assert_eq!(count(BlockKind::Datatype), 2);

    assert_eq!(file.spec().and_then(|s| s.version()), Some(1));
    // myexample.skema is deliberately runnable on its own (docs/README,
    // "All constructs in a single file") – imports are shown by
    // examples/generic/.
    assert_eq!(file.imports().count(), 0);
}

#[test]
fn table_order_is_fully_broken_down() {
    let (file, _) = parse(EXAMPLE);

    let order = file
        .blocks()
        .find(|b| b.name().is_some_and(|n| n.to_dotted() == "Order"))
        .expect("Table Order");

    let items: Vec<_> = order.items().collect();

    // `use common_base;`
    assert!(matches!(
        items[0].kind(),
        ast::ItemKind::Use(u) if u.target().to_dotted() == "common_base"
    ));

    // `customer >Customer.id [required, onDelete=cascade];`
    let ast::ItemKind::Field(customer) = items[1].kind() else {
        panic!("expected a field, found {:?}", items[1].kind());
    };
    assert_eq!(customer.name().text(), "customer");
    let Some(FieldTypeKind::Relation { op, target }) = customer.ty().and_then(|t| t.kind()) else {
        panic!("expected a relationship");
    };
    assert_eq!(op, RelationOp::OneToMany);
    assert_eq!(target.to_dotted(), "Customer.id");
    let flags: Vec<_> = customer.flags().collect();
    assert_eq!(flags.len(), 2);
    assert_eq!(flags[0].key().text(), "required");
    assert_eq!(flags[1].key().text(), "onDelete");

    // `gross_sum decimal(10, 2);` – a data type with arguments
    let gross_sum = order
        .items()
        .find_map(|item| match item.kind() {
            ast::ItemKind::Field(f) if f.name().text() == "gross_sum" => Some(f),
            _ => None,
        })
        .expect("gross_sum");
    let Some(FieldTypeKind::Named { path, args }) = gross_sum.ty().and_then(|t| t.kind()) else {
        panic!("expected a named type");
    };
    assert_eq!(path.to_dotted(), "decimal");
    assert_eq!(args.len(), 2);
}

#[test]
fn group_contains_refs_instead_of_declarations() {
    let (file, _) = parse(EXAMPLE);
    let group = file
        .blocks()
        .find(|b| b.kind() == BlockKind::Group)
        .expect("Group Shop");

    let refs: Vec<_> = group
        .items()
        .filter_map(|item| match item.kind() {
            ast::ItemKind::Ref(r) => Some((r.kind(), r.path().to_dotted())),
            _ => None,
        })
        .collect();

    assert_eq!(
        refs,
        [
            // Within the same file, the namespace prefix is optional
            // (`docs/02`) – that's why myexample.skema omits it.
            (BlockKind::Table, "Customer".to_owned()),
            (BlockKind::Table, "Order".to_owned()),
            (BlockKind::Enum, "OrderStatus".to_owned()),
        ]
    );
}

/// The `Datatype` block uses three special features at once: parameters,
/// a `Field` sub-block, and a `Dialect` sub-block.
#[test]
fn datatype_with_parameters_and_sub_blocks() {
    let (file, _) = parse(EXAMPLE);
    let dt = file
        .blocks()
        .find(|b| {
            b.kind() == BlockKind::Datatype && b.name().is_some_and(|n| n.to_dotted() == "decimal")
        })
        .expect("Datatype decimal");

    assert_eq!(dt.name().map(|n| n.to_dotted()).as_deref(), Some("decimal"));
    let params: Vec<_> = dt
        .params()
        .map(|p| p.name().map(|n| n.text().to_owned()).unwrap_or_default())
        .collect();
    assert_eq!(params, ["precision", "scale"]);

    let sub_blocks: Vec<_> = dt
        .items()
        .filter_map(|item| match item.kind() {
            ast::ItemKind::Block(b) => Some(b.kind()),
            _ => None,
        })
        .collect();
    assert_eq!(
        sub_blocks,
        [BlockKind::Field, BlockKind::Dialect, BlockKind::Dialect]
    );
}
