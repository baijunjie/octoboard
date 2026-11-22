# Leaving a form with unsaved changes

The forms of the window's pages — the Remote devices form (see "The form" in `docs/product/remote-devices-page.md`) and
the Scheduled tasks form (see "The form" in `docs/product/scheduled-tasks-page.md`) — are not dropped unasked. While one
holds changes that are not saved, an action that would leave it first asks. Forms in dialogs are not covered by this.

## The question

The question is one destructive confirmation (see "Destructive confirmations" in `docs/product/dialogs.md`), titled
**Discard changes**, saying the changes in the form have not been saved and will be lost, with **Keep editing** and
**Discard**.

- **Keep editing**, Escape and the close button leave everything as it was: the form keeps its changes, and focus goes
  back where it was, or, where that has gone (the file viewer a quote closed, say), to the page.
- **Discard** drops the changes, as the form's own Revert or Cancel would, and then carries the action out.

A form holds changes while it would save something other than what is saved; a new item's form, once anything in it was
changed. **While a save of the form is under way, an action that would leave it does nothing.**

## What asks

- choosing another item in the page's sidebar, or starting a new one;
- the rail: a console's avatar, the current console's included, the waiting count, a session chosen from an avatar's
  menu, the jump from the rail's Processes popover, and the other page's button (Remote devices from the Scheduled tasks
  page, and the reverse);
- a session or a console this window opens, and a session chosen anywhere that leaves the page, such as a run's session
  on the Scheduled tasks page or the menu bar icon's menu;
- **Back** and **Forward**, by their buttons and their shortcuts: a move declined is not made, and nothing is recorded
  (see "Moving back and forward" in `docs/product/navigation-history.md`);
- Quote in the file viewer or the Find in files preview, which leaves the page to show the session terminal it writes
  into (see "Selecting and copying" in `docs/product/file-viewer.md`); kept, the page stays and nothing else is undone:
  the quote is written and the dialog it came from is closed;
- choosing the notification of a failed scheduled run (see "The notification of a failed run" in
  `docs/product/scheduled-tasks.md`), and the menus' Remote Devices item (see "Where remote daemons are reached from" in
  `docs/product/remote-daemons.md`);
- **quitting**, a close of the window that quits, and **restarting** — for an update, or from the notice that the daemon
  exited or the screen for a daemon of another version. The question comes first, before anything a quit or restart asks
  about running sessions, and brings the window forward so it is seen (see "Quitting" and "Restarting" in
  `docs/product/application-lifecycle.md`);
- in a browser, closing or reloading the tab, which asks with the browser's own prompt instead.

## What does not ask

- **Revert** and **Cancel**, each being the discard itself;
- the Scheduled tasks page's own tabs, which keep the changes (see "A saved task" in
  `docs/product/scheduled-tasks-page.md`), and the rail button of the page already shown;
- a form with no changes, and a window that is only hidden or closed into the background.

## Changes nobody asked for

**A change the user did not make never drops a form's changes.** While a form holds changes:

- the session or console the page was opened over being deleted, which would otherwise end the page, leaves the page
  where it is;
- the Remote devices page opened while the local daemon could not be used stays when the daemon becomes usable (see
  "Without the local daemon" in `docs/product/remote-devices-page.md`);
- a question or failure of a remote connection that the Remote devices page would show opens its dialog over the window
  without switching the page or the device it shows (see "What is handed to the page" in
  `docs/product/remote-devices-page.md`).

The Scheduled tasks page still goes away, with its form, when its daemon turns out to be of another version or refuses
the window's key, since nothing on it can be saved then.

## What cannot ask

Closing a remote window, logging out of or shutting down the system (see "When the system ends the session" in
`docs/product/application-lifecycle.md`), a forced quit, and the desktop application reloading its own window drop a
form's changes without asking.
