# Finding your way

## The path bar

The path bar above the files has a button for each folder on the way to the one you're in: `Home › Pictures › 2026`. Click a button to go to that folder, or middle-click it to open the folder in a new tab. Files dragged onto a button go into that folder.

## Typing a location

Press <kbd>Ctrl</kbd>+<kbd>L</kbd> (**Go → Location…**) and the path bar turns into a text field. Type or paste a path and press <kbd>Enter</kbd>. As you type, Arkha fills in the rest of the folder name you've started, selected, so you can keep typing over it. <kbd>Escape</kbd> brings the buttons back.

The field also takes addresses of servers, such as `sftp://server/folder` or `smb://nas/share`: see [Drives and network places](drives-and-network.md#network-places).

To always see the location as text, turn on **Preferences → Display → Always show the location as text**.

## Back, forward and up

| To | Use |
|---|---|
| Go back to the folder you were in before | The Back button, <kbd>Alt</kbd>+<kbd>←</kbd>, or the mouse's back button |
| Go forward again | The Forward button, <kbd>Alt</kbd>+<kbd>→</kbd>, or the mouse's forward button |
| Go to the folder that holds this one | The Up button, <kbd>Alt</kbd>+<kbd>↑</kbd> or <kbd>Backspace</kbd> |
| Go home | The Home button or <kbd>Alt</kbd>+<kbd>Home</kbd> |

A right-click on the Back button lists the folders you can go back to. Each tab has its own history; **Go → Clear History** forgets it.

The mouse's back and forward buttons are buttons 8 and 9 on most mice. If yours are others, set them in Arkha's settings file (see [Questions and problems](troubleshooting.md#where-arkha-keeps-its-settings)): `mouse_back_button` and `mouse_forward_button`, as Caja keeps them.

## The Go menu

Besides Back, Forward and Open Parent, the Go menu goes straight to your **Home Folder**, the **Desktop**, the **File System** (the top folder, `/`) and the **Trash**. You can give any of them a keyboard shortcut (see [Making Arkha yours](customizing.md#keyboard-shortcuts)).

## Bookmarks

Bookmarks are folders you go to often, listed in the side pane and the Bookmarks menu.

- **Bookmarks → Add Bookmark** (<kbd>Ctrl</kbd>+<kbd>D</kbd>) bookmarks the folder you're in. Folders dragged onto the side pane's Bookmarks heading, or onto a bookmark, become bookmarks too (with <kbd>Shift</kbd>, dropped on a bookmark, they go into its folder instead).
- **Bookmarks → Edit Bookmarks…** (<kbd>Ctrl</kbd>+<kbd>B</kbd>) lists them: drag them into another order, double-click a name to rename it, add or remove them.

Arkha's bookmarks are the ones Caja, other GTK file managers and the GTK file chooser (the Open and Save dialogs of most programs) use, so a bookmark added in one shows in all of them.

## The side pane

The side pane (<kbd>F9</kbd> shows or hides it) has three sections:

- **Computer:** your home folder, the desktop, the file system, the trash, and the like.
- **Devices:** drives, memory cards, discs, phones and servers: see [Drives and network places](drives-and-network.md).
- **Bookmarks:** your bookmarks.

Drag a section's heading up or down to put the sections in another order. Which entries the Computer section shows, and how tall the rows are, is set in **Preferences → Side Pane**. Files dropped on an entry go there.
