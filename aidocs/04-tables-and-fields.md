# 04 — Tables and Fields

> Status: **Draft.** Sections marked ⚠️ are not yet decided.

## 4.1 The one basic shape

Before getting to fields, an observation about the whole language is
worthwhile. Almost everything in erplang has the same shape:

```
Name  Type  [Flags]  { Body }
```

| Example | Name | Type | Flags | Body |
|---|---|---|---|---|
| `Table Order { … }` | `Order` | — | — | ✓ |
| `email varchar [required] { … }` | `email` | `varchar` | ✓ | ✓ |
| `customer >Customer.id [onDelete=CASCADE];` | `customer` | `>Customer.id` | ✓ | — |
| `ix_email (email) [unique];` | `ix_email` | `(email)` | ✓ | — |
| `Style @table { … }` | `@table` | — | — | ✓ |
| `Dialect @postgres { … }` | `@postgres` | — | — | ✓ |
| `Datatype decimal(p, s) { … }` | `decimal(p, s)` | — | — | ✓ |
| `draft { label = "Draft" }` | `draft` | — | — | ✓ |

Any part may be missing, the **order** never. Once you've grasped this,
you can read any line of the language, even one you've never seen
before.

Beyond that there are only two more statement kinds:

| Kind | Form | Examples |
|---|---|---|
| Assignment | `key = value;` | `mode = draft;` · `note = "…";` |
| Directive | `lowercase …;` | `spec 1;` · `import "…" as base;` · `use common_base;` |

Three forms, nothing more. That's the yardstick every extension has to
be measured against.

## 4.2 The field: three zones, three questions

```
username  regexstring(`^[a-z0-9_]{3,20}$`)  [required]  { note = "Login name"; }
          └──────────── Type ─────────────┘  └─ Flags ─┘  └────── Body ──────┘
```

Each zone answers exactly **one** question:

| Zone | Question | Affects |
|---|---|---|
| **Type** | *Which values are allowed?* | range, shape, precision |
| **`[…]`** | *What role does the field play in the schema?* | key, uniqueness, relationships, defaults |
| **`{…}`** | *What does a human need to know about it?* | description, examples, custom keys |

The test for any new property is thus: *Does it constrain the value?
Then it's the type. Does it describe the role in the table? Then it's a
flag. Does it explain something for humans? Then it's the body.*

This split is stronger than the original two-way split of "`[]`
generates DDL, `{}` describes". It additionally explains where things
like `regex`, length specifications, and precision belong — namely in
neither bracket, but in the type.

## 4.3 Zone 1 — the type

### Simple types

In `draft` mode, `string`, `int`, `float`, `bool` suffice (see `02`,
section 2.7).

### Types with arguments

Precision, length, and shape are **arguments of the type**, not flags:

```
gross_sum  decimal(10, 2);
name       varchar(80);
username   regexstring(`^[a-z0-9_]{3,20}$`);
```

The payoff is composability: `decimal(10,2)` is a different type from
`decimal(19,4)`, not the same type with different flags. And because
types are defined via `Datatype` itself (see `09`), every project can
bring its own, without changing the language.

### Regexes are strings with escapes

Regexes go into ordinary strings, backslashes are doubled (see `01`,
section 1.3):

```
regex = "^\\d{4}-\\d{2}$";
```

Backticks stay reserved for SQL expressions and are **not** used for
regexes — otherwise there would be two raw-text forms with different
meanings.

### Regexes are descriptive, not enforced

A regex generates **no** `CHECK` in the database. It feeds form
generators, API schemas, test data, and documentation.

This is a deliberate restraint: not every engine understands the same
regex syntax, and a model that validates in Postgres but not in MySQL
promises more than it can keep. The language would rather state honestly
what it actually guarantees.

## 4.4 Zone 2 — the flags

```
id       int  [pk];
publicId uuid [unique];
email    varchar [required, column="email_address"];
customer >Customer.id [onDelete=CASCADE];
created  timestamp [default=`now()`];
```

| Flag | Effect |
|---|---|
| `pk` | primary key |
| `unique` | unique |
| `required` | not null |
| `default=…` | default value; raw expression in backticks allowed |
| `column="…"` | physical column name (see `07`) |
| `onDelete=…` | delete behavior of a relationship (see `05`) |
| `onUpdate=…` | update behavior of a relationship |

### On the worry that `[]` is "really a list"

That a list contains a mix of bare words (`pk`) and pairs (`default=4`)
is not a design flaw. `[pk]` is shorthand for `[pk=true]` — a flag
without a value means "yes". HTML attributes, Rust attributes, and dbml
have carried the same pattern for years. It's the right choice here: the
normal case stays a single word, the special case gets a value.

⚠️ Still to clarify: the casing — `onDelete` (example, camelCase) versus
`on_delete` (`description.txt`, snake_case). Per `01`, section 1.8,
built-in keys are camelCase — so `onDelete`.

## 4.5 Zone 3 — the body

```
sum float [required] {
  note           = "Gross sum including shipping";
  exampleValue   = "19.99";
  "x-customkey"  = "Foobar";
}
```

A flat set of key-value pairs, nothing else. Known keys are checked,
custom ones carry `x-` (see `01`, section 1.11).

| Key | Meaning |
|---|---|
| `note` | description; becomes hover text, DDL comment, documentation |
| `exampleValue` | sample value for test data and documentation |

### `regex` doesn't belong here

`examples/myexample.skema` introduces a `regex = ""` in the field body on
line 75. This goes away without replacement. A regex constrains the
**value range** and is therefore a matter of the type, not of the field:

```
# wrong
username varchar { regex = "^[a-z0-9_]+$"; }

# right
Datatype Username {
  Field { type = string; regex = "^[a-z0-9_]+$"; }
}

Table User { username Username; }
```

This makes the zone rule hold without exception: **the field body
contains not a single word about permitted values.** Anyone looking for
a constraint looks at the type — always, without having to think about
it.

The side effect is the actual payoff: a constraint that has a name gets
reused. One that lives in the field gets copied.

⚠️ Open: May `Meta` appear in the field body (for `author`)? Per the
rule from `03`, `Meta` is the place for "who and why". In the field,
however, `note` and `exampleValue` are noted directly, without a `Meta`
wrapper. Two possibilities: treat the field body as its own small `Meta`
block (then the exception is named) or allow `Meta` there too (then the
structure is consistent, but the normal case gets longer).

## 4.6 Avoiding repetition: named types

The biggest lever against long field lines lies not in the field's
syntax, but in naming constraints **once** instead of repeating them.
That's what `Datatype` provides (see `09`):

```
Datatype Email {
  Field {
    type  = string;
    regex = "^[^@]+@[^@]+$";
    note  = "Email address, not checked for deliverability";
  }
}

Datatype Money {
  Field { type = float; }
  Dialect @postgres { sql = "numeric(19,4)"; }
  Dialect @mysql    { sql = "DECIMAL(19,4)"; }
}
```

At the point of use, almost nothing is left — and what remains reads
like domain language instead of technical detail:

```
Table User {
  id       int      [pk];
  email    Email    [required, unique];
  guthaben Money;
}
```

`Email` tells a reader more than `varchar` plus a regex next to it. A
change to the rule takes effect everywhere, instead of having to be
carried over into five files.

## 4.7 Nullability

Optionality is **always a flag**, never a type suffix. The default
matches SQL's: **without any statement, a field may be NULL.**

```
name  varchar(80) [required];   # NOT NULL
notiz varchar;                  # NULL allowed
notiz varchar [optional];       # the same, just explicit
```

| Flag | Effect |
|---|---|
| `required` | NOT NULL |
| `optional` | NULL allowed — the default, written explicitly |

There is **no** project-wide setting that flips this default. That's
intentional: the same line means the same thing in every project. A
model whose fields are required or optional depending on a setting
elsewhere can no longer be read without knowing that setting — let alone
copied from a foreign file.

In particular: such a setting must never live in the `Lint` block.
`mode = draft` ignores all lint rules (see `02`, section 2.7) — putting
the NULL default there would mean that switching from `draft` to
`strict` changes the **meaning of every field**. A switch for warnings
must never change the semantics of the model.

⚠️ Small open question: `[optional]` has no effect, it merely spells out
the default. Should it be allowed (readability) or flagged as redundant
(less noise)?

## 4.8 `pk` implies `required` and `unique`

`id int [pk, required]` in `examples/myexample.skema` contains a
redundancy: a primary key is always required and always unique.

Proposal: `[pk]` alone suffices. Anyone who additionally writes
`required` or `unique` gets a lint hint, not an error — the statement
isn't wrong, just redundant.

## 4.9 Decided

- Three zones: type = value range, `[]` = role in the schema, `{}` = human knowledge (4.2)
- Regexes are descriptive and live exclusively in `Datatype` (4.3, 4.5)
- Nullability is a flag; the default is NULL, as in SQL (4.7)
- The counterpart to `[required]` is called `[optional]` (4.7)

## 4.10 Open questions in this chapter

1. May `[optional]` be written even though it's the default? (4.7)
2. May `Meta` appear in the field body? (4.5)
3. Composite primary keys — `[pk]` on two fields, or its own block?
4. Confirmation: `pk` implies `required` and `unique` (4.8)
