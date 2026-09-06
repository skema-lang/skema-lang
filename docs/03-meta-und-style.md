# 03 — Meta und Style

> Status: **Entwurf.** Abschnitte mit ⚠️ sind noch nicht entschieden.

Beide Blöcke gehören zur `{}`-Seite der Sprache: sie beschreiben, sie
erzeugen kein Schema (siehe `01`, Abschnitt 1.7).

- **`Meta`** — Wissen *über* eine Deklaration: wer, warum, welcher Stand.
- **`Style`** — Aussehen im Diagramm.

## 3.1 `Meta`

```
Table Customer {
  id int [pk];

  Meta {
    author       = "anna";
    note         = "Kundenstamm, DSGVO-relevant";
    version      = "1.2";
    styleClasses = [highlight];
    "x-jira"     = "SHOP-412";
  }
}
```

### Standardschlüssel

| Schlüssel | Typ | Bedeutung |
|---|---|---|
| `author` | String | Wer hat das entworfen? Für Zusammenarbeit und Rückfragen. |
| `note` | String | Fachliche Beschreibung. Ziel für Hover-Texte im Editor und Kommentare im generierten DDL. |
| `version` | String | Bearbeitungsstand, frei wählbar (siehe `01`, Abschnitt 1.9). |
| `styleClasses` | Liste | Zuweisung von Style-Klassen, siehe 3.4. |

### Eigene Schlüssel

`Meta` ist **erweiterbar, aber nicht beliebig**: eigene Schlüssel müssen das
`x-`-Präfix tragen und in Anführungszeichen stehen.

```
Meta {
  author    = "anna";        # bekannt
  "x-owner" = "team-billing"; # eigen, wird durchgereicht
  owner     = "team-billing"; # FEHLER: unbekannter Schlüssel ohne x-
}
```

Damit ist ein Tippfehler wie `athor` ein Fehler und kein stiller Ausfall,
während projekteigene Informationen weiterhin Platz haben. Eigene Schlüssel
landen unverändert im JSON-Export und sind für eigene Generatoren nutzbar.

Siehe `01`, Abschnitt 1.11 — dieselbe Regel gilt in `Style` und in
Feld-Attributblöcken.

### Wo `Meta` stehen darf ⚠️

Im Beispiel steht `Meta` in `Project`, `Document` und `Table`;
`docs/beschreibung.txt` zeigt es zusätzlich in `Datatype`.

Vorschlag: **`Meta` ist überall erlaubt, wo eine benannte Deklaration steht** —
`Project`, `Document`, `Table`, `Partial`, `Enum`, `Group`, `Datatype`. Eine
Ausnahmeliste wäre nicht begründbar; jede Deklaration kann einen Autor und
eine Beschreibung vertragen.

### ⚠️ Regelbruch: `dbName` in `Meta`

`examples/myexample.skema` legt in Zeile 86 den physischen Tabellennamen in
den `Meta`-Block:

```
Table Order {
  Meta {
    dbName = "order";
  }
}
```

Das widerspricht der zentralen Regel der Sprache. `dbName` **verändert das
erzeugte DDL** — es entscheidet, wie die Tabelle in der Datenbank heißt. Damit
gehört es nicht in einen Block, dessen Zusage lautet „ich beschreibe nur".

Verschärfend: dieselbe Datei löst dasselbe Problem bei `Table Customer` in den
Zeilen 60–63 über einen völlig anderen Mechanismus:

```
Table Customer {
  Database {
    name            = "shopsystem";
    postgres_schema = "public";
  }
}
```

Zwei Wege für eine Sache, und einer davon bricht die Grundregel. Aufgelöst
wird das in `07-index-und-physisches-mapping.md`.

## 3.2 `Style` — zwei Arten von Regeln

Es gibt genau zwei Formen, und das `@` unterscheidet sie:

```
Style @table   { … }    # Selektor: gilt für ALLE Tabellen
Style highlight { … }   # Klasse:   gilt für zugewiesene Elemente
```

> **`@` bedeutet: eingebauter Elementtyp der Sprache. Kein `@` bedeutet:
> selbst vergebener Name.**

Dieselbe Regel trägt später auch andere Erweiterungen, ohne ein weiteres
Sonderzeichen zu brauchen.

### Selektoren

| Selektor | Wirkung |
|---|---|
| `@project` | Rahmenbedingungen des ganzen Diagramms |
| `@document` | alle Elemente dieser Datei |
| `@table` | Tabellenkästen |
| `@enum` | Enum-Kästen |
| `@group` | Gruppenrahmen |
| `@field` ⚠️ | Feldzeilen — z. B. Schlüsselfelder hervorheben |
| `@relation` ⚠️ | Beziehungslinien — Farbe, Strichart, Pfeilform |

`@field` und `@relation` stehen noch nicht in `docs/beschreibung.txt`.
`@relation` ist der wichtigste Nachtrag: in einem ER-Diagramm tragen die
Linien die halbe Information — Kardinalität und Löschverhalten. Ohne
Selektor ist sie nicht gestaltbar.

## 3.3 Style-Klassen

Eine Klasse ist ein `Style`-Block **ohne** `@`. Sie wirkt erst, wenn sie einem
Element über `Meta.styleClasses` zugewiesen wird.

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

Damit ist die Lücke geschlossen, die `examples/myexample.skema` derzeit hat:
dort wird `highlight` dreimal zugewiesen, aber nie deklariert.

⚠️ Offen: Dürfen Klassen auch im `Document` definiert werden, oder nur im
`Project`? Für die Kaskade in 3.4 wäre beides handhabbar; die einfachere
Antwort ist „nur `Project`", analog zu `Lint`.

## 3.4 Kaskade: Was gewinnt?

Es gibt **keine** Styles am einzelnen Element. Wer eine Tabelle auszeichnen
will, definiert eine Klasse und weist sie zu.

Das ist eine bewusste Einschränkung — dieselbe Lehre, die HTML mit
Inline-Styles gemacht hat: sobald Gestaltung im Modell verstreut werden darf,
wird sie verstreut. So bleiben Modelldateien frei von Aussehen, und alles
Visuelle steht an einem Ort.

Die Kaskade hat damit genau drei Stufen, von schwach nach stark:

1. `Style @…` im `Project` — projektweite Vorgabe
2. `Style @…` im `Document` — Vorgabe dieser Datei
3. Klassen aus `styleClasses`, in Listenreihenfolge; die letzte gewinnt

Regel: **spezifischer schlägt allgemeiner, später schlägt früher.** Keine
Gewichtungsrechnung wie in CSS — die versteht nach zwei Wochen niemand mehr.

## 3.5 Style-Eigenschaften

Es gibt einen **standardisierten Kernsatz**, der geprüft und
autovervollständigt wird, und `x-` für alles Weitere:

```
Style @table {
  headerBackgroundColor = "#cc0000";   # bekannt, wird geprüft
  bgColor               = "#fff";      # FEHLER: unbekannt, kein x-
  "x-mermaidClass"      = "wichtig";   # frei, wird durchgereicht
}
```

Ohne diese Trennung bliebe `bgColor` statt `backgroundColor` stumm
wirkungslos — der unangenehmste aller Fehler, weil er wie ein Renderer-Problem
aussieht.

⚠️ Offen: der Kernsatz selbst. Zu klären ist mindestens: Farben (Hintergrund,
Text, Rahmen), Schrift (Größe, Schnitt), Rahmen (Breite, Radius, Strichart),
Linien für `@relation` (Farbe, Strichart, Pfeilform) und Abstände.

## 3.6 Entschieden

- `Style` ohne `@` ist eine Klasse, mit `@` ein Elementtyp-Selektor (3.2, 3.3)
- Keine Styles am einzelnen Element; Kaskade mit drei Stufen (3.4)
- Style-Eigenschaften: Kernsatz plus `x-` (3.5)
- `Meta`: bekannte Schlüssel plus `x-` (3.1)

## 3.7 Offene Fragen dieses Kapitels

1. Selektoren `@field` und `@relation` aufnehmen? (3.2)
2. Dürfen Style-Klassen auch im `Document` definiert werden? (3.3)
3. Welche Eigenschaften gehören in den Kernsatz? (3.5)
4. `Meta` in allen Deklarationen erlaubt? (3.1)
5. Wohin mit `dbName`? (3.1) → wird in `07` entschieden
