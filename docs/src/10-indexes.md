# 10 — Indexes

[← Index](README.md)

The `Index` block collects a table's indexes.

```skema
Table Order {
  order_no   varchar;
  customer   int;
  order_date date;

  Index {
    (order_no) [unique];                             # unnamed
    ix_customer_order_date (customer, order_date);   # named
  }
}
```

## Entry shape

```skema
(column, …) [flags];            # unnamed
name (column, …) [flags];       # named
```

The name is **optional** and comes first, like every named thing in the language.

Naming an index makes it stable across schema changes: an unnamed index gets a generator-assigned name, and if that name changes when a column changes, a migration drops and recreates an index that, in business terms, never changed. A deliberately chosen name avoids that churn.

## Column order is significant

`(customer, order_date)` is **not** the same as `(order_date, customer)` for a composite index — the index serves queries that start with the first column. The order is not automatically rearranged.

## unique exists in two places

```skema
publicId uuid [unique];          # at the field
Index { (publicId) [unique]; }   # in the block
```

Both generate the same result. The field-level shorthand covers the common single-column case; the block covers anything composite or explicitly named. Writing both at once is redundant but not an error.

## Composite index example

```skema
Table Book {
  isbn   types.Isbn   [required, unique];
  title  varchar(200) [required];
  author >Author.id   [required, onDelete=restrict];

  Index {
    ix_book_title        (title);
    ix_book_author_title (author, title);
  }
}
```

From [`examples/generic/catalog.skema`](../../examples/generic/catalog.skema).

## Indexes inside a Partial

An `Index` block is allowed inside a `Partial` (see [09 — What a Partial may contain](09-partials.md#what-a-partial-may-contain)) and is copied into every table that `use`s it. A named index defined this way currently keeps the same name in every consuming table — if two tables use the same `Partial`, their index names collide. Qualifying the name per table, or keeping composite/critical indexes out of shared `Partial`s, avoids the collision until this is resolved — see [Reference → Known limitations](13-reference.md#known-limitations).

## Not yet specified

Index types (`btree`, `gin`, `hash`), partial indexes (`WHERE …`), expression indexes (`(lower(email))`), and the naming scheme for automatically generated index names are not part of the language yet — see [Reference → Known limitations](13-reference.md#known-limitations).

## See also

- [05 — Tables & Fields](05-tables-and-fields.md) — the `[unique]` field flag
- [07 — Relationships](07-relationships.md) — indexing foreign key columns
