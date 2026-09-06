# 05 — Relationships

> Status: **Draft.** Sections marked ⚠️ are not yet decided.

## 5.1 Basic shape

A relationship is a field whose type is an arrow to another field:

```
Table Order {
  customer >Customer.id [onDelete=cascade];
}
```

This fits the basic shape of the language (`04`, section 4.1): `customer`
is the name, `>Customer.id` the type, `[…]` the flags.

The **data type is not stated** — it's derived from the target field. If
`Customer.id` is `int`, `Order.customer` is `int` too. That's correct:
both ends of a relationship must have the same type, and anything that
can be derived shouldn't have to be copied.

## 5.2 The arrows

> **The arrow points to the side where there is exactly one.**

That's the whole rule. Everything else follows from it.

| Notation | Meaning | Foreign key column |
|---|---|---|
| `a >B.b` | many `A` belong to one `B` | created in `A` |
| `a <B.b` | one `A` has many `B` | created in `B` |
| `a -B.b` | one to one | created in `A` |

```
Table Order {
  customer >Customer.id;      # many orders -> one customer
}
```

Because the arrow points at the "one" side, a `>` field always sits **on
the many side** — exactly where the column is also created. The line and
the column live in the same place. That's the reason for this reading,
and dbml handles it the same way.

> **Correction to `description.txt`:** there, `>` is labeled
> "OneToMany" and `<` "Many to One". Both are swapped. Applied to line 70
> of the example, an order would have many customers.

### Many-to-many doesn't exist as an arrow

`<>` is dropped without replacement. SQL has no native n:m relationship
— it always needs a junction table, and that's what you write:

```
Table OrderPosition {
  order   >Order.id;
  product >Product.id;
  quantity int;
  price    Money;
}
```

That's more honest than an arrow that silently creates a table nobody
named. And it pays off early: almost every n:m relationship eventually
gets an extra field — quantity, date, ordering. Anyone who has it
auto-generated then has to rewrite everything.

## 5.3 Not every relationship creates a column

This is the point most easily overlooked when reading, because every
arrow looks syntactically the same:

```
Table Order {
  customer >Customer.id;      # creates column Order.customer
}

Table Customer {
  orders   <Order.customer;   # creates NO column
}
```

A `<` field is a **backreference**: it names the reverse direction for
diagrams, ORMs, and documentation, but the foreign key lives on the
other side. Anyone looking for `orders` in the generated table won't
find it — and that's correct.

### One-to-one

`-` is written symmetrically, but the schema isn't: one of the two
tables gets the column plus `UNIQUE`. It's **the side the field is
written on** — the same rule as with `>`.

```
Table Profil {
  user -User.id;      # column Profil.user, UNIQUE
}
```

So the same rule of thumb holds for every arrow: **the column is created
wherever the line is written** — except `<`, which creates none.

## 5.4 Both sides may declare — flags only on one

`A.x <B.y` and `B.y >A.x` describe the same relationship. Both may
appear in the model; tooling recognizes them by the **column pair** and
draws one edge, not two.

To keep this from ever becoming a contradiction, a hard rule applies:

> **Flags are written exclusively on the `>` side.**

```
Table Order {
  customer >Customer.id [onDelete=cascade];    # behavior lives here
}

Table Customer {
  orders <Order.customer;                      # backreference only
  orders <Order.customer [onDelete=restrict];  # ERROR
}
```

Reason: `onDelete` describes what happens to the foreign key column.
That column exists exactly once, so its annotation may exist exactly
once too. If both sides were allowed, someone would have to keep two
places in sync — and eventually someone won't.

## 5.5 Flags on relationships

```
customer >Customer.id [onDelete=cascade, onUpdate=restrict];
```

| Flag | Values |
|---|---|
| `onDelete` | `cascade`, `restrict`, `setNull`, `setDefault`, `noAction` |
| `onUpdate` | the same |

**Values are lowercase**, like every value in the language
(`mode = draft`, `type = float`). The generator writes them uppercase in
the SQL output — that's its concern, not the model's.

### Two corrections to the current state

1. **`RESTRICTED` doesn't exist.** `description.txt` names this
   value; SQL has `RESTRICT`, so in this language it's `restrict`.
2. **`onDelete=CASCADE`** in the example becomes `onDelete=cascade`.

Key casing is also still inconsistent: `onDelete` in the example versus
`on_delete` in `description.txt`. Per `01`, section 1.8, built-in
keys are camelCase — so `onDelete`.

## 5.6 The target column is always named

```
customer >Customer.id;      # correct
customer >Customer;         # not allowed
```

A shorthand that silently substitutes the primary key would be shorter.
But it has a cost that only shows up when reading someone else's model:
you'd have to look up the target table to know what's being referenced.
A relationship is the point where two tables touch — being explicit
there is worth the three extra characters.

## 5.7 ⚠️ Physical name of the foreign key column

```
Table Order {
  customer >Customer.id;
}
```

Is the column named `customer` or `customer_id`? The language has to say
so, or every generator will do it differently. Three options: the field
name unchanged, an automatic suffix per a `Lint` or `Project` rule, or
explicitly via `[column="…"]` (see `07`).

The most honest option is the field name unchanged — anyone who wants
`customer_id` names the field that way.

## 5.8 Decided

- The arrow points to the one side; `>` sits on the many side (5.2)
- `<>` is dropped — n:m is written as its own table (5.2)
- `<` creates no column, it's a backreference (5.3)
- `-` creates the column wherever the field is written (5.3)
- Both sides may declare, flags only on the `>` side (5.4)
- Flag values are lowercase: `cascade`, `restrict`, `setNull` (5.5)
- The target column is always named, no shorthand (5.6)

## 5.9 Open questions in this chapter

1. Physical name of the foreign key column (5.7)
2. Must a `<` backreference point at an actually existing `>`
   relationship, or may it stand alone and establish the relationship
   itself?
