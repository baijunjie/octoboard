# Processes panel

The **Processes panel** is a kind of panel in the right pane's stack (see `docs/product/right-pane-panels.md`) that
lists the background process trees Octoboard follows (see `docs/product/background-processes.md`) and lets the user
stop or keep them. Both kinds of owner have it:

- **a project's stack**, where it lists the trees of the project's running sessions and the project's processes, the
  trees of the project no session owns (see "Processes left behind in a project" in
  `docs/product/background-processes.md`); it is stacked under the Files panel and above the Terminal panel;
- **a console session's stack**, where it lists the console session's own trees; it is stacked under the report panel.

It is **closed** until the user opens it, from its toggle in the top bar, whose icon is a square with a pulse line
across it and whose label is "Show processes panel" or "Hide processes panel" (see "The panel toggles in the top bar"
in `docs/product/right-pane-panels.md`). Its header's title is "Processes", with nothing beside it and no Maximize
panel; for assistive technology the panel is a region named "Processes".

## What it lists

Only while the displayed list includes a session's trees, a note leads the list, saying that processes started by a
session stop automatically when the session ends. It is absent when only project processes are listed, while loading,
and in the empty state.

**In a project's panel with no session filtered on** (see "The session filter" below), the trees are grouped:

- **one group per session** that has trees, headed by the session's displayed name (see "Displayed names" in
  `docs/product/session-titles.md`), or "Unknown session" for a session the window does not know; the groups come in
  the order of their oldest tree;
- then **Project processes**, the trees no session owns, those the user kept and those left behind alike, under a note
  saying that they belong to no session, are never stopped automatically, and only the user can stop them. Kept trees
  and trees left behind are listed alike.

**With a session filtered on**, only that session's trees are listed, with no heading, and no project processes. **A
console session's panel** lists its own trees the same way; a console session with no process lists none.

Within a group, trees are oldest first. A heading too long for its line fades out where it is cut (see "Names too
long for their space" in `docs/product/labels-and-tooltips.md`).

**With nothing to list**, the panel shows the empty state "No processes" (see "The full panel" in
`docs/product/empty-states.md`), centred in the remaining body below any session filter and refresh warning. A line
under the heading says, with all of a project's sessions shown, that no session or project processes are running;
with a session filtered on or in a console session's panel, that nothing the session started is still running.

## The session filter

A project's panel has a **session filter** under its header, a select with no visible label (its name for assistive
technology says it filters by session). Left empty it shows the placeholder "All sessions" and the panel lists every
session's trees and the project processes; picking a session narrows the list to that session's trees, and the
select's clear button goes back to all of them. It offers the project's sessions that are not archived, by displayed
name; an archived session only while it is the one filtered on. A console session's panel has no filter.

**The filter follows the sidebar's selection**: whenever the session selected in the sidebar changes, the filter is set
to that session when it belongs to the project, and emptied otherwise. Opening the panel from **Open panel** in the
project's action menu (see "Opening a panel from a project's action menu" in `docs/product/right-pane-panels.md`) leaves
it empty, whatever session is selected. A choice made in the filter, clearing it included, lasts until the sidebar's
selection changes again or the right pane passes to another owner.

## A tree's card

Each tree is a card:

- **its command**, after a green play glyph, in monospace on one line; a command too long for the line fades out where
  it is cut, with the whole command as its tooltip (see "Names too long for their space" in
  `docs/product/labels-and-tooltips.md`). A command that could not be read reads "Unknown command";
- **its working directory**, after a folder glyph, or a crossed-out folder glyph and a line saying it is unknown,
  with an amber "Directory deleted" badge after it when the directory was removed;
- **a line of facts**: how much memory the tree uses, the ports it listens on, when its top process started ("started
  just now", or how long ago, kept up to date), and, for a tree of more than one process, how many it holds. The facts
  stay on one line, fading out where they are cut, with the whole line as their tooltip; this also applies to the
  facts in a process's confirmation and Details disclosure.

What a tree is and what these facts are taken from is in "Process information and actions" in
`docs/product/background-processes.md`.

## Stop and Keep

At the bottom end of a card, beside its facts, are **Keep** followed by **Stop**, text buttons, Stop in the danger
colour. They show while the card is hovered or holds keyboard focus, and always on a device whose primary input
cannot hover; hidden, they take no room from the facts and can still be reached with `Tab`. Showing the buttons leaves
the command and directory their full width. Each is named for assistive technology with the command, "Stop *command*"
or "Keep *command*", so the cards' buttons can be told apart. A mouse or touch press on one does not take keyboard focus.

**Keep is offered only on a project session's trees**: a console session keeps nothing, and a project process already
belongs to no session, so their cards offer Stop alone (see "Automatic stopping and its boundaries" in
`docs/product/background-processes.md`).

**Each asks for confirmation first**, showing the tree's command whole, wrapped rather than cut, and its working
directory and facts:

- **Stop process** says the process and its child processes will be stopped; its confirm button, **Stop**, is in the
  danger colour, and the command's glyph is a crossed-out play glyph in the danger colour.
- **Keep process** says the process and its child processes will no longer stop when the session ends, and stay
  listed under the project until the user stops them; its confirm button is **Keep**.

While a confirmation is open every card's buttons are unavailable. A tree that leaves the list before its confirmation
is confirmed closes the confirmation. What the two actions do is in "Process information and actions" in
`docs/product/background-processes.md`.

- **Once confirmed**, the tree leaves the list at once, the list is read again, and a toast says the process was
  stopped or kept (see `docs/product/toasts.md`); a tree that had already gone by then leaves no toast.
- **A failure** is shown in an error alert over the confirmation, titled that the process could not be stopped or kept,
  and the confirmation stays open (see "The error alert" in `docs/product/error-alerts.md`).
- **Keyboard focus**: where the confirmation was opened from the keyboard, so the card's button held focus, and the
  tree has left the list, focus goes, with its focus ring, to the first button of the card that followed it, or, with
  none, to where `F6` lands in the panel. A card that goes away while it or one of its buttons holds focus hands focus
  to where `F6` lands in the panel as well. A confirmation that closes because a refresh took its tree out of the list,
  where it was opened from the keyboard, hands focus on the same way: to the first button of the card that followed the
  tree, or, with none, to where `F6` lands in the panel.

## Loading, refreshing and failures

Until the first answer the panel says it is loading the processes, which assistive technology is told once the panel's
own announcement has finished. Then the list is read again:

- every **3 seconds** while the panel is on screen — open, or floating in (see "Floating a panel in" in
  `docs/product/right-pane-panels.md`);
- every **15 seconds** while it is not on screen in a window where the right pane floats (below 782 px), so that a
  swipe knows whether it has anything to show (see "What a swipe does" in `docs/product/touch-swipes.md`);
- not at all otherwise.

A reading that falls due while the window is hidden waits until it is shown again.

**When the first reading fails**, the panel's body shows an empty-state failure (see "Failures shown on the screen" in
`docs/product/error-alerts.md`) saying the processes could not be listed because the list did not come from
Octoboard's background service, with **Try again**; the list is not read again until Try again is pressed. The session
filter stays above it. Pressing Try again gives the failure's place to the loading line while the answer is awaited,
and where focus was on Try again it goes first to where `F6` lands in the panel; a retry that fails again shows the
failure, and announces it, afresh.

**When a later reading fails**, the last list stays, under an amber warning that the list could not be refreshed and
the processes shown may be out of date; readings go on, and the warning goes with the next one that succeeds.

## Keyboard focus

The panel is a stop of its own in the `F6` cycle, in the stack's order; where `F6` lands in it is in "Moving focus
between regions with F6" in `docs/product/moving-focus-between-regions.md`.
