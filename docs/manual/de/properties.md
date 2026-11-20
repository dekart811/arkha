# Eigenschaften

**Eigenschaften** (<kbd>Alt</kbd>+<kbd>Eingabe</kbd> oder <kbd>Strg</kbd>+<kbd>I</kbd> oder im Kontextmenü) öffnet ein Fenster mit allem über die ausgewählten Objekte, in Reitern. Bei mehreren ausgewählten Objekten zeigt es, was sie gemeinsam haben, und ändert alle auf einmal.

Um die Eigenschaften eines anderen Objekts zu sehen, ziehen Sie es in das offene Eigenschaftenfenster: Das Fenster zeigt es, auf dem Reiter, den Sie gewählt hatten.

## Allgemein

Der Name (den Sie hier ändern können), der Typ, die Größe, der Ort und die Daten, an denen es geändert, zuletzt geöffnet und erstellt wurde. Die Größe eines Ordners wird im Hintergrund gezählt, zusammen mit der Zahl der Objekte darin. Für den obersten Ordner eines Laufwerks zeigt Allgemein auch, wie voll das Laufwerk ist.

**Klicken Sie auf das Symbol**, um dem Objekt ein eigenes Symbol aus einer Bilddatei zu geben; in diesem Fenster kehrt **Zurücksetzen** zum üblichen Symbol zurück. Caja zeigt dasselbe Symbol.

## Embleme

Embleme sind kleine Bilder auf dem Symbol eines Objekts, etwa ein Stern oder „wichtig“, aus Ihrem Symbolthema. Haken Sie die an, die das Objekt haben soll. Sie werden von GVfs aufbewahrt und mit Caja geteilt.

Sie können Embleme auch auf Objekte ziehen: siehe [Hintergründe und Embleme](#hintergründe-und-embleme) unten.

## Zugriffsrechte

Wer das Objekt lesen, ändern (schreiben) und ausführen darf: sein **Besitzer**, seine **Gruppe** und **Andere**. Bei einem Ordner bedeutet Ausführen, hineingehen zu dürfen. Nur der Besitzer kann sie ändern.

- **Besondere** Zugriffsrechte: **Benutzerkennung setzen** und **Gruppenkennung setzen** lassen ein Programm als sein Besitzer oder seine Gruppe laufen, wer auch immer es startet; ein Ordner mit **Sticky** lässt nur den Besitzer eines Objekts es löschen oder umbenennen.
- Die **Gruppe** kann der Besitzer in eine andere seiner Gruppen ändern. Nur der Systemverwalter kann ein Objekt einem anderen Besitzer geben.
- Bei einem Ordner gibt **Auf enthaltene Objekte anwenden …** allem darin dieselben Zugriffsrechte. Das lässt sich nicht rückgängig machen.

Wo das System SELinux verwendet, steht hier auch der SELinux-Sicherheitskontext.

## Öffnen mit

Welche Anwendung Dateien dieses Typs öffnet. Wählen Sie eine andere aus und drücken Sie **Als Vorgabe festlegen**.

## Notizen

Eine Notiz, die beim Objekt aufbewahrt wird, für alles, was Sie sich dazu merken wollen. Objekte mit einer Notiz zeigen das Notiz-Emblem. Notizen werden von GVfs aufbewahrt und mit Caja geteilt. Eine Notiz wird einen Moment, nachdem Sie aufgehört haben zu tippen, gespeichert, und wenn das Fenster geschlossen wird.

## Medieninfo

Für Videos, Musik und Bilder: Codecs, Auflösung, Dauer, Bitrate und Ähnliches, wie [MediaInfo](https://mediaarea.net/MediaInfo) sie meldet. Das braucht das Programm `mediainfo`. Welche Arten von Dateien diesen Reiter bekommen, stellen Sie unter **Einstellungen → Vorschau → Medieninfo** ein.

## Prüfsummen

<picture>
<source media="(prefers-color-scheme: dark)" srcset="../../screenshots/checksums-dark.png">
<img src="../../screenshots/checksums-light.png" alt="Der Reiter Prüfsummen in den Eigenschaften eines Bildes, mit CRC32, MD5, SHA-1 und SHA-256">
</picture>

Eine Prüfsumme ist eine lange Zahl, die aus dem Inhalt einer Datei berechnet wird: Haben zwei Dateien dieselbe Prüfsumme, sind sie gleich. Webseiten geben sie für Downloads an, damit Sie prüfen können, ob Ihre Datei unverändert angekommen ist.

Drücken Sie den Knopf **Berechnen** einer Prüfsumme, um sie auszurechnen (CRC32, MD5, SHA-1, SHA-256 und, wenn Sie sie wählen, auch SHA-384, SHA-512 und BLAKE2b), oder **Alle berechnen**, um alle in einem Durchgang auszurechnen. Ein Fortschrittsbalken zeigt die verbleibende Zeit, und die Berechnung lässt sich abbrechen. **Kopieren** legt eine Prüfsumme in die Zwischenablage. Welche Prüfsummen angeboten werden, und in welcher Reihenfolge, stellen Sie unter **Einstellungen → Eigenschaften → Prüfsummen** ein.

## Die Reiter wählen

Klicken Sie mit der rechten Maustaste auf einen Reiter und wählen Sie **Diesen Reiter ausblenden**, um ihn aus dem Fenster zu nehmen (das löscht nichts); zurück holen Sie ihn unter **Einstellungen → Eigenschaften**. Alle Reiter außer Allgemein lassen sich ausblenden. Ziehen Sie die Reiter im Fenster selbst in eine andere Reihenfolge; Arkha behält sie.

## Hintergründe und Embleme

**Bearbeiten → Hintergründe und Embleme …** öffnet ein Fenster mit Mustern, Farben und Emblemen, wie in Caja:

- Ziehen Sie ein Muster oder eine Farbe auf die freie Fläche eines Ordners, um diesem Ordner einen Hintergrund zu geben (Caja zeigt ihn auch), oder auf die Seitenleiste. Ziehen Sie **Zurücksetzen** dorthin, um ihn wieder zu entfernen.
- Ziehen Sie ein Emblem auf ein Objekt oder auf alle ausgewählten. Ziehen Sie **Löschen** auf ein Objekt, um seine Embleme zu entfernen.
- Fügen Sie eigene Farben, Muster und Embleme hinzu; ein Rechtsklick entfernt sie. Ihre eigenen Embleme werden dort aufbewahrt, wo Caja sie aufbewahrt, sodass beide sie zeigen.
