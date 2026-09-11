# 05 — Tables & Fields

[← Index](README.md)

## Declaring a table

```skema
Table Post {
  id    int          [pk];
  title varchar(200) [required];
}
```

`Table` is a top-level declaration (see [01 — The one declaration shape](01-language-basics.md#the-one-declaration-shape)). Its body is a list of field declarations, plus optional `Meta`, `Database`, and `Index` blocks.

## The field: three zones, three questions

```skema
username  regexstring(`^[a-z0-9_]{3,20}$`)  [required]  { note = "Login name"; }
          └──────────── Type ─────────────┘  └─ Flags ─┘  └────── Body ──────┘
```

| Zone | Question | Affects |
|---|---|---|
| **Type** | *Which values are allowed?* | range, shape, precision |
| **`[…]`** | *What role does this field play in the schema?* | keys, uniqueness, relationships, defaults |
| **`{…}`** | *What does a human need to know?* | description, examples, custom keys |

Test for any new field property: does it constrain the value — type. Does it describe the field's role in the table — flag. Does it explain something for humans — body.

## Zone 1: Type

### Simple types

Under `mode = draft`: `string`, `int`, `float`, `bool` (see [02 — The mode switch](02-project-and-documents.md#the-mode-switch)).

### Types with arguments

Precision, length, and shape are **arguments of the type**, not flags:

```skema
gross_sum decimal(10, 2);
name      varchar(80);
username  regexstring(`^[a-z0-9_]{3,20}$`);
```

`decimal(10,2)` and `decimal(19,4)` are different types, not the same type with different flags. See [06 — Data Types](06-data-types.md) for the full built-in and custom type system.

### Regexes are strings, not raw expressions

```skema
regex = "^\\d{4}-\\d{2}$";
```

Backslashes are doubled as in any skema string (see [01 — Strings and escapes](01-language-basics.md#strings-and-escapes)). Backticks are reserved for SQL expressions and are not used for regexes.

A regex is **descriptive, not enforced** — it does not produce a database `CHECK` constraint. It feeds form generators, API schemas, test data, and documentation instead, since not every target engine shares the same regex dialect.

### Regexes live in Datatype, not in a field

```skema
# wrong
username varchar { regex = "^[a-z0-9_]+$"; }

# right
Datatype Username {
  Field { type = string; regex = "^[a-z0-9_]+$"; }
}

Table User { username Username; }
```

A field body carries no statement about allowed values — a constraint with a name gets reused; one written inline gets copied. See [06 — Data Types](06-data-types.md).

## Zone 2: Flags

```skema
id       int  [pk];
publicId uuid [unique];
email    varchar [required, column="email_address"];
customer >Customer.id [onDelete=cascade];
created  timestamp [default=`now()`];
```

| Flag | Effect |
|---|---|
| `pk` | primary key |
| `unique` | uniqueness constraint |
| `required` | `NOT NULL` |
| `optional` | `NULL` allowed — the default, written explicitly |
| `default=…` | default value; a raw expression in backticks is allowed |
| `column="…"` | physical column name — see [11 — Physical column name](11-physical-mapping.md#physical-column-name) |
| `onDelete=…` | delete behavior of a relationship — see [07 — Relationships](07-relationships.md) |
| `onUpdate=…` | update behavior of a relationship |

A flag without a value means "true": `[pk]` is shorthand for `[pk=true]`. The common case stays one word; the exceptional case gets a value.

### Nullability

Optionality is **always a flag**, never a type suffix. The default follows SQL: **without an annotation, a field allows `NULL`.**

```skema
name  varchar(80) [required];   # NOT NULL
notiz varchar;                  # NULL allowed
notiz varchar [optional];       # same, written explicitly
```

There is no project-wide setting that flips this default — the same line means the same thing in every project, regardless of `mode`. Nullability is never controlled through `Lint`: a lint switch governs warnings, not the meaning of a field.

### pk implies required and unique

`[pk]` alone is sufficient — a primary key is always mandatory and always unique. Writing `[pk, required, unique]` is not wrong, just redundant.

## Zone 3: Body

```skema
sum float [required] {
  note          = "The full sum";
  exampleValue  = "19.99";
  label         = "Sum";
  "x-customkey" = "Foobar";
}
```

A flat set of key/value pairs, nothing else. Flags are optional too, so a
field with only a body needs no `[]` at all:

```skema
sum float { label = "Sum"; }
```

| Key | Meaning |
|---|---|
| `note` | description — becomes hover text, DDL comment, documentation |
| `exampleValue` | sample value for test data and documentation |
| `label` | text for humans — dropdowns, generated forms, diagrams |

Custom keys follow the `x-` rule from [01 — Custom keys](01-language-basics.md#custom-keys).

## Worked example

```skema
Partial common_base {
  id         int       [pk];
  public_id  uuid      [unique];
  created_at timestamp [required, default=`now()`];
}

Table Order {
  use common_base;

  customer     >Customer.id [required, onDelete=cascade];
  order_status OrderStatus;

  sum float [required] {
    note         = "The full sum";
    exampleValue = "19.99";
  }

  gross_sum decimal(10, 2);

  Database { table = "order"; }

  Meta {
    note         = "";
    styleClasses = [highlight];
  }
}
```

`use common_base` is explained in [09 — Partials](09-partials.md); `customer >Customer.id` in [07 — Relationships](07-relationships.md); `Database { table = "order"; }` in [11 — Physical Mapping](11-physical-mapping.md).

## See also

- [06 — Data Types](06-data-types.md)
- [07 — Relationships](07-relationships.md)
- [10 — Indexes](10-indexes.md)
- [11 — Physical Mapping](11-physical-mapping.md)
