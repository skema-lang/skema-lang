# 06 — Data Types

[← Index](README.md)

## Built-in types

Under `mode = draft`, four simple types are available: `string`, `int`, `float`, `bool` (see [02 — The mode switch](02-project-and-documents.md#the-mode-switch)).

Under `mode = strict`, more precise, portable types are expected. The example corpus uses `varchar(n)`, `text`, `uuid`, `date`, and `timestamp` alongside `Datatype`-based types for anything domain-specific. The exact catalog of built-in precise types is not fully finalized — see [Reference → Known limitations](13-reference.md#known-limitations).

Neither list is enforced per mode today: every known type is accepted under both (see [02 — The mode switch](02-project-and-documents.md#the-mode-switch)).

## Datatype — defining your own types

`Datatype` is where a project defines its own types. It covers three needs:

1. **Domain names** — `Email`, `Money`, `Username` instead of `varchar` plus a regex next to it.
2. **A single point of constraint** — the regex is written once, not copied into five tables.
3. **Portability** — the same type is generated differently per engine; the model stays the same.

It is also the **only** place value-range constraints belong — see [05 — Regexes live in Datatype, not in a field](05-tables-and-fields.md#regexes-live-in-datatype-not-in-a-field).

## Anatomy

```skema
Datatype decimal(precision int, scale int) {
  precision { note = "Total number of digits"; }
  scale     { note = "Fixed number of decimal places"; default = 2; }

  Field {
    type = float;
    note = "Fixed-precision decimal number";
  }

  Dialect @postgres { sql = "numeric({precision}, {scale})"; }
  Dialect @mysql    { sql = "DECIMAL({precision}, {scale})"; }
}
```

A `Datatype` has up to four parts, all following the [one declaration shape](01-language-basics.md#the-one-declaration-shape):

| Part | Role |
|---|---|
| Signature | name and typed parameters |
| Parameter bodies | description and default per parameter |
| `Field` | how a field of this type behaves, independent of any database |
| `Dialect @engine` | how the type is generated for one specific engine |

## Parameters

Parameters are declared like fields — name, then type:

```skema
Datatype decimal(precision int, scale int) { … }
```

Parameters are what makes `decimal` a type of the project, not of the language: `decimal` is nowhere in the built-in list above — a model has it because a `Datatype` block declares it, as [`examples/generic/types.skema`](../../examples/generic/types.skema) does. Its parameters are then part of its contract, so `decimal("abc", 2)` is a type error at the model level rather than a surprise discovered in the generated SQL.

Each parameter may carry its own body:

```skema
precision { note = "Total number of digits"; }
scale     { note = "Fixed number of decimal places"; default = 2; }
```

| Key | Meaning |
|---|---|
| `note` | description, shown at the call site |
| `default` | value used when the argument is omitted |

A `default` makes an argument optional: with the definition above, `decimal(10)` is valid and means `decimal(10, 2)`.

## The Field block

Describes the type **at the model level** — what a field of this type is before any database is involved, and what a target without its own `Dialect` block falls back to. The same block appears in `Enum` — see [08 — Enums](08-enums.md#the-field-block).

| Key | Meaning |
|---|---|
| `type` | what this type is built on: a built-in type, or another `Datatype` — see [Layering types](#layering-types) |
| `default` | default value for fields of this type |
| `regex` | value-range constraint, descriptive — see [05](05-tables-and-fields.md#regexes-are-strings-not-raw-expressions) |
| `note` | description of the type |

### default in two places

```skema
Datatype decimal(precision int, scale int) {
  scale { default = 2; }        # default for the ARGUMENT
  Field { default = 0; }        # default for the FIELD
}
```

`decimal(10)` means `decimal(10, 2)`. A field `amount decimal(10)` with no value of its own gets `0`. Each `default` has an unambiguous referent because it sits in its own body.

## Dialect @engine blocks

```skema
Dialect @postgres { sql = "numeric({precision}, {scale})"; }
Dialect @mysql    { sql = "DECIMAL({precision}, {scale})"; }
```

`sql` holds the type expression for that engine. The `@` prefix follows the [at sign convention](01-language-basics.md#the-at-sign-convention): the engine name is a built-in identifier of the language, not one the project invents.

This is the general shape of a `Dialect` block. [11 — Physical Mapping](11-physical-mapping.md) reuses it for schema/table-level engine settings (tablespace, storage engine, charset).

### Missing dialect

If `dialect = oracledb` is set and no `Dialect @oracledb` block exists:

| `mode` | Behavior |
|---|---|
| `draft` | falls back to `Field.type`, with a hint |
| `strict` | error — the type is not defined for this target |

## Parameter substitution

`{name}` is replaced by the argument **only if `name` is a declared parameter of this `Datatype`.** Anything else is left untouched:

```skema
Datatype ShortCode {
  Field { regex = "^[a-z]{3,20}$"; }    # {3,20} is untouched — not a parameter name
}
```

No escaping is needed, and there is no collision with regex quantifiers: `3,20` is not a valid parameter name, so nothing is substituted.

## Layering types

A `Datatype` may build on another:

```skema
Datatype Money {
  Field {
    type = decimal(10);
    note = "Amount in the home currency";
  }
}
```

## Building your own primitives

Because parameters are typed and flow into every value, the language does not need a built-in `regexstring` type — a project builds it itself:

```skema
Datatype regexstring(pattern string) {
  pattern { note = "Pattern the value must match"; }

  Field {
    type  = string;
    regex = "{pattern}";
  }
}
```

And domain types on top of it:

```skema
Datatype Email {
  Field {
    type  = string;
    regex = "^[^@]+@[^@]+$";
    note  = "Email address, not checked for deliverability";
  }
}

Table User {
  email Email [required, unique];
}
```

Fewer built-in types, more project-defined ones — this is the type system's guiding principle.

## See also

- [05 — Tables & Fields](05-tables-and-fields.md) — the three zones a type fits into
- [08 — Enums](08-enums.md) — the same `Field` block, applied to a closed set of values
- [11 — Physical Mapping](11-physical-mapping.md) — `Dialect` blocks for schema/table settings
