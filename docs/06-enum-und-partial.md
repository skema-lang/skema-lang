# 06 — Enum und Partial

> Status: **Entwurf.** Abschnitte mit ⚠️ sind noch nicht entschieden.

Zwei Deklarationen, die auf den ersten Blick nichts miteinander zu tun haben.
Sie stehen hier zusammen, weil beide dasselbe Ziel verfolgen: **etwas einmal
festlegen und mehrfach verwenden.**

## 6.1 `Enum` — ein abgeschlossener Wertevorrat

```
Enum OrderStatus {
  Field {
    type    = string;
    default = paid;
    note    = "Zustände einer Bestellung";
  }

  draft     { label = "Entwurf"; }
  paid      { label = "Bezahlt"; }
  shipped   { label = "Versandt"; value = "S"; }
  cancelled;
}
```

Ein `Enum` besteht aus zwei Teilen: einem `Field`-Block, der beschreibt wie
sich ein Feld dieses Typs verhält, und der Liste der Einträge.

### Der `Field`-Block — derselbe wie in `Datatype`

Das ist kein Zufall und keine Doppelung: `Field` beantwortet in beiden Fällen
dieselbe Frage — *wie verhält sich ein Feld dieses Typs?* Wer `Datatype`
verstanden hat, kann `Enum` sofort lesen (siehe `09`, Abschnitt 9.4).

| Schlüssel | Bedeutung |
|---|---|
| `type` | Gespeicherter Typ: `string` oder `int` |
| `default` | Vorgabewert für Felder dieses Typs |
| `note` | Beschreibung des Enums |

`type` beantwortet dabei die Frage nach der physischen Abbildung fast von
selbst: `string` speichert die Werte lesbar, `int` speichert Positionen.

### Die Einträge

Jeder Eintrag folgt der Grundform der Sprache (`04`, Abschnitt 4.1): ein Name,
optional ein Body. Wer nichts zu ergänzen hat, schreibt nur den Namen und ein
Semikolon.

| Schlüssel | Bedeutung |
|---|---|
| `label` | Text **für Menschen** — Auswahllisten, Berichte, Diagramm |
| `value` | Wert, der **tatsächlich gespeichert** wird |

Beide sind optional. Ohne Angabe gilt der Eintragsname für beides. Sie
beantworten zwei verschiedene Fragen und werden deshalb getrennt gehalten:

```
shipped { label = "Versandt"; value = "S"; }
```

Angezeigt wird „Versandt", gespeichert wird `"S"`, im Modell heißt es
`shipped`. Genau diese Dreiteilung braucht man, sobald eine Altanwendung
Kurzcodes erwartet.

### Verwendung

```
Table Order {
  status OrderStatus;                            # nutzt Field.default
  vorstatus OrderStatus [default=OrderStatus.draft];   # überschreibt
}
```

Der Enum-Name steht an der Stelle des Datentyps. Ein Vorgabewert am Feld wird
über den vollen Pfad `OrderStatus.draft` angegeben — nicht als blankes `draft`,
sonst wäre nicht erkennbar, ob eine Zeichenkette oder ein Enum-Eintrag gemeint
ist.

### Reihenfolge ist bedeutsam

Die Reihenfolge der Einträge ist Teil der Deklaration. Diagramme, Auswahllisten
und bei `type = int` auch die gespeicherten Zahlen richten sich danach. Der
Formatter sortiert sie nicht um.

> Folge für `type = int`: das Einfügen eines Eintrags in der Mitte verschiebt
> alle folgenden Werte. Das ist eine Migration, keine Modelländerung — die
> Referenzdoku sollte ausdrücklich davor warnen.

### `Meta` im Enum

`Meta` ist wie bei jeder Deklaration erlaubt (siehe `03`):

```
Enum OrderStatus {
  draft;
  paid;

  Meta {
    author = "anna";
    note   = "Fachlich abgestimmt mit Vertrieb";
  }
}
```

⚠️ Offen: Darf ein einzelner **Eintrag** ein `Meta` haben, oder gilt sein Body
selbst schon als Metadaten-Bag — so wie beim Feld (siehe `04`, Abschnitt 4.5)?
Für Einheitlichkeit spricht Letzteres.

## 6.2 `Partial` — wiederverwendbare Feldgruppen

```
Partial common_base {
  id         int       [pk];
  publicId   uuid      [unique];
  created_at timestamp [required, default=`now()`];
}

Table Customer {
  use common_base;
  email varchar [required];
}
```

`Table Customer` hat damit vier Felder. Ein `Partial` wird **nie selbst zu
einer Tabelle** — es ist eine Textbausteinsammlung, kein Modellobjekt. Im
Diagramm taucht es nicht auf.

Der Nutzen ist konkret: Zeitstempel, technische Schlüssel und Mandantenfelder
wiederholen sich in fast jeder Tabelle. Einmal deklariert, überall verwendet,
und eine Änderung wirkt sofort im ganzen Modell.

### `use` fügt an seiner Position ein

Die Position der `use`-Zeile bestimmt, wo die Felder landen:

```
Table Customer {
  use common_base;      # id, publicId, created_at
  email varchar;        # danach
}

Table Product {
  sku varchar;          # zuerst
  use common_base;      # danach
}
```

Das ist wichtig, weil die Feldreihenfolge in der erzeugten Tabelle und im
Diagramm sichtbar ist. Eine Regel wie „Partial-Felder kommen immer zuerst"
wäre kürzer, würde aber genau dann im Weg stehen, wenn man sie braucht.

Mehrere `use` in einer Tabelle sind erlaubt und werden in ihrer Reihenfolge
eingefügt.

### Über Dateigrenzen

```
use base.timestamps;
```

Ein `Partial` aus einer importierten Datei wird mit Präfix angesprochen, wie
jede andere Deklaration (siehe `02`, Abschnitt 2.4).

### Namenskonflikte sind ein Fehler

```
Partial common_base { id int [pk]; }

Table Customer {
  use common_base;
  id uuid [pk];        # FEHLER: id kommt bereits aus common_base
}
```

Es gibt **kein** stilles Überschreiben. Wer abweichen will, verwendet ein
anderes `Partial` oder lässt `use` weg — beides ist im Quelltext sichtbar.

Der Grund: ein überschriebenes Feld sieht man dem `Partial` nicht an. Man
liest `use common_base`, glaubt zu wissen, was die Tabelle enthält, und liegt
falsch. Diese Art Fehler bemerkt man erst in der Migration.

Derselbe Fall entsteht bei zwei `use`-Zeilen, die dasselbe Feld mitbringen —
auch das ist ein Fehler.

### Was in einem `Partial` stehen darf

| Inhalt | erlaubt | Anmerkung |
|---|---|---|
| Felder | ✓ | der Normalfall |
| Beziehungen | ✓ | erzeugt in jeder verwendenden Tabelle eine Beziehung |
| `Index` | ✓ | ein `Partial` mit `created_at` bringt dessen Index gleich mit |
| `Meta` | ✓ | beschreibt das `Partial` selbst, wird **nicht** mitvererbt |
| `use` | ✓ | `Partial` in `Partial`, mit Zyklenprüfung |
| `Database` | — | ein `Partial` hat keinen physischen Ort |

Damit ist ein `Partial` fast eine Tabelle — nur ohne eigene Existenz. Das
lohnt sich: Bausteine lassen sich schichten.

```
Partial Zeitstempel {
  created_at timestamp [required, default=`now()`];
  updated_at timestamp;

  Index { ix_created (created_at); }
}

Partial Auditierbar {
  use Zeitstempel;
  created_by >User.id;
}
```

⚠️ Offen: Beziehungen in `Partial`s brauchen eine Regel für Indexnamen. Wird
`ix_created` in zehn Tabellen eingefügt, kollidieren zehn gleichnamige Indizes
— es sei denn, der Name wird je Tabelle ergänzt.

## 6.3 ⚠️ Schreibweise

Das Beispiel schreibt `common_base` in snake_case, während alle anderen
Deklarationen PascalCase sind (`Customer`, `OrderStatus`). Enum-Einträge sind
klein (`draft`, `paid`).

Da Namenskonventionen Projektsache sind (siehe `01`, Abschnitt 1.8), gehört
das in den `Lint`-Block — aber die mitgelieferten Beispiele sollten eine Linie
vorleben.

## 6.4 Entschieden

- `Enum` hat einen `Field`-Block mit `type`, `default`, `note` — derselbe
  Block wie in `Datatype` (6.1)
- Einträge haben `label` (für Menschen) und `value` (gespeichert) (6.1)
- Feldkonflikte durch `use` sind ein Fehler, kein Überschreiben (6.2)
- Ein `Partial` darf Felder, Beziehungen, `Index`, `Meta` und `use` enthalten (6.2)

## 6.5 Offene Fragen dieses Kapitels

1. Physische Abbildung eines Enums über `Dialect`-Blöcke — zurückgestellt (6.1)
2. Hat ein Enum-Eintrag einen Metadaten-Body oder einen `Meta`-Block? (6.1)
3. Indexnamen aus `Partial`s, die in vielen Tabellen landen (6.2)
4. Zulässige Werte für `Field.type` im `Enum` — nur `string` und `int`?
