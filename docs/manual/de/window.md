# Das Fenster

<picture>
<source media="(prefers-color-scheme: dark)" srcset="../../screenshots/icons-dark.png">
<img src="../../screenshots/icons-light.png" alt="Arkhas Fenster: oben die Menüs und die Werkzeugleiste, links die Seitenleiste, in der Mitte Bilder in der Symbolansicht, rechts der Infobereich">
</picture>

Von oben nach unten hat Arkhas Fenster:

- **Die Menüleiste:** Datei, Bearbeiten, Ansicht, Gehe zu, Lesezeichen und Hilfe. Dort finden Sie alles, was Arkha kann.
- **Die Werkzeugleiste:** Zurück, Vor, Nach oben (der übergeordnete Ordner), Neu laden, Persönlicher Ordner, Dateisystem und Suchen, und neben dem Pfad eine Leiste mit Knöpfen für Dateivorgänge, die geteilte Ansicht, die Ansicht, das Zoomen und den Infobereich.
- **Die Pfadleiste:** ein Knopf für jeden Ordner auf dem Weg zu dem, in dem Sie sind. Klicken Sie auf einen, um dorthin zu gehen, oder mit der mittleren Maustaste, um ihn in einem neuen Reiter zu öffnen. Mit <kbd>Strg</kbd>+<kbd>L</kbd> geben Sie stattdessen einen Ort ein (siehe [Sich zurechtfinden](navigation.md)).
- **Die Seitenleiste** links: Rechner (Ihr persönlicher Ordner, der Schreibtisch, das Dateisystem, der Papierkorb …), Geräte (Laufwerke, Discs, Telefone und Server) und Lesezeichen. Ziehen Sie die Überschrift eines Abschnitts, um die Abschnitte anders anzuordnen.
- **Die Dateien** des Ordners, in dem Sie sind, in einer von vier [Ansichten](views.md).
- **Die Statusleiste** unten: wie viele Objekte es gibt oder ausgewählt sind, ihre Größe und der freie Platz auf dem Laufwerk.

Ordner öffnen sich in **Reitern**, und das Fenster kann zwei Ordner nebeneinander zeigen (**geteilte Ansicht**): siehe [Reiter und geteilte Ansicht](tabs-and-panes.md). Die Werkzeugleiste, die Ortsleiste, die Seitenleiste und die Statusleiste lassen sich jeweils im Menü Ansicht ausblenden.

## Dateien und Ordner öffnen

- **Doppelklicken** Sie auf eine Datei oder einen Ordner, um sie zu öffnen, oder wählen Sie sie aus und drücken Sie <kbd>Eingabe</kbd>. Im Modus **einfacher Klick** (Einstellungen → Verhalten) öffnet ein Klick.
- Eine Datei öffnet sich in der Anwendung, die für ihren Typ eingestellt ist. Für eine andere klicken Sie mit der rechten Maustaste darauf und wählen **Öffnen mit**: Es listet die Anwendungen für ihren Typ auf und bietet jede andere an.
- Ein Ordner öffnet sich an Ort und Stelle. **In neuem Reiter öffnen** (<kbd>Strg</kbd>+<kbd>Eingabe</kbd> oder ein Mittelklick) und **In neuem Fenster öffnen** stehen in seinem Kontextmenü.
- **Im Terminal öffnen** (<kbd>F4</kbd>) öffnet ein Terminal in dem Ordner, in dem Sie sind, oder im ausgewählten Ordner.
- Ein Programm oder Skript, das Sie öffnen, fragt, was geschehen soll: es **Ausführen**, **Im Terminal ausführen** oder **Anzeigen** (den Text eines Skripts, in seinem Editor).

## Auswählen

- Klicken Sie auf ein Objekt, um es auszuwählen; <kbd>Strg</kbd>+Klick nimmt Objekte zur Auswahl hinzu oder aus ihr heraus, <kbd>Umschalt</kbd>+Klick wählt alles bis zum angeklickten Objekt aus.
- Ziehen Sie über eine freie Fläche, um ein Rechteck um Objekte zu ziehen.
- **Tippen Sie den Anfang eines Namens**, um zum ersten Objekt zu springen, das so beginnt.
- Weitere Arten auszuwählen (alle Objekte, nach einem Muster wie `*.jpg`, die Auswahl umkehren) stehen unter [Suchen und Auswählen](search.md).

## Rechtsklick

Ein Rechtsklick auf ein Objekt öffnet sein **Kontextmenü** mit dem, was sich damit tun lässt. Ein Rechtsklick auf eine freie Fläche eines Ordners öffnet das Menü des Ordners: einen Ordner oder ein Dokument anlegen, einfügen, die Ansicht ändern, ein Lesezeichen hinzufügen. Beide Menüs lassen sich ändern: siehe [Arkha anpassen](customizing.md#das-kontextmenü).

## Verborgene Dateien

Dateien und Ordner, deren Namen mit einem Punkt beginnen (`.config`), und Sicherungsdateien, die auf `~` enden, sind verborgen. **Ansicht → Verborgene Dateien anzeigen** (<kbd>Strg</kbd>+<kbd>H</kbd>) zeigt sie. Die Statusleiste kann nennen, wie viele verborgen sind (Einstellungen → Anzeige).

## Arkha zur Dateiverwaltung machen

Damit andere Programme und der Schreibtisch Ordner mit Arkha öffnen, machen Sie es zur Standardanwendung für Ordner, in den Einstellungen Ihrer Arbeitsumgebung für Standardanwendungen oder mit:

```
xdg-mime default arkha.desktop inode/directory
```

Programme, die „eine Datei in ihrem Ordner zeigen“ (zum Beispiel die Download-Listen von Webbrowsern), fragen die Dateiverwaltung, die gerade läuft. Arkha antwortet ihnen, solange es läuft; mit **Einstellungen → Verhalten → Ordner öffnen, wenn andere Programme danach fragen, auch wenn Arkha nicht läuft** starten sie es, wenn es nicht läuft.
