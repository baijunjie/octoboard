# Dropping files onto a terminal

Files dragged from the system's file manager and dropped onto a terminal paste their paths into it, as a system
terminal pastes the paths of files dropped onto it, so an agent reads a dropped file with its own tools, and an agent
that takes pasted images takes a dropped image file as an attached image. Where the window runs on the machine its
daemon runs on, the files' own paths are pasted; elsewhere the window first uploads each file to the daemon's host and
pastes the path it was saved at.
Pasting an image from the clipboard is in `docs/product/pasting-images.md`, pasting text in
`docs/product/terminal-copy-paste-and-selection.md`.

## What is pasted

- **One paste for the whole drop**, as typed input subject to the program's bracketed paste mode, like any paste.
- **Each path** is pasted bare, or, when it contains whitespace, in single quotes as a POSIX shell quotes it (a single
  quote inside it written `'\''`), as a pasted image's path is (see "What is pasted" in
  `docs/product/pasting-images.md`).
- **The paths are joined by single spaces and followed by one more space**, so what is typed next does not run into the
  last path.

## In the desktop application's main window

In the main window of the desktop application, whose daemon runs on this computer, the system hands the window the
dropped files' paths on this computer, and those are pasted as they are, nothing being uploaded:

- **Every terminal that can take input takes a drop**: the selected session's terminal and the shell terminals of the
  Terminal panel (see `docs/product/terminal-panel.md`). A terminal that cannot take input at that moment, such as one
  under a cover or reconnecting, takes nothing.
- **Folders are pasted too**, by their paths, like files.
- **The drag shows the system's copy cursor (a plus) over the whole window**, the terminals that refuse a drop and the
  rest of the window included: the system, not the window, decides the cursor here. Only a terminal that takes the drop
  shows the drop target (see "The drop target" below), and a drop anywhere else does nothing.

## In a remote window and in a browser

A remote window of the desktop application (see "The remote window" in `docs/product/remote-daemons.md`) and a browser
share no file system with the daemon, so a dropped file is uploaded to the daemon's host, as a phone attaches one (see
`docs/product/attaching-files.md`), and the path the daemon saved it at is pasted.

### Which terminals take a drop

- **Only an agent session's terminal**: the terminal of the selected project or console session, while it can take
  input. A shell terminal of the Terminal panel refuses the drag (the drag shows that nothing can be dropped there), and
  so does a terminal under a cover.

### How each file is sent

Each file is sent in the way that lets the agent take it best:

- **A PNG, JPEG, GIF or WebP image of up to 10 MiB**, for a session whose agent takes pasted images (see "Which pastes
  paste an image" in `docs/product/pasting-images.md`), is saved as a pasted image (`save_pasted_image`, see "The
  request" in `docs/product/pasting-images.md`), so the agent takes its path as an attached image.
- **Any other file**, an image in another format or over 10 MiB included, and any image dropped onto the terminal of a
  session whose agent takes no pasted images, is attached: uploaded in parts and saved in
  the session's scratch directory (see "Uploading and saving a file" in `docs/product/attaching-files.md`), up to the
  largest file the daemon takes, `max_bytes` of the `snapshot`'s `attachments`, 100 MiB today (see "Limits" in
  `docs/product/attaching-files.md`). It is not converted.
- **A daemon that takes no attachments** (its `snapshot` carries no `attachments`, see "Whether a daemon takes
  attachments" in `docs/product/attaching-files.md`) takes only the images of the first kind; every other file is
  refused.

### Files refused

A file is refused, before anything of it is sent, when it is:

- a folder: only files can be attached;
- empty;
- larger than the daemon's limit for an attachment;
- not saved as a pasted image (as "How each file is sent" above sets out), where the daemon takes no attachments: the
  reason says that the daemon the session runs on is too old to take anything but PNG, JPEG, GIF and WebP images of up
  to 10 MiB.

A file whose bytes cannot be read on the way, and one the daemon fails to save, are refused as well. A refusal does not
stop the drop: the drop's other files are still sent, and the paths of those that arrived are pasted together once the
last one is done.

The files refused in one drop are told in one error alert (see "The error alert" in `docs/product/error-alerts.md`),
once the drop is done, with no remedy:

- **One file refused**: the alert says the file could not be attached, and its body gives the reason, naming the file.
- **Two or more**: the alert says the files could not be attached; its body leads with a sentence saying these files
  were not attached, each for its own reason, followed by a list with one row per file. A file refused before it was
  sent has the reason naming it; one the daemon failed has its name, that it could not be attached, then the daemon's
  failure. A long name wraps.

A failure that concerns the session rather than the file, the session no longer running (`session_not_running`) or no
longer known (`unknown_session`), ends the drop: its remaining files are not sent, the files refused so far are told as
above, and the session's failure has an alert of its own saying the file could not be attached.

### The destination is checked again

The paths are pasted only once the drop's last file is done, which can take long, and only when the same session is
still selected and can still take input; otherwise nothing is pasted, and the paths are never pasted into another
session or terminal. The same check is made before each file is sent: once the session is no longer selected or cannot
take input, the drop's remaining files are not sent, and nothing is told, neither the files refused so far nor the ones
left. Files already saved stay on the daemon's host until removed with the session's scratch directory (see "Where the
file is kept" in `docs/product/attaching-files.md` and "Where the image is kept" in `docs/product/pasting-images.md`).

Typing does not wait for the uploads: what is typed meanwhile reaches the agent before the paths.

### One file at a time

All the window's drops share one queue: files are sent one at a time, across every drop onto any session's terminal,
and a drop made while others are under way waits for them to end.

### The upload notice

While files dropped onto the selected session's terminal are queued or under way, a one-line notice floats centred over
the top of the terminal, which leaves the terminal's last rows, where an agent's input and cursor are, clear. It is not
shown for a session without a running process, which shows its own notice at the bottom instead.

- **A progress ring** shows the share of the file under way that the daemon has acknowledged, part by part. It spins
  where that cannot be told: for an image saved as a pasted image, which goes in one message, and while the session's
  files wait for another drop to end.
- **The sentence** says that a file is being uploaded, its place among the files queued for the session, how many
  there are, and the file's name. The count takes in every drop onto the session's terminal since its uploads were last
  cancelled. The name stays on the line: where it does not fit it fades out at its end, and hovering it then shows the
  full name in a tooltip.
- **Cancel**, a small button, whose accessible name says it cancels the uploads.

The notice is not spoken as its ring moves: the sentence is announced when it changes, so once for each file and when
more files join the count.

**Cancel** stops the session's uploads: a file being attached stops before its next part and its upload is discarded
(an image already on its way as a pasted image is still saved), no further file of the session is sent, nothing is
pasted, and the notice goes. The files refused before Cancel are still told. A drop made after Cancel is counted
afresh, from one.

**Keyboard**: the terminal keeps `Tab`, so the notice is a stop of its own in the `F6` cycle, just before the terminal,
while it shows (see "Moving focus between regions with F6" in `docs/product/moving-focus-between-regions.md`); `F6`
lands on Cancel. When the notice goes while Cancel holds keyboard focus, focus returns to the terminal.

## The drop target

While a file drag is over a terminal that would take the drop, the terminal shows an inset accent-coloured ring around
its edge and a soft accent tint over its whole area; the tint tells it apart from the keyboard focus ring, which is the
ring alone. A terminal that would refuse the drop shows neither, and neither does a terminal under a cover.

## A drop anywhere else

A file dropped anywhere else in the window or browser tab, outside a terminal that takes it, does nothing: the page is
never replaced by the dropped file. In a remote window and a browser the drag there shows that nothing can be dropped.
