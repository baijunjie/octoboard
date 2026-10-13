# A fade wherever content scrolls out of sight

## Problem

Where a surface holds more than it can show, the edge it hides content behind should fade, so that there is
something to see rather than only something to try. Much of the interface already does this — the sidebar, focus
mode, the archive, the file tree, the change list and tree, the directory picker, the settings panels, and every
dialog body — but it was added surface by surface, so the ones nobody thought about have nothing:

- The file viewer's reading surfaces: its code surface, both of them, and its rendered Markdown. They sit inside a
  dialog, but they are the viewer's own content rather than the dialog's body, so the body's fade does not reach
  them.
- Lists inside a popover: the branch list of a branch comparison, and the tag list of the project filter.
- The rail's console column, the startup screen, and the error boundary's stack block.

What keeps this from staying fixed once it is: nothing fails when a new scroll container arrives without a fade, so
the next one will be missed the same way these were.

## Plan outline

Give every remaining scroller the fade; then make a scroll container with no fade fail a check rather than wait to be
noticed.

Only the vertical edges fade — that much is settled. What the rule turned out to owe the other axis is not a fade but
a strip the mask leaves alone where a surface scrolls sideways, so the fade does not wash out the scrollbar sitting
there; the capability does that itself.

## Key design decisions

- **Every overflowing surface fades, not only a dialog's.** The goal is the interface over, so the question for a
  new scroller is which fade it takes, never whether it takes one.
- **It is solved at the component layer, never per site.** A surface that needs a fade reaches for the shared way to
  get one; it does not hand-build a mask, and it does not restate the rule in its own styles.
- **A modal whose content is one body uses `Dialog`; one whose content is a set of panels does not.** `Dialog` is a
  titled frame around a single scrolling body, with the fade on that body, the scroll padding that keeps focus clear
  of it and the focusable region for a body whose controls all sit in the footer. The settings dialog is the other
  shape — a panel per section, each scrolling on its own, the sections beside the selected panel rather than under a
  title — so it keeps its own frame and takes the fade per panel; widening `Dialog` to cover it would add three
  switches no other modal would ever pass.
- **The fade never goes on an element that animates away.** Putting it on one leaves that element on screen for
  good: the fade is driven by a scroll-timeline animation that never ends, and an exit transition waits for the
  animations on its element to finish. This was met on the settings dialog's tab panels, where the panel left behind
  stayed over its replacement with its exiting state stuck. The scroller goes inside such an element instead.

## Open

Not settled; none of these changes what a milestone is for.

- Whether a reading surface takes the same fade size as a list, given that a fade over code hides characters where a
  fade over a row hides the row's ends.
- What form the check takes — a lint rule, a test over the built styles, or something else.

## Milestones

1. 01 Every remaining scroller fades (closed)
1a. 01a One way to take the fade (closed)
2. 02 Hand-built modals move onto `Dialog` (dropped)
3. [A scroller with no fade fails a check](03-a-check-for-a-missing-fade.md)
4. 04 Final confirmation (closed)
