# Arkha anpassen

## Das Kontextmenü

**Einstellungen → Kontextmenü** listet die Einträge der beiden Kontextmenüs auf: **Dateien und Ordner** (ein Rechtsklick auf Objekte) und **Freie Fläche eines Ordners**. Nehmen Sie den Haken bei einem Eintrag weg, um ihn auszublenden, und ziehen Sie ihn an seinem Griff (oder wählen Sie ihn aus und nehmen Sie die Pfeile), um ihn zu verschieben. **Trennlinie hinzufügen** setzt eine Linie zwischen Einträge; **Reihenfolge zurücksetzen** bringt jeden Eintrag an seinen üblichen Platz zurück. Einträge, die nicht passen, werden von selbst weggelassen, etwa Wiederherstellen außerhalb des Papierkorbs.

## Eigene Befehle

**Einstellungen → Eigene Aktionen** fügt den Kontextmenüs Einträge hinzu, die einen eigenen Befehl mit den ausgewählten Dateien ausführen: sie in einer Bildbearbeitung öffnen, sie umwandeln, sie irgendwohin schicken.

**Hinzufügen …** fragt nach:

- **Name:** was das Menü zeigt.
- **Befehl:** was ausgeführt wird, in dem Ordner, in dem Sie sind. Darin stehen diese für die Auswahl:

  | | Steht für |
  |---|---|
  | `%f` | Die erste ausgewählte Datei |
  | `%F` | Alle ausgewählten Dateien |
  | `%n` | Den Namen der ersten ausgewählten Datei, ohne ihren Ordner |
  | `%N` | Die Namen aller ausgewählten Dateien |
  | `%d` | Den Ordner, in dem Sie sind |
  | `%%` | Ein `%`-Zeichen |

  Pfade werden für Sie in Anführungszeichen gesetzt, sodass Namen mit Leerzeichen funktionieren: Schreiben Sie `gimp %F`, nicht `gimp "%F"`.

- **Anzeigen für:** Dateien, Ordner und/oder die freie Fläche eines Ordners (wo `%d` der Ordner ist und nichts ausgewählt ist).
- **Dateitypen** und **Namensmuster:** ihn nur für manche Dateien anbieten: Videos, Audio, Bilder, Textdateien, Dokumente oder Archive und/oder Namen wie `*.srt; *.nfo`. Eine Datei braucht einen der angehakten Typen oder einen Namen, der zu einem Muster passt; ist beides leer, wird die Aktion für jede Datei angeboten. Ordner richten sich nur nach den Mustern.

Einige Beispiele:

| Name | Befehl | Anzeigen für |
|---|---|---|
| In GIMP bearbeiten | `gimp %F` | Dateien, Bilder |
| Als PDF speichern | `libreoffice --headless --convert-to pdf %F` | Dateien, Dokumente |
| Als tar.xz packen | `tar -cJf archiv.tar.xz %N` | Dateien und Ordner |
| In VS Code öffnen | `code %d` | Freie Fläche |

Befehle laufen mit `sh`, es sei denn, Sie stellen unter **Befehle ausführen mit** eine andere Shell ein, eine, die `-c` versteht (bash, zsh, fish). Ihre eigenen Befehle werden nur für Ordner auf diesem Rechner angeboten, nicht auf Netzwerkorten oder im Papierkorb, da sie Pfade bekommen.

Ihre Einträge erscheinen in den Kontextmenüs und lassen sich dort wie die anderen verschieben und ausblenden, unter **Einstellungen → Kontextmenü**.

## Eigene Vorschauerzeuger

Arkha verwendet die Vorschauerzeuger, die auf Ihrem System installiert sind (für PDFs, Videos, Office-Dokumente, Comics und was sie sonst können). Unter **Einstellungen → Vorschau** können Sie eigene hinzufügen, die für ihre Typen vor ihnen kommen:

- **Name:** für die Liste.
- **Dateitypen:** MIME-Typen, durch `;` getrennt, wobei `*` auf alles passt: `video/*`, `image/x-krita`.
- **Befehl:** erzeugt das Vorschaubild. `%i` ist die Datei, `%o` das zu schreibende Bild (PNG oder JPEG) und `%s` die Größe in Pixeln. Zum Beispiel: `ffmpegthumbnailer -i %i -o %o -s %s -f`.

Der Befehl läuft mit `sh` und darf bis zu 30 Sekunden brauchen. Er läuft so, wie Sie ihn geschrieben haben, mit Zugriff auf Ihre Dateien, und darf daher ein Skript in Ihrem persönlichen Ordner verwenden. (Die Vorschauerzeuger des Systems und die mitgelieferten von Arkha laufen dagegen in einer Sandbox, wenn [bubblewrap](https://github.com/containers/bubblewrap) installiert ist: Sie sehen nur das System und die Datei, für die sie ein Vorschaubild erzeugen, sodass eine Datei, die einen Vorschauerzeuger angreifen soll, Ihre anderen Dateien nicht erreicht.)

## Tastenkürzel

**Einstellungen → Tastatur** listet jede Aktion auf, die ein Tastenkürzel haben kann. Doppelklicken Sie auf eine Aktion (oder wählen Sie sie aus und drücken Sie **Ändern …**) und drücken Sie dann die neuen Tasten; <kbd>Esc</kbd> bricht ab. **Leeren** entfernt ein Tastenkürzel, **Zurücksetzen** holt das übliche zurück und **Alle zurücksetzen** alle. Geänderte Tastenkürzel werden fett angezeigt.

Eine Buchstaben- oder Zifferntaste braucht <kbd>Strg</kbd>, <kbd>Alt</kbd> oder <kbd>Super</kbd> dazu, damit das Tippen eines Namens, um ein Objekt zu finden, weiter funktioniert. Gehören die Tasten schon einer anderen Aktion, fragt Arkha, ob sie neu zugewiesen werden sollen.

**Hilfe → Tastenkürzel** (<kbd>Strg</kbd>+<kbd>F1</kbd>) zeigt die Tastenkürzel, wie sie gerade sind; [Tastenkürzel](shortcuts.md) listet die üblichen auf.

## Die Knöpfe der Werkzeugleiste

Die Knöpfe neben der Pfadleiste lassen sich anders anordnen: Schalten Sie **Ansicht → Werkzeugleiste sperren** aus (am Anfang ist es eingeschaltet, damit sie nicht aus Versehen verrutschen) und ziehen Sie sie. Die, die Sie nicht brauchen, blenden Sie unter **Einstellungen → Anzeige → Fenster** aus.
