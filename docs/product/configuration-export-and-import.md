# Configuration export and import

Octoboard can lift its configuration out of one machine and build it again on another. What travels is a **configuration
document**: one console or every console, the projects associated with them and, for a full export, the application
settings. Nothing of what a machine is doing goes with it, so what arrives on the other side is the layout of consoles
and projects, with its agents still to be signed in and its directories still to be trusted.

A **machine**, here, is the host the daemon runs on, which is where the projects, the accounts and the default clone
directory are. The window is not always on it: the desktop app's is, but a browser tab reaching a daemon on a server is
on another machine. So a path, a directory or an account this document speaks of is the host's, never the browser's, and
the window's own machine is only where a file is saved or picked.

## In the window

Exporting is the **Export…** button of Settings' **Configuration** section, and nowhere else.
Importing is the **Import…** button of that section, and the **Import configuration…** button of the welcome screen,
which is what an installation with no console yet shows. Both go through the window's own file dialogs, so no place is
imposed on the file: the desktop app raises its save and open dialogs, and a browser tab downloads the file and opens a
file picker. Where the window can do neither, the welcome screen's button is not shown and the Configuration section
says that exporting and importing are not available (see "Configuration" in `docs/product/settings.md`).

**The file.** Both dialogs offer JSON files (`.json`). The name offered for saving is `octoboard-<console name>.json`
for one console — any character a file name cannot hold (a control character, `/`, `\`, `:`, `*`, `?`, `"`, `<`, `>`,
`|`) becomes a space, and runs of spaces become one — and `octoboard-configuration.json` for everything, or for a
console whose name leaves nothing. The user can change it in the save dialog; a browser may file the download under a
name of its own. The desktop app's open dialog reads the picked file whole and refuses, before anything is sent to the
daemon, one larger than 64 MiB, as too large to be a configuration file, and one that is not UTF-8 text, as not a text
file; either is told in the import dialog, and another file can be chosen.

**Exporting** opens a dialog that asks what to export: the console the sidebar shows, or everything; with no console
there is only everything. Each choice says whether the application settings go with it: everything carries them, this
device's keyboard shortcuts included, and a single console does not. Under that choice is a checkbox, **Include full project paths**, unchecked by default; its
note says that without them the file records only each project's directory name, and that an import then places each
project in the default clone directory or clones it from its remote (see "Project paths" below). Only while it is
checked, and so before anything is saved, a warning says that the file contains each project's full directory path on
the machine running Octoboard, which may not be the device the user is on, that those paths can reveal details such as
the user name, and that the file is to be shared only with people the user trusts. The dialog's **Export** button then
asks the daemon for the document and opens the save dialog. In the desktop app a toast names the file it was saved as. A browser files a download itself, under its own name and wherever
its settings say, and does not report one that was blocked or cancelled, so there the toast says only that the
configuration was downloaded and names no file. Dismissing the desktop app's save dialog saves nothing and leaves the
export dialog open; a file that could not be written is said in the dialog.

**Importing** opens a dialog in three steps. The user picks a file — the step says beforehand that the file is one
exported earlier, that its contents can be reviewed and where each project goes on the machine running Octoboard chosen
before anything is created, and that an export of everything also includes the application settings — and the window
sends its document to the daemon to be inspected; a file that is not JSON, or that the daemon refuses (not a configuration, a version this
Octoboard does not read, unreadable), is told with its reason, and another can be chosen. A browser too old to report a
dismissed file picker never answers for one, so the dialog is not held while the picker is open: it can be closed, or
the picker opened again. What the daemon answers is then **reviewed before anything is created**: for each console, a
row for each of its projects, with its name and, at the end of the same line, the choice of where the project goes;
under them the path the project had on the exporting machine and the directory the choice would use, or where a clone
would come from and go to. A project the document records by its directory name alone has no exported path line; its
row keeps only the sentence saying where the project was found, or that it was not found and whether it can be cloned.
A row offers only the choices its state allows, with the daemon's default preselected, and always offers **Another directory**, which asks for a path (typed or browsed to) and refuses one that is not absolute or
does not start with `~/`, and **Leave out**. A project left out says nothing more, unless it was found nowhere on the
daemon's host, which the row then says. Four buttons apply one way to every project that offers it (exported paths, the
default clone directory, clone from remotes) or leave every project out, and a project they do not reach keeps its row.
The account references that would fall back to a default account are listed on the console or the project that holds
them. A document with application settings gets a checkbox for them, unchecked, whose note has a line for each setting
the document carries — one an older document leaves out has no line — saying what applying it would change from what
is set now: that it turns the setting on, and what then follows, in that setting's own words (see "The application
settings a full export carries" below); that it turns it off; or that it is already as the document has it, so
applying changes nothing. A document that carries keyboard shortcuts adds a line for them, which says that applying
sets this device's shortcuts to the file's — with how many of them differ from the defaults, counting only those that
can be used here — and that they apply to this device or browser only; or that applying puts this device's shortcuts
back to their defaults; or that the file's are the ones this device already has, so applying changes nothing.

The review's **Import** button sends the document unchanged, with the choice of every project that is placed, in one
request; a project left out is sent none. A directory the user typed that is not usable stops the import before any
request and is marked on its row. The dialog cannot be dismissed while the request runs, which with clones can be long.
The **result** replaces the review. It opens with a summary of the totals (only what happened, and that nothing failed
when so) and, when the application settings were applied, a line for each setting applied, saying it is now on or off, and
for keyboard shortcuts a line saying how many changes from the defaults this device's shortcuts now have, or that
they are back to their defaults, followed by how many of the file's shortcuts could not be used here and were left
out, when any were; when they could not be applied, a red alert saying so with the reason. Under it a table has a line for each project,
under its console, with its result (created, left out, not tried, each in words with a glyph) and a detail: how a
created project was placed (at its exported path, in the default clone directory, cloned, in the directory chosen), or
that a project was not tried because its console could not be created. A line that failed has no detail: its result
is a red circled `!` followed by the word **Failed**, like the other results. Where the failure came with a reason,
the two together are one control, named for that project — or console — having failed, which opens a popover with
why, any raw diagnostic included (see "Failures marked on a row" in `docs/product/error-alerts.md`); where it came with
none, they are the glyph and the word alone, with nothing to open. A console that could not be created, or that came out
with no projects, has a line of its own. In a narrow dialog each line becomes a block that names every value. Each
account that fell back to a default account is listed under its own heading below. The result is the only place the import's failures are shown: no error alert opens for them.

Exporting and importing are also requests of the daemon, which any program on its host can reach (see "Who can reach
the daemon" in `docs/product/application-lifecycle.md`).

## The configuration document

The document is JSON, and it carries a format marker and a version number so that a file that is not one of these is
told apart from one this Octoboard cannot read. It is both the file the user keeps and the input of an import, so
nothing in it is rewritten or built by hand: an export produces it and an import reads it back unchanged.

### Scope

There are two scopes, and the document has the same shape either way:

- **one console** — that console and its projects, with no application settings;
- **everything** — every console with its projects, plus the application settings.

A full export is a list of consoles that happens to hold all of them, and it is the presence of the application
settings that says a document is a full one. Within a document the consoles stand in the order they were created and
each console's projects by name, so the same configuration exports the same way every time. Every project of a console
travels, whether or not its directory is a git repository.

### What each console carries

Its name, its avatar image if it has one, its default agent for console sessions, its default agent for project sessions
— none while it follows the one for console sessions — the account it names for each agent, the models and reasoning
efforts it sets for its console sessions and for its project sessions, one pair of each per agent, and its console
session instructions if it has any (see "Console session instructions" in `docs/product/consoles.md`). An import
restores the instructions trimmed, as saving them in the console dialog does, and a document that does not carry them
gives the console none. A document that does not carry a default agent for project sessions, such as one exported before
consoles had one, gives the console none, so its project sessions follow its default agent for console sessions; one
that does not carry the project session models and efforts leaves them unset.

A console's working directory is **not** in the document. It is no directory of the user's choosing: Octoboard creates
it under its own data directory and names it after the console, so it would carry nothing another machine could use. An
import creates the console, and with it a working directory of its own.

### What each project carries

Its name, its description if it has one, the directory it points at, how it was associated, the remote URL recorded for
it, its own default agent if it has one, whether it is pinned, its tags (see `docs/product/project-tags.md`), and — one
set per agent — the account its sessions run under and the model and reasoning effort they launch with (see
`docs/product/projects.md`). An import stores the description trimmed, and a project of a document that carries no
description is imported with none. Export and import preserve the full length of project names and descriptions,
including values beyond the editing limits in "Project names" and "The project's description" in
`docs/product/projects.md`; an import neither rejects nor truncates them for their length.

An import treats a project's path as a starting point and never as a place to write (see "Where an imported project
goes" below).

### Project paths

An export either includes full project paths or leaves them out; the export dialog leaves them out unless its checkbox
is checked, and a request to the daemon that does not say which includes them. Either way the document has the same
format and version.

- **Full paths included** — each project's path is its absolute directory path on the exporting machine, and its remote
  URL is the one recorded for it, whatever it is. A project's path, and a remote URL that is itself a local path or a
  `file://` URL, are the only paths of the exporting machine anywhere in the document, which is the thing to look at
  before handing the file to someone else.
- **Full paths left out** — each project's path is only the last segment of its directory path, its directory name
  (empty for a path with none, such as `/`), and its remote URL is kept only when it may be cloned from (see "What may be
  cloned from" below); any other remote URL, such as a local path or a `file://` URL, is left out. Such a document holds
  no path of the exporting machine.

### How an account travels

An account reference travels as the account's **name alone**, and which agent it belongs to is settled by the setting
it sits in — the console's Claude Code account names a Claude Code account. Neither the account's stored id nor its
config directory is in the document: the id means nothing elsewhere and the directory is the layout of the machine it
was on. The accounts themselves therefore do not travel either; an import rebinds every reference to the importing
machine's own accounts (see "Accounts are rebound by name" below).

A reference to an account that has since been deleted, or that belongs to another agent than the setting it sits in, is
dropped from the document, leaving exactly what deleting that account leaves behind: the console on that agent's
default account, the project back to inheriting its console's reference (see "Agent config directories" in
`docs/product/consoles.md`).

A project keeps all three states its account setting can be in: inheriting its console's reference, the agent's default
account pinned even where the console names another, or a named account.

### Models and efforts travel as stored

A model name and a reasoning effort travel as they are stored. They are read only when a session is launched, so a
value the other machine's agent does not offer is nothing for an export or an import to refuse; it is the launch that
meets it (see `docs/product/model-and-reasoning-effort.md`).

### What is not configuration and never travels

Sessions and their records, the pages console sessions pushed to the report panel, the conversations the agents keep,
and folder trust — a project's trust permission and the trusted folders alike — are the state of one machine rather than
configuration, and are in no document. Neither is the default clone directory: that one is the importing machine's own
to choose, and an import places what it clones under it (see "Default clone directory" in `docs/product/settings.md`).
No console and no project carries its stored id. Codex's and Claude Code's permission approval settings also stay
local: exports omit them and imports leave the importing machine's choices unchanged, since importing them could
silently turn on **Approve everything**, which on its own machine is saved only after a confirmation (see "Approval
modes" in `docs/product/agent-accounts.md`). So do the list of added agents, the names the user gave the default
accounts and which accounts are turned off (see "Added agents", "The default account" and "Turning an account off" in
`docs/product/agent-accounts.md`). Account quota readings are not stored at all and are in no document either (see
"How a quota is read" in `docs/product/account-quotas.md`).

### The application settings a full export carries

Seven, each under its own key. The first is in every document that has application settings. Each of the other six
is missing from a document exported before it was carried, and applying that document's settings then leaves the
importing machine's value as it is; the import review notes each of the six only when the document carries it as a
boolean.

- **Automatically pull repositories** (see "Automatically pulling repositories" in
  `docs/product/project-git-status.md`), under `auto_pull_repositories`. An import also reads it from a document that
  carries it under the key `auto_sync_repositories`; an export writes only `auto_pull_repositories`.
- **Automatically delete archived sessions whose conversation is gone** (see "Deleting archived sessions whose
  conversation is gone" in `docs/product/deleting-sessions.md`), under `auto_delete_unresumable_archives`.
- **Automatically select an account** (see that section in `docs/product/settings.md`), under `auto_select_account`.
  The review's line for turning it on says that a new session then uses the account whose weekly quota resets soonest.
- **Order the sidebar by status** (see that section in `docs/product/settings.md`), under `sidebar_order_by_status`.
  The review's line for turning it on says that sessions and projects in the sidebar then move as their status changes.
- **Start usage windows automatically** (see that section in `docs/product/settings.md`), under
  `auto_start_quota_windows`. The review's line for turning it on says that a usage window that has not started
  counting is then started with one small request, which uses a little of the account's quota.
- **Automatically approve session requests** (see "Approving requests automatically" in
  `docs/product/session-requests.md`), under `auto_approve_session_requests`. The review's line for turning it on says
  that a session's request to start a session that reports to the user is then approved without asking first.
- **Automatically approve resume requests** (see the same section), under `auto_approve_resume_requests`. The
  review's line for turning it on says that a session's request to reopen or resume a session, or to resume a
  conversation in a new session, is then approved without asking first.

**The keyboard shortcuts of the exporting client** travel with these, under `keyboard_shortcuts`. They are not the
daemon's: each client keeps its own (see "Where the shortcuts are kept" in `docs/product/keyboard-shortcuts.md`), so
the client adds them to the document it saves — only the shortcuts it has changed from their defaults, and `{}` when
it has changed none — and the daemon neither builds nor reads them. A full document asked of the daemon by any other
program therefore has none. Their form is in "The shortcuts as data" in `docs/product/keyboard-shortcuts.md`. A
document without them leaves the importing client's shortcuts as they are when its settings are applied; one with `{}`
puts them back to their defaults.

A single console's document carries no application settings at all. The settings that belong to one machine or to its
accounts stay out of every document (see "What is not configuration and never travels" above).

### Versions

The format marker tells a configuration document from any other JSON. The version is a whole number, 1 today, and is
raised only when a change would make a reader written for an older version misread a document — a field dropped, or
given another meaning. Adding a field is not such a change, and a reader ignores a field it does not know; neither is
renaming one whose old name an import still reads, as with `auto_sync_repositories` above. An import refuses a document
whose format it does not recognize, and one whose version is 0 or newer than it reads, before it creates anything.

A document of an older version is not refused: before reading it, an import upgrades it to the current version one
version at a time, so a file exported before the version was raised stays importable. The upgrade is part of the import
and the file itself is left as it is. Only version 1 exists today, so no document needs upgrading yet.

A document also carries, under `exported_by`, the version of Octoboard (its daemon) that exported it. It is there only
for diagnosing a document that misbehaves: an import never reads it, it has no bearing on the version number, and a
document exported before it existed does not carry it.

## Importing

An import is two steps. **Inspecting** reads a document and answers what importing it would meet on the daemon's host;
it changes nothing and runs no `git`. **Applying** takes the same document, together with the choice made for each
project, and creates. The document is untrusted input: every path and URL in it is data the daemon's host checks, and
nothing in it is acted on as it stands.

### Where an imported project goes

For each project of the document the inspection answers which of four directories it could be placed in, what each one
comes to on the daemon's host, and which of them is the default:

| Where | The directory it uses | When it is offered |
|---|---|---|
| The recorded path | the project's path as the document records it | only when that path is absolute and is an existing directory on the daemon's host |
| Under the default clone directory | the last segment of the recorded path — or, for a project recorded by its directory name alone, that name — under the host's default clone directory (`~/Projects` unless the user has set one) | only when that directory already exists; this is the ordinary case of the same layout of projects under another root |
| A fresh clone | a clone of the project's recorded remote URL, into the default clone directory | only when that URL may be cloned from (see below) and the directory the clone would create does not exist yet |
| A directory the user points at | that directory | never offered: only the user can name one, and any project can be placed this way |

The **default** is the first of the recorded path, the derived path and the clone that is offered, and there is no
default when none of them is. A project recorded by its directory name alone (see "Project paths" above) is never
offered the recorded path, since a name is not an absolute path, so it can be offered only the directory under the
default clone directory and a fresh clone; an empty name derives no directory, which leaves only the clone. A project with nothing offered is left out of the import unless the user points at a
directory for it.

A directory the user points at has to be absolute, or to start with `~/`, which is expanded to the home directory of
the host the daemon runs on.

The existence checks of one inspection have **15 seconds in all**, and a project whose check has not come back by then
is offered nothing. The paths in a document were written on another machine, and one that points into a network mount
no longer there is an ordinary case rather than a fault.

### What may be cloned from

A document came from somewhere else, so a URL in it is held to a stricter rule than one the user types into the project
form, which leaves the judgement to git (see "Associating a project" in `docs/product/projects.md`):

- **accepted**: an `http`, `https`, `ssh`, `git`, `git+ssh` or `ssh+git` URL, and the `host:path` form, with no
  whitespace or control character anywhere in it and a host that cannot be read as an option;
- **refused**: a path on the daemon's host, a `file://` URL, `ftp://` and `ftps://`, and git's helper transports
  (`ext::`, `fd::`), which can make git run a command of the document's choosing.

A URL that is not accepted is not offered as a clone, and asking for the clone of such a project anyway is refused for
that project.

### Accounts are rebound by name

Every account reference of the document is bound to the account of the daemon's host that has the same name and belongs
to the agent whose setting the reference sits in, matched **ignoring letter case** — which is how account names are
unique within an agent in the first place (see "Adding, editing and removing an account" in
`docs/product/agent-accounts.md`). A reference that matches no account falls back to that agent's **default
account**, pinned rather than left inheriting, and every such fallback is listed one by one: the inspection says
which references would fall back, the result of an import which ones did.

### What applying does

**An import always creates.** Every console of the document becomes a new console, whether or not one of the same name
is already there — console names are not unique, so two of a name stand side by side.

Each project then goes through **exactly the association a project added by hand goes through** (see "Associating a
project" in `docs/product/projects.md`): its directory has to exist, its name, tags and launch settings are
normalized the same way, and a directory already associated with the console being imported into is not associated a
second time, which is what two projects of one console resolving to the same directory runs into — the second of them is
reported as failed. What the document says about a project — its pin, its tags, its default agent (none stays none, and
nothing is detected from the directory), its account references, its models and efforts — is applied as it stands. How
the project was associated on the exporting machine does not carry over: what is recorded is how the daemon's host
placed it, as a clone or as a local directory.

**An imported project is untrusted like any other.** Folder trust is in no document, so the first session opened under
an imported project goes through its agent's trust confirmation just as one added by hand would, unless its directory
falls under a folder the user has already trusted on the daemon's host (see "The trust permission" in
`docs/product/folder-trust.md`).

Every console and every project appears as soon as it is created rather than when the import finishes, so a long import
fills the window in as it goes.

### Application settings are left alone unless they are asked for

A document's application settings are applied **only when the import is explicitly asked to apply them**, and are not
applied by default. Automatically pull repositories makes the daemon fast-forward the user's own repositories by itself,
automatically deleting archived sessions makes it delete sessions by itself, automatic account selection makes a
later new session that names no account take the one whose weekly quota resets soonest, starting usage windows
automatically spends a little of the accounts' quota by itself, and the two automatic approvals let agents' requests
through without asking the user, so applying them changes how the daemon's host behaves rather than only adding to what
it holds. The inspection says both what the document sets and what is set now, so what applying would change can be
shown before anything is done. The settings are applied last, through the same path as a change made in Settings — so an
import that turns automatic pulling on runs the same immediate fast-forward pass turning it on by hand does (see
"Turning the switch on" in `docs/product/project-git-status.md`), one that turns the automatic deletion on deletes
nothing until the next scheduled check, as the switch does, one that turns automatic account selection on or off changes
no session already open, one that turns the starting of usage windows on looks over the quota readings already held at
once (see "When an account is kicked" in `docs/product/account-quotas.md`), and one that turns an automatic approval on
or off changes no request already waiting — and the result of the import reports what was applied. Asking to apply the
settings of a document that carries none is refused rather than quietly ignored, so a request never does less than it
was told to.

The keyboard shortcuts follow the same choice, and are applied by the client that runs the import, to itself alone,
once the daemon reports the other settings applied; when those could not be applied, neither are the shortcuts. They
**replace** that client's shortcuts: every command the document does not name goes back to its default. The document's
shortcuts are judged as one set, under the same rules as a shortcut the user records, and an entry that cannot be used
on the importing client is dropped and counted (see "The shortcuts as data" in `docs/product/keyboard-shortcuts.md`).
Other clients — another browser, or the desktop app — keep their own.

### What is refused, and what is reported instead

Refused before anything at all is created:

- a document that is not a configuration document, one whose version is 0 or newer than this Octoboard reads, and one
  that cannot be read;
- a choice that cannot be right: it names a project the document does not have, it names one project twice, or the
  directory it points at is neither absolute nor starts with `~/`;
- a request to apply application settings a document does not carry;
- anything creating a console by hand would refuse of any console in the document — an avatar that is not an image, or
  one too large (see "Avatar" in `docs/product/consoles.md`).

**Past that point nothing is refused and nothing is undone**, because by then earlier consoles and projects exist and
have already appeared:

- a console that cannot be created is recorded as failed, every project of it as not attempted, and the next console of
  the document is imported;
- a project that cannot be placed — the recorded path is gone, a clone fails or the directory it would create already
  exists, the directory pointed at is not there, the directory is already associated with that console — is recorded as
  failed with the reason, and the remaining projects are imported as usual;
- application settings that could not be applied are reported as such, and the import stands.

The result says, for every console and every project of the document, whether it was created, left out for want of a
choice, failed and why, or never attempted — so the outcome never has to be pieced together from what appeared in the
window.

### How long an import takes

An import that clones can run long. The clones are made one at a time, in the document's order, and each is cut off
after **30 minutes**. There is no way to cancel an import while it runs, and it answers only once the last clone has
finished — which can be as long as all of them together. Everything created before that point is already in the
window.
