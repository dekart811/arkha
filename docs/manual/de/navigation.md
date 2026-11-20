# Sich zurechtfinden

## Die Pfadleiste

Die Pfadleiste über den Dateien hat einen Knopf für jeden Ordner auf dem Weg zu dem, in dem Sie sind: `Persönlicher Ordner › Bilder › 2026`. Klicken Sie auf einen Knopf, um zu diesem Ordner zu gehen, oder mit der mittleren Maustaste, um den Ordner in einem neuen Reiter zu öffnen. Dateien, die Sie auf einen Knopf ziehen, kommen in diesen Ordner.

## Einen Ort eingeben

Drücken Sie <kbd>Strg</kbd>+<kbd>L</kbd> (**Gehe zu → Ort …**), und die Pfadleiste wird zu einem Textfeld. Tippen oder fügen Sie einen Pfad ein und drücken Sie <kbd>Eingabe</kbd>. Während Sie tippen, ergänzt Arkha den angefangenen Ordnernamen, ausgewählt, sodass Sie einfach weitertippen können. <kbd>Esc</kbd> bringt die Knöpfe zurück.

Das Feld nimmt auch Adressen von Servern an, etwa `sftp://server/ordner` oder `smb://nas/freigabe`: siehe [Laufwerke und Netzwerkorte](drives-and-network.md#netzwerkorte).

Um den Ort immer als Text zu sehen, schalten Sie **Einstellungen → Anzeige → Den Ort immer als Text anzeigen** ein.

## Zurück, vor und nach oben

| Um | Nehmen Sie |
|---|---|
| zum Ordner zurückzugehen, in dem Sie vorher waren | den Knopf Zurück, <kbd>Alt</kbd>+<kbd>←</kbd> oder die Zurück-Taste der Maus |
| wieder vorzugehen | den Knopf Vor, <kbd>Alt</kbd>+<kbd>→</kbd> oder die Vor-Taste der Maus |
| zum Ordner zu gehen, der diesen enthält | den Knopf Nach oben, <kbd>Alt</kbd>+<kbd>↑</kbd> oder <kbd>Rücktaste</kbd> |
| nach Hause zu gehen | den Knopf Persönlicher Ordner oder <kbd>Alt</kbd>+<kbd>Pos1</kbd> |

Ein Rechtsklick auf den Knopf Zurück listet die Ordner auf, zu denen Sie zurückgehen können. Jeder Reiter hat seine eigene Chronik; **Gehe zu → Chronik leeren** vergisst sie.

Die Zurück- und Vor-Tasten der Maus sind bei den meisten Mäusen die Tasten 8 und 9. Sind es bei Ihrer andere, stellen Sie sie in Arkhas Einstellungsdatei ein (siehe [Fragen und Probleme](troubleshooting.md#wo-arkha-seine-einstellungen-aufbewahrt)): `mouse_back_button` und `mouse_forward_button`, wie Caja sie aufbewahrt.

## Das Menü Gehe zu

Außer Zurück, Vor und Übergeordneten Ordner öffnen führt das Menü Gehe zu direkt zu Ihrem **Persönlichen Ordner**, zum **Schreibtisch**, zum **Dateisystem** (dem obersten Ordner, `/`) und zum **Papierkorb**. Jedem davon können Sie ein Tastenkürzel geben (siehe [Arkha anpassen](customizing.md#tastenkürzel)).

## Lesezeichen

Lesezeichen sind Ordner, die Sie oft besuchen; sie stehen in der Seitenleiste und im Menü Lesezeichen.

- **Lesezeichen → Lesezeichen hinzufügen** (<kbd>Strg</kbd>+<kbd>D</kbd>) setzt ein Lesezeichen für den Ordner, in dem Sie sind. Ordner, die Sie auf die Überschrift Lesezeichen in der Seitenleiste oder auf ein Lesezeichen ziehen, werden ebenfalls zu Lesezeichen (mit <kbd>Umschalt</kbd>, auf ein Lesezeichen abgelegt, kommen sie stattdessen in dessen Ordner).
- **Lesezeichen → Lesezeichen bearbeiten …** (<kbd>Strg</kbd>+<kbd>B</kbd>) listet sie auf: Ziehen Sie sie in eine andere Reihenfolge, doppelklicken Sie auf einen Namen, um ihn zu ändern, fügen Sie welche hinzu oder entfernen Sie sie.

Arkhas Lesezeichen sind die, die auch Caja, andere GTK-Dateiverwaltungen und der GTK-Dateiauswahldialog (die Öffnen- und Speichern-Dialoge der meisten Programme) verwenden: Ein Lesezeichen, das in einem hinzugefügt wird, erscheint in allen.

## Die Seitenleiste

Die Seitenleiste (<kbd>F9</kbd> blendet sie ein oder aus) hat drei Abschnitte:

- **Rechner:** Ihr persönlicher Ordner, der Schreibtisch, das Dateisystem, der Papierkorb und Ähnliches.
- **Geräte:** Laufwerke, Speicherkarten, Discs, Telefone und Server: siehe [Laufwerke und Netzwerkorte](drives-and-network.md).
- **Lesezeichen:** Ihre Lesezeichen.

Ziehen Sie die Überschrift eines Abschnitts nach oben oder unten, um die Abschnitte anders anzuordnen. Welche Einträge der Abschnitt Rechner zeigt und wie hoch die Zeilen sind, stellen Sie unter **Einstellungen → Seitenleiste** ein. Dateien, die auf einem Eintrag abgelegt werden, kommen dorthin.
