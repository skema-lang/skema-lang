# 04 — Table und Felder

> Status: **Entwurf.** Abschnitte mit ⚠️ sind noch nicht entschieden.

## 4.1 Die eine Grundform

Bevor es um Felder geht, lohnt eine Beobachtung über die ganze Sprache. Fast
alles in erplang hat dieselbe Gestalt:

```
Name  Typ  [Flags]  { Body }
```

| Beispiel | Name | Typ | Flags | Body |
|---|---|---|---|---|
| `Table Order { … }` | `Order` | — | — | ✓ |
| `email varchar [required] { … }` | `email` | `varchar` | ✓ | ✓ |
| `customer >Customer.id [onDelete=CASCADE];` | `customer` | `>Customer.id` | ✓ | — |
| `ix_email (email) [unique];` | `ix_email` | `(email)` | ✓ | — |
| `Style @table { … }` | `@table` | — | — | ✓ |
| `Dialect @postgres { … }` | `@postgres` | — | — | ✓ |
| `Datatype decimal(p, s) { … }` | `decimal(p, s)` | — | — | ✓ |
| `draft { label = "Draft" }` | `draft` | — | — | ✓ |

Jeder Teil darf fehlen, die **Reihenfolge nie**. Wer das einmal begriffen hat,
kann jede Zeile der Sprache lesen, auch eine, die er noch nie gesehen hat.

Daneben gibt es nur zwei weitere Anweisungsarten:

| Art | Form | Beispiele |
|---|---|---|
| Zuweisung | `schlüssel = wert;` | `mode = draft;` · `note = "…";` |
| Direktive | `lowercase …;` | `spec 1;` · `import "…" as base;` · `use common_base;` |

Drei Formen, mehr nicht. Das ist der Maßstab, an dem sich jede Erweiterung
messen lassen muss.

## 4.2 Das Feld: drei Zonen, drei Fragen

```
username  regexstring(`^[a-z0-9_]{3,20}$`)  [required]  { note = "Loginname"; }
          └──────────── Typ ─────────────┘  └─ Flags ─┘  └────── Body ──────┘
```

Jede Zone beantwortet genau **eine** Frage:

| Zone | Frage | Wirkt auf |
|---|---|---|
| **Typ** | *Welche Werte sind erlaubt?* | Wertebereich, Form, Genauigkeit |
| **`[…]`** | *Welche Rolle spielt das Feld im Schema?* | Schlüssel, Eindeutigkeit, Beziehungen, Vorgabewerte |
| **`{…}`** | *Was muss ein Mensch darüber wissen?* | Beschreibung, Beispiele, eigene Schlüssel |

Der Test bei jeder neuen Eigenschaft lautet damit: *Beschränkt sie den Wert?
Dann Typ. Beschreibt sie die Rolle in der Tabelle? Dann Flag. Erklärt sie
etwas für Menschen? Dann Body.*

Diese Aufteilung ist stärker als die ursprüngliche Zweiteilung „`[]` erzeugt
DDL, `{}` beschreibt". Sie erklärt zusätzlich, wohin Dinge wie `regex`,
Längenangaben und Genauigkeit gehören — nämlich in keine der beiden Klammern,
sondern in den Typ.

## 4.3 Zone 1 — Der Typ

### Einfache Typen

Im `draft`-Modus genügen `string`, `int`, `float`, `bool` (siehe `02`,
Abschnitt 2.7).

### Typen mit Argumenten

Genauigkeit, Länge und Form sind **Argumente des Typs**, keine Flags:

```
gross_sum  decimal(10, 2);
name       varchar(80);
username   regexstring(`^[a-z0-9_]{3,20}$`);
```

Der Gewinn ist Kompositionsfähigkeit: `decimal(10,2)` ist ein anderer Typ als
`decimal(19,4)`, nicht derselbe Typ mit anderen Flags. Und weil Typen in
`Datatype` selbst definiert werden (siehe `09`), kann jedes Projekt eigene
mitbringen, ohne die Sprache zu ändern.

### Regexes sind Zeichenketten mit Escapes

Regexes stehen in normalen Zeichenketten, Backslashes werden verdoppelt
(siehe `01`, Abschnitt 1.3):

```
regex = "^\\d{4}-\\d{2}$";
```

Backticks bleiben für SQL-Ausdrücke reserviert und werden **nicht** für
Regexes verwendet — sonst gäbe es zwei Roh-Text-Formen mit unterschiedlicher
Bedeutung.

### Regexes sind beschreibend, nicht erzwungen

Eine Regex erzeugt **kein** `CHECK` in der Datenbank. Sie speist
Formulargeneratoren, API-Schemata, Testdaten und die Dokumentation.

Das ist eine bewusste Zurückhaltung: nicht jede Engine kennt dieselbe
Regex-Syntax, und ein Modell, das in Postgres prüft und in MySQL nicht,
verspricht mehr als es hält. Die Sprache sagt lieber ehrlich, was sie
garantiert.

## 4.4 Zone 2 — Die Flags

```
id       int  [pk];
publicId uuid [unique];
email    varchar [required, column="email_address"];
customer >Customer.id [onDelete=CASCADE];
created  timestamp [default=`now()`];
```

| Flag | Wirkung |
|---|---|
| `pk` | Primärschlüssel |
| `unique` | eindeutig |
| `required` | nicht NULL |
| `default=…` | Vorgabewert; Raw-Ausdruck in Backticks erlaubt |
| `column="…"` | physischer Spaltenname (siehe `07`) |
| `onDelete=…` | Löschverhalten einer Beziehung (siehe `05`) |
| `onUpdate=…` | Änderungsverhalten einer Beziehung |

### Zur Sorge, `[]` sei „eigentlich eine Liste"

Dass in einer Liste teils blanke Wörter (`pk`) und teils Paare (`default=4`)
stehen, ist kein Entwurfsfehler. `[pk]` ist die Kurzform von `[pk=true]` — ein
Flag ohne Wert bedeutet „ja". Dasselbe Muster tragen HTML-Attribute,
Rust-Attribute und dbml seit Jahren. Es ist an dieser Stelle das Richtige:
der Normalfall bleibt ein Wort, der Sonderfall bekommt einen Wert.

⚠️ Zu klären bleibt die Schreibweise: `onDelete` (Beispiel, camelCase) gegen
`on_delete` (`docs/beschreibung.txt`, snake_case). Nach `01`, Abschnitt 1.8
sind eingebaute Schlüssel camelCase — also `onDelete`.

## 4.5 Zone 3 — Der Body

```
sum float [required] {
  note           = "Bruttosumme inklusive Versand";
  exampleValue   = "19.99";
  "x-customkey"  = "Foobar";
}
```

Ein flacher Satz Schlüssel-Wert-Paare, sonst nichts. Bekannte Schlüssel werden
geprüft, eigene tragen `x-` (siehe `01`, Abschnitt 1.11).

| Schlüssel | Bedeutung |
|---|---|
| `note` | Beschreibung; wird zu Hover-Text, DDL-Kommentar, Doku |
| `exampleValue` | Beispielwert für Testdaten und Dokumentation |

### `regex` gehört nicht hierher

`examples/myexample.skema` führt in Zeile 75 ein `regex = ""` im Feld-Body.
Das entfällt ersatzlos. Eine Regex beschränkt den **Wertebereich** und ist
damit Sache des Typs, nicht des Feldes:

```
# falsch
username varchar { regex = "^[a-z0-9_]+$"; }

# richtig
Datatype Username {
  Field { type = string; regex = "^[a-z0-9_]+$"; }
}

Table User { username Username; }
```

Damit gilt die Zonenregel ohne Ausnahme: **im Feld-Body steht kein einziges
Wort über erlaubte Werte.** Wer eine Beschränkung sucht, schaut in den Typ —
immer, ohne nachzudenken.

Der Nebeneffekt ist der eigentliche Gewinn: eine Beschränkung, die einen Namen
hat, wird wiederverwendet. Eine, die im Feld steht, wird kopiert.

⚠️ Offen: Darf `Meta` im Feld-Body stehen (für `author`)? Nach der Regel aus
`03` ist `Meta` der Ort für „wer und warum". Im Feld sind `note` und
`exampleValue` aber direkt notiert, ohne `Meta`-Umhüllung. Zwei Möglichkeiten:
den Feld-Body als eigenständigen kleinen `Meta`-Block auffassen (dann ist die
Ausnahme benannt) oder `Meta` auch dort zulassen (dann ist die Struktur
gleich, aber der Normalfall wird länger).

## 4.6 Wiederholung vermeiden: benannte Typen

Der größte Hebel gegen lange Feldzeilen liegt nicht in der Syntax des Feldes,
sondern darin, Beschränkungen **einmal** zu benennen statt sie zu wiederholen.
Das leistet `Datatype` (siehe `09`):

```
Datatype Email {
  Field {
    type  = string;
    regex = "^[^@]+@[^@]+$";
    note  = "E-Mail-Adresse, nicht auf Zustellbarkeit geprüft";
  }
}

Datatype Money {
  Field { type = float; }
  Dialect @postgres { sql = "numeric(19,4)"; }
  Dialect @mysql    { sql = "DECIMAL(19,4)"; }
}
```

An der Verwendungsstelle bleibt fast nichts übrig — und was übrig bleibt,
liest sich fachlich statt technisch:

```
Table User {
  id       int      [pk];
  email    Email    [required, unique];
  guthaben Money;
}
```

`Email` sagt einem Leser mehr als `varchar` plus eine Regex daneben. Eine
Änderung an der Regel wirkt überall, statt in fünf Dateien nachgezogen werden
zu müssen.

## 4.7 Nullability

Optionalität ist **immer ein Flag**, kein Typ-Suffix. Die Vorgabe ist die von
SQL: **ohne Angabe darf ein Feld NULL sein.**

```
name  varchar(80) [required];   # NOT NULL
notiz varchar;                  # NULL erlaubt
notiz varchar [optional];       # dasselbe, nur ausdrücklich
```

| Flag | Wirkung |
|---|---|
| `required` | NOT NULL |
| `optional` | NULL erlaubt — die Vorgabe, ausdrücklich hingeschrieben |

Es gibt **keine** projektweite Einstellung, die diese Vorgabe umdreht. Das ist
Absicht: dieselbe Zeile bedeutet in jedem Projekt dasselbe. Ein Modell, dessen
Felder je nach einer Einstellung an anderer Stelle pflichtig oder optional
sind, lässt sich nicht mehr lesen, ohne diese Einstellung zu kennen — und
schon gar nicht aus einer fremden Datei kopieren.

Besonders gilt: Eine solche Einstellung dürfte nie im `Lint`-Block stehen.
`mode = draft` ignoriert alle Lint-Regeln (siehe `02`, Abschnitt 2.7) — die
NULL-Vorgabe dort abzulegen hieße, dass das Umschalten von `draft` auf `strict`
die **Bedeutung jedes Feldes** ändert. Ein Schalter für Warnungen darf niemals
die Semantik des Modells verändern.

⚠️ Kleine offene Frage: `[optional]` hat keine Wirkung, es schreibt die
Vorgabe nur hin. Soll es erlaubt sein (Lesbarkeit) oder als überflüssig
gemeldet werden (weniger Rauschen)?

## 4.8 `pk` impliziert `required` und `unique`

`id int [pk, required]` in `examples/myexample.skema` enthält eine
Redundanz: ein Primärschlüssel ist immer pflichtig und immer eindeutig.

Vorschlag: `[pk]` genügt. Wer `required` oder `unique` zusätzlich schreibt,
bekommt einen Lint-Hinweis, keinen Fehler — die Angabe ist ja nicht falsch,
nur überflüssig.

## 4.9 Entschieden

- Drei Zonen: Typ = Wertebereich, `[]` = Rolle im Schema, `{}` = Menschenwissen (4.2)
- Regexes sind beschreibend und stehen ausschließlich im `Datatype` (4.3, 4.5)
- Nullability ist ein Flag; Vorgabe ist NULL wie in SQL (4.7)
- Das Gegenstück zu `[required]` heißt `[optional]` (4.7)

## 4.10 Offene Fragen dieses Kapitels

1. Darf `[optional]` geschrieben werden, obwohl es die Vorgabe ist? (4.7)
2. Darf `Meta` im Feld-Body stehen? (4.5)
3. Zusammengesetzte Primärschlüssel — `[pk]` an zwei Feldern oder eigener Block?
4. Bestätigung: `pk` impliziert `required` und `unique` (4.8)
