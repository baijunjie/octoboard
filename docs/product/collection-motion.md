# Collection motion

## Items moving within a collection

When items are added, removed, reordered or folded within a visible collection, items that remain visible slide to their
new positions over **300 ms**. This applies to navigation lists, session cards and archive rows, settings records, the
rows of Settings' Git section that come and go with its Where sessions develop choice (see "Development" in
`docs/product/git-settings.md`), the section tabs of Settings and the rows, group headings and blocks of a Settings section
as a search hides and shows them (see `docs/product/searching-settings.md`), selected tags and wrapping chips, the file and Git change lists and trees, the accounts
listed in the account quota panel on the rail, the rows of the rail's Processes popover, and tab strips: the agent tabs in Settings' Agent accounts section, in
the account quota panel and in the console dialog's Advanced section, and the Terminal panel's tabs. The quota meters
within one account are not such a collection: they appear and disappear without motion as its readings change, and
neither are the pickers of the project dialog's Git workflow tab, which appear and disappear without motion as its
Where sessions develop picker changes. Horizontal and wrapping collections move horizontally and vertically as needed,
in either reading direction. A tab dropped at the end of a drag slides into its new place from where it was let go.

New items fade in from **25% of their own width to the left** of their final positions; removed items fade out
while moving **25% of their own width to the right** from their previous positions. These directions stay the same
in either reading direction. These effects run over the same **300 ms**, together with the movement of surviving
items. Entrances and exits accelerate and then decelerate; surviving items ease out into their new positions.
Actions take effect without waiting for the animation to finish. A departing item no longer accepts input or
keyboard focus and is no longer exposed to assistive technology. A surviving control keeps keyboard focus; when
the focused item is removed, that surface's usual focus handoff applies. In the collections whose rows are separated
by dividers — the rows of a Settings section and the accounts in the account quota panel — the last row that
stays loses its divider as soon as the rows after it start leaving, not once they have gone.

The first appearance of a collection shows its items without animation. Adding an item to an already displayed empty
collection uses the entrance effect. Scrolling and adjusting the collection's width do not animate its items. In the tab
strips listed above, a tab that grows or shrinks on its own — an agent tab gaining or losing its not-installed warning
mark, a terminal tab whose title changes — has the tabs after it slide to their new positions; every tab changing width
at once, as when the text is zoomed or its font changes, counts as adjusting the width and does not animate. In the
virtualized file and Git change collections, adding or removing rows from the displayed list uses the same entrance and
exit effects for rendered rows. Rows that appear or vanish because their directory, commit or change section is
expanded or folded instead fade in from **8 px above** their final positions, or fade out toward **8 px above** their
previous positions, over the same **300 ms**; the rows around them slide to their new positions. The horizontal
entrance and exit effects apply to top-level rows, to rows added or removed under a directory, commit or change
section that is expanded both before and after the change, and to rows added or removed together with their
parent, unless that parent folds in or out, in which case they fold with it. In the file tree, an expanded directory
counts as expanded only once its contents are listed: while it shows only its loading or error row, rows that arrive or
leave, including the rows that replace that loading or error row, use the directory's vertical fold effect. Only rows that stay rendered can slide to new positions. Scrolling new rows into view, resizing the viewport
and reusing an off-screen row for another item do not animate those rows. Replacing a whole page or directory listing
and displaying a fixed result snapshot do not require collection movement.

In the sidebar, expanding or folding a project brings its sessions in or out as one body with the same **8 px**
vertical fade, rather than animating each session separately. Its height follows that body as described below.

With the system's **Reduce motion** preference enabled, items appear, disappear and change positions immediately.
Enabling it during an animation stops the effects already in progress and clears departing items.

## A collection's height following its rows

The Development group of Settings' Git section (see "Development" in `docs/product/git-settings.md`) and the sessions
body below each sidebar project have a height that follows their contents. In Settings, when Squash commits before
delivery, Rebase before delivery and Delivery leave or come back with the Where sessions develop choice, or a row
grows or shrinks on its own, as when the Where sessions
develop description changes how many lines it takes, the group's height changes over the same **300 ms**, easing out
like the surviving rows, so the Protection group below it glides along instead of jumping. Meanwhile what the group
holds is clipped at its top and bottom edges: departing rows are never drawn over what follows the group, and an
entering row, or the new part of a row that grew, shows as the group's edge reaches it. A change that arrives while
another is still playing starts from the height shown at that moment.

In the sidebar, a project's sessions body grows or shrinks over the same **300 ms**, clipping its contents at its top
and bottom edges while they fade in or out. Projects below it follow that changing height without a second slide
on top of it. A change to the actual order of projects or sessions still uses the collection movement described
above, including while a project is expanding or folding.

When the panel holding the Settings section on screen is scrolled to its end and the group shrinks, the scroll
position comes down with the group over the same time instead of jumping at once.

With **Reduce motion** enabled, these surfaces take their new heights immediately.
