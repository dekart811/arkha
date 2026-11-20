# Laufwerke und Netzwerkorte

## Laufwerke, Karten, Discs und Telefone

USB-Sticks, Speicherkarten, Discs, Telefone (MTP) und eingehängte Server erscheinen in der Seitenleiste unter **Geräte**.

- **Klicken** Sie auf eines, um es zu öffnen; ist es noch nicht eingehängt, hängt Arkha es vorher ein.
- **Entfernen Sie es sicher** mit dem Auswurfknopf daneben oder mit **Aushängen**, **Auswerfen** oder **Sicher entfernen** in seinem Kontextmenü. Eine Meldung sagt, wann das Laufwerk abgezogen werden kann, und, solange noch Daten darauf geschrieben werden, dass es noch nicht so weit ist.

Arkha hängt Laufwerke und Datenträger ein, sobald Sie sie einlegen, auch ohne Arbeitsumgebung. Abschalten können Sie das unter **Einstellungen → Datenträger → Laufwerke und Datenträger beim Einlegen einhängen**; dort sorgt **Der Auswurfknopf hängt nur aus** dafür, dass der Knopf das Laufwerk eingeschaltet lässt. Unter GNOME, Cinnamon, MATE, Xfce, KDE, LXDE und LXQt ist diese Einstellung die der Arbeitsumgebung selbst: Ändern Sie sie in Arkha, ändert sie sich dort auch, und umgekehrt. Eigene Automounter (udiskie, devmon und ähnliche) lassen sich von Arkha aus nicht abschalten; es sagt Ihnen einmal Bescheid, wenn es einen laufen sieht.

### Wenn etwas eingelegt wird

Je nachdem, was ein eingelegter Datenträger enthält (eine Audio-CD, eine Video-DVD, die Fotos einer Kamera, einen Musikspieler, Software), kann Arkha ein Programm starten, den Ordner öffnen, nichts tun oder Sie fragen, wie Caja. Wenn es fragt, können Sie ihm sagen, bei dieser Art von Datenträger immer dasselbe zu tun. Ihre Wahl ändern Sie unter **Einstellungen → Datenträger**; dort gibt es auch **Beim Einlegen von Datenträgern niemals nachfragen oder Programme starten** und **Datenträger beim Einlegen durchsuchen**. Unter GNOME, Cinnamon und MATE sind das die Einstellungen der Arbeitsumgebung selbst, geteilt mit Caja.

### Für Windows formatierte Laufwerke

Laufwerke mit FAT, exFAT oder NTFS (die meisten USB-Sticks und Speicherkarten) erlauben manche Zeichen in Namen nicht: `" * : < > ? \ |`, und kein Leerzeichen oder Punkt am Ende. Arkha kopiert solche Namen mit `_` an deren Stelle, wie Nautilus, statt an jedem einzeln zu scheitern. FAT32-Laufwerke können außerdem keine Dateien von 4 GB oder mehr aufnehmen: Arkha sagt das bei einer solchen Datei sofort, nicht erst, nachdem es 4 GB davon geschrieben hat. (Mit exFAT formatiert könnte das Laufwerk sie aufnehmen.)

## Netzwerkorte

Arkha öffnet Ordner auf anderen Rechnern und Servern, mit GVfs:

| Adresse | Für |
|---|---|
| `sftp://server/ordner` | Einen Rechner, den Sie per SSH erreichen |
| `smb://nas/freigabe` | Eine Windows-Freigabe oder ein NAS |
| `ftp://server/ordner` | Einen FTP-Server |
| `davs://server/ordner` | Einen WebDAV-Server (`dav://` ohne Verschlüsselung) |

Tippen oder fügen Sie die Adresse in die Ortsleiste ein (<kbd>Strg</kbd>+<kbd>L</kbd>), oder öffnen Sie ein Lesezeichen oder einen Link darauf. Fragt der Server nach einem Passwort, fragt Arkha Sie danach und kann es sich merken.

**Datei → Mit Server verbinden …** baut die Adresse für Sie aus dem Namen des Servers, seinem Typ (SSH, FTP, Windows-Freigabe, WebDAV), dem Port, dem Ordner, Ihrem Benutzernamen und der Domäne zusammen und kann ein Lesezeichen dafür unter einem Namen Ihrer Wahl anlegen.

Ein Server, mit dem Sie verbunden sind, erscheint in der Seitenleiste unter Geräte und als erster Knopf der Pfadleiste. Hängen Sie ihn dort aus, wenn Sie fertig sind.

> Auf Netzwerkorten werden Vorschaubilder und die Anzahl der Objekte in Ordnern zunächst weggelassen, und der Infobereich lässt die Größe von Ordnern weg: Sie würden den Server nach jeder Datei fragen. Vorschaubilder und Anzahlen schalten Sie dafür unter **Einstellungen → Vorschau** ein.
