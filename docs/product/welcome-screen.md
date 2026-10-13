# The welcome screen

With **no console at all** — a fresh install, or the last console deleted, by this client or another — the window's
content panel holds a single **welcome screen** in place of its panes, filling the whole area (the panes it replaces
are in `docs/product/window-layout.md`). The thing to do in this state is make a console — by hand, or from a
configuration exported on another machine — and that is all the screen offers.

## What the welcome screen shows

- A glyph and a heading saying there are no consoles yet, in the shape every empty state takes (see "The full panel" in
  `docs/product/empty-states.md`).
- A line explaining what a console is: that it holds the projects the user works in and the sessions they run in them.
- A button, **New console**, which opens the same new-console dialog the rail's New console opens (see "Consoles" in
  `docs/product/consoles-and-projects.md`). A console created here becomes the current one as soon as it appears, as
  one created from the rail does (see "The console switcher" in `docs/product/sidebar.md`).
- Under it, quieter, **Import configuration…**, which opens the import dialog for a configuration document exported on
  another machine (see "In the window" in `docs/product/configuration-export-and-import.md`) — the way a machine with
  nothing on it is set up from one that has. It is there only where the window can open a file, which the macOS
  application and a browser both can; where it cannot, New console stands alone.

**Once every agent has been determined unavailable** on the machine, the screen also carries, under the panel, the
fuller of the two wordings Octoboard has for that: which agents it supports and that one of them has to be on the user's
`PATH`, without saying how to install one, rather than the short row the sidebar's console sessions section has room
for, which says only that no agent was found (see "Opening a session" in `docs/product/sessions.md` and "The console
sessions section and the project list" in `docs/product/sidebar.md`). **New console stays enabled even so**, unlike the
console sessions section's new-session action: a console's two agent fields are only the defaults its sessions start
from, so creating a console starts no agent and nothing about it would be refused.

## What is not on screen with no console

- **No sidebar and no right pane.** Neither is rendered: there is no console for the sidebar to show (see
  `docs/product/sidebar.md`) and nothing to own the right pane (see "What the right pane shows" in
  `docs/product/window-layout.md`). So there is no sidebar to float in on hover either, and no drawer to open below
  1148 px.
- **No terminal.** Neither of the terminal's empty states appears here: the panel saying no session is selected, which
  is what a console with no session selected shows, nor the panel offering **Add project**, which a console with nothing
  in it shows (see "The terminal" and "A console with nothing in it" in `docs/product/sessions.md`).
- **Neither pane toggle in the top bar**: there is no sidebar for the sidebar toggle to control, and the right pane's
  toggle, at the bar's end, is there only while the right pane has an owner (see "The two pane toggles" in
  `docs/product/window-layout.md`). The top bar keeps **Back** and **Forward**, and **both are disabled**: every place
  the window remembers either names a console that no longer exists or is this same empty state, so each is passed
  over (see "Moving back and forward" in `docs/product/navigation-history.md`). There is no breadcrumb, no session
  being selected.
- **On the rail, no waiting count** — no session is waiting. The rail is otherwise unchanged: the console list, empty
  here, then New console, and at the bottom the notifications bell and Settings (see "The rail" in
  `docs/product/window-layout.md`).

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
  `docs/product/moving-focus-between-regions.md`); Import configuration…, where shown, is the next Tab stop.
- Wherever the window would hand keyboard focus to the terminal — the connection banner's Retry going away, a toast
  being dealt with — it hands focus to the New console button instead, and leaves focus on nothing when it is already
  on nothing. This is the rule the terminal itself follows with no session selected and nothing to offer in its place,
  which hands focus to the top bar (see "The terminal" in `docs/product/sessions.md`); here the top bar has no enabled
  control to take it.
- **When the first console is created** from the welcome screen's New console, focus moves to the first control in the
  top bar a keyboard can reach. The hand-off is New console's alone: consoles arriving through Import configuration…
  appear while its dialog is still open on the import's result, and get no such hand-off.
- **When the last console goes away** while keyboard focus is inside the sidebar, the archive view, the terminal or
  the right pane — all of which go with it — focus is handed to the rail's New console button. Focus that was already
  on nothing stays on nothing.
