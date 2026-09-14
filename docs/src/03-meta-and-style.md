# 03 — Meta & Style

[← Index](README.md)

Both blocks belong to the `{}` side of the language ([01 — Brackets and braces](01-language-basics.md#brackets-and-braces)): they describe, they don't generate schema.

- **`Meta`** — knowledge *about* a declaration: who, why, what state.
- **`Style`** — appearance in the diagram.

## Meta

```skema
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
| `author` | string | Who designed this — for collaboration and questions. |
| `note` | string | Domain description. Feeds editor hover text and generated DDL comments. |
| `version` | string | The model's own revision — see [`spec` vs `Meta.version`](01-language-basics.md#spec-vs-metaversion). |
| `styleClasses` | list | Assigns style classes — see [Style classes](#style-classes) below. |

Custom keys follow the `x-` rule from [01 — Custom keys](01-language-basics.md#custom-keys).

### Where Meta is allowed

Inside any top-level declaration: `Project`, `Document`, `Table`, `Partial`, `Enum`, `Group`, `Datatype`. A field describes itself in its own body instead — see [05 — Zone 3: Body](05-tables-and-fields.md#zone-3-body).

## Style — two forms, marked by @

```skema
Style @table   { … }    # selector: applies to ALL tables
Style highlight { … }   # class: applies only where assigned
```

`@` means: built-in element type of the language. No `@` means: user-chosen name.

### Selectors

| Selector | Applies to |
|---|---|
| `@project` | the whole diagram's frame |
| `@document` | every element of one file |
| `@table` | table boxes |
| `@enum` | enum boxes |
| `@group` | group frames |
| `@relation` | relationship lines — color, line style, arrowhead |

### Style classes

A class is a `Style` block **without** `@`. It has no effect until assigned via `Meta.styleClasses`:

```skema
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

## Cascade

There is **no** styling directly on an individual element. To highlight one table, define a class and assign it — this keeps every visual decision in one place instead of scattered through the model.

Three cascade levels, weakest to strongest:

1. `Style @…` in `Project` — project-wide default
2. `Style @…` in `Document` — this file's override
3. Classes from `styleClasses`, in list order — the last one wins

Rule: **more specific beats more general, later beats earlier.** No specificity arithmetic to memorize.

```mermaid
flowchart TB
    A["Style @table in Project<br/>(project-wide default)"] --> B["Style @table in Document<br/>(this file's override)"]
    B --> C["Meta.styleClasses, in order<br/>(last class wins)"]
    C --> D["Rendered element"]
```

## Style properties

Property names are decided by `@skema/diagram`, not the language. The language validates only the *shape* of each entry:

- Unquoted keys must be camelCase — e.g., `headerBackgroundColor`, not `Header_Background_Color` or `HEADERBACKGROUNDCOLOR`.
- Quoted keys follow the `x-` custom-key rule (must start with `x-`; see [01 — Custom keys](01-language-basics.md#custom-keys)).
- Values must be strings — `"#cc0000"`, not `5` or `[a, b]`.

```skema
Style @table {
  headerBackgroundColor = "#cc0000";   # valid — camelCase key, string value
  "x-mermaidClass"      = "important"; # valid — custom key, string value
}
```

Examples of properties used in practice across the example corpus:

| Property | Used on |
|---|---|
| `headerBackgroundColor`, `headerTextColor` | `@table` |
| `backgroundColor` | `@document`, classes |
| `borderColor`, `borderWidth` | classes |
| `fontSize` | `@project` |
| `lineColor` | `@relation` |

## Full example

```skema
Project {
  Style @project  { fontSize = "1.1em"; }
  Style @table    { headerBackgroundColor = "#1f4e79"; headerTextColor = "white"; }
  Style @relation { lineColor = "#888888"; }

  # Classes carry no @ — they are assigned via Meta.styleClasses
  Style core       { borderColor = "#1f4e79"; borderWidth = "3px"; }
  Style historical { backgroundColor = "#f0f0f0"; }
}

Table Author {
  Meta { styleClasses = [core]; }
}

Table LoanArchive {
  Meta { styleClasses = [historical]; }
}
```

From [`examples/generic/`](../../examples/generic/).

## See also

- [12 — Groups](12-groups.md) — the `@group` selector's target
- [04 — Linting](04-linting.md) — the other project-wide governance block
- [11 — Physical Mapping](11-physical-mapping.md) — the same cascade pattern applied to physical names
