//! Task 66 spike: the rowan tree is lossless, and the typed view over it
//! describes exactly what the owned AST of the same parse describes.

// Helpers outside `#[test]` fns are not covered by clippy.toml's test allowance.
#![allow(clippy::expect_used)]

use skema_syntax::ast::{self, AstNode};
use skema_syntax::legacy_ast as old;
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
        assert_eq!(file.syntax().syntax().to_string(), src, "{name}");
    }
}

#[test]
fn view_describes_the_same_tree_as_the_owned_ast() {
    for (name, src) in corpus()
        .into_iter()
        .chain(garbage().into_iter().map(|s| ("garbage".to_owned(), s)))
    {
        let (file, _) = parse(&src);
        let lowered = lower_file(&file.syntax(), &file);
        assert_eq!(lowered, file, "{name}:\n{src}");
    }
}

#[test]
fn offsets_are_the_source_byte_offsets() {
    let src = "Meta { note = \"Grüße\"; } # ä\nTable Ä { id int; }";
    let (file, _) = parse(src);
    let view = file.syntax();
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
    let table = file.syntax().blocks().next().expect("a block");
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
        .syntax()
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
    let texts: Vec<_> = file
        .syntax()
        .items()
        .map(|i| i.syntax().to_string())
        .collect();
    assert_eq!(texts, ["spec 1;", "/* customers */\nTable Customer { }"]);
    let table = file.syntax().blocks().next().expect("Customer");
    assert_eq!(
        table.span(),
        old_span(&file, 0),
        "span ignores the attached comment"
    );
}

fn old_span(file: &old::SourceFile, block: usize) -> skema_syntax::Span {
    file.items
        .iter()
        .filter(|i| matches!(i.kind, old::ItemKind::Block(_)))
        .nth(block)
        .expect("block")
        .span
}

// ---------------------------------------------------------------------------
// Permissive parsing: broken input still yields a navigable tree
// ---------------------------------------------------------------------------

#[test]
fn a_broken_field_stays_local_in_the_view() {
    let (file, messages) = parse(BROKEN[0]);
    assert!(!messages.is_empty());
    let view = file.syntax();
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
            ast::ItemKind::Field(f) => f.name().map(|n| n.text().to_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(fields, ["id", "email"]);
    assert!(customer.items().any(|i| i.kind() == ast::ItemKind::Error));
}

#[test]
fn unclosed_block_at_end_of_file_is_navigable() {
    let (file, _) = parse("Table A { id int [pk];");
    let a = file.syntax().blocks().next().expect("Table A");
    assert_eq!(a.items().count(), 1);
    assert_eq!(a.body_start(), Some(9));
}

#[test]
fn a_severity_override_is_part_of_its_setting_not_its_attr() {
    let (file, _) = parse("Lint { fieldNaming = snake_case [error]; }");
    let lint = file.syntax().blocks().next().expect("Lint");
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
        let lint = file.syntax().blocks().next().expect("Lint");
        let ast::ItemKind::Setting(first) = lint.items().next().expect("item").kind() else {
            panic!("expected a setting in {src:?}");
        };
        assert!(first.severity_override().is_none(), "{src:?}");
    }
    let (file, _) = parse(BROKEN[BROKEN.len() - 3]);
    let keys: Vec<_> = file
        .syntax()
        .blocks()
        .next()
        .expect("Lint")
        .items()
        .filter_map(|i| match i.kind() {
            ast::ItemKind::Setting(a) => a.key().map(|k| k.text()),
            _ => None,
        })
        .collect();
    assert_eq!(keys, ["fieldNaming", "tableNaming"]);
}

// ---------------------------------------------------------------------------
// view → owned AST, for the parity test
// ---------------------------------------------------------------------------

fn lower_file(file: &ast::SourceFile, parsed: &old::SourceFile) -> old::SourceFile {
    old::SourceFile {
        items: file.items().map(|i| lower_item(&i)).collect(),
        span: file.span(),
        green: parsed.green.clone(),
    }
}

fn lower_item(item: &ast::Item) -> old::Item {
    let kind = match item.kind() {
        ast::ItemKind::Spec(s) => old::ItemKind::Spec(old::Spec {
            version: s.version(),
        }),
        ast::ItemKind::Import(i) => old::ItemKind::Import(old::Import {
            path: i.path(),
            alias: i.alias().map(|a| ident(&a)),
        }),
        ast::ItemKind::Use(u) => old::ItemKind::Use(old::Use {
            target: path(&u.target().expect("use target")),
        }),
        ast::ItemKind::Setting(a) => old::ItemKind::Setting(attr(&a)),
        ast::ItemKind::Field(f) => old::ItemKind::Field(old::Field {
            name: ident(&f.name().expect("field name")),
            ty: f.ty().map(|t| old::FieldType {
                span: t.span(),
                kind: match t.kind().expect("checked by ty()") {
                    ast::FieldTypeKind::Named { path: p, args } => old::FieldTypeKind::Named {
                        path: path(&p),
                        args: args.iter().map(value).collect(),
                    },
                    ast::FieldTypeKind::Relation { op, target } => old::FieldTypeKind::Relation {
                        op,
                        target: path(&target),
                    },
                },
            }),
            flags: f.flags().map(|a| attr(&a)).collect(),
            items: f.items().map(|i| lower_item(&i)).collect(),
        }),
        ast::ItemKind::Block(b) => old::ItemKind::Block(old::Block {
            keyword: ident(&b.keyword().expect("keyword")),
            kind: b.kind(),
            selector: b.selector().map(|s| ident(&s)),
            name: b.name().map(|n| path(&n)),
            params: b
                .params()
                .map(|p| old::Param {
                    name: ident(&p.name().expect("param name")),
                    ty: p.ty().map(|t| path(&t)),
                    span: p.span(),
                })
                .collect(),
            body_brace: b.body_brace().expect("body brace"),
            items: b.items().map(|i| lower_item(&i)).collect(),
        }),
        ast::ItemKind::Ref(r) => old::ItemKind::Ref(old::Ref {
            keyword: ident(&r.keyword().expect("keyword")),
            kind: r.kind(),
            path: path(&r.path().expect("ref path")),
        }),
        ast::ItemKind::IndexEntry(e) => old::ItemKind::IndexEntry(old::IndexEntry {
            name: e.name().map(|n| ident(&n)),
            columns: e.columns().map(|c| ident(&c)).collect(),
            flags: e.flags().map(|a| attr(&a)).collect(),
        }),
        ast::ItemKind::Error => old::ItemKind::Error,
    };
    old::Item {
        kind,
        span: item.span(),
    }
}

fn ident(i: &ast::Ident) -> old::Ident {
    old::Ident {
        text: i.text().to_owned(),
        span: i.span(),
    }
}

fn path(p: &ast::Path) -> old::Path {
    old::Path {
        segments: p.segments().map(|s| ident(&s)).collect(),
        span: p.span(),
    }
}

fn attr(a: &ast::Attr) -> old::Attr {
    let key = a.key().expect("key");
    old::Attr {
        key: old::Key {
            text: key.text(),
            quoted: key.quoted(),
            span: key.span(),
        },
        value: a.value().map(|v| value(&v)),
        severity_override: a.severity_override().map(|i| ident(&i)),
        span: a.span(),
    }
}

fn value(v: &ast::Value) -> old::Value {
    let kind = match v.kind().expect("filled value") {
        ast::ValueKind::Path(p) => old::ValueKind::Path(path(&p)),
        ast::ValueKind::Str(s) => old::ValueKind::Str(s),
        ast::ValueKind::Int(n) => old::ValueKind::Int(n),
        ast::ValueKind::Float(f) => old::ValueKind::Float(f),
        ast::ValueKind::Raw(r) => old::ValueKind::Raw(r),
        ast::ValueKind::List(items) => old::ValueKind::List(items.iter().map(value).collect()),
        ast::ValueKind::Type { path: p, args } => old::ValueKind::Type {
            path: path(&p),
            args: args.iter().map(value).collect(),
        },
    };
    old::Value {
        kind,
        span: v.span(),
    }
}
