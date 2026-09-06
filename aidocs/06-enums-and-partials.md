# 06 — Enum and Partial

> Status: **Draft.** Sections marked ⚠️ are not yet decided.

Two declarations that seem, at first glance, to have nothing to do with
each other. They're grouped here because both pursue the same goal:
**define something once, use it many times.**

## 6.1 `Enum` — a closed set of values

```
Enum OrderStatus {
  Field {
    type    = string;
    default = paid;
    note    = "States of an order";
  }

  draft     { label = "Draft"; }
  paid      { label = "Paid"; }
  shipped   { label = "Shipped"; value = "S"; }
  cancelled;
}
```

An `Enum` consists of two parts: a `Field` block describing how a field
of this type behaves, and the list of entries.

### The `Field` block — the same as in `Datatype`

This is no coincidence and no duplication: `Field` answers the same
question in both cases — *how does a field of this type behave?* Anyone
who has understood `Datatype` can read `Enum` immediately (see `09`,
section 9.4).

| Key | Meaning |
|---|---|
| `type` | stored type: `string` or `int` |
| `default` | default value for fields of this type |
| `note` | description of the enum |

`type` answers the question of physical mapping almost on its own:
`string` stores the values in readable form, `int` stores positions.

### The entries

Each entry follows the basic shape of the language (`04`, section 4.1):
a name, optionally a body. Anyone with nothing to add writes just the
name and a semicolon.

| Key | Meaning |
|---|---|
| `label` | text **for humans** — select lists, reports, diagrams |
| `value` | value that is **actually stored** |

Both are optional. Without a statement, the entry name applies to both.
They answer two different questions and are therefore kept separate:

```
shipped { label = "Shipped"; value = "S"; }
```

What's displayed is "Shipped", what's stored is `"S"`, and in the model
it's called `shipped`. This exact three-way split is needed as soon as a
legacy application expects short codes.

### Usage

```
Table Order {
  status OrderStatus;                            # uses Field.default
  vorstatus OrderStatus [default=OrderStatus.draft];   # overrides
}
```

The enum name stands in the position of the data type. A default value
on a field is given via the full path `OrderStatus.draft` — not as a
bare `draft`, or it wouldn't be recognizable whether a string or an enum
entry is meant.

### Order matters

The order of the entries is part of the declaration. Diagrams, select
lists, and — with `type = int` — the stored numbers depend on it. The
formatter does not reorder them.

> Consequence for `type = int`: inserting an entry in the middle shifts
> all subsequent values. That's a migration, not a model change — the
> reference docs should explicitly warn about this.

### `Meta` on Enum

`Meta` is allowed as with any declaration (see `03`):

```
Enum OrderStatus {
  draft;
  paid;

  Meta {
    author = "anna";
    note   = "Signed off by sales";
  }
}
```

⚠️ Open: May a single **entry** have a `Meta`, or does its body already
count as a metadata bag — like the field does (see `04`, section 4.5)?
For consistency, the latter is favored.

## 6.2 `Partial` — reusable field groups

```
Partial common_base {
  id         int       [pk];
  publicId   uuid      [unique];
  created_at timestamp [required, default=`now()`];
}

Table Customer {
  use common_base;
  email varchar [required];
}
```

`Table Customer` now has four fields. A `Partial` **never becomes a
table itself** — it's a snippet collection, not a model object. It
doesn't appear in the diagram.

The benefit is concrete: timestamps, technical keys, and tenant fields
repeat in almost every table. Declared once, used everywhere, and a
change takes effect immediately across the whole model.

### `use` inserts at its position

The position of the `use` line determines where the fields land:

```
Table Customer {
  use common_base;      # id, publicId, created_at
  email varchar;        # after
}

Table Product {
  sku varchar;          # first
  use common_base;      # after
}
```

This matters because field order is visible in the generated table and
in the diagram. A rule like "partial fields always come first" would be
shorter, but would get in the way exactly when it's needed most.

Multiple `use` statements in one table are allowed and are inserted in
order.

### Across file boundaries

```
use base.timestamps;
```

A `Partial` from an imported file is addressed with a prefix, like any
other declaration (see `02`, section 2.4).

### Naming conflicts are an error

```
Partial common_base { id int [pk]; }

Table Customer {
  use common_base;
  id uuid [pk];        # ERROR: id already comes from common_base
}
```

There is **no** silent overwrite. Anyone wanting to deviate uses a
different `Partial` or drops the `use` — both are visible in the source.

Reason: an overridden field isn't visible from looking at the `Partial`.
You read `use common_base`, think you know what the table contains, and
you're wrong. This kind of error is only noticed during migration.

The same case occurs with two `use` statements that bring in the same
field name — that's also an error.

### What may go into a `Partial`

| Content | allowed | Note |
|---|---|---|
| Fields | ✓ | the normal case |
| Relationships | ✓ | creates a relationship in every table that uses it |
| `Index` | ✓ | a `Partial` with `created_at` can bring its index along |
| `Meta` | ✓ | describes the `Partial` itself, is **not** inherited by the user |
| `use` | ✓ | `Partial` inside `Partial`, with cycle checking |
| `Database` | — | a `Partial` has no physical location of its own |

This makes a `Partial` almost a table — just without existence of its
own. That pays off: building blocks can be layered.

```
Partial Timestamps {
  created_at timestamp [required, default=`now()`];
  updated_at timestamp;

  Index { ix_created (created_at); }
}

Partial Auditable {
  use Timestamps;
  created_by >User.id;
}
```

⚠️ Open: relationships inside `Partial`s need a rule for index names. If
`ix_created` gets inserted into ten tables, ten identically-named indexes
collide — unless the name is qualified per table.

## 6.3 ⚠️ Casing

The example writes `common_base` in snake_case, while all other
declarations are PascalCase (`Customer`, `OrderStatus`). Enum entries are
lowercase (`draft`, `paid`).

Since naming conventions are a project matter (see `01`, section 1.8),
this belongs in the `Lint` block — but the shipped examples should model
one consistent style.

## 6.4 Decided

- `Enum` has a `Field` block with `type`, `default`, `note` — the same
  block as in `Datatype` (6.1)
- Entries have `label` (for humans) and `value` (stored) (6.1)
- Field conflicts from `use` are an error, not an overwrite (6.2)
- A `Partial` may contain fields, relationships, `Index`, `Meta`, and
  `use` (6.2)

## 6.5 Open questions in this chapter

1. Physical mapping of an enum via `Dialect` blocks — deferred (6.1)
2. Does an enum entry have a metadata body or a `Meta` block? (6.1)
3. Index names from `Partial`s that land in many tables (6.2)
4. Allowed values for `Field.type` in `Enum` — only `string` and `int`?
