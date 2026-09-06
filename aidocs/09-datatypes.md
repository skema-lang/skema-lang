# 09 — Datatype

> Status: **Draft.** Sections marked ⚠️ are not yet decided.

## 9.1 What it's for

`Datatype` is the place where a project defines its own types. That
covers three needs that would otherwise be scattered throughout the
model:

1. **Business names** — `Email`, `Money`, `Username` instead of
   `varchar` plus a regex next to it.
2. **Constraints in one place** — the regex is written once, not in five
   tables.
3. **Portability** — the same type is generated differently per engine,
   the model stays the same.

Following the decision from `04`, it is additionally the **only** place
for value-range constraints. Nothing about that remains in the field.

## 9.2 Structure

```
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

A `Datatype` consists of at most four parts:

| Part | Job |
|---|---|
| Signature | name and typed parameters |
| Parameter bodies | description and default per parameter |
| `Field` | how a field of this type behaves — independent of any database |
| `Dialect @engine` | how the type is generated in a particular engine |

All four follow the basic shape of the language, `Name Type [Flags] {
Body }` (see `04`, section 4.1). There's no special rule to learn here.

## 9.3 Parameters

Parameters are **declared like fields**: name, then type.

```
Datatype decimal(precision int, scale int) { … }
```

That makes `decimal("abc", 2)` an error, rather than a mystery only
discovered in the generated SQL. The editor can show what's expected at
the call site.

Every parameter may get its own body — the same shape as a field:

```
precision { note = "Total number of digits"; }
scale     { note = "Fixed number of decimal places"; default = 2; }
```

| Key | Meaning |
|---|---|
| `note` | description; appears in the editor when the type is invoked |
| `default` | default value if the argument is omitted |

A `default` makes arguments optional: with the definition above,
`decimal(10)` is valid and means `decimal(10, 2)`.

> Why not `precision = "Total count"` as before: throughout the
> language, `key = value` means the key **has** this value. A
> description at this position would reinterpret that — and
> `default = 12` in the old `File` block was unresolvable for the same
> reason: default for `precision`, or for every field of this type? The
> dedicated body answers that unambiguously.

⚠️ Open: Must parameters with `default` appear at the end of the
signature, so that `decimal(10)` unambiguously omits `scale`? Otherwise
named arguments like `decimal(scale=4)` are needed.

## 9.4 The `Field` block

Describes the type **at the model level** — what a field of this type
is, before any database enters the picture. This is also what applies in
`draft` mode and in the JSON export.

| Key | Meaning |
|---|---|
| `type` | base type: `string`, `int`, `float`, `bool` |
| `default` | default value for fields of this type |
| `regex` | constraint on the value range, descriptive (see `04`) |
| `note` | description of the type |

The same block also appears in `Enum` (see `06`, section 6.1) and
answers the same question there. Anyone who has read one can read the
other.

### `default` in two places — and why that's not a contradiction

```
Datatype decimal(precision int, scale int) {
  scale { default = 2; }        # default for the ARGUMENT
  Field { default = 0; }        # default for the FIELD
}
```

`decimal(10)` means `decimal(10, 2)`. A field `betrag decimal(10)`
without its own value gets `0`.

These two meanings were exactly what couldn't be distinguished in the
original draft: `default = 12` sat unlabeled in the `File` block, while
the parameters lay next to it as loose assignments. Thanks to the
parameter bodies, every `default` now has an unambiguous referent.

> On naming: `Field` here, inside `Datatype`, means "the field's view of
> this type" — not a field of a table. When writing the reference docs,
> this distinction should be called out explicitly, or it will be
> misread.

⚠️ Open: May `type` point at another `Datatype`? That would allow
layering — `Money` building on `decimal(19,4)`. Powerful, but it needs
cycle checking.

## 9.5 Dialects

```
Dialect @postgres { sql = "numeric({precision}, {scale})"; }
Dialect @mysql    { sql = "DECIMAL({precision}, {scale})"; }
```

`sql` contains this engine's type expression.

### Missing the matching dialect

If `dialect = oracledb` is set and there's no `Dialect @oracledb` block:

| `mode` | Behavior |
|---|---|
| `draft` | `Field.type` is used, with a hint |
| `strict` | error — the type is not defined for this target |

This fits the mode logic from `02`: `draft` sketches, `strict` gives
guarantees. A mode switch surfaces gaps instead of hiding them.

## 9.6 Substituting parameters

`{name}` is replaced by the argument — **but only if `name` is a
declared parameter of this `Datatype`.** Everything else is left
unchanged.

```
Datatype ShortCode {
  Field { regex = "^[a-z]{3,20}$"; }    # {3,20} stays as it is
}
```

This means there's **no escaping** and no collision with regex
quantifiers: `3,20` isn't a valid parameter name, so nothing happens.
The rule can be explained in one sentence and works the same way in
`sql`, `regex`, and every other value.

The one cost: a typo like `{precission}` stays silently unchanged
instead of complaining. That's what the linter is for — if a `{…}`
looks like an identifier but isn't a parameter, it produces a warning.

## 9.7 What this makes possible

Because parameters are typed and can flow into any value, the language
doesn't need `regexstring` as a built-in type — every project can build
it itself:

```
Datatype regexstring(pattern string) {
  pattern { note = "Pattern the value must satisfy"; }

  Field {
    type  = string;
    regex = "{pattern}";
  }
}
```

And on top of that, the project's own business types:

```
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

This is the yardstick the design should be measured against: **the
language should need as few built-in types as possible, because the
project can build its own.**

## 9.8 ⚠️ Casing of type names

`examples/myexample.skema` writes `decimal` lowercase, `Email` uppercase
in other examples. Both are self-defined types, so the `@` rule doesn't
apply — but there should still be one consistent line.

Likely candidate: built-in base types lowercase (`string`, `int`,
`float`, `bool`), project-defined types PascalCase (`Email`, `Money`) —
enforced via a `typeNaming` rule in the `Lint` block. `decimal` would
remain a borderline case: it mimics SQL, but is self-defined.

## 9.9 Corrections to the existing example

For `examples/myexample.skema`, lines 96–110:

| Line | Finding |
|---|---|
| 96 | `comma` is called `scale` in SQL |
| 96 | parameter untyped |
| 97–98 | descriptions written as assignments |
| 100 | `File` → `Field` |
| 102 | `default = 12` couldn't be attributed — argument or field? See 9.4 |
| 107–108 | `comma` without curly braces, not substituted |
| 107–108 | `numeric` lowercase, `DECIMAL` uppercase — inconsistent |
| 106 | `Dialect { postgres = … }` → `Dialect @postgres { sql = … }` |

## 9.10 Decided

- `File` is called `Field` (9.4)
- Parameters are declared with types and have their own body (9.3)
- `{name}` is only substituted for declared parameters, no escaping (9.6)
- Missing dialect: hint in `draft`, error in `strict` (9.5)

## 9.11 Open questions in this chapter

1. Must parameters with `default` appear at the end of the signature? (9.3)
2. May `Field.type` point at another `Datatype`? (9.4)
3. Casing of type names (9.8)
4. Which base types are built in — only `string`, `int`, `float`, `bool`?
