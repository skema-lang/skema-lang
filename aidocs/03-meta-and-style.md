# 03 — Meta and Style

> Status: **Draft.** Sections marked ⚠️ are not yet decided.

Both blocks belong to the `{}` side of the language: they describe,
they generate no schema (see `01`, section 1.7).

- **`Meta`** — knowledge *about* a declaration: who, why, what state.
- **`Style`** — appearance in the diagram.

## 3.1 `Meta`

```
Table Customer {
  id int [pk];

  Meta {
    author       = "anna";
    note         = "Customer master data, GDPR-relevant";
    version      = "1.2";
    styleClasses = [highlight];
    "x-jira"     = "SHOP-412";
  }
}
```

### Standard keys

| Key | Type | Meaning |
|---|---|---|
| `author` | string | Who designed this? For collaboration and follow-up questions. |
| `note` | string | Business description. Target for editor hover text and comments in generated DDL. |
| `version` | string | Editing state, freely chosen (see `01`, section 1.9). |
| `styleClasses` | list | Assignment of style classes, see 3.4. |

### Custom keys

`Meta` is **extensible, but not arbitrary**: custom keys must carry the
`x-` prefix and be quoted.

```
Meta {
  author    = "anna";        # known
  "x-owner" = "team-billing"; # custom, passed through
  owner     = "team-billing"; # ERROR: unknown key without x-
}
```

This makes a typo like `athor` an error instead of a silent no-op, while
still leaving room for project-specific information. Custom keys land
unchanged in the JSON export and can be used by custom generators.

See `01`, section 1.11 — the same rule applies in `Style` and in field
attribute blocks.

### Where `Meta` may appear ⚠️

In the example, `Meta` appears in `Project`, `Document`, and `Table`;
`description.txt` additionally shows it in `Datatype`.

Proposal: **`Meta` is allowed anywhere a named declaration appears** —
`Project`, `Document`, `Table`, `Partial`, `Enum`, `Group`, `Datatype`.
An exception list would not be justifiable; every declaration can use
an author and a description.

### ⚠️ Rule violation: `dbName` in `Meta`

`examples/myexample.skema` places the physical table name in the `Meta`
block on line 86:

```
Table Order {
  Meta {
    dbName = "order";
  }
}
```

This contradicts the central rule of the language. `dbName` **changes
the generated DDL** — it decides what the table is called in the
database. That means it doesn't belong in a block whose promise is "I
only describe."

To make matters worse: the same file solves the same problem for
`Table Customer`, on lines 60–63, via a completely different mechanism:

```
Table Customer {
  Database {
    name            = "shopsystem";
    postgres_schema = "public";
  }
}
```

Two paths for one thing, and one of them breaks the fundamental rule.
This is resolved in `07-indexes-and-physical-mapping.md`.

## 3.2 `Style` — two kinds of rules

There are exactly two forms, and `@` distinguishes them:

```
Style @table   { … }    # selector: applies to ALL tables
Style highlight { … }   # class:    applies to assigned elements
```

> **`@` means: built-in element type of the language. No `@` means:
> self-assigned name.**

The same rule later carries further extensions as well, without needing
another special character.

### Selectors

| Selector | Effect |
|---|---|
| `@project` | framing conditions of the whole diagram |
| `@document` | all elements of this file |
| `@table` | table boxes |
| `@enum` | enum boxes |
| `@group` | group frames |
| `@field` ⚠️ | field rows — e.g. highlighting key fields |
| `@relation` ⚠️ | relationship lines — color, line style, arrowhead shape |

`@field` and `@relation` do not yet appear in `description.txt`.
`@relation` is the most important addition: in an ER diagram, the lines
carry half the information — cardinality and delete behavior. Without a
selector, it can't be styled.

## 3.3 Style classes

A class is a `Style` block **without** `@`. It only takes effect once
it's assigned to an element via `Meta.styleClasses`.

```
Project {
  Style @table {
    headerBackgroundColor = "#cc0000";
    headerTextColor       = "white";
  }

  Style highlight {
    borderColor = "#cc0000";
    borderWidth = "3px";
  }
}

Table Order {
  Meta { styleClasses = [highlight]; }
}
```

This closes the gap `examples/myexample.skema` currently has: `highlight`
is assigned there three times but never declared.

⚠️ Open: May classes also be defined in `Document`, or only in `Project`?
For the cascade in 3.4, either would be workable; the simpler answer is
"`Project` only", analogous to `Lint`.

## 3.4 Cascade: what wins?

There are **no** styles on the individual element. Anyone who wants to
mark up a table defines a class and assigns it.

This is a deliberate restriction — the same lesson HTML learned from
inline styles: as soon as styling is allowed to be scattered through the
model, it gets scattered. This keeps model files free of appearance, and
everything visual lives in one place.

The cascade therefore has exactly three tiers, from weak to strong:

1. `Style @…` in `Project` — project-wide default
2. `Style @…` in `Document` — this file's default
3. Classes from `styleClasses`, in list order; the last one wins

Rule: **more specific beats more general, later beats earlier.** No
weighting arithmetic like CSS has — nobody understands that after two
weeks.

## 3.5 Style properties

There is a **standardized core set**, which is checked and
autocompleted, and `x-` for everything else:

```
Style @table {
  headerBackgroundColor = "#cc0000";   # known, checked
  bgColor               = "#fff";      # ERROR: unknown, no x-
  "x-mermaidClass"      = "important"; # free, passed through
}
```

Without this separation, `bgColor` instead of `backgroundColor` would
remain silently ineffective — the most unpleasant kind of error, because
it looks like a renderer problem.

⚠️ Open: the core set itself. At minimum to clarify: colors (background,
text, border), font (size, weight), border (width, radius, style), lines
for `@relation` (color, style, arrowhead shape), and spacing.

## 3.6 Decided

- `Style` without `@` is a class, with `@` an element-type selector (3.2, 3.3)
- No styles on individual elements; a three-tier cascade (3.4)
- Style properties: core set plus `x-` (3.5)
- `Meta`: known keys plus `x-` (3.1)

## 3.7 Open questions in this chapter

1. Add selectors `@field` and `@relation`? (3.2)
2. May style classes also be defined in `Document`? (3.3)
3. Which properties belong in the core set? (3.5)
4. `Meta` allowed on all declarations? (3.1)
5. Where does `dbName` go? (3.1) → decided in `07`
