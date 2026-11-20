# Mit Dateien arbeiten

## Kopieren und verschieben

- Mit **Kopieren** (<kbd>Strg</kbd>+<kbd>C</kbd>) oder **Ausschneiden** (<kbd>Strg</kbd>+<kbd>X</kbd>) nehmen Sie die ausgewählten Objekte in die Zwischenablage; gehen Sie dann in einen anderen Ordner und wählen Sie **Einfügen** (<kbd>Strg</kbd>+<kbd>V</kbd>). Ausschneiden und Einfügen verschiebt sie. **In Ordner einfügen** fügt in den ausgewählten Ordner ein statt in den, in dem Sie sind.
- **Kopieren nach …** und **Verschieben nach …** fragen nach dem Ordner, in den kopiert oder verschoben werden soll.
- **Duplizieren** legt eine Kopie neben dem Original an; **Verknüpfung anlegen** (<kbd>Strg</kbd>+<kbd>M</kbd>) legt eine Verknüpfung darauf an.

Arkhas Zwischenablage ist die, die auch Caja und Nautilus verwenden: Kopieren Sie in einem und fügen Sie im anderen ein.

### Ziehen

Ziehen Sie Objekte in einen anderen Ordner, auf einen Ordner, einen Reiter, einen Knopf der Pfadleiste oder einen Eintrag der Seitenleiste.

| Beim Ziehen gedrückt | Bewirkt |
|---|---|
| nichts | Verschieben, wenn es auf demselben Laufwerk bleibt; Kopieren, wenn es auf ein anderes geht |
| <kbd>Umschalt</kbd> | Verschieben |
| <kbd>Strg</kbd> | Kopieren |
| <kbd>Alt</kbd> | Beim Ablegen aus einem Menü wählen: Hierher verschieben, Hierher kopieren, Hierher verknüpfen, Hierher verschieben und prüfen, Hierher kopieren und prüfen |

Dateien, die Sie aus einer Archivverwaltung ziehen (Engrampa, Xarchiver, ältere File-Roller-Versionen), werden dort entpackt, wo Sie sie ablegen. Das braucht X11; unter Wayland können die Archivverwaltungen das nicht.

### Fortschritt

Kopien und Verschiebungen laufen im Hintergrund, sodass Sie weiterarbeiten können. Ein Knopf in der Ortsleiste zeigt, wie weit sie sind; klicken Sie darauf, um jeden Vorgang zu sehen und einen abzubrechen. Wenn Sie lieber ein eigenes Fenster möchten, schalten Sie **Einstellungen → Verhalten → Dateivorgänge in einem eigenen Fenster anzeigen** ein.

### Wenn ein Name schon vergeben ist

Liegt dort, wohin Sie kopieren oder verschieben, schon ein Objekt mit demselben Namen, fragt Arkha:

- **Überspringen:** dieses Objekt auslassen.
- **Beide behalten:** es unter einem freien Namen kopieren, etwa `foto (2).jpg`. Unter **Einen neuen Namen für das Ziel wählen** können Sie einen eigenen Namen eingeben.
- **Ersetzen:** es an die Stelle des anderen setzen. Bei zwei Ordnern heißt das **Zusammenführen**: Die Inhalte kommen zusammen, und Arkha fragt für jede Datei, die in beiden liegt, noch einmal nach.

**Diese Aktion auf alle Konflikte anwenden** antwortet für den Rest des Vorgangs genauso.

## Geprüfte Kopien

Wenn eine Kopie stimmen muss (Fotos, die Sie von der Speicherkarte der Kamera löschen werden, eine Sicherung), lassen Sie Arkha sie prüfen: Jede Kopie wird vom Laufwerk oder der Freigabe zurückgelesen, nicht aus dem Speicher, und mit ihrem Original verglichen. Eine Verschiebung löscht ein Original erst, wenn seine Kopie übereinstimmt.

- **Bearbeiten → Einfügen und prüfen** (<kbd>Strg</kbd>+<kbd>Umschalt</kbd>+<kbd>V</kbd>, auch im Kontextmenü eines Ordners)
- **Die Kopien prüfen** in Kopieren nach und Verschieben nach
- **Hierher kopieren und prüfen** oder **Hierher verschieben und prüfen** im Menü eines Ablegens mit <kbd>Alt</kbd>

Eine Kopie, die nicht übereinstimmt, fragt, ob sie noch einmal kopiert oder übersprungen werden soll. Am Ende sagt Arkha, dass alle Kopien übereinstimmten, oder listet die auf, bei denen es nicht so war.

## Der Papierkorb

- **In den Papierkorb verschieben** (<kbd>Entf</kbd>) verschiebt Objekte in den Papierkorb, aus dem Sie sie zurückholen können. Arkha kann vorher fragen: **Einstellungen → Verhalten → Vor dem Verschieben in den Papierkorb nachfragen**.
- **Gehe zu → Papierkorb** zeigt, was darin liegt. **Wiederherstellen** bringt ausgewählte Objekte dorthin zurück, woher sie kamen; die Listenansicht des Papierkorbs zeigt, wo das war.
- **Papierkorb leeren** (im Menü Datei, in der Leiste über dem Papierkorb oder in seinem Kontextmenü) löscht alles darin endgültig.
- **Löschen** (<kbd>Umschalt</kbd>+<kbd>Entf</kbd>) löscht Objekte sofort, ohne Papierkorb. Arkha fragt vorher; das lässt sich nicht rückgängig machen. Wo es keinen Papierkorb gibt (manche Laufwerke und Netzwerkfreigaben), bietet Arkha nach einer Nachfrage stattdessen das Löschen an.

## Rückgängig und Wiederholen

**Bearbeiten → Rückgängig** (<kbd>Strg</kbd>+<kbd>Z</kbd>) nimmt den letzten Dateivorgang zurück, und **Wiederholen** (<kbd>Strg</kbd>+<kbd>Umschalt</kbd>+<kbd>Z</kbd> oder <kbd>Strg</kbd>+<kbd>Y</kbd>) führt ihn erneut aus. Arkha merkt sich die letzten 20. Das können sein:

- Kopien und Verknüpfungen (die Kopien und Verknüpfungen kommen in den Papierkorb),
- Verschiebungen (die Objekte gehen dorthin zurück, wo sie waren; nach einem Zusammenführen wird nur zurückgenommen, was hinzukam, und eine Kopie, die eine Datei ersetzt hat, bleibt, wie sie ist),
- das Verschieben in den Papierkorb und das Wiederherstellen daraus,
- Umbenennungen (ein ganzes „Alle umbenennen“ als ein Schritt),
- neue Ordner und Dokumente,
- Änderungen an Zugriffsrechten, Besitzer und Gruppe in den Eigenschaften.

Endgültiges Löschen, das Leeren des Papierkorbs und **Auf enthaltene Objekte anwenden** in den Eigenschaften lassen sich nicht rückgängig machen. Alles Neue, was Sie nach einem Rückgängig tun, leert Wiederholen.

## Neue Ordner und Dokumente

Im Kontextmenü der freien Fläche eines Ordners:

- **Ordner anlegen** (<kbd>Strg</kbd>+<kbd>Umschalt</kbd>+<kbd>N</kbd>) legt einen neuen Ordner an und lässt Sie ihn benennen.
- **Dokument anlegen** legt ein **Leeres Dokument** an oder eine Kopie einer der Dateien in Ihrem Vorlagenordner (meist `~/Vorlagen`): Legen Sie dort einen Briefkopf oder eine Tabelle ab, mit der Sie oft anfangen, und sie wird hier angeboten.

## Archive

**Hier entpacken** entpackt ein ausgewähltes Archiv (`.zip`, `.tar.gz`, `.7z` …) in den Ordner, in dem es liegt, und **Komprimieren …** packt die ausgewählten Objekte in eines. Beides braucht Engrampa oder File Roller.

## Änderungen von anderswo

Arkha folgt Änderungen auf dem Datenträger: Dateien, die andere Programme hinzufügen, ändern, umbenennen oder löschen, erscheinen, ändern sich oder verschwinden sofort, ohne Neuladen. **Ansicht → Neu laden** (<kbd>F5</kbd>) liest einen Ordner trotzdem neu ein.
