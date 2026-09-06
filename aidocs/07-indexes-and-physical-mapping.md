# 07 — Index and Physical Mapping

> Status: **Draft, in progress.** Sections marked ⚠️ are not yet decided.

## 7.1 The problem

A model has two names for everything: the **business** name and the
**physical** name.

```
Table Order          # business:  Order
                     # physical:  "orders" in schema "sales"
```

There are good reasons why they diverge: the database is old, another
team named it, `Order` is a reserved word in SQL, or house rules demand
plural forms. The language has to be able to carry both, without making
the model ugly.

Four things need physical names:

| Level | Example |
|---|---|
| Database | `shopsystem` |
| Schema | `public`, `sales` |
| Table | `orders` |
| Column | `email_address` |

## 7.2 The current state is contradictory

`examples/myexample.skema` solves the same problem two different ways:

```
Table Customer {
  Database {
    name            = "shopsystem";
    postgres_schema = "public";
  }
}

Table Order {
  Meta {
    dbName = "order";
  }
}
```

Three problems:

1. **`dbName` sits in `Meta`**, and thereby breaks the fundamental rule
   of the language: `Meta` describes, it changes no schema. The physical
   table name absolutely does change the generated DDL.
2. **Two mechanisms for one thing.** Anyone learning the language has to
   guess which one applies.
3. **`postgres_schema`** carries the dialect in its key name —
   snake_case, dialect-bound, and unrelated to the `Dialect` block that
   `Datatype` already uses for exactly this purpose. Moreover, a schema
   is not a Postgres special case: Oracle and SQL Server have it too.

And an ambiguous reading: does `Database { name = "shopsystem" }`
**inside** `Table Customer` mean the database name or the table name? By
its value, the database is meant — but then the statement sits at the
wrong level, since it hardly applies only to this one table.

## 7.3 The `Database` block

`Database` is the **only** place for physical names. `Meta.dbName` is
dropped without replacement.

### Keys

| Key | Meaning | allowed in |
|---|---|---|
| `database` | name of the database | `Project`, `Document` |
| `schema` | name of the schema | `Project`, `Document`, `Table` |
| `table` | physical table name | `Table` |

Every key means the same thing at every level. A single `name` that
meant the database in `Project` and the table in `Table` would be
shorter — but it would be the exact mistake that `spec` deliberately
avoids relative to `version`: a key whose meaning depends on its
position.

### Three levels, one override rule

```
# ---- project.skema ----
Project {
  Database {
    database = "shopsystem";
    schema   = "public";
  }
}

# ---- order.skema ----
Document shop {
  Database { schema = "sales"; }     # applies to every table in this file
}

Table Order {
  Database { table = "orders"; }     # -> shopsystem.sales.orders
}

Table Position {
  Database { schema = "archiv"; }    # -> shopsystem.archiv.Position
}

Table Note { }                       # -> shopsystem.sales.Note
```

The cascade is the same as with `Style`: **general is overridden by
specific.** `Project` sets the default, `Document` refines it per file,
`Table` refines it per case.

What's not specified is derived from the business name: `Table Note`
without a `table` key is physically `Note`.

> The practical payoff lies at the `Document` level: a file usually maps
> to exactly one schema. Without this level, `schema = "sales"` would
> have to appear on every single table in the file.

The physical schema is **not** derived automatically from the document
name. Namespace and schema are two different things, and separating them
again later would be expensive — anyone who wants them to match writes
one line.

## 7.4 Dialect-specific settings

Some settings apply to only one engine: `TABLESPACE` in Postgres,
`ENGINE` and `CHARSET` in MySQL. They go into a **named `Dialect`
block** — the name is the engine:

```
Table Order {
  Database {
    table = "orders";

    Dialect postgres { tablespace = "fast_ssd"; }
    Dialect mysql    { engine = "InnoDB"; charset = "utf8mb4"; }
  }
}
```

The same form applies in `Datatype`, so that the whole language has
exactly one notation for "this only applies to engine X":

```
Datatype decimal(precision, scale) {
  Dialect postgres { sql = "numeric({precision}, {scale})"; }
  Dialect mysql    { sql = "DECIMAL({precision}, {scale})"; }
}
```

The key is called `sql`, because that's exactly what it contains: this
engine's SQL type expression. A neutral name like `template` would
describe the mechanism (substituting placeholders) instead of the thing
itself. And `sql` leaves room for further per-engine keys — details in
`09-datatypes.md`.

The payoff is twofold: dialect-bound settings are visibly separated from
the portable ones — anyone switching dialects immediately sees what
doesn't come along — and the language has one fewer pattern to learn.

⚠️ Open: engine names are built-in identifiers of the language. Per the
rule from `03` (`@` marks what's built in), it would have to be
`Dialect @postgres`. See question 2.

## 7.5 Physical column names ⚠️

A field has no sub-blocks besides its `{}` attribute block — and that
one is, per the basic rule, reserved for descriptions. The physical
column name, however, changes DDL and thus belongs in square brackets:

```
Table Customer {
  email varchar [column="email_address"];
}
```

This preserves the language's rule at every level: `[]` generates DDL.
That tables use a block and fields use a flag follows from the fact that
tables carry several physical facts (database, schema, name) and fields
carry exactly one.

## 7.6 `Index`

The `Index` block collects a table's indexes. Each entry has the form:

```
[name] (column, …) [flags];
```

The name is **optional** and comes first — as everywhere in the language
where something gets a name:

```
Table Customer {
  email  varchar;
  customer   int;
  order_date date;

  Index {
    (email) [unique];                        # unnamed
    ix_customer_order_date (customer, order_date) [unique];  # named
  }
}
```

**Why naming pays off:** without one, the generator creates a name.
If a column later changes, the generated name changes with it — and the
migration drops one index and creates an apparently new one, even though
nothing changed from a business perspective. A set name keeps the index
stable over time.

### Order matters

For composite indexes, `(customer, order_date)` is **not** the same as
`(order_date, customer)`. The index serves queries that start with the first
column. The language does not reorder them, nor does the formatter.

### `[unique]` exists twice

```
publicId uuid [unique];      # on the field
Index { (publicId) [unique]; }   # in the block
```

Both generate the same DDL. That's intentional: the shorthand on the
field for the normal case of a single column, the block for anything
composite and anything named. Writing both at once is a lint hint, not
an error.

### ⚠️ Still open

- **Index types** — `[type=btree]`, `[type=gin]`, `[type=hash]`.
  Dialect-bound; does this belong in the flag list or in a `Dialect`
  block?
- **Partial indexes** — `WHERE deleted_at IS NULL`. Conceivable as a raw
  expression: `[where=`deleted_at IS NULL`]`.
- **Expression indexes** — `(lower(email))`. Needs raw expressions in the
  column list.
- **Naming scheme for generated names** — by what pattern does the
  generator name an unnamed index?

## 7.7 Decided

- `Database` is the only place for physical names; `Meta.dbName` is
  dropped (7.3)
- Keys `database`, `schema`, `table` — same meaning at every level (7.3)
- Three levels: `Project`, `Document`, `Table`, specific overrides
  general (7.3)
- The schema is **not** derived automatically from the document name (7.3)
- Dialect-specific settings go into named `Dialect <engine>` blocks —
  also in `Datatype` (7.4)
- In `Datatype`, the type expression is called `sql` (7.4)
- Index entries carry an optional name up front (7.6)

## 7.8 Open questions in this chapter

1. Physical column name on the field — as a flag `[column="…"]`? (7.5)
2. `@` before engine names: `Dialect @postgres` instead of
   `Dialect postgres`? (7.4)
3. Index types, partial indexes, expression indexes (7.6)
4. Naming scheme for automatically generated index names (7.6)
