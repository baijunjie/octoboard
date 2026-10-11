# The welcome screen

With **no console at all** — a fresh install, or the last console deleted, by this client or another — the window's
content panel holds a single **welcome screen** in place of its panes, filling the whole area (the panes it replaces
are in `docs/product/window-layout.md`). The one thing to do in this state is make a console, and that is all the
screen offers.

## What the welcome screen shows

- A glyph and a heading saying there are no consoles yet.
- A line explaining what a console is: that it holds the projects the user works in and the sessions they run in them.
- One button, **New console**, which opens the same new-console dialog the rail's New console opens (see "Consoles" in
  `docs/product/consoles-and-projects.md`). A console created here becomes the current one as soon as it appears, as
  one created from the rail does (see "The console switcher" in `docs/product/sidebar.md`).

**Once every agent has been determined unavailable** on the machine, the screen also carries the line Octoboard shows
wherever an agent is needed and none is installed — which agents it supports and that one of them has to be on the
user's `PATH`, without saying how to install one (see "Opening a session" in `docs/product/sessions.md`). **New console
stays enabled even so**, unlike the console sessions section's new-session action (see "The console sessions section
and the project list" in `docs/product/sidebar.md`): a console's two agent fields are only the defaults its sessions
start from, so creating a console starts no agent and nothing about it would be refused.

## What is not on screen with no console

- **No sidebar and no right pane.** Neither is rendered: there is no console for the sidebar to show (see
  `docs/product/sidebar.md`) and nothing to own the right pane (see "What the right pane shows" in
  `docs/product/window-layout.md`). So there is no sidebar to float in on hover either, and no drawer to open below
  1148 px.
- **No terminal.** The terminal's "select a session" empty state does not appear here; that state is what a console
  with no session selected shows (see "The terminal" in `docs/product/sessions.md`).
- **No sidebar toggle in the top bar**, there being no sidebar for it to control — the same rule the right pane's
  toggle follows, which is on the rail only while the right pane has an owner (see "The two pane toggles" in
  `docs/product/window-layout.md`). The top bar keeps **Back** and **Forward**, and **both are disabled**: every place
  the window remembers either names a console that no longer exists or is this same empty state, so each is passed
  over (see "Moving back and forward" in `docs/product/navigation-history.md`). There is no breadcrumb, no session
  being selected.
- **On the rail, no waiting count and no right pane toggle** — no session is waiting and nothing owns the right pane.
  The rail is otherwise unchanged: the console list, empty here, then New console, and at the bottom the notifications
  bell and Settings (see "The rail" in `docs/product/window-layout.md`).

## The panes' saved preferences are left alone

Whether each pane is hidden and each pane's chosen width (see "Resizing the sidebar and the right pane" and "Hiding
the sidebar and the right pane" in `docs/product/window-layout.md`) are **not touched** by there being no console: no
console is an absence of content, not a choice the user made. So when the first console appears, the panes come back
exactly as the user had left them — a sidebar the user had hidden is still hidden.

## Keyboard focus

- **The welcome screen never takes keyboard focus when it appears**: launching the application leaves focus on nothing
  rather than pulling it onto a control.
- It is **a region of its own** in the `F6` cycle, between the rail and where the sidebar would be, and focus lands on
  its New console button (see "Moving focus between regions with F6" in
  `docs/product/moving-focus-between-regions.md`).
- Wherever the window would hand keyboard focus to the terminal — the connection banner's Retry going away, a toast
  being dealt with — it hands focus to the New console button instead, and leaves focus on nothing when it is already
  on nothing. This is the rule the terminal itself follows with no session selected, which hands focus to the top bar
  (see "The terminal" in `docs/product/sessions.md`); here the top bar has no enabled control to take it.
- **When the first console is created** from the welcome screen, focus moves to the first control in the top bar a
  keyboard can reach.
- **When the last console goes away** while keyboard focus is inside the sidebar, the archive view, the terminal or
  the right pane — all of which go with it — focus is handed to the rail's New console button. Focus that was already
  on nothing stays on nothing.
