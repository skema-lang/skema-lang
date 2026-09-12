<!-- title: skema Documentation -->

# skema

Technical reference for **skema**, a declarative language for describing entity-relationship models. A `.skema` file is written once and serves as diagram source, documentation, and DDL template across multiple databases.

## Quickstart

```skema
spec 1;

Document blog { }

Table Post {
  id    int          [pk];
  title varchar(200) [required];
}
```

- `spec 1;` — language version, first statement in every file.
- `Document blog { }` — this file's namespace.
- `Table Post { … }` — a table declaration with two fields.

That's a complete, valid file. Everything else in this documentation builds on this shape.

## Documentation map

Read in order for a first pass, or jump directly to the page you need.

| # | Page | Covers |
|---|---|---|
| 01 | [Language Basics](01-language-basics.md) | File anatomy, statement forms, comments, strings, brackets vs. braces, naming |
| 02 | [Project & Documents](02-project-and-documents.md) | `Project`, `Document`, namespaces, imports, `mode`, `dialect` |
| 03 | [Meta & Style](03-meta-and-style.md) | `Meta`, `Style`, selectors, classes, cascade |
| 04 | [Linting](04-linting.md) | `Lint`, naming conventions |
| 05 | [Tables & Fields](05-tables-and-fields.md) | `Table`, the three field zones, flags |
| 06 | [Data Types](06-data-types.md) | Built-in types, `Datatype`, parameters, `Dialect` |
| 07 | [Relationships](07-relationships.md) | `>` `<` `-` arrows, cardinality, `onDelete`/`onUpdate` |
| 08 | [Enums](08-enums.md) | `Enum`, entries, `label`/`value` |
| 09 | [Partials](09-partials.md) | `Partial`, `use`, field reuse |
| 10 | [Indexes](10-indexes.md) | `Index` block, composite indexes |
| 11 | [Physical Mapping](11-physical-mapping.md) | `Database`, schema/table/column names, engine-specific `Dialect` |
| 12 | [Groups](12-groups.md) | `Group`, visual/documentation grouping |
| 13 | [Reference](13-reference.md) | Glossary, keyword tables, grammar cheat sheet, known limitations |

## Example projects

| Path | Shows |
|---|---|
| [`examples/myexample.skema`](../../examples/myexample.skema) | All constructs in a single file |
| [`examples/generic/`](../../examples/generic/) | Multi-file project with namespaces, imports, and per-file schema mapping |

Start with `examples/generic/`: `project.skema` is the root file, `types.skema` holds shared building blocks, `catalog.skema` and `lending.skema` hold the actual model.

## Conventions used in this documentation

- Code blocks tagged `skema` are `.skema` source; `bash` blocks are shell commands, `mermaid` blocks are diagrams.
- A note marked "Not yet specified" points at a rule the language design has not finalized — see [Reference → Known limitations](13-reference.md#known-limitations).
