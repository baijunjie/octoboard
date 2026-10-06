# Settings

Settings is a large dialog over the whole window. A list of sections runs down its left side and the selected
section's settings fill the right, one row per setting: its name and a line saying what it does on the left, its
control on the right; under a right-to-left language the sides swap (see "Right-to-left layout" in
`docs/product/window-layout.md`). The sections, in order, are **General**, **Trusted folders** and
**Notifications**; the dialog opens on General.

## Moving between sections

The list of sections is a vertical tab list, with the selected section highlighted. Opening Settings puts keyboard
focus on the selected section's tab. Only that tab is reached with Tab: the arrow keys move to the previous or next
section and show it at once, wrapping around at either end, and Tab from the list moves on into the section's
settings. When a control holding keyboard focus disappears from a section — a trusted folder's Remove button, which
goes with its folder, or the Notifications Enable button once the browser has answered — focus goes back to the
selected section's tab rather than leaving the dialog.

## Opening and closing Settings

Settings opens from:

- the **Settings** button at the right end of the top bar (see "The top bar" in `docs/product/window-layout.md`);
- in the macOS application, the application menu's **Settings…** item, shortcut `Cmd+,`. The menu item does nothing
  while the window has nothing to show yet (before the daemon's state has first arrived), while Settings is already
  open, and while another dialog or a row's action menu is open.

Settings closes on `Escape`, on its close button, and on a press on the dimmed area around it. Closing it puts
keyboard focus back on the selected session's terminal.

While Settings is open everything under it stays as it was, the terminal included, and its size does not change.

## General

The settings that shape the whole window rather than one feature: an **Appearance** row, then a **Language** row.

### Appearance

The Light / Dark / System choice for the window's appearance, as a three-way segmented control. What each option means,
which one is the default and where the choice is kept are in `docs/product/appearance.md`.

### Language

The language of the UI, chosen from a drop-down of the offered languages, each named in its own language. What the
options are, how the first launch picks a language, what follows the choice and where it is kept are in
`docs/product/language.md`.

## Trusted folders

The folders under which Octoboard answers Claude Code's workspace-trust prompt for every project, with a way to stop
trusting each one. What the list shows and what removing a folder does are in "Trusted folders" in
`docs/product/launching-agents.md`.

## Notifications

Whether Octoboard may show a system notification when a session starts waiting for the user (see "The raised hand" in
`docs/product/sessions.md`). The row says what the current state is:

| Client | What the row shows |
|---|---|
| The macOS application | That Octoboard posts notifications directly, and that they are turned off in the system's own notification settings. No permission prompt is ever shown, and there is nothing to press. |
| A browser, not yet decided | That notifications are not enabled yet, with an **Enable** button that asks the browser. |
| A browser, allowed | That they are allowed. |
| A browser, blocked | That they are blocked, and that they are allowed again from the browser's own settings for the site. |
| A client with no notifications at all | That notifications are not available there. |

A browser is asked only from a press of the user's, never on its own: a browser ignores or refuses an ask made
otherwise. Besides the Enable button here, a browser that has not decided yet gets the same ask as the **Turn on
notifications** button in the top bar: a bell icon with a small dot on it, shown only while the browser's answer is
undecided (see "The top bar" in `docs/product/window-layout.md`). It cannot be dismissed; pressing it asks the
browser. Once the browser has answered, whichever the answer, the bell is gone, and answering from either place
updates the other. The macOS application never shows the bell, since it asks nothing. A browser's state is read
again whenever the window regains focus, since it can be changed in the browser's own settings meanwhile.
