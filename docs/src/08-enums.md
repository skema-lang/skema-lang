# 08 — Enums

[← Index](README.md)

`Enum` declares a closed set of values.

```skema
Enum OrderStatus {
  Field {
    type    = string;
    default = paid;
    note    = "States an order can be in";
  }

  draft     { label = "Draft"; }
  paid      { label = "Paid"; }
  shipped   { label = "Shipped"; value = "S"; }
  cancelled;
}
```

## The Field block

Identical in shape to the `Field` block in [`Datatype`](06-data-types.md#the-field-block) — it answers the same question: *how does a field of this type behave?*

| Key | Meaning |
|---|---|
| `type` | stored type: `string` or `int` |
| `default` | default value for fields of this enum type |
| `note` | description of the enum |

`default` names one of the enum's own entries, written bare (`default = paid;`, never `OrderStatus.paid`).

`type` doubles as the physical storage decision: `string` stores readable values, `int` stores positions.

## Entries

Each entry follows the [one declaration shape](01-language-basics.md#the-one-declaration-shape): a name, optionally a body. An entry with nothing to add is just a name and a semicolon.

| Key | Meaning |
|---|---|
| `label` | text **for humans** — dropdowns, reports, diagrams |
| `value` | the value actually **stored** |

Both are optional; without them, the entry name is used for both. They answer different questions and are kept separate:

```skema
shipped { label = "Shipped"; value = "S"; }
```

Displayed as "Shipped", stored as `"S"`, referenced in the model as `shipped`. This three-way split is what a legacy system's short codes need.

## Using an enum

```skema
Table Order {
  status      OrderStatus;                             # uses Field.default
  prev_status OrderStatus [default=OrderStatus.draft];  # overrides it
}
```

The enum name is written where a data type would go. A default at the field level uses the full path `OrderStatus.draft`, never a bare `draft` — otherwise it is ambiguous whether a string literal or an enum member is meant.

## Order is significant

Entry order is part of the declaration. Diagrams, dropdowns, and — when `type = int` — the stored numbers all follow it. Tooling does not reorder entries.

Inserting an entry in the middle of an `int`-backed enum shifts every following stored integer — treat that as a migration, not a plain model edit.

## Meta on Enum

Allowed like on any declaration (see [03 — Meta & Style](03-meta-and-style.md)):

```skema
Enum OrderStatus {
  draft;
  paid;

  Meta {
    author = "anna";
    note   = "Agreed with sales";
  }
}
```

Per-entry metadata (an author on a single member) is not yet specified — see [Reference → Known limitations](13-reference.md#known-limitations).

## See also

- [06 — Data Types](06-data-types.md) — `Datatype`, the other consumer of the `Field` block
- [05 — Tables & Fields](05-tables-and-fields.md) — using an enum as a field type
