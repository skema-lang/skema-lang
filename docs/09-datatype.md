# 09 — Datatype

> Status: **Entwurf.** Abschnitte mit ⚠️ sind noch nicht entschieden.

## 9.1 Wozu

`Datatype` ist der Ort, an dem ein Projekt seine eigenen Typen definiert.
Damit deckt es drei Bedürfnisse ab, die sonst überall im Modell verstreut
wären:

1. **Fachliche Namen** — `Email`, `Money`, `Username` statt `varchar` plus
   Regex daneben.
2. **Beschränkungen an einer Stelle** — die Regex steht einmal, nicht in
   fünf Tabellen.
3. **Portabilität** — derselbe Typ wird je Engine anders erzeugt, das Modell
   bleibt gleich.

Nach der Entscheidung aus `04` ist es zusätzlich der **einzige** Ort für
Wertebereichs-Beschränkungen. Im Feld steht nichts mehr darüber.

## 9.2 Aufbau

```
Datatype decimal(precision int, scale int) {
  precision { note = "Gesamtzahl der Stellen"; }
  scale     { note = "Feste Nachkommastellen"; default = 2; }

  Field {
    type = float;
    note = "Dezimalzahl mit fester Genauigkeit";
  }

  Dialect @postgres { sql = "numeric({precision}, {scale})"; }
  Dialect @mysql    { sql = "DECIMAL({precision}, {scale})"; }
}
```

Ein `Datatype` besteht aus höchstens vier Teilen:

| Teil | Aufgabe |
|---|---|
| Signatur | Name und typisierte Parameter |
| Parameter-Bodies | Beschreibung und Vorgabewert je Parameter |
| `Field` | wie sich ein Feld dieses Typs verhält — unabhängig von jeder Datenbank |
| `Dialect @engine` | wie der Typ in einer bestimmten Engine erzeugt wird |

Alle vier folgen der Grundform der Sprache, `Name Typ [Flags] { Body }`
(siehe `04`, Abschnitt 4.1). Es gibt hier keine Sonderregel zu lernen.

## 9.3 Parameter

Parameter werden **deklariert wie Felder**: Name, dann Typ.

```
Datatype decimal(precision int, scale int) { … }
```

Damit ist `decimal("abc", 2)` ein Fehler und nicht erst in der erzeugten SQL
ein Rätsel. Der Editor kann beim Aufruf anzeigen, was erwartet wird.

Jeder Parameter darf einen eigenen Body bekommen — dieselbe Form wie bei
einem Feld:

```
precision { note = "Gesamtzahl der Stellen"; }
scale     { note = "Feste Nachkommastellen"; default = 2; }
```

| Schlüssel | Bedeutung |
|---|---|
| `note` | Beschreibung; erscheint im Editor beim Aufruf des Typs |
| `default` | Vorgabewert, falls das Argument weggelassen wird |

Ein `default` macht Argumente optional: mit obiger Definition ist
`decimal(10)` gültig und bedeutet `decimal(10, 2)`.

> Warum nicht `precision = "Gesamtzahl"` wie bisher: In der ganzen Sprache
> bedeutet `schlüssel = wert`, dass der Schlüssel diesen Wert **hat**. Eine
> Beschreibung an dieser Stelle würde das umdeuten — und `default = 12` im
> alten `File`-Block war aus demselben Grund nicht auflösbar: Vorgabe für
> `precision`, oder für jedes Feld dieses Typs? Der eigene Body beantwortet
> das eindeutig.

⚠️ Offen: Müssen Parameter mit `default` am Ende der Signatur stehen, damit
`decimal(10)` eindeutig `scale` weglässt? Andernfalls braucht es benannte
Argumente wie `decimal(scale=4)`.

## 9.4 Der `Field`-Block

Beschreibt den Typ **auf Modellebene** — was ein Feld dieses Typs ist, bevor
irgendeine Datenbank ins Spiel kommt. Das ist auch das, was im `draft`-Modus
und im JSON-Export gilt.

| Schlüssel | Bedeutung |
|---|---|
| `type` | Basistyp: `string`, `int`, `float`, `bool` |
| `default` | Vorgabewert für Felder dieses Typs |
| `regex` | Beschränkung des Wertebereichs, beschreibend (siehe `04`) |
| `note` | Beschreibung des Typs |

Derselbe Block steht auch im `Enum` (siehe `06`, Abschnitt 6.1) und beantwortet
dort dieselbe Frage. Wer eines von beiden gelesen hat, kann das andere lesen.

### `default` an zwei Orten — und warum das kein Widerspruch ist

```
Datatype decimal(precision int, scale int) {
  scale { default = 2; }        # Vorgabe für das ARGUMENT
  Field { default = 0; }        # Vorgabe für das FELD
}
```

`decimal(10)` bedeutet `decimal(10, 2)`. Ein Feld `betrag decimal(10)` ohne
eigenen Wert bekommt `0`.

Genau diese beiden Bedeutungen ließen sich im ursprünglichen Entwurf nicht
unterscheiden: dort stand `default = 12` unbeschriftet im `File`-Block, während
die Parameter als lose Zuweisungen daneben lagen. Durch die Parameter-Bodies
hat jetzt jeder `default` einen eindeutigen Bezug.

> Zur Benennung: `Field` steht hier innerhalb von `Datatype` und meint „die
> Sicht des Feldes auf diesen Typ" — nicht ein Feld einer Tabelle. Beim
> Schreiben der Referenzdoku sollte dieser Unterschied ausdrücklich erwähnt
> werden, sonst liest man ihn falsch.

⚠️ Offen: Darf `type` auf einen anderen `Datatype` zeigen? Das erlaubte
Schichtung — `Money` baut auf `decimal(19,4)` auf. Mächtig, aber es braucht
eine Zyklenprüfung.

## 9.5 Dialekte

```
Dialect @postgres { sql = "numeric({precision}, {scale})"; }
Dialect @mysql    { sql = "DECIMAL({precision}, {scale})"; }
```

`sql` enthält den Typausdruck dieser Engine.

### Fehlt der passende Dialekt

Ist `dialect = oracledb` gesetzt und es gibt keinen `Dialect @oracledb`-Block:

| `mode` | Verhalten |
|---|---|
| `draft` | `Field.type` wird verwendet, mit Hinweis |
| `strict` | Fehler — der Typ ist für dieses Ziel nicht definiert |

Das fügt sich in die Modus-Logik aus `02`: `draft` skizziert, `strict` gibt
Garantien. Ein Moduswechsel deckt Lücken auf, statt sie zu verstecken.

## 9.6 Parameter einsetzen

`{name}` wird durch das Argument ersetzt — **aber nur, wenn `name` ein
deklarierter Parameter dieses `Datatype` ist.** Alles andere bleibt
unverändert stehen.

```
Datatype Kuerzel {
  Field { regex = "^[a-z]{3,20}$"; }    # {3,20} bleibt, wie es ist
}
```

Damit gibt es **kein Escaping** und keine Kollision mit Regex-Quantoren:
`3,20` ist kein gültiger Parametername, also passiert nichts. Die Regel ist
in einem Satz erklärbar und funktioniert in `sql`, `regex` und jedem anderen
Wert gleichermaßen.

Der einzige Preis: ein Tippfehler wie `{precission}` bleibt still stehen,
statt zu meckern. Dafür ist der Lint zuständig — sieht ein `{…}` wie ein
Bezeichner aus, ist aber kein Parameter, gibt es eine Warnung.

## 9.7 Was damit möglich wird

Weil Parameter typisiert sind und in jeden Wert einfließen dürfen, braucht die
Sprache `regexstring` nicht als eingebauten Typ — jedes Projekt kann ihn selbst
bauen:

```
Datatype regexstring(pattern string) {
  pattern { note = "Muster, das der Wert erfüllen muss"; }

  Field {
    type  = string;
    regex = "{pattern}";
  }
}
```

Und darauf die fachlichen Typen des Projekts:

```
Datatype Email {
  Field {
    type  = string;
    regex = "^[^@]+@[^@]+$";
    note  = "E-Mail-Adresse, nicht auf Zustellbarkeit geprüft";
  }
}

Table User {
  email Email [required, unique];
}
```

Das ist der Maßstab, an dem sich der Entwurf messen lässt: **die Sprache
braucht möglichst wenige eingebaute Typen, weil das Projekt seine eigenen
bauen kann.**

## 9.8 ⚠️ Schreibweise von Typnamen

`examples/myexample.skema` schreibt `decimal` klein, `Email` in anderen
Beispielen groß. Beides sind selbstdefinierte Typen, also greift die `@`-Regel
nicht — eine Linie sollte es trotzdem geben.

Naheliegend: eingebaute Basistypen klein (`string`, `int`, `float`, `bool`),
projekteigene PascalCase (`Email`, `Money`) — durchgesetzt über eine
`typeNaming`-Regel im `Lint`-Block. `decimal` bliebe ein Grenzfall: es ahmt
SQL nach, ist aber selbstdefiniert.

## 9.9 Korrekturen am bestehenden Beispiel

Für `examples/myexample.skema`, Zeilen 96–110:

| Zeile | Befund |
|---|---|
| 96 | `comma` heißt in SQL `scale` |
| 96 | Parameter untypisiert |
| 97–98 | Beschreibungen als Zuweisung geschrieben |
| 100 | `File` → `Field` |
| 102 | `default = 12` war nicht zuzuordnen — Argument oder Feld? Siehe 9.4 |
| 107–108 | `comma` ohne geschweifte Klammern, wird nicht ersetzt |
| 107–108 | `numeric` klein, `DECIMAL` groß — uneinheitlich |
| 106 | `Dialect { postgres = … }` → `Dialect @postgres { sql = … }` |

## 9.10 Entschieden

- `File` heißt `Field` (9.4)
- Parameter werden typisiert deklariert und haben einen eigenen Body (9.3)
- `{name}` wird nur bei deklarierten Parametern ersetzt, kein Escaping (9.6)
- Fehlender Dialekt: Hinweis in `draft`, Fehler in `strict` (9.5)

## 9.11 Offene Fragen dieses Kapitels

1. Müssen Parameter mit `default` am Ende der Signatur stehen? (9.3)
2. Darf `Field.type` auf einen anderen `Datatype` zeigen? (9.4)
3. Schreibweise von Typnamen (9.8)
4. Welche Basistypen sind eingebaut — nur `string`, `int`, `float`, `bool`?
