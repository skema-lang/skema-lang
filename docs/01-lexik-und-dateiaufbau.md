# 01 — Lexik und Dateiaufbau

> Status: **Entwurf.** Beschreibt den Ist-Zustand aus `examples/myexample.skema`.
> Abschnitte mit ⚠️ sind noch nicht entschieden, siehe `10-offene-entwurfsfragen.md`.

## 1.1 Was ist eine `.skema`-Datei?

Eine Datei beschreibt ein **Modell**: Tabellen, ihre Felder, ihre Beziehungen
zueinander sowie Metadaten für Darstellung und Codegenerierung. Sie ist
deklarativ und reihenfolgeunabhängig — bis auf den Dateikopf.

Der grobe Aufbau, von oben nach unten:

```
spec 1;                          # Sprachversion  (genau einmal, zuerst)
import "./common.erp" as base;   # Imports        (beliebig viele)

Project { … }                    # Projektweite Einstellungen
Document shop { … }              # Dokumentidentität
                                 # danach: Deklarationen in beliebiger Reihenfolge
Partial  common_base { … }
Enum     OrderStatus  { … }
Table    Customer     { … }
Group    Shop         { … }
Datatype decimal(…)   { … }
```

## 1.2 Kommentare

Es gibt drei Schreibweisen:

```
# Zeilenkommentar
// Zeilenkommentar

/* Blockkommentar,
   über mehrere Zeilen */
```

`#` und `//` sind gleichwertig. `#` ist die Schreibweise der bisherigen
Beispiele und passt zum Konfigurationscharakter der Sprache, `//` kommt aus
C-artigen Sprachen und ist die häufigste Fingergewohnheit.

Der Blockkommentar `/* … */` ist zum vorübergehenden Stilllegen ganzer
Deklarationen gedacht.

**Blockkommentare schachteln.** `/* /* … */ */` ist gültig; erst das
zugehörige schließende Zeichen beendet den Kommentar. Ohne Schachtelung würde
das Auskommentieren eines Bereichs, der schon einen Kommentar enthält, still
an der falschen Stelle enden — genau dann, wenn man es am wenigsten bemerkt.
Rust macht es so, C nicht.

### `#` in Zeichenketten

Innerhalb einer Zeichenkette leitet `#` **keinen** Kommentar ein. Das ist
kein Randfall, sondern der Normalfall dieser Sprache:

```
Style @table { headerBackgroundColor = "#cc0000"; }
```

### Kein Doc-Kommentar

Es gibt bewusst kein `///`. Beschreibungen gehören in `note = "…"` und damit
in den AST — von dort erreichen sie Hover-Texte im Editor, den JSON-Export und
Kommentare im generierten DDL. Ein Doc-Kommentar wäre ein zweiter Weg für
dieselbe Sache, und der schlechtere: Lexer-Trivia lässt sich nicht
strukturiert weiterverarbeiten.

> Anmerkung: Drei Kommentarformen bedeuten, dass fremde Modelle
> unterschiedlich aussehen. Ein Formatter kann das auf eine Form vereinheitlichen
> — offen ist, ob er das tun soll.

## 1.3 Token-Arten

| Art | Beispiel | Bedeutung |
|---|---|---|
| Bezeichner | `Customer`, `created_at`, `draft` | Namen und Aufzählungswerte |
| Qualifizierter Name | `shop.OrderStatus.paid` | punktgetrennter Pfad |
| String | `"#cc0000"` | Textwert |
| Zahl | `12`, `10` | numerischer Wert |
| Raw-Ausdruck | `` `now()` `` | wird unverändert an die Zieldatenbank gereicht |
| Liste | `[highlight]` | in `{}`-Blöcken: Werteliste |
| Flag-Liste | `[pk, required]` | hinter einem Feld: Semantik-Flags |

### Zeichenketten und Escape-Sequenzen

Zeichenketten stehen in doppelten Anführungszeichen und kennen die üblichen
Escape-Sequenzen:

| Sequenz | Bedeutung |
|---|---|
| `\"` | Anführungszeichen |
| `\\` | Backslash |
| `\n` | Zeilenumbruch |
| `\t` | Tabulator |

```
note = "Erste Zeile\nZweite Zeile mit \"Zitat\"";
```

**Folge für Regexes:** ein Backslash muss verdoppelt werden.

```
regex = "^\\d{4}-\\d{2}$";     # entspricht  ^\d{4}-\d{2}$
```

Das ist die vertraute Regel aus JSON und den meisten Sprachen — und zugleich
die häufigste Fehlerquelle beim Schreiben von Regexes. Der Editor sollte hier
warnen, wenn eine unbekannte Sequenz wie `\s` auftaucht.

### Backticks = Fremdsprache

Alles zwischen Backticks ist für erplang **undurchsichtig**. Es wird nicht
geparst, nicht validiert, nicht escaped — nur durchgereicht:

```
created_at timestamp [default=`now()`];
```

Backticks sind für **SQL-Ausdrücke** gedacht, nicht als allgemeiner
Roh-String. Damit ist auch klar, was sie kosten: sie sind dialektspezifisch.
Ein Modell mit `` `now()` `` ist nicht mehr ohne Weiteres nach Oracle
portierbar.

## 1.4 Abschluss von Anweisungen ⚠️

Beobachtete Regel im Beispiel:

- Eine Anweisung, die mit einem **Wert** endet, endet mit `;`
  → `mode = draft;` · `gross_sum decimal(10, 2);` · `cancelled;`
- Eine Anweisung, die mit einem **Block** `}` endet, endet **ohne** `;`
  → `sum float [required] { … }` · `draft { label="Draft" }`

Diese Regel ist im Beispiel konsistent durchgehalten und sollte so
dokumentiert und erzwungen werden. (Der Parser kann `;` nach `}` tolerieren,
der Formatter entfernt es.)

## 1.5 Zuweisungen: immer `=`

Ein Wert wird **überall** mit `=` zugewiesen — in `{}`-Blöcken wie in
`[]`-Flag-Listen:

```
fontSize = "2em";
created_at timestamp [default=`now()`];
```

`:` ist kein Zuweisungszeichen der Sprache. (`docs/beschreibung.txt` verwendet
es an einigen Stellen noch — veraltet.)

Begründung: Ein Wechsel des Zuweisungszeichens je nach Klammerart wäre eine
Regel ohne Nutzen. `=` liest sich zudem als „ist", `:` als „hat den Typ" — und
Typen stehen in dieser Sprache ohne Doppelpunkt hinter dem Feldnamen.

## 1.6 Groß-/Kleinschreibung von Schlüsselwörtern

Es gibt zwei Klassen, und die Schreibweise verrät die Klasse:

| Klasse | Schreibweise | Beispiele |
|---|---|---|
| Direktive / Anweisung | `lowercase` | `spec`, `import`, `as`, `use` |
| Block | `PascalCase` | `Project`, `Document`, `Table`, `Enum`, `Partial`, `Group`, `Datatype`, `Meta`, `Style`, `Index`, `Database`, `File`, `Dialect` |

Das ist eine gute Regel — sie macht beim Lesen sofort sichtbar, ob eine Zeile
etwas *öffnet* oder etwas *tut*. Sie sollte als verbindliche Regel für alle
künftigen Schlüsselwörter festgeschrieben werden.

Innerhalb der Block-Klasse gibt es faktisch zwei Untergruppen, die sich
syntaktisch unterscheiden:

- **Deklarationen** — haben einen Namen: `Table Customer`, `Enum OrderStatus`
- **Einstellungsblöcke** — haben keinen Namen: `Meta`, `Index`, `Database`,
  `File`, `Dialect`, und dürfen nur innerhalb bestimmter Eltern stehen

## 1.7 `[]` gegen `{}` — die zentrale Trennlinie

Das ist die wichtigste Konvention der Sprache:

| | `[ … ]` | `{ … }` |
|---|---|---|
| Inhalt | Flags, kommagetrennt | Schlüssel-Wert-Paare, `;`-getrennt |
| Wirkung | **verändert das erzeugte Schema** | **verändert das Schema nicht** |
| Beispiel | `[pk, required, default=0]` | `{ note = "…"; exampleValue = "…" }` |

Merksatz: *Eckige Klammern erzeugen DDL. Geschweifte Klammern erzeugen
Dokumentation.*

⚠️ Im Beispiel wird diese Linie an einer Stelle überschritten: `regex = "…"`
steht im `{}`-Block, ist aber eine Zusicherung über gültige Werte — siehe
`04-table-und-felder.md`.

## 1.8 Namen und Namenskonventionen

### Gültige Bezeichner

Ein Bezeichner beginnt mit einem Buchstaben oder `_` und enthält danach
Buchstaben, Ziffern und `_`.

### Konventionen sind Projektsache, nicht Sprachsache

Feldnamen gehören dem Nutzer. Die Sprache schreibt **keinen** Stil vor —
`created_at`, `createdAt` und `CreatedAt` sind alle gültig.

Wer Einheitlichkeit will, konfiguriert sie im `Project`-Block über den
`Lint`-Block. Damit ist die Konvention eine bewusste Projektentscheidung,
die im Modell selbst dokumentiert ist, statt einer unsichtbaren Regel im
Werkzeug. Details: `02-project-document-namespaces.md`.

> Hinweis zum aktuellen Beispiel: `examples/myexample.skema` mischt drei
> Stile — `Customer` (Pascal), `created_at` (snake), `publicId` (camel).
> Solange kein `Lint`-Block gesetzt ist, ist das zulässig; als Vorzeigebeispiel
> der Sprache ist es unglücklich.

### Was die Sprache selbst benennt

Für die eingebauten Schlüssel gilt eine feste Schreibweise, weil sie Teil der
Sprache sind und nicht der Nutzerdomäne:

- Blöcke: PascalCase — `Table`, `Meta`, `Dialect`
- Einstellungsschlüssel und Flags: camelCase — `styleClasses`, `onDelete`,
  `exampleValue`

## 1.9 Der Dateikopf `spec`

```
spec 1;
```

Gibt an, **nach welcher Sprachdefinition diese Datei gepflegt wird**. Muss die
erste Anweisung der Datei sein.

`spec` und `version` sind zwei verschiedene Dinge, und beide Namen sind
vergeben — deshalb dürfen sie sich nicht überschneiden:

| | Beschreibt | Wer setzt ihn | Wo |
|---|---|---|---|
| `spec 1;` | die Sprachdefinition, nach der die Datei geschrieben ist | die Sprache | Dateikopf |
| `version = "1.4.0"` | den Bearbeitungsstand des Modells | der Nutzer | `Meta`-Block |

`spec` ist eine ganze Zahl, keine Semver-Angabe: es gibt nur ganze
Sprachstände. Eine Datei mit `spec 2;` darf von einem Werkzeug, das nur `1`
kennt, mit klarer Meldung abgelehnt werden. `version` dagegen ist ein
freier String — der Nutzer entscheidet, ob dort `"1.4.0"`, `"2026-Q1"` oder
`"Entwurf Anna"` steht.

## 1.10 Dateiendung

Verbindlich ist **`.skema`**. Die früher verwendete Endung `.erp` ist
veraltet; Tooling und Import-Auflösung berücksichtigen sie nicht.

> `examples/myexample.skema` importiert in Zeile 3 noch `"./common.erp"` und
> das README nennt `examples/shop.erp` — beides ist nachzuziehen.

## 1.11 Eigene Schlüssel: das `x-`-Präfix

Überall dort, wo die Sprache Schlüssel-Wert-Paare kennt, gilt dieselbe Regel:

- **Bekannte Schlüssel** werden geprüft, autovervollständigt und vom Tooling
  verstanden.
- **Eigene Schlüssel** müssen mit `x-` beginnen und in Anführungszeichen
  stehen. Sie werden nicht geprüft, sondern unverändert in den JSON-Export
  durchgereicht.
- **Alles andere ist ein Fehler.**

```
Meta {
  author    = "anna";           # bekannt
  "x-jira"  = "SHOP-412";       # eigen
  athor     = "anna";           # FEHLER — Tippfehler, kein x-
}
```

Der Zweck der dritten Zeile ist der eigentliche: ohne sie bliebe ein
Tippfehler stumm wirkungslos. Das ist der unangenehmste Fehlertyp, weil er
wie ein Werkzeugproblem aussieht und nicht wie ein Schreibfehler.

Die Regel gilt in `Meta`, in `Style` und in den Attributblöcken von Feldern.

## 1.12 Imports

```
import "./common.skema" as base;
```

- Der Pfad ist relativ zur importierenden Datei.
- `as <alias>` bindet den Inhalt an einen lokalen Namensraum-Präfix.
- Zugriff danach: `base.Address`, `base.Currency`, …

⚠️ Offen: Das Beispiel importiert `base`, verwendet den Alias aber nie —
stattdessen taucht `shop.` als Präfix auf, das nirgends durch einen Import
eingeführt wird. Wie ein Namensraum entsteht, klärt
`02-project-document-namespaces.md`.
