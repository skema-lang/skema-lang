# 05 — Relationen

> Status: **Entwurf.** Abschnitte mit ⚠️ sind noch nicht entschieden.

## 5.1 Grundform

Eine Beziehung ist ein Feld, dessen Typ ein Pfeil auf ein anderes Feld ist:

```
Table Order {
  customer >Customer.id [onDelete=cascade];
}
```

Das fügt sich in die Grundform der Sprache (`04`, Abschnitt 4.1): `customer`
ist der Name, `>Customer.id` der Typ, `[…]` die Flags.

Der **Datentyp wird nicht angegeben** — er ergibt sich aus dem Zielfeld.
Zeigt `Customer.id` auf `int`, ist auch `Order.customer` ein `int`. Das ist
richtig so: zwei Enden einer Beziehung müssen denselben Typ haben, und was
sich ableiten lässt, sollte man nicht abschreiben müssen.

## 5.2 Die Pfeile

> **Der Pfeil zeigt dahin, wo es genau einen gibt.**

Das ist die ganze Regel. Alles andere folgt daraus.

| Schreibweise | Bedeutung | Fremdschlüsselspalte |
|---|---|---|
| `a >B.b` | viele `A` gehören zu einem `B` | entsteht in `A` |
| `a <B.b` | ein `A` hat viele `B` | entsteht in `B` |
| `a -B.b` | eins zu eins | entsteht in `A` |

```
Table Order {
  customer >Customer.id;      # viele Bestellungen -> ein Kunde
}
```

Weil der Pfeil auf die Eins-Seite zeigt, steht ein `>`-Feld **immer auf der
Viele-Seite** — also genau dort, wo auch die Spalte entsteht. Zeile und Spalte
sind am selben Ort. Das ist der Grund für diese Lesart, und dbml handhabt es
ebenso.

> **Korrektur an `docs/beschreibung.txt`:** dort ist `>` als „OneToMany" und
> `<` als „Many to One" beschriftet. Beides ist vertauscht. Angewendet auf
> Zeile 70 des Beispiels hätte eine Bestellung viele Kunden.

### Viele-zu-viele gibt es nicht als Pfeil

`<>` entfällt ersatzlos. SQL kennt keine n:m-Beziehung — es braucht immer eine
Zwischentabelle, und die schreibt man hin:

```
Table OrderPosition {
  order   >Order.id;
  product >Product.id;
  menge   int;
  preis   Money;
}
```

Das ist ehrlicher als ein Pfeil, der stillschweigend eine Tabelle erzeugt,
deren Namen niemand gewählt hat. Und es zahlt sich früh aus: fast jede
n:m-Beziehung bekommt irgendwann ein Zusatzfeld — Menge, Datum, Sortierung.
Wer sie automatisch erzeugen lässt, muss dann alles umschreiben.

## 5.3 Nicht jede Beziehung erzeugt eine Spalte

Das ist der Punkt, der beim Lesen am leichtesten übersehen wird, weil alle
Pfeile syntaktisch gleich aussehen:

```
Table Order {
  customer >Customer.id;      # erzeugt Spalte Order.customer
}

Table Customer {
  orders   <Order.customer;   # erzeugt KEINE Spalte
}
```

Ein `<`-Feld ist ein **Rückverweis**: es benennt die Gegenrichtung für
Diagramm, ORM und Dokumentation, aber der Fremdschlüssel liegt drüben. Wer
`orders` in der erzeugten Tabelle sucht, findet es nicht — und das ist
richtig so.

### Eins-zu-eins

`-` ist symmetrisch geschrieben, das Schema ist es nicht: eine der beiden
Tabellen bekommt die Spalte samt `UNIQUE`. Es ist **die Seite, auf der das
Feld steht** — dieselbe Regel wie bei `>`.

```
Table Profil {
  user -User.id;      # Spalte Profil.user, UNIQUE
}
```

Damit gilt für alle Pfeile dieselbe Merkregel: **wo die Zeile steht, entsteht
die Spalte** — außer bei `<`, das gar keine erzeugt.

## 5.4 Beide Seiten dürfen deklarieren — Flags nur eine

`A.x <B.y` und `B.y >A.x` beschreiben dieselbe Beziehung. Beide dürfen im
Modell stehen; das Werkzeug erkennt sie am **Spaltenpaar** und zeichnet eine
Kante, nicht zwei.

Damit daraus kein Widerspruch entstehen kann, gilt eine harte Regel:

> **Flags stehen ausschließlich auf der `>`-Seite.**

```
Table Order {
  customer >Customer.id [onDelete=cascade];    # hier steht das Verhalten
}

Table Customer {
  orders <Order.customer;                      # nur der Rückverweis
  orders <Order.customer [onDelete=restrict];  # FEHLER
}
```

Der Grund: `onDelete` beschreibt, was mit der Fremdschlüsselspalte geschieht.
Diese Spalte gibt es nur einmal, also darf es auch die Angabe nur einmal
geben. Wären beide Seiten erlaubt, müsste jemand zwei Stellen im Einklang
halten — und irgendwann tut es jemand nicht.

## 5.5 Flags an Beziehungen

```
customer >Customer.id [onDelete=cascade, onUpdate=restrict];
```

| Flag | Werte |
|---|---|
| `onDelete` | `cascade`, `restrict`, `setNull`, `setDefault`, `noAction` |
| `onUpdate` | dieselben |

**Werte werden kleingeschrieben**, wie alle Werte der Sprache
(`mode = draft`, `type = float`). Der Generator schreibt in der SQL-Ausgabe
groß — das ist seine Sache, nicht die des Modells.

### Zwei Korrekturen zum Ist-Zustand

1. **`RESTRICTED` gibt es nicht.** `docs/beschreibung.txt` nennt diesen Wert;
   SQL kennt `RESTRICT`, in dieser Sprache also `restrict`.
2. **`onDelete=CASCADE`** im Beispiel wird zu `onDelete=cascade`.

Auch die Schlüsselschreibweise ist noch uneinheitlich: `onDelete` im Beispiel
gegen `on_delete` in `docs/beschreibung.txt`. Nach `01`, Abschnitt 1.8 sind
eingebaute Schlüssel camelCase — also `onDelete`.

## 5.6 Die Zielspalte wird immer genannt

```
customer >Customer.id;      # richtig
customer >Customer;         # nicht erlaubt
```

Eine Kurzform, die stillschweigend den Primärschlüssel einsetzt, wäre kürzer.
Sie hat aber einen Preis, der erst beim Lesen fremder Modelle auffällt: man
müsste in der Zieltabelle nachschlagen, um zu wissen, worauf verwiesen wird.
Eine Beziehung ist die Stelle, an der zwei Tabellen sich berühren — dort ist
Ausdrücklichkeit die drei Zeichen wert.

## 5.7 ⚠️ Physischer Name der Fremdschlüsselspalte

```
Table Order {
  customer >Customer.id;
}
```

Heißt die Spalte `customer` oder `customer_id`? Die Sprache muss es sagen,
sonst tut es jeder Generator anders. Drei Möglichkeiten: der Feldname
unverändert, ein automatisches Anhängsel nach einer `Lint`- oder
`Project`-Regel, oder ausdrücklich über `[column="…"]` (siehe `07`).

Am ehrlichsten ist der Feldname unverändert — wer `customer_id` will, nennt
das Feld so.

## 5.8 Entschieden

- Der Pfeil zeigt auf die Eins-Seite; `>` steht auf der Viele-Seite (5.2)
- `<>` entfällt — n:m wird als eigene Tabelle geschrieben (5.2)
- `<` erzeugt keine Spalte, es ist ein Rückverweis (5.3)
- `-` erzeugt die Spalte dort, wo das Feld steht (5.3)
- Beide Seiten dürfen deklarieren, Flags nur auf der `>`-Seite (5.4)
- Flag-Werte werden kleingeschrieben: `cascade`, `restrict`, `setNull` (5.5)
- Die Zielspalte wird immer genannt, keine Kurzform (5.6)

## 5.9 Offene Fragen dieses Kapitels

1. Physischer Name der Fremdschlüsselspalte (5.7)
2. Muss ein `<`-Rückverweis auf eine tatsächlich existierende `>`-Beziehung
   zeigen, oder darf er allein stehen und die Beziehung selbst begründen?
