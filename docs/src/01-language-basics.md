# 01 — Language Basics

[← Index](README.md)

File extension: **`.skema`**.

## File anatomy

A file is read top to bottom. Only the first statement is position-fixed; everything else is order-independent.

```skema
spec 1;                          # language version — first statement, always
import "./common.skema" as base; # imports — any number, any order

Project { }                      # project-wide settings — exactly one per project
Document shop { }                # this file's namespace — exactly one per file

# declarations follow, in any order:
Partial  common_base { }
Enum     OrderStatus { }
Table    Customer    { }
Group    Shop        { }
Datatype Money       { }
```

`Project` and `Document` are covered in [02 — Project & Documents](02-project-and-documents.md).

## Three statement forms

Every line in skema is one of three forms.

| Form | Shape | Examples |
|---|---|---|
| Declaration | `Name Type [Flags] { Body }` | `Table Order { … }` · `email varchar [required] { … }` |
| Assignment | `key = value;` | `mode = draft;` · `note = "…";` |
| Directive | `lowercaseWord …;` | `spec 1;` · `import "…" as base;` · `use common_base;` |

That's the entire grammar surface. Any part of a declaration may be omitted; the **order** of the parts never changes.

## The one declaration shape

```skema
username  regexstring("^[a-z0-9_]{3,20}$")  [required]  { note = "Login name"; }
          └──────────── Type ─────────────┘  └─ Flags ─┘  └────── Body ──────┘
```

| Example | Name | Type | Flags | Body |
|---|---|---|---|---|
| `Table Order { … }` | `Order` | — | — | ✓ |
| `email varchar [required] { … }` | `email` | `varchar` | ✓ | ✓ |
| `customer >Customer.id [onDelete=cascade];` | `customer` | `>Customer.id` | ✓ | — |
| `ix_email (email) [unique];` | `ix_email` | `(email)` | ✓ | — |
| `Style @table { … }` | `@table` | — | — | ✓ |
| `Datatype decimal(p, s) { … }` | `decimal(p, s)` | — | — | ✓ |
| `draft { label = "Draft"; }` | `draft` | — | — | ✓ |

Once you recognize this shape, you can read any line in the language, including ones you've never seen before.

## Brackets and braces

This is the central convention of the language:

| | `[ … ]` | `{ … }` |
|---|---|---|
| Content | flags, comma-separated | key/value pairs, `;`-terminated |
| Effect | **changes the generated schema** | **does not change the schema** |
| Example | `[pk, required, default=0]` | `{ note = "…"; exampleValue = "…"; }` |

Rule of thumb: **square brackets generate DDL, curly braces generate documentation.**

## Statement termination

- A statement that ends in a **value** ends with `;`: `mode = draft;` · `gross_sum decimal(10, 2);` · `cancelled;`
- A statement that ends in a **block** `}` does **not** take a trailing `;`: `sum float [required] { … }` · `draft { label = "Draft"; }`

## Assignment operator

A value is assigned with `=` everywhere in the language — inside `{}` blocks and inside `[]` flag lists alike:

```skema
fontSize = "2em";
created_at timestamp [default=`now()`];
```

`:` is not an assignment operator in skema.

## Comments

```skema
# line comment
// line comment (equivalent)

/* block comment,
   spans multiple lines */
```

`#` and `//` are equivalent. Block comments **nest**: `/* /* … */ */` is valid, and only the matching closing marker ends the outer comment.

`#` inside a string is not a comment:

```skema
Style @table { headerBackgroundColor = "#cc0000"; }
```

There is no doc-comment syntax (`///`). Descriptions belong in `note = "…"` inside a `Meta` block or field body — see [03 — Meta & Style](03-meta-and-style.md).

## Strings and escapes

Strings use double quotes and cannot span multiple lines.

| Sequence | Meaning |
|---|---|
| `\"` | quote |
| `\\` | backslash |
| `\n` | newline |
| `\t` | tab |

```skema
note = "First line\nSecond line with \"quote\"";
```

Backslashes must be doubled in regexes:

```skema
regex = "^\\d{4}-\\d{2}$";   # matches ^\d{4}-\d{2}$
```

## Raw expressions

Content between backticks is passed through unmodified — not parsed, not validated, not escaped:

```skema
created_at timestamp [default=`now()`];
```

Backticks are for **SQL expressions**, not a general raw-string type. A raw expression is dialect-specific: `` `now()` `` may not translate across database engines.

## Numbers and identifiers

- Numbers: `12`, `10`, `3.14`.
- Identifiers start with a letter or `_`, then letters, digits, or `_`.
- Qualified paths are dot-separated: `shop.OrderStatus.paid`.

Field, table, and enum names belong to the project — the language itself imposes no casing style on them. `created_at`, `createdAt`, and `CreatedAt` are all valid; a project that wants consistency configures it explicitly — see [04 — Linting](04-linting.md).

## Case conventions and keywords

Three keyword classes, distinguished by casing:

| Class | Casing | Examples |
|---|---|---|
| Directive | lowercase | `spec`, `import`, `as`, `use` |
| Block keyword | PascalCase | `Project`, `Document`, `Table`, `Enum`, `Partial`, `Group`, `Datatype`, `Meta`, `Style`, `Index`, `Database`, `Dialect`, `Lint` |
| Setting key / flag | camelCase | `styleClasses`, `onDelete`, `exampleValue` |

Casing tells you what a line does at a glance: lowercase opens a directive, PascalCase opens a block.

Within the block class, two sub-groups exist:

- **Declarations** — carry a name: `Table Customer`, `Enum OrderStatus`.
- **Settings blocks** — carry no name: `Meta`, `Index`, `Database`, `Dialect`, and are only valid inside specific parent blocks.

## The at sign convention

A name with `@` belongs to the language; a name without it belongs to the user:

```skema
Style @table    { … }   # built-in element type
Style highlight { … }   # user-defined class
Dialect @postgres { … } # built-in engine identifier
```

See [03 — Meta & Style](03-meta-and-style.md) for the full selector list.

## Custom keys

Anywhere skema accepts key/value pairs, the same rule applies:

- **Known keys** are validated and autocompleted.
- **Custom keys** must start with `x-` and be quoted. Where listed [below](#custom-keys-in-the-export), they reach the JSON export.
- **Anything else is an error** — this is what catches a typo that would otherwise fail silently.

```skema
Meta {
  author    = "anna";        # known
  "x-jira"  = "SHOP-412";    # custom, passed through
  athor     = "anna";        # ERROR — typo, not a known key, missing x-
}
```

This rule applies in `Meta`, in `Style`, and in field attribute blocks.

### Custom keys in the export

| Where the key is written | Where `skema export --json` carries it |
|---|---|
| `Meta` of a `Table` | the table's `extensions` |
| `Meta` of a `Group` | the group's `extensions` |
| `Meta` of a `Document` | the `extensions` of that file's entry in `documents` |
| `Meta` and `Field { }` block of an `Enum` | the enum's `extensions`; for a key in both, the `Field` block wins |
| an enum entry's body | the entry's `extensions` |
| a field's body, also inside a `Partial` | the field's `extensions` |
| `[]` flags of a field or an index | its `flags`, next to the known flags |

`extensions` maps each key to its value as text, the same way flags do: a string without its quotes, a raw expression without its backticks, a list comma-joined, `true` as `"true"`. Within one block, the first occurrence of a key wins. An entity without custom keys has `extensions: {}`.

Everywhere else a custom key is validated but not exported — for example in the `Meta` of a `Project` or a `Partial`, in a `Datatype` parameter's body, and in `Style`, `Dialect`, `Database` and `Lint` blocks.

## spec vs Meta.version

Two different things, two different names:

| | Describes | Set by | Where |
|---|---|---|---|
| `spec 1;` | which language revision the file follows | the language | file header, first statement |
| `version = "1.4.0";` | the model's own revision | the project | inside a `Meta` block |

`spec` is a plain integer — there are only whole language revisions, no semantic versioning. `version` is a free string: `"1.4.0"`, `"2026-Q1"`, whatever fits the project's workflow.

## File extension

`.skema` is the only recognized extension for model files.

## See also

- [02 — Project & Documents](02-project-and-documents.md) — file-level and project-level setup
- [13 — Reference](13-reference.md) — full keyword and flag tables
