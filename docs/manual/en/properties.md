# Properties

**Properties** (<kbd>Alt</kbd>+<kbd>Enter</kbd> or <kbd>Ctrl</kbd>+<kbd>I</kbd>, or in the context menu) opens a window with everything about the selected items, in tabs. With several items selected, it shows what they have in common and changes all of them at once.

To see another item's properties, drag it into the open Properties window: the window shows it, on the tab you had chosen.

## Basic

The name (which you can change here), type, size, location, and the dates it was modified, accessed and created. A folder's size is counted in the background, with how many items it holds. For a drive's top folder, Basic also shows how full the drive is.

**Click the icon** to give the item an icon of your own, picked from a picture file; in that window, **Revert** goes back to the usual icon. Caja shows the same icon.

## Emblems

Emblems are small pictures on an item's icon, such as a star or "important", from your icon theme. Tick the ones the item should have. They are kept by GVfs, shared with Caja.

You can also drag emblems onto items: see [Backgrounds and emblems](#backgrounds-and-emblems) below.

## Permissions

Who may read, change (write) and run (execute) the item: its **owner**, its **group**, and **others**. For a folder, execute means being allowed to go into it. Only the owner can change them.

- **Special** permissions: **Set user ID** and **Set group ID** make a program run as its owner or group, whoever starts it; a folder with **Sticky** set only lets an item's owner delete or rename it.
- The **group** can be changed to another of your groups, by the owner. Only the administrator can give an item to another owner.
- For a folder, **Apply to Enclosed Items…** gives everything inside it the same permissions. This can't be undone.

The SELinux security context shows here too, where the system uses SELinux.

## Open With

Which application opens files of this type. Select another one and press **Set as Default**.

## Notes

A note kept with the item, for anything you want to remember about it. Items with a note show the note emblem. Notes are kept by GVfs and shared with Caja. A note is saved a moment after you stop typing, and when the window closes.

## Media Info

For videos, music and pictures: codecs, resolution, duration, bit rate and the like, as [MediaInfo](https://mediaarea.net/MediaInfo) reports them. It needs the `mediainfo` program. Which kinds of files get this tab is set in **Preferences → Previews → Media Info**.

## Checksums

<picture>
<source media="(prefers-color-scheme: dark)" srcset="../../screenshots/checksums-dark.png">
<img src="../../screenshots/checksums-light.png" alt="The Checksums tab of a picture's Properties, with its CRC32, MD5, SHA-1 and SHA-256">
</picture>

A checksum is a long number worked out from a file's contents: if two files have the same checksum, they are the same. Websites give them for downloads, so you can check yours arrived unchanged.

Press a checksum's **Calculate** button to work it out (CRC32, MD5, SHA-1, SHA-256, and SHA-384, SHA-512 and BLAKE2b too if you choose them), or **Calculate All** to work out all of them in one go. A progress bar shows the time left, and the work can be cancelled. **Copy** puts a checksum on the clipboard. Which checksums are offered, and in which order, is set in **Preferences → Properties → Checksums**.

## Choosing the tabs

Right-click a tab and choose **Hide This Tab** to take it out of the window (this deletes nothing); bring it back in **Preferences → Properties**. All tabs but Basic can be hidden. Drag tabs into another order in the window itself; Arkha keeps it.

## Backgrounds and emblems

**Edit → Backgrounds and Emblems…** opens a window of patterns, colours and emblems, as in Caja:

- Drag a pattern or colour onto a folder's empty space to give that folder a background (Caja shows it too), or onto the side pane. Drag **Reset** there to take it off again.
- Drag an emblem onto an item, or onto all the selected ones. Drag **Erase** onto an item to take its emblems off.
- Add your own colours, patterns and emblems; right-click one to remove it. Your own emblems are kept where Caja keeps them, so both show them.
