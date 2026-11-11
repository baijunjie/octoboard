# Report panel

The **report panel** is the right pane's Report panel, to the right of the terminal (to its left under a right-to-left
language, see `docs/product/right-to-left-layout.md`): a card like any other in the right pane, with its own toggle in
the top bar (see `docs/product/right-pane-panels.md`), showing the pages a session pushed. Two kinds of session push
pages: a **console session** and a **project coordinator**, whether independent or reporting to a parent (see
`docs/product/hub-orchestration.md`). A **worker** has no report panel tools; becoming a worker withdraws the session's
pages (see "When a session becomes a worker" below).

**Pages belong to the session that pushed them**, not to its console or project: each session has its own pages, and
the panel shows those of the selected session only, so two sessions never share a history.

- **A console session**: the report panel comes first in its right pane, followed by its Processes panel (see
  `docs/product/processes-panel.md`). Their arrangement follows "Opening and closing panels" in
  `docs/product/right-pane-panels.md`. Selecting the console session gives it the right pane, where the report panel
  shows while it is open.
- **A project session**: its Report panel comes first in its project's right pane, followed by the project's Terminal,
  Files and Processes panels, arranged at most two per column. Those panels stay the project's own and are shared by
  all its sessions. It is offered only while the selected project session has pages, or listing them failed (see "A
  project session's Report panel" in `docs/product/right-pane-panels.md`).

When the right pane shows something else instead — a project's Files panel in a console session's place, or nothing — is
in "What the right pane shows" in `docs/product/window-layout.md`. How the panel is opened, closed, floated in and
maximized is in `docs/product/right-pane-panels.md`, and how wide the pane is and how the user resizes it in
"Resizing the sidebar and the right pane" in `docs/product/pane-docking.md`.

A page is a tree of components the session builds from a fixed catalogue — Markdown, callouts, tables, charts, form
fields and the rest — which Octoboard draws with its own interface components, for anything better shown than typed
into the terminal: a table, a comparison, a report, a set of choices, a question with a form. The catalogue is in
`docs/product/report-page-components.md`. A page may carry a form, and what the user submits comes back into the
session as a message, once.

## Pushing a page

The session pushes a page with its `show_page` tool (see "The console session's tools" and "Project coordinators' tools"
in `docs/product/hub-orchestration.md`), whose arguments are the page itself: an optional `title` and `body`, the
components top to bottom (see "The page object" in `docs/product/report-page-components.md`). The call answers with the
new page's **id**, which is what a later form submission names, so a session that has pushed several pages can tell
which one a submission came from. Its description tells the session to read the catalogue with `page_guide` before its
first page (see "The page guide" in `docs/product/report-page-components.md`).

**A page that does not pass the daemon's checks is refused, and nothing is stored or announced** (see "Checking a page
when it is pushed" in `docs/product/report-page-components.md`). The refusal names the failing location as a JSON path
(such as `body[2].children[0].options[1].value`) and what is wrong there, and carries the JSON Schema of the failing
component's kind, or of the page object when the failure is in it, so the session can correct the page and push it
again without another call. Nothing in a page is silently dropped.

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
  `show_page` returned it), `created_at` (milliseconds since the epoch), `content_bytes` (the size of its content as
  JSON, in bytes) and `title` (the page's `title`; `null` when it has none). The content itself is left out.
- With `page`, a page's id, it answers with that one page in full: `page`, `created_at`, `title`, `content` (the page
  as it was pushed, its title and component tree) and `submission` (what its form was submitted with, `null` until it
  is; see "Submitting a form back to the session" below). An id that is not one of the caller's pages — another
  session's included — is refused, the same way as an id that names no page, so the answer does not reveal that the
  page exists.

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
`transfer_session` (see `docs/product/changing-session-owners.md`). Either takes `show_page`, `page_guide`,
`list_pages` and `delete_pages` away from it, and **every page it pushed is withdrawn at once**, as `delete_pages` with `all: true`
withdraws them: the panel follows in every window as in "Withdrawing pages" above, so no form is left offered that the
session could no longer read the submission of. Nothing brings the pages back, a later release included.

- **Keeping the coordinator role keeps its pages.** Setting a console session as parent, or transferring a
  coordinator to another console session, keeps the report panel tools and pages.
- **The agent is told.** The descriptions of `show_page`, `bind_to_session` and `transfer_session`, and what a project
  session's role description says of the latter two, say beforehand that such a binding withdraws the pages, and the
  binding's own word to the session — the `bind_to_session` result, the message a transfer writes into it, or the notice
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
its order):

- ◀ and ▶ to step one page back and one forward, and between them the position as "current / total", counted from 1
  with the oldest page first;
- the time the page was pushed, to the minute: the time alone for a page pushed today, the date and the time for an
  earlier one, formatted for the UI's current language (see "What follows the language" in
  `docs/product/language.md`);
- a "Read-only" badge on any page but the newest (see "History pages are read-only" below), and a "Submitted" badge on
  a page whose form was submitted (see "Submitting a form back to the session" below).

The position, the total and the badges all count the selected session's pages only. Where the header is short of room,
the time is what shortens first (see "A panel's header" in `docs/product/right-pane-panels.md`).

The page itself fills the card under the header and scrolls within it (see `docs/product/scrolling-surfaces.md`). The
place the user has paged to is not kept: whenever the report panel gives way in the right pane — another session
selected, or Open panel in a project's action menu putting a project's panel in its place — coming back to it shows its
newest page again, with anything typed into any page's form and not submitted gone.

- The panel follows the newest page: a page pushed while the user is on the newest one moves the view
  to it.
- A page pushed while the user is reading **history** does not move them. The view stays on the page
  they paged to, and only the total grows.
- Paging away from a page keeps what was typed into its form: paging back to it shows those values again, for as
  long as the panel stays in the right pane. A page that is withdrawn takes what was typed into it with it.
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

- the page's form is locked: its fields show what they hold — what the page was submitted with, where it was — as
  values to read (see "Input fields" in `docs/product/report-page-components.md`), and its submitters are
  unavailable, so nothing can be submitted;
- the "Read-only" badge is shown in the panel's header.

What changes nothing on the page still works on a history page: copying a block's text, sorting and filtering a table,
switching tabs and a chart's own interactions. The rule does not depend on the panel: the daemon refuses any submission
naming a page that is not its session's newest.

## What a page may contain, and what it cannot do

A page is **data, not a document**: a tree of components from the fixed catalogue in
`docs/product/report-page-components.md`, with no HTML, no styling and no script of its own anywhere in it. Octoboard
draws every component with its own interface components, so a page follows the chosen appearance (see "What follows
the choice" in `docs/product/appearance.md`), the window's keyboard and assistive-technology behavior, and, for every
word Octoboard supplies itself, the UI's language and direction (see "Language and direction in a page" in
`docs/product/report-page-components.md`). The page's own words are the session's, shown as written. The page has no
surface of its own: it sits on its card's background, its content starting 16 px from the card's edge like every
panel's (see "The window chrome and the content panel" in `docs/product/window-layout.md`).

**What a page can do is Octoboard's own interaction**, with no code of the page's involved: showing or hiding a part as
the user picks (see "Conditional display" in `docs/product/report-page-components.md`), checking its fields before it
is sent, locking its form once submitted, sorting and filtering a table, switching tabs, copying a block's text, and a
chart's tooltip, legend and zoom. A page cannot submit by itself, nor send anything but the fields of its form.

**A page's content makes no network request**, so that a page written under prompt injection cannot carry what the
user typed out in an address:

- an image in its Markdown is shown only from a `data:` URL; any other stays its description and nothing is requested
  for it;
- a link to an absolute web or mail address opens in the system browser only when the user clicks it, the user's own
  act; a relative link or a link to a heading is plain text;
- a chart loads and opens nothing: the chart subset leaves out every option that would (see "Charts" in
  `docs/product/report-page-components.md`).

The only way data leaves a page is a form the user submits (see "Submitting a form back to the session" below).

In the desktop application the window adds a second barrier underneath: it blocks non-image `http` and `https`
requests (on Windows by answering them with a 403). HTTP(S) images are allowed for Markdown documents elsewhere in the
window, so it is the page's own drawing that keeps its images to `data:` URLs. Development builds also allow the local
development server's resources. If the window's block cannot be set up at launch, the window opens without it.

## Submitting a form back to the session

A page sends data back **only through its form**, and only when the user submits it: pressing a `submit` button or one
of a `choices` component's buttons, or pressing Enter in a single-line field, which presses the page's first shown
`submit` button (see "Submitters" in `docs/product/report-page-components.md`). Nothing on a page can submit on its own,
and a page without a submitter has no form and takes no submission. The press first checks the page's shown fields;
while any is wrong nothing is sent (see "Checking a form before it is sent" in
`docs/product/report-page-components.md`).

**A page takes one submission, and it is never sent twice.**

- Pressing a submitter **locks the page's form** at once: the pressed button shows the submission is under way, the
  other submitters are unavailable, the fields are locked (see "Input fields" in
  `docs/product/report-page-components.md`), and assistive technology is told the form is being submitted.
- The daemon **marks the page submitted before it delivers the message**, in one step that also checks that the page
  is its session's newest and not submitted yet, and tells every window. So however often the user presses, and from
  however many windows, the session gets exactly one message; every other attempt is refused as one whose page was
  already submitted (`page_already_submitted`). Every window ends up showing the same state of the page, whatever
  order the news of its changes reaches it in.
- **A submitted page is read-only**, in every window and after any reload: its fields show the values it was submitted
  with, which are stored with the page, the button that sent it stays highlighted, and the header shows the
  "Submitted" badge — in the window that sent it, once its submission is no longer under way.
- A submitter that holds keyboard focus as it turns unavailable hands focus to the report panel's first control
  rather than losing it.

**What is sent**: one field per shown input field, keyed by its `name`, and, for a pressed submitter with a `name`,
`name: value`. An optional field left empty — a text of only blanks, no option chosen, no box ticked — is left out, and
so is a hidden field (see "Conditional display" in `docs/product/report-page-components.md`), so the message has no line
for them. How each kind of field's value is sent is in "Input fields" in `docs/product/report-page-components.md`.

The submission is written into the session that pushed the page as a **user message**: a line naming the page it came
from by its id (see "The headers" in `docs/product/message-headers.md`), a blank line, then one `key: value` line
per field in the page's order, a multiple choice's values joined with ", ", a boolean as `true` or `false` and a number
as its plain figure:

```text
Report panel form submission — page 1f0e…

name: Ada
topics: rust, sqlite
subscribe: true
action: approve
```

A submission with no field — every field hidden or left empty, and a submitter with no name — is the opening line
alone.

Delivery takes the same path as every other write into a running session (see "Messages held until a session can take
them" in `docs/product/message-delivery.md`): written while the session can take a message, and **queued** while a hold
applies or the session is waiting for the user — queued rather than refused, because the user is the one submitting
and there is nobody to ask to answer the session's prompt first.

The submission goes to the session that pushed the page, found from the page itself, whichever session the user happens
to be looking at. It fails, and the failure is shown to the user as an error alert, when the page is no longer its
session's newest, when it was already submitted, when the session's process is not running, or when the session is a
worker by then, which has no tool to read the submission — the daemon refuses that page as one that does not exist. On
a Codex session whose messages go into Codex's own queue, it also fails when Codex refuses the submission
(`message_refused`, see "A message Codex's own queue refuses" in `docs/product/message-delivery.md`), or when Codex may
have taken it without confirming it (`message_acceptance_unconfirmed`, see "A message whose acceptance is uncertain" in
`docs/product/message-delivery.md`). The message names which session the submission was meant for, so that it cannot be
read as being about whichever session the user happens to be looking at. What a failure leaves:

- **A delivery that certainly did not happen** — the session not running, the daemon stopping, or Codex refusing the
  message — takes the page's submitted mark off again, in every window, and the form unlocks with the values the user
  entered, to be submitted again.
- **One Codex may have taken without confirming** keeps the mark: the message may already be in the session, so the
  page stays submitted, showing what was sent, and nothing is sent again.
- A submission refused before the page was marked — not the newest page, already submitted, or a worker's — changes
  nothing on the page.
