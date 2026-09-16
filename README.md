# skema

A DSL for entity-relationship models: parser, semantic checking, CLI,
language server, Python bindings. The core is Rust and platform-neutral, so
the same frontend runs in a browser as WebAssembly.

Model files use the **`.skema`** extension. The language definition lives in
[`docs/src/`](docs/src/README.md), reference models in
[`examples/`](examples/).

> `docs/` is the finished language reference (English). This README covers
> the implementation on top of it — crates, tools, build status.

> Architecture, open tasks and decisions spanning all four `skema-language`
> repos live in the shared [brain](../brain/README.md) — read it before
> starting non-trivial work here, and update it before you finish.

## Getting started

```bash
direnv allow          # once; the toolchain is then available automatically
# or without direnv:
devenv shell

check                 # fmt + clippy + tests
t                     # tests only
fix                   # fmt + clippy --fix
wasm-test             # skema-wasm's tests under wasm32, in Node
```

Without `direnv`, prefix every command with `devenv shell -- `. Fish users
want this hook:

```fish
echo 'direnv hook fish | source' >> ~/.config/fish/config.fish
```

## The three tools

### CLI – `skema check` / `skema export` / `skema diff` / `skema fmt`

```bash
cargo run -p skema-cli -- check examples/generic/project.skema
cargo run -p skema-cli -- check examples/generic/project.skema --json
cargo run -p skema-cli -- export --json examples/generic/project.skema > model.json
cargo run -p skema-cli -- export --json --locations examples/generic/project.skema
cargo run -p skema-cli -- diff examples/diff/old.skema examples/diff/new.skema
cargo run -p skema-cli -- diff --json examples/diff/old.skema examples/diff/new.skema
cargo run -p skema-cli -- diff --fail-on-breaking examples/diff/old.skema examples/diff/new.skema
cargo run -p skema-cli -- fmt examples/generic/project.skema
cargo run -p skema-cli -- fmt --check examples/generic/project.skema
cargo run -p skema-cli -- fmt --sort examples/generic/project.skema
```

`check` loads the root file plus everything it reaches via `import` (project
membership is reachability-defined, `docs/02`) and reports every syntax and
semantic error as `path:line:column: severity: text`. `export` also writes
the resolved model to stdout — `Partial`s applied, relationships resolved,
physical names cascaded — with messages on stderr, so the redirect above
yields the model alone.

`diff` loads and checks two project roots independently — they are plain
paths, not git refs, so comparing two revisions means checking each out to
its own directory first — and structurally compares their exported models:
tables and enums added, dropped, renamed (`Meta.movedFrom`), or changed
inside (a column, its relation, or an index), each flagged `breaking` or
not. Either side failing its own check is reported the same way `check`
would report it, and no diff is attempted. Default output is one line per
change, e.g.:

```
Table Purchase: renamed from Order
Table Purchase: column customer_id dropped [BREAKING]
Table Shipment: table added
```

`--fail-on-breaking` exits non-zero when any entry is breaking; without it,
`diff` is report-only and always exits zero.

`fmt` reindents a file to 2 spaces per `{`/`}` depth and collapses runs of
blank lines to one, working straight off the lexer's token stream
(`skema-syntax::format`) rather than the AST — so every comment, every
declaration, and their relative order stay exactly where the author put
them; it never reorders anything. Rewrites the file in place by default;
`--check` writes nothing and exits non-zero if the file isn't already
formatted, for a CI gate.

`--sort` (`skema-syntax::reorder`, composes with `--check`) is the one
operation that does reorder: it sorts a file's top-level declarations into
one canonical order — `spec`, `import`s, `Project`, `Document`, then
`Partial`/`Table`/`Enum`/`Datatype`/`Group` each alphabetized by name — and
drops every comment in the process, warning on stderr with the count when
it removes any; it refuses (leaving the file untouched) rather than guess
at anything it can't place unambiguously, e.g. an unnamed declaration.

All three of `check`/`export`/`diff` emit `skema-api`'s types
([`crates/skema-api/src/dto.rs`](crates/skema-api/src/dto.rs),
[`crates/skema-api/src/diff.rs`](crates/skema-api/src/diff.rs)), the same
ones the language server and the browser editor answer with. `--locations`
adds a `location` (file, line, 1-based UTF-16 column) to every entity in
`export`. Off by default: it roughly doubles the output and a consumer
reading only the structure never looks at it. Diagnostics always carry
theirs.

### Language server – `skema-lsp`

Stdio-based, for VSCode extensions and similar integrations. Serves live
diagnostics, hover, go-to-definition, completion, semantic tokens and the
document outline — all from `skema-api`, so an editor and the browser editor
answer identically.

It **elects the root file** rather than assuming one. Every workspace
`.skema` file declaring a `Project` block is a candidate, and a file belongs
to the candidate whose import graph reaches it (`docs/02`). A file no
candidate reaches is analyzed as its own root and the client is told so; it
then lacks `mode` and `dialect`, which is a property of the language, not a
limitation of the server.

An ER diagram needs two things LSP has no word for, served as custom
`skema/` messages ([`protocol.rs`](crates/skema-lsp/src/protocol.rs)):
`skema/model` (the resolved model plus `Meta` offsets and `Style` blocks) and
a `skema/modelChanged` notification. Where a dragged position is stored is
not one of them — that is a layout file beside the model, and the client's
business. They carry `skema-api`'s own types, so a client types them with the
`.d.ts` `skema-wasm` generates instead of a second, hand-written schema.

### Python bindings – `skema`

```python
import skema

result = skema.check("project.skema")
if not result["ok"]:
    for d in result["diagnostics"]:
        loc = d["location"]
        print(f"{loc['file']}:{loc['line']}:{loc['column']}: {d['severity']}: {d['message']}")

model = skema.export("project.skema")["model"]
for table in model["tables"]:
    print(table["name"], [f["name"] for f in table["fields"]])

# Adds a `location` to every entity, like `skema export --locations`.
located = skema.export("project.skema", with_locations=True)["model"]
```

Build with [`maturin`](https://www.maturin.rs/) from `crates/skema-py`
(`maturin develop` in an activated venv). Both functions always return a
`dict` and never raise: errors are data (`ok: False` plus `diagnostics`),
not control flow, as everywhere in the core. An unreadable root file is one
more diagnostic in that same list.

The `dict` holds `skema-api`'s structures, so it has the same keys as the
CLI's `--json`, the language server and the browser editor.

## Layout

```
crates/
  skema-syntax/   ✅ Lexer · parser · AST · spans · diagnostics
  skema-hir/      ✅ Name resolution, rule checking, resolved model
  skema-loader/   ✅ Loads a root file plus its import graph from disk, and analyzes it
  skema-api/      ✅ Editor-facing services: resolved model, Meta offsets, Style blocks, highlighting
  skema-cli/      ✅ Binary `skema`: check / export --json / diff / fmt
  skema-lsp/      ✅ Binary `skema-lsp`: language server (stdio)
  skema-py/       ✅ pyo3 bindings over skema-loader: check() / export()
  skema-wasm/     ✅ wasm-bindgen wrapper around skema-api
docs/src/                Language definition (mdBook, English)
examples/                Reference models – also the test corpus for skema-hir
```

Within `skema-syntax`:

| Module | Purpose |
|---|---|
| `span.rs` | Positions – byte offsets, nothing else |
| `token.rs` | The language's token kinds |
| `lexer.rs` | Characters → tokens, lossless and panic-free |
| `ast.rs` | The syntax tree's nodes |
| `parser.rs` | Cursor, error handling, recovery (*how* it is read) |
| `grammar.rs` | The language's rules (*what* is read) |
| `dump.rs` | Tree as text – for inspection |

```bash
cargo run -p skema-syntax --example dump_ast -- examples/myexample.skema
```

Within `skema-hir`:

| Module | Purpose |
|---|---|
| `symbols.rs` | File-local declaration collection |
| `namespace.rs` | Visible namespaces: own `Document` name + imports/aliases |
| `rules/placement.rs` | Data-driven check of which block may go where |
| `rules/keys.rs` | The "known key or `x-`" rule from `docs/01` |
| `rules/project.rs` | The single `Project` block, `mode`/`dialect` |
| `rules/group.rs` | `Group` member resolution, kind checking, uniqueness |
| `rules/resolve.rs` | Applies `use` partials, resolves types and relationships, builds the model |
| `rules/lint.rs` | Naming conventions and `unusedImport`, only under `mode = strict` |
| `model.rs` | The resolved model, exportable to JSON |

### The architecture rule

`skema-syntax` and `skema-hir` are **platform-neutral**: no `std::fs`, no
network, no `tokio`. The caller reads files and passes down `&str` or
already-parsed trees. That is what keeps the core compilable to WASM and
embeddable in the language server.

`skema-loader` is the one crate with both a filesystem and `skema-api`, so
it is where a project becomes an answer: `analyze` runs the checker and
returns `skema-api`'s model and diagnostics. All three disk-based front ends
go through it, which is why `skema check --json`, `skema.export()` and the
language server describe a model identically. For the browser, where there
is no filesystem, `skema-api` keeps its own in-memory counterpart
(`project.rs`).

`skema-wasm` and `skema-py` each get their **own** crate — mixing
`wasm-bindgen` and `pyo3` leads to feature-flag chaos.

`skema-api` is where the two editor front ends meet. An editor needs more
than "is this valid": the resolved model with a location per entity, the
exact offsets of a `Meta` entry, the `Style` cascade, a token stream to
colour. None of that belongs in `skema-hir`, which knows byte offsets and
nothing else, but all of it is needed twice. Written per front end, the two
would drift and a diagram drawn in VSCode would stop matching the browser's.
Its `typescript` feature (off by default) adds the `tsify`/`wasm-bindgen`
derives only the WASM boundary needs, so `skema-lsp` links the same types
without either dependency.

## Releasing

Releases go through the shared `skema-ci` component
(`.gitlab-ci.yml`'s `include:`), which cascades this repo's release down to
skema-lib and, from there, the two app repos. Design, branches, versioning
and job contracts: [brain/release-chain.md](../brain/release-chain.md).

**No Cargo registry.** Publishing the library crates to GitLab's Cargo
registry is not wired up: GitLab has not shipped it. The feature flag is not
rolled out on GitLab.com and `cargo publish` is still open on
[gitlab-org/gitlab#33060](https://gitlab.com/gitlab-org/gitlab/-/work_items/33060)
— a maturity gap, not a tier one. Binaries, wheel and WASM bundle go to the
generic package registry instead, which is what the downstream repos
consume. See `.gitlab-ci.yml` for what to restore when it ships.

## Project state

Roadmap, technical decisions and known gaps now live in the shared
[brain](../brain/README.md) — see
[brain/features.md](../brain/features.md) for what's built and planned,
[brain/decisions.md](../brain/decisions.md) for the "why" behind this crate
layout, and [brain/architecture.md](../brain/architecture.md) for open
problems, kept current across all four repos rather than in one README each.

