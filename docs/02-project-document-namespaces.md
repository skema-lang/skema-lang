# 02 — Project, Document und Namensräume

> Status: **Entwurf.** Abschnitte mit ⚠️ sind noch nicht entschieden.

## 2.1 Die beiden Ebenen

erplang kennt genau zwei Ebenen oberhalb der Deklarationen:

| | `Project` | `Document` |
|---|---|---|
| Anzahl | **genau einmal pro Projekt** | **genau einmal pro Datei** |
| Pflicht | ja | ja |
| Regelt | Modus, Dialekt, Lint, projektweite Styles | Namensraum der Datei, Datei-Metadaten |
| Analogie | `Cargo.toml` / `tsconfig.json` | `package`-Deklaration in Java oder Go |

Daraus folgt: In einem Projekt aus zehn Dateien gibt es zehn `Document`-Blöcke
und **einen** `Project`-Block. Eine zweite `Project`-Deklaration — auch in einer
importierten Datei — ist ein Fehler.

## 2.2 `Document` ist der Namensraum

Der Name im `Document`-Block ist der Namensraum aller Deklarationen der Datei.

```
spec 1;

Document shop { }

Enum  OrderStatus { … }     # vollständiger Name: shop.OrderStatus
Table Order       { … }     # vollständiger Name: shop.Order
```

Wichtig: Deklarationen stehen **neben** dem `Document`-Block, nicht darin. Der
Block deklariert die Identität der Datei, er umschließt nichts. Genau wie
`package shop;` in Java nicht die Klassen der Datei einklammert.

Der Document-Name ist unabhängig vom Dateinamen. `myexample.skema` darf
`Document shop` deklarieren. (Ein Lint-Hinweis bei Abweichung ist denkbar,
siehe 2.7.)

## 2.3 Position im Dateikopf

Der `Document`-Block darf **an beliebiger Stelle** auf oberster Ebene stehen —
vor, zwischen oder nach den Deklarationen. Nur `spec` hat eine feste Position.

Die Sprache erzwingt hier nichts. Wer Einheitlichkeit will, greift zum
`Lint`-Block (2.7); der Formatter darf den Block nach oben ziehen.

## 2.4 Referenzen innerhalb derselben Datei

Innerhalb einer Datei ist das Präfix **optional**, auf importierte
Deklarationen ist es **Pflicht**:

```
Document shop { }

Enum OrderStatus { paid; }

Table Order {
  status   OrderStatus [default=OrderStatus.paid];   # lokal, ohne Präfix
  waehrung base.Currency;                            # importiert, mit Präfix
}
```

Daraus folgt eine Regel, die man beim Lesen sofort anwenden kann:

> **Ein Präfix im Quelltext bedeutet immer: das kommt aus einer anderen Datei.**

Der volle Pfad ist lokal weiterhin erlaubt, aber nicht empfohlen — er bricht
bei jeder Umbenennung des Namensraums. `examples/myexample.skema` schreibt in
Zeile 71 `default=shop.OrderStatus.paid`, obwohl das Enum in derselben Datei
steht.

## 2.5 Imports und Aliase

Der Namensraum wird **an der Quelle** vergeben und darf **an der Verwendungs­stelle**
umbenannt werden.

```
# ---- customer.skema ----
Document customer { }
Table User { id int [pk]; }
```

```
# ---- order.skema ----
Document shop { }

import "./customer.skema";              # ansprechbar als  customer.User.id
import "./customer.skema" as user;      # ansprechbar als  user.User.id
```

- **Ohne `as`** gilt der Document-Name der importierten Datei.
- **Mit `as`** wird dieser Name für diese Datei überschrieben.

Der Alias wirkt **nur in der importierenden Datei**. Dieselbe Datei darf in
`order.skema` als `user` und in `invoice.skema` als `customer` erscheinen. Der
Document-Name gehört der Quelldatei, der Alias dem Aufrufer.

### Die vollständige Sichtbarkeitsregel

| Woher | Schreibweise |
|---|---|
| eigene Datei | `Order.id` — oder gleichbedeutend `shop.Order.id` |
| importierte Datei | `customer.User.id` bzw. unter Alias `user.User.id` |

`shop.Order.id` und `Order.id` bezeichnen innerhalb von `shop` dasselbe.

### Eindeutigkeit der Namensräume

Innerhalb einer Datei muss jeder Namensraum eindeutig sein — der eigene
Document-Name eingeschlossen. Zwei Imports mit gleichem Document-Namen sind
ein Fehler, bis einer davon per `as` umbenannt wird:

```
import "./de/common.skema";           # Document common
import "./en/common.skema" as en;     # Document common, umbenannt
```

Damit hat `as` eine klare, einzige Aufgabe: Kollisionen auflösen und
Lesbarkeit verbessern. Es ist kein Pflichtrauschen an jeder Import-Zeile.

### Was ein Import mitbringt

Ein Import macht die **Deklarationen** der anderen Datei sichtbar: `Table`,
`Enum`, `Partial`, `Datatype`, `Group`. Er bringt **nicht** deren
`Document`-Metadaten oder -Styles mit.

### Imports sind nicht transitiv

Importiert `a` die Datei `b` und `b` die Datei `c`, so sind die Deklarationen
aus `c` in `a` **nicht** sichtbar. Wer `c` benutzen will, importiert `c`.

Damit ist die Import-Liste am Dateikopf die vollständige und ehrliche Liste
der Abhängigkeiten dieser Datei. Eine Umstrukturierung in `b` kann `a` nicht
unbemerkt brechen.

### Zyklen sind erlaubt

`a` darf `b` importieren und `b` gleichzeitig `a`:

```
# ---- order.skema ----
Document shop { }
import "./customer.skema";
Table Order { kunde >customer.User.id; }

# ---- customer.skema ----
Document customer { }
import "./order.skema";
Table User { letzte_bestellung >shop.Order.id; }
```

Das ist Absicht: gegenseitige Verweise sind im ER-Modell der Normalfall, und
die Sprache soll deswegen keine Dateiaufteilung erzwingen. Auflösbar ist es,
weil Deklarationen reihenfolgeunabhängig sind — das Tooling sammelt erst alle
Namen des gesamten Graphen und prüft danach die Referenzen.

> Für die Implementierung heißt das: der Modulgraph muss beim Einlesen bereits
> besuchte Dateien erkennen, sonst läuft die Auflösung endlos.

## 2.6 Die Wurzeldatei

Genau eine Datei im Projekt trägt den `Project`-Block. Diese Datei ist die
**Wurzeldatei**: sie importiert — direkt oder über Zwischenstufen — alles,
was zum Projekt gehört, und sie ist das, was man dem Werkzeug übergibt.

```
erplang check main.skema
erplang export --json main.skema
```

Daraus folgen drei Dinge:

1. **Projektzugehörigkeit ist erreichbarkeitsdefiniert.** Eine `.skema`-Datei,
   die von der Wurzeldatei aus nicht erreichbar ist, gehört nicht zum Projekt —
   auch wenn sie im selben Verzeichnis liegt. Der Import-Graph *ist* die
   Projektdefinition; es gibt keine zweite Liste, die man pflegen müsste.
2. **Der Import-Graph ist ein Baum mit einer Wurzel.** Eine zweite
   `Project`-Deklaration irgendwo im Graphen ist ein Fehler.
3. **Eine einzelne Datei ist für sich allein nicht vollständig prüfbar.**
   `mode` und `dialect` stehen in der Wurzeldatei; ohne sie weiß der Prüfer
   nicht, ob `float` zulässig ist.

### Konsequenz für den Language Server ⚠️

Punkt 3 trifft die IDE. Öffnet jemand `order.skema` direkt, führt von dort
kein Weg nach oben zur Wurzeldatei — Imports zeigen nur nach unten.

Der Language Server braucht die Wurzeldatei deshalb aus einer anderen Quelle.
Praktikabel ist eine Editor-Einstellung:

```jsonc
// .vscode/settings.json
{ "erplang.rootFile": "main.skema" }
```

Fehlt sie, sollte der Server nicht raten, sondern zwei Dinge tun: Syntax und
dateilokale Fehler weiter melden, und für alles, was `mode`/`dialect` braucht,
einen sichtbaren Hinweis ausgeben („keine Wurzeldatei konfiguriert —
Typprüfung inaktiv"). Stillschweigend auf `draft` zurückzufallen wäre der
schlechteste Weg: das Modell sähe fehlerfrei aus und wäre es nicht.

## 2.7 Der `Project`-Block im Einzelnen

```
Project {
  mode    = draft;      # draft | strict
  dialect = generic;    # generic | postgres | mysql | oracledb

  Meta  { … }
  Style @table { … }
  Lint  { … }
}
```

### `mode` — der Hauptschalter für Strenge

`mode` ist **keine** reine Typ-Einstellung. Er entscheidet, ob das Projekt
überhaupt auf Regeln geprüft wird:

| | `mode = draft` | `mode = strict` |
|---|---|---|
| Lint-Regeln | werden **ignoriert** | werden **angewendet** |
| Datentypen | einfache genügen: `string`, `int`, `float`, `bool` | genauere sind Pflicht |
| Gedacht für | Skizzieren, Whiteboard, erste Runde | belastbares Modell, Codegenerierung |

Der Nutzen ist die Zweistufigkeit des Arbeitsprozesses: ein Modell darf
unscharf und unaufgeräumt beginnen — Ideen zuerst, Sorgfalt später. Das
Umschalten auf `strict` macht aus jeder Unschärfe eine sichtbare Meldung. Der
Übergang kostet ein einziges Wort.

Daraus folgt auch: **im `draft`-Modus ist der `Lint`-Block wirkungslos.** Er
darf trotzdem dort stehen — er beschreibt, was gelten *wird*.

> Bewusste Kopplung: Lint und Typgenauigkeit sind streng genommen zwei
> unabhängige Achsen. Sie werden hier absichtlich an einen Schalter gebunden,
> weil zwei Schalter vier Zustände ergeben, von denen zwei niemand erklären
> möchte. Sollte sich der Bedarf zeigen, ist eine spätere Aufspaltung
> abwärtskompatibel möglich.

### `dialect`

Wählt die Zieldatenbank und damit die verfügbare Typtabelle. `generic`
bedeutet: nur portable Typen, keine dialektspezifischen.

`mode` und `dialect` wirken zusammen und ergeben drei sinnvolle Stufen:

| `mode` | `dialect` | Erlaubte Typen |
|---|---|---|
| `draft` | `generic` | einfache Typen — `string`, `int`, `float`, `bool` |
| `strict` | `generic` | genauere, aber portable Typen — `varchar(80)`, `decimal(10,2)`, `timestamp`, `uuid` |
| `strict` | `postgres` … | zusätzlich alles Dialektspezifische — `jsonb`, `tsvector`, `citext` |

Die mittlere Stufe ist die eigentlich interessante: **präzise und trotzdem
portabel**. Sie ist der Normalzustand eines gepflegten Modells; die dritte
Stufe verlässt bewusst die Portabilität.

⚠️ Offen: die genaue Abgrenzung der mittleren Stufe — welche Typen zählen als
„portabel präzise"? Detaillierung in `09-datatype.md`.

### `Lint`

Konventionen sind Projektsache, nicht Sprachsache (siehe `01`, Abschnitt 1.8).
Der `Lint`-Block macht sie explizit und versioniert sie mit dem Modell:

```
Project {
  mode = strict;

  Lint {
    fieldNaming = snake_case;      # Vorgabe: any
    tableNaming = PascalCase;
    enumNaming  = PascalCase;

    unusedImport = warn;
    danglingRef  = error;
  }
}
```

`Lint` darf **ausschließlich im `Project`-Block** stehen. Es gibt keine
Ausnahmen pro Datei, pro Tabelle oder pro Zeile: eine Regel gilt für das
gesamte Projekt oder gar nicht.

Das ist eine bewusste Härte. Der Preis: eine einzelne Alttabelle mit fremden
Feldnamen kann eine Regel projektweit kippen. Der Gegenwert: es gibt keinen
Ort, an dem eine Regel heimlich abgeschaltet ist — wer wissen will, was gilt,
liest genau einen Block. Der Ausweg für Altlasten ist `mode = draft`, bis
aufgeräumt ist.

Mögliche Werte für Namenskonventionen: `snake_case`, `camelCase`,
`PascalCase`, `SCREAMING_SNAKE_CASE`, `any`.

⚠️ Offen: Schweregrade. Standard könnte `warn` sein, mit Verschärfung über die
`[]`-Konvention der Sprache — eckige Klammern verändern Verhalten:

```
fieldNaming = snake_case [error];
tableNaming = PascalCase [off];
```

### `Meta` und `Style`

Beschreibt `03-meta-und-style.md`.

## 2.8 Entschieden

- `Document` darf frei auf oberster Ebene stehen (2.3)
- Präfix lokal optional, bei Imports Pflicht (2.4)
- Ohne `as` gilt der Document-Name der Quelldatei; `as` überschreibt ihn
  für die importierende Datei (2.5)
- Imports sind nicht transitiv (2.5)
- Import-Zyklen sind erlaubt (2.5)
- Der `Project`-Block liegt in der Wurzeldatei, die alles importiert und dem
  Werkzeug übergeben wird (2.6)
- `mode` schaltet Lint **und** Typgenauigkeit gemeinsam (2.7)
- `Lint` steht ausschließlich im `Project`-Block, ohne lokale Ausnahmen (2.7)

## 2.9 Offene Fragen dieses Kapitels

1. Schweregrad-Syntax für Lint-Regeln — `fieldNaming = snake_case [error];`? (2.7)
2. Vollständige Liste der Lint-Regeln
3. Abgrenzung „portabel präziser" Typen bei `strict` + `generic` → `09-datatype.md`
4. Muss der Language Server die Wurzeldatei über eine Editor-Einstellung
   erfahren? (2.6)
