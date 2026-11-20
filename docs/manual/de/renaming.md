# Umbenennen

## Ein Objekt

Wählen Sie es aus und drücken Sie <kbd>F2</kbd> (oder **Umbenennen …** in seinem Kontextmenü), tippen Sie den neuen Namen und drücken Sie <kbd>Eingabe</kbd>. Arkha wählt den Namen ohne seine Dateiendung aus, sodass `urlaub.jpg` sein `.jpg` behält, wenn Sie es nicht mit auswählen. <kbd>Esc</kbd> behält den alten Namen.

## Viele Objekte auf einmal: Alle umbenennen

Wählen Sie die Objekte aus, klicken Sie mit der rechten Maustaste darauf und wählen Sie **Alle umbenennen …** (bei einem einzelnen Objekt heißt dasselbe Fenster **Erweitertes Umbenennen …**). Es funktioniert wie caja-rename von MATE und zeigt jeden neuen Namen neben dem alten, während Sie die Einstellungen ändern. **Anwenden** benennt sie um und lässt das Fenster für weitere Änderungen offen.

<picture>
<source media="(prefers-color-scheme: dark)" srcset="../../screenshots/rename-dark.png">
<img src="../../screenshots/rename-light.png" alt="Alle umbenennen ersetzt IMG_ durch Holiday in acht Bildnamen, die neuen Namen neben den alten">
</picture>

Die Reiter wirken zusammen, in dieser Reihenfolge: Schreibung, Einfügen, Entfernen, Ersetzen, Regex und zuletzt Nummerieren.

| Reiter | Bewirkt | Beispiel |
|---|---|---|
| **Schreibung** | Großbuchstaben, Kleinbuchstaben, Wortanfänge groß, oder lässt sie unverändert | `IMG_0042.JPG` → `img_0042.jpg` |
| **Einfügen** | Fügt Text an einer Position ein, vom Anfang gezählt oder mit **Vom Ende her** vom Ende; **Vor der Dateiendung** zählt nur im Namen vor ihr | `Bericht.pdf` → `2026 Bericht.pdf` |
| **Entfernen** | Nimmt **Länge** Zeichen ab **Ab** weg (ein negatives Ab zählt vom Ende) | `IMG_0042.jpg` → `0042.jpg` |
| **Ersetzen** | Ersetzt jedes Vorkommen eines Textes; **Dateiendungen auslassen** lässt sie aus | `Foto 1.jpg` → `Urlaub 1.jpg` |
| **Regex** | Wie Ersetzen, mit einem regulären Ausdruck | `2026-11-20 notizen.txt` → `20.11.2026 notizen.txt` |
| **Nummerieren** | Nummeriert die Objekte in der gezeigten Reihenfolge, ab **Beginn**, mit mindestens **Stellen** Ziffern; vor dem Namen oder **Am Ende** (und **Vor der Dateiendung**) | `strand.jpg`, `düne.jpg` → `01 strand.jpg`, `02 düne.jpg` |

Die Reiter lassen sich in eine andere Reihenfolge ziehen; Arkha behält sie.

### Reguläre Ausdrücke

Im Reiter Regex wird nach dem **Regulären Ausdruck** in jedem Namen gesucht, und jeder Treffer wird durch **Ersetzen durch** ersetzt. Dort fügen `$1`, `$2` … ein, was die Gruppen des Ausdrucks in Klammern gefunden haben, `${name}` eine benannte Gruppe, `$0` den ganzen Treffer und `$$` ein Dollarzeichen. Das Beispiel oben verwendet `(\d+)-(\d+)-(\d+)` und `$3.$2.$1`. Ein Fehler im Ausdruck wird darunter angezeigt; solange er nicht behoben ist, ändert der Reiter Regex nichts.

### Bevor etwas umbenannt wird

Arkha lehnt Namen ab, die sich nicht verwenden lassen (leere, `.`, `..` oder mit einem `/`), listet die Objekte auf, die überschrieben würden, und fragt vorher, und sagt Ihnen, wenn manche nicht umbenannt werden konnten. **Bearbeiten → Rückgängig** nimmt das ganze „Alle umbenennen“ in einem Schritt zurück.
