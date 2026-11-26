# The message composer

An optional editor below the session terminal, where the user writes a whole message to the session's agent, attaches
images and files to it, and sends it in one piece, at once or onto the session's queue, without what Octoboard or
another session writes into the terminal meanwhile getting in the way. The queue a message goes onto, shared by every
client, how and when the daemon sends what is on it, and how its messages are managed, are in
`docs/product/composer-queue.md`.

## Turning the composer on

**Message composer**, a switch in Settings' General section, in its Terminals group (see "Message composer" in
`docs/product/general-settings.md`), is **off to begin with**. It is kept by each client for itself, never by the
daemon: every window of the desktop application, its remote windows included, follows the application's choice, a
change in one window taking effect in the others at once, while a browser keeps its own, so a phone can use the
composer while the Mac uses the terminal. A configuration export that includes the General section carries it (see
"The settings a client holds" in `docs/product/settings-in-a-configuration.md`).

While it is on, the composer shows below the terminal of the selected session, taking at most 60% of the column's
height, when:

- a session is selected and it is not archived (an archived session shows its history and no composer); and
- the daemon keeps a composer queue: a daemon that predates it (see `docs/product/composer-queue.md`) gets no composer,
  whatever the switch says.

**The terminal still takes typing directly** with the composer on: answering an agent's menus, permission prompts and
questions still happens there. While the archive view covers the terminal, the composer is covered with it and takes
nothing.

While the switch is off, a session with queued messages shows a compact line over its terminal instead (see "The queue
line with the composer off" in `docs/product/composer-queue.md`). Turning the switch off ends every edit of a queued
message under way in that window (see "Editing a queued message" below).

## The editor

From top to bottom the composer holds the session's queued messages, while it has any (see "The queue in the
composer" in `docs/product/composer-queue.md`); a note on the session's state, while one applies; the files attached
to the message; the text area; and a row of controls.

- **The text area**, named **Message**, three lines high, takes the message's text; its direction follows what is
  typed. It is where `F6` lands in the composer (see "Moving focus between regions with F6" in
  `docs/product/moving-focus-between-regions.md`).
- **The note**: while the session waits on the user, that the agent is waiting for an answer in the terminal, which is
  also said once by screen readers as the session starts waiting under the open composer; while the session has no
  process (interrupted), that queued messages wait until it is resumed and that sending now resumes it.
- **Attach files**, an icon button that opens the system's file picker, several files at once (see "Attaching files"
  below).
- **When to send**, a selector of the session's default send mode: **Send now**, **When idle** or **Manual** (see
  "Send modes" in `docs/product/composer-queue.md`). It shows the mode the daemon holds for the session, and choosing
  another changes that default for every client; a failure raises an error alert saying when messages are sent could
  not be changed. A mode this build does not know shows as none chosen, and a message sent meanwhile takes the daemon's
  default. The selector is hidden while a queued message is being edited.
- **The send button**, labelled for what it does with the chosen mode: **Send** for Send now, **Add to queue** for When
  idle and Manual, and **Save** while a queued message is being edited, with **Cancel** beside it. Its tooltip shows
  the key that sends.

**Keys**: ⌘ Enter on macOS, Ctrl+Enter elsewhere, sends, as the send button does; Enter starts a new line. A key
pressed with Shift or Alt as well, or one an input method is composing with, never sends.

**The text being written is the window's own.** It is kept per session while the user moves between sessions, and
while the composer is turned off and on again; it is not on the queue, no other window or client sees it, and reloading
the window loses it.

## Sending a message

The message sent is the text with its leading blank lines and its trailing whitespace dropped, indentation kept,
together with the attached files. The send button and its key do nothing, the button being unavailable, while there is
neither text nor an attached file, and while an attached file is still on its way to the daemon. A message carries at
most **10 images**: with more attached, the button is unavailable, saying under its name that a message carries at most
10 images and to remove some.

**A lone slash command goes as a command**: text that is one line starting with `/` and a name that holds no second
`/`, optionally followed by arguments, with nothing attached, goes as a slash command for the agent to run where its
agent takes slash commands (see "Sending a slash command" in `docs/product/slash-commands-and-image-messages.md`), and
as text otherwise.

Sending hands the message to the session's queue with the chosen mode: with Send now it goes to the session's message
delivery at once; with When idle or Manual it joins the end of the queue (see "Sending a queued message now" and "Send
modes" in `docs/product/composer-queue.md`). This works for a session with no process too: a queued message waits for
it to be resumed, and Send now resumes it. While the message is on its way the text area is read-only. Once the daemon
has taken it, what was sent leaves the editor; text that Quote or Add as context added meanwhile (see "Text from
elsewhere: Quote and Add as context" below) stays. A refusal leaves everything in the editor, so nothing typed is lost,
and raises an error alert saying the message could not be sent, with the reason (see "The error alert" in
`docs/product/error-alerts.md`).

## Attaching files

Files are attached with **Attach files**, by pasting files into the text area, or by dropping them onto the composer,
which shows the same drop target as a terminal while a drag over it would be taken (see "The drop target" in
`docs/product/dropping-files-onto-a-terminal.md`); a composer covered by the archive view takes no drop.

- **A file dropped in the desktop application's main window**, whose daemon runs on this computer, is attached by its
  own path on this computer, as a file the message names; nothing is uploaded, and an image goes as a file too.
- **Every other file** — a file picked or pasted in any window, or dropped in a remote window or a browser — is sent to
  the daemon as a file dropped onto a terminal in a remote window is, one at a time in the window's queue of uploads
  (see "How each file is sent", "Files refused" and "One file at a time" in
  `docs/product/dropping-files-onto-a-terminal.md`): a PNG, JPEG, GIF or WebP image of up to 10 MiB, for a session
  whose agent takes pasted images, is saved as an image of the message, which the agent takes as an attached image;
  any other file is attached and named by its path. A session with no process takes them too (see "Files of queued
  messages" in `docs/product/composer-queue.md`).

Each file shows at once in the composer's list of **attached files**, with a wait indicator until it has arrived; one
refused or cancelled leaves the list, its failure told as a dropped file's is, and removing one still on its way stops
the files after it. Each file is a tag with its name, an **Image** badge for an image the message carries as one, and a
remove button. The list is one tab stop: the arrow keys move between the tags, Delete or Backspace removes the one
focused, and Enter or a press opens it in the file viewer (see `docs/product/file-viewer.md`), read from the daemon's
host by its path as a path link in a terminal is, keyboard focus going back to the tag as the viewer closes (or to the
text area where the tag has gone meanwhile). A tag still on its way opens nothing.

How the images and files reach the agent when the message is sent is in "Sending" in
`docs/product/composer-queue.md`.

## Editing a queued message

**Edit**, in a queued message's menu (see "The queue in the composer" in `docs/product/composer-queue.md`), puts the
message's text and attached files in the editor, which is then named **Queued message**, and marks the message's row as
being edited; the new message that was being written is set aside until the edit ends. Editing another message ends
this edit first as Cancel does, the set-aside message staying aside.

- **A message waiting to go When idle is held as Manual while it is edited**, so the daemon does not send it
  half-changed; its mode comes back when the edit ends. Choosing a mode in its menu during the edit is recorded and
  applied when the edit ends. Send now is unavailable on the message being edited.
- **Save** stores the text and the attached files to the queued message, ends the edit and brings the set-aside message
  back, followed by anything added to the editor while the save was on its way. A refusal keeps the edit and raises an
  error alert saying the queued message could not be changed.
- **Cancel** ends the edit without saving and brings the set-aside message back.
- **A message that leaves the queue during the edit** — sent, withdrawn from another client, or dropped — ends the edit:
  what the editor holds stays, as a new message after the set-aside text.
- **Turning the composer off** ends every edit under way in that window as Cancel does, so a message held as Manual gets
  its mode back rather than waiting unseen.

## Mentioning a file or a session with @

Typing `@` at the start of the text or after whitespace opens a picker under the text area, filtered by what follows
the `@` up to the caret; an `@` inside a word, as in an email address, opens none. The picker lists, under a heading
each, the files and directories of the session's project (none for a console session), then sessions.

- **Files and directories** are asked of the daemon 150 ms after the last keystroke. For text with no `/`, the picker
  offers the entries of the project's top directory whose name contains it, directories first, then the files a
  project-wide file-name search finds (the one Find in files' file-name filter runs, see `docs/product/finding-in-files.md`)
  and the directories above them whose name contains the text. For text with a `/`, it offers the entries of the
  directory named before the last `/` whose name contains what follows it. At most 30 are listed. While a search is
  under way the last results stay, with a note that files are being searched; a failed search says that project files
  could not be searched, and none found that there are no matching files.
- **Sessions** are every unarchived session other than this one whose title as shown contains the text, ignoring
  case: those of the same project first, then those of the same console, then those of other consoles, the most
  recently started first within each, at most 30, each with where it is. None matching says so.

The Up and Down arrows move through the choices, Enter or Tab picks the one moved to, and Escape closes the picker
until another `@` is begun; a press picks a choice too, and leaving the text area closes the picker. A picked choice
replaces what was typed from the `@` to the end of its word, followed by a space unless whitespace follows already,
with the caret after it:

- **a file or directory** inserts its path relative to the project's root, bare, or in single quotes as a POSIX shell
  quotes it when it contains whitespace, so a session working in a worktree reads the same file on its own branch;
- **a session** inserts `@「<title>」(session <id>)`, its title as shown and its full id, which a person can read and an
  agent can pass to Octoboard's session tools.

## Text from elsewhere: Quote and Add as context

While the composer shows for the session the window shows, the actions that write into a session's input write into
the composer instead of the terminal, without sending anything:

- **Quote** in the session terminal's editing menu (see "The editing menu" in
  `docs/product/terminal-copy-paste-and-selection.md`) writes its block quote into the composer, keyboard focus staying
  where the menu leaves it. It is offered whenever there is text to quote, whatever the terminal could take.
- **Quote** in the file viewer's text menu and the Find in files preview (see "Selecting and copying" in
  `docs/product/file-viewer.md`) writes its block quote into the composer of the session the window shows; once the
  dialog has closed and the terminal is uncovered, keyboard focus goes to the composer's text area. It is offered
  whenever text is selected.
- **Add as context** on a file or folder of the Files mode (see "A row's actions" in `docs/product/project-pane.md`)
  writes the entry's path, relative to the project's root and quoted as a picked file's is.

The quote is built as everywhere (see "What Quote writes" in `docs/product/terminal-copy-paste-and-selection.md`), but
the rules that read the terminal's screen and its bracketed paste mode do not apply. What is written goes at the end of
the composer's text, on a line of its own: after a line break where the text does not end with one; a composer holding
only whitespace is replaced. It is written also while the composer is not on screen, and is there when it next shows.
