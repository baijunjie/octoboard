# Collection motion

## Items moving within a collection

When items are added, removed, reordered or folded within a visible collection, items that remain visible slide
to their new positions over **260 ms**. This applies to navigation lists, session cards and archive rows, settings
records, selected tags and wrapping chips, and the file and Git change lists and trees. Horizontal and wrapping
collections move horizontally and vertically as needed, in either reading direction.

New items appear immediately and removed items disappear immediately. Actions take effect without waiting for
movement to finish. A surviving control keeps keyboard focus; when the focused item is removed, that surface's
usual focus handoff applies.

The first appearance of a collection does not move its items into place. Scrolling and adjusting the collection's
width do not animate its items. In the file and Git change collections, only rows that stay rendered can slide;
scrolling new rows into view, resizing the viewport and reusing an off-screen row for another item do not animate
those rows. Replacing a whole page or directory listing and displaying a fixed result snapshot do not require
collection movement.

With the system's **Reduce motion** preference enabled, positions update immediately. Enabling it during a slide
stops the movement already in progress.
