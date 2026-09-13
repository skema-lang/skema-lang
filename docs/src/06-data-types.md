# 06 — Data Types

[← Index](README.md)

## Built-in types

Four simple types — `string`, `int`, `float`, `bool` — are all `mode = draft` requires (see [02 — The mode switch](02-project-and-documents.md#the-mode-switch)). `draft` never rejects anything more specific, so all of the below are equally fine to write under `draft` too.

`mode = strict` requires a more precise, portable type instead: any other built-in, a custom `Enum`, or a `Datatype`. The dialect-independent precise catalog is exactly `varchar(n)`, `text`, `uuid`, `date`, and `timestamp` — a project needing a precise `int`/`float`/`bool` reaches for a `Datatype` (`Field { type = int; }` plus whatever constraint makes it precise) rather than a built-in that doesn't exist.

With `dialect = postgres`/`mysql`/`oracledb`, a project also gets that engine's own built-in type catalog — see [Per-dialect type catalog](#per-dialect-type-catalog) below. A type name outside the dialect-independent catalog and (when set) the active dialect's own catalog is always an error, the same as an unknown type under `dialect = generic`.

## Per-dialect type catalog

Each table below is the complete set of engine-native type names `dialect = <engine>` accepts, on top of the dialect-independent catalog above. Sourced from each engine's own current reference documentation (PostgreSQL's "Data Types" chapter, the MySQL 8.4 Reference Manual, the Oracle Database 23 SQL Language Reference). Engine mechanisms for defining new types — PostgreSQL's `CREATE TYPE` enums/composites/domains/custom ranges, `type[]` array syntax on any type — are deliberately excluded: `Enum` and `Datatype` already cover that need in Skema. Built-in *fixed* range types (`int4range` and friends) are catalog names, not a mechanism, so they are included.

A name marked † has no short native alias, so Skema gives it its own camelCase identifier (`doublePrecision` for `double precision`, and so on); a future DDL exporter maps it back to the real SQL keywords. Where the parenthesized alternate spelling is itself the engine's usual short form (`bigint`/`int8`, `timestampWithTimeZone`/`timestamptz`, …), both spellings are accepted.

### `postgres`

| Type name | Parameters | Native SQL | Notes |
|---|---|---|---|
| `bigint` (`int8`) | — | `bigint` | |
| `smallint` (`int2`) | — | `smallint` | |
| `bigserial` (`serial8`) | — | `bigserial` | auto-increment |
| `serial` (`serial4`) | — | `serial` | auto-increment |
| `smallserial` (`serial2`) | — | `smallserial` | auto-increment |
| `numeric` (`decimal`) | `(precision, scale)`, scale optional | `numeric(p,s)` | |
| `real` (`float4`) | — | `real` | |
| `doublePrecision`† (`float8`) | — | `double precision` | |
| `money` | — | `money` | |
| `char` (`character`) | `(n)` | `character(n)` | fixed-length |
| `bytea` | — | `bytea` | |
| `time` | `(p)` optional | `time(p)` | no time zone |
| `timeWithTimeZone`† (`timetz`) | `(p)` optional | `time(p) with time zone` | |
| `timestampWithTimeZone`† (`timestamptz`) | `(p)` optional | `timestamp(p) with time zone` | |
| `interval` | — | `interval` | field restriction (`year`, `day to second`, …) is a modifier, not a separate type name |
| `point`, `line`, `lseg`, `box`, `path`, `polygon`, `circle` | — | same | geometric |
| `inet`, `cidr`, `macaddr`, `macaddr8` | — | same | network address |
| `bit` | `(n)` | `bit(n)` | |
| `bitVarying`† (`varbit`) | `(n)` | `bit varying(n)` | |
| `tsvector`, `tsquery` | — | same | text search |
| `xml` | — | `xml` | |
| `jsonb`, `jsonpath` | — | same | (`json` is not built in; use `jsonb`) |
| `int4range`, `int8range`, `numrange`, `tsrange`, `tstzrange`, `daterange` | — | same | built-in range types only |
| `oid` | — | `oid` | object identifier |
| `pg_lsn` | — | `pg_lsn` | log sequence number |
| `pg_snapshot` | — | `pg_snapshot` | |

### `mysql`

| Type name | Parameters | Native SQL | Notes |
|---|---|---|---|
| `tinyint`, `smallint`, `mediumint`, `bigint` | `(M)` optional display width | same | (`int` is dialect-independent) |
| `decimal` (`numeric`) | `(precision, scale)`, scale optional | `decimal(M,D)` | |
| `double` | `(precision, scale)` optional | `double(M,D)` | distinct from the dialect-independent `float`, which maps to MySQL's own `float(M,D)` |
| `bit` | `(n)` | `bit(M)` | |
| `time` | `(fsp)` optional | `time(fsp)` | |
| `datetime` | `(fsp)` optional | `datetime(fsp)` | |
| `year` | — | `year` | |
| `char` | `(n)` | `char(M)` | fixed-length |
| `binary` | `(n)` | `binary(M)` | |
| `varbinary` | `(n)` | `varbinary(M)` | |
| `tinyblob`, `blob`, `mediumblob`, `longblob` | — | same | |
| `tinytext`, `mediumtext`, `longtext` | — | same | (`text` is dialect-independent) |
| `enum` | `(...)`, one or more string values | `enum(...)` | |
| `set` | `(...)`, one or more string values | `set(...)` | |
| `geometry`, `point`, `linestring`, `polygon`, `multipoint`, `multilinestring`, `multipolygon`, `multicurve`, `multisurface`, `geometrycollection` | — | same | spatial/GIS |
| `json` | — | `json` | |

### `oracledb`

| Type name | Parameters | Native SQL | Notes |
|---|---|---|---|
| `char` | `(size)` | `char(size)` | fixed-length |
| `nchar` | `(size)` optional | `nchar(size)` | Unicode |
| `varchar2` | `(size)` | `varchar2(size)` | |
| `nvarchar2` | `(size)` | `nvarchar2(size)` | Unicode |
| `number` | `(precision, scale)` optional | `number(p,s)` | |
| `float` | `(precision)` optional | `float(p)` | subtype of `number` |
| `binary_float`, `binary_double` | — | same | |
| `timestamp` | `(fsp)` optional | `timestamp(fsp)` | overrides the dialect-independent `timestamp` with this optional argument |
| `timestampWithTimeZone`† | `(fsp)` optional | `timestamp(fsp) with time zone` | |
| `timestampWithLocalTimeZone`† | `(fsp)` optional | `timestamp(fsp) with local time zone` | |
| `intervalYearToMonth`† | `(yearPrecision)` optional | `interval year(p) to month` | |
| `intervalDayToSecond`† | `(dayPrecision, fractionalSecondsPrecision)` optional | `interval day(p) to second(p)` | |
| `raw` | `(size)` | `raw(size)` | |
| `longRaw`† | — | `long raw` | deprecated by Oracle itself; included since documented |
| `rowid` | — | `rowid` | |
| `urowid` | `(size)` optional | `urowid(size)` | |
| `clob`, `nclob`, `blob`, `bfile` | — | same | large objects |
| `json` | — | `json` | |
| `vector` | `(dimension, format)` both optional | `vector(dim, fmt)` | Oracle 23ai+; `format` is one of `int8`/`float32`/`float64`/`binary` |
| `long` | — | `long` | deprecated by Oracle itself; included since documented |

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
