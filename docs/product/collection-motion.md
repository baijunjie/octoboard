# Collection motion

## Items moving within a collection

When items are added, removed, reordered or folded within a visible collection, items that remain visible slide
to their new positions over **300 ms**. This applies to navigation lists, session cards and archive rows, settings
records, selected tags and wrapping chips, and the file and Git change lists and trees. Horizontal and wrapping
collections move horizontally and vertically as needed, in either reading direction.

New items fade in from **25% of their own width to the left** of their final positions; removed items fade out
while moving **25% of their own width to the right** from their previous positions. These directions stay the same
in either reading direction. These effects run over the same **300 ms**, together with the movement of surviving
items. Entrances and exits accelerate and then decelerate; surviving items ease out into their new positions.
Actions take effect without waiting for the animation to finish. A departing item no longer accepts input or
keyboard focus and is no longer exposed to assistive technology. A surviving control keeps keyboard focus; when
the focused item is removed, that surface's usual focus handoff applies.

The first appearance of a collection shows its items without animation. Adding an item to an already displayed
empty collection uses the entrance effect. Scrolling and adjusting the collection's width do not animate its
items. In the virtualized file and Git change collections, only rows that stay rendered can slide; rows appear
and disappear immediately, without entrance or exit effects. Scrolling new rows into view, resizing the viewport
and reusing an off-screen row for another item do not animate those rows. Replacing a whole page or directory
listing and displaying a fixed result snapshot do not require collection movement.

With the system's **Reduce motion** preference enabled, items appear, disappear and change positions immediately.
Enabling it during an animation stops the effects already in progress and clears departing items.
