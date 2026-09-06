# 08 — Group

> Status: **Draft.** Sections marked ⚠️ are not yet decided.

## 8.1 Current state

```
Group Shop {
  Table shop.User;
  Table shop.Order;
  Enum  shop.OrderStatus;
}
```

A `Group` bundles declarations together — in the diagram as a bordered
area, in the documentation as a chapter.

## 8.2 ⚠️ The boundary is still fuzzy

The language now has **three** ways of bundling things together. Without
a clear distinction, every time you'll have to guess which one is meant:

| Construct | Bundles | Affects |
|---|---|---|
| `Document` | everything in a file | namespace |
| `Database { schema }` | tables of a schema | physical storage |
| `Group` | freely chosen elements | ⚠️ ? |

`Document` and `Database` are settled (see `02` and `07`). For `Group`
it needs to be stated whether it's **purely visual** or also generates
something.

The clean answer is presumably: **purely visual and documentary.**
Everything else is already spoken for — the namespace belongs to
`Document`, physical storage belongs to `Database`. What's left is
exactly what the other two can't do: a collection **across** files and
schemas, purely for understanding.

This also explains why `Group` is needed at all: a model with 60 tables
is unreadable as a diagram. Groups are the means to break it into
graspable parts — "order management", "master data", "billing" — without
having to align the file layout to match.

## 8.3 ⚠️ Error in the example

```
Group Shop {
  Table shop.User;        # doesn't exist
  Table shop.Order;
  Enum  shop.OrderStatus;
}
```

`examples/myexample.skema` declares `Table Customer` and `Table Order`,
but no `Table User`. The reference points into nothing.

This should be exactly a lint error (`danglingRef`, see `02`, section
2.7). A group pointing at something nonexistent is the most common decay
in maintained models: the table gets renamed, the group stays as is.

Also: per `02`, section 2.4, the prefix within the same file is
optional. `Table Order;` suffices.

## 8.4 The kind is stated alongside — and must be correct

```
Group OrderProcessing {
  Table Order;
  Table OrderPosition;
  Enum  OrderStatus;
}
```

The keyword is a reading aid: in a list of twenty entries, you can tell
tables from enums at a glance.

But it's **not mere decoration** — it's checked:

```
Enum Order;      # ERROR, if Order is a Table
```

This lets the group catch two errors instead of one: the wrong name and
the wrong kind.

### One name per namespace

A `Table` and an `Enum` may **not** carry the same name within the same
namespace.

```
Enum  Status { … }
Table Status { … }     # ERROR: name already taken
```

`Status` as both a table and an enum side by side would be confusing for
humans too — and without this rule, every reference in the model would
have to carry its kind along with it.

## 8.5 The group names its members

Grouping happens **from the group's side**, not from the element's:

```
Group Abrechnung {
  Table Invoice;
  Table billing.Payment;
}
```

A single glance shows what belongs together — and that's the whole
point. The opposite direction (`Table Order { Meta { group = … } }`)
would be more convenient to write, but then there'd be no single place
where a group is visible as a whole.

The cost is known and bearable: creating a new table means remembering
to add the entry. A lint hint on ungrouped tables catches this.

## 8.6 One table, one group

Groups **do not overlap** and **do not nest**.

```
Group Stammdaten { Table Customer; }
Group Abrechnung { Table Customer; }   # ERROR: already in Stammdaten
```

This is deliberately strict. The reason is the rendering: a box can't sit
inside two frames. A language that lets you express something that can't
be drawn just shifts the problem onto the renderer — which then solves
it somehow.

A table may also belong to **no** group at all. Groups are a tool for
large models, not a mandatory chore.

## 8.7 `Meta` and styles

A `Group` is a declaration like any other and may carry `Meta` (see
`03`):

```
Group OrderProcessing {
  Table Order;
  Table OrderPosition;

  Meta {
    note         = "Everything related to order processing";
    styleClasses = [highlight];
  }
}
```

The `@group` selector styles all groups (see `03`, section 3.2).

## 8.8 Groups across file boundaries

Since a group may cut across the file layout, members from imported
files are the normal case, not the exception:

```
Group Abrechnung {
  Table Invoice;
  Table billing.Payment;
  Table billing.Dunning;
}
```

This is exactly the reason `Group` exists alongside `Document`.
Otherwise the file would already be the group.

## 8.9 Decided

- The kind stands before the reference and is checked (8.4)
- One name per namespace, regardless of kind (8.4)
- The group names its members (8.5)
- No overlap, no nesting (8.6)

## 8.10 Open questions in this chapter

1. Confirmation: `Group` is purely visual and generates no schema (8.2)
2. Should ungrouped tables produce a lint hint? (8.5)
