# Suchen und Auswählen

## Suchen

Drücken Sie <kbd>Strg</kbd>+<kbd>F</kbd> (**Gehe zu → Filter …** oder der Knopf Suchen in der Werkzeugleiste), und ein Suchfeld tritt an die Stelle des Pfads.

- Tippen Sie, was Sie suchen, und drücken Sie **Eingabe**: Arkha durchsucht den offenen Ordner und alle Ordner darin (siehe [Nur diesen Ordner](#nur-diesen-ordner)) nach Namen, die es enthalten, jedes Wort davon, in beliebiger Reihenfolge, in Groß- oder Kleinschreibung (`ord unb` findet „Unbenannter Ordner“).
- Die Ergebnisse erscheinen als eigener Ort, mit einer Spalte **Ort**, die sagt, wo jedes Objekt liegt; **Zurück** führt zum Ordner zurück. **Übergeordneten Ort öffnen** im Kontextmenü eines Ergebnisses öffnet den Ordner, in dem es liegt.
- Um mit anderen Wörtern neu zu suchen, ändern Sie sie und drücken erneut **Eingabe**.
- <kbd>Esc</kbd> schließt das Suchfeld.

### Weitere Kriterien

Über den Ergebnissen können Sie, wie in Caja, die Suche eingrenzen. Der Knopf **+** fügt ein Kriterium hinzu, **−** entfernt es, und **Neu laden** sucht mit ihnen erneut:

| Kriterium | Findet |
|---|---|
| **Ort** | Objekte in einem anderen Ordner als dem offenen; ist **Unterordner einbeziehen** angehakt, auch in den Ordnern darin |
| **Dateityp** | Dokumente, Musik, Videos, Bilder, Illustrationen, Tabellen, Präsentationen, PDF und PostScript, Textdateien oder jeden anderen Typ, den Sie wählen (**Anderer Typ …**) |
| **Schlagwörter** | Dateien, die alle Schlagwörter haben, die Sie durch Leerzeichen getrennt eingeben |
| **Änderungszeit** | Dateien, die innerhalb von oder vor mehr als einer Stunde, einem Tag, einer Woche, einem Monat, sechs Monaten oder einem Jahr geändert wurden |
| **Größe** | Dateien, die höchstens oder mindestens so groß sind wie gewählt |
| **Enthaltener Text** | Dateien, die den Text enthalten |

### Nur diesen Ordner

Um nur die Objekte des Ordners selbst zu durchsuchen, nicht die Ordner darin, nehmen Sie in der Zeile Ort über den Ergebnissen den Haken bei **Unterordner einbeziehen** weg und drücken Sie **Neu laden**. Ordner, deren Namen passen, werden weiter gefunden; was in ihnen liegt, nicht.

Ob eine neue Suche mit angehaktem **Unterordner einbeziehen** beginnt, stellen Sie unter **Einstellungen → Verhalten → Unterordner einbeziehen** ein. Am Anfang ist es eingeschaltet.

## Auswählen

| Um auszuwählen | Nehmen Sie |
|---|---|
| alles im Ordner | **Bearbeiten → Alles auswählen** (<kbd>Strg</kbd>+<kbd>A</kbd>) |
| Objekte, deren Namen zu einem Muster passen | **Bearbeiten → Passende Objekte auswählen …** (<kbd>Strg</kbd>+<kbd>S</kbd>) |
| alles, was nicht ausgewählt ist, und nichts, was es ist | **Bearbeiten → Auswahl umkehren** (<kbd>Strg</kbd>+<kbd>Umschalt</kbd>+<kbd>I</kbd>) |
| das erste Objekt, dessen Name mit einigen Buchstaben beginnt | Tippen Sie sie |

In einem Muster steht `*` für beliebig viele Zeichen, `?` für eines und `[abc]` für eines der Zeichen in den Klammern: `*.jpg` wählt alle JPEG-Bilder aus, `IMG_20??*` alle Namen, die mit `IMG_20` und zwei weiteren Zeichen beginnen. Groß- und Kleinschreibung spielen keine Rolle.
