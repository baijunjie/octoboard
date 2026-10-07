# Report panel

The **report panel** is the third pane of the window, to the right of the terminal (to its left under a right-to-left
language, see "Right-to-left layout" in `docs/product/window-layout.md`). It belongs to a
**console** rather than to a session: its pages are pushed by that console's hub session, and the
panel is on screen only while the selected session is a hub session. Selecting a project session
leaves the terminal to fill the pane on its own. How wide the panel is, how the user resizes and hides it, how a
hidden one floats in, and what becomes of it in a window too narrow for three panes, is described in
`docs/product/window-layout.md`.

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

The panel's bar carries, left to right (mirrored under a right-to-left language): ◀ and ▶ to step one page back
and one forward, the position as "current / total" counted from 1 with the oldest page first, the page's creation
time formatted for the UI's current language (see "What follows the language" in `docs/product/language.md`), and — on
any page but the newest — a "Read-only" badge.

- The panel follows the newest page: a page pushed while the user is on the newest one moves the view
  to it.
- A page pushed while the user is reading **history** does not move them. The view stays on the page
  they paged to, and only the total grows.
- Paging away from a page discards it: paging back to it starts the page over from its markup, and whatever had
  been typed into its form is gone.
- A console that has never had a page pushed reads "No pages yet."

## History pages are read-only

Only a console's **newest** page can be submitted from. On every older page:

- the page's form controls are disabled, and submitting its form sends nothing;
- the "Read-only" badge is shown in the panel's bar.

The rule does not depend on the panel: the daemon refuses any submission naming a page that is not
the console's newest, whatever that page's own markup does.

## What a page may contain, and what it cannot do

A page is a **static, self-contained HTML document**. None of its own scripts run, nothing external
loads, and the only way data leaves it is a form the user submits (see "Submitting a form back to the
hub" below).

Before a page is shown, its HTML is cleaned inside the page's frame. Removed, silently — the push is
not refused, and the rest of the page is shown without them:

- scripts of every kind: `<script>` elements, event-handler attributes (`onclick` and the like) and
  `javascript:` URLs;
- elements that load or embed something: `<link>`, `<iframe>`, `<frame>`, `<object>`, `<embed>`,
  `<meta>`, `<base>`, `<audio>`, `<video>`, `<source>` and `<track>`; `<noscript>` and `<template>` go
  as well;
- a form's `action`, `method`, `enctype` and `target`, and a button's `formaction`; `ping`, `srcset`
  and `sizes`;
- any URL with a scheme — `http:`, `https:`, `mailto:`, `data:` and the like — and any scheme-relative
  `//host` URL, wherever a URL is given (`href`, `src` and the like). The one exception is a `data:`
  URL as an image's source.

What stays: the document's structure and text, `<style>` elements and inline `style` attributes,
inline SVG, images given as `data:` URLs (`<img>`, and `<image>` inside SVG), native form controls
with their `required` and `pattern` validation, links to anchors within the page (`#…`), and the
page's own `lang` and `dir`. Stylesheets and fonts cannot be loaded, so a page has to stick to generic
font families. If what remains after cleaning still contains anything from the removed list, the
page is not rendered at all and the panel shows an empty page.

**What a page cannot do, as a consequence:** react to what the user does — no live calculation, no
showing or hiding parts depending on input, no client-side sorting or filtering, no chart drawn by
script (a chart has to be static, for example inline SVG) — submit by itself, or send anything but
the flat text fields of a form. A form never navigates the frame, and a page cannot navigate its own
frame to an external URL either.

**A page has no route to the network.** No `fetch`, `XMLHttpRequest`, WebSocket, `sendBeacon` or
WebRTC, since no script of the page's own runs, and no connection opened or name looked up ahead of
time by `<link rel="preconnect">` or `rel="dns-prefetch"`, since no `<link>` is kept. In the macOS
application the window adds a second barrier underneath: it blocks every `http` and `https` request
its web view would make — the application's own window loads nothing over the web — so a link or
load the cleaning missed still goes nowhere. If that block cannot be set up at launch, the window
opens without it and pages rely on the cleaning alone.

Each page is rendered in a sandboxed frame of its own, with no access to the application around it or
to any other page, and on a light surface whatever the window's appearance is — see "A report page
stays on a light surface in both appearances" in `docs/product/appearance.md`. It also keeps its own
language and writing direction whatever the UI's language is — see "What follows the language" in
`docs/product/language.md`.

## Submitting a form back to the hub

A page sends data back **only through a native HTML form**, and only when the user submits it —
pressing a submit button, or pressing Enter in a field where the browser submits the form. Native
validation runs first: a `required` field left empty or a value not matching its `pattern` stops the
submission. Nothing on a page can submit on its own.

A submission carries the form's fields in document order, the way a browser builds a form
submission: each field is named by its `name` attribute, so a field without one is not sent, and
neither is a disabled field or an unchecked checkbox or radio button. The submit button the user
pressed is included as one more field when it has a `name` and `value`, which is how a set of choices
is offered: `<button name="choice" value="…">`. Several fields sharing one name — a group of
checkboxes, say — become one field whose values are joined with ", ". A file field sends only the
chosen file's name, never its contents.

The submission is written into the console's hub session as a **user message**: a line naming the
page it came from, then one `key: value` line per field. A form with no named fields is still
delivered, with `{}` in place of the field lines.

Delivery takes the same path as every other write into a running session (see "Messages held until a
session can take them" in `docs/product/hub-orchestration.md`): written straight away while the hub
is working or awaiting instructions, and **queued** while the hub is waiting for the user — queued
rather than refused, because the user is the one submitting and there is nobody to ask to answer the
hub's prompt first.

A submission fails, and the failure is shown to the user, when the page is no longer the console's
newest or the hub's process is not running. A failed submission changes nothing. The message names
which console's hub the submission was meant for, so that it cannot be read as being about whichever
session the user happens to be looking at.

## Escape and F6 inside a page

Pressing Escape while focus is inside a page also tells the window so, on a history page too. The
key never reaches the window from inside the frame, so without this it could not close the floating
report panel or the narrow-window drawer. It closes whatever overlay is open, as Escape does
anywhere else, and with nothing open it does nothing. An Escape pressed during an input
method's composition, where it cancels the composition, is not relayed. Inside the page the key keeps
its usual effect either way, such as closing an open drop-down list.

`F6` and `Shift+F6` pressed while focus is inside a page are likewise passed to the window, on a
history page too, and move focus to the next or previous region of the window (see "Moving focus
between regions with F6" in `docs/product/window-layout.md`); the browser's own handling of the key
inside the frame is cancelled. The signal is acted on only while focus is inside the page's frame.
Neither this signal nor Escape's carries anything from the page.
