# Report panel

The **report panel** is the third pane of the window, to the right of the terminal. It belongs to a
**console** rather than to a session: its pages are pushed by that console's hub session, and the
panel is on screen only while the selected session is a hub session. Selecting a project session
leaves the terminal to fill the pane on its own. How wide the panel is, and what becomes of it in a
window too narrow for three panes, is described in `docs/product/window-layout.md`.

A page is a whole HTML document the hub wrote, for anything better shown than typed into the
terminal — a table, a comparison, a set of choices. A page may carry a form, and what the user
submits comes back into the hub session as a message.

## Pushing a page

The hub pushes a page with its `show_page` tool (see "The hub's tools" in
`docs/product/hub-orchestration.md`). The call answers with the new page's **id**, which is what a
later form submission names, so a hub that has pushed several pages can tell which one a submission
came from. An `html` argument that is empty or only whitespace is refused.

Pushing a page keeps it and moves the panel to it. Nothing else in Octoboard shows a page and nothing
announces one: a page pushed while the user was looking at another session is simply there in the
panel when they come back to the hub.

## Paging through the history

**Every page ever pushed is kept.** Pages belong to the console and are deleted with it (see
"Deleting a console" in `docs/product/consoles-and-projects.md`). Nothing else deletes a page, and
the hub cannot withdraw one it has pushed.

The panel's bar carries, left to right: ◀ and ▶ to step one page back and one forward, the position
as "current / total" counted from 1 with the oldest page first, the page's creation time in the
user's own locale format, and — on any page but the newest — a "Read-only" badge.

- The panel follows the newest page: a page pushed while the user is on the newest one moves the view
  to it.
- A page pushed while the user is reading **history** does not move them. The view stays on the page
  they paged to, and only the total grows.
- Paging away from a page stops it: its scripts do not keep running in the background, and paging
  back to it starts the page over from its markup.
- A console that has never had a page pushed reads "No pages yet."

## History pages are read-only

Only a console's **newest** page can be submitted from. On every older page:

- the page's form controls are disabled, including controls the page's own script adds later;
- `octoboard.submit` throws instead of sending anything;
- the "Read-only" badge is shown in the panel's bar.

The rule does not depend on the panel: the daemon refuses any submission naming a page that is not
the console's newest, whatever that page's own markup does.

## What a page may contain, and what it cannot do

A page is a **self-contained** HTML document. Inline `<style>` and inline `<script>` run, and an
image has to be a `data:` URL. **A page cannot load anything external**: no subresources of any kind,
no external scripts, stylesheets or fonts — so a page has to stick to generic font families — and no
`fetch`, `XMLHttpRequest`, WebSocket or `sendBeacon`. Native form submission does not navigate
anywhere, and a page cannot navigate its own frame to an external URL either. The one way Octoboard
provides for data to leave a page is `octoboard.submit`.

**Known gap: a page can still reach the network through two routes** that no content security policy
directive covers in WebKit. Both were measured in the macOS application:

- **WebRTC.** `RTCPeerConnection` is available inside the page, and a page that uses it makes the
  application send STUN Binding and TURN Allocate requests over UDP to an address the page chose.
- **`<link rel="preconnect">`.** It opens a TCP connection to a host and port the page chose.

The destination is the page's to pick, so either route lets a page carry out what it computed or what
the user typed into it, encoded in the address it contacts. Whether link-based DNS prefetching issues
lookups as well was not observed.

Apart from those routes the outbound channel is closed; that is not a claim that a page can do
nothing. A page is model-authored HTML and its script runs, inside Octoboard's own window, over
whatever the user puts into it.

Each page is rendered in a sandboxed frame of its own, with no access to the application around it or
to any other page, and on a light surface whatever the window's appearance is — see "A report page
stays on a light surface in both appearances" in `docs/product/appearance.md`.

## Submitting a form back to the hub

A page is given exactly one bridge call:

```js
octoboard.submit(data); // `data` is any JSON-serializable value
```

Nothing gates when it may be called: any call from the newest page reaches the hub, whether or not
the user triggered it.

What `submit` was called with is written into the console's hub session as a **user message**, naming
the page it came from and then the data. An object of plain values is rendered as one `key: value`
line per field, **in the order the page sent them** rather than sorted. Anything else — a nested
object, an array, a bare value — is written as pretty-printed JSON instead.

Delivery takes the same path as every other write into a running session (see "Messages held until a
session can take them" in `docs/product/hub-orchestration.md`): written straight away while the hub
is working or awaiting instructions, and **queued** while the hub is waiting for the user — queued
rather than refused, because the user is the one submitting and there is nobody to ask to answer the
hub's prompt first.

A submission fails, and the failure is shown to the user, when the page is no longer the console's
newest or the hub's process is not running. A failed submission changes nothing. The message names
which console's hub the submission was meant for, so that it cannot be read as being about whichever
session the user happens to be looking at.
