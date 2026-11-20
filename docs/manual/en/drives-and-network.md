# Drives and network places

## Drives, cards, discs and phones

Flash drives, memory cards, discs, phones (MTP) and mounted servers show in the side pane under **Devices**.

- **Click** one to open it; if it isn't mounted yet, Arkha mounts it first.
- **Take it out safely** with the eject button next to it, or **Unmount**, **Eject** or **Safely Remove** in its context menu. A notice says when the drive can be unplugged, and, while data is still being written to it, that it shouldn't be yet.

Arkha mounts drives and media as you insert them, even without a desktop environment. Turn this off in **Preferences → Media → Mount drives and media when they're inserted**; there, **The eject button only unmounts** makes the button leave the drive powered. On GNOME, Cinnamon, MATE, Xfce, KDE, LXDE and LXQt this setting is the desktop's own: changing it in Arkha changes it there too, and the other way round. Separate automounters (udiskie, devmon and the like) can't be switched off from Arkha; it tells you once if it finds one running.

### When something is inserted

For what an inserted medium holds (an audio CD, a video DVD, a camera's photos, a music player, software), Arkha can start a program, open the folder, do nothing, or ask you, as Caja does. When it asks, you can tell it to always do the same for that kind of medium. Change your choices in **Preferences → Media**; there are also **Never prompt or start programs** and **Browse media when inserted**. On GNOME, Cinnamon and MATE these are the desktop's own settings, shared with Caja.

### Drives formatted for Windows

Drives formatted with FAT, exFAT or NTFS (most flash drives and memory cards) don't allow some characters in names: `" * : < > ? \ |`, and no space or dot at the end. Arkha copies such names with `_` in their place, as Nautilus does, instead of failing on them one by one. FAT32 drives also can't hold files of 4 GB or more: Arkha says so for such a file at once, not after writing 4 GB of it. (Formatted as exFAT, the drive could hold it.)

## Network places

Arkha opens folders on other computers and servers, with GVfs:

| Address | For |
|---|---|
| `sftp://server/folder` | A computer you can reach with SSH |
| `smb://nas/share` | A Windows share, or a NAS |
| `ftp://server/folder` | An FTP server |
| `davs://server/folder` | A WebDAV server (`dav://` without encryption) |

Type or paste the address into the location bar (<kbd>Ctrl</kbd>+<kbd>L</kbd>), or open a bookmark or a link to one. If the server asks for a password, Arkha asks you, and can remember it.

**File → Connect to Server…** builds the address for you from the server's name, its type (SSH, FTP, Windows share, WebDAV), the port, the folder, your user name and domain, and can bookmark it under a name of your choice.

A server you're connected to shows in the side pane under Devices, and as the first button of the path bar. Unmount it there when you're done.

> On network places, thumbnails and folder item counts are left out by default, and the info panel leaves out folder sizes: they would ask the server about every file. Turn thumbnails and counts on for them in **Preferences → Previews**.
