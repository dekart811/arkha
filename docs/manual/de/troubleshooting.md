# Fragen und Probleme

## Vorschaubilder erscheinen nicht

- **Ist es für diesen Ort eingeschaltet?** Zunächst werden Vorschaubilder nur für Dateien auf diesem Rechner erzeugt, nicht auf Netzwerkorten oder Telefonen, und nicht für Bilder über 100 MB. Beides stellen Sie unter **Einstellungen → Vorschau** ein.
- **Gibt es einen Vorschauerzeuger für den Typ?** Von Bildern erzeugt Arkha Vorschaubilder selbst. Für andere Dateien verwendet es die Vorschauerzeuger, die auf dem System installiert sind, wie Caja: Evince oder Atril für PDF, PostScript, DjVu und Comics, ffmpegthumbnailer (oder ffmpeg) für Videos, libgsf für Office-Dokumente und so weiter. Installieren Sie den für Ihre Dateien oder fügen Sie einen eigenen hinzu (siehe [Arkha anpassen](customizing.md#eigene-vorschauerzeuger)).
- **Ist es schon einmal fehlgeschlagen?** Scheitert ein Vorschauerzeuger an einer Datei, merkt sich Arkha das und versucht es erst wieder, wenn sich die Datei ändert. Löschen Sie nach dem Installieren eines Vorschauerzeugers den Ordner `~/.cache/thumbnails/fail/arkha`, damit Arkha es bei allen Dateien noch einmal versucht. Das Speichern eigener Vorschauerzeuger in den Einstellungen tut das auch.
- **Steht die Sandbox im Weg?** Ist bubblewrap installiert, laufen die Vorschauerzeuger des Systems in einer Sandbox, die ihnen nur das System und die Datei zeigt. Funktioniert ein Vorschauerzeuger ohne sie, aber nicht in ihr, starten Sie Arkha zum Prüfen mit `ARKHA_THUMBNAIL_SANDBOX=0 arkha`, und melden Sie es bitte.

Vorschaubilder werden in `~/.cache/thumbnails` aufbewahrt, geteilt mit Caja und anderen Programmen. Sie wandern mit Dateien, die Arkha kopiert, verschiebt oder umbenennt, sodass sie nicht neu erzeugt werden müssen.

## Etwas fehlt oder funktioniert nicht

| Problem | Warum, und was hilft |
|---|---|
| Kein Papierkorb, keine Laufwerke, keine Netzwerkorte | Sie brauchen GVfs (das Paket `gvfs`; `gvfs-smb`, `gvfs-mtp` und Ähnliche für Windows-Freigaben und Telefone). Die meisten Arbeitsumgebungen haben es schon. |
| Hier entpacken und Komprimieren … fehlen | Sie brauchen eine Archivverwaltung: Engrampa, File Roller oder Xarchiver. |
| „Kein Terminal gefunden“ | Arkha sucht nach mate-terminal, xfce4-terminal, gnome-terminal, konsole, alacritty, kitty, xterm und anderen. Installieren Sie eines oder setzen Sie die Umgebungsvariable `TERMINAL` auf Ihres. |
| Kein Reiter Medieninfo | Er braucht das Programm `mediainfo` und erscheint nur für die Arten von Dateien, die unter **Einstellungen → Vorschau → Medieninfo** angehakt sind. |
| Aus einer Archivverwaltung gezogene Dateien werden nicht entpackt | Das braucht X11: Unter Wayland können Archivverwaltungen keine Dateien herausziehen lassen. File Roller ab Version 43 kann das überhaupt nicht. |
| Laufwerke werden eingehängt, obwohl das Einhängen abgeschaltet ist | Ein anderer Automounter (udiskie, devmon, ldm, udisks-glue) läuft; Arkha sagt Ihnen einmal Bescheid, wenn es einen findet. Beenden Sie ihn oder lassen Sie ihn das Einhängen übernehmen. |
| Programme öffnen Ordner in einer anderen Dateiverwaltung | Machen Sie Arkha zur Standardanwendung für Ordner: siehe [Das Fenster](window.md#arkha-zur-dateiverwaltung-machen). |
| Die Sprache hat sich nicht geändert | Sie ändert sich beim nächsten Start von Arkha. |

## Wo Arkha seine Einstellungen aufbewahrt

| Datei | Enthält |
|---|---|
| `~/.config/arkha/arkha.conf` | Die Einstellungen |
| `~/.config/arkha/folders.conf` | Die Ansicht und Sortierung jedes Ordners |
| `~/.config/arkha/actions.conf` | Ihre eigenen Befehle im Kontextmenü |
| `~/.config/arkha/thumbnailers.conf` | Ihre eigenen Vorschauerzeuger |

Manches teilt Arkha mit Caja und anderen Programmen und bewahrt es dort auf, wo diese es tun:

- Lesezeichen: `~/.config/gtk-3.0/bookmarks`
- Vorschaubilder: `~/.cache/thumbnails`
- Notizen, Embleme, eigene Symbole und Ordnerhintergründe: in den Metadaten von GVfs, wie Caja sie aufbewahrt
- Ihre eigenen Embleme: dort, wo Caja sie aufbewahrt

Die Einstellungsdateien sind reiner Text und lassen sich bearbeiten, während Arkha nicht läuft. Manche Einstellungen gibt es nur dort, nicht in den Einstellungen, etwa die Zurück- und Vor-Tasten der Maus (`mouse_back_button`, `mouse_forward_button` und `mouse_extra_buttons`, unter `[General]`).

Lässt sich eine Zeile einer Einstellungsdatei nicht lesen, lässt Arkha sie weg und liest den Rest, statt mit verlorenen Einstellungen neu anzufangen. Die Datei, wie sie war, wird daneben aufbewahrt, mit der Endung `.broken` (`arkha.conf.broken`), sodass Sie sehen können, was nicht stimmte.

Um mit den üblichen Einstellungen neu anzufangen, beenden Sie Arkha und löschen Sie `~/.config/arkha/arkha.conf`.

## Ein Problem melden

Bitte melden Sie Fehler und Ideen im [Issue-Tracker von Arkha auf GitHub](https://github.com/dekart811/arkha/issues). Es hilft, wenn Sie angeben:

- Arkhas Version und Build, aus **Hilfe → Info**,
- Ihre Distribution und Arbeitsumgebung (oder Ihren Fenstermanager) und ob es X11 oder Wayland ist,
- was Sie getan haben, was passiert ist und was Sie erwartet hatten,
- was Arkha in einem Terminal ausgibt: Starten Sie es dort mit `arkha` und tun Sie es noch einmal.
