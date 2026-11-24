# Views

Arkha can show a folder in four ways. Switch between them in the View menu, with the view buttons beside the path bar, or with <kbd>Ctrl</kbd>+<kbd>1</kbd> to <kbd>Ctrl</kbd>+<kbd>4</kbd>.

| View | Shows | Good for |
|---|---|---|
| **Icons** (<kbd>Ctrl</kbd>+<kbd>1</kbd>) | Big icons or thumbnails in rows, names below | Pictures, videos, documents you know by sight |
| **List** (<kbd>Ctrl</kbd>+<kbd>2</kbd>) | One row per item, with columns for size, type, date and more | Comparing and sorting, big folders |
| **Compact** (<kbd>Ctrl</kbd>+<kbd>3</kbd>) | Small icons in columns, names beside them | Many items at a glance |
| **Columns** (<kbd>Ctrl</kbd>+<kbd>4</kbd>) | The folders on the way side by side, the selection's contents or preview on the right | Going down deep folder trees |

In the **columns view**, <kbd>←</kbd> and <kbd>→</kbd> move between the columns. Every column has its own context menu, takes drops, and follows changes on the disk like the other views.

Which view a new folder opens in is set in **Preferences → Views → View new folders using**.

A folder or search with more than 20,000 items in the icon or compact view is shown as a list instead, as icons would take seconds to arrange each time something changes; the status bar says so. With fewer than 10,000 it goes back to icons. Arkha doesn't remember the list for the folder, and if you switch back to icons yourself, they stay until you open another folder or search.

## Each folder as you left it

Arkha remembers the view and sorting of each folder you change, so a folder opens the way you left it. **View → Reset View to Defaults** makes a folder forget it. Turn this off, or forget all folders at once, in **Preferences → Views → Individual Folders**.

## Sorting

- **View → Arrange Items** (also in the context menu of a folder's empty space) sorts by the name or one of the details the view shows; **Reversed Order** turns it around, and **Folders Before Files** keeps folders at the top.
- In the list view, click a column's heading to sort by it, and click it again to reverse the order.

## The list's columns

The list view always shows the **Name**, and any of: Size, Type, Modified, Accessed, Created, Owner, Group, Permissions, Octal Permissions, MIME Type, Extension, Location and Size on Disk. In the Trash, **Trashed On** and **Original Location** show where items came from; in search results, **Location** shows where they were found.

Choose them with **View → Visible Columns…**, in **Preferences → List Columns**, or with a right-click on the column headings. Drag a heading sideways to put its column somewhere else.

**Preferences → Display** can give the list alternating row colors, and **Preferences → Views → List View** can hide the icons beside the names.

## Captions in the icon view

Under the names in the icon view, up to three details can show, such as the size or the date: choose them in **Preferences → Display → Icon Captions**. More of them show as you zoom in.

## Zoom

Make icons and text bigger or smaller with **View → Zoom In** (<kbd>Ctrl</kbd>+<kbd>+</kbd>) and **Zoom Out** (<kbd>Ctrl</kbd>+<kbd>-</kbd>), with <kbd>Ctrl</kbd> and the mouse wheel, or with the zoom buttons. **Normal Size** (<kbd>Ctrl</kbd>+<kbd>0</kbd>) goes back. Each view has its own zoom level, and the sizes Zoom steps through are set in **Preferences → Views → Zoom**.

## Thumbnails and previews

Pictures, videos, PDFs and many other files show a small picture of what's in them (a **thumbnail**) instead of an icon, and text files can show their first lines in their icon. Folders can show how many items they hold. What is shown, and whether also for files on network shares, is set in **Preferences → Previews**. Why a thumbnail may be missing is explained under [Questions and problems](troubleshooting.md#thumbnails-dont-show).

## The info panel

The info panel shows the selected item beside the files, larger, with a few details: a picture itself, a video's or document's thumbnail, or a folder's cover picture (a `folder.jpg` or `cover.jpg` in it). With several items selected, it shows their number and total size.

Show it with **View → Info Panel** (<kbd>F11</kbd>) or its button beside the path bar. It's turned on per folder: a music or photo folder can have it, your home folder not.

**Preferences → Previews → Info Panel** sets which file names count as covers (matched ignoring case), what it shows for several items and for folders without a cover, and whether it shows the details.

## Scrolling

Where a view only has a horizontal scrollbar (the compact view), the mouse wheel scrolls sideways. A middle-click on empty space starts **autoscrolling**, as in web browsers: a marker shows where you clicked, and the view scrolls towards the mouse pointer, faster the further away it is. A click, a key or the wheel stops it. In the list view it also starts on files and folders when the list can scroll; otherwise, or with <kbd>Shift</kbd>, a middle-click opens them. It can be turned off in **Preferences → Behavior → Scrolling**.
