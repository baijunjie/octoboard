# Dialogs

How a dialog — a form dialog, a confirmation, an error alert or a failure modal — is titled, where keyboard focus starts
in it, where a question the user did not ask for stands among other dialogs, what its confirm button says, what Enter
does in it, what it and its controls do while a request it started runs — with how a button anywhere in the window shows
that its action is running — and how it changes size while open. What an open dialog puts
out of reach in the rest of the window is in "What an open dialog, menu or popover covers" in
`docs/product/window-layout.md`; error alerts and failure modals themselves are in `docs/product/error-alerts.md`.

## A dialog's title

**A dialog's title is a plain name** for what it does or what failed — "Switch account", "Delete session", "Cannot open
a session" — the same words whatever it is opened on, for every form dialog, confirmation, error alert and failure
modal. It names no session, account, project, branch or path and holds no count: what the dialog is about is named by
the first sentence of its body ("The session Fix search will be switched to Work."), everything else following it. An
account's two dialogs are titled "Account details" and "Account settings", the body's first line naming the account
(see "The account details dialog" and "The account settings dialog" in `docs/product/agent-accounts.md`). A
dialog that shows one piece of content rather than asking or reporting anything, the file viewer, keeps the name of what
it shows as its title (see `docs/product/file-viewer.md`).

The one exception is the confirmation of a setting choice that raises risk, such as Claude Code's "Approve everything":
its title asks the question ("Approve everything?", 「确认全部批准？」), since what it confirms is a standing choice
the user is about to grant, not an action on a subject. It still holds no variable content. Destructive confirmations
(deleting, removing, archiving) keep the plain name.

## Destructive confirmations

**A destructive confirmation is a warning modal**: the warning icon leads its header, and what the action does and takes
along is its body text — the sessions it ends, archives or deletes along with its subject listed there, and a line for
each option ticked (see "The confirmation" in `docs/product/branch-and-worktree-cleanup.md`) — never an alert inside the
dialog. That is every confirmation whose confirm button is drawn in the danger colour (see below), and the archive
confirmation, which carries the warning icon even before an option makes it destructive. The unbind confirmation is a
warning modal too, with a consequence line in its body, though it is not destructive: its confirm button is the primary
one and keyboard focus starts on it (see "Unbinding a session from the window" in
`docs/product/changing-session-owners.md`).

## Where keyboard focus starts in a confirmation

A confirmation opens with keyboard focus on its confirm button, so Enter confirms at once. Three kinds start elsewhere:

- **A destructive confirmation** — one whose confirm button is drawn in the danger colour, such as quitting with
  sessions running, removing a project, deleting a session or removing an account — opens with focus on the dialog
  itself, so Enter does nothing until the user moves focus onto a control. Archiving a session is destructive only while
  one of its deletion options is ticked (see "What a confirmation's confirm button names" below); it opens with them
  unticked, so focus starts on its confirm button, and becoming destructive once one is ticked does not move focus back
  onto the dialog.
- **A confirmation that asks for a word to be typed** opens with focus in its text field.
- **A question the user did not ask for** — a folder trust prompt (see "The trust dialog" in
  `docs/product/folder-trust.md`) or a session request (see "The confirmation dialog" in
  `docs/product/session-requests.md`) — opens with focus on the dialog itself as well, and focus is put back on the
  dialog each time it moves on to the next waiting question, so an Enter held past one answer does not answer the next;
  not while a dialog opened after it covers it (see "Where a question the user did not ask for stands among other
  dialogs" below).

## Where a question the user did not ask for stands among other dialogs

A question the user did not ask for — a folder trust prompt or a session request — appears above any open dialog and
takes keyboard focus, including a dialog opening in the same moment, even one that another dialog opens as it mounts
(**View in Settings** opening Settings, which opens the account settings dialog). An error alert that comes up in the
same moment is the exception: the alert goes on top and the question waits under it, taking focus once the alert is
acknowledged.

A dialog that opens later, while the question is up — an error alert among them — goes on top of it and takes keyboard
focus. The question waits under it, out of reach; moving on to the next waiting question meanwhile does not take focus
from that dialog. Closing that dialog puts focus back in the question.

When the last waiting question closes and keyboard focus has nowhere to return to — as when the question came in the
same moment a press without a pointer opened the dialog under it, which never had focus — focus goes where it goes when
the last error alert is acknowledged with nowhere to return to: into the dialog still open, on the control that dialog
opens on, else to the selected session's terminal, the sidebar or the rail (see "The error alert" in
`docs/product/error-alerts.md`). Where focus had a control to return to when the question came, it goes back there.

## What a confirmation's confirm button names

A confirmation's confirm button carries one short label for its action, never a list of actions. Where the confirmation
offers options, the label changes at most once, to say the action now deletes more, and changes back when no such option
is ticked:

- **Archiving a session** reads **Archive**, and **Archive and delete** while an option that deletes is ticked —
  force-removing the worktrees and branches Git keeps, or deleting the locked worktrees. While such an option is ticked,
  the button is also drawn in the danger colour, making it a destructive confirmation; with none ticked it is an
  ordinary confirm button.
- **Deleting a session** reads **Delete**, and **Delete all** while any of its extra deletion options is ticked — moving
  the agent's own session records to the Trash, force-removing the worktrees and branches Git keeps, or deleting the
  locked worktrees.
- **Deleting all archived sessions** always reads **Delete all**, which already says all; its options leave it as it is.
- **Removing a project** always reads **Remove**, or **End and remove** when the project has sessions running; its
  options leave it as it is — moving the project folder to the Trash, and the locked worktrees with it — since the
  message then says the folder goes.
- **Removing an account** always reads **Remove**; its option of moving the account directory to the Trash leaves it
  as it is, since the message then says the directory goes (see "The confirmation" in
  `docs/product/removing-accounts.md`).

What a ticked option adds is said in the confirmation's message and in the lines under its options, not on the button.
While the action runs, the button shows as pending under the label it already has (see "A button while its action
runs" below).

## A button while its action runs

**A button whose action is running shows as pending**: a spinner comes before its label, which stays as it was — no
button changes its words to say what is under way. An icon-only button shows the spinner in place of its icon. A pending
button cannot be pressed, keeps keyboard focus when it has it, and is reported to assistive technology as busy; the
spinner itself is not announced. This holds for every button in the window, in a dialog or not, and for a link drawn
as a button while what it opens is being opened.

## A dialog cannot be left while its request runs

**While a request a dialog started is running, the dialog is busy: it cannot be left, and nothing in it can be used but
the button that started the request.** Escape and a press outside it do nothing, and every other control in it is
disabled — **Cancel**, the close button and the other buttons, and the fields, pickers, checkboxes, radio buttons,
switches, tabs, menu and disclosure triggers, tags, list items and links of its body. The button that started the
request shows as pending (see "A button while its action runs" above). The disabled controls stay where they are and
show their own disabled look; nothing is laid over the dialog, and assistive technology still reads them, as
unavailable. A link in a busy dialog cannot be followed, by a click, a middle click or the keyboard, and Tab passes over
it; it is still read as a link, unavailable.

It applies to every form dialog and confirmation that sends a request — renaming, the console, session and project
dialogs, handing a session off, the account and Add agent dialogs, exporting and importing a configuration, and every
confirmation — and lasts until the request has been answered, whether it succeeded or failed. The console dialog counts
an avatar image being read after it was picked as such a request. The export dialog's save dialog is part of its
request, so the export dialog is busy while the save dialog is open (see "The export dialog" in
`docs/product/configuration-export-and-import-dialogs.md`). Waiting on something the user can still walk away from is
not such a request, and leaves the dialog operable and closable: the import dialog's file picker (see "Choosing the file
to import" in `docs/product/configuration-export-and-import-dialogs.md`), a check a confirmation makes before its action
can start, which holds its confirm button pending meanwhile but disables nothing else, or a login, which closing the
dialog cancels (see `docs/product/logging-accounts-in.md`).

**Keyboard focus stays in a busy dialog.** When the request starts with focus on a control it disables, focus moves to
the pending button, or to the dialog itself where there is none; focus on the pending button or on the dialog stays
where it is. When the request ends with the dialog still open, as after a failure, focus goes back to the control it was
moved from; where the failure opened an error alert, it does so once the alert has been acknowledged. Where the user pressed
somewhere in the dialog, or pressed Tab in it, while the request ran, focus is left where they put it instead. Where
that control is gone or disabled by then, focus that would be left on nothing goes to the button that was pending, or
to the dialog. A dialog whose refusal concerns one field returns focus to that field instead, however the request was
started: the session dialog's Session ID field after a refused resume (see "Opening a new session on a found
conversation" in `docs/product/resuming-a-conversation-by-its-session-id.md`).

## Where keyboard focus starts in a form dialog

A form dialog opens with keyboard focus in its first text field, so the user can type at once and Enter submits the
form, however it was opened — by the mouse, the keyboard, or a press without a pointer such as one from assistive
technology: the Name field of a console, session or account dialog and of editing a project, the Directory field of adding a
project, the field of a rename dialog, and the path field of the application's directory browser, where Enter lists the
path typed. A field that appears only after a choice in the dialog does not take focus when it appears: switching a project session's
dialog between its **New conversation** and **Resume conversation** tabs leaves focus on the tab, and switching **Add
project**'s **Source** to a git repository leaves it on that picker. A form dialog with no text field opens with focus
on its submit button instead: the export dialog's **Export** button, and the import dialog's **Choose file** button in
its first step (see "The export dialog" and "Choosing the file to import" in
`docs/product/configuration-export-and-import-dialogs.md`).

## What Enter submits in a form dialog

Enter submits a form dialog from a control that holds a choice as well as from a text field or a checkbox: a closed
picker (such as the agent, account, console-session or model picker), a tab (such as **New conversation** and **Resume
conversation**), a radio button or a switch. Space and the arrow keys still open or operate the control, and Enter in a
picker's open list still picks the option. A button that acts, such as **Cancel**, a disclosure such as **Advanced** or
a close button, keeps Enter for itself. While the submit button is disabled or the dialog is busy, Enter submits
nothing.

## A held Enter or Space does not confirm

Every confirmation and every form dialog ignores the auto-repeats of a held Enter or Space, so the press that opened it
(a menu item acts as the key goes down) cannot go on to confirm it; a fresh press still does. Where text is being typed
the repeats stay: Space in any text field, Enter in a multi-line one.

## A dialog whose content changes its height

**A dialog whose content changes its height while it is open eases to the new height** — a tab switched, a section
opened, a failure line appearing — over **200 ms**, growing or shrinking about its centre. Its first size and a change
of the window's size are not eased, and where the system asks for reduced motion the height changes at once. Every
dialog does this except Settings.
