# Searching and selecting

## Searching

Press <kbd>Ctrl</kbd>+<kbd>F</kbd> (**Go → Filter…**, or the Search button in the toolbar) and a Search field takes the place of the path.

- Type what you're looking for and press **Enter**: Arkha searches the open folder and all the folders in it (see [Only this folder](#only-this-folder)) for names that contain it, every word of it, in any order, in upper or lower case (`fold un` finds “Untitled Folder”).
- The results show as a place of their own, with a **Location** column saying where each item is; **Back** returns to the folder. **Open parent location** in a result's context menu opens the folder it's in.
- To search again with other words, change them and press **Enter** again.
- <kbd>Escape</kbd> closes the Search field.

### More criteria

Above the results, as in Caja, you can narrow the search down. The **+** button adds a criterion, **−** removes it, and **Reload** searches again with them:

| Criterion | Finds |
|---|---|
| **Location** | Items in another folder than the open one; with **include subfolders** ticked, also in the folders in it |
| **File Type** | Documents, music, videos, pictures, illustrations, spreadsheets, presentations, PDF and PostScript, text files, or any other type you pick (**Other Type…**) |
| **Tags** | Files that have all the tags you type, separated by spaces |
| **Modification Time** | Files changed within, or longer ago than, an hour, a day, a week, a month, six months or a year |
| **Size** | Files at most, or at least, a size you choose |
| **Contained text** | Files that contain the text |

### Only this folder

To search only the items of the folder itself, not the folders in it, untick **include subfolders** in the Location row above the results: the search runs again at once. Folders whose names match are still found; what's inside them isn't.

Whether a new search starts with **include subfolders** ticked is set in **Preferences → Behavior → Include subfolders**. It's on to begin with.

## Selecting

| To select | Use |
|---|---|
| Everything in the folder | **Edit → Select All** (<kbd>Ctrl</kbd>+<kbd>A</kbd>) |
| Items whose names match a pattern | **Edit → Select Items Matching…** (<kbd>Ctrl</kbd>+<kbd>S</kbd>) |
| Everything that isn't selected, and nothing that is | **Edit → Invert Selection** (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>I</kbd>) |
| The first item whose name starts with some letters | Type them |

In a pattern, `*` stands for any number of characters, `?` for one, and `[abc]` for one of those in the brackets: `*.jpg` selects all JPEG pictures, `IMG_20??*` all names starting with `IMG_20` and two more characters. Upper and lower case don't matter.
