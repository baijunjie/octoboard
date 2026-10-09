# Report panel

The **report panel** is what the window's right pane shows while a **console session** owns it, to the right of the
terminal (to its left under a right-to-left language, see "Right-to-left layout" in `docs/product/window-layout.md`).
It belongs to the console session rather than to its console: each console session has its own pages, and the panel
shows those of the selected console session, so two console sessions in one console do not share a history. Selecting
a console session puts its report panel in the right pane; when the right pane shows something else instead — a
project pane, or nothing — is in "What the right pane shows" in `docs/product/window-layout.md`. How wide the pane is,
how the user resizes and hides it, how a hidden one floats in, and what becomes of it in a window too narrow for three
panes, is described in `docs/product/window-layout.md`.

A page is a whole HTML document the console session wrote, for anything better shown than typed
into the terminal — a table, a comparison, a set of choices. A page may carry a form, and what the
user submits comes back into the console session as a message.

## Pushing a page

The console session pushes a page with its `show_page` tool (see "The console session's tools" in
`docs/product/hub-orchestration.md`). The call answers with the new page's **id**, which is what a
later form submission names, so a console session that has pushed several pages can tell which one a
submission came from. An `html` argument that is empty or only whitespace is refused.

Pushing a page keeps it and moves the panel to it. Nothing else in Octoboard shows a page and nothing
announces one: a page pushed while the user was looking at another session is simply there in the
panel when they come back to the console session.

## Paging through the history

**Every page ever pushed is kept.** Pages belong to the console session that pushed them and are deleted with it,
and so with its console, which deletes its console sessions (see "Deleting a console" in
`docs/product/consoles-and-projects.md`). Nothing else deletes a page, and the console session cannot withdraw one it
has pushed.

The panel's bar carries, left to right (mirrored under a right-to-left language): ◀ and ▶ to step one page back
and one forward, the position as "current / total" counted from 1 with the oldest page first, the page's creation
time formatted for the UI's current language (see "What follows the language" in `docs/product/language.md`), and — on
any page but the newest — a "Read-only" badge. The position, the total and the badge all count the selected console
session's pages only. The place the user has paged to is not kept: whenever the report panel gives way in the right
pane — another session selected, or Browse files putting a project pane in its place — coming back to it shows its
newest page again, started over from its markup, with anything typed into its form and not submitted gone.

- The panel follows the newest page: a page pushed while the user is on the newest one moves the view
  to it.
- A page pushed while the user is reading **history** does not move them. The view stays on the page
  they paged to, and only the total grows.
- Paging away from a page discards it: paging back to it starts the page over from its markup, and whatever had
  been typed into its form is gone.
- A console session that has never pushed a page reads "No pages yet."

## History pages are read-only

Only a console session's **newest** page can be submitted from. On every older page:

- the page's form controls are disabled, and submitting its form sends nothing;
- the "Read-only" badge is shown in the panel's bar.

The rule does not depend on the panel: the daemon refuses any submission naming a page that is not
its console session's newest, whatever that page's own markup does.

## What a page may contain, and what it cannot do

A page is a **static, self-contained HTML document**. None of its own scripts run, nothing external
loads, and the only way data leaves it is a form the user submits (see "Submitting a form back to the
console session" below).

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

## Submitting a form back to the console session

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

The submission is written into the console session as a **user message**: a line naming
the page it came from, then one `key: value` line per field. A form with no named fields is still
delivered, with `{}` in place of the field lines.

Delivery takes the same path as every other write into a running session (see "Messages held until a
session can take them" in `docs/product/hub-orchestration.md`): written straight away while the
console session is working or awaiting instructions, and **queued** while the console session is
waiting for the user — queued rather than refused, because the user is the one submitting and there
is nobody to ask to answer the console session's prompt first.

The submission goes to the console session that pushed the page, found from the page itself, whichever console
session the user happens to be looking at. It fails, and the failure is shown to the user, when the page is no longer
its console session's newest or the console session's process is not running. A failed submission changes nothing. The
message names which console session the submission was meant for, so that it cannot be
read as being about whichever session the user happens to be looking at.

## Escape and F6 inside a page

Pressing Escape while focus is inside a page also tells the window so, on a history page too. The
key never reaches the window from inside the frame, so without this it could not close the floating
right pane or the narrow-window drawer. It closes whatever overlay is open, as Escape does
anywhere else, and with nothing open it does nothing. An Escape pressed during an input
method's composition, where it cancels the composition, is not relayed. Inside the page the key keeps
its usual effect either way, such as closing an open drop-down list.

`F6` and `Shift+F6` pressed while focus is inside a page are likewise passed to the window, on a
history page too, and move focus to the next or previous region of the window (see "Moving focus
between regions with F6" in `docs/product/moving-focus-between-regions.md`); the browser's own handling
of the key inside the frame is cancelled. The signal is acted on only while focus is inside the page's frame.
Neither this signal nor Escape's carries anything from the page.

## Window shortcuts inside a page

In the macOS application, two of the window's keyboard shortcuts are passed to the window when pressed while focus is
inside a page, on a history page too, and are kept from the page itself:

- `Cmd+[` and `Cmd+]`, with no other modifier held, go back and forward (see "Keyboard shortcuts" in
  `docs/product/navigation-history.md`).
- `Ctrl+Tab` and `Ctrl+Shift+Tab`, with neither `Cmd` nor `Option` held, move to the next or previous console session
  (see "The switch strip" in `docs/product/focus-mode.md`). The page never gets them, even when the window has
  nowhere to move at that moment; a key held down is passed on once.

As with Escape and `F6`, neither is passed on during an input method's composition, the signal is acted on only while
focus is inside the page's frame, and it carries nothing from the page. In a browser neither is passed on, and the
page keeps the browser's own handling of the keys.

## Right-click inside a page

A page follows the window's rule (see "Right-click menus" in `docs/product/window-layout.md`): in the desktop app a
right-click on a page shows nothing, except in its input fields and text areas, which keep the system's menu for copy
and paste; in a browser the frame keeps the browser's own menu.
