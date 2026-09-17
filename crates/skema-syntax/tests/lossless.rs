//! The rowan tree is lossless, and the typed view over it is pinned by a
//! golden snapshot of the corpus.

// Helpers outside `#[test]` fns are not covered by clippy.toml's test allowance.
#![allow(clippy::expect_used)]

use std::fmt::Write as _;

use skema_syntax::ast::{self, AstNode};
use skema_syntax::{SyntaxKind, parse};

const EXAMPLES: &[(&str, &str)] = &[
    (
        "myexample",
        include_str!("../../../examples/myexample.skema"),
    ),
    (
        "generic/catalog",
        include_str!("../../../examples/generic/catalog.skema"),
    ),
    (
        "generic/lending",
        include_str!("../../../examples/generic/lending.skema"),
    ),
    (
        "generic/project",
        include_str!("../../../examples/generic/project.skema"),
    ),
    (
        "generic/types",
        include_str!("../../../examples/generic/types.skema"),
    ),
];

const DOCS: &[&str] = &[
    include_str!("../../../docs/src/01-language-basics.md"),
    include_str!("../../../docs/src/02-project-and-documents.md"),
    include_str!("../../../docs/src/03-meta-and-style.md"),
    include_str!("../../../docs/src/04-linting.md"),
    include_str!("../../../docs/src/05-tables-and-fields.md"),
    include_str!("../../../docs/src/06-data-types.md"),
    include_str!("../../../docs/src/07-relationships.md"),
    include_str!("../../../docs/src/08-enums.md"),
    include_str!("../../../docs/src/09-partials.md"),
    include_str!("../../../docs/src/10-indexes.md"),
    include_str!("../../../docs/src/11-physical-mapping.md"),
    include_str!("../../../docs/src/12-groups.md"),
    include_str!("../../../docs/src/README.md"),
];

/// Deliberately broken input, including the grammar tests' error cases.
const BROKEN: &[&str] = &[
    "Table Customer {\n  id    int [pk];\n  ???   ???\n  email varchar [unique];\n}\nTable Order { id int [pk]; }",
    "Meta { author: \"anna\"; }",
    "Table A { id int email varchar; }",
    "Table A { id int [pk];",
    "Meta { = ; }",
    "Table A { tags <>Tag.id; }",
    "Meta { author = \"a\" note = \"b\" }",
    "Table A { customer >; name text; }",
    "Table A { x decimal(10, ; }",
    "use ;\nimport ;\nspec ;\nspec 99999999999999999999;",
    "Project { Lint { } } Table A { Lint { } }",
    "Meta { \"unterminated = 1; }",
    "Meta { raw = `oops; }",
    "/* never closed\nTable A { }",
    "Table § { a int; }",
    "Datatype decimal(precision int, , scale) { }",
    "Group G { Table ; Table a.; }",
    "Style @ { } Style @table highlight { }",
    "(a, 1 b) [unique, =];",
    "Table A {\r\n  # crlf\r\n  id int;\r\n}\r\n",
    "Lint { fieldNaming = snake_case []; }",
    "Lint { fieldNaming = snake_case [a, b]; }",
    "Lint { fieldNaming = snake_case [1]; }",
    "Lint {\n  fieldNaming = snake_case [\n  tableNaming = PascalCase;\n}",
    "Lint { fieldNaming = snake_case [error; tableNaming = x [warn] }",
    "Lint { fieldNaming [error]; = x [warn]; }",
];

/// Settings with a well-formed severity override.
const SEVERITY_OVERRIDES: &[&str] = &[
    "Lint { fieldNaming = snake_case [error]; }",
    "Meta { author = \"anna\" [warn]; }",
    "Project {\n  Lint {\n    # strict\n    tableNaming = PascalCase [ error ]; # why\n    fieldNaming = snake_case [warn];\n  }\n}",
    "Table A { email varchar [unique]; (email) [unique]; }",
];

fn doc_snippets() -> Vec<String> {
    let mut snippets = Vec::new();
    for doc in DOCS {
        let mut current: Option<String> = None;
        for line in doc.lines() {
            match current.as_mut() {
                None if line.trim_start().starts_with("```skema") => current = Some(String::new()),
                Some(_) if line.trim_start().starts_with("```") => {
                    snippets.extend(current.take());
                }
                Some(snippet) => {
                    snippet.push_str(line);
                    snippet.push('\n');
                }
                None => {}
            }
        }
    }
    snippets
}

fn garbage() -> Vec<String> {
    let pieces = [
        "Table", "{", "}", "[", "]", "(", ")", ";", "=", ",", ".", "@", ">", "<>", "\"a\"", "`r`",
        "1", "x", "use", "spec", "import", ":", "# c\n", "\n\n",
    ];
    let mut out = Vec::new();
    for a in pieces {
        for b in pieces {
            for c in pieces {
                out.push(format!("{a} {b} {c}"));
            }
        }
    }
    out
}

fn corpus() -> Vec<(String, String)> {
    let mut all: Vec<(String, String)> = EXAMPLES
        .iter()
        .map(|(name, src)| ((*name).to_owned(), (*src).to_owned()))
        .collect();
    all.extend(
        doc_snippets()
            .into_iter()
            .enumerate()
            .map(|(i, s)| (format!("docs#{i}"), s)),
    );
    all.extend(
        SEVERITY_OVERRIDES
            .iter()
            .enumerate()
            .map(|(i, s)| (format!("severity#{i}"), (*s).to_owned())),
    );
    all.extend(
        BROKEN
            .iter()
            .enumerate()
            .map(|(i, s)| (format!("broken#{i}"), (*s).to_owned())),
    );
    all
}

#[test]
fn every_corpus_file_round_trips_byte_for_byte() {
    let corpus = corpus();
    assert!(
        corpus.len() > 80,
        "corpus unexpectedly small: {}",
        corpus.len()
    );
    for (name, src) in corpus
        .iter()
        .cloned()
        .chain(garbage().into_iter().map(|s| ("garbage".to_owned(), s)))
    {
        let (file, _) = parse(&src);
        assert_eq!(file.syntax().to_string(), src, "{name}");
    }
}

#[test]
fn offsets_are_the_source_byte_offsets() {
    let src = "Meta { note = \"Grüße\"; } # ä\nTable Ä { id int; }";
    let (view, _) = parse(src);
    for token in view
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
    {
        let range = token.text_range();
        assert_eq!(
            &src[usize::from(range.start())..usize::from(range.end())],
            token.text()
        );
    }
    let table = view.blocks().nth(1).expect("Table");
    assert_eq!(table.name().expect("name").span().text(src), "Ä");
}

// ---------------------------------------------------------------------------
// Trivia attachment
// ---------------------------------------------------------------------------

fn items_text(src: &str) -> Vec<String> {
    let (file, _) = parse(src);
    let table = file.blocks().next().expect("a block");
    table.items().map(|i| i.syntax().to_string()).collect()
}

#[test]
fn a_comment_directly_above_a_statement_belongs_to_it() {
    let items = items_text("Table T {\n  # the key\n  // really\n  id int;\n}");
    assert_eq!(items, ["# the key\n  // really\n  id int;"]);
}

#[test]
fn a_blank_line_detaches_a_comment() {
    let src = "Table T {\n  # section\n\n  id int;\n}";
    assert_eq!(items_text(src), ["id int;"]);
    let (file, _) = parse(src);
    let body = file
        .blocks()
        .next()
        .expect("T")
        .syntax()
        .children()
        .find(|n| n.kind() == SyntaxKind::Body)
        .expect("body");
    assert!(
        body.children_with_tokens()
            .any(|e| e.kind() == SyntaxKind::LineComment),
        "a detached comment stays in the enclosing body"
    );
}

#[test]
fn a_comment_on_the_same_line_after_a_statement_belongs_to_it() {
    let items = items_text("Table T {\n  id int; # pk\n  name text;\n}");
    assert_eq!(items, ["id int; # pk", "name text;"]);
}

#[test]
fn a_top_level_leading_comment_belongs_to_the_declaration() {
    let (file, _) = parse("spec 1;\n\n/* customers */\nTable Customer { }\n");
    let texts: Vec<_> = file.items().map(|i| i.syntax().to_string()).collect();
    assert_eq!(texts, ["spec 1;", "/* customers */\nTable Customer { }"]);
    let table = file.blocks().next().expect("Customer");
    assert_eq!(
        table.span(),
        skema_syntax::Span::new(25, 43),
        "span ignores the attached comment"
    );
}

// ---------------------------------------------------------------------------
// Permissive parsing: broken input still yields a navigable tree
// ---------------------------------------------------------------------------

#[test]
fn a_broken_field_stays_local_in_the_view() {
    let (view, messages) = parse(BROKEN[0]);
    assert!(!messages.is_empty());
    let names: Vec<_> = view
        .blocks()
        .filter_map(|b| b.name())
        .map(|n| n.to_dotted())
        .collect();
    assert_eq!(names, ["Customer", "Order"]);
    let customer = view.blocks().next().expect("Customer");
    let fields: Vec<_> = customer
        .items()
        .filter_map(|i| match i.kind() {
            ast::ItemKind::Field(f) => Some(f.name().text().to_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(fields, ["id", "email"]);
    assert!(customer.items().any(|i| i.kind() == ast::ItemKind::Error));
}

#[test]
fn unclosed_block_at_end_of_file_is_navigable() {
    let (file, _) = parse("Table A { id int [pk];");
    let a = file.blocks().next().expect("Table A");
    assert_eq!(a.items().count(), 1);
    assert_eq!(a.body_start(), Some(9));
}

#[test]
fn a_severity_override_is_part_of_its_setting_not_its_attr() {
    let (file, _) = parse("Lint { fieldNaming = snake_case [error]; }");
    let lint = file.blocks().next().expect("Lint");
    let item = lint.items().next().expect("setting");
    assert_eq!(
        item.syntax().to_string(),
        "fieldNaming = snake_case [error];"
    );
    let ast::ItemKind::Setting(setting) = item.kind() else {
        panic!("expected a setting");
    };
    assert_eq!(setting.syntax().to_string(), "fieldNaming = snake_case");
    assert_eq!(
        setting.severity_override().map(|i| i.text().to_owned()),
        Some("error".to_owned())
    );
}

#[test]
fn malformed_severity_overrides_expose_none_and_keep_the_next_setting() {
    for src in &BROKEN[BROKEN.len() - 6..BROKEN.len() - 2] {
        let (file, messages) = parse(src);
        assert!(!messages.is_empty(), "{src:?}");
        let lint = file.blocks().next().expect("Lint");
        let ast::ItemKind::Setting(first) = lint.items().next().expect("item").kind() else {
            panic!("expected a setting in {src:?}");
        };
        assert!(first.severity_override().is_none(), "{src:?}");
    }
    let (file, _) = parse(BROKEN[BROKEN.len() - 3]);
    let keys: Vec<_> = file
        .blocks()
        .next()
        .expect("Lint")
        .items()
        .filter_map(|i| match i.kind() {
            ast::ItemKind::Setting(a) => Some(a.key().text()),
            _ => None,
        })
        .collect();
    assert_eq!(keys, ["fieldNaming", "tableNaming"]);
}

// ---------------------------------------------------------------------------
// Cast-time validation: a node missing a required part reads as `Error`,
// and the accessor for that part is never `Option` (task 92, owner decision
// "validate required parts at cast time").
//
// The grammar never actually emits a `Block`/`Field`/`Use`/`Ref`/`Setting`
// node without its required part — `parse_item_kind` only calls into these
// productions once the required first token is already confirmed present,
// so a failure there falls back to a plain `Error` item before the node is
// even opened (`grammar::parse_block_or_ref`, `parse_field`, `parse_use`).
// These tests build the malformed shape directly with a `GreenNodeBuilder`,
// bypassing the parser, to pin down `AstNode::cast`'s own contract rather
// than a parser behaviour that already happens to hold.
// ---------------------------------------------------------------------------

use rowan::{GreenNodeBuilder, Language};
use skema_syntax::syntax_kind::SkemaLanguage;

fn raw(kind: SyntaxKind) -> rowan::SyntaxKind {
    SkemaLanguage::kind_to_raw(kind)
}

/// A lone node of `kind`, with no children — the shape every one of these
/// tests needs: present, but with its required part missing.
fn synthetic_item(kind: SyntaxKind) -> ast::Item {
    let mut builder = GreenNodeBuilder::new();
    builder.start_node(raw(SyntaxKind::SourceFile));
    builder.start_node(raw(kind));
    builder.finish_node();
    builder.finish_node();
    ast::SourceFile::new_root(builder.finish())
        .items()
        .next()
        .expect("one item")
}

#[test]
fn block_without_a_keyword_reads_as_error() {
    assert_eq!(
        synthetic_item(SyntaxKind::Block).kind(),
        ast::ItemKind::Error
    );
}

#[test]
fn field_without_a_name_reads_as_error() {
    assert_eq!(
        synthetic_item(SyntaxKind::Field).kind(),
        ast::ItemKind::Error
    );
}

#[test]
fn use_without_a_target_reads_as_error() {
    assert_eq!(synthetic_item(SyntaxKind::Use).kind(), ast::ItemKind::Error);
}

#[test]
fn ref_without_a_path_reads_as_error() {
    assert_eq!(synthetic_item(SyntaxKind::Ref).kind(), ast::ItemKind::Error);
}

/// A `Setting` wraps an `Attr`; an `Attr` with no key token makes the
/// `Setting` unreadable too, since `Attr::cast` — not just `Setting::cast` —
/// refuses it.
#[test]
fn setting_whose_attr_has_no_key_reads_as_error() {
    let mut builder = GreenNodeBuilder::new();
    builder.start_node(raw(SyntaxKind::SourceFile));
    builder.start_node(raw(SyntaxKind::Setting));
    builder.start_node(raw(SyntaxKind::Attr));
    builder.token(raw(SyntaxKind::Eq), "=");
    builder.finish_node(); // Attr
    builder.finish_node(); // Setting
    builder.finish_node(); // SourceFile
    let file = ast::SourceFile::new_root(builder.finish());
    let item = file.items().next().expect("one item");
    assert_eq!(item.kind(), ast::ItemKind::Error);

    // `Attr::cast` itself refuses the same node directly.
    let attr_node = file
        .syntax()
        .descendants()
        .find(|n| n.kind() == SyntaxKind::Attr)
        .expect("the Attr node is still in the tree");
    assert!(ast::Attr::cast(attr_node).is_none());
}

/// The other side of cast-time validation: once a node *does* have its
/// required part, the accessor hands it back directly — no `Option` to
/// thread through every call site.
#[test]
fn accessors_for_required_parts_are_not_optional() {
    let (view, messages) =
        parse("use base;\nmode = draft;\nTable Customer { id int; }\nGroup g { Table Customer; }");
    assert!(messages.is_empty());

    let ast::ItemKind::Use(u) = view.items().next().expect("use item").kind() else {
        panic!("expected a use item");
    };
    let _: ast::Path = u.target();

    let ast::ItemKind::Setting(setting) = view.items().nth(1).expect("setting item").kind() else {
        panic!("expected a setting item");
    };
    let _: ast::Key = setting.key();

    let table = view.blocks().next().expect("Table");
    let _: ast::Ident = table.keyword();
    let ast::ItemKind::Field(f) = table.items().next().expect("field").kind() else {
        panic!("expected a field");
    };
    let _: ast::Ident = f.name();

    let group = view.blocks().nth(1).expect("Group");
    let ast::ItemKind::Ref(r) = group.items().next().expect("ref").kind() else {
        panic!("expected a ref");
    };
    let _: ast::Path = r.path();
}

// ---------------------------------------------------------------------------
// Golden snapshot of every view accessor and diagnostic per corpus file,
// captured while the view still matched the removed owned AST (task 92f).
// Regenerate after an intended grammar change: SKEMA_UPDATE_SNAPSHOT=1.
// ---------------------------------------------------------------------------

const SNAPSHOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/corpus.snap");

#[test]
fn corpus_matches_the_golden_snapshot() {
    let mut actual = String::new();
    for (name, src) in corpus() {
        let _ = writeln!(actual, "== {name}");
        actual.push_str(&describe(&src));
    }
    // Too many to store in full; a digest still catches any change.
    let garbage: String = garbage().iter().map(|src| describe(src)).collect();
    let _ = writeln!(actual, "== garbage fnv1a64={:016x}", fnv1a64(&garbage));

    if std::env::var_os("SKEMA_UPDATE_SNAPSHOT").is_some() {
        std::fs::write(SNAPSHOT, &actual).expect("write snapshot");
        return;
    }
    let expected = std::fs::read_to_string(SNAPSHOT).expect("read snapshot");
    for (line, (a, e)) in actual.lines().zip(expected.lines()).enumerate() {
        assert_eq!(a, e, "snapshot line {}", line + 1);
    }
    assert_eq!(actual.lines().count(), expected.lines().count());
}

fn fnv1a64(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn describe(src: &str) -> String {
    let (view, diagnostics) = parse(src);
    let mut out = String::new();
    let _ = writeln!(out, "file {:?}", view.span());
    for item in view.items() {
        describe_item(&mut out, 1, &item);
    }
    for d in diagnostics {
        let _ = writeln!(out, "diag {:?} {:?} {:?}", d.span, d.severity, d.message);
    }
    out
}

fn describe_item(out: &mut String, depth: usize, item: &ast::Item) {
    let head = match item.kind() {
        ast::ItemKind::Spec(s) => format!("Spec {:?}", s.version()),
        ast::ItemKind::Import(i) => format!("Import {:?} {}", i.path(), opt(i.alias(), id)),
        ast::ItemKind::Use(u) => format!("Use {}", path(&u.target())),
        ast::ItemKind::Setting(a) => format!("Setting {}", attr(&a)),
        ast::ItemKind::Field(f) => format!(
            "Field {} {} [{}]",
            id(&f.name()),
            opt(f.ty(), field_type),
            list(f.flags(), attr)
        ),
        ast::ItemKind::Block(b) => format!(
            "Block {} {:?} {} {} ({}) {:?}",
            id(&b.keyword()),
            b.kind(),
            opt(b.selector(), id),
            opt(b.name(), path),
            list(b.params(), |p| format!(
                "{} {} {:?}",
                opt(p.name(), id),
                opt(p.ty(), path),
                p.span()
            )),
            b.body_brace()
        ),
        ast::ItemKind::Ref(r) => format!(
            "Ref {} {:?} {}",
            opt(r.keyword(), id),
            r.kind(),
            path(&r.path())
        ),
        ast::ItemKind::IndexEntry(e) => format!(
            "IndexEntry {} ({}) [{}]",
            opt(e.name(), id),
            list(e.columns(), id),
            list(e.flags(), attr)
        ),
        ast::ItemKind::Error => "Error".to_owned(),
    };
    let _ = writeln!(
        out,
        "{:indent$}{:?} {head}",
        "",
        item.span(),
        indent = depth * 2
    );
    let children: Vec<ast::Item> = match item.kind() {
        ast::ItemKind::Field(f) => f.items().collect(),
        ast::ItemKind::Block(b) => b.items().collect(),
        _ => Vec::new(),
    };
    for child in children {
        describe_item(out, depth + 1, &child);
    }
}

fn opt<T>(value: Option<T>, show: impl Fn(&T) -> String) -> String {
    value.map_or_else(|| "-".to_owned(), |v| show(&v))
}

fn list<T>(values: impl Iterator<Item = T>, show: impl Fn(&T) -> String) -> String {
    values.map(|v| show(&v)).collect::<Vec<_>>().join(", ")
}

fn id(ident: &ast::Ident) -> String {
    format!("{:?}@{:?}", ident.text(), ident.span())
}

fn path(p: &ast::Path) -> String {
    format!("<{}>@{:?}", list(p.segments(), id), p.span())
}

fn attr(a: &ast::Attr) -> String {
    let key = a.key();
    format!(
        "{{{:?} quoted={} @{:?} = {} override={}}}@{:?}",
        key.text(),
        key.quoted(),
        key.span(),
        opt(a.value(), value),
        opt(a.severity_override(), id),
        a.span()
    )
}

fn field_type(t: &ast::FieldType) -> String {
    let kind = match t.kind() {
        Some(ast::FieldTypeKind::Named { path: p, args }) => {
            format!("Named {} ({})", path(&p), list(args.iter(), |v| value(v)))
        }
        Some(ast::FieldTypeKind::Relation { op, target }) => {
            format!("Relation {op:?} {}", path(&target))
        }
        None => "?".to_owned(),
    };
    format!("{kind}@{:?}", t.span())
}

fn value(v: &ast::Value) -> String {
    let kind = match v.kind() {
        Some(ast::ValueKind::Path(p)) => path(&p),
        Some(ast::ValueKind::Str(s)) => format!("Str {s:?}"),
        Some(ast::ValueKind::Int(n)) => format!("Int {n}"),
        Some(ast::ValueKind::Float(f)) => format!("Float {f:?}"),
        Some(ast::ValueKind::Raw(r)) => format!("Raw {r:?}"),
        Some(ast::ValueKind::List(items)) => format!("[{}]", list(items.iter(), |i| value(i))),
        Some(ast::ValueKind::Type { path: p, args }) => {
            format!("Type {} ({})", path(&p), list(args.iter(), |a| value(a)))
        }
        None => "?".to_owned(),
    };
    format!("{kind}@{:?}", v.span())
}
