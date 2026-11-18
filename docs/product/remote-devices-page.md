# The Remote devices page

The main window of the macOS and Windows applications has a **Remote devices page**, where the remote daemons the
owner has saved are listed, added, changed, removed and connected to. It is a page of the main window as a console's
sessions are, and takes their place while it shows: the sidebar lists the saved remote daemons instead of a console,
and the content area shows the selected one's settings instead of a session. A remote window and a browser have no
such page. What a remote daemon is, what is saved of one and what connecting to one does are in
`docs/product/remote-daemons.md`.

## Opening and leaving the page

The page is opened by:

- the rail's **Remote devices** button, under the consoles (see "The rail" in `docs/product/window-layout.md`). The
  button only opens the page: pressed while the page shows, it does nothing. While the page shows, the button is drawn
  as the current console's tile is and tells assistive technology it is the current item, and no console's avatar is
  marked current. Its tooltip, and its name, are Remote devices, followed while any connection is held by how many
  are connected;
- the **Remote Devices** item of the application's menus (see "Where remote daemons are reached from" in
  `docs/product/remote-daemons.md`), which brings the main window forward, back from the background or from being
  hidden, showing the page;
- a question or a failure of a connection that the page has to show (see "Connecting from the page" and "What is handed
  to the page" below).

**The page is left by going to another place of the window**: choosing a console's avatar on the rail, or anything that
changes the current console or the selected session — a session chosen from a notification, the menu bar icon's menu
or the waiting count — or Back and Forward to a place without the page. `Escape` does not leave it. What the window
showed before is kept as it was while the page shows — the terminals go on with their state, the sidebar keeps its
console's folds and filters — and is there again once the page is left.

**The page is part of the navigation history** (see `docs/product/navigation-history.md`): opening it is a visit, and so
is choosing another device on it. Opening a new device's form is not. A place naming a device that is no longer saved
is passed over.

**Which device the page shows when it opens**: the one last chosen on it, remembered across launches, while it is still
saved, else the first saved one.

**What was changed on the form and not saved is dropped without asking** when another device is chosen, when Add
remote daemon is started, when the page is left, and when a question or failure handed to the page selects another
device. A new device's form being filled in is dropped the same way.

## What the page shows

The top bar keeps **Back**, **Forward** and the sidebar toggle, which reads **Show remote devices** or **Hide remote
devices**. Its breadcrumb is **Remote devices** followed by the shown device's name, or Remote devices alone while a new
device is being added or none is saved. There are no panel toggles and no right pane on the page.

Nothing but the frame is drawn, in the sidebar and in the content area, until the saved remote daemons have been read.

### The list of devices

The sidebar lists the saved remote daemons. It is the sessions' sidebar, docked, resized, hidden, floating and opened as
a drawer as that one is (see `docs/product/pane-docking.md` and `docs/product/sidebar-drawer.md`), and it is there even
with no console at all. To assistive technology it is named Remote devices.

- **Its header** reads **Remote devices**, with a **+** button, Add remote daemon.
- **One row for each saved remote daemon**, in the order they were saved: a server glyph, its name, and where it is
  connected a coloured status glyph. A green chain link means **Connected**, a green browser window means
  **Opened in browser**, and an amber arc means **Reconnecting**. The arc turns once every two seconds when motion is allowed.
  Hovering over the glyph shows the status in words. The glyphs follow connections as they are made, dropped and ended.
  The row of the device the content area shows is marked as the selected one, and choosing a row shows that device.
- **A row's ⋮ menu**, also opened by a right-click on the row, holds **Connect** (**Open** for one that is connected)
  and, after a separator, **Remove**. Connect and Open are unavailable while another device is being connected.
- **With none saved**, one compact empty row says there are no remote daemons yet; pressing it starts Add remote daemon
  (see "The compact row" in `docs/product/empty-states.md`). It stays while a new device's form is open.

While the page shows, resting the mouse on a console's avatar does not float the sidebar in. In a window too narrow
for the sidebar to dock, pressing the Remote devices button inside the open drawer leaves the drawer open, now listing
the devices; choosing a device, or starting Add remote daemon, closes the drawer, as choosing a session does. The
dimmed area behind the drawer is named Close remote devices.

### A saved device

The content area shows the selected device under a header holding its name, as the page's heading, the badge of its
connection where it is connected, its SSH destination as `user@host:port`, read left to right in every language, and
**Connect** (**Open** for one that is connected; see "Connecting from the page" below). The **Reconnecting** badge
pairs the same amber arc with its status text. Under the header is the device's form (see "The form" below), and under
the form, in a card of its own, the **Remove remote daemon** row with **Remove**.

**Remove**, from this row or from the row's menu in the sidebar, asks first, in a confirmation saying the daemon is
removed from the remote daemons and the server and its daemon stay the same; confirmed, it removes the saved daemon and
the device key kept for it (see "Device keys" in `docs/product/remote-daemons.md`). A window open on it stays until it
is closed. The page then shows the first saved device, or, with none left, the empty state.

### Adding a device

**Add remote daemon** — the sidebar header's **+**, the empty row or the empty state's button — shows a blank form in
the content area, headed **Add remote daemon**, with focus in its first field. Its **Save** saves the new device and
then shows it, once the list holds it. **Cancel** returns to the device shown before, or to the empty state. Leaving
the page drops the form.

### With none saved

With no remote daemon saved, the content area shows only the empty state, centred (see "The full panel" in
`docs/product/empty-states.md`): a server glyph, the heading **No remote daemons yet**, as the page's heading, a line
saying that a remote daemon is the daemon on another computer, reached over SSH, and **Add remote daemon** as its
action.

## The form

The form holds the fields of a saved remote daemon, whose meaning and rules are in "Saved remote daemons" in
`docs/product/remote-daemons.md`, in three cards:

- **Connection**: Connection details, with its Paste button (see "Pasting connection details" below), then Name, then
  User, Host and SSH port in one row, which stacks where the content area is too narrow for it. The row mirrors under a
  right-to-left language, as any row of controls does; the fields' text stays left to right.
- **Sign-in**: Authentication, and the key file's path where Key file is chosen.
- **Advanced**: Path of octoboardd, then the pinned host key.

Under the cards are **Save** and **Revert** for a saved device, and **Save** and **Cancel** for a new one.

- **Save** is available whenever the form would save something other than what is saved, and is disabled only while
  nothing is changed or while a save is under way. Two inputs that save the same count as no change: text is saved
  without the spaces around it, a blank name as the host, the port as a number, and a key file's path only where Key
  file is the way to sign in. A save that fails is an error alert titled that the remote daemon could not be saved, and
  leaves the form as it was.
- **Revert** drops what was changed, and is disabled while nothing is or while a save is under way.
- **A field that was not changed follows the saved device** when that changes elsewhere — the host key pinned at a
  connection, the path of `octoboardd` saved from a question — and a changed field the saved device comes to match
  follows it again from then on. A field being edited is never overwritten.

**What is wrong with a field is said under it** (see "Errors under a field" in `docs/product/error-alerts.md`) once the
field was edited and then left, never merely because another control changed: a missing host or user, a port outside 1
to 65535, a missing key file path while Key file is chosen, and text in Connection details that is not connection
details. Pressing Save while anything is wrong saves nothing, says what is wrong under every such field, and moves focus
to the first of them.

### Pasting connection details

At the end of the Connection details field is a **Paste** button, an icon button there only where the application can
read the clipboard. It reads the clipboard only when pressed, and puts the clipboard's text, without the spaces around
it, in place of the field's text, which fills the other fields as text pasted into the field does; text that is not
connection details says so under the field at once. Its tooltip and name, **Paste**, turn to **Pasted** after a paste,
which is also announced, and its icon to a check, going back as a copy button's do (see "Copy buttons" in
`docs/product/labels-and-tooltips.md`). A clipboard with no text — empty, or holding only an image — pastes nothing and
says nothing. A clipboard that cannot be read raises an error alert titled that the connection details could not be
pasted, saying the clipboard could not be read (see "Recovery and unexpected errors" in
`docs/product/error-alerts.md`).

## Connecting from the page

**Connect** connects to the device (see "Connecting" in `docs/product/remote-daemons.md`). One connection is made at a
time from the page: the pressed button shows that it is under way, and every other device's Connect and Open, in the
content area and in the rows' menus, is unavailable meanwhile. **Open** brings forward the window that shows that
daemon, or opens the browser at it again, and asks for nothing. Once connected, the daemon opens in a window of its
own, in front, and the page stays as it was.

What the connection asks or reports — a password, a host key to confirm, a failure — is a dialog over the main window
(see "What a connection asks or reports" in `docs/product/remote-daemons.md`). A connection started from the page goes
on when the page is left, or when the main window's page is loaded again; where it then has something to ask or report,
the page comes back to show it.

## What is handed to the page

A question or a failure of a connection made from a menu's item for a saved daemon (see "Where remote daemons are
reached from" in `docs/product/remote-daemons.md`) is handed to the page: the main window is shown and focused, and
the page shows it, selecting its device. What is handed over waits until the saved remote daemons have been read, no
connection from the page is under way and none of the page's dialogs is open, and is then shown one at a time, in the
order it came; only the latest one of each saved daemon is kept. One for a daemon no longer saved is dropped, and so is
a question for a daemon that has been connected since; a failure is still shown.

## Without the local daemon

Remote connections do not go through the local daemon, so the page works while the main window has nothing from the
local daemon to show: while it connects to the daemon at launch, before anything has been read from it, and while
that first connection is not answered; on the screen for a daemon that could not start; and on the screen for a daemon
of another version, which takes the place of whatever the window showed. The main window then keeps its top bar, with
Back and Forward, and its rail, which holds only the Remote devices button: no consoles, no New console and nothing of
the rail's bottom group, Settings included. The content panel shows the daemon's state where the panes would be (see
"The window chrome and the content panel" in `docs/product/window-layout.md`), with the connection banner as usual
while that first connection is not answered.

**A daemon lost once the window has shown what it read from it does not bring this frame back**: the window keeps the
consoles, sessions and everything else it last read, with the connection banner along its bottom, while it reconnects
(see "Losing the daemon connection" in `docs/product/application-lifecycle.md`).

- **The page shows only when asked for**, by the Remote devices button, the menus' Remote Devices item or a question or
  failure handed to it; the brief connecting at launch never shows it.
- **Back from the page returns to the daemon's state**, whichever of those states it is.
- **A page opened while the local daemon could not be used is left once it can**, and the window shows its consoles.
- **A page opened while the local daemon could be used stays** when the daemon becomes unusable, and again when it
  becomes usable, with the form as it was.

## Keyboard and assistive technology

- **Opening the page moves keyboard focus into it**, once the saved remote daemons have been read: to a new device's
  first field while one is being added, otherwise to the page's first control. It does not while a dialog is open, nor
  where focus is already in the page, nor while the narrow window's drawer is open over it, where focus moves in once
  the drawer closes. A page opened with the mouse shows no focus ring there.
- **The content area is a region of its own** in the `F6` cycle, right after the sidebar (see "Moving focus between
  regions with F6" in `docs/product/moving-focus-between-regions.md`), named by the page's heading: the device's name,
  Add remote daemon, or the empty state's heading. Where the window would hand focus to the terminal, it hands it to
  the page.
- **Each button and menu is named with its daemon**: "Connect to Staging", "Open Staging", "Remove Staging", and the
  row's ⋮ "Actions for remote daemon Staging". A row is named by the daemon's name and, where it is connected, the
  state its glyph shows. Tapping the glyph selects the row.
- **Focus mode's shortcut does nothing** while the page shows (see `docs/product/focus-mode.md`).
