# 10 — Offene Entwurfsfragen

Gesammelte Fragen aus allen Kapiteln, sortiert nach Tragweite. Was hier steht,
ist noch **nicht** entschieden — Kapitel 01 bis 09 enthalten jeweils am Ende
die Liste dessen, was entschieden ist.

## A — Tragen Folgen für das erzeugte Schema

Diese Fragen ändern, was am Ende in der Datenbank steht. Sie sollten vor dem
ersten Generator beantwortet sein.

| # | Frage | Kapitel |
|---|---|---|
| A1 | Physischer Name der Fremdschlüsselspalte: `customer` oder `customer_id`? | 05.7 |
| A2 | Physischer Spaltenname am Feld — Flag `[column="…"]`? | 07.5 |
| A3 | Zusammengesetzte Primärschlüssel: `[pk]` an zwei Feldern oder eigener Block? | 04.10 |
| A4 | Bestätigung: `pk` impliziert `required` und `unique` | 04.8 |
| A5 | Physische Abbildung eines `Enum` — nativer Typ, `varchar`+`CHECK`, `int`? | 06.5 |
| A6 | Darf `Field.type` auf einen anderen `Datatype` zeigen? (Schichtung, Zyklenprüfung) | 09.11 |
| A7 | Index-Typen (`btree`, `gin`), Teilindizes, Ausdrucksindizes | 07.6 |
| A8 | Namensschema für automatisch erzeugte Index-Namen | 07.6 |
| A9 | Indexnamen aus `Partial`s, die in vielen Tabellen landen — sie kollidieren | 06.5 |
| A10 | **Welche Typen bringt die Sprache mit?** siehe unten | 09.11 |

### A10 im Einzelnen — die größte verbliebene Lücke

Kapitel 02 nennt als einfache Typen `string`, `int`, `float`, `bool`. Beide
Beispiele verwenden aber selbstverständlich auch `varchar(120)`, `text`,
`uuid`, `date` und `timestamp` — ohne dass irgendwo stünde, woher die kommen.

Drei mögliche Antworten:

1. **Eingebaut.** Die Sprache liefert einen festen Satz mit — etwa `string`,
   `text`, `varchar(n)`, `int`, `bigint`, `float`, `decimal(p,s)`, `bool`,
   `date`, `time`, `timestamp`, `uuid`, `json`, `blob`. Bequem, aber jede
   Ergänzung ist eine Sprachänderung.
2. **Als Standardbibliothek.** Ein mitgeliefertes `std.skema` voller
   `Datatype`-Deklarationen, das implizit importiert wird. Die Sprache bleibt
   klein, der Satz ist les- und kopierbar, und ein Projekt kann ihn
   überschreiben oder ergänzen. Das passt zu dem Grundsatz aus 09.7: möglichst
   wenige eingebaute Typen.
3. **Gar nichts.** Jedes Projekt deklariert alles selbst. Maximal sauber,
   praktisch unbrauchbar — niemand will `Datatype uuid` schreiben.

Variante 2 ist die einzige, die sowohl den Grundsatz hält als auch benutzbar
ist. Sie braucht dann aber eine Antwort darauf, wie `std` importiert wird und
ob sein Namensraum ohne Präfix sichtbar ist — was der Regel aus 02.4
widerspräche.

## B — Betreffen die Sprachoberfläche

Beeinflussen, wie sich die Sprache schreibt und liest, aber nicht das Ergebnis.

| # | Frage | Kapitel |
|---|---|---|
| B1 | Darf `[optional]` geschrieben werden, obwohl es die Vorgabe ist? | 04.7 |
| B2 | Darf `Meta` im Feld-Body stehen, oder ist der Body selbst schon Metadaten? | 04.5 |
| B3 | Hat ein Enum-Eintrag einen Metadaten-Body oder einen `Meta`-Block? | 06.5 |
| B4 | `Meta` in allen Deklarationen erlaubt? | 03.7 |
| B5 | Dürfen Style-Klassen auch im `Document` definiert werden? | 03.7 |
| B6 | Selektoren `@field` und `@relation` aufnehmen? | 03.7 |
| B7 | Muss ein `<`-Rückverweis auf eine bestehende `>`-Beziehung zeigen? | 05.9 |
| B8 | Müssen Parameter mit `default` am Ende der Signatur stehen? | 09.11 |
| B9 | Schreibweise von Typnamen — `decimal` klein, `Email` groß? | 09.8 |
| B10 | Zulässige Werte für `Field.type` im `Enum` — nur `string` und `int`? | 06.5 |
| B11 | Bestätigung: `Group` ist rein visuell und erzeugt kein Schema | 08.10 |

## C — Betreffen Werkzeuge

| # | Frage | Kapitel |
|---|---|---|
| C1 | Muss der Language Server die Wurzeldatei über eine Editor-Einstellung erfahren? | 02.9 |
| C2 | Schweregrad-Syntax für Lint-Regeln — `fieldNaming = snake_case [error];`? | 02.9 |
| C3 | Vollständige Liste der Lint-Regeln | 02.9 |
| C4 | Welche Eigenschaften gehören in den Style-Kernsatz? | 03.7 |
| C5 | Sollen gruppenlose Tabellen einen Lint-Hinweis erzeugen? | 08.10 |
| C6 | Soll der Formatter die drei Kommentarformen vereinheitlichen? | 01.2 |
| C7 | Abgrenzung „portabel präziser" Typen bei `strict` + `generic` | 02.9 |
| C8 | Welche Basistypen sind eingebaut — nur `string`, `int`, `float`, `bool`? | 09.11 |

## D — Ideen ohne Entscheidung

### D1 — Ein `Object`-Schlüsselwort

`docs/beschreibung.txt` führte die Schlüsselwortliste mit einem
`( Object? )` — als Frage notiert, nie ausgeführt.

Denkbar wäre damit ein Modellobjekt, das **keine Tabelle** ist: ein
zusammengesetzter Wert, ein JSON-Dokument innerhalb einer Spalte, oder eine
Sicht (`VIEW`). Alle drei sind reale Bedürfnisse, aber sie sind
unterschiedlich genug, dass ein einzelnes Schlüsselwort sie kaum abdeckt.

Vor einer Entscheidung wäre zu klären, welches der drei gemeint war.

### D2 — Sichten und materialisierte Sichten

Nicht Teil des bisherigen Entwurfs, aber in jedem realen Schema vorhanden.
Eine Sicht ist keine Tabelle: sie hat keine eigenen Daten, keine
Primärschlüssel und ihr Inhalt ist ein SQL-Ausdruck. Ob die Sprache das
tragen soll, ist eine Grundsatzfrage — sie würde damit von einer
Modellierungs- zu einer Schemasprache.

### D3 — Migrationen

Die Sprache beschreibt einen Zustand, keinen Übergang. Sobald Modelle
produktiv verwendet werden, stellt sich die Frage nach Umbenennungen: eine
Tabelle, die von `Kunde` zu `Customer` wird, ist für einen naiven Vergleich
eine gelöschte und eine neue.

Ein stabiler Bezeichner je Deklaration — etwa `Meta { "x-id" = "…" }` — wäre
die kleinste Vorkehrung, um das später überhaupt lösen zu können.

## E — Bekannte Ungereimtheiten im Bestand

| Wo | Was |
|---|---|
| `README.md` | nennt `examples/shop.erp` und `docs/grammar.md`; beides existiert nicht |
| `README.md` | „Bekannte Baustellen" verweist auf `--` in Beziehungen — die Pfeile sind inzwischen `>`, `<`, `-` |
| `docs/beschreibung.txt` | vollständig durch `docs/00`–`10` abgelöst; kann entfallen |
