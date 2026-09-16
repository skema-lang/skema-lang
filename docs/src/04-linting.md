# 04 — Linting

[← Index](README.md)

`Lint` makes naming and modeling conventions explicit and versions them with the model, instead of leaving them as unwritten tool defaults.

```skema
Project {
  mode = strict;

  Lint {
    fieldNaming = snake_case;
    tableNaming = PascalCase;
    enumNaming  = PascalCase;

    unusedImport = warn;
    danglingRef  = error;
  }
}
```

## Placement

`Lint` is valid **only** inside `Project`. There are no per-file, per-table, or per-line exceptions — a rule applies to the whole project or not at all. Reading one block tells you everything that's enforced; there is no second place where a rule is quietly turned off.

## Effect is gated by mode

`Lint` rules are enforced only when `mode = strict`. Under `mode = draft` they are ignored (see [02 — The mode switch](02-project-and-documents.md#the-mode-switch)). They may still be written under `draft` — they describe what will hold once the project switches.

## Naming rules

| Rule | Governs |
|---|---|
| `fieldNaming` | field names |
| `tableNaming` | table names |
| `enumNaming` | enum names |

Accepted values: `snake_case`, `camelCase`, `PascalCase`, `SCREAMING_SNAKE_CASE`, `any`.

Naming style itself is a project decision, not a language rule — skema accepts `created_at`, `createdAt`, and `CreatedAt` equally (see [01 — Language Basics](01-language-basics.md#numbers-and-identifiers)). `Lint` is where a project commits to one style.

A violation is a `warn` by default. Append `[warn]` or `[error]` after the value to override it — the only place this bracket is legal:

```skema
fieldNaming = snake_case [error];
```

## Other rules

| Rule | Governs |
|---|---|
| `danglingRef` | a reference (for example, in a `Group`) pointing at a nonexistent declaration |
| `unusedImport` | an `import` whose namespace is never referenced |
| `requirePrimaryKey` | a `Table` with no `pk` field |
| `requireForeignKeyIndex` | a foreign key column (`>`/`-`) with no covering `[unique]`/`[pk]` or leading `Index` entry |
| `requireTableNote` | a `Table` with no `Meta.note` |
| `requireFieldNote` | a `Table`/`Partial` field with no `note` |

These rules already choose their severity through their value (`warn` or `error`) — the `[warn]`/`[error]` override above is not accepted here.

## See also

- [02 — Project & Documents](02-project-and-documents.md) — where `Project` and `mode` live
- [13 — Reference](13-reference.md) — full keyword reference
