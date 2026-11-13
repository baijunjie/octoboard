# Order of projects and sessions

How the sidebar (see `docs/product/sidebar.md`) orders its projects and sessions, and pinning, which takes part in
that order.

The order depends on the sort the user chooses in Settings, one choice for sessions and one for each of the two
segments of projects (see "Choosing the sort" below). The rules below give each choice.

**Projects** are listed:

1. projects with at least one unarchived session first, including interrupted sessions — the **active** projects —
   then the **inactive** projects, which have none;
2. within each segment, pinned projects first;
3. then by the choice for that segment. For active projects:
   - **Status**: by its most urgent unarchived session's status, in the status order below: a project with a session
     waiting for the user, then one with a session dormant, paused, done, awaiting instructions, with child sessions
     working, working in background, working, and last a project whose unarchived sessions are all interrupted;
   - **Name**: this rule is skipped;
   - **Recently used**: by when its most recently started unarchived session started, the most recent first.

   For inactive projects:
   - **Name**: this rule is skipped;
   - **Recently used**: by when its most recently ended archived session ended (for a session with no recorded end,
     when it started), the most recent first; a project with no session at all comes after every project that has one;
4. within each of those, by name, case-insensitively and in natural order, so that "item 2" comes before "item 10".

Each rule applies only among projects the rules before it leave tied. A project's most urgent unarchived session
sets its status priority, using the same order as sessions below. An active project comes before even a pinned
inactive one, whatever the choices.

**Inactive** projects end the list and, pinned ones included, are drawn dimmed as one group. Projects with only
interrupted sessions remain at full strength. Pointing at any inactive project, keyboard focus inside one, or an open
menu in one brings the whole inactive group back to full strength while it lasts.

**Sessions** within a project are listed, by the same scheme:

1. pinned sessions first;
2. then by the choice for sessions:
   - **Status**: by status, in the status order below;
   - **Recently started**: this rule is skipped;
3. then the most recently started first.

The console sessions section lists its console sessions by that same session scheme.

These three rules apply **within one level** of the project's list: the sessions the project lists at its own level are
ranked against each other, and the sessions under one owner against each other, each group following its owner wherever
the rules put it (see "Project rows" in `docs/product/sidebar-project-rows.md`). So a pinned session comes first among
the sessions beside it, not first in the whole project, and a session under an owner is never ranked against that owner.
Everything walked in this order follows the nesting the same way, so a session bound to a project session comes after
its owner wherever both appear.

Where Status is chosen, the rows it orders follow status changes as they happen; under any other choice, a session's
status changing while it stays unarchived moves none of the rows that choice orders. Row movement follows "Items moving
within a collection" in `docs/product/collection-motion.md`. The rail's waiting count walks waiting sessions in this
same order (see "The raised hand" in `docs/product/session-statuses.md`).

## The status order

Sessions that need a person to act or look come first, and sessions busy working, best left undisturbed, come last;
interrupted sessions come last of all. The statuses are those of "Session statuses" in
`docs/product/session-statuses.md`:

1. waiting for the user;
2. dormant;
3. paused;
4. done;
5. awaiting instructions;
6. child sessions working;
7. working in background;
8. working;
9. interrupted.

Archived sessions are not in the lists this order ranks; they are listed separately.

**Starting and switching account have no place of their own.** A session that is starting, or switching account, sorts
with the status it will have once that is over:

- with *working* when the launch will hand the agent a prompt: a new session opened with a task (a brief, an approved
  session request's opening prompt, or the brief of a handoff), a relaunch that continues a turn after an account switch
  or a change of model and effort, and a resume handed an instruction or a notice owed to the session as an owner;
- otherwise with *done* when the session has a conversation;
- otherwise with *awaiting instructions*.

For an account switch or a change of model and effort, whether the relaunch continues a turn is decided only once the
old process has stopped, so until then such a session sorts by whether it has a conversation. For a moment after its
launch has finished, a session that was handed a prompt can read as at its prompt before its agent's first report moves
it on to *working*, and sorts as such.

**Reconnecting has no place of its own either.** A session shown as reconnecting (see "Which status a session shows" in
`docs/product/session-statuses.md`) sorts with the status it stores.

## Choosing the sort

The sort is chosen in the **Sorting** group at the end of Settings' General section (see "Sorting" in
`docs/product/general-settings.md`), one select per part:

| Row | Orders | Choices |
|---|---|---|
| **Sessions** | sessions | **Status** (the default), **Recently started** |
| **Active projects** | projects with at least one unarchived session | **Status** (the default), **Name**, **Recently used** |
| **Inactive projects** | projects with no unarchived session | **Name** (the default), **Recently used** |

A choice applies at once, to the rules above. The three choices make **one sort, shared by every console and every
view**; neither the sidebar nor a focus mode has a sort control of its own. The sort is kept by this client and
survives reloading; it is not shared with other clients and the daemon does not hold it. A configuration export that
includes the General section carries this client's sort, and an import asked to apply that section sets it on the
client that runs the import alone (see "The application settings an export carries" and "The settings a client holds"
in `docs/product/settings-in-a-configuration.md`). Another page of the same
client open at the same time, such as a second browser tab, picks up a change only once reloaded. If client storage is
unavailable, a choice lasts for the current page load only.

**What follows the sort**: in the full sidebar, the console sessions section, the project list and each project's
sessions; in a project's focus mode, its session cards and the chips for the console sessions above its left-out
sessions; in a console session's focus mode, its project groups and the session cards under them (see
`docs/product/focus-mode.md`); the order in which the rail's waiting count walks waiting sessions (see "The raised hand"
in `docs/product/session-statuses.md`); the menu bar icon's session lists (see "The icon's menu" in
`docs/product/menu-bar-icon.md`); the list of running sessions in the confirmation for removing a project (see
`docs/product/removing-projects.md`); and the list of running sessions in the confirmation for removing an account with
its directory (see "The confirmation" in `docs/product/removing-accounts.md`). A console session's focus mode lists only
projects with an unarchived session under that console session, so its project groups are all active projects.

**What does not follow it**: other lists of sessions in confirmations, such as the archive confirmation's list of the
sessions under an owner, stay ranked by status. The menu of projects behind a console session's focus-mode header's
**+** lists them pinned first, then by name. The switch strip never ranks by status (see "The switch strip" in
`docs/product/focus-mode.md`).

## Pinning

**Pinning** is the user's own mark on a project or a session, set and cleared from its action menu's **Pin** /
**Unpin**, and cleared by its pin button too; nothing is pinned to begin with. Its only effect is the order above. The
pin is stored by the daemon with the project or the session, so every window and every client sees the same pins, and
they survive a restart. A pinned session stays pinned when it is archived and when it is resumed; an archived
session's menu offers no Pin or Unpin. A focus mode offers no Pin or Unpin for what is in focus, neither in a
project's focus mode header nor on a console session's own card in its focus mode (see `docs/product/focus-mode.md`).

**The pin button.** A pinned project or session shows a pin button: on its row in the sidebar, after the row's markers
(a project's branch badge and status marker, a session's binding badge) and before the buttons that show only on hover,
so that while those are hidden the pin buttons of project and session rows line up in one column at the rows' end; after
the name on a project's heading in a console session's focus mode; and on the status line of its card in a focus mode,
except a console session's own card in its focus mode, which never shows one. It is always shown while the item is
pinned, and is reached with Tab like the row's other buttons. While the pointer is over it or it holds keyboard focus,
its pin glyph turns into the unpin glyph. Its tooltip is "Unpin", while the name assistive technology announces for it
also names the item ("Unpin …"). Pressing it unpins the item, as the menu's Unpin does; a mouse press neither selects
the row nor moves keyboard focus. The button goes away with the pin, so when it held keyboard focus, focus is handed
first to the row or card it was on — or, on a project's heading in a console session's focus mode, to that heading's
**+** button — its focus ring showing only when the last input was a key.
