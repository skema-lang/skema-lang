# 02 — Project, Document, and Namespaces

> Status: **Draft.** Sections marked ⚠️ are not yet decided.

## 2.1 The two levels

erplang has exactly two levels above declarations:

| | `Project` | `Document` |
|---|---|---|
| Count | **exactly once per project** | **exactly once per file** |
| Required | yes | yes |
| Governs | mode, dialect, lint, project-wide styles | this file's namespace, file metadata |
| Analogous to | `Cargo.toml` / `tsconfig.json` | a `package` declaration in Java or Go |

It follows that: a project consisting of ten files has ten `Document`
blocks and **one** `Project` block. A second `Project` declaration
anywhere — including in an imported file — is an error.

## 2.2 `Document` is the namespace

The name in the `Document` block is the namespace of all declarations in
the file.

```
spec 1;

Document shop { }

Enum  OrderStatus { … }     # full name: shop.OrderStatus
Table Order       { … }     # full name: shop.Order
```

Important: declarations sit **beside** the `Document` block, not inside
it. The block declares the identity of the file, it does not enclose
anything. Just like `package shop;` in Java does not bracket the file's
classes.

The document name is independent of the filename. `myexample.skema` may
declare `Document shop`. (A lint hint on mismatch is conceivable, see
2.7.)

## 2.3 Position in the file header

The `Document` block may appear **anywhere** at the top level — before,
between, or after declarations. Only `spec` has a fixed position.

The language enforces nothing here. Anyone wanting consistency reaches
for the `Lint` block (2.7); the formatter may pull the block to the top.

## 2.4 References within the same file

Within a file the prefix is **optional**; on imported declarations it is
**mandatory**:

```
Document shop { }

Enum OrderStatus { paid; }

Table Order {
  status   OrderStatus [default=OrderStatus.paid];   # local, no prefix
  currency base.Currency;                            # imported, with prefix
}
```

A rule follows from this that can be applied immediately while reading:

> **A prefix in the source always means: this comes from another file.**

The full path remains permitted locally, but it's not recommended — it
breaks on every rename of the namespace. `examples/myexample.skema` writes
`default=shop.OrderStatus.paid` on line 71, even though the enum lives in
the same file.

## 2.5 Imports and aliases

The namespace is assigned **at the source** and may be renamed **at the
point of use**.

```
# ---- customer.skema ----
Document customer { }
Table User { id int [pk]; }
```

```
# ---- order.skema ----
Document shop { }

import "./customer.skema";              # reachable as  customer.User.id
import "./customer.skema" as user;      # reachable as  user.User.id
```

- **Without `as`**, the imported file's own document name applies.
- **With `as`**, that name is overridden for this file.

The alias takes effect **only in the importing file**. The same file may
appear as `user` in `order.skema` and as `customer` in `invoice.skema`.
The document name belongs to the source file, the alias belongs to the
caller.

### The full visibility rule

| Where from | Notation |
|---|---|
| own file | `Order.id` — or equivalently `shop.Order.id` |
| imported file | `customer.User.id`, or under alias `user.User.id` |

`shop.Order.id` and `Order.id` denote the same thing within `shop`.

### Uniqueness of namespaces

Within a file, every namespace must be unique — including the file's own
document name. Two imports with the same document name are an error,
until one of them is renamed via `as`:

```
import "./de/common.skema";           # Document common
import "./en/common.skema" as en;     # Document common, renamed
```

This gives `as` a clear, single job: resolving collisions and improving
readability. It is not mandatory noise on every import line.

### What an import brings in

An import makes the other file's **declarations** visible: `Table`,
`Enum`, `Partial`, `Datatype`, `Group`. It does **not** bring in that
file's `Document` metadata or styles.

### Imports are not transitive

If `a` imports file `b`, and `b` imports `c`, the declarations from `c`
are **not** visible in `a`. Anyone who wants to use `c` imports `c`.

This makes the import list at the top of a file the complete and honest
list of that file's dependencies. A restructuring in `b` cannot silently
break `a`.

### Cycles are allowed

`a` may import `b`, and `b` may import `a` at the same time:

```
# ---- order.skema ----
Document shop { }
import "./customer.skema";
Table Order { buyer >customer.User.id; }

# ---- customer.skema ----
Document customer { }
import "./order.skema";
Table User { last_order >shop.Order.id; }
```

This is intentional: mutual references are the normal case in an ER
model, and the language should therefore not force files apart. It's
resolvable because declarations are order-independent — tooling first
collects all names across the whole graph and only then checks
references.

> For implementation this means: the module graph must recognize
> already-visited files while reading, or resolution will run forever.

## 2.6 The root file

Exactly one file in the project carries the `Project` block. That file
is the **root file**: it imports — directly or through intermediate
files — everything that belongs to the project, and it's what gets
handed to the tool.

```
erplang check main.skema
erplang export --json main.skema
```

Three things follow from this:

1. **Project membership is reachability-defined.** A `.skema` file not
   reachable from the root file does not belong to the project — even if
   it sits in the same directory. The import graph *is* the project
   definition; there is no second list to maintain.
2. **The import graph is a tree with one root.** A second `Project`
   declaration anywhere in the graph is an error.
3. **A single file cannot be fully checked in isolation.** `mode` and
   `dialect` live in the root file; without them, the checker doesn't
   know whether `float` is allowed.

### Consequence for the language server ⚠️

Point 3 hits the IDE. If someone opens `order.skema` directly, there's no
path from there up to the root file — imports only point downward.

The language server therefore needs the root file from another source. A
practical option is an editor setting:

```jsonc
// .vscode/settings.json
{ "erplang.rootFile": "main.skema" }
```

If it's missing, the server should not guess, but do two things: keep
reporting syntax and file-local errors, and issue a visible notice for
anything that needs `mode`/`dialect` ("no root file configured — type
checking inactive"). Silently falling back to `draft` would be the worst
option: the model would look error-free and would not be.

## 2.7 The `Project` block in detail

```
Project {
  mode    = draft;      # draft | strict
  dialect = generic;    # generic | postgres | mysql | oracledb

  Meta  { … }
  Style @table { … }
  Lint  { … }
}
```

### `mode` — the main switch for strictness

`mode` is **not** merely a type setting. It decides whether the project
is checked against rules at all:

| | `mode = draft` | `mode = strict` |
|---|---|---|
| Lint rules | are **ignored** | are **enforced** |
| Data types | simple ones suffice: `string`, `int`, `float`, `bool` | more precise ones are required |
| Intended for | sketching, whiteboard, first pass | a resilient model, code generation |

The benefit is the two-stage nature of the workflow: a model may start
out fuzzy and untidy — ideas first, rigor later. Switching to `strict`
turns every fuzziness into a visible diagnostic. The transition costs a
single word.

It also follows that: **in `draft` mode the `Lint` block has no effect.**
It may still be written there — it describes what *will* apply.

> Deliberate coupling: strictly speaking, lint and type precision are two
> independent axes. They are intentionally tied to a single switch here,
> because two switches would produce four states, two of which nobody
> wants to explain. Should the need arise, a later split remains possible
> in a backward-compatible way.

### `dialect`

Selects the target database and, with it, the available type table.
`generic` means: portable types only, nothing dialect-specific.

`mode` and `dialect` interact to produce three meaningful tiers:

| `mode` | `dialect` | Allowed types |
|---|---|---|
| `draft` | `generic` | simple types — `string`, `int`, `float`, `bool` |
| `strict` | `generic` | more precise but portable types — `varchar(80)`, `decimal(10,2)`, `timestamp`, `uuid` |
| `strict` | `postgres` … | plus everything dialect-specific — `jsonb`, `tsvector`, `citext` |

The middle tier is the actually interesting one: **precise and still
portable**. It is the normal state for a maintained model; the third
tier deliberately leaves portability behind.

⚠️ Open: the exact boundary of the middle tier — which types count as
"portably precise"? Detailed in `09-datatypes.md`.

### `Lint`

Conventions are a project matter, not a language matter (see `01`,
section 1.8). The `Lint` block makes them explicit and versions them
with the model:

```
Project {
  mode = strict;

  Lint {
    fieldNaming = snake_case;      # default: any
    tableNaming = PascalCase;
    enumNaming  = PascalCase;

    unusedImport = warn;
    danglingRef  = error;
  }
}
```

`Lint` may **only** appear inside the `Project` block. There are no
exceptions per file, per table, or per line: a rule applies to the
whole project or not at all.

This is a deliberate hard constraint. The cost: a single legacy table
with foreign field names can tip a rule for the whole project. The
payoff: there is no place where a rule is secretly turned off — anyone
who wants to know what applies reads exactly one block. The escape
hatch for legacy baggage is `mode = draft`, until cleanup is done.

Possible values for naming conventions: `snake_case`, `camelCase`,
`PascalCase`, `SCREAMING_SNAKE_CASE`, `any`.

⚠️ Open: severity levels. The default could be `warn`, tightened via the
language's `[]` convention — square brackets change behavior:

```
fieldNaming = snake_case [error];
tableNaming = PascalCase [off];
```

### `Meta` and `Style`

Described in `03-meta-and-style.md`.

## 2.8 Decided

- `Document` may appear freely at the top level (2.3)
- Prefix optional locally, mandatory on imports (2.4)
- Without `as`, the source file's document name applies; `as` overrides
  it for the importing file (2.5)
- Imports are not transitive (2.5)
- Import cycles are allowed (2.5)
- The `Project` block lives in the root file, which imports everything
  and is handed to the tool (2.6)
- `mode` switches lint **and** type precision together (2.7)
- `Lint` appears only in the `Project` block, with no local exceptions (2.7)

## 2.9 Open questions in this chapter

1. Severity syntax for lint rules — `fieldNaming = snake_case [error];`? (2.7)
2. Complete list of lint rules
3. Boundary of "portably precise" types under `strict` + `generic` →
   `09-datatypes.md`
4. Does the language server need to learn the root file via an editor
   setting? (2.6)
