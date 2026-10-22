# Order of projects and sessions

How the sidebar (see `docs/product/sidebar.md`) orders its projects and sessions, and pinning, which takes part in
that order.

The order depends on the **Order the sidebar by status** setting (see that section in `docs/product/settings.md`),
which is on to begin with. While it is on, the order is the one below; how it differs while the setting is off is in
"Ordering without status" below.

**Projects** are listed:

1. projects with at least one unarchived session first, including interrupted sessions;
2. within each group, pinned projects first;
3. then a project with a session waiting for the user;
4. then a project with a session working, a session working in background included;
5. then a project with a session running and awaiting instructions, or still starting;
6. then a project whose unarchived sessions are all interrupted;
7. within each of those, by name, case-insensitively and in natural order, so that "item 2" comes before "item 10".

Each rule applies only among projects the rules before it leave tied. A project with an unarchived session comes
before even a pinned project with none.

**Inactive** projects have no unarchived session. They end the list and, pinned ones included, are drawn dimmed as
one group. Projects with only interrupted sessions remain at full strength. Pointing at any inactive project,
keyboard focus inside one, or an open menu in one brings the whole inactive group back to full strength while it lasts.

**Sessions** within a project are listed, by the same scheme:

1. pinned sessions first;
2. then by status: waiting for the user, then awaiting instructions and starting alike, then working in background,
   then working, then interrupted;
3. then the most recently started first.

The console sessions section lists its console sessions by that same session scheme.

These three rules apply **within one level** of the project's list: the sessions the project lists at its own level are
ranked against each other, and the sessions under one owner against each other, each group following its owner wherever
the rules put it (see "Project rows" in `docs/product/sidebar-project-rows.md`). So a pinned session comes first among
the sessions beside it, not first in the whole project, and a session under an owner is never ranked against that owner.
Everything walked in this order follows the nesting the same way, so a session bound to a project session comes after
its owner wherever both appear.

The order follows status changes as they happen. Row movement follows "Items moving within a collection" in
`docs/product/collection-motion.md`. The rail's waiting count walks waiting sessions in this same order (see
"The raised hand" in `docs/product/sessions.md`).

## Ordering without status

With **Order the sidebar by status** off, no project and no session is ranked by its status; everything else above
stands.

- **Projects**: rules 3 to 6 are skipped, so within each group pinned projects come first, and pinned and unpinned
  projects are each ordered by name as in rule 7. Projects with at least one unarchived session still come first, and
  the inactive projects still end the list as one dimmed group; a project still joins that group when its last
  unarchived session is archived and leaves it when a session in it is opened or resumed.
- **Sessions**: rule 2 is skipped, so pinned sessions come first, then the most recently started first. That holds at
  every level — the sessions a project lists at its own level, the sessions under one owner, and the console sessions
  section — with the same nesting as above.

A session's status changing while it stays unarchived then moves no row.

The setting reaches every list that follows this order: both focus modes' lists and project groups, and the chips of a
project's focus mode for the console sessions above its left-out sessions (see `docs/product/focus-mode.md`); the
order in which the rail's waiting count walks waiting sessions (see "The raised hand" in `docs/product/sessions.md`);
the menu bar icon's session lists (see "The icon's menu" in `docs/product/menu-bar-icon.md`); and the list of running
sessions in the confirmation for removing a project (see `docs/product/removing-projects.md`). Other lists of sessions in confirmations, such as the
archive confirmation's list of the sessions under an owner, stay ranked by status whatever the setting. The switch
strip never ranks by status (see "The switch strip" in `docs/product/focus-mode.md`).

## Pinning

**Pinning** is the user's own mark on a project or a session, set and cleared from its action menu's **Pin** /
**Unpin**, and cleared by its pin button too; nothing is pinned to begin with. Its only effect is the order above. The
pin is stored by the daemon with the project or the session, so every window and every client sees the same pins, and
they survive a restart. A pinned session stays pinned when it is archived and when it is resumed; an archived
session's menu offers no Pin or Unpin. A focus mode offers no Pin or Unpin for what is in focus, neither in a
project's focus mode header nor on a console session's own card in its focus mode (see `docs/product/focus-mode.md`).

**The pin button.** A pinned project or session shows a pin button: on its row in the sidebar, after the row's markers
(a project's branch badge and activity marker, a session's binding badge) and before the buttons that show only on
hover, so that while those are hidden the pin buttons of project and session rows line up in one column at the rows'
end; after the name on a project's heading in a console session's focus mode; and on the status line of its card in a
focus mode, except a console session's own card in its focus mode, which never shows one. It is always
shown while the item is pinned, and is reached with Tab like the row's other buttons. While the pointer is over it or it
holds keyboard focus, its pin glyph turns into the unpin glyph. Its tooltip is "Unpin", while the name assistive
technology announces for it also names the item ("Unpin …"). Pressing it unpins the item, as the menu's Unpin does; a
mouse press neither selects the row nor moves keyboard focus. The button goes away with the pin, so when it held
keyboard focus, focus is handed first, with its focus ring showing, to the row or card it was on — or, on a project's
heading in a console session's focus mode, to that heading's **+** button.
