# Report panel

The **report panel** is the right pane's Report panel, to the right of the terminal (to its left under a right-to-left
language, see `docs/product/right-to-left-layout.md`): a card like any other in the right pane, with its own toggle in
the top bar (see `docs/product/right-pane-panels.md`), showing the pages a session pushed. Two kinds of session push
pages: a **console session** and a **project coordinator**, whether independent or reporting to a parent (see
`docs/product/hub-orchestration.md`). A **worker** has no report panel tools; becoming a worker withdraws the session's
pages (see "When a session becomes a worker" below).

**Pages belong to the session that pushed them**, not to its console or project: each session has its own pages, and
the panel shows those of the selected session only, so two sessions never share a history.

- **A console session**: the report panel heads its right pane, above its Processes panel (see
  `docs/product/processes-panel.md`). Selecting the console session gives it the right pane, where the report panel
  shows while it is open.
- **A project session**: its Report panel sits in its project's right pane, above the project's Files, Processes and
  Terminal panels, which stay the project's own and are shared by all its sessions. It is offered only while the
  selected project session has pages, or listing them failed (see "A project session's Report panel" in
  `docs/product/right-pane-panels.md`).

When the right pane shows something else instead — a project's Files panel in a console session's place, or nothing —
is in "What the right pane shows" in `docs/product/window-layout.md`. How the panel is opened, closed and floated in
is in `docs/product/right-pane-panels.md`, and how wide the pane is and how the user resizes it in
`docs/product/window-layout.md`.

A page is a whole HTML document the session wrote, for anything better shown than typed into the terminal — a table, a
comparison, a set of choices. A page may carry a form, and what the user submits comes back into the session as a
message.

## Pushing a page

The session pushes a page with its `show_page` tool (see "The console session's tools" and "Project coordinators' tools"
in `docs/product/hub-orchestration.md`). The call answers with the new page's **id**, which is what a later form
submission names, so a session that has pushed several pages can tell which one a submission came from. An `html`
argument that is empty or only whitespace is refused.

A push is also refused, and leaves no page, when the session is a worker by the time the page is
stored — a binding made while the call was under way. A page withdrawn while it is being pushed — by such a binding,
or by a `delete_pages` of the same session's — is refused too, rather than its id handed out, and every window is told
it is deleted, so no panel keeps it.

Pushing a page keeps it and moves the panel to it. Nothing else in Octoboard shows a page and nothing announces one: a
page pushed while the user was looking at another session is simply there in the panel when they come back to the
session that pushed it.

## Reading back the pages pushed

The session reads back the pages it has pushed with its `list_pages` tool, offered to the same sessions as `show_page`.
It reads only the caller's own pages, the same ones its report panel pages through: another session's pages are no more
readable to it than they are in that session's panel.

- Without arguments it answers with `pages`, every page the caller pushed, oldest first, each as `page` (its id, as
  `show_page` returned it), `created_at` (milliseconds since the epoch), `html_bytes` (the size of its HTML in bytes)
  and `title` (the text of its `<title>`, with runs of whitespace collapsed to one space; `null` when it has none or
  only a blank one). The HTML itself is left out.
- With `page`, a page's id, it answers with that one page in full: `page`, `created_at`, `title` and `html`. An id
  that is not one of the caller's pages — another session's included — is refused, the same way as an id that names
  no page, so the answer does not reveal that the page exists.

It is how a session finds the id of an earlier page, which tells a form submission from that page apart from one from
another, and builds on what an earlier page showed.

## Withdrawing pages

The session withdraws pages it has pushed with its `delete_pages` tool, offered to the same sessions as `show_page`.
Deleting is for good: there is no undo.

- It takes exactly one of `pages`, a non-empty list of page ids (as `show_page` and `list_pages` give them), or
  `all: true`, every page the caller pushed. Neither, both, an empty list or a list that is not of ids is refused.
- **Only the caller's own pages can be deleted, and a call is all or nothing.** A named id that is not one of the
  caller's pages — another session's, one already deleted, one that never was — is refused, the same way whichever it
  is, so the refusal does not reveal another session's page, and then nothing is deleted, the other ids named
  included. An id named twice counts once.
- It answers with `deleted`, the ids it removed: an empty list when `all` found no page. Of two calls deleting the same
  page at once, only one reports it.
- Deleting the newest page makes the one before it the newest, the one a form can be submitted from (see "History
  pages are read-only" below).

The panel follows a deletion at once, in every window: the deleted pages leave the pager, and the total shrinks. A
view on a page that was deleted moves to the newest page; one on a history page that is still there stays on it. When
the last page goes, a console session's panel shows its empty state (see "Paging through the history" below), while a
project session's Report panel goes, its toggle with it (see "A project session's Report panel" in
`docs/product/right-pane-panels.md`).

## When a session becomes a worker

A session that pushed pages can come to have the worker role in two ways: an independent project session binding
itself to a project session with `bind_to_session`, and a coordinator with a parent handed to a project session with
`transfer_session` (see `docs/product/changing-session-owners.md`). Either takes `show_page`, `list_pages` and
`delete_pages` away from it, and **every page it pushed is withdrawn at once**, as `delete_pages` with `all: true`
withdraws them: the panel follows in every window as in "Withdrawing pages" above, so no form is left offered that the
session could no longer read the submission of. Nothing brings the pages back, a later release included.

- **Keeping the coordinator role keeps its pages.** Setting a console session as parent, or transferring a
  coordinator to another console session, keeps the report panel tools and pages.
- **The agent is told.** The descriptions of `show_page`, `bind_to_session` and `transfer_session`, and what a project
  session's role description says of the latter two, say beforehand that such a binding withdraws the pages, and the
  binding's own word to the session — the `bind_to_session` result, the message a handover writes into it, or the notice
  at its next launch (see "A change of owner told at the next launch" in `docs/product/changing-session-owners.md`) —
  says afterwards that its pages were withdrawn with the tools for them.
- **Pages a withdrawal missed go when Octoboard starts.** Each time the daemon starts, it withdraws every page still
  held by a session with the worker role — one whose withdrawal failed, say — and the windows are told as for
  any withdrawal. Until then a form submitted from such a page is refused (see "Submitting a form back to the session"
  below).

## Paging through the history

**Every page pushed is kept** until the session that pushed it withdraws it (see "Withdrawing pages" above), becomes a
worker (see "When a session becomes a worker" above), or is deleted itself: a page is deleted
with its session, and so with the session's project or console, which delete their sessions (see "Deleting archived
sessions" and "Deleting an unused session" in `docs/product/deleting-sessions.md`, `docs/product/removing-projects.md`
and "Deleting a console" in `docs/product/consoles.md`). Nothing else deletes a page.

The panel's header (see "A panel's header" in `docs/product/right-pane-panels.md`) carries, after its title, left to
right (mirrored under a right-to-left language, except for what `docs/product/right-to-left-layout.md` lists as keeping
its order): ◀ and ▶ to step one page back and one forward, and between them the position as "current / total", counted
from 1 with the oldest page first. A thin row under the header carries the page's creation time formatted for the UI's
current language (see "What follows the language" in `docs/product/language.md`) and — on any page but the newest — a
"Read-only" badge. The position, the total and the badge all count the selected session's pages only. The place
the user has paged to is not kept: whenever the report panel gives way in the right pane — another session selected, or
Open panel in a project's action menu putting a project's panel in its place — coming back to it shows its newest page
again, started over from its markup, with anything typed into its form and not submitted gone.

- The panel follows the newest page: a page pushed while the user is on the newest one moves the view
  to it.
- A page pushed while the user is reading **history** does not move them. The view stays on the page
  they paged to, and only the total grows.
- Paging away from a page discards it: paging back to it starts the page over from its markup, and whatever had
  been typed into its form is gone.
- A console session with no pages — none pushed yet, or all withdrawn — shows, under the panel's header, an empty panel
  filling the card, saying there are no pages yet (see "The full panel" in `docs/product/empty-states.md`). While the
  console session's pages are first being listed, the card shows its header alone; keyboard focus on a header control
  stays there when the listing arrives. A project session's Report panel never shows this empty state: it is not offered
  while the session has no pages.
- The pages are listed when the session's panel is shown — for a project session, as soon as it is selected, to know
  whether its Report panel is offered — and again whenever the window reloads its state
  from the daemon (a reconnect, or catching up after falling behind). **A listing that fails** is shown in the panel,
  with the reason and **Try again**, which lists them again, and opens no error alert. With no pages to show, an
  empty-state failure under the header fills the card; it is announced to assistive technology, a repeated one again
  (see "Failures shown on the screen" in `docs/product/error-alerts.md`). With pages already there, the page stays
  usable and the failure is an amber warning mark right after the panel's title, named that the pages could not be
  listed; its popover holds what happened, what to do, any details and, where trying again can help, **Try again** (see
  "Failures, warnings and notes marked at their point" in `docs/product/error-alerts.md`). The panel's status region
  says it once as it appears. The failure goes once a listing succeeds; when it goes while it, or the mark's popover,
  holds keyboard focus, focus moves to the panel's first enabled control.

When the page holding keyboard focus is replaced — by following a new page or paging through history — focus moves
to the report panel's first enabled header control, with a visible focus ring. If a modal dialog is open, focus
stays within it instead. Replacing a page leaves focus elsewhere in the window where it is.

## History pages are read-only

Only a session's **newest** page can be submitted from. On every older page:

- the page's form controls are disabled, and submitting its form sends nothing;
- the "Read-only" badge is shown in the row under the panel's header.

The rule does not depend on the panel: the daemon refuses any submission naming a page that is not
its session's newest, whatever that page's own markup does.

## What a page may contain, and what it cannot do

A page is a **static, self-contained HTML document**. None of its own scripts run, nothing external
loads, and the only way data leaves it is a form the user submits (see "Submitting a form back to the session"
below).

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

A page's body gets a default inline margin of **16 px** on each side, in place of the browser's 8 px, so its content
starts where the panel's header does. Any margin the page sets itself takes precedence, a universal reset such as
`* { margin: 0 }` included; the default leaves the body's top and bottom margins to the browser.

## Submitting a form back to the session

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

The submission is written into the session that pushed the page as a **user message**: a line naming
the page it came from, then one `key: value` line per field. A form with no named fields is still
delivered, with `{}` in place of the field lines.

Delivery takes the same path as every other write into a running session (see "Messages held until a
session can take them" in `docs/product/message-delivery.md`): written straight away while the
session is working or awaiting instructions, and **queued** while the session is waiting for the user — queued rather
than refused, because the user is the one submitting and there is nobody to ask to answer the session's prompt first.

The submission goes to the session that pushed the page, found from the page itself, whichever session the user happens
to be looking at. It fails, and the failure is shown to the user, when the page is no longer its session's newest, when
the session's process is not running, or when the session is a worker by then, which has no tool to read
the submission — the daemon refuses that page as one that does not exist. A failed submission changes nothing. The
message names which session the submission was meant for, so that it cannot be read as being about whichever session the
user happens to be looking at.

## Escape and F6 inside a page

Pressing Escape while focus is inside a page also tells the window so, on a history page too. The
key never reaches the window from inside the frame, so without this it could not send away a floating
report panel or close the sidebar's narrow-window drawer. It closes whatever overlay is open, as Escape does
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

- Back and Forward's shortcuts, `Cmd+[` and `Cmd+]` by default, go back and forward (see "Keyboard shortcuts" in
  `docs/product/navigation-history.md`).
- Next and Previous console session's shortcuts, `Ctrl+Tab` and `Ctrl+Shift+Tab` by default, move to the next or
  previous console session (see "The switch strip" in `docs/product/focus-mode.md`). The page never gets them, even
  when the window has nowhere to move at that moment; a key held down is passed on once.

As with Escape and `F6`, neither is passed on during an input method's composition, the signal is acted on only while
focus is inside the page's frame, and it carries nothing from the page. In a browser neither is passed on, and the
page keeps the browser's own handling of the keys.

The keys are passed on only as the exact combinations they are bound to, as the window matches them (see "How a
combination is written and matched" in `docs/product/keyboard-shortcuts.md`). Changing one of these four shortcuts
starts the page the panel shows over again from its markup, so anything typed into its form and not submitted is gone;
setting a shortcut to the keys it already has does not.

## Right-click inside a page

A page follows the window's rule (see "Right-click menus" in `docs/product/window-layout.md`): in the desktop app a
right-click on a page shows nothing, except in its input fields and text areas, which keep the system's menu for copy
and paste; in a browser the frame keeps the browser's own menu.
