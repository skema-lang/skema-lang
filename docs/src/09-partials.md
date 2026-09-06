# 09 — Partials

[← Index](README.md)

`Partial` declares a reusable group of fields.

```skema
Partial common_base {
  id         int       [pk];
  public_id  uuid      [unique];
  created_at timestamp [required, default=`now()`];
}

Table Customer {
  use common_base;
  email varchar [required];
}
```

`Table Customer` now has four fields. A `Partial` never becomes a table itself — it is a snippet collection, not a model object, and it does not appear in the diagram.

## use inserts at its position

Where the `use` line sits determines where the fields land:

```skema
Table Customer {
  use common_base;      # id, public_id, created_at
  email varchar;        # after
}

Table Product {
  sku varchar;          # first
  use common_base;      # after
}
```

Field order matters — it is visible in the generated table and the diagram. Multiple `use` statements in one table are allowed and applied in order.

## Across files

```skema
use base.timestamps;
```

A `Partial` from an imported file is referenced with its prefix, exactly like any other declaration (see [02 — Referencing within vs. across files](02-project-and-documents.md#referencing-within-vs-across-files)).

## Naming conflicts are errors

```skema
Partial common_base { id int [pk]; }

Table Customer {
  use common_base;
  id uuid [pk];        # ERROR: id already comes from common_base
}
```

There is no silent overwrite. To deviate, use a different `Partial` or drop the `use` — both are visible in the source. The same error occurs with two `use` statements that bring in the same field name.

## What a Partial may contain

| Content | Allowed | Note |
|---|---|---|
| Fields | ✓ | the normal case |
| Relationships | ✓ | creates a relationship in every table that uses it |
| `Index` | ✓ | a `Partial` with `created_at` can bring its index along |
| `Meta` | ✓ | describes the `Partial` itself; is **not** inherited by the user |
| `use` | ✓ | `Partial` inside `Partial`, cycle-checked |
| `Database` | — | a `Partial` has no physical location of its own |

A `Partial` is almost a table — just without existence of its own. This lets building blocks be layered:

```skema
Partial Identity {
  id        int  [pk];
  public_id uuid [unique];
}

Partial Timestamps {
  created_at timestamp [required, default=`now()`];
  updated_at timestamp;
}

Partial Base {
  use Identity;
  use Timestamps;
}
```

From [`examples/generic/types.skema`](../../examples/generic/types.skema); consumed as `use types.Base;` throughout the example project.

> An index defined inside a `Partial` currently gets the same name wherever it is used — reused index names across tables collide unless qualified per table. See [10 — Indexes](10-indexes.md#indexes-inside-a-partial) and [Reference → Known limitations](13-reference.md#known-limitations).

## See also

- [05 — Tables & Fields](05-tables-and-fields.md) — the fields a `Partial` is made of
- [02 — Project & Documents](02-project-and-documents.md) — `use` vs. `import`, not the same mechanism
