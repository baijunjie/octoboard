# Processes on the rail

The rail's **Processes** button, directly below the waiting count (see "The rail" in `docs/product/window-layout.md`),
shows how many background process trees are running across every project and console session, of every console, and
opens a popover saying where they run. Each of its rows leads to the Processes panel that lists those trees (see
`docs/product/processes-panel.md`). The popover lists no process itself and offers no Stop or Keep: those are the
panel's.

## What is counted

The count is of process trees, the cards a Processes panel shows (see "A tree's card" in
`docs/product/processes-panel.md`):

- **for a project**, the trees of its running sessions and its project processes, those the user kept and those left
  behind alike (see "Processes left behind in a project" in `docs/product/background-processes.md`) — everything its
  Processes panel lists with all sessions shown;
- **for a console session**, its own trees.

Kept processes that belong to no project, which no Processes panel lists either (see "Automatic stopping and its
boundaries" in `docs/product/background-processes.md`), are not counted, and neither are the trees of a project the
window no longer knows, so the button's count is always the sum of the popover's rows. The count makes no distinction
between session processes and project processes, or between kept trees and trees left behind.

## The button

The button's glyph is the Processes panel's (a square with a pulse line across it), with the count in an
green badge at the button's top right — its top left under a right-to-left language, as the waiting count's
(see "What is mirrored" in `docs/product/right-to-left-layout.md`) — reading **99+** beyond 99. It is there only while at
least one tree is counted: not while none is, and not before the first count has come in.

It has no tooltip (see "Tooltips on icon-only controls" in `docs/product/labels-and-tooltips.md`). Its name for
assistive technology says how many processes are running and that pressing it shows where they run.

When the button goes while it or its popover holds keyboard focus, focus moves to the rail's next control (Turn on
notifications or Account usage).

## Opening and closing the popover

The popover opens and closes as the account quota panel does (see "Opening the panel", "Closing the panel" and "The
panel is not modal while the mouse has floated it in" in `docs/product/account-quota-panel.md`): resting the mouse on
the button floats it in, without moving keyboard focus and without taking the window around it; a mouse click opens
nothing; a tap, assistive technology and **Enter** or **Space** open it as a modal popover. Assistive technology is told
it is a dialog named "Processes". What differs:

- **Opened by a tap or the keyboard**, keyboard focus goes to its first row.
- **The Details popover of its refresh warning** (see "Keeping the count current" below) counts as part of it: a press
  inside the Details popover does not close it, and **Escape** there closes the Details popover alone. While the
  Details popover is open, the popover stays open however the mouse leaves it; once the Details popover has gone, a
  popover the mouse floated in closes at once unless the mouse is resting on it or on the button.
- **It closes by itself** when the last counted tree goes, and when a modal dialog, menu or popover opens other than from
  inside it, such as Settings from the application menu.
- **Closing with keyboard focus inside it** — on the Details button, say — hands focus to the Processes button, unless
  something else has taken focus by then. Pressing a row is the exception (below).

## The rows

One row per project that has trees counted, then one per console session that has, each showing:

- **its name** — the project's, or the console session's displayed name (see "Displayed names" in
  `docs/product/session-titles.md`) — on one line, and beneath it, smaller and muted, **the name of its console**; each
  fades out where it is cut, with the whole name as its tooltip (see "Names too long for their space" in
  `docs/product/labels-and-tooltips.md`);
- **its count**, in a small bordered green status badge;
- a **chevron** pointing to the reading direction's end, mirrored under a right-to-left language.

Its name for assistive technology says the name, its console, how many processes are running there, and that pressing
it shows the Processes panel. The list is at most 70% of the window's height, and scrolls and fades at its edges beyond
that (see "The surfaces that fade" in `docs/product/scrolling-surfaces.md`). Rows that come, go or move between counts
do so with the collection motion (see "Items moving within a collection" in `docs/product/collection-motion.md`).

**Pressing a row** closes the popover and then:

- makes the row's console the current one;
- gives the right pane to the row's project or console session and opens its Processes panel, as **Open panel** does
  from a project's action menu (see "Opening a panel from a project's action menu" in
  `docs/product/right-pane-panels.md`): a project's panel lists the processes of all its sessions and its project
  processes, its session filter empty whatever session is selected. It selects, starts and resumes no session;
- sends keyboard focus into that panel, where `F6` lands in it (see "Moving focus between regions with F6" in
  `docs/product/moving-focus-between-regions.md`), once the panel is on screen and has something to take focus, waiting
  for it at most 3 seconds; focus the user has moved elsewhere meanwhile stays there.

A row whose project or console session no longer exists by then opens nothing, and focus goes to the Processes button.

## Keeping the count current

The counts are read afresh from the background service every **15 seconds**, and every **3 seconds** while the popover
is open. A reading that falls due while the window is hidden waits until it is shown again.

The background service shares its counts between readings, from every window and client connected to it: counts it
worked out less than **2 seconds** earlier answer a reading again, unless a tree has been stopped or kept, or a launch
has ended, since. Readings that arrive while counts are being worked out wait for that one result rather than each
working out their own. A reading waits for it at most about **10 seconds**; past that it is answered with the last
counts worked out, however old, or fails with the code `process_count_timed_out` when there are none yet.

A reading that fails keeps the last counts, on the button and in the rows. While the failure lasts, the popover shows an
amber warning alert at its top (see "Failures shown on the screen" in `docs/product/error-alerts.md`), titled that the
list could not be refreshed, saying that the counts shown may be out of date, with **Details**. Where Octoboard has
words of its own for the failure, the alert says why first, then that the counts may be out of date: for
`process_count_timed_out`, that counting the processes took too long. A failure it has no words for says only that the
counts may be out of date, its own text left to **Details**. It is announced to assistive technology only when it
arrives while the popover is open, and goes with the next reading that succeeds.
