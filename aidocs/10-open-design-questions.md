# 10 — Open Design Questions

Questions collected from all chapters, sorted by impact. What's listed
here is **not yet** decided — chapters 01 through 09 each carry, at their
end, the list of what has been decided.

## A — Have consequences for the generated schema

These questions change what ends up in the database. They should be
answered before the first generator.

| # | Question | Chapter |
|---|---|---|
| A1 | Physical name of the foreign key column: `customer` or `customer_id`? | 05.7 |
| A2 | Physical column name on the field — flag `[column="…"]`? | 07.5 |
| A3 | Composite primary keys: `[pk]` on two fields, or its own block? | 04.10 |
| A4 | Confirmation: `pk` implies `required` and `unique` | 04.8 |
| A5 | Physical mapping of an `Enum` — native type, `varchar`+`CHECK`, `int`? | 06.5 |
| A6 | May `Field.type` point at another `Datatype`? (layering, cycle checking) | 09.11 |
| A7 | Index types (`btree`, `gin`), partial indexes, expression indexes | 07.6 |
| A8 | Naming scheme for automatically generated index names | 07.6 |
| A9 | Index names from `Partial`s that land in many tables — they collide | 06.5 |
| A10 | **Which types does the language ship with?** see below | 09.11 |

### A10 in detail — the largest remaining gap

Chapter 02 names `string`, `int`, `float`, `bool` as the simple types.
But both examples naturally also use `varchar(120)`, `text`, `uuid`,
`date`, and `timestamp` — without anything stating where these come
from.

Three possible answers:

1. **Built in.** The language ships a fixed set — say `string`, `text`,
   `varchar(n)`, `int`, `bigint`, `float`, `decimal(p,s)`, `bool`,
   `date`, `time`, `timestamp`, `uuid`, `json`, `blob`. Convenient, but
   every addition is a language change.
2. **As a standard library.** A bundled `std.skema` full of `Datatype`
   declarations, imported implicitly. The language stays small, the set
   is readable and copyable, and a project can override or extend it.
   This fits the principle from 09.7: as few built-in types as possible.
3. **Nothing at all.** Every project declares everything itself.
   Maximally clean, practically unusable — nobody wants to write
   `Datatype uuid`.

Option 2 is the only one that both upholds the principle and remains
usable. It then needs an answer for how `std` is imported and whether
its namespace is visible without a prefix — which would contradict the
rule from 02.4.

## B — Affect the language surface

Influence how the language is written and read, but not the result.

| # | Question | Chapter |
|---|---|---|
| B1 | May `[optional]` be written even though it's the default? | 04.7 |
| B2 | May `Meta` appear in the field body, or is the body itself already metadata? | 04.5 |
| B3 | Does an enum entry have a metadata body or a `Meta` block? | 06.5 |
| B4 | `Meta` allowed on all declarations? | 03.7 |
| B5 | May style classes also be defined in `Document`? | 03.7 |
| B6 | Add selectors `@field` and `@relation`? | 03.7 |
| B7 | Must a `<` backreference point at an existing `>` relationship? | 05.9 |
| B8 | Must parameters with `default` appear at the end of the signature? | 09.11 |
| B9 | Casing of type names — `decimal` lowercase, `Email` uppercase? | 09.8 |
| B10 | Allowed values for `Field.type` in `Enum` — only `string` and `int`? | 06.5 |
| B11 | Confirmation: `Group` is purely visual and generates no schema | 08.10 |

## C — Affect tooling

| # | Question | Chapter |
|---|---|---|
| C1 | Does the language server need to learn the root file via an editor setting? | 02.9 |
| C2 | Severity syntax for lint rules — `fieldNaming = snake_case [error];`? | 02.9 |
| C3 | Complete list of lint rules | 02.9 |
| C4 | Which properties belong in the style core set? | 03.7 |
| C5 | Should ungrouped tables produce a lint hint? | 08.10 |
| C6 | Should the formatter normalize the three comment forms? | 01.2 |
| C7 | Boundary of "portably precise" types under `strict` + `generic` | 02.9 |
| C8 | Which base types are built in — only `string`, `int`, `float`, `bool`? | 09.11 |

## D — Ideas without a decision

### D1 — An `Object` keyword

`description.txt` listed the keyword list with an `( Object? )` —
noted as a question, never followed through.

Conceivable would be a model object that is **not** a table: a composite
value, a JSON document within a column, or a view (`VIEW`). All three
are real needs, but different enough that a single keyword can hardly
cover them.

Before deciding, it would need to be clarified which of the three was
meant.

### D2 — Views and materialized views

Not part of the design so far, but present in every real schema. A view
is not a table: it has no data of its own, no primary key, and its
content is a SQL expression. Whether the language should support this is
a fundamental question — it would move the language from a modeling
language toward a schema language.

### D3 — Migrations

The language describes a state, not a transition. As soon as models are
used in production, the question of renames arises: a table that changes
from `Client` to `Customer` looks, to a naive comparison, like one deleted
and one new declaration.

A stable identifier per declaration — something like
`Meta { "x-id" = "…" }` — would be the smallest provision to make this
solvable later at all.

## E — Known inconsistencies in the existing material

| Where | What |
|---|---|
| `README.md` | mentions `examples/shop.erp` and `docs/grammar.md`; neither exists |
| `README.md` | "Known gaps" refers to `--` in relationships — the arrows are now `>`, `<`, `-` |
| `description.txt` | fully superseded by chapters `00`–`10`; can be removed |
