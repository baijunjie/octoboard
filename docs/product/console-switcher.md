# The console switcher

This is how the console the sidebar shows (see `docs/product/sidebar.md`) is picked.

The consoles are switched from the top of the rail, at the window's left edge (see "The rail" in
`docs/product/window-layout.md`): one avatar per console (see "Avatar" in `docs/product/consoles.md`), in
the order the consoles were created, followed by **New console**. The console the sidebar shows is the **current
console**; its avatar has a tile behind it and is announced as the current one. (A sidebar floating in to preview
another console is the one exception, see "Previewing a console from the rail" below.) Pressing another avatar makes
that console the current one, its view fading in (see "How the sidebar's view changes" in `docs/product/sidebar.md`);
pressing it with the mouse leaves keyboard focus where it was. A console created from this window — by the rail's New
console or the welcome screen's (see `docs/product/welcome-screen.md`) — becomes the current one as soon as it appears;
one created from another client is only added to the rail.

An avatar carries a **14 px circular badge** at its bottom end corner for one session chosen from all the console's
sessions, its console sessions included. The status priority is **waiting for the user, awaiting instructions, working
in background, then working**, with the most recently started session breaking a tie. Pinning, project grouping and
the sidebar's choice to order by status or date do not affect that choice. Starting, interrupted and archived sessions
are excluded, so a console with only starting sessions has no badge. Session rows still place starting alongside
awaiting instructions (see "Order of projects and sessions" in `docs/product/order-of-projects-and-sessions.md`).

| Chosen session's status | Badge |
|---|---|
| Waiting for the user | A contrasting waving hand on a yellow disc |
| Awaiting instructions | A contrasting speech bubble on a green disc |
| Working in background | A green disc with a contrasting hollow centre, 8 px across |
| Working | A blue disc with a contrasting solid centre, 6 px across |

If the chosen session is switching account, the badge instead shows a turning arc on a neutral disc, and its status
text says it is switching account. The choice still goes by the session's underlying status, as session ordering does
(see "Session statuses" in `docs/product/session-statuses.md`).

Only the centre of a working or working-in-background badge breathes: its opacity eases from 100% to 90% and back over
2.4 seconds, repeating while the disc and centre keep their size. There is no outward ripple. Where the system asks
for reduced motion, the centres remain fully opaque, the hand does not wave and the switching arc does not turn.

The badge is decorative and has no tooltip or accessible name of its own. **The avatar's tooltip and accessible name
are the same text**: the console's name joined with the chosen session's exact localized status wording, or the bare
console name when no session qualifies. The joining punctuation or words follow the chosen language and may vary with
the runtime's locale data. When that tooltip opens, and why a mouse user with the docked sidebar hidden does not get
to read it, is in "Tooltips on icon-only controls" in `docs/product/labels-and-tooltips.md`.

**Right-clicking** (or Control-clicking) an avatar opens that console's action menu at the pointer, the same menu as
the sidebar's header carries (see "The console header" in `docs/product/sidebar.md`). It does not switch to the console.

**The current console is remembered per client**, in that client's own browser storage, as the panes' widths are
(see "Resizing the sidebar and the right pane" in `docs/product/window-layout.md`). A remembered console that no
longer exists falls back to the first console. Switching console switches the terminal's area and the right pane with
it (see "What switching console shows" below). Selecting a session anywhere switches the sidebar to that session's
console (see "Selecting a session" in `docs/product/sidebar.md`).

## What switching console shows

Making another console the current one — pressing its avatar, a press inside the floating sidebar while it previews
that console (see "Previewing a console from the rail" below), or a console created from this window appearing —
switches the whole window to that console, not only the sidebar:

- **The terminal's area shows the session last selected in that console**, the way Back and Forward show a place: an
  interrupted session is shown without being resumed, and an archived one is shown still archived, not reopened (see
  "Selecting a session" in `docs/product/sidebar.md` for what selecting does instead). With no session remembered for
  the console, or with the remembered one no longer existing, nothing is selected, and the terminal's area shows its
  empty state for that console (see "With no session selected" in `docs/product/session-terminal.md`); a console just
  created always shows that.
- **The right pane goes to that session's own owner** — a console session with its report panel, a project session's
  project with its Files panel — and with nothing selected there is no right pane (see "What the right pane shows" in
  `docs/product/window-layout.md`).
- **Focus mode is left**, and an **open archive view closes**.

**The session last selected in each console is remembered per window, in memory only**: whichever way the session
was selected (a row, a menu, Back and Forward, a session this window opened, and so on), it becomes the one shown the
next time its console is switched to. It is not stored, so reloading the window or restarting the application forgets
it, and every client has its own.

Selecting a session of another console — from a menu, the rail's waiting count or the menu bar icon's menu — also
makes its console the current one, but shows **the session chosen**, not the one remembered for that console; Back and
Forward moving to a place in another console likewise show that place's own session, or none.

## Previewing a console from the rail

While the docked sidebar is hidden, at 1168 px and wider, resting the mouse on a console's avatar floats the sidebar
in showing **that** console — when it comes up, how it looks and when it goes away are in "A hidden pane floats in on
hover" in `docs/product/window-layout.md`. The sidebar's toggle in the top bar floats it in as well, but always
showing the **current** console, so that way in is never a preview: a pointer moving from an avatar onto the toggle
puts the floating card back on the current console. For any console other than the current one, resting on its avatar
is a **preview**:

- It shows the console's own view — the header, its console sessions and its projects — never a focus mode, which
  belongs to the current console only. Resting on the current console's avatar shows the sidebar as it is, focus mode
  included. Moving from one avatar to another fades the new console's view in, as switching console does.
- **Hovering changes nothing**: the current console, its tile on the rail, the remembered console and the window's
  history all stay as they were.
- **A press anywhere inside the floating sidebar**, with any mouse button, makes the previewed console the current
  one, and the press then goes on to do what it does there — select a row, open a menu, start a dialog. Pressing the
  avatar itself makes it current too, as always. Such a press closes an open archive view at once, but the terminal's
  area and the right pane change only **once the press has ended**, and to what the press chose itself where it
  chose something: a session the press selected is shown, with its own pane; otherwise the console's remembered
  session is shown as described in "What switching console shows" above. A press that only gave the right pane to
  something, as Open panel in a project's action menu does, still brings the remembered session into the terminal's
  area, and the right pane keeps what the press gave it. In the window's history the console change together with the press's selection, or
  with the remembered session shown at its end, counts as one visit (see "What counts as a place" in
  `docs/product/navigation-history.md`).
- Showing the sidebar for good with its toggle while it previews a console shows the current console, not the one the
  card was previewing. The card goes away and the column eases open; see "Hiding the sidebar"
  and "A hidden pane floats in on hover" in `docs/product/window-layout.md`.
