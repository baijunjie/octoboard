# Error alerts

Where a failure is shown follows from how it arises, not from the screen it arises on:

| The failure | Where it is shown |
|---|---|
| A form's own validation: what is checked the moment the user submits (a field left empty, a malformed or too long value, a duplicate of a name the window knows) | Under its field (see "Validation under a field" below) |
| A failure of an operation the user started, once it has been sent: a submitted form, a menu or row action, a switch, copying, opening, and daemon and connection failures reported as errors | The **error alert**, a dialog that requires acknowledgement (see "The error alert" below) |
| A failure that is the state of what a screen shows: a page, pane, panel, list or submenu that could not be loaded, permissions that could not be read, a lookup a form makes on its own while the user types | On that screen: in place of a whole subject that could not be loaded, or in a compact alert among other content (see "Failures shown on the screen" below) |
| A failure that belongs to one row, table cell or spot in a rendered document, or to one part of a pane that still shows the rest | A mark on that row or part (see "Failures marked on a row" below) |

Validation, a failure shown on the screen and a row's mark never open an error alert: each is that failure's only
presentation.

Errors never use timed toasts or a failure line inside the dialog that started the operation. Notices that are not
errors may use toasts; see `docs/product/toasts.md`. Background states that are not reported as errors, such as the
daemon's reconnecting banner, keep their own status and recovery presentation (see "Losing the daemon connection" in
`docs/product/application-lifecycle.md`).

## Validation under a field

What is wrong with one field, such as a required field left blank, is said under that field, and the field is marked
invalid. A field is never marked before the user has tried to submit: the message appears on the first attempt —
which sends nothing while any field is still wanting — and from then on follows what is typed, so it goes as soon as
the field is corrected and comes back if the field is emptied again.

Once a submit has passed the form's own validation, anything that goes wrong afterwards is a failure of the operation
and opens the error alert, with the form left open and its values intact. This includes a refusal by the daemon about
a single field's value, such as an account directory another account of the agent already uses (see "Adding, editing
and removing an account" in `docs/product/agent-accounts.md`) or a conversation the session dialog cannot resume (see
"Resuming a conversation by its session ID" in `docs/product/sessions.md`).

## The error alert

The error alert uses the standard danger icon and a danger acknowledgement button. It appears above any open dialog
and takes keyboard focus. It identifies the failure and preserves available details, including the affected
session's location when the failure carries one. It has one acknowledgement button. Escape, a click outside and a
close button cannot dismiss it, and it never dismisses itself. Acknowledgement does not retry the failed operation.
The originating dialog remains available for correcting inputs or retrying unless that operation's own contract
closes it, or a recovery action navigates Settings and closes its account or agent forms (see "Opening and closing
Settings" in `docs/product/settings.md`).

Errors are queued and normally shown one at a time in arrival order. An identical failure already waiting for
acknowledgement is not queued again: its message, affected session, original diagnostic message, recovery category
and available daemon code and parameters must all match. Pending errors survive changes between the application's
screens until acknowledged. Startup and rendering failures use the same acknowledgement flow.

When the last pending alert is acknowledged and keyboard focus has nowhere to return to — the control that had it is
gone, as when a dialog closed itself before the alert appeared — focus goes to the dialog still open, if any.
Otherwise it goes to the selected session's terminal. With no session selected, or no console at all, neither the
terminal's empty panel nor the welcome screen takes it, as neither takes focus while it is on nothing (see "With no
session selected" in `docs/product/session-terminal.md` and "Keyboard focus" in `docs/product/welcome-screen.md`):
it goes to the sidebar instead, else, when the sidebar is not shown or has no control to take it, to the rail. Each
region lands focus where `F6` would (see "Moving focus between regions with F6" in
`docs/product/moving-focus-between-regions.md`).

The error alert announces the failure it shows; nothing else announces that failure again.

## Failures shown on the screen

A failure that is the state of what a screen shows is shown there, in one of two shapes, chosen by whether anything
else is shown with it. Either way, what is on screen is at most two sentences in Octoboard's own words: what happened,
and the one main thing to do about it. Everything else is behind **Details**, a button that opens a popover holding,
in this order, any further explanation in Octoboard's words (a reason per cause, each cause's own fix, further
guidance), then the text that was not written for the reader: a message passed through from the system, Git or an
agent, a diagnostic block, raw entries. Such text is never shown outside Details, and the two sentences on screen are
never behind it. A failure with nothing beyond its two sentences has no Details button. A failure Octoboard has no
message of its own for, whose only words are the system's raw text, says that something unexpected went wrong, with
the raw text behind Details. Nothing about the failure stands beside it as loose text.

**An empty-state failure** stands in place of a whole subject that could not be loaded — a page, pane, panel or list
with nothing else to show. It takes the shape of the empty state's full panel (see "The full panel" in
`docs/product/empty-states.md`): a red circled `!` as its glyph, a heading saying what could not be loaded, at most two
sentences under it, and in its centred row **Try again** as the primary button, where the subject can be read again,
and **Details** as a secondary one, where there is more to show.

**A compact alert** is a failure shown among other content: in a form dialog, a settings section, a popover or a
submenu, or above content still shown. It is red, with the danger glyph, or amber, with the warning glyph, where its
subject is still usable. It holds an optional title and one or two sentences, and at its trailing end at most two
buttons, in the alert's own colour: **Details**, where there is more to show, then the recovery action, where there is
one — usually **Try again**, or the action that fixes it. A failed lookup a form makes on its own shows its alert
directly under the field it is about, which it describes to assistive technology.

A failure of one part of a pane that still shows the rest is neither: it is a mark on that part (see "Failures marked
on a row" below).

Such a failure opens no error alert. It is announced to assistive technology once, as it appears — its heading or
title and its first sentence, never what to do about it, its details or its actions — when it appears after the view
was drawn; the same failure already there when the view opens is not announced. A retry that fails again in the same
words is announced again. The places with an empty-state failure:

- the project pane: its code that could not be loaded, the project's directory that could not be listed, and a file
  name search that failed (see `docs/product/project-pane.md`);
- the Git mode: the repository that could not be read or Git review being unavailable, the branches when none has been
  listed yet, and the change list when nothing is listed and the one part of it that failed left it empty (see
  `docs/product/project-pane-git-mode.md` and "What the list says instead" in
  `docs/product/project-pane-git-change-list.md`);
- the file viewer: a file or change that cannot be opened, and an image that cannot be decoded (see "Notices and
  stand-ins" in `docs/product/file-viewer.md`). The viewer's failure notices over content it still shows open no error
  alert either;
- an archived session's history that could not be loaded (see "Loading, empty, unavailable and
  failed" in `docs/product/session-history.md`);
- the report panel's pages that could not be listed while it has no page to show (see "Paging through the history" in
  `docs/product/report-panel.md`).

The places with a compact alert:

- the report panel's pages that could not be listed while it shows a page, in amber (see "Paging through the history"
  in `docs/product/report-panel.md`);
- the file viewer: a file still in conflict whose content cannot be read, under its conflict notice (see "Notices and
  stand-ins" in `docs/product/file-viewer.md`);
- an account's sharing change that did not finish, at its switch, in amber (see "Sharing the default configuration" in
  `docs/product/agent-accounts.md`);
- the directory browser's listing (see "Browsing directories" in `docs/product/projects.md`);
- the handoff dialog's check of the branches a session is responsible for (see "Handing off from the window" in
  `docs/product/handing-off-sessions.md`);
- a submenu whose items are read as it opens, which is the project menu's Switch branch (see "Switching the local
  branch" in `docs/product/project-git-actions.md`). Its alert is the exception to the two sentences above: it says
  the items could not be loaded followed by the reason as it arrived, and has no Details;
- Settings' Permissions section when the grants could not be read (see "Permissions" in `docs/product/settings.md`);
- a configuration import's application settings that could not be applied, in the import's result (see "In the
  window" in `docs/product/configuration-export-and-import.md`);
- the lookups forms make on their own: a project folder's or repository's detection (see "Associating a project" in
  `docs/product/projects.md`), the session dialog's Session ID lookup, including a conversation not found or held by a
  session elsewhere (see "Finding the conversation" in `docs/product/sessions.md`), and a model list that is not to be
  had (see "While the list is being fetched, and when there is none" in
  `docs/product/model-and-reasoning-effort.md`).

## Failures marked on a row

A failure that belongs to one item of a list, one table cell or one spot in a rendered document, or to one part of a
pane that still shows the rest, does not turn that row, part or sentence into an error message. The item shows a small
**mark** where its status or trailing content sits — beside the heading or the control of a part, inline with the text
in a document: a red circled `!` for a failure, or an amber warning triangle where what the row shows is still there
but out of date. The mark is a button with an accessible name saying what failed, and it has no tooltip: the reason
opens from it in one **popover**, the only floating thing it shows. The popover holds what happened, the one main
thing to do about it, any further explanation, then any raw text (see "Failures shown on the screen" above), and,
where the failure can be retried from there, **Try again** at its trailing end. A reason that is one short line and
has nothing more to it (an image or a formula in a document that could not be shown) is that popover with the one
line in it. The exception is a mark followed by a word that has nothing more to say (a failed import line with no
reason given): it is the glyph and the word as plain text, no button and no popover.

The popover opens by the input the device has:

- **Resting the mouse on the mark** for 250 ms opens it **without moving keyboard focus**, so the terminal or a text
  field keeps receiving keystrokes; a pointer that only crosses the mark opens nothing, and touch and pen never open
  it this way. It stays while the mouse is on the mark or inside the popover, and goes 150 ms after the mouse has left
  both; the popover's buttons and text are reached by moving the mouse into it. A press outside closes it, and so does
  **Escape**, which then goes no further (it reaches neither a terminal nor a drawer or floating pane under it) —
  except while keyboard focus is in a terminal (the agent's or a shell's) or in a text field holding text, or while an
  input method is composing, where the key stays with them and the popover stays open.
- **A mouse click on the mark opens nothing**: where there is a mouse there is the hover, and the popover is its.
- **A tap** (touch or pen), **assistive technology** and **Enter or Space** open it with keyboard focus inside, so Try
  again and the raw text can be reached. Escape, a press outside or another press of the mark closes it, and keyboard
  focus goes back where it was when the popover opened; where that would be the mark on a section's heading, the
  heading takes it. A tap on a popover the mouse opened turns it into this form.

After the popover is closed in any way but the mouse leaving — Escape, a press, Try again — a mouse still resting on
the mark does not open it again until it has left the mark and come back.

The places:

- a folder in the Files tree that could not be listed (see "Rows and the keyboard" in `docs/product/project-pane.md`);
- a project or console of a configuration import that failed, in the import's result, where the word **Failed**
  follows the mark, as the other results have their words, and where a reason was given the mark and word open the
  popover as one (see "In the window" in `docs/product/configuration-export-and-import.md`);
- a project's branch badge whose last check failed, where the mark stands in for the badge's branch glyph; there it is
  no button and opens no popover, the reason being part of the badge's own tooltip and name (see "The branch badge" in
  `docs/product/project-git-status.md`);
- an account whose quota could not be read, in Settings and the rail's quota panel (see "Quota states" in
  `docs/product/account-quotas.md`);
- a project image or a formula inside a paragraph of a Markdown document that could not be shown, where the mark takes
  the text's size and is a tab stop in the document like a link, and one inside a link is drawn right after the link
  rather than in it, so pressing it does not follow the link (see "A Markdown file as a document" in
  `docs/product/file-viewer.md`);
- in the Git mode, a part of the change list that could not be read, on its section's heading, and a list of branches
  that could not be read again, beside the branch selector (see "What the list says instead" in
  `docs/product/project-pane-git-change-list.md` and "The branch selector" in `docs/product/project-pane-git-mode.md`).

## Names and diagnostic details

An error that references a console, project, session, shell terminal or account by its record uses the current name
when that record is available. The name appears as an inline name chip. If the record is missing, the supplied ID
remains visible as a technical code chip; a report page, which has no display name, is identified by its ID. Names
supplied directly by the operation remain names, even when their text happens to match another record's ID. Agent
and product names referenced in the explanation use name chips too. Each name keeps its own text direction;
technical values keep left-to-right order. Accessible labels, system notifications and other text-only surfaces
retain usable plain wording with the same names or fallback IDs.

Known raw output from Git and agent commands appears separately from the short explanation as selectable code,
including when it is the cause of a launch or trust-confirmation failure, an import failure, or a model or quota
reading failure. The output retains its line breaks, indentation and left-to-right order. Long lines scroll
horizontally, with a labelled region reachable from the keyboard. A short ordinary reason remains part of the
explanation; it is not turned into a diagnostic block simply because it accompanies an error. Unrecognized daemon
messages retain the daemon's own readable wording; on the screen and in a mark's popover, that wording follows the
sentence saying something unexpected went wrong (see "Failures shown on the screen" above). These rules hold wherever
the failure is shown: in an error alert, in a failure on the screen, where such output is behind Details, or in a
mark's popover, where it comes last.

## Recovery and unexpected errors

The alert distinguishes failures with a known user remedy from unexpected failures. Both use the danger presentation
and require acknowledgement.

**An actionable failure** includes a specific next step alongside the original reason. References to controls such as
Retry, Browse and Unlock appear as inline references rather than quoted text. For example:

- A full disk asks the user to free space on the disk used by Octoboard and the project, then retry.
- A locked session asks the user to choose Unlock in that session's action menu, then retry.
- An invalid or unavailable path asks the user to correct it or browse to an existing, accessible location.
- An account name already in use asks for an unused name for that agent; an occupied destination asks for a different
  destination while preserving the existing folder.
- A clipboard failure explains granting clipboard access or using the terminal's keyboard copy and paste shortcuts.

Known domain failures, the causes the daemon names for some of them (such as why a session's conversation could not
be moved to another account; see "When a switch fails" in `docs/product/switching-session-accounts.md`), and
recognized operating-system causes provide the remedy; a recognized operating-system cause takes precedence over the
others, except for a switch's copy that failed and could not put back an older item: that is classified by the item
put aside, and the operating-system cause's remedy is given as well. A failure whose named cause is not one the window
knows stays unexpected. An unfamiliar failure does not get generic retry advice merely because it occurred during a
familiar operation.

**A remedy that lives in Settings' Agent accounts section has a button** in the footer, a secondary action before the
acknowledgement button, instead of the step naming where to go:

| Failure | Button | What it opens |
|---|---|---|
| The agent is not added (`agent_not_added`) | **Add agent…** | Settings on Agent accounts, with the Add agent dialog in front (see "Adding an agent" in `docs/product/agent-accounts.md`) |
| An account problem (`unknown_account`, `grok_home_not_initialized`) | **Manage accounts…** | Settings on Agent accounts |
| The added agents not being as the change expected (`agent_already_added`, `agent_order_invalid`) | **Manage accounts…** | Settings on Agent accounts |

Pressing the button acknowledges the alert and opens Settings there, above whatever dialog was open; when Settings is
already open, it moves to that section instead (see "Opening and closing Settings" in `docs/product/settings.md`). The
step in the alert's body then says only what to do, such as adding the agent and trying again.

**A plain alert** says only what happened, with no next step and nothing but its acknowledgement button. It is used
when a dialog closes itself because what it would choose from went away while it was open (see "Opening a session" in
`docs/product/sessions.md` and "Handing off from the window" in `docs/product/handing-off-sessions.md`).

**An unexpected failure** explains that it may be a bug the user cannot resolve in the current interface. The alert
keeps the error explanation and offers expandable diagnostic details, **Error details**, when the original message or
stack trace adds information to its summary. This includes diagnostic strings and errors from another rendering
context, even when they have no stack trace. It asks the user to take a screenshot and file an Issue describing the
steps that led to the failure.

**Report issue** is a secondary action in the footer, before the acknowledgement button, with a trailing
external-link icon. The guidance names it with a non-interactive inline reference; only the footer action is a link. It
opens the explicit `issueUrl` configured in `config/app.json` when the user activates it. Repeated activation is
blocked while opening is pending. It does not create an Issue or attach diagnostics or a screenshot automatically.
The error stays open until acknowledged.
If opening the page fails, a separate actionable error immediately appears above the original error, with the
address as selectable text on a line of its own and instructions to open it manually. Acknowledging that failure
returns to the original pending error. Where the desktop platform cannot open external links, the guidance includes
the address as a selectable chip for manual use.

**A batch of failures**, such as a set of account switches, uses one acknowledgement dialog listing each failed item.
It gives each failed item only its short reason, and says each remedy once after the list: a code's own remedy once
for that code, and one sentence per kind of remedy for the rest (see "The alert for failed switches" in
`docs/product/switching-session-accounts.md`).

If any item is unexpected, the dialog adds one shared screenshot and Issue-reporting instruction and the same Issue
entry or manual address described above; it does not replace the other items' remedies with a common instruction.
The footer carries a remedy's button (see above) only when every failure in the batch has that same remedy. A batch
whose items have raw text of their own, such as a set of account switches, sets **Error details** apart by item, a
heading naming the item over its own block, in place of the details worked out from the errors: it shows the
disclosure whether or not a failure is unexpected, and leaves out an item whose raw text adds nothing to its reason.
When no item of such a batch has raw text, the alert has no **Error details** at all, even when a failure is
unexpected. The result or retry controls remain available after acknowledgement under the operation's own rules.
