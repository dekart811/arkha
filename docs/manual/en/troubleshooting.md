# Questions and problems

## Thumbnails don't show

- **Is it turned on for this place?** By default, thumbnails are made for files on this computer only, not on network places or phones, and not for pictures over 100 MB. Both are set in **Preferences → Previews**.
- **Is there a thumbnailer for the type?** Arkha makes thumbnails of pictures itself. For other files it uses the thumbnailers installed on the system, as Caja does: Evince or Atril for PDF, PostScript, DjVu and comic books, ffmpegthumbnailer (or ffmpeg) for videos, libgsf for office documents, and so on. Install the one for your files, or add your own (see [Making Arkha yours](customizing.md#your-own-thumbnailers)).
- **Did it fail before?** When a thumbnailer fails on a file, Arkha notes it and doesn't try again until the file changes. After installing a thumbnailer, delete the folder `~/.cache/thumbnails/fail/arkha` to let Arkha try all files again. Saving your own thumbnailers in Preferences does this too.
- **Does the sandbox get in the way?** With bubblewrap installed, the system's thumbnailers run in a sandbox that only shows them the system and the file. If a thumbnailer works without it but not in it, start Arkha with `ARKHA_THUMBNAIL_SANDBOX=0 arkha` to check, and please report it.

Thumbnails are kept in `~/.cache/thumbnails`, shared with Caja and other programs. They move with files that Arkha copies, moves or renames, so they needn't be made again.

## Something is missing or doesn't work

| Problem | Why, and what helps |
|---|---|
| No Trash, no drives, no network places | They need GVfs (the `gvfs` package; `gvfs-smb`, `gvfs-mtp` and the like for Windows shares and phones). Most desktops have it already. |
| Extract Here and Compress… are missing | They need an archive manager: Engrampa, File Roller or Xarchiver. |
| "No terminal found" | Arkha looks for mate-terminal, xfce4-terminal, gnome-terminal, konsole, alacritty, kitty, xterm and others. Install one, or set the `TERMINAL` environment variable to yours. |
| No Media Info tab | It needs the `mediainfo` program, and is only shown for the kinds of files ticked in **Preferences → Previews → Media Info**. |
| Files dragged out of an archive manager aren't extracted | This needs X11: on Wayland, archive managers can't drag files out. File Roller 43 and later can't drag files out at all. |
| Drives are mounted although automounting is off | Another automounter (udiskie, devmon, ldm, udisks-glue) is running; Arkha tells you once when it finds one. Stop it, or let it do the mounting. |
| Programs open folders in another file manager | Make Arkha the default application for folders: see [The window](window.md#making-arkha-your-file-manager). |
| The language didn't change | It changes the next time Arkha starts. |

## Where Arkha keeps its settings

| File | Holds |
|---|---|
| `~/.config/arkha/arkha.conf` | The preferences |
| `~/.config/arkha/folders.conf` | Each folder's view and sorting |
| `~/.config/arkha/actions.conf` | Your own context menu commands |
| `~/.config/arkha/thumbnailers.conf` | Your own thumbnailers |

Some things Arkha shares with Caja and other programs, and keeps where they do:

- Bookmarks: `~/.config/gtk-3.0/bookmarks`
- Thumbnails: `~/.cache/thumbnails`
- Notes, emblems, custom icons and folder backgrounds: GVfs's metadata, as Caja keeps them
- Your own emblems: where Caja keeps them

The settings files are plain text and can be edited while Arkha isn't running. Some settings are only there, not in Preferences, such as the mouse's back and forward buttons (`mouse_back_button`, `mouse_forward_button` and `mouse_extra_buttons`, under `[General]`).

If a line in a settings file can't be read, Arkha leaves it out and reads the rest, instead of starting over with all settings lost. The file as it was is kept beside it, ending in `.broken` (`arkha.conf.broken`), so you can see what was wrong.

To start over with the usual settings, quit Arkha and delete `~/.config/arkha/arkha.conf`.

## Reporting a problem

Please report bugs and ideas on [Arkha's issue tracker on GitHub](https://github.com/dekart811/arkha/issues). It helps to say:

- Arkha's version and build, from **Help → About**,
- your distribution and desktop (or window manager), and whether it's X11 or Wayland,
- what you did, what happened, and what you expected,
- what Arkha writes in a terminal: start it from one with `arkha` and do it again.
