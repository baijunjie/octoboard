# Touch swipes

In a window narrower than 782 px — where the sidebar is a drawer that comes in from the start side, with the rail in it
(see `docs/product/sidebar-drawer.md`), and the right pane's panels float over the terminal one at a time from the end
side (see "Below 782 px: one panel floats over the terminal" in `docs/product/right-pane-panels.md`) — a finger swiping
sideways opens and closes them. At 782 px and wider swipes do nothing.

## What counts as a swipe

- **Only a finger.** A mouse, a trackpad and a pen never swipe.
- **Anywhere in the window**, over the terminal, the drawer, a floating panel or the top bar alike, except:
  - a touch that starts on a resize handle or a slider, which takes the drag itself;
  - a touch that starts in something that can still scroll sideways in the direction the finger moves — a code view,
    a diff, a wide table — which scrolls instead; one that has nothing more to show that way lets the
    swipe through;
  - any touch while a dialog, a menu or a popover is open.
- **It goes sideways.** It counts once the finger has moved **56 px** sideways while having moved at least twice as far
  sideways as up or down, at that moment rather than when the finger lifts; nothing follows the finger meanwhile. A move
  that goes more than 12 px up or down, and further so than sideways, is a scroll, and the rest of that touch is not a
  swipe even if it then goes sideways.
- **One finger.** A second finger touching down cancels it.
- **One swipe per touch.**

Its direction is the reading direction's: in a left-to-right language a finger moving right swipes towards the end and
one moving left towards the start; under a right-to-left language the other way round (see
`docs/product/right-to-left-layout.md`). What follows is written for left-to-right.

## What a swipe does

**A finger moving right:**

- with a panel floating over the terminal, closes it;
- otherwise opens the sidebar's drawer, while there is a console and it is closed;
- otherwise does nothing.

**A finger moving left:**

- with the drawer open, closes it;
- otherwise, while the right pane has an owner (see "What the right pane shows" in `docs/product/window-layout.md`):
  - with no panel floating over the terminal, floats the panel last on screen for the owner's kind (see "Below 782 px:
    one panel floats over the terminal" in `docs/product/right-pane-panels.md`) while its toggle is offered, or,
    when there is none, the first panel the top bar offers a toggle for, in their order (see "The panel toggles in the
    top bar" in `docs/product/right-pane-panels.md`); a Terminal panel opened this way starts a shell when the project
    has none, as its toggle does;
  - with a panel floating, puts the next panel the top bar offers in its place, the last one wrapping round to the
    first; an owner with only one kind of panel, such as a console session with its report panel, gets nothing;
- with no owner, does nothing.

A swipe opens and closes these as their toggles do, so the drawer and a floating panel are still never on screen
together. With no console (the welcome screen, see `docs/product/welcome-screen.md`) there is neither a drawer nor a
right pane, so no swipe does anything.

## Keyboard focus after a swipe

- **A swipe that floats a panel** moves focus into it as a panel opened by a tap does (see "Focus after a tap" in
  `docs/product/moving-focus-between-regions.md`).
- **A swipe that opens the drawer** leaves focus where it is; a tooltip already showing on a top bar control that has
  keyboard focus stays until focus moves.
- **A swipe that closes the drawer or a panel** hands focus on as any other close does (see "Below 1168 px: the
  sidebar becomes a drawer" in `docs/product/sidebar-drawer.md` and "Below 782 px: one panel floats over the terminal"
  in `docs/product/right-pane-panels.md`).
