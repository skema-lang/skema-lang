# 13 — Reference

[← Index](README.md)

## Glossary

| Term | Definition |
|---|---|
| `spec` | Directive declaring which language revision a file follows. First statement of a file. |
| `Project` | Top-level settings block: `mode`, `dialect`, project-wide `Meta`/`Style`/`Lint`/`Database`. Exactly one per project, in the root file. |
| `Document` | A file's namespace declaration. Exactly one per file. |
| `Table` | A table declaration: fields plus optional `Meta`, `Database`, `Index`. |
| `Partial` | A reusable group of fields, inserted with `use`. Never becomes a table. |
| `Enum` | A closed, ordered set of named values with a `Field` block describing storage. |
| `Group` | A named, cross-file collection of declarations for diagrams and documentation. No schema effect. |
| `Datatype` | A user-defined type: parameters, a `Field` block, and per-engine `Dialect` blocks. |
| `Meta` | Descriptive block (author, note, version, style classes). Never affects generated schema. |
| `Style` | Visual block. `@name` selects a built-in element type; a bare name defines a reusable class. |
| `Lint` | Project-wide naming/consistency rules, enforced only under `mode = strict`. |
| `Database` | The block for physical names: `database`, `schema`, `table`. |
| `Dialect` | Engine-specific block, named `@postgres` / `@mysql` / etc., used inside `Datatype` and `Database`. |
| `Index` | A table's or partial's list of indexes. |
| `Example` | A table's sample-data container: any number of `Record` blocks. Descriptive only. |
| `Record` | One sample row inside `Example`: `field = value;` assignments, unnamed, identified by position. |
| Field (declaration) | A named entry in a `Table`, `Partial`, or `Enum` body — see the [three-zone shape](05-tables-and-fields.md). |
| `Field` (sub-block) | Inside `Datatype`/`Enum`: describes how a field of that type behaves at the model level. |
| root file | The one file in a project that carries the `Project` block and, transitively, imports everything else. |
| raw expression | Text between backticks, passed to the target database unmodified — for example `` `now()` ``. |

## Keyword classes

| Class | Casing | Examples |
|---|---|---|
| Directive | lowercase | `spec`, `import`, `as`, `use` |
| Block keyword | PascalCase | `Project`, `Document`, `Table`, `Enum`, `Partial`, `Group`, `Datatype`, `Meta`, `Style`, `Index`, `Database`, `Dialect`, `Lint`, `Example`, `Record` |
| Setting key / flag | camelCase | `styleClasses`, `onDelete`, `exampleValue` |

Full explanation: [01 — Case conventions and keywords](01-language-basics.md#case-conventions-and-keywords).

## Statement forms

| Form | Shape |
|---|---|
| Declaration | `Name Type [Flags] { Body }` — any part may be omitted, order never changes |
| Assignment | `key = value;` |
| Directive | `lowercaseWord …;` |

Full walkthrough: [01 — Language Basics](01-language-basics.md).

## Flag reference

| Flag | Where | Meaning |
|---|---|---|
| `pk` | field | primary key (implies `required`, `unique`) |
| `unique` | field | uniqueness constraint |
| `required` | field | `NOT NULL` |
| `optional` | field | `NULL` allowed (the default, written explicitly) |
| `default=…` | field | default value; accepts a raw expression |
| `column="…"` | field | physical column name |
| `onDelete=…` | relationship field, `>` side only | `cascade` \| `restrict` \| `setNull` \| `setDefault` \| `noAction` |
| `onUpdate=…` | relationship field, `>` side only | same values as `onDelete` |
| `unique` | `Index` entry | uniqueness constraint on the listed columns |

## Relationship operators

| Operator | Reads as | FK column created in |
|---|---|---|
| `>` | many belong to one | the declaring table |
| `<` | one has many (backreference) | nowhere — no column |
| `-` | one to one | the declaring table, plus `UNIQUE` |

No operator exists for many-to-many — write the junction table. Full explanation: [07 — Relationships](07-relationships.md).

## Block quick reference

| Block | Top-level? | Named? | Typical parents | Typical children |
|---|---|---|---|---|
| `Project` | yes | no | — | `Meta`, `Style`, `Lint`, `Database` |
| `Document` | yes | yes | — | `Meta`, `Style` |
| `Table` | yes | yes | — | fields, `Meta`, `Database`, `Index`, `Example` |
| `Partial` | yes | yes | — | fields, `Meta`, `Index`, `use` |
| `Enum` | yes | yes | — | `Field`, entries, `Meta` |
| `Group` | yes | yes | — | `Table`/`Enum` references, `Meta` |
| `Datatype` | yes | yes | — | parameter bodies, `Field`, `Dialect` |
| `Meta` | no | no | any named declaration | key/value pairs |
| `Style` | no | no (`@name`) or yes (class) | `Project`, `Document` | key/value pairs |
| `Lint` | no | no | `Project` only | key/value pairs |
| `Database` | no | no | `Project`, `Document`, `Table` | key/value pairs, `Dialect` |
| `Dialect` | no | yes (`@engine`) | `Database`, `Datatype` | key/value pairs |
| `Index` | no | no | `Table`, `Partial` | index entries |
| `Example` | no | no | `Table` | `Record` |
| `Record` | no | no (position only) | `Example` | key/value pairs |

## Known limitations

The items below are demonstrated in examples or proposed in the design docs, but not yet fully finalized. Expect them to stabilize, not necessarily to keep this exact shape.

| Area | Open question | See |
|---|---|---|
| Foreign key column naming | Whether an unqualified relationship field gets an automatic suffix (`customer` vs. `customer_id`) | [07](07-relationships.md#physical-foreign-key-column-name) |
| `[column="…"]` | Full interaction with generated DDL not finalized | [05](05-tables-and-fields.md#zone-2-flags), [11](11-physical-mapping.md#physical-column-name) |
| Composite primary keys | Syntax for a multi-field primary key is undecided | [05](05-tables-and-fields.md#zone-2-flags) |
| Enum physical storage | Native enum type vs. `varchar` + `CHECK` vs. plain `int`, per dialect | [08](08-enums.md) |
| Enum entry metadata | Whether a single entry can carry its own `Meta` block | [08](08-enums.md#meta-on-enum) |
| Index types | `btree`, `gin`, `hash`, and other dialect-specific index kinds | [10](10-indexes.md#not-yet-specified) |
| Partial / expression indexes | `WHERE …` conditions and `(lower(email))`-style expressions | [10](10-indexes.md#not-yet-specified) |
| Automatically generated index names | Naming scheme for an unnamed index | [10](10-indexes.md#not-yet-specified) |
| Index names from `Partial`s | Same name repeated across every consuming table | [09](09-partials.md#what-a-partial-may-contain), [10](10-indexes.md#indexes-inside-a-partial) |
| `Style` core property set | Full list of standardized, validated style properties | [03](03-meta-and-style.md#style-properties) |
| `Dialect` property set | `sql` is defined inside `Datatype`; which engine settings are known inside `Database` (`tablespace`, `engine`, `charset`, …) is not | [06](06-data-types.md#dialect-engine-blocks), [11](11-physical-mapping.md#engine-specific-settings) |
| `@field` selector | Styling individual field rows | [03](03-meta-and-style.md#selectors) |
| Lint severities | Per-rule severity override syntax (for example, `fieldNaming = snake_case [error];`) | [04](04-linting.md#other-rules) |

## Not enforced yet

Rules this documentation states that the checker does not yet verify. They are
implementation gaps, not open questions — the language has decided, `skema-hir`
has not caught up. A model may therefore pass `skema check` and still break one
of them.

| Rule | Stated in | What the checker does today |
|---|---|---|
| `Example` row values for an `Enum`-typed field, or a `Datatype` field whose own `Field.type` names another custom type | [05](05-tables-and-fields.md#example-rows) | not checked — only a built-in or a one-level-resolved `Datatype` is |

## See also

- [README](README.md) — documentation map and quickstart
