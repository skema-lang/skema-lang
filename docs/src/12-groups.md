# 12 — Groups

[← Index](README.md)

`Group` collects declarations for the diagram and the documentation — a bordered area in the diagram, a chapter in the docs.

```skema
Group OrderProcessing {
  Table Order;
  Table OrderPosition;
  Enum  OrderStatus;
}
```

`Group` is purely visual and documentary: it does not affect the namespace (that is `Document`'s role, see [02](02-project-and-documents.md)) and does not affect physical placement (that is `Database`'s role, see [11](11-physical-mapping.md)). It exists for what those two cannot do: a cross-cutting view for human understanding, independent of file layout or schema.

## The kind is checked

The keyword before each reference is not decoration — it is validated against the declaration's actual kind:

```skema
Enum Order;      # ERROR if Order is a Table
```

A `Group` catches two mistakes instead of one: a wrong name and a wrong kind. This also means a `Table` and an `Enum` can never share a name in the same namespace, independent of any `Group`.

## The group names its members

Grouping happens **from the group**, not from the element:

```skema
Group Billing {
  Table Invoice;
  Table payments.Payment;
}
```

One place shows what belongs together — that's the point. The reverse — assigning a group from inside a table's own `Meta` — would be more convenient to write, but would leave no single place where a group is visible as a whole.

Within the same file, the namespace prefix is optional (see [02 — Referencing within vs. across files](02-project-and-documents.md#referencing-within-vs-across-files)) — `Table Order;` is enough there.

## One table, one group

Groups do not overlap and do not nest:

```skema
Group MasterData { Table Customer; }
Group Billing    { Table Customer; }   # ERROR: already in MasterData
```

A table may also belong to **no** group — groups are a tool for large models, not a mandatory step.

## Meta and styling

`Group` accepts `Meta` like any declaration, and the `@group` selector in `Style` styles every group frame (see [03 — Meta & Style](03-meta-and-style.md)):

```skema
Group OrderProcessing {
  Table Order;
  Table OrderPosition;

  Meta {
    note         = "Everything around order processing";
    styleClasses = [highlight];
  }
}
```

## Groups across files

Because a group can cut across file boundaries, members from imported files are the normal case, not the exception:

```skema
Group Catalog {
  Table Author;
  Table Book;
  Table Copy;
  Enum  CopyState;
}
```

From [`examples/generic/catalog.skema`](../../examples/generic/catalog.skema).

## See also

- [03 — Meta & Style](03-meta-and-style.md) — the `@group` selector
- [02 — Project & Documents](02-project-and-documents.md) — `Document` as the namespace-level grouping
