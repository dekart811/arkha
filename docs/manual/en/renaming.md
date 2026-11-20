# Renaming

## One item

Select it and press <kbd>F2</kbd> (or **Rename…** in its context menu), type the new name and press <kbd>Enter</kbd>. Arkha selects the name without its extension, so `holiday.jpg` keeps its `.jpg` unless you select that too. <kbd>Escape</kbd> keeps the old name.

## Many items at once: Rename All

Select the items, right-click them and choose **Rename All…** (for a single item, the same window is called **Advanced Rename…**). It works like MATE's caja-rename and shows each new name beside the old one as you change the settings. **Apply** renames them and leaves the window open for more.

<picture>
<source media="(prefers-color-scheme: dark)" srcset="../../screenshots/rename-dark.png">
<img src="../../screenshots/rename-light.png" alt="Rename All replacing IMG_ with Holiday in eight picture names, the new names shown beside the old ones">
</picture>

The tabs work together, in this order: Case, Insert, Remove, Replace, Regex, and Enumerate last.

| Tab | Does | Example |
|---|---|---|
| **Case** | Upper case, lower case, title case, or leaves it unchanged | `IMG_0042.JPG` → `img_0042.jpg` |
| **Insert** | Puts text in at a position, counted from the start, or back from the end with **From the end**; **Before the extension** counts only in the name before it | `Report.pdf` → `2026 Report.pdf` |
| **Remove** | Takes away **Length** characters starting at **From** (a negative From counts from the end) | `IMG_0042.jpg` → `0042.jpg` |
| **Replace** | Replaces every occurrence of a text; **Exclude file extensions** leaves them out | `Photo 1.jpg` → `Holiday 1.jpg` |
| **Regex** | Like Replace, with a regular expression | `2026-11-20 notes.txt` → `20.11.2026 notes.txt` |
| **Enumerate** | Numbers the items in the order shown, from **Start**, with at least **Digits** digits; in front of the name, or **At the end** (and **Before the extension**) | `beach.jpg`, `dune.jpg` → `01 beach.jpg`, `02 dune.jpg` |

The tabs can be dragged into another order; Arkha keeps it.

### Regular expressions

In the Regex tab, **Regular expression** is searched for in each name and every match is replaced with **Replace with**. There, `$1`, `$2` … insert what the expression's groups in parentheses matched, `${name}` a named group, `$0` the whole match, and `$$` a dollar sign. The example above uses `(\d+)-(\d+)-(\d+)` and `$3.$2.$1`. A mistake in the expression is shown under it; until it's fixed, the Regex tab changes nothing.

### Before anything is renamed

Arkha refuses names that can't be used (empty ones, `.`, `..`, or with a `/`), lists the items that would be overwritten and asks first, and tells you if some couldn't be renamed. **Edit → Undo** takes back the whole Rename All in one step.
