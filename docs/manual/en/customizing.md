# Making Arkha yours

## The context menu

**Preferences → Context Menu** lists the entries of the two context menus: **Files and Folders** (a right-click on items) and **A Folder's Empty Space**. Untick an entry to hide it, and drag it by its handle (or select it and use the arrows) to move it. **Add Separator** puts a line between entries; **Reset Order** puts every entry back in its usual place. Entries that don't apply are left out by themselves, such as Restore outside the Trash.

## Your own commands

**Preferences → Custom Actions** adds entries to the context menus that run a command of your own on the selected files: open them in an image editor, convert them, send them somewhere.

**Add…** asks for:

- **Name:** what the menu shows.
- **Command:** what runs, in the folder you're in. In it, these stand for the selection:

  | | Stands for |
  |---|---|
  | `%f` | The first selected file |
  | `%F` | All selected files |
  | `%n` | The first selected file's name, without its folder |
  | `%N` | All selected files' names |
  | `%d` | The folder you're in |
  | `%%` | A `%` sign |

  Paths are quoted for you, so names with spaces work: write `gimp %F`, not `gimp "%F"`.

- **Show for:** files, folders, and/or a folder's empty space (where `%d` is the folder and nothing is selected).
- **File types** and **Name patterns:** offer it only for some files: videos, audio, images, text, documents or archives, and/or names like `*.srt; *.nfo`. A file needs one of the ticked types or a name matching a pattern; with neither set, the action is offered for every file. Folders only go by the patterns.

Some examples:

| Name | Command | Show for |
|---|---|---|
| Edit in GIMP | `gimp %F` | Files, images |
| Make a PDF | `libreoffice --headless --convert-to pdf %F` | Files, documents |
| Pack as tar.xz | `tar -cJf archive.tar.xz %N` | Files and folders |
| Open in VS Code | `code %d` | Empty space |

Commands run with `sh` unless you set another shell, one that understands `-c` (bash, zsh, fish), under **Run commands with**. Your own commands are only offered for folders on this computer, not on network places or in the Trash, as they get paths.

Your entries show up in the context menus, and can be moved and hidden there like the others, under **Preferences → Context Menu**.

## Your own thumbnailers

Arkha uses the thumbnailers installed on your system (for PDFs, videos, office documents, comic books, and whatever else they handle). In **Preferences → Previews** you can add your own, which go before them for the types they cover:

- **Name:** for the list.
- **File types:** MIME types, separated by `;`, where `*` matches anything: `video/*`, `image/x-krita`.
- **Command:** makes the thumbnail. `%i` is the file, `%o` the picture to write (PNG or JPEG), and `%s` the size in pixels. For example: `ffmpegthumbnailer -i %i -o %o -s %s -f`.

The command runs with `sh` and may take up to 30 seconds. It runs as you wrote it, with access to your files, so it may use a script in your home folder. (The system's thumbnailers and Arkha's built-in ones, by contrast, run in a sandbox when [bubblewrap](https://github.com/containers/bubblewrap) is installed: they only see the system and the file being thumbnailed, so a file made to attack a thumbnailer can't reach your other files.)

## Keyboard shortcuts

**Preferences → Keyboard** lists every action that can have a shortcut. Double-click an action (or select it and press **Change…**), then press the new keys; <kbd>Escape</kbd> cancels. **Clear** removes a shortcut, **Reset** brings back the usual one, and **Reset All** all of them. Changed shortcuts are shown in bold.

A letter or number key needs <kbd>Ctrl</kbd>, <kbd>Alt</kbd> or <kbd>Super</kbd> with it, so typing a name to find an item keeps working. If the keys already belong to another action, Arkha asks whether to move them.

**Help → Keyboard Shortcuts** (<kbd>Ctrl</kbd>+<kbd>F1</kbd>) shows the shortcuts as they are now; [Keyboard shortcuts](shortcuts.md) lists the usual ones.

## The toolbar's buttons

The buttons beside the path bar can be put in another order: turn off **View → Lock Toolbar** (it's on to begin with, so they don't move by accident) and drag them. Hide the ones you don't use in **Preferences → Display → Window**.
