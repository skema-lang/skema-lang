# Overview

skema is a language for describing entity-relationship models.
A model is written once and then serves as a diagram, as
documentation, and as a template for DDL across different databases.

File extension: **`.skema`**

## Minimal Example

```
spec 1;

Document blog { }

Table Post {
  id    int          [pk];
  title varchar(200) [required];
}
```
## Quickstart

The language uses Blocks to define groups of Key value Pairs.
The following Blocks are available

```
Document name {

}

Project {

}

Table name {

}

Partial name {

}

Datatype name {

}
```
All Blocks had a name to. The only exception is the Project Block.
All of these Blocks are defined at Root Level inside a .skema file. 
These blocks cant be nested. 


Inside Partial and Table Blocks are Field Deklarations are available.
Fields are defined like this:

```
columname datatype [ flags ] { body };

```

All other values are defined via asignments like:

```
key = value;
```

There exists also a few statements like.

```
use Partial.name;
import file as namespace;
```

The following keywords are also reserved.
```
lowercase, required, optional, pk, index, unique, uppercase, draft, postgres, mysql, mariadb, generic
```








## Three statement types, nothing more

Anyone who knows these three forms can read any line of the language — even
one they've never seen before.

| Kind | Form | Examples |
|---|---|---|
| **Declaration** | `Name Type [Flags] { Body }` | `Table Order { … }` · `email varchar [required] { … }` |
| **Assignment** | `key = value;` | `mode = draft;` · `note = "…";` |
| **Directive** | `lowercase …;` | `spec 1;` · `import "…" as base;` · `use Base;` |

In a declaration, any part may be missing — the **order**, never.

## The two guiding rules

**1. Three zones, three questions.** Every property of a field has exactly one
correct place:

```
username Username [required] { note = "Login name"; }
         └─ Type ┘ └─Flags─┘ └────── Body ───────┘
```

| Zone | Question |
|---|---|
| Type | *Which values are allowed?* |
| `[…]` | *What role does the field play in the schema?* |
| `{…}` | *What does a human need to know about it?* |

**2. `@` marks what's built in.** A name with `@` belongs to the language, a
name without `@` belongs to the user:

```
Style @table   { … }    # built-in element type
Style highlight { … }   # self-assigned class
Dialect @postgres { … } # built-in engine
```

## Keywords

### Directives — lowercase

| Word | Meaning |
|---|---|
| `spec` | language version; first line of the file |
| `import` … `as` | include a file, optionally rename it |
| `use` | insert a `Partial` at this position |

### Declarations — PascalCase, carry a name

| Word | Meaning | Chapter |
|---|---|---|
| `Project` | project settings; **exactly once**, in the root file | 02 |
| `Document` | namespace of the file; **exactly once per file** | 02 |
| `Table` | table | 04 |
| `Enum` | closed set of values | 06 |
| `Partial` | reusable field group | 06 |
| `Group` | visual collection | 08 |
| `Datatype` | custom data type | 09 |

### Settings blocks — PascalCase, no name

| Word | Appears in | Meaning | Chapter |
|---|---|---|---|
| `Meta` | every declaration | author, description, status | 03 |
| `Style` | `Project`, `Document` | appearance | 03 |
| `Lint` | `Project` | conventions | 02 |
| `Database` | `Project`, `Document`, `Table` | physical names | 07 |
| `Dialect @…` | `Database`, `Datatype` | engine-specific settings | 07, 09 |
| `Index` | `Table`, `Partial` | indexes | 07 |
| `Field` | `Datatype`, `Enum` | behavior of a field of this type | 09, 06 |

## Relationships

> **The arrow points to the side where there is exactly one.**

| Notation | Meaning | Column created |
|---|---|---|
| `a >B.b` | many `A` belong to one `B` | in `A` |
| `a <B.b` | one `A` has many `B` | in `B` |
| `a -B.b` | one to one | in `A` |

For many-to-many there is no arrow — instead you write the junction
table. See chapter 05.

## Chapters

| File | Contents |
|---|---|
| [01](01-lexical-structure-and-file-layout.md) | Tokens, comments, strings, `spec`, imports |
| [02](02-project-document-namespaces.md) | `Project`, `Document`, namespaces, `mode`, `Lint` |
| [03](03-meta-and-style.md) | `Meta`, `Style`, selectors, classes, cascade |
| [04](04-tables-and-fields.md) | Tables, fields, the three zones, flags |
| [05](05-relationships.md) | Arrows, cardinality, `onDelete` |
| [06](06-enums-and-partials.md) | `Enum`, `Partial`, `use` |
| [07](07-indexes-and-physical-mapping.md) | `Database`, `Dialect`, `Index` |
| [08](08-groups.md) | `Group` |
| [09](09-datatypes.md) | `Datatype`, parameters, dialects |
| [10](10-open-design-questions.md) | What hasn't been decided yet |

## Examples

| File | Shows |
|---|---|
| `examples/myexample.skema` | all constructs in one file |
| `examples/generic/` | multi-file project with namespaces and imports |

The `examples/generic/` directory is the better starting point: `project.skema`
is the root file, `types.skema` contains building blocks, `catalog.skema` and
`lending.skema` hold the actual model.
