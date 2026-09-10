# skema

A DSL for entity-relationship models: parser, semantic checking, CLI,
language server, Python bindings. The core is Rust and platform-neutral, so
the same frontend runs in a browser as WebAssembly.

Model files use the **`.skema`** extension. The language definition lives in
[`docs/src/`](docs/src/README.md), reference models in
[`examples/`](examples/).

> `docs/` is the finished language reference (English). This README covers
> the implementation on top of it — crates, tools, build status.

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

### CLI – `skema check` / `skema export`

```bash
cargo run -p skema-cli -- check examples/generic/project.skema
cargo run -p skema-cli -- check examples/generic/project.skema --json
cargo run -p skema-cli -- export --json examples/generic/project.skema > model.json
cargo run -p skema-cli -- export --json --locations examples/generic/project.skema
```

`check` loads the root file plus everything it reaches via `import` (project
membership is reachability-defined, `docs/02`) and reports every syntax and
semantic error as `path:line:column: severity: text`. `export` also writes
the resolved model to stdout — `Partial`s applied, relationships resolved,
physical names cascaded — with messages on stderr, so the redirect above
yields the model alone.

Both emit `skema-api`'s types
([`crates/skema-api/src/dto.rs`](crates/skema-api/src/dto.rs)), the same ones
the language server and the browser editor answer with. `--locations` adds a
`location` (file, line, 1-based UTF-16 column) to every entity. Off by
default: it roughly doubles the output and a consumer reading only the
structure never looks at it. Diagnostics always carry theirs.

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

An ER diagram needs three things LSP has no word for, served as custom
`skema/` messages ([`protocol.rs`](crates/skema-lsp/src/protocol.rs)):
`skema/model` (the resolved model plus `Meta` offsets and `Style` blocks),
`skema/positionEdit` (the smallest edit that stores a dragged position) and
a `skema/modelChanged` notification. They carry `skema-api`'s own types, so
a client types them with the `.d.ts` `skema-wasm` generates instead of a
second, hand-written schema.

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
  skema-cli/      ✅ Binary `skema`: check / export --json
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
| 10 | **LSP features** – hover, go-to-definition, completion, semantic tokens, outline | ✅ |
| 11 | **Workspace-wide root file detection** in the LSP (instead of "open file = root") | ✅ |
| 12 | **VSCode extension** – TS client + bundled binary | ✅ ([skema-vscode](https://gitlab.com/skema-language/skema-vscode)) |
| 13 | **WASM** – parser in the browser | ✅ |
| 14 | **Formatter** – requires a lossless syntax tree (`rowan`) | ⬜ |

## Releasing

A release is a chain. Only a green default branch starts it:

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

Pushing a tag by hand does the same from step 2 on; the automation adds a
trigger, not a second release path.

**How the version is picked.** Conventional-commit markers where the history
has them, `patch` where it does not — this repo's log is not written that
way today, so the default is the smallest step. `feat:` earns a minor; `!`
or a `BREAKING CHANGE:` trailer earns a major. Pre-1.0 that major is capped
at a minor: reaching 1.0 should be a decision, not an accident.

```bash
scripts/release.sh --dry-run     # what would happen, touching nothing
scripts/release-test.sh          # the bump logic's own tests
```

Override the derivation by running the pipeline manually with `BUMP` set to
`major`/`minor`/`patch`, or `RELEASE_VERSION=1.0.0` to name one outright.

**What CI needs**, in each of the three projects — no personal or project
access token anywhere:

- Settings → CI/CD → Job token permissions → **Allow Git push requests to the
  repository**. Off by default; GA in GitLab 18.4, every tier. `CI_JOB_TOKEN`
  then does all the pushing.
- A `RELEASE_TRIGGER_TOKEN` variable (masked, protected) holding a pipeline
  trigger token. A job-token push starts no pipeline — which is what stops
  the release job re-triggering itself — so the publishing pipeline has to
  be requested explicitly. A trigger token can only start a pipeline, so it
  is far smaller to leak than one with repository write access.
- Both downstream projects must allow this one to trigger them, under
  Settings → CI/CD → Job token permissions.

Everything in the chain is Free-tier: multi-project triggering left Premium in
12.8, and only the pipeline *graph* that draws it is paid.

**No Cargo registry.** Publishing the library crates to GitLab's Cargo
registry is not wired up: GitLab has not shipped it. The feature flag is not
rolled out on GitLab.com and `cargo publish` is still open on
[gitlab-org/gitlab#33060](https://gitlab.com/gitlab-org/gitlab/-/work_items/33060)
— a maturity gap, not a tier one. Binaries, wheel and WASM bundle go to the
generic package registry instead, which is what the downstream repos
consume. See `.gitlab-ci.yml` for what to restore when it ships.

## Technical decisions

| Decision | Why |
|---|---|
| Hand-written lexer + recursive-descent parser | Full control over spans and error recovery. Generators like `pest` cannot meaningfully recover from syntax errors — the normal case in an IDE. |
| Byte offsets (`u32`), no line/column | Halves the footprint per node. Exactly one place converts, `skema-api::location`, and it counts UTF-16 code units — what JavaScript and LSP both measure in. `skema-lsp` subtracts 1 for its 0-based positions instead of recounting. |
| Only `spec`/`import`/`as`/`use` reserved | Block keywords (`Table`, `Group`, `Field`, …) are contextual, so `Table Group { … }` stays writable — in an ERP model those are normal table names. |
| Permissive parser, strict checking phase | The parser accepts any nesting and always produces a tree. "`Lint` belongs in `Project`" is then a clear message from `skema-hir`, not a syntax error that drags down the file. |
| `(Ast, Vec<Diagnostic>)` instead of `Result` | An IDE needs *all* errors and *still* a tree. `skema-hir::analyze` follows through: always a model, always a diagnostic list. |
| `skema-loader` as its own crate | The CLI and Python read only from disk, `skema-lsp` prefers the unsaved editor buffer. One swappable read function (`load_with`) instead of three copies of the import-graph walk, and one `analyze` instead of three copies of "run the checker, then map every `FileId` back to a path". |
| `lsp-server` instead of `tower-lsp` | Synchronous, no `tokio`. Fewer concepts at once, and it is the library behind rust-analyzer. |
| `pyo3` with `abi3-py39` | One built extension runs on Python 3.9 and up, with no recompile per version. |
| devenv instead of rustup | On NixOS, `nix profile` provides no `clippy`/`rustfmt`/`rust-analyzer`; devenv pins the whole toolchain per project in `devenv.lock`. |

## Known gaps

**Language design** – open questions are listed in
[`docs/13`, "Known limitations"](docs/src/13-reference.md#known-limitations):
composite primary keys, naming scheme for unnamed indexes, index name
collisions across shared `Partial`s, the full `Style`/`Dialect` property
catalog, per-rule `Lint` severity overrides. `skema-hir` checks only the
settled part, so it never enforces a rule the language has not decided.

**Implementation** – what goes beyond the current state:

- For `Datatype` arguments, `skema-hir` checks the **count** only, not each
  argument's type (`decimal("abc", 2)` is not caught).
- The built-in type catalog for `dialect = postgres/mysql/oracledb` is not
  maintained (`docs/13`). An unknown type name under those dialects is a
  warning, not an error.
- `skema-lsp`'s go-to-definition resolves a name from the file's `Document`
  name and `import … as` aliases, not through `skema-hir`'s name
  resolution. A wrong guess costs a jump that does not happen, never a wrong
  diagnostic — but it is a second answer to a question the checker already
  answers properly.
- The root election re-elects on save, not per keystroke: an `import` typed
  but unsaved does not move a file into a project.
- No lossless syntax tree. A comment-preserving formatter will need `rowan`
  instead of the current AST.
