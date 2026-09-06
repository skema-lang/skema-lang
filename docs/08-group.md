# 08 — Group

> Status: **Entwurf.** Abschnitte mit ⚠️ sind noch nicht entschieden.

## 8.1 Ist-Zustand

```
Group Shop {
  Table shop.User;
  Table shop.Order;
  Enum  shop.OrderStatus;
}
```

Eine `Group` fasst Deklarationen zusammen — im Diagramm als umrandeter Bereich,
in der Dokumentation als Kapitel.

## 8.2 ⚠️ Die Abgrenzung ist noch unscharf

Die Sprache hat inzwischen **drei** Arten, Dinge zusammenzufassen. Ohne klare
Trennung wird jedes Mal geraten, welche gemeint ist:

| Konstrukt | Fasst zusammen | Wirkt auf |
|---|---|---|
| `Document` | alles in einer Datei | Namensraum |
| `Database { schema }` | Tabellen eines Schemas | physische Ablage |
| `Group` | frei gewählte Elemente | ⚠️ ? |

`Document` und `Database` sind geklärt (siehe `02` und `07`). Für `Group` muss
gesagt werden, ob sie **rein visuell** ist oder auch etwas erzeugt.

Die saubere Antwort ist vermutlich: **rein visuell und dokumentarisch.** Denn
alles andere ist bereits vergeben — der Namensraum gehört `Document`, die
physische Ablage gehört `Database`. Was übrig bleibt, ist genau das, was die
anderen beiden nicht können: eine Zusammenstellung **quer** zu Datei und
Schema, allein zum Verstehen.

Daraus folgt auch, warum `Group` überhaupt gebraucht wird: ein Modell mit 60
Tabellen ist als Diagramm unlesbar. Gruppen sind das Mittel, es in
begreifbare Teile zu zerlegen — „Bestellwesen", „Stammdaten", „Abrechnung" —
ohne die Dateiaufteilung danach richten zu müssen.

## 8.3 ⚠️ Fehler im Beispiel

```
Group Shop {
  Table shop.User;        # existiert nicht
  Table shop.Order;
  Enum  shop.OrderStatus;
}
```

`examples/myexample.skema` deklariert `Table Customer` und `Table Order`, aber
keine `Table User`. Der Verweis geht ins Leere.

Genau das sollte ein Lint-Fehler sein (`danglingRef`, siehe `02`,
Abschnitt 2.7). Eine Gruppe, die auf Nichtexistierendes zeigt, ist der
häufigste Verfall in gepflegten Modellen: die Tabelle wird umbenannt, die
Gruppe bleibt stehen.

Außerdem: nach `02`, Abschnitt 2.4 ist das Präfix innerhalb derselben Datei
optional. `Table Order;` genügt.

## 8.4 Die Art steht dabei — und muss stimmen

```
Group Bestellwesen {
  Table Order;
  Table OrderPosition;
  Enum  OrderStatus;
}
```

Das Schlüsselwort ist Lesehilfe: in einer Liste aus zwanzig Einträgen
unterscheidet man auf einen Blick Tabellen von Aufzählungen.

Es ist aber **keine bloße Verzierung** — es wird geprüft:

```
Enum Order;      # FEHLER, wenn Order eine Table ist
```

Damit fängt die Gruppe zwei Fehler statt einem: den falschen Namen und die
falsche Art.

### Ein Name pro Namensraum

Eine `Table` und ein `Enum` dürfen im selben Namensraum **nicht** denselben
Namen tragen.

```
Enum  Status { … }
Table Status { … }     # FEHLER: Name bereits vergeben
```

`Status` als Tabelle und als Aufzählung nebeneinander wäre auch für Menschen
verwirrend — und ohne diese Regel müsste jede Referenz im Modell ihre Art
mitführen.

## 8.5 Die Gruppe nennt ihre Mitglieder

Gruppiert wird **von der Gruppe aus**, nicht vom Element:

```
Group Abrechnung {
  Table Invoice;
  Table billing.Payment;
}
```

Ein Blick zeigt, was zusammengehört — und das ist der Zweck der Sache. Die
Gegenrichtung (`Table Order { Meta { group = … } }`) wäre bequemer beim
Schreiben, aber es gäbe dann keine Stelle mehr, an der eine Gruppe als Ganzes
sichtbar ist.

Der Preis ist bekannt und tragbar: beim Anlegen einer neuen Tabelle muss man
an den Eintrag denken. Ein Lint-Hinweis auf gruppenlose Tabellen fängt das ab.

## 8.6 Eine Tabelle, eine Gruppe

Gruppen **überschneiden sich nicht** und **verschachteln sich nicht**.

```
Group Stammdaten { Table Customer; }
Group Abrechnung { Table Customer; }   # FEHLER: bereits in Stammdaten
```

Das ist bewusst streng. Der Grund ist die Darstellung: ein Kasten kann nicht
in zwei Rahmen liegen. Eine Sprache, die etwas ausdrücken lässt, das sich
nicht zeichnen lässt, verschiebt das Problem nur auf den Renderer — und der
löst es dann irgendwie.

Eine Tabelle darf auch in **keiner** Gruppe stehen. Gruppen sind ein
Hilfsmittel für große Modelle, keine Pflichtübung.

## 8.7 `Meta` und Styles

Eine `Group` ist eine Deklaration wie jede andere und darf `Meta` tragen
(siehe `03`):

```
Group Bestellwesen {
  Table Order;
  Table OrderPosition;

  Meta {
    note         = "Alles rund um die Bestellabwicklung";
    styleClasses = [highlight];
  }
}
```

Der Selektor `@group` gestaltet alle Gruppen (siehe `03`, Abschnitt 3.2).

## 8.8 Gruppen über Dateigrenzen

Da eine Gruppe quer zur Dateiaufteilung liegen darf, sind Mitglieder aus
importierten Dateien der Normalfall, nicht die Ausnahme:

```
Group Abrechnung {
  Table Invoice;
  Table billing.Payment;
  Table billing.Dunning;
}
```

Genau das ist der Grund, warum `Group` neben `Document` existiert. Sonst wäre
die Datei bereits die Gruppe.

## 8.9 Entschieden

- Die Art steht vor der Referenz und wird geprüft (8.4)
- Ein Name pro Namensraum, unabhängig von der Art (8.4)
- Die Gruppe nennt ihre Mitglieder (8.5)
- Keine Überschneidung, keine Verschachtelung (8.6)

## 8.10 Offene Fragen dieses Kapitels

1. Bestätigung: `Group` ist rein visuell und erzeugt kein Schema (8.2)
2. Sollen gruppenlose Tabellen einen Lint-Hinweis erzeugen? (8.5)
