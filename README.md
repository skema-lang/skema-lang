# skema

A DSL for describing entity-relationship models – with a parser, semantic
checking, a CLI, a language server, and Python bindings. The core is written
in Rust and is platform-neutral, so the same frontend also runs in a browser
as WebAssembly.

Model files use the **`.skema`** extension; the language definition lives in
[`docs/`](docs/README.md), reference models in [`examples/`](examples/).

> `docs/` is the finished, final language reference (English) and is no
> longer changed by this project. This README describes the implementation
> built on top of it – crates, tools, build status.

## Getting started

```bash
direnv allow          # once – after that the toolchain is available automatically
# or without direnv:
devenv shell

check                 # fmt + clippy + tests
t                      # tests only
fix                    # fmt + clippy --fix
```

Without `direnv`, every command must be prefixed with `devenv shell -- `. For
fish users, the following hook is worth adding:

```fish
echo 'direnv hook fish | source' >> ~/.config/fish/config.fish
```

## The three tools

### CLI – `skema check` / `skema export`

```bash
cargo run -p skema-cli -- check examples/generic/project.skema
cargo run -p skema-cli -- check examples/generic/project.skema --json
cargo run -p skema-cli -- export --json examples/generic/project.skema > model.json
cargo run -p skema-cli -- export --json --locations examples/generic/project.skema
```

`check` loads the root file along with everything it reaches via `import`
("Project membership is reachability-defined", `docs/02`), and reports every
syntax and semantic error as `path:line:column: severity: text`. `export`
additionally writes the resolved model to stdout – tables with `Partial`s
applied, resolved relationships, and cascaded physical names – with messages
on stderr, so the redirect above yields nothing but the model.

Both emit `skema-api`'s types
([`crates/skema-api/src/dto.rs`](crates/skema-api/src/dto.rs)), the same ones
the language server and the browser editor answer with. `--locations` adds a
`location` (file, line, 1-based UTF-16 column) to every entity in the model;
it is off by default because it roughly doubles the output and a consumer
reading only the structure never looks at it. Diagnostics always carry
theirs.

### Language server – `skema-lsp`

Stdio-based, for VSCode extensions and similar editor integrations. Serves
live diagnostics, hover, go-to-definition, completion, semantic tokens and
the document outline — everything from `skema-api`, so an editor and the
browser editor answer the same questions the same way.

It **elects the root file** rather than assuming one: every `.skema` file in
the workspace that declares a `Project` block is a candidate, and a file
belongs to the candidate whose import graph reaches it (`docs/02`, "project
membership is reachability-defined"). A file no candidate reaches is
analyzed as its own root, and the client is told so — it then lacks `mode`
and `dialect`, which is a property of the language, not a limitation of the
server.

Three things a live ER diagram needs have no LSP equivalent and are served
as custom `skema/` messages ([`protocol.rs`](crates/skema-lsp/src/protocol.rs)):
`skema/model` (the resolved model plus `Meta` offsets and `Style` blocks),
`skema/positionEdit` (the smallest edit that stores a dragged position), and
a `skema/modelChanged` notification. They carry `skema-api`'s own types, so
a client can type them with the `.d.ts` that `skema-wasm` generates instead
of a second, hand-written schema.

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
`dict`, never an exception for a faulty model – errors are data (`ok: False`
plus `diagnostics`), not control flow, the same stance as throughout the
core. An unreadable root file is one diagnostic in that same list rather
than a separately shaped error.

The `dict` holds `skema-api`'s structures, so it has the same keys the CLI's
`--json`, the language server and the browser editor use.

## Layout

```
crates/
  skema-syntax/   ✅ Lexer · parser · AST · spans · diagnostics
  skema-hir/      ✅ Name resolution, rule checking, resolved model
  skema-loader/   ✅ Loads a root file plus its import graph from disk, and analyzes it
  skema-api/      ✅ Editor-facing services: resolved model, Meta offsets, Style blocks, highlighting
  skema-cli/      ✅ Binary `skema`: check / export --json
  skema-lsp/      ✅ Binary `skema-lsp`: language server (stdio)
  skema-py/       ✅ pyo3 bindings over skema-loader: check() / export()
  skema-wasm/     ✅ wasm-bindgen wrapper around skema-api
docs/                    Language definition (finished, English)
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
network, no `tokio`. Files are read by the calling side (`skema-loader`,
used by `skema-cli`, `skema-py` and `skema-lsp`); only `&str` or
already-parsed trees are passed down. Only this way does the core stay
compilable to WASM and embeddable in the language server.

`skema-loader` is the one crate that has both a filesystem and `skema-api`,
so it is where a project turns into an answer: `analyze` runs the checker
and returns `skema-api`'s model and diagnostics. All three disk-based front
ends go through it, which is what makes `skema check --json`,
`skema.export()` and the language server describe a model identically.
`skema-api` itself keeps its own in-memory counterpart (`project.rs`) for
the browser, where there is no filesystem to load from.

`skema-wasm` and `skema-py` each get their **own** crate. Mixing
`wasm-bindgen` and `pyo3` in the same crate leads to feature-flag chaos.

`skema-api` follows the same rule and is where the two editor front ends
meet. An editor needs more than "is this valid": the resolved model with a
location per entity, the exact offsets of a `Meta` entry, the `Style`
cascade, a token stream to colour. None of that is `skema-hir`'s business –
it knows byte offsets "and nothing else" – but all of it is needed twice,
by `skema-wasm` in a browser and by `skema-lsp` in an editor. Written once
per front end, the two would drift, and a diagram drawn in VSCode would
slowly stop matching the one drawn in the browser. Its `typescript` feature
(off by default) adds the `tsify`/`wasm-bindgen` derives that only the WASM
boundary needs, so `skema-lsp` links the same types without either
dependency.

## Roadmap

| # | Step | Status |
|---|---------|--------|
| 1 | Toolchain, workspace, spans, diagnostics | ✅ |
| 2 | **Lexer** – `&str` → `Vec<Token>`, lossless, panic-free | ✅ |
| 3 | **Parser** – recursive descent with error recovery → AST + diagnostics | ✅ |
| 4 | **Rule checking** – name resolution, `docs/02`–`docs/13` | ✅ |
| 5 | **CLI** `skema check` / `skema export --json` | ✅ |
| 6 | **LSP scaffolding** – document management, live diagnostics | ✅ |
| 7 | **Python bindings** – `check()` / `export()` | ✅ |
| 8 | **JSON export** – serde serialization of the model | ✅ |
| 9 | **Imports** – module graph, cycle detection | ✅ |
| 10 | **LSP features** – hover, go-to-definition, autocompletion | ✅ |
| 11 | **Workspace-wide root file detection** in the LSP (instead of "open file = root") | ✅ |
| 12 | **VSCode extension** – TS client + bundled binary | ✅ ([skema-vscode](https://gitlab.com/skema-language/skema-vscode)) |
| 13 | **WASM** – parser in the browser | ✅ |
| 14 | **Formatter** – requires a lossless syntax tree (`rowan`) | ⬜ |

## Releasing

A release is a chain, and the only thing that starts it is a green default
branch:

1. `release` (default branch) works out the next version from the commits
   since the last tag, writes it into `Cargo.toml`/`Cargo.lock`, and pushes
   `vX.Y.Z`. It builds nothing.
2. The tag pipeline publishes: binaries and the Python wheel to the Package
   Registry, the library crates to the Cargo registry, the container image,
   and — for the two downstream repos — the per-platform `skema-lsp` binaries
   and the browser WASM bundle.
3. It then triggers [skema-vscode](https://gitlab.com/skema-language/skema-vscode)
   and [skema-editor](https://gitlab.com/skema-language/skema-editor), which
   fetch *that tag's* artifacts, commit them, and tag themselves. Both are
   `strategy: depend`, so a downstream failure shows up here.

Pushing a tag by hand does the same thing from step 2 on; the automation adds
a trigger, not a second release path.

**How the version is picked.** Conventional-commit markers when the history
has them, `patch` when it does not — this repo's log is not written that way
today, so the honest default is the smallest step. `feat:` earns a minor, a
`!` or a `BREAKING CHANGE:` trailer earns a major. Pre-1.0 that major is
capped at a minor: reaching 1.0 should be a decision someone makes, not
something a `!` does by accident.

```bash
scripts/release.sh --dry-run     # what would happen, touching nothing
scripts/release-test.sh          # the bump logic's own tests
```

Override the derivation by running the pipeline manually with `BUMP` set to
`major`/`minor`/`patch`, or `RELEASE_VERSION=1.0.0` to name one outright.

**What CI needs.** A `RELEASE_TOKEN` (masked and protected) in each of the
three projects — `CI_JOB_TOKEN` cannot push. Use a personal access token with
the `write_repository` scope, from an account allowed to push to the protected
default branch. Not a *project* access token: those are Premium on GitLab.com
for projects that live in a group, and these three do. A deploy key with write
access is the person-independent alternative, at the cost of SSH setup in the
job. Both downstream projects must also allow this one to trigger them, under
Settings → CI/CD → Job token permissions.

Everything in the chain is Free-tier: multi-project triggering left Premium in
12.8, and only the pipeline *graph* that draws it is paid.

**No Cargo registry.** Publishing the library crates to GitLab's Cargo
registry is not wired up, because GitLab has not shipped it — the feature flag
is not rolled out on GitLab.com and the `cargo publish` flow is still open on
[gitlab-org/gitlab#33060](https://gitlab.com/gitlab-org/gitlab/-/work_items/33060).
That is a maturity gap, not a tier one. The binaries, the wheel and the WASM
bundle all go to the generic package registry instead, which is what the
downstream repos consume; see the note in `.gitlab-ci.yml` for what to restore
when it does ship.

## Technical decisions

| Decision | Why |
|---|---|
| Hand-written lexer + recursive-descent parser | Full control over spans and error recovery. Parser generators like `pest` cannot meaningfully recover from syntax errors – the normal case in an IDE. |
| Byte offsets (`u32`), no line/column | Halves the memory footprint per node. Conversion happens in exactly one place, `skema-api::location`, and counts UTF-16 code units – what JavaScript and the LSP protocol both measure in. `skema-lsp` subtracts 1 for its 0-based positions rather than counting again. |
| Only `spec`/`import`/`as`/`use` are reserved words | The block keywords (`Table`, `Group`, `Field`, …) are context-dependent, so that `Table Group { … }` remains writable – in an ERP model those are normal table names. |
| Permissive parser, strict checking phase | The parser accepts any nesting and always produces a tree; "`Lint` belongs in `Project`" is a clear message from `skema-hir` rather than a syntax error that drags down the rest of the file. |
| `(Ast, Vec<Diagnostic>)` instead of `Result` | An IDE needs *all* errors and *still* a tree. `skema-hir::analyze` carries the principle through: always a model, always a diagnostic list. |
| `skema-loader` as its own crate | `skema-cli` and `skema-py` read exclusively from disk, `skema-lsp` should use the unsaved editor content for open files. A swappable read function (`load_with`) instead of three copies of the same import-graph logic – and one `analyze` instead of three copies of "run the checker, then pair every `FileId` back with its path". |
| `lsp-server` instead of `tower-lsp` | Synchronous, without `tokio`. Fewer Rust concepts at once, and it's the library behind rust-analyzer. |
| `pyo3` with `abi3-py39` | A built extension runs on Python 3.9 and up, without needing to recompile per Python version. |
| devenv instead of rustup | On NixOS, `nix profile` doesn't provide `clippy`/`rustfmt`/`rust-analyzer`; devenv pins the entire toolchain per-project in `devenv.lock`. |

## Known gaps

**Language design** – questions not yet finally settled are listed in
[`docs/13`, "Known limitations"](docs/13-reference.md#known-limitations):
composite primary keys, naming scheme for unnamed indexes, index name
collisions across shared `Partial`s, the full `Style`/`Dialect` property
catalog, per-rule `Lint` severity overrides. `skema-hir` deliberately checks
only the already-settled part there, so as not to enforce a rule the
language itself hasn't decided on yet.

**Implementation** – what goes beyond the current state:

- For `Datatype` arguments, `skema-hir` only checks the **count**, not the
  type of each individual argument (`decimal("abc", 2)` is not detected as a
  type error).
- The built-in type catalog for `dialect = postgres/mysql/oracledb` is
  deliberately not maintained (`docs/13`) – unknown type names under these
  dialects produce only a warning, not an error.
- `skema-lsp`'s go-to-definition resolves a name by the file's `Document`
  name and its `import … as` aliases rather than through `skema-hir`'s own
  name resolution. A wrong guess costs a jump that does not happen, never a
  wrong diagnostic - but it is a second, simpler answer to a question the
  checker already answers properly.
- The root election re-elects on save, not on every keystroke: an `import`
  typed but not yet saved does not move a file into a project.
- No lossless syntax tree. A formatter that preserves comments will later
  need `rowan` instead of the current AST.
