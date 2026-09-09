# skema

A DSL for describing entity-relationship models – with a parser, semantic
checking, a CLI, a language server, and Python bindings. The core is written
in Rust and is platform-neutral, so it can also be used from WASM (planned,
see [Roadmap](#roadmap)).

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
```

`check` loads the root file along with everything it reaches via `import`
("Project membership is reachability-defined", `docs/02`), and reports every
syntax and semantic error as `path:line:column: severity: text`. `export`
additionally outputs the resolved model – tables with `Partial`s applied,
resolved relationships, and cascaded physical names, see
[`crates/skema-hir/src/model.rs`](crates/skema-hir/src/model.rs).

### Language server – `skema-lsp`

Stdio-based, for VSCode extensions and similar editor integrations. Reports
syntax and semantic errors live while typing (`textDocument/didOpen` /
`didChange` / `didClose`, full document synchronization). If an editor opens
a file that is not the root of its project, it lacks `mode` and `dialect`
from the `Project` block – that is a property of the language itself
(`docs/02`), not a limitation of the server. Details and open points are in
the module documentation of
[`crates/skema-lsp/src/main.rs`](crates/skema-lsp/src/main.rs).

### Python bindings – `skema`

```python
import skema

result = skema.check("project.skema")
if not result["ok"]:
    for d in result["diagnostics"]:
        print(f"{d['path']}:{d['line']}:{d['column']}: {d['severity']}: {d['message']}")

model = skema.export("project.skema")["model"]
for table in model["tables"]:
    print(table["name"], [f["name"] for f in table["fields"]])
```

Build with [`maturin`](https://www.maturin.rs/) from `crates/skema-py`
(`maturin develop` in an activated venv). Both functions always return a
`dict`, never an exception for a faulty model – errors are data (`ok: False`
plus `diagnostics`), not control flow, the same stance as throughout the
core.

## Layout

```
crates/
  skema-syntax/   ✅ Lexer · parser · AST · spans · diagnostics
  skema-hir/      ✅ Name resolution, rule checking, resolved model
  skema-loader/   ✅ Loads a root file along with its import graph from the filesystem
  skema-api/      ✅ Editor-facing services: resolved model, Meta offsets, Style blocks, highlighting
  skema-cli/      ✅ Binary `skema`: check / export --json
  skema-lsp/      ✅ Binary `skema-lsp`: language server (stdio)
  skema-py/       ✅ pyo3 bindings: check() / export()
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
used by `skema-cli` and `skema-lsp`); only `&str` or already-parsed trees are
passed down. Only this way does the core stay compilable to WASM and
embeddable in the language server.

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
| 10 | **LSP features** – hover, go-to-definition, autocompletion | ⬜ |
| 11 | **Workspace-wide root file detection** in the LSP (instead of "open file = root") | ⬜ |
| 12 | **VSCode extension** – TS client + bundled binary | ⬜ |
| 13 | **WASM** – parser in the browser | ✅ |
| 14 | **Formatter** – requires a lossless syntax tree (`rowan`) | ⬜ |

## Technical decisions

| Decision | Why |
|---|---|
| Hand-written lexer + recursive-descent parser | Full control over spans and error recovery. Parser generators like `pest` cannot meaningfully recover from syntax errors – the normal case in an IDE. |
| Byte offsets (`u32`), no line/column | Halves the memory footprint per node; conversion to line/column (terminal) or UTF-16 positions (LSP) happens at exactly one place each: `skema-cli`/`skema-loader::report` and `skema-lsp::diagnostics`. |
| Only `spec`/`import`/`as`/`use` are reserved words | The block keywords (`Table`, `Group`, `Field`, …) are context-dependent, so that `Table Group { … }` remains writable – in an ERP model those are normal table names. |
| Permissive parser, strict checking phase | The parser accepts any nesting and always produces a tree; "`Lint` belongs in `Project`" is a clear message from `skema-hir` rather than a syntax error that drags down the rest of the file. |
| `(Ast, Vec<Diagnostic>)` instead of `Result` | An IDE needs *all* errors and *still* a tree. `skema-hir::analyze` carries the principle through: always a model, always a diagnostic list. |
| `skema-loader` as its own crate | `skema-cli` reads exclusively from disk, `skema-lsp` should use the unsaved editor content for open files. A swappable read function (`load_with`) instead of two copies of the same import-graph logic. |
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
- `skema-lsp` so far offers only diagnostics, no hover, no autocompletion,
  no go-to-definition, and treats every opened file as the project root
  instead of finding the real root file in the workspace.
- No lossless syntax tree. A formatter that preserves comments will later
  need `rowan` instead of the current AST.
