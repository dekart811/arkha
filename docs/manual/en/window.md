# The window

<picture>
<source media="(prefers-color-scheme: dark)" srcset="../../screenshots/icons-dark.png">
<img src="../../screenshots/icons-light.png" alt="Arkha's window: the menus and toolbar at the top, the side pane on the left, pictures in the icon view in the middle, the info panel on the right">
</picture>

From top to bottom, Arkha's window has:

- **The menu bar:** File, Edit, View, Go, Bookmarks and Help. Everything Arkha does can be found there.
- **The toolbar:** Back, Forward, Up (the parent folder), Reload, Home, File System and Search, with a bar of buttons for file operations, split view, the view, zoom and the info panel beside the path.
- **The path bar:** a button for each folder on the way to the one you're in. Click one to go there, or middle-click it to open it in a new tab. Press <kbd>Ctrl</kbd>+<kbd>L</kbd> to type a location instead (see [Finding your way](navigation.md)).
- **The side pane** on the left: Computer (your home folder, the desktop, the file system, the trash…), Devices (drives, discs, phones and servers) and Bookmarks. Drag a section's heading to put the sections in another order.
- **The files** of the folder you're in, in one of four [views](views.md).
- **The status bar** at the bottom: how many items there are or are selected, their size, and the free space on the drive.

Folders open in **tabs**, and the window can show two folders side by side (**split view**): see [Tabs and split view](tabs-and-panes.md). The toolbar, location bar, side pane and status bar can each be hidden in the View menu.

## Opening files and folders

- **Double-click** a file or folder to open it, or select it and press <kbd>Enter</kbd>. With **single-click** mode (Preferences → Behavior), one click opens it.
- A file opens in the application set for its type. To use another one, right-click it and choose **Open With**, which lists the applications for its type and offers any other.
- A folder opens in place. **Open in New Tab** (<kbd>Ctrl</kbd>+<kbd>Enter</kbd>, or a middle-click) and **Open in New Window** are in its context menu.
- **Open in Terminal** (<kbd>F4</kbd>) opens a terminal in the folder you're in, or in the selected folder.
- A program or script you open asks what to do: **Run** it, **Run in Terminal**, or **Display** it (a script's text, in its editor).

## Selecting

- Click an item to select it; <kbd>Ctrl</kbd>+click adds items to the selection or takes them out, <kbd>Shift</kbd>+click selects everything up to the clicked item.
- Drag across empty space to draw a rectangle around items.
- **Type the start of a name** to jump to the first item that starts with it.
- More ways to select (all items, by a pattern like `*.jpg`, inverting the selection) are under [Searching and selecting](search.md).

## Right-clicking

A right-click on an item opens its **context menu**, with what can be done with it. A right-click on a folder's empty space opens the folder's menu: create a folder or document, paste, change the view, add a bookmark. Both menus can be changed: see [Making Arkha yours](customizing.md#the-context-menu).

## Hidden files

Files and folders whose names start with a dot (`.config`) and backup files ending in `~` are hidden. **View → Show Hidden Files** (<kbd>Ctrl</kbd>+<kbd>H</kbd>) shows them. The status bar can say how many are hidden (Preferences → Display).

## Making Arkha your file manager

To open folders with Arkha from other programs and the desktop, make it the default application for folders, in your desktop's settings for default applications, or with:

```
xdg-mime default arkha.desktop inode/directory
```

Programs that "show a file in its folder" (web browsers' download lists, for example) ask the file manager that's running. Arkha answers them while it runs; with **Preferences → Behavior → Open folders when other programs ask, also when Arkha isn't running**, they start it when it isn't.
