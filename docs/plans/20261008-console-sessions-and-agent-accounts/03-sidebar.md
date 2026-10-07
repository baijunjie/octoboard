# The sidebar's console sessions

> Goal: console sessions are listed and managed in the sidebar the way project sessions are, and a bound project
> session shows which console session owns it.
> Completion criteria: a console with three console sessions lists all three; a new one can be created, selected and
> renamed; each shows its own colour; a project session bound to one carries a badge in that colour whose tooltip
> names it; the archive of console sessions is reached from the section rather than from one session's row. Static
> checks and the test suite pass.

## Technical design

- [ ] The single row that held the console's one console session becomes a **section listing every console session
  of the console that is not archived**, above the project list, ordered by the project's existing ordering rules.
- [ ] The section has an action for **creating a console session**, which opens one with the console's current
  console-session agent and agent config directory.
- [ ] A console session's row offers, at least: selecting it, **Rename**, entering its focus mode, and **Archive**.
  There is no Resume item; selecting an interrupted console session resumes it, as for any session.
- [ ] **Archived console sessions are reached from the section**, not from a row: a submenu of the newest few, each
  selectable, and the way to the archive view of all of them. This is the entry that sat in the wrong place and is
  what the section replaces.
- [ ] The archive view's scope for console sessions is the console's console sessions, titled accordingly.
- [ ] A **bound project session carries a binding badge** in its owner's colour, wherever a project session is
  listed: its sidebar row and its card in a project's focus mode. The badge's tooltip names the owner. An unbound
  session carries no badge.
- [ ] The badge carries no information its tooltip does not: colour alone never distinguishes two owners for a user
  who cannot tell the colours apart.
- [ ] The project focus mode card's "reports to hub" line is replaced by the badge plus the owner's name.
- [ ] A console session's archive, as seen from the archive view, is its own archived bound sessions; a project's
  archive is still that project's archived sessions. They are two filters over the same records.

## Implementation plan

- [ ] Replace the single row with the section, reusing the sidebar's row, ordering and menu building blocks rather
  than new ones.
- [ ] Give the section's menu the archive submenu, and give each row its own menu.
- [ ] Add the binding badge as one component used by both the row and the focus-mode card.
- [ ] Extend the archive view's scope so it can be a console session's bound sessions, a project's sessions, or a
  console's console sessions.
- [ ] Add the strings for every locale, following the project's copy conventions.

## Notes for the developer

**Reusable capabilities**

- The sidebar's rows, action menus, archive submenu, pinning, ordering and inactive-group dimming all exist and are
  what the section is built from.
- The archive view already takes a scope; it is widened rather than duplicated.
- Tooltips and their dismissal behaviour exist as a shared component.

**Development notes**

- A console session carries no project, so it is not subject to the project list's filtering or its inactive-group
  dimming.
- The sidebar follows the selected session; with several console sessions that rule needs no change, but check that
  nothing assumed the console's console session was reachable without searching.

**Reference docs**

- `docs/product/sidebar.md`, `docs/product/sessions.md`, `docs/product/appearance.md`
