# Error alerts

Where a failure, a warning or a note is shown follows from how it arises, not from the screen it arises on:

| How it arises | Where it is shown |
|---|---|
| A failure of the user's own action, once it has been sent: a submitted form, a Save or a confirm, a switch, a menu or row action, copying, opening, and daemon and connection failures reported in reply | The **error alert**, a dialog that requires acknowledgement (see "The error alert" below). A submit or a save that fails inside a dialog opens it over that dialog, which keeps what was entered |
| An error located at one input field: the form's own validation (a field left empty, a malformed or too long value, a duplicate of a name the window knows); a lookup the form makes on its own that finds the field's value wrong (a path that does not exist or is not a directory, a conversation ID found nowhere); a value the field holds that cannot be used (an account turned off, used up or having its sharing changed) | Under its field, which is marked invalid (see "Errors under a field" below) |
| An important failure of automatic behaviour — a load, a refresh, a lookup a form makes on its own that could not run — one that blocks what follows, or that a component cannot do its job without | On that screen: in place of a whole page, pane, panel or list that could not be loaded, an empty-state failure; among other content, a compact alert (see "Failures shown on the screen" below) |
| A failure or a warning that does not block what follows, and a note | A mark at the point it concerns — a row, a table cell, a field, a checkbox, a setting, a list section's heading, a page's heading or a pane's or view's title, a line naming what it is about, a spot in a rendered document; one with no point to be located at is a compact alert in a shared place, the top of a dialog's body or of a section (see "Failures, warnings and notes marked at their point" below) |
| A result view showing a failed result | A compact alert in that view: a configuration import's application settings that could not be applied (see "In the window" in `docs/product/configuration-export-and-import.md`) |
| A failure that ends what a dialog was opened for, while it is open | The dialog turns into a failure modal (see "The failure modal" below) |

A destructive confirmation is not a failure: it is a warning modal, the warning icon in its header and what it does as
its body text (see "What an open dialog, menu or popover covers" in `docs/product/window-layout.md`).

A failure under a field, on the screen or as a mark never opens an error alert: each is that failure's only
presentation, with these exceptions:

- The screens for a daemon that could not start and for a UI that failed also open an error alert, shown over them
  first (see "Failures shown on the screen" below).
- A check for updates the user pressed that does not finish opens an error alert and still leaves the Updates row's
  warning mark (see "The Updates and Update channel rows" in `docs/product/in-app-updates.md`).
- An account's sharing change, a long-running change that its own switch retries, that ends unfinished opens an error
  alert in the window that asked for it, and still leaves the warning mark at the switch as its lasting state, the
  alert alone speaking it there; the same change seen ending unfinished from another window, or found so later, is the
  mark alone (see "Sharing the default configuration" in `docs/product/agent-accounts.md`).
- The directory browser's alert for the listing made as it opens stays as it is when a navigation the user then starts
  — **Go**, or its **Try again** — fails: that failure opens an error alert (see "Browsing directories" in
  `docs/product/projects.md`).

Errors never use timed toasts or a failure line inside the dialog that started the operation. Notices that are not
errors may use toasts; see `docs/product/toasts.md`. Background states that are not reported as errors, such as the
daemon's reconnecting banner, keep their own status and recovery presentation (see "Losing the daemon connection" in
`docs/product/application-lifecycle.md`).

## Errors under a field

What is wrong with one field is said under that field, and the field is marked invalid; for a field made of an agent
picker joined to an account picker, under the pair. Assistive technology hears it as the field's description where
focus lands, and it is announced as it appears.

**The form's own validation** — what is checked the moment the user submits, such as a required field left blank —
never marks a field before the user has tried to submit: the message appears on the first attempt — which sends
nothing while any field is still wanting — and from then on follows what is typed, so it goes as soon as the field is
corrected and comes back if the field is emptied again.

**What the form finds out on its own about a field's value** is said as soon as it is known, without waiting for a
submit, and goes when the value changes or stops being wrong; the submit button is disabled meanwhile:

- a lookup the form makes that finds the value wrong: a project directory that does not exist or is not a directory
  (see "Associating a project" in `docs/product/projects.md`), or a conversation ID no account holds (see "Finding the
  conversation" in `docs/product/sessions.md`). A lookup that could not be made says nothing about the value and is a
  compact alert instead (see "Failures shown on the screen" below);
- an account the field holds that cannot be used: turned off, used up or having its sharing changed, in the session,
  handoff and session request dialogs (see the **Agent** field in "Opening a session" in `docs/product/sessions.md`,
  "Handing off from the window" in `docs/product/handing-off-sessions.md` and "The user's approval of an owner's
  switch" in `docs/product/switching-session-accounts.md`), and, in a switch request, an account asked for that no
  longer exists or that the session is on already.

Once a submit has passed the form's own validation, anything that goes wrong afterwards is a failure of the operation
and opens the error alert, with the form left open and its values intact. This includes a refusal by the daemon about
a single field's value, such as an account directory another account of the agent already uses (see "Adding, editing
and removing an account" in `docs/product/agent-accounts.md`) or a conversation the session dialog cannot resume (see
"Resuming a conversation by its session ID" in `docs/product/sessions.md`).

## The error alert

The error alert uses the standard danger icon and a danger acknowledgement button. It appears above any open dialog and
takes keyboard focus. It identifies the failure and preserves available details. Its title says in plain words what
failed — "Could not start the session", "Could not export the configuration" — and names no session, account or other
subject (see "What an open dialog, menu or popover covers" in `docs/product/window-layout.md`). Every alert raised by a
user's action, and every one a daemon broadcast raises, has such a title: the action's — "Could not save the
setting", "Could not hand off the session", "Could not switch branches", "Could not copy", or, for a session's
**Resume**, "Could not reopen the session" when it is archived and "Could not resume the session" otherwise — or the
daemon code's own, as "Branches and worktrees not removed" (`session_branches_not_removed`) and "Branches and worktrees
left in place" (`project_branches_not_determined`) are. A daemon code's own title wins over the action's only for an
alert a daemon broadcast raises and for some actions' failures outside a dialog; an alert raised by a dialog's submit or
confirmation always takes that dialog's title for its action, whatever code the failure carries.
A failure of the application's own lifecycle has its own title too — "The daemon failed to start", "The daemon exited
unexpectedly", "Could not quit", "Could not restart" (see "Starting up", "Quitting" and "Restarting" in
`docs/product/application-lifecycle.md`). Only a failure nothing titles — an error nothing caught — is titled
"Unexpected error" when it is unexpected, and "Error" when its cause is a known one. A title of its own does not change
how a failure is classified: the daemon exiting, and a quit or a restart that did not happen, are still unexpected
failures (see "Recovery and unexpected errors" below). When the failure is
about a session, the first line of its body, "Session: …", names that session and where it is (its project or
console), since the daemon's message says only "this session"; a session whose record is already gone, as after a
failed launch, is named there by the name the failure carried, with no place. The rest of the body is in Octoboard's
own words — what happened, the next step, any further explanation — and holds no raw text: that is only under the
collapsed **Error details** disclosure at the body's end (see "Names and diagnostic details" below). It has one
acknowledgement button. Escape, a click outside and a close button cannot dismiss it, and it never dismisses itself.
Acknowledgement does not retry the failed operation. The originating dialog remains available for correcting inputs or
retrying unless that operation's own contract closes it, or a recovery action navigates Settings and closes its account
or agent forms (see "Opening and closing Settings" in `docs/product/settings.md`).

Errors are queued and normally shown one at a time in arrival order. An identical failure already waiting for
acknowledgement is not queued again: its message, affected session, original diagnostic message, recovery category
and available daemon code and parameters must all match. Pending errors survive changes between the application's
screens until acknowledged. Startup and rendering failures use the same acknowledgement flow.

When the last pending alert is acknowledged and keyboard focus has nowhere to return to — the control that had it is
gone, as when a dialog closed before the alert appeared — focus goes to the dialog still open, if any.
Otherwise it goes to the selected session's terminal. With no session selected, or no console at all, neither the
terminal's empty panel nor the welcome screen takes it, as neither takes focus while it is on nothing (see "With no
session selected" in `docs/product/session-terminal.md` and "Keyboard focus" in `docs/product/welcome-screen.md`):
it goes to the sidebar instead, else, when the sidebar is not shown or has no control to take it, to the rail. Each
region lands focus where `F6` would (see "Moving focus between regions with F6" in
`docs/product/moving-focus-between-regions.md`).

The error alert announces the failure it shows; nothing else announces that failure again.

## The failure modal

A state in which a dialog's action cannot succeed is prevented where it can be — an entry that would open such a
dialog is disabled, as New session is while no agent can be picked. When that state comes about while the dialog is
open, so that it can no longer do what it was opened for, the dialog does not close itself, and no error alert opens
over it: **the dialog turns into a failure modal** in place, in the error alert's form. Its content is replaced by the
standard danger icon in its header, a title naming the failure, the explanation as sentences in its body (with names
as chips, as in "Names and diagnostic details" below) and any raw text behind **Error details**, and a single
acknowledgement button: nothing to submit or retry, no remedy button and no request to report an issue. Escape, a
press outside and a close button do not dismiss it; acknowledging it closes the dialog, and what the dialog would
have done is not done. It is announced as an alert dialog, with its explanation. It stays as it was when it appeared
until it is acknowledged, even if the state that caused it passes.

| Dialog | Title | Why |
|---|---|---|
| The session dialog | That a session cannot be opened | No agent can be picked any more, or the console session it was opened to report to is archived or gone (see "Opening a session" in `docs/product/sessions.md`) |
| The handoff dialog | That the session cannot be handed off | No agent other than the session's own can be picked any more (see "Handing off from the window" in `docs/product/handing-off-sessions.md`) |
| The confirmation of a request to switch a session's account | That the account cannot be switched | The session has been archived; acknowledging refuses the request (see "The user's approval of an owner's switch" in `docs/product/switching-session-accounts.md`) |

## Failures shown on the screen

A failure that is the state of what a screen shows is shown there, in one of two shapes, chosen by whether anything
else is shown with it. Either way, what is on screen is a heading or title saying what happened and, under it, at most
two sentences in Octoboard's own words: why, where that is known, rather than the heading said again, and the one main
thing to do about it. Everything else is behind **Details**, a button that opens a popover holding,
in this order, any further explanation in Octoboard's words (a reason per cause, each cause's own fix, further
guidance), then the text that was not written for the reader: a message passed through from the system, Git or an
agent, a diagnostic block, raw entries. Such text is never shown outside Details, and the two sentences on screen are
never behind it. A failure with nothing beyond its two sentences has no Details button. Nothing about the failure
stands beside it as loose text.

**A failure Octoboard has no message of its own for**, whose only words are the system's raw text, opens with a
sentence of Octoboard's saying what that text shows it to be: "The disk is full.", "Access was denied." or "The
connection failed." where the text names one of those causes; "The operation could not be completed." for any other
failure Octoboard can still tell the user how to act on; and "Something unexpected went wrong." only for one that is
unexpected (see "Recovery and unexpected errors" below). The raw text is behind Details. This holds wherever such a
failure is shown — on the screen, in a mark's popover, and in an error alert, where the raw text is under **Error
details**.

**An empty-state failure** stands in place of a whole subject that could not be loaded — a page, pane, panel or list
with nothing else to show. It takes the shape of the empty state's full panel (see "The full panel" in
`docs/product/empty-states.md`): a red circled `!` as its glyph, a heading saying what could not be loaded, at most two
sentences under it, and in its centred row **Try again** as the primary button, where the subject can be read again,
and **Details** as a secondary one, where there is more to show — further explanation in Octoboard's words, then the
raw text, as above. The screens for a daemon that could not start and for a UI that failed have no **Details**: their
row is kept for the recovery and **Quit**, and their raw text is in the popover of an error mark right after the
heading (see "Failures, warnings and notes marked at their point" below).

**A compact alert** is a component-level error shown among other content — one that blocks what the user is doing,
or one a component cannot do its job without: in a form dialog, a settings section, a popover or a submenu, or above
content still shown. It is red, with the danger glyph. It always has a title, with its sentences under it, and at its
trailing end at most two buttons, in the alert's own colour: **Details**, where there is more to show, then the
recovery action, where there is one — usually **Try again**, or the action that fixes it. It is used even where the
error can be located: a failed lookup a form makes on its own stops the form from going on, so it shows its alert
directly under the field it is about, which it describes to assistive technology, and the Git mode's uncommitted
changes that could not be listed leave the change list without them, so their alert stands at the top of that list.

A failure shown in either shape opens no error alert. It is announced to assistive technology once, as it appears — its
heading or title and its first sentence, never what to do about it, its details or its actions — when it appears after
the view was drawn; the same failure already there when the view opens is not announced. A retry that fails again in the
same words is announced again. Unlike the others, the Git mode's alert for uncommitted changes that could not be listed
does not announce itself: the view's status region speaks its title once instead, when it appears after the view was
drawn.

The places with an empty-state failure:

- the project pane: its code that could not be loaded, the project's directory that could not be listed, and a file
  name search that failed (see `docs/product/project-pane.md`);
- the Git mode: the repository that could not be read or Git review being unavailable, the branches when none has been
  listed yet, and the change list when nothing is listed and the one part of it that failed left it empty (see
  `docs/product/project-pane-git-mode.md` and "What the list says instead" in
  `docs/product/project-pane-git-change-list.md`);
- the file viewer: a file or change that cannot be opened, a file still in conflict whose content cannot be read,
  and an image that cannot be decoded (see "Notices and stand-ins" in `docs/product/file-viewer.md`). The viewer's
  failure notices about content it still shows are marks, and open no error alert either;
- an archived session's history that could not be loaded (see "Loading, empty, unavailable and
  failed" in `docs/product/session-history.md`);
- the report panel's pages that could not be listed while it has no page to show (see "Paging through the history" in
  `docs/product/report-panel.md`);
- the screens for a daemon that could not start and for a UI that failed, each the whole page, its heading the page's
  top-level heading followed by the error mark holding the raw text, with **Quit**, where the application can quit
  itself, after the recovery or, with none, as the one action (see "The window chrome and the content panel" in
  `docs/product/window-layout.md`). Unlike the others, each also opens an error alert, shown
  over it first, and so is not announced itself.

The places with a compact alert:

- the Git mode's uncommitted changes that could not be listed while other changes still show, at the top of the change
  list, with **Try again** (see "What the list says instead" in `docs/product/project-pane-git-change-list.md`);
- the directory browser's listing made as it opens, when it fails; a navigation the user then starts that fails, its
  **Try again** included, opens the error alert instead (see "Browsing directories" in `docs/product/projects.md`);
- a submenu whose items are read as it opens, which is the project menu's Switch branch (see "Switching the local
  branch" in `docs/product/project-git-actions.md`), titled that the items could not be loaded;
- Settings' Permissions section when the grants could not be read (see "Permissions" in `docs/product/settings.md`);
- a configuration import's application settings that could not be applied, in the import's result (see "In the
  window" in `docs/product/configuration-export-and-import.md`);
- the lookups forms make on their own that stop the form from going on, directly under the field they are about,
  which they describe to assistive technology: a project folder's detection that could not be made, and a
  repository's check (see "Associating a project" in `docs/product/projects.md`), and the session dialog's Session ID
  lookup that failed, or that found the conversation held by a session elsewhere or by an agent or account no longer
  offered (see "Finding the conversation" in `docs/product/sessions.md`). A lookup that finds the field's value wrong
  is that field's error instead (see "Errors under a field" above). A model list that is not to be had leaves the
  dialog usable, so it is a warning mark after the Model field's label instead (see "Failures, warnings and notes
  marked at their point" below);
- a reason a dialog cannot go on that lies outside its fields, at the top of its body: in the confirmation of a
  request to switch a session's account or to hand a session off, what holds the session itself, whatever is chosen
  (see "The user's approval of an owner's switch" in `docs/product/switching-session-accounts.md` and "The user's
  approval of an owner's handoff" in `docs/product/handing-off-sessions.md`).

Any other failure, and every warning and note — one that leaves its component doing its job, such as a failure of one
part of a pane that still shows the rest — is neither: it is a mark at the point it concerns, a row, a name, a
control, a field, a checkbox, a setting, one section's heading, a page's heading or a pane's or view's title (the
listing or the file it names, while its content is still shown), or a line naming what it is about (see "Failures,
warnings and notes marked at their point" below).

## Failures, warnings and notes marked at their point

A warning, a note, or a failure that leaves its component doing its job, and that belongs to one point — one item of a
list, one table cell, a field, a checkbox, a setting, one section's heading, a page's heading or a pane's or view's
title, a line naming what it is about, or one spot in a rendered document — does not turn that row, part or sentence
into a message. It shows a small **mark** at that point: right after the item's name, or in the status column where
the item has one; right after a section's heading, a page's heading or a pane's or view's title; at the end of the
line it qualifies; for a field, right after the field's label when it is about the whole field (the list of options it
could not read), and inside the field, at its trailing end before its Clear button, when it is about the value the
field holds; right after a checkbox's label, outside it, on its line; in a setting's row, beside its control — before
a button, after a switch; inline with the text in a document, where it takes the text's size.

**Several at one point are one mark**, never marks side by side: its name covers them all, and its popover says each in
a paragraph of its own. A failure and a warning at one point are one mark in the failure's tone.

A failure's mark is a red circled `!`, a warning's an amber warning
triangle, and a note's, which only informs, an accent-coloured circled `i`. Nothing about it is written beside the mark
as loose text. The mark is a button with an accessible name saying what it is about, and it has no tooltip: what it has
to say opens from it in one **popover**, the only floating thing it shows. The popover holds what happened, the one main
thing to do about it, any further explanation, then any raw text (see "Failures shown on the screen" above), and, where
the failure can be retried from there, **Try again** at its trailing end. A reason that is one short line and has
nothing more to it (an image or a formula in a document that could not be shown) is that popover with the one line in
it. A popover taller than the room beside its mark scrolls its content within itself, with **Try again** kept in view
under it, and the scrolling content is then a stop for the keyboard, so it can be scrolled from there. The exception is
a mark followed by a word that has nothing more to say (a failed import line with no reason given): it is the glyph and
the word as plain text, no button and no popover.

The popover opens by the input the device has:

- **Resting the mouse on the mark** for 250 ms opens it **without moving keyboard focus**, so the terminal or a text
  field keeps receiving keystrokes; a pointer that only crosses the mark opens nothing, and touch and pen never open
  it this way. It stays while the mouse is on the mark or inside the popover, and goes 150 ms after the mouse has left
  both; the popover's buttons and text are reached by moving the mouse into it, and its text can be selected by
  dragging with the mouse and copied, in a popover over a dialog too. A press outside closes it, and so does
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

The places with an error mark:

- a folder in the Files tree that could not be listed, after its name (see "Rows and the keyboard" in
  `docs/product/project-pane.md`);
- a project or console of a configuration import that failed, in the import's result, where the word **Failed**
  follows the mark, as the other results have their words, and where a reason was given the mark and word open the
  popover as one (see "In the window" in `docs/product/configuration-export-and-import.md`);
- a project's branch badge whose last check failed, where the mark stands in for the badge's branch glyph, and is a
  tab stop of its own on the sidebar row and in focus mode's header; Git's own message is the last thing in its
  popover and in no tooltip or name (see "The branch badge" in `docs/product/project-git-status.md`);
- an account whose quota could not be read, in Settings and the rail's quota panel (see "Quota states" in
  `docs/product/account-quotas.md`);
- in the Git mode, the committed part of the change list that could not be read — the branch not compared, or its
  commits not listed — on the Committed heading, and a list of branches that could not be read again, beside the
  branch selector (see "What the list says instead" in `docs/product/project-pane-git-change-list.md` and "The branch
  selector" in `docs/product/project-pane-git-mode.md`);
- the handoff dialog's check of the branches a session is responsible for that failed, right after the **Moves to the
  new session** heading, with **Try again** where trying again can help; a retry that fails again is announced again
  (see "Handing off from the window" in `docs/product/handing-off-sessions.md`);
- the error mark after the heading of the screens for a daemon that could not start and for a UI that failed, whose
  popover holds the daemon's message or the stack trace (see "The window chrome and the content panel" in
  `docs/product/window-layout.md`).

The places with a warning mark:

- a console or project of a configuration import whose account references fall back to a default account, after its
  name, in the review and in the result, and a project of the review not found on the machine running Octoboard,
  after its name — one mark saying both when the project has fallbacks too (see "In the window" in
  `docs/product/configuration-export-and-import.md`);
- the export dialog's **Include full project paths** checkbox while it is ticked, after its label, saying that the
  file will contain the projects' paths (see "In the window" in
  `docs/product/configuration-export-and-import.md`);
- a project removal's option to move the folder to the Trash, after its label, when what goes with the folder could
  not be looked up (see "With a project's folder moved to the Trash" in
  `docs/product/branch-and-worktree-cleanup.md`);
- the model list a session, project or console dialog could not read, after the Model field's label — in the console
  dialog, after the Model row's name — with no **Try again** (see "While the list is being fetched, and when there is
  none" in `docs/product/model-and-reasoning-effort.md`);
- the Files tab's root listing that was cut short, after the pane's title, a folder of the tree whose listing was cut
  short, after its name, and a file name search that is incomplete, inside the search field (see "Rows and the
  keyboard" and "Filtering by file name" in `docs/product/project-pane.md`);
- in the Git mode, uncommitted changes of the change list that were cut short, at the end of the Worktree line, and
  committed changes that were cut short, on the Committed heading (see "What the list says instead" in
  `docs/product/project-pane-git-change-list.md`), and the base of the comparison on screen that no longer exists,
  right after its name in the Forked from line (see "A branch that goes away" in
  `docs/product/project-pane-git-change-list.md`);
- the report panel's pages that could not be listed while it shows a page, right after the panel's title (see "Paging
  through the history" in `docs/product/report-panel.md`);
- an account whose last sharing change did not finish, in the confirmation of another session's request that would
  run a session on it: right after the account in the question of a request to resume a conversation (see "The
  confirmation dialog" in `docs/product/requests-to-resume-a-conversation.md`), and inside the account picker of a
  request to switch a session's account or hand it off (see "The user's approval of an owner's switch" in
  `docs/product/switching-session-accounts.md` and "The user's approval of an owner's handoff" in
  `docs/product/handing-off-sessions.md`);
- Settings' Updates row when the last check for updates did not finish, before **Check for updates** (see "The Updates
  and Update channel rows" in `docs/product/in-app-updates.md`);
- an account's sharing change that did not finish, after its Share default configuration switch in the account's
  settings dialog (see "Sharing the default configuration" in `docs/product/agent-accounts.md`).

The file viewer's notices about content it still shows are marks of all three tones: failures, warnings — whose amber
warning says what has to be known before reading the content (a side of a change outside the project) — and notes.
They stand after the file's path in the header for the whole file or change, beside a side's heading in an image
change, and at the spot in a Markdown document: a diagram or a block formula at the start of its source, and a
project image or a formula inside a paragraph in the text, where the mark takes the text's size and is a tab stop in
the document like a link, and one inside a link is drawn right after the link rather than in it, so pressing it does
not follow the link (see "Notices and stand-ins" in `docs/product/file-viewer.md`).

A menu has no room for a mark: a limit the menu hit is said by a last item of muted text, with no glyph, lined up with
the items' labels, that cannot be chosen and is not worded as a failure (see "Switching the local branch" in
`docs/product/project-git-actions.md`).

## Names and diagnostic details

An error that references a console, project, session, shell terminal or account by its record uses the current name
when that record is available. The name appears as an inline name chip. If the record is missing, the supplied ID
remains visible as a technical code chip; a report page, which has no display name, is identified by its ID. Names
supplied directly by the operation remain names, even when their text happens to match another record's ID. Agent
and product names referenced in the explanation use name chips too. Each name keeps its own text direction;
technical values keep left-to-right order. Accessible labels, system notifications and other text-only surfaces
retain usable plain wording with the same names or fallback IDs.

**Raw text is never part of a sentence.** A message passed through from the operating system, the daemon, Git or an
agent, a stack trace and raw entries never appear inside an alert's body, a failure's sentence on the screen, a mark's
name or the sentence in its popover, a tooltip, or an accessible name; only the names and references the wording cites
on purpose — a path, a branch name, an address — appear inside it. Raw text is shown on its own, as selectable code: in
an error alert under **Error details**, on the screen behind **Details**, and in a mark's popover as the last thing in
it. That includes known raw output from Git and agent commands, also when it is the cause of a launch or
trust-confirmation failure, an import failure, or a model or quota reading failure. A launch or trust-confirmation
failure whose cause Octoboard has no words for gives a generic cause in its sentence — for a launch, the sentence of an
uncatalogued failure (see "Failures shown on the screen" above); for a trust confirmation, that something unexpected
went wrong — and the cause's own text is the raw text. The output retains its line breaks, indentation and left-to-right
order. Long lines scroll horizontally, with a labelled region reachable from the keyboard. Raw text that stands as a
block of its own — in an alert's body, behind **Error details** or **Details**, or in a mark's popover — sits on a
tinted surface of its own rather than directly on the alert or popover holding it, with whatever names it (the
disclosure's summary, a heading) outside that surface. A crash's stack trace, which can be taller than its room, also
scrolls vertically within its block. A daemon's startup failure, written in lines of prose, wraps its long lines
instead of scrolling sideways; in its screen's heading mark, whose popover's height is bounded, it also scrolls
vertically within its block. A reason Octoboard words itself remains part of the explanation; it is not turned into a
diagnostic block simply because it accompanies an error. Unrecognized daemon messages retain the daemon's own readable
wording as their raw text, after the sentence of an uncatalogued failure (see "Failures shown on the screen" above).
These rules hold wherever the failure is shown: in an error alert, where such output is under Error details, in a
failure on the screen, where it is behind Details, or in a mark's popover, where it comes last.

## Recovery and unexpected errors

The alert distinguishes failures with a known user remedy from unexpected failures. Both use the danger presentation
and require acknowledgement.

**An actionable failure** includes a specific next step alongside its reason, any raw text staying under Error
details. References to controls such as Retry, Browse and Unlock appear as inline references rather than quoted text.
For example:

- A full disk asks the user to free space on the disk used by Octoboard and the project, then retry.
- A locked session asks the user to choose Unlock in that session's action menu, then retry.
- An invalid or unavailable path asks the user to correct it or browse to an existing, accessible location. A file or
  folder that File manager or Default application no longer finds instead says it may have been moved, renamed or
  deleted (see "A row's actions" in `docs/product/project-pane.md`).
- A folder a request is scoped to that cannot be read (`source_unavailable`) — the project's own directory, or its
  place in a worktree of its repository — is not something the user typed or picked, so its remedy asks for it to be
  put back rather than corrected, worded by the cause the daemon names. The alert's sentence says the folder cannot
  be read, naming its path; the daemon's reason is under Error details.
  - **missing**: it may have been moved, renamed or deleted, or be absent from the branch checked out there; restore
    it at that path, then try again. A daemon that names no cause gets this remedy too.
  - **link**: part of that path is a symbolic link, which Octoboard does not follow in a worktree; replace the link
    with a real folder, then try again.
  - **not a directory**: something other than a folder is at that path; put the folder back there, then try again.
  - **any other cause**, or one the window does not know: make sure the folder is still there and its disk is
    connected, then try again.
- An account name already in use asks for an unused name for that agent; an occupied destination asks for a different
  destination while preserving the existing folder.
- A clipboard failure explains granting clipboard access or using the terminal's keyboard copy and paste shortcuts.
- A failure of Git — Git could not read something (`git_failed`), or a clone, push, branch switch or file restore
  failed — asks the user to open the project's terminal from its action menu, resolve the Git error, including remote
  access or an unfinished Git operation, then retry. It offers no **Report issue**.

Known domain failures, the causes the daemon names for some of them (such as why a session's conversation could not be
moved to another account, see "When a switch fails" in `docs/product/switching-session-accounts.md`; or why a folder a
request is scoped to cannot be read), and recognized operating-system causes provide the remedy; a recognized
operating-system cause takes precedence over the others, except for a switch's copy that failed and could not put back
an older item: that is classified by the item put aside, and the operating-system cause's remedy is given as well; and
except for a file the user picked that could not be read, which keeps its own remedy for reading a file (see "In the
window" in `docs/product/configuration-export-and-import.md`), whatever operating-system cause it names. A failure whose
named cause is not one the window knows stays unexpected, except a folder a request is scoped to that cannot be read,
which gets the remedy for any other cause (see above). An unfamiliar failure does not get generic retry advice merely
because it occurred during a familiar operation.

**An alert whose body is the daemon's message, where that message already says what to do**, adds no remedy line of
its kind after it: the message's own step is the remedy — "Reopen it first." for an archived session in a handoff, the
step of a launch failure whose configuration directory cannot be reached, the step of `directory_unreadable`. Its
classification, and any remedy button it has (see below), stay as they are. An alert whose body Octoboard words for the
operation keeps its kind's remedy line.

**A remedy that lives in Settings' Agent accounts section has a button** in the footer, a secondary action before the
acknowledgement button, instead of the step naming where to go:

| Failure | Button | What it opens |
|---|---|---|
| The agent is not added (`agent_not_added`) | **Add agent…** | Settings on Agent accounts, with the Add agent dialog in front (see "Adding an agent" in `docs/product/agent-accounts.md`) |
| An account problem (`unknown_account`, `grok_home_not_initialized`) | **Manage accounts…** | Settings on Agent accounts |
| An account that is turned off (`account_disabled`) | **Manage accounts…** | Settings on Agent accounts, where it can be turned back on (see "Turning an account off" in `docs/product/agent-accounts.md`) |
| The added agents not being as the change expected (`agent_already_added`, `agent_order_invalid`) | **Manage accounts…** | Settings on Agent accounts |

Pressing the button acknowledges the alert and opens Settings there, above whatever dialog was open; when Settings is
already open, it moves to that section instead (see "Opening and closing Settings" in `docs/product/settings.md`). The
step in the alert's body then says only what to do, such as adding the agent and trying again.

**A plain alert** is a known refusal that says what happened in its own sentences, with no remedy line added after
them, no request to report an issue and nothing but its acknowledgement button. It is used for a switch confirmed onto
an account that was turned off or used up after the confirmation opened (see "Confirming a switch" in
`docs/product/switching-session-accounts.md`), and it is the form of every failure modal (see "The failure modal"
above).

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
If opening the page fails, a separate actionable error immediately appears above the original error, titled "Could not
open the Issue page", saying that the browser did not open the Issue page and so its address is shown below, with the
address as selectable text on a line of its own and instructions to open it manually; the reason the system gave, where
it gave one, is under that alert's **Error details**. Acknowledging that failure
returns to the original pending error. Any other link Octoboard has the system open in the browser that does not open
is reported the same way, in an alert of its own titled "Could not open the link", with its address on a line of its
own and the remedy to check that the application it needs is installed or to open the address manually. Where the
desktop platform cannot open external links, the guidance includes the address as a selectable chip for manual use.

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
