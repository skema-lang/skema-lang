# Agent guide — skema (the language repo)

Rust workspace implementing the `.skema` DSL. Read [README.md](README.md)
for the full picture; this file is the short list of rules to work by.

Cross-repo architecture, open tasks and decisions live in the shared
[brain](../brain/README.md) — read it before non-trivial work, update it
before you finish.

## Commands

Run inside `devenv shell` (or with `direnv allow` active):

```bash
check        # fmt + clippy -D warnings + tests   ← run before finishing
t            # tests only
fix          # fmt + clippy --fix
wasm-test    # skema-wasm's tests under wasm32, in Node
```

`cargo fmt`/`cargo clippy` are not on the bare PATH. Use `devenv shell -- …`.

## Crate map

| Crate | Role | Platform-neutral |
|---|---|---|
| `skema-syntax` | lexer, parser, AST, spans | yes |
| `skema-hir` | name resolution, rules, resolved model | yes |
| `skema-api` | editor-facing services: DTOs, locations, `Meta`, `Style`, highlighting | yes |
| `skema-loader` | disk: import graph + `analyze` | no |
| `skema-cli` | binary `skema` | no |
| `skema-lsp` | binary `skema-lsp` (stdio) | no |
| `skema-py` | pyo3 bindings | no |
| `skema-wasm` | wasm-bindgen wrapper over `skema-api` | no |

## Hard rules

1. **Platform neutrality.** `skema-syntax`, `skema-hir` and `skema-api` use
   no `std::fs`, no network, no `tokio`. Callers read files and pass `&str`
   or parsed trees. Breaking this breaks the WASM and LSP builds.
2. **Never panic.** `unwrap_used`, `expect_used` and `panic` are warnings in
   `Cargo.toml`, allowed only in tests (`clippy.toml`). A parser receives
   broken input by definition.
3. **Errors are data.** Nothing here returns `Result` for an invalid model.
   `parse`, `analyze`, `check` and `export` always return a result plus a
   diagnostic list.
4. **Parse permissively, validate strictly.** The parser accepts any
   nesting and always produces a tree. Placement and key rules belong to
   `skema-hir`.
5. **Byte offsets in the core.** `skema_syntax::Span` is UTF-8 byte offsets.
   `skema-api::location` is the only place that converts to line/column or
   UTF-16, and every front end goes through it.
6. **One catalog.** Keyword, key, flag and type lists live in
   `skema_hir::catalog` and reach the front ends via `skema_api::builtins`.
   Never hand-copy a list into TypeScript or a second Rust constant.

## Documentation

- `docs/src/` is the language reference (mdBook) and the source of truth for
  language questions. Treat it as stable; do not change language rules there
  as a side effect of an implementation change.
- Cite it by **heading**, not section number: `` `docs/01`, "Custom keys" ``.
  A `1.11`-style citation in an older comment predates `docs/src/`.
- Open language questions live in `docs/src/13-reference.md`, "Known
  limitations". `skema-hir` deliberately does not enforce anything listed
  there.
- Rules the language *has* decided but `skema-hir` does not yet check belong
  in the same file under "Not enforced yet". Closing one means moving a row
  out of that table, not quietly leaving it.

## Comments

- As few as possible. Prefer names and structure.
- Explain **why**, not what. Delete a comment that restates the code.
- Module docs carry the architectural reason for the module's existence,
  short. Keep them out of tutorial territory.
- Delete a comment the moment it stops being true.
