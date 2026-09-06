# 01 — Lexical Structure and File Layout

> Status: **Draft.** Describes the current state as seen in `examples/myexample.skema`.
> Sections marked ⚠️ are not yet decided — see `10-open-design-questions.md`.

## 1.1 What is a `.skema` file?

A file describes a **model**: tables, their fields, their relationships
to one another, and metadata for presentation and code generation. It is
declarative and order-independent — except for the file header.

The rough layout, top to bottom:

```
spec 1;                          # language version  (exactly once, first)
import "./common.erp" as base;   # imports        (any number)

Project { … }                    # project-wide settings
Document shop { … }              # document identity
                                 # after that: declarations in any order
Partial  common_base { … }
Enum     OrderStatus  { … }
Table    Customer     { … }
Group    Shop         { … }
Datatype decimal(…)   { … }
```

## 1.2 Comments

There are three notations:

```
# line comment
// line comment

/* block comment,
   spanning multiple lines */
```

`#` and `//` are equivalent. `#` is the notation used in the existing
examples and fits the configuration-file character of the language; `//`
comes from C-like languages and is the most common muscle-memory habit.

The block comment `/* … */` is meant for temporarily disabling whole
declarations.

**Block comments nest.** `/* /* … */ */` is valid; only the matching
closing marker ends the comment. Without nesting, commenting out a region
that already contains a comment would silently end at the wrong place —
exactly when it's least likely to be noticed. Rust does it this way, C
does not.

### `#` inside strings

Inside a string, `#` does **not** start a comment. This isn't an edge
case — it's the normal case in this language:

```
Style @table { headerBackgroundColor = "#cc0000"; }
```

### No doc comment

There is deliberately no `///`. Descriptions belong in `note = "…"` and
thus in the AST — from there they reach hover text in the editor, the
JSON export, and comments in the generated DDL. A doc comment would be a
second path for the same thing, and the worse one: lexer trivia can't be
processed in a structured way.

> Note: three comment forms mean that models from different authors will
> look different. A formatter could normalize this to one form — whether
> it should is still open.

## 1.3 Token kinds

| Kind | Example | Meaning |
|---|---|---|
| Identifier | `Customer`, `created_at`, `draft` | names and enum values |
| Qualified name | `shop.OrderStatus.paid` | dot-separated path |
| String | `"#cc0000"` | text value |
| Number | `12`, `10` | numeric value |
| Raw expression | `` `now()` `` | passed to the target database unchanged |
| List | `[highlight]` | inside `{}` blocks: a value list |
| Flag list | `[pk, required]` | after a field: semantic flags |

### Strings and escape sequences

Strings are written in double quotes and support the usual escape
sequences:

| Sequence | Meaning |
|---|---|
| `\"` | quote |
| `\\` | backslash |
| `\n` | newline |
| `\t` | tab |

```
note = "First line\nSecond line with \"quote\"";
```

**Consequence for regexes:** a backslash must be doubled.

```
regex = "^\\d{4}-\\d{2}$";     # matches  ^\d{4}-\d{2}$
```

This is the familiar rule from JSON and most languages — and at the same
time the most common source of error when writing regexes. The editor
should warn here when an unknown sequence like `\s` shows up.

### Backticks = foreign language

Everything between backticks is **opaque** to erplang. It is not parsed,
not validated, not escaped — only passed through:

```
created_at timestamp [default=`now()`];
```

Backticks are meant for **SQL expressions**, not as a general raw-string
type. That also makes their cost clear: they are dialect-specific. A
model containing `` `now()` `` cannot be readily ported to Oracle.

## 1.4 Terminating statements ⚠️

Observed rule in the example:

- A statement that ends with a **value** ends with `;`
  → `mode = draft;` · `gross_sum decimal(10, 2);` · `cancelled;`
- A statement that ends with a **block** `}` ends **without** `;`
  → `sum float [required] { … }` · `draft { label="Draft" }`

This rule is applied consistently throughout the example and should be
documented and enforced as such. (The parser can tolerate `;` after `}`;
the formatter removes it.)

## 1.5 Assignments: always `=`

A value is assigned with `=` **everywhere** — in `{}` blocks just as in
`[]` flag lists:

```
fontSize = "2em";
created_at timestamp [default=`now()`];
```

`:` is not an assignment character in the language. (`description.txt`
still uses it in a few places — outdated.)

Rationale: switching the assignment character depending on the bracket
type would be a rule with no benefit. `=` also reads as "is", `:` as "has
type" — and in this language, types stand behind the field name without a
colon.

## 1.6 Case conventions for keywords

There are two classes, and the casing reveals the class:

| Class | Casing | Examples |
|---|---|---|
| Directive / statement | `lowercase` | `spec`, `import`, `as`, `use` |
| Block | `PascalCase` | `Project`, `Document`, `Table`, `Enum`, `Partial`, `Group`, `Datatype`, `Meta`, `Style`, `Index`, `Database`, `File`, `Dialect` |

This is a good rule — it makes it immediately visible when reading
whether a line *opens* something or *does* something. It should be
fixed as a binding rule for all future keywords.

Within the block class there are effectively two subgroups that differ
syntactically:

- **Declarations** — carry a name: `Table Customer`, `Enum OrderStatus`
- **Settings blocks** — carry no name: `Meta`, `Index`, `Database`,
  `File`, `Dialect`, and are only allowed within certain parents

## 1.7 `[]` vs `{}` — the central dividing line

This is the most important convention of the language:

| | `[ … ]` | `{ … }` |
|---|---|---|
| Content | flags, comma-separated | key-value pairs, `;`-separated |
| Effect | **changes the generated schema** | **does not change the schema** |
| Example | `[pk, required, default=0]` | `{ note = "…"; exampleValue = "…" }` |

Rule of thumb: *Square brackets generate DDL. Curly braces generate
documentation.*

⚠️ In the example this line is crossed in one place: `regex = "…"` sits
in the `{}` block, but it's an assertion about valid values — see
`04-tables-and-fields.md`.

## 1.8 Names and naming conventions

### Valid identifiers

An identifier starts with a letter or `_` and thereafter contains
letters, digits, and `_`.

### Conventions are a project matter, not a language matter

Field names belong to the user. The language prescribes **no** style —
`created_at`, `createdAt`, and `CreatedAt` are all valid.

Anyone who wants consistency configures it in the `Project` block via the
`Lint` block. This makes the convention a deliberate project decision,
documented within the model itself, instead of an invisible rule baked
into the tool. Details: `02-project-document-namespaces.md`.

> Note on the current example: `examples/myexample.skema` mixes three
> styles — `Customer` (Pascal), `created_at` (snake), `publicId` (camel).
> As long as no `Lint` block is set, that's allowed; as a showcase
> example for the language, it's unfortunate.

### What the language itself names

The built-in keys follow a fixed casing, because they are part of the
language and not the user's domain:

- Blocks: PascalCase — `Table`, `Meta`, `Dialect`
- Setting keys and flags: camelCase — `styleClasses`, `onDelete`,
  `exampleValue`

## 1.9 The file header `spec`

```
spec 1;
```

States **which language definition this file is maintained against**.
Must be the first statement in the file.

`spec` and `version` are two different things, and both names are
taken — which is why they must not overlap:

| | Describes | Set by | Where |
|---|---|---|---|
| `spec 1;` | the language definition the file is written against | the language | file header |
| `version = "1.4.0"` | the model's own editing state | the user | `Meta` block |

`spec` is an integer, not a semver value: there are only whole language
revisions. A file with `spec 2;` may be rejected with a clear message by
a tool that only understands `1`. `version`, on the other hand, is a free
string — the user decides whether it says `"1.4.0"`, `"2026-Q1"`, or
`"Anna's draft"`.

## 1.10 File extension

The binding extension is **`.skema`**. The formerly used extension
`.erp` is outdated; tooling and import resolution do not take it into
account.

> `examples/myexample.skema` still imports `"./common.erp"` in line 3,
> and the README mentions `examples/shop.erp` — both need to be updated.

## 1.11 Custom keys: the `x-` prefix

Everywhere the language recognizes key-value pairs, the same rule
applies:

- **Known keys** are checked, autocompleted, and understood by tooling.
- **Custom keys** must start with `x-` and be quoted. They are not
  checked, but passed through unchanged into the JSON export.
- **Everything else is an error.**

```
Meta {
  author    = "anna";           # known
  "x-jira"  = "SHOP-412";       # custom
  athor     = "anna";           # ERROR — typo, no x-
}
```

The purpose of the third line is the actual point: without it, a typo
would remain silently ineffective. That's the most unpleasant kind of
error, because it looks like a tooling problem rather than a writing
mistake.

The rule applies in `Meta`, in `Style`, and in field attribute blocks.

## 1.12 Imports

```
import "./common.skema" as base;
```

- The path is relative to the importing file.
- `as <alias>` binds the content to a local namespace prefix.
- Access afterward: `base.Address`, `base.Currency`, …

⚠️ Open: The example imports `base` but never uses the alias — instead
`shop.` appears as a prefix, which is introduced by no import anywhere.
How a namespace comes into being is clarified in
`02-project-document-namespaces.md`.
