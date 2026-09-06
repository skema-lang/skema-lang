# 00 — Übersicht

erplang ist eine Sprache zur Beschreibung von Entity-Relationship-Modellen.
Ein Modell wird einmal geschrieben und dient danach als Diagramm, als
Dokumentation und als Vorlage für DDL in verschiedenen Datenbanken.

Dateiendung: **`.skema`**

## Die kleinste vollständige Datei

```
spec 1;

Document blog { }

Table Post {
  id    int          [pk];
  title varchar(200) [required];
}
```

## Drei Anweisungsarten, mehr nicht

Wer diese drei Formen kennt, kann jede Zeile der Sprache lesen — auch eine,
die er noch nie gesehen hat.

| Art | Form | Beispiele |
|---|---|---|
| **Deklaration** | `Name Typ [Flags] { Body }` | `Table Order { … }` · `email varchar [required] { … }` |
| **Zuweisung** | `schlüssel = wert;` | `mode = draft;` · `note = "…";` |
| **Direktive** | `lowercase …;` | `spec 1;` · `import "…" as base;` · `use Base;` |

Bei einer Deklaration darf jeder Teil fehlen, die **Reihenfolge nie**.

## Die zwei Leitregeln

**1. Drei Zonen, drei Fragen.** Jede Eigenschaft eines Feldes hat genau einen
richtigen Platz:

```
username Username [required] { note = "Loginname"; }
         └─ Typ ─┘ └─Flags─┘ └────── Body ───────┘
```

| Zone | Frage |
|---|---|
| Typ | *Welche Werte sind erlaubt?* |
| `[…]` | *Welche Rolle spielt das Feld im Schema?* |
| `{…}` | *Was muss ein Mensch darüber wissen?* |

**2. `@` markiert Eingebautes.** Ein Name mit `@` gehört der Sprache, ein Name
ohne `@` gehört dem Nutzer:

```
Style @table   { … }    # eingebauter Elementtyp
Style highlight { … }   # selbst vergebene Klasse
Dialect @postgres { … } # eingebaute Engine
```

## Schlüsselwörter

### Direktiven — kleingeschrieben

| Wort | Bedeutung |
|---|---|
| `spec` | Sprachversion; erste Zeile der Datei |
| `import` … `as` | Datei einbinden, optional umbenennen |
| `use` | `Partial` an dieser Stelle einfügen |

### Deklarationen — PascalCase, tragen einen Namen

| Wort | Bedeutung | Kapitel |
|---|---|---|
| `Project` | Projekteinstellungen; **genau einmal**, in der Wurzeldatei | 02 |
| `Document` | Namensraum der Datei; **genau einmal je Datei** | 02 |
| `Table` | Tabelle | 04 |
| `Enum` | abgeschlossener Wertevorrat | 06 |
| `Partial` | wiederverwendbare Feldgruppe | 06 |
| `Group` | visuelle Zusammenstellung | 08 |
| `Datatype` | eigener Datentyp | 09 |

### Einstellungsblöcke — PascalCase, ohne Namen

| Wort | Steht in | Bedeutung | Kapitel |
|---|---|---|---|
| `Meta` | jeder Deklaration | Autor, Beschreibung, Stand | 03 |
| `Style` | `Project`, `Document` | Aussehen | 03 |
| `Lint` | `Project` | Konventionen | 02 |
| `Database` | `Project`, `Document`, `Table` | physische Namen | 07 |
| `Dialect @…` | `Database`, `Datatype` | engine-spezifisches | 07, 09 |
| `Index` | `Table`, `Partial` | Indizes | 07 |
| `Field` | `Datatype`, `Enum` | Verhalten eines Feldes dieses Typs | 09, 06 |

## Beziehungen

> **Der Pfeil zeigt dahin, wo es genau einen gibt.**

| Schreibweise | Bedeutung | Spalte entsteht |
|---|---|---|
| `a >B.b` | viele `A` gehören zu einem `B` | in `A` |
| `a <B.b` | ein `A` hat viele `B` | in `B` |
| `a -B.b` | eins zu eins | in `A` |

Für viele-zu-viele gibt es keinen Pfeil — dafür schreibt man die
Zwischentabelle hin. Siehe Kapitel 05.

## Kapitel

| Datei | Inhalt |
|---|---|
| [01](01-lexik-und-dateiaufbau.md) | Tokens, Kommentare, Zeichenketten, `spec`, Imports |
| [02](02-project-document-namespaces.md) | `Project`, `Document`, Namensräume, `mode`, `Lint` |
| [03](03-meta-und-style.md) | `Meta`, `Style`, Selektoren, Klassen, Kaskade |
| [04](04-table-und-felder.md) | Tabellen, Felder, die drei Zonen, Flags |
| [05](05-relationen.md) | Pfeile, Kardinalität, `onDelete` |
| [06](06-enum-und-partial.md) | `Enum`, `Partial`, `use` |
| [07](07-index-und-physisches-mapping.md) | `Database`, `Dialect`, `Index` |
| [08](08-group.md) | `Group` |
| [09](09-datatype.md) | `Datatype`, Parameter, Dialekte |
| [10](10-offene-entwurfsfragen.md) | Was noch nicht entschieden ist |

## Beispiele

| Datei | Zeigt |
|---|---|
| `examples/myexample.skema` | alle Konstrukte in einer Datei |
| `examples/generic/` | mehrdateiliges Projekt mit Namensräumen und Imports |

Das Verzeichnis `examples/generic/` ist der bessere Einstieg: `project.skema`
ist die Wurzeldatei, `types.skema` enthält Bausteine, `catalog.skema` und
`lending.skema` das eigentliche Modell.
