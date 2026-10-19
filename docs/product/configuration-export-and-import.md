# Configuration export and import

Octoboard can lift its configuration out of one machine and build it again on another. What travels is a **configuration
document**: one console or every console, the projects associated with them and, for a full export, one application
setting. Nothing of what a machine is doing goes with it, so what arrives on the other side is the layout of consoles
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
there is only everything. Under that choice, before anything is saved, the dialog says that the file contains the path
of each project on the machine running Octoboard, which may not be the one the window is on, so sharing it is a choice
made knowing so. The dialog's **Export** button then asks the daemon for the document and opens the save dialog. In the
desktop app a toast names the file it was saved as. A browser files a download itself, under its own name and wherever
its settings say, and does not report one that was blocked or cancelled, so there the toast says only that the
configuration was downloaded and names no file. Dismissing the desktop app's save dialog saves nothing and leaves the
export dialog open; a file that could not be written is said in the dialog.

**Importing** opens a dialog in three steps. The user picks a file, and the window sends its document to the daemon to
be inspected; a file that is not JSON, or that the daemon refuses (not a configuration, a version this Octoboard does
not read, unreadable), is told with its reason, and another can be chosen. A browser too old to report a dismissed file
picker never answers for one, so the dialog is not held while the picker is open: it can be closed, or the picker opened
again. What the daemon answers is then **reviewed before anything is created**: for each console, a row for each of its
projects, with its name and, at the end of the same line, the choice of where the project goes; under them the path the
project had on the exporting machine and the directory the choice would use, or where a clone would come from and go to.
A row offers only the choices its state allows, with the daemon's default preselected, and always offers **Another
directory**, which asks for a path (typed or browsed to) and refuses one that is not absolute or does not start with
`~/`, and **Leave out**. A project left out says nothing more, unless it was found nowhere on the daemon's host, which
the row then says. Four buttons apply one way to every project that offers it (exported paths, the default clone
directory, clone from remotes) or leave every project out, and a project they do not reach keeps its row. The account
references that would fall back to a default account are listed on the console or the project that holds them. A
document with application settings gets a checkbox for them, unchecked, whose note says what applying would change from
what is set now.

The review's **Import** button sends the document unchanged, with the choice of every project that is placed, in one
request; a project left out is sent none. A directory the user typed that is not usable stops the import before any
request and is marked on its row. The dialog cannot be dismissed while the request runs, which with clones can be long.
The **result** replaces the review. It opens with a summary of the totals (only what happened, and that nothing failed
when so) and, for a document that had application settings, whether they were applied. Under it a table has a line for
each project, under its console, with its result (created, left out, failed, not tried) and a detail: how a created
project was placed (at its exported path, in the default clone directory, cloned, in the directory chosen), why a failed
one failed, or that a project was not tried because its console could not be created. A console that could not be
created, or that came out with no projects, has a line of its own. In a narrow dialog each line becomes a block that
names every value. Each account that fell back to a default account is listed under its own heading below.

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
description is imported with none.

A project's path is **the one path of the exporting machine anywhere in the document**, which is the thing to look at
before handing the file to someone else. An import treats it as a starting point and never as a place to write (see
"Where an imported project goes" below).

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
No console and no project carries its stored id. Codex and Claude Code account approval modes, including the default
accounts', also stay local: exports omit them and imports leave the importing machine's choices unchanged. Account quota
readings are not stored at all and are in no document either (see "How a quota is read" in
`docs/product/account-quotas.md`).

### The application setting a full export carries

One: **Automatically pull repositories** (see "Automatically pulling repositories" in
`docs/product/project-git-status.md`), under the key `auto_pull_repositories`. An import also reads it from a document
that carries it under the key `auto_sync_repositories`; an export writes only `auto_pull_repositories`. A single
console's document carries no application settings at all.

### Versions

The format marker tells a configuration document from any other JSON. The version is a whole number, 1 today, and is
raised only when a change would make a reader written for an older version misread a document — a field dropped, or
given another meaning. Adding a field is not such a change, and a reader ignores a field it does not know. An import
refuses a document whose format it does not recognize, and one whose version is newer than it reads, before it creates
anything.

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
| Under the default clone directory | the last segment of the recorded path, under the host's default clone directory (`~/Projects` unless the user has set one) | only when that directory already exists; this is the ordinary case of the same layout of projects under another root |
| A fresh clone | a clone of the project's recorded remote URL, into the default clone directory | only when that URL may be cloned from (see below) and the directory the clone would create does not exist yet |
| A directory the user points at | that directory | never offered: only the user can name one, and any project can be placed this way |

The **default** is the first of the recorded path, the derived path and the clone that is offered, and there is no
default when none of them is. A project with nothing offered is left out of the import unless the user points at a
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
unique within an agent in the first place (see "Agent accounts" in `docs/product/settings.md`). A reference that matches
no account falls back to that agent's **default account**, pinned rather than left inheriting, and every such fallback
is listed one by one: the inspection says which references would fall back, the result of an import which ones did.

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
so applying it changes how the daemon's host behaves rather than only adding to what it holds. The inspection says both
what the document sets and what is set now, so what applying would change can be shown before anything is done. The
settings are applied last, through the same path as a change made in Settings — so an import that turns the switch on
runs the same immediate fast-forward pass turning it on by hand does (see "Turning the switch on" in
`docs/product/project-git-status.md`) — and the result of the import reports what was applied. Asking to apply the
settings of a document that carries none is refused rather than quietly ignored, so a request never does less than it
was told to.

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
