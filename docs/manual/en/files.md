# Working with files

## Copying and moving

- **Copy** (<kbd>Ctrl</kbd>+<kbd>C</kbd>) or **Cut** (<kbd>Ctrl</kbd>+<kbd>X</kbd>) the selected items, go to another folder and **Paste** (<kbd>Ctrl</kbd>+<kbd>V</kbd>). Cut and paste moves them. **Paste Into Folder** pastes into the selected folder instead of the one you're in.
- **Copy To…** and **Move To…** ask for the folder to copy or move to.
- **Duplicate** makes a copy next to the original; **Make Link** (<kbd>Ctrl</kbd>+<kbd>M</kbd>) makes a link to it.

Arkha's clipboard is the one Caja and Nautilus use: copy in one and paste in the other.

### Dragging

Drag items to another folder, onto a folder, a tab, a button of the path bar or an entry of the side pane.

| While dragging, hold | To |
|---|---|
| nothing | Move, if it stays on the same drive; copy, if it goes to another drive |
| <kbd>Shift</kbd> | Move |
| <kbd>Ctrl</kbd> | Copy |
| <kbd>Alt</kbd> | Choose from a menu when you drop: Move Here, Copy Here, Link Here, Move and Check Here, Copy and Check Here |

Files dragged out of an archive manager are extracted where you drop them: out of KDE's Ark on X11 and Wayland alike, out of Engrampa, Xarchiver and older File Roller versions on X11. For those on Wayland, see [Questions and problems](troubleshooting.md#something-is-missing-or-doesnt-work).

### Progress

Copies and moves run in the background, so you can go on working. A button in the location bar shows how far along they are; click it to see each operation and to cancel one. If you prefer a window of its own, turn on **Preferences → Behavior → Show file operations in their own window**.

### When a name is already taken

If an item with the same name is already where you copy or move to, Arkha asks:

- **Skip:** leave this item out.
- **Keep Both:** copy it under a free name, such as `photo (2).jpg`. Under **Select a new name for the destination** you can type a name of your own.
- **Replace:** put it in place of the other one. For two folders this is **Merge**: the contents come together, and Arkha asks again for each file that's in both.

**Apply this action to all conflicts** answers the same for the rest of the operation.

## Checked copies

When a copy has to be right (photos you will delete from the camera's card, a backup), let Arkha check it: each copy is read back from the drive or share, not from memory, and compared with its original. A move deletes an original only once its copy matched.

- **Edit → Paste and Check** (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd>, also in a folder's context menu)
- **Check the copies** in Copy To and Move To
- **Copy and Check Here** or **Move and Check Here** in the menu of a drop made with <kbd>Alt</kbd>

A copy that doesn't match asks whether to copy it again or skip it. At the end, Arkha says that all copies matched, or lists the ones that didn't.

## The trash

- **Move to Trash** (<kbd>Delete</kbd>) moves items into the trash, from where you can get them back. Arkha can ask first: **Preferences → Behavior → Ask before moving files to the Trash**.
- **Go → Trash** shows what's in it. **Restore** puts selected items back where they came from; the Trash's list view shows where that was.
- **Empty Trash** (in the File menu, the bar above the trash folder, or its context menu) deletes everything in it for good.
- **Delete** (<kbd>Shift</kbd>+<kbd>Delete</kbd>) deletes items at once, without the trash. Arkha asks first; this can't be undone. Where there's no trash (some drives and network shares), Arkha offers to delete instead, after asking.

## Undo and redo

**Edit → Undo** (<kbd>Ctrl</kbd>+<kbd>Z</kbd>) takes back the last file operation, and **Redo** (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd> or <kbd>Ctrl</kbd>+<kbd>Y</kbd>) does it again. Arkha remembers the last 20. They can be:

- copies and links (the copies and links go to the trash),
- moves (the items go back where they were; after a merge only what was added is taken back, and a copy that replaced a file is left alone),
- moving to the trash and restoring from it,
- renames (all of a Rename All as one step),
- new folders and documents,
- permission, owner and group changes made in Properties.

Deleting for good, emptying the trash and Properties' **Apply to Enclosed Items** can't be undone. Anything new you do after an undo clears Redo.

## New folders and documents

In the context menu of a folder's empty space:

- **Create Folder** (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>N</kbd>) makes a new folder and lets you name it.
- **Create Document** makes an **Empty Document**, or a copy of one of the files in your `~/Templates` folder: put a letterhead or a spreadsheet you often start from there, and it's offered here.

## Archives

**Extract Here** unpacks a selected archive (`.zip`, `.tar.gz`, `.7z` …) into the folder it's in, and **Compress…** packs the selected items into one. Both need an archive manager: Engrampa, File Roller, Xarchiver or KDE's Ark. With several installed, choose which in **Preferences → Behavior → Archive manager**; **Automatic** takes the one set as the application for archives.

## Changes made elsewhere

Arkha follows changes on the disk: files that other programs add, change, rename or delete appear, change or go at once, without reloading. **View → Reload** (<kbd>F5</kbd>) reads a folder again anyway.
