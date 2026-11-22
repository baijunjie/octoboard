# Values a newer daemon sends

A window may show a daemon newer than the window's own build that speaks the same protocol generation, such as a remote
window whose daemon was updated before this application (see "A version mismatch" in `docs/product/remote-daemons.md`;
a daemon of another generation is refused instead). Within one generation a daemon may add values to what it reports —
a new session status, a new kind of quota window, a new choice of a setting — which the window does not know. The window
keeps working, and such a value:

- **is never shown by its wire value**, and never explained with the words of another state;
- **decides nothing**: no action is offered or enabled because of it, and it does not count a record as running, at
  work, blocking or used up, but for the clean-up before uninstalling, where it keeps the action available (see "The
  clean-up before uninstalling" below);
- **leaves its record shown** as the sections below say.

## Sessions

- **A status the window does not know** is *status unknown*: see "Status unknown" in
  `docs/product/session-statuses.md`.
- **A session of a kind the window does not know** that belongs to no project is listed among its console's console
  sessions, for display only: none of a console session's own features, such as its colour, is offered for it. One that
  belongs to a project is listed under that project as usual.
- **A session request** of an action or a session kind the window does not know is not offered (see "The confirmation
  dialog" in `docs/product/session-requests.md`).

## Entries left out and labels left blank

An entry of a list whose kind the window does not know is left out; a label within an entry that is still shown is left
blank, the entry keeping the rest of its content:

- **A quota window** of an unknown kind is not shown, and never counts toward an account being used up. An account
  whose only windows are of unknown kinds reads as one with no windows (see "Quota states" in
  `docs/product/account-quotas.md`).
- **A change in the Git change list** of an unknown group is left out of the list. In the change viewer, a conflict of
  an unknown kind is named as the conflicted section is, a side of an unknown kind is shown as a file, and a side in an
  unknown state as absent.
- **A message of the session history** from an unknown speaker is shown as plain text with no speaker name above it,
  and not as the user's.
- **A configuration import**: a progress report about an item of an unknown kind is not listed or counted, and a
  Settings section of an unknown name gets no card and is not applied; an item's progress state or outcome the window
  does not know leaves its status blank, and a project created in an unknown way has no detail of how. In the review, an
  account of an unknown status shows its name alone, with Leave out as its only choice, and a project whose preselected
  way the window does not know starts left out until the user chooses a way.
- **A branch or worktree** of an unknown kind or state in the cleanup of an archive or a deletion is not named back to
  the daemon, which then leaves it as it is.
- **A scheduled task** whose schedule is of an unknown kind shows no schedule, and its form leaves the schedule as it
  is until a known kind is picked; a run's unknown trigger is left blank, and an unknown outcome shows nothing of what
  came of the run (see "Values a newer daemon sends" in `docs/product/scheduled-tasks-page.md`).

## Settings holding an unknown value

A setting whose current value the window does not know keeps its row, showing an empty value; picking a known value
writes it, and nothing is written until then. This holds for Development and Delivery in Git settings (see
`docs/product/git-settings.md`), the update channel (see "Update channel" in `docs/product/update-settings.md`), When to
prevent sleep (see `docs/product/keeping-the-computer-awake.md`) and a project's own git workflow values in the project
dialog (see "A project's own git workflow settings" in `docs/product/git-workflow-settings.md`). Development and When to
prevent sleep, whose description describes the chosen value, show no description meanwhile. In the project dialog,
"Same as global setting" shows no badge while the application-wide value is one the window does not know.

A provider setting of a kind the window does not know is not shown, and a provider switch whose declared default the
window does not know reads as off until it is set.

In the import review, a git workflow line whose wanted value is unknown is left out, and one whose current value is
unknown shows the wanted value alone; When to prevent sleep's current value is treated the same way (see "Settings in
the review" in `docs/product/configuration-export-and-import-dialogs.md`).

## The clean-up before uninstalling

No window receives the daemon's list over the protocol: the desktop application's main window gets this computer's
daemon's items from the application, which asks its own daemon for them on a local route, and the running daemon
that answers may be of another build than the application. An item of a kind the application does not know is not
listed, counted or named: it is reported beside the known items as the daemon's part not read in full, so the row says
that not everything in effect could be read, the confirmation shows the alert **Could not read what the daemon
added**, and **Remove all** stays available because of it (see "The row" in
`docs/product/clean-up-before-uninstalling.md`). Clearing asks the daemon to clear it with the rest, the daemon knowing
what it is; the result reports it the same way, under **Could not remove what the daemon added**, some of it possibly
removed.

## Values read as a known one

- **The daemon's own update status**: a phase the window does not know is taken as idle, so nothing is offered to
  install, download or restart for, the running version is shown and a check can be made; a last check that ended in an
  outcome the window does not know reads as neither failed nor up to date; an update found on a channel the window does
  not know is not taken as found on the chosen one.
- **An agent CLI's install method** the window does not know is an unknown install, which offers no Update (see "The
  Version row" in `docs/product/agent-cli-updates.md`).
- **MCP servers and connectors**: a state or a reason the window does not know, see
  `docs/product/account-mcp-servers-and-connectors.md`.
- **Report pages**: a component of an unknown kind is not shown (see "Checking a page when it is pushed" in
  `docs/product/report-page-components.md`), and a callout's tone and a stat's trend and sentiment fall back as
  "Content" in the same doc says.
- **A file path in a terminal** the daemon answers about with an unknown kind is not a link.
- **A file body** of an unknown kind is shown as a binary file is.
- **A project's source** the window does not know is edited as a local project's.
- **An agent's default model** from an unknown source reads as none configured.
- **Whether a session's records exist on disk**, answered with a value the window does not know, is unknown, which
  leaves a deletion's cleanup option available.
- **The daemon host's path style** the window does not know is the Unix style; an operating system it has no name for
  is written in an issue report as the daemon sent it.

## Outcomes and reasons

- **Redeeming a usage limit reset** with an outcome the window does not know: see "How a redeem ends" in
  `docs/product/usage-limit-resets.md`.
- **A login** reaching a status the window does not know closes the login dialog, not as a success.
- **Remote access or browser access failing** for a reason the window does not know shows its error with no reason in
  the application's words, the system's own words still behind its details (see "When remote access cannot be served"
  and "Allow browser access" in `docs/product/remote-access-settings.md`).
- **Moving an account directory to the Trash**, refused for a directory shared with something of a kind the window does
  not know: see "Checking the directory when the option is ticked" in `docs/product/removing-accounts.md`.
