# 07 — Index und physisches Mapping

> Status: **Entwurf, in Arbeit.** Abschnitte mit ⚠️ sind noch nicht entschieden.

## 7.1 Das Problem

Ein Modell hat zwei Namen für jede Sache: den **fachlichen** und den
**physischen**.

```
Table Order          # fachlich:  Order
                     # physisch:  "orders" im Schema "sales"
```

Warum sie auseinanderfallen, hat gute Gründe: die Datenbank ist alt, ein
anderes Team hat sie benannt, `Order` ist in SQL ein reserviertes Wort, oder
die Hausregel verlangt Pluralformen. Die Sprache muss beides tragen können,
ohne das Modell hässlich zu machen.

Physisch zu benennen sind vier Dinge:

| Ebene | Beispiel |
|---|---|
| Datenbank | `shopsystem` |
| Schema | `public`, `sales` |
| Tabelle | `orders` |
| Spalte | `email_address` |

## 7.2 Der Ist-Zustand ist widersprüchlich

`examples/myexample.skema` löst dasselbe Problem auf zwei Wegen:

```
Table Customer {
  Database {
    name            = "shopsystem";
    postgres_schema = "public";
  }
}

Table Order {
  Meta {
    dbName = "order";
  }
}
```

Drei Probleme:

1. **`dbName` steht in `Meta`** und bricht damit die Grundregel der Sprache:
   `Meta` beschreibt, es verändert kein Schema. Der physische Tabellenname
   verändert sehr wohl das erzeugte DDL.
2. **Zwei Mechanismen für eine Sache.** Wer die Sprache lernt, muss raten,
   welcher gilt.
3. **`postgres_schema`** trägt den Dialekt im Schlüsselnamen — snake_case,
   dialektgebunden, und ohne Bezug zum `Dialect`-Block, den `Datatype`
   für genau diesen Zweck bereits verwendet. Zudem ist ein Schema kein
   Postgres-Sonderfall: Oracle und SQL Server kennen es ebenso.

Und eine offene Lesart: Steht `Database { name = "shopsystem" }` **innerhalb**
von `Table Customer`, ist unklar, ob `name` den Datenbanknamen oder den
Tabellennamen meint. Dem Wert nach ist die Datenbank gemeint — dann ist die
Angabe aber an der falschen Ebene, denn sie gilt kaum nur für diese eine
Tabelle.

## 7.3 Der `Database`-Block

`Database` ist der **einzige** Ort für physische Namen. `Meta.dbName`
entfällt ersatzlos.

### Schlüssel

| Schlüssel | Bedeutung | erlaubt in |
|---|---|---|
| `database` | Name der Datenbank | `Project`, `Document` |
| `schema` | Name des Schemas | `Project`, `Document`, `Table` |
| `table` | physischer Tabellenname | `Table` |

Jeder Schlüssel bedeutet auf jeder Ebene dasselbe. Ein einzelnes `name`, das
im `Project` die Datenbank und in der `Table` die Tabelle meinte, wäre kürzer
— aber es wäre derselbe Fehler, den `spec` gegenüber `version` bewusst
vermeidet: ein Schlüssel, dessen Bedeutung von seiner Position abhängt.

### Drei Ebenen, eine Überschreibungsregel

```
# ---- project.skema ----
Project {
  Database {
    database = "shopsystem";
    schema   = "public";
  }
}

# ---- order.skema ----
Document shop {
  Database { schema = "sales"; }     # gilt für alle Tabellen dieser Datei
}

Table Order {
  Database { table = "orders"; }     # -> shopsystem.sales.orders
}

Table Position {
  Database { schema = "archiv"; }    # -> shopsystem.archiv.Position
}

Table Note { }                       # -> shopsystem.sales.Note
```

Die Kaskade ist dieselbe wie bei `Style`: **allgemein wird von speziell
überschrieben.** `Project` setzt die Vorgabe, `Document` verfeinert sie für
eine Datei, `Table` für den Einzelfall.

Was nicht angegeben ist, wird aus dem fachlichen Namen abgeleitet: `Table Note`
ohne `table`-Schlüssel heißt physisch `Note`.

> Der praktische Gewinn liegt auf der `Document`-Ebene: eine Datei entspricht
> meist genau einem Schema. Ohne diese Ebene stünde `schema = "sales"` in jeder
> einzelnen Tabelle der Datei.

Das physische Schema wird dabei **nicht** automatisch aus dem Document-Namen
abgeleitet. Namensraum und Schema sind zwei verschiedene Dinge, und sie später
wieder zu trennen wäre teuer — wer beide gleich haben will, schreibt eine
Zeile.

## 7.4 Dialektspezifische Angaben

Manche Angaben gelten nur für eine Engine: `TABLESPACE` in Postgres, `ENGINE`
und `CHARSET` in MySQL. Sie stehen in einem **benannten `Dialect`-Block** —
der Name ist die Engine:

```
Table Order {
  Database {
    table = "orders";

    Dialect postgres { tablespace = "fast_ssd"; }
    Dialect mysql    { engine = "InnoDB"; charset = "utf8mb4"; }
  }
}
```

Dieselbe Form gilt in `Datatype`, damit es in der ganzen Sprache genau eine
Schreibweise für „das hier gilt nur für Engine X" gibt:

```
Datatype decimal(precision, scale) {
  Dialect postgres { sql = "numeric({precision}, {scale})"; }
  Dialect mysql    { sql = "DECIMAL({precision}, {scale})"; }
}
```

Der Schlüssel heißt `sql`, weil er genau das enthält: den SQL-Typausdruck
dieser Engine. Ein neutraler Name wie `template` beschriebe die Technik
(Platzhalter einsetzen) statt die Sache. Und `sql` lässt Raum für weitere
Schlüssel je Engine — Details in `09-datatype.md`.

Der Gewinn ist doppelt: dialektgebundene Angaben stehen sichtbar getrennt von
den portablen — wer den Dialekt wechselt, sieht sofort, was nicht mitkommt —
und die Sprache hat ein Muster weniger.

⚠️ Offen: Engine-Namen sind eingebaute Bezeichner der Sprache. Nach der Regel
aus `03` (`@` markiert Eingebautes) müsste es `Dialect @postgres` heißen.
Siehe Frage 2.

## 7.5 Physische Spaltennamen ⚠️

Ein Feld hat keine Unterblöcke außer seinem `{}`-Attributblock — und der ist
laut Grundregel für Beschreibungen reserviert. Der physische Spaltenname
verändert aber DDL und gehört damit in die eckigen Klammern:

```
Table Customer {
  email varchar [column="email_address"];
}
```

Damit gilt auf jeder Ebene die Regel der Sprache: `[]` erzeugt DDL. Dass
Tabellen einen Block und Felder ein Flag verwenden, folgt daraus, dass
Tabellen mehrere physische Angaben tragen (Datenbank, Schema, Name) und
Felder genau eine.

## 7.6 `Index`

Der `Index`-Block sammelt die Indizes einer Tabelle. Jeder Eintrag hat die
Form:

```
[name] (spalte, …) [flags];
```

Der Name ist **optional** und steht vorn — wie überall in der Sprache, wo
etwas benannt wird:

```
Table Customer {
  email  varchar;
  kunde  int;
  datum  date;

  Index {
    (email) [unique];                        # ohne Namen
    ix_kunde_datum (kunde, datum) [unique];  # mit Namen
  }
}
```

**Warum der Name sich lohnt:** ohne ihn erzeugt der Generator einen. Ändert
sich später eine Spalte, ändert sich der erzeugte Name — und die Migration
löscht einen Index und legt einen scheinbar neuen an, obwohl fachlich nichts
passiert ist. Ein gesetzter Name macht den Index über die Zeit stabil.

### Reihenfolge ist bedeutsam

Bei zusammengesetzten Indizes ist `(kunde, datum)` **nicht** dasselbe wie
`(datum, kunde)`. Der Index bedient Abfragen, die mit der ersten Spalte
beginnen. Die Sprache ändert die Reihenfolge nicht, auch der Formatter nicht.

### `[unique]` gibt es zweimal

```
publicId uuid [unique];      # am Feld
Index { (publicId) [unique]; }   # im Block
```

Beides erzeugt dasselbe DDL. Das ist Absicht: die Kurzform am Feld für den
Normalfall einer einzelnen Spalte, der Block für alles Zusammengesetzte und
alles Benannte. Beides gleichzeitig zu schreiben ist ein Lint-Hinweis, kein
Fehler.

### ⚠️ Noch offen

- **Index-Typen** — `[type=btree]`, `[type=gin]`, `[type=hash]`. Dialektgebunden;
  gehört das in die Flag-Liste oder in einen `Dialect`-Block?
- **Teilindizes** — `WHERE deleted_at IS NULL`. Als Raw-Ausdruck denkbar:
  `[where=`deleted_at IS NULL`]`.
- **Ausdrucksindizes** — `(lower(email))`. Braucht Raw-Ausdrücke in der
  Spaltenliste.
- **Namensschema für erzeugte Namen** — nach welchem Muster benennt der
  Generator einen namenlosen Index?

## 7.7 Entschieden

- `Database` ist der einzige Ort für physische Namen; `Meta.dbName` entfällt (7.3)
- Schlüssel `database`, `schema`, `table` — auf jeder Ebene dieselbe Bedeutung (7.3)
- Drei Ebenen: `Project`, `Document`, `Table`, speziell überschreibt allgemein (7.3)
- Das Schema wird **nicht** automatisch aus dem Document-Namen abgeleitet (7.3)
- Dialektspezifisches steht in benannten `Dialect <engine>`-Blöcken — auch in
  `Datatype` (7.4)
- In `Datatype` heißt der Typausdruck `sql` (7.4)
- Index-Einträge tragen einen optionalen Namen vorn (7.6)

## 7.8 Offene Fragen dieses Kapitels

1. Physischer Spaltenname am Feld — als Flag `[column="…"]`? (7.5)
2. `@` vor Engine-Namen: `Dialect @postgres` statt `Dialect postgres`? (7.4)
3. Index-Typen, Teilindizes, Ausdrucksindizes (7.6)
4. Namensschema für automatisch erzeugte Index-Namen (7.6)
