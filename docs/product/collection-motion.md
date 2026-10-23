# Collection motion

## Items moving within a collection

When items are added, removed, reordered or folded within a visible collection, items that remain visible slide
to their new positions over **300 ms**. This applies to navigation lists, session cards and archive rows, settings
records, selected tags and wrapping chips, the file and Git change lists and trees, the accounts listed in the account
quota panel on the rail, and tab strips: the agent tabs in Settings' Agent accounts section, in the account quota panel
and in the console dialog's Advanced section, and the Terminal panel's tabs. The quota meters within one account are
not such a collection: they appear and disappear without motion as its readings change. Horizontal and wrapping
collections move horizontally and vertically as needed, in either reading direction. A tab dropped at the end of a drag
slides into its new place from where it was let go.

New items fade in from **25% of their own width to the left** of their final positions; removed items fade out
while moving **25% of their own width to the right** from their previous positions. These directions stay the same
in either reading direction. These effects run over the same **300 ms**, together with the movement of surviving
items. Entrances and exits accelerate and then decelerate; surviving items ease out into their new positions.
Actions take effect without waiting for the animation to finish. A departing item no longer accepts input or
keyboard focus and is no longer exposed to assistive technology. A surviving control keeps keyboard focus; when
the focused item is removed, that surface's usual focus handoff applies.

The first appearance of a collection shows its items without animation. Adding an item to an already displayed
empty collection uses the entrance effect. Scrolling and adjusting the collection's width do not animate its
items. In the virtualized file and Git change collections, adding or removing rows from the displayed list uses
the same entrance and exit effects for rendered rows, except for rows that fold. Rows that appear or vanish because
their directory, commit or change section is expanded or folded have no entrance or exit effect; only the rows
around them slide to their new positions. The entrance and exit effects apply to top-level rows, to rows added or
removed under a directory, commit or change section that is expanded both before and after the change, and to rows
added or removed together with their parent, unless that parent folds in or out, in which case they fold with it.
In the file tree, an expanded directory counts as expanded only once its contents are listed: while it shows only
its loading or error row, rows that arrive or leave, including the rows that replace that loading or error row,
fold with the directory rather than entering or leaving. Only rows that stay rendered can slide to new positions. Scrolling new rows into view,
resizing the viewport and reusing an off-screen row for another item do not animate those rows. Replacing a whole
page or directory listing and displaying a fixed result snapshot do not require collection movement.

With the system's **Reduce motion** preference enabled, items appear, disappear and change positions immediately.
Enabling it during an animation stops the effects already in progress and clears departing items.
