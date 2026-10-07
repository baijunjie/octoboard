# The two focus modes

> Goal: focus mode can be entered for a console session as well as for a project, and each of the two shows only the
> sessions that belong in it.
> Completion criteria: entering a console session's focus mode shows only the projects and sessions bound to it, and
> a session created there is bound to it with no choice offered; entering a project's focus mode lists only that
> project's unbound sessions and says how many others are running in the project and whose they are, with that line
> leading to the console session; the keyboard shortcut enters and leaves both. Static checks and the test suite pass.

## Technical design

- [ ] Focus mode's target becomes either a project or a console session, instead of a project only. It is still
  remembered per client together with the console shown.

**A console session's focus mode**

- [ ] Shows only the projects that have a session bound to this console session, and within each, only those
  sessions. Sessions bound to another console session, and unbound sessions, are not shown.
- [ ] Its header names the console session, with its colour, and offers its own actions — rename and archive — and
  leaving focus mode.
- [ ] The user may create a session here, and it is **bound to this console session with no choice offered**: the
  dialog shows the owner as fixed rather than as a field.
- [ ] Actions that have no meaning in this view are suppressed — a project cannot be entered into its own focus mode
  from here, and the project list's filtering is not offered.
- [ ] Its archived list and its archive view are this console session's archived bound sessions.

**A project's focus mode**

- [ ] Lists only the project's **unbound** sessions. A bound session is not listed, and carries no badge here because
  none appears.
- [ ] Carries a single line saying how many sessions are running in this project for which console sessions, naming
  them. Following it enters that console session's focus mode. With none, the line is absent.
- [ ] Creating a session here offers **no owner choice and always creates an unbound session**.
- [ ] Its archived list and its archive view are the project's archived sessions, bound ones included — the archive is
  not filtered by binding, only the live list is.

**The shortcut**

- [ ] Outside focus mode the shortcut enters focus mode for the selected session's context: a project session's
  project, or a console session itself — where today it does nothing for a console session.
- [ ] In either focus mode it leaves focus mode. Its other rules — ignored while a dialog or menu is open, during
  composition, and never reaching the terminal — are unchanged.

## Implementation plan

- [ ] Widen the stored and remembered focus target to the two kinds, and the sidebar's view switch with it.
- [ ] Build the console session's focus view from the existing focus view's building blocks — header, session cards,
  archived list — rather than a second implementation.
- [ ] Filter the project focus view's live session list by the absence of a binding, and add the summary line.
- [ ] Have the new-session dialog take the owner as fixed, as absent, or as a choice, according to where it was
  opened from.
- [ ] Extend the shortcut's entry rule, and the rules for where keyboard focus lands on entering and leaving.
- [ ] Add the strings for every locale, following the project's copy conventions.

## Notes for the developer

**Reusable capabilities**

- The project's focus view already has the header, the session cards, the archived list, the view-change animation,
  the per-client memory of the mode, and the shortcut; the console session's focus mode is the same machinery with a
  different target.
- The breadcrumb in the top bar already names what the view is scoped to.

**Development notes**

- Milestone 7 names a session's account wherever its agent is named rather than drawn as an icon, and a focus mode's
  session cards are one such place. The console session's focus view is a third surface of that kind and has to name
  the account on its cards too.
- Focus mode is left by switching console, by selecting a session that does not belong to the focused thing, and when
  the focused thing stops existing; both kinds of target need all three.
- Animation is suppressed where the system asks for reduced motion.

**Reference docs**

- `docs/product/sidebar.md` for focus mode, the archive view and the ordering rules
- `docs/product/window-layout.md` for the breadcrumb and the region cycle
