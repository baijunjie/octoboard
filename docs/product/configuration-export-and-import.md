# Configuration export and import

Octoboard can lift its configuration out of one machine and build it again on another. What travels is a **configuration
document**, made of up to three **regions**, each chosen on its own: the **consoles and projects** (one console, every
console, or none), the **accounts** — the structure of the agents' accounts, never anything to log one in with — and the
**application settings**. Nothing of what a machine is doing goes with it, so what arrives on the other side is the
layout of consoles, projects and accounts, with its accounts still to be logged in and its directories still to be
trusted.

A **machine**, here, is the host the daemon runs on, which is where the projects, the accounts and the default clone
directory are. The window is not always on it: the desktop app's is, but a browser tab reaching a daemon on a server is
on another machine. So a path, a directory or an account this document speaks of is the host's, never the browser's, and
the window's own machine is only where a file is saved or picked.

How the window exports and imports — where it starts, the export dialog, and the import's review and result — is in
`docs/product/configuration-export-and-import-dialogs.md`. Exporting and importing are also requests of the daemon,
which any program on its host can reach (see "Who can reach the daemon" in `docs/product/application-lifecycle.md`).

## The configuration document

The document is JSON, and it carries a format marker and a version number so that a file that is not one of these is
told apart from one this Octoboard cannot read. It is both the file the user keeps and the input of an import, so
nothing in it is rewritten or built by hand: an export produces it and an import reads it back unchanged.

### Scope

A document holds the regions its export chose, and has the same shape whichever they are:

- **consoles and projects** — one console, every console, or none, each with its projects; a document without them has
  an empty list of consoles;
- **application settings** — present exactly when they were chosen, whichever consoles were;
- **accounts** — present exactly when they were chosen, and then always every account, whichever consoles were.

No region says anything about another: a document that carries the application settings is not therefore one of every
console, and one that carries a single console may carry the settings and the accounts too. An export that would hold
nothing — no console, no application settings and no accounts — is refused (`export_empty`). A request to the daemon
that does not say which consoles exports the one it names, or every console when it names none; one that does not say
whether the application settings are included includes them exactly when every console is exported; and one that does
not say whether the accounts are included leaves them out.

Within a document the consoles stand in the order they were created and each console's projects by name, so the same
configuration exports the same way every time. Every project of a console travels, whether or not its directory is a
git repository.

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
`docs/product/projects.md`), and its own git workflow settings under `git_workflow`, `null` for each item it leaves
to the application-wide value (see "A project's own git workflow settings" in
`docs/product/git-workflow-settings.md`). Every export of consoles carries them, of one console or of all. An import
stores the description trimmed, and a project of a document that carries no description is imported with none; one
whose document carries no git workflow settings follows every application-wide value. Export and import preserve the
full length of project names and descriptions, including values beyond the editing limits in "Project names" and "The
project's description" in `docs/product/projects.md`; an import neither rejects nor truncates them for their length.

An import treats a project's path as a starting point and never as a place to write (see "Where an imported project
goes" below).

### Project paths

An export either includes full project paths or leaves them out; the export dialog leaves them out unless its checkbox
is checked, and a request to the daemon that does not say which includes them. Either way the document has the same
format and version. The choice is about projects alone: the accounts' directories are written whatever it says (see
"The accounts" below).

- **Full paths included** — each project's path is its absolute directory path on the exporting machine, and its remote
  URL is the one recorded for it, whatever it is. A project's path, a remote URL that is itself a local path or a
  `file://` URL, and an account directory outside the home directory are the only paths of the exporting machine
  anywhere in the document, which is the thing to look at before handing the file to someone else.
- **Full paths left out** — each project's path is only the last segment of its directory path, its directory name
  (empty for a path with none, such as `/`), and its remote URL is kept only when it may be cloned from (see "What may
  be cloned from" below); any other remote URL, such as a local path or a `file://` URL, is left out. Such a document
  holds no path of the exporting machine but, when it carries the accounts, an account directory outside the home
  directory.

### How an account reference travels

An account reference — the account a console or a project names for an agent — travels as the account's **name alone**,
and which agent it belongs to is settled by the setting it sits in — the console's Claude Code account names a Claude
Code account. Neither the account's stored id nor its config directory is in the reference: the id means nothing
elsewhere and the directory is the layout of the machine it was on. An import rebinds every reference by name to the
importing machine's own accounts, which include those it creates from the document's accounts (see "Accounts are rebound
by name" below).

A reference to an account that has since been deleted, or that belongs to another agent than the setting it sits in, is
dropped from the document, leaving exactly what deleting that account leaves behind: the console on that agent's
default account, the project back to inheriting its console's reference (see "Agent config directories" in
`docs/product/consoles.md`).

A project keeps all three states its account setting can be in: inheriting its console's reference, the agent's default
account pinned even where the console names another, or a named account.

### The accounts

The accounts region is the structure of the exporting machine's accounts (see `docs/product/agent-accounts.md`), and
never a credential, a login state or a quota reading. It holds:

- the **added agents**, in their order (see "Added agents" in `docs/product/agent-accounts.md`); left out while the
  exporting machine's list is still undecided, and an import of such a document then adds no agent;
- for each agent that has an account the user owns or a default account the user has named, in the order Claude Code,
  Codex, Grok Build:
  - the **default account**'s name as the user gave it, if any, and its place in the agent's display order — how many
    accounts come before it. The default account's directory, which the agent resolves on each machine, and whether it
    is turned off are not carried;
  - each **account the user owns**, in the agent's display order (see "Reordering accounts" in
    `docs/product/agent-accounts.md`): its name, its directory, whether it shares the default configuration and whether
    it is turned off.

No account carries its stored id, and an account whose last sharing change did not finish is not marked as such in the
document.

An account's **directory is always written**, whatever the choice about project paths, since an account cannot be
created without one: as `~/` followed by its path under the exporting machine's home directory when it lies there (`~`
for the home directory itself), so the user name is not in it, and as the absolute path otherwise. On a file system that
ignores letter case, a directory spelled in another case than the home directory still counts as under it.

### Models and efforts travel as stored

A model name and a reasoning effort travel as they are stored. They are read only when a session is launched, so a
value the other machine's agent does not offer is nothing for an export or an import to refuse; it is the launch that
meets it (see `docs/product/model-and-reasoning-effort.md`).

### What is not configuration and never travels

Sessions and their records, the pages sessions pushed to the report panel, the conversations the agents keep, and folder
trust — a project's trust permission and the trusted folders alike — are the state of one machine rather than
configuration, and are in no document. Neither is the default clone directory: that one is the importing machine's own
to choose, and an import places what it clones under it (see "Default clone directory" in `docs/product/settings.md`).
No console, project or account carries its stored id. Codex's and Claude Code's permission approval settings also stay
local: exports omit them and imports leave the importing machine's choices unchanged, since importing them could
silently turn on **Approve everything**, which on its own machine is saved only after a confirmation (see "Approval
modes" in `docs/product/per-agent-settings.md`). Nothing to log an account in with — credentials, login state — is in any
document, so every account an import creates starts not logged in; nor are the default account's directory and whether
the default account is turned off. The added agents, the accounts and their order, sharing and turned-off choices, and
the default accounts' names travel only in the accounts region (see "The accounts" above). Account quota readings are
not stored at all and are in no document either (see "How a quota is read" in `docs/product/account-quotas.md`).

### The application settings an export carries

Thirteen, each under its own key. The first is in every document that has application settings. Each of the other
twelve is missing from a document exported before it was carried, and applying that document's settings then leaves
the importing machine's value as it is; the import review notes each of the ten switches after the first only when the
document carries it as a boolean. A document exported by a build that carried a "Talk to me in the interface language"
switch, under `reply_in_interface_language`, still imports: that key is ignored. The language the daemon holds for the
user is not carried either (see "The language the daemon holds" in `docs/product/language.md`): applying the document's
settings leaves it as the importing daemon holds it.

- **Automatically pull repositories** (see "Automatically pulling repositories" in
  `docs/product/project-git-status.md`), under `auto_pull_repositories`. An import also reads it from a document that
  carries it under the key `auto_sync_repositories`; an export writes only `auto_pull_repositories`.
- **Automatically delete archived sessions whose conversation is gone** (see "Deleting archived sessions whose
  conversation is gone" in `docs/product/deleting-sessions.md`), under `auto_delete_unresumable_archives`.
- **Automatically select an account** (see that section in `docs/product/settings.md`), under `auto_select_account`. The
  review's line for turning it on says that a new session then uses the account whose weekly limit resets soonest.
- **Automatically switch accounts at the usage limit** (see that section in `docs/product/settings.md`), under
  `auto_switch_on_usage_limit`. The review's line for turning it on says that a session that stops at its account's
  usage limit is then switched to another account with usage left and carries on.
- **Order the sidebar by status** (see that section in `docs/product/settings.md`), under `sidebar_order_by_status`. The
  review's line for turning it on says that sessions and projects in the sidebar then move as their status changes.
- **Start usage windows automatically** (see that section in `docs/product/settings.md`), under
  `auto_start_quota_windows`. The review's line for turning it on says that a usage window that has not started counting
  is then started with one small request, which counts a little toward the account's usage limits.
- **Automatically approve session requests** (see "Approving requests automatically" in
  `docs/product/session-requests.md`), under `auto_approve_session_requests`. The review's line for turning it on says
  that a session's request to start a session that reports to the user is then approved without asking first.
- **Automatically approve resume requests** (see the same section), under `auto_approve_resume_requests`. The review's
  line for turning it on says that a session's request to reopen or resume a session, or to resume a conversation in a
  new session, is then approved without asking first.
- **Automatically approve account switches** (see the same section), under `auto_approve_account_switches`. The review's
  line for turning it on says that a session's request to switch the account of one of its sessions is then approved
  without asking first.
- **Automatically approve handoffs** (see the same section), under `auto_approve_handoffs`. The review's line for
  turning it on says that a session's request to hand one of its sessions off to another agent is then approved without
  asking first.
- **Ask questions as choices** (see that section in `docs/product/settings.md`), under `ask_questions_as_choices`. The
  review's line for turning it on says that Octoboard then asks sessions started afterwards to put a question that needs
  the user's decision as options to pick from.
- **The application-wide git workflow settings** (see "The five settings" in `docs/product/git-workflow-settings.md`),
  under `git_workflow`, an object holding all five of `development`, `squash_before_delivery`, `rebase_before_merge`,
  `delivery` and `revert_gate`. Applying the document's settings sets the carried items to the document's values, and a
  document without it, or with `null` there, leaves them as they are. A carried `git_workflow` object that omits
  `squash_before_delivery` leaves that item at its current application-wide value, in both the import review and the
  applied settings. A project override that omits it follows the application-wide value.
  The import review and its result each have one line for all five (see "The import review" and "The import result" in
  `docs/product/configuration-export-and-import-dialogs.md`).
- **Claude compatibility** (see "Claude compatibility" in `docs/product/per-agent-settings.md`), under
  `claude_compatibility`, an object keyed by agent (`codex`, `grok`) holding `true` or `false`. It carries only the
  choices the user made: an agent never chosen for is absent, so on the importing machine it keeps following that
  machine's default — an unset Codex there follows whether Claude Code is added there — and an export where no choice
  was made carries `{}`. Applying the document's settings records each choice it carries as the importing machine's own,
  agent by agent, and leaves an agent it does not carry as it is; an agent the document names that has no such setting
  here, Claude Code or one this build does not know, is skipped rather than refused.

**The keyboard shortcuts of the exporting client** travel with these, under `keyboard_shortcuts`. They are not the
daemon's: each client keeps its own (see "Where the shortcuts are kept" in `docs/product/keyboard-shortcuts.md`), so
the client adds them to the document it saves whenever it carries the application settings — only the shortcuts it has
changed from their defaults, and `{}` when it has changed none — and the daemon neither builds nor reads them. A
document with application settings asked of the daemon by any other program therefore has none. Their form is in "The
shortcuts as data" in `docs/product/keyboard-shortcuts.md`. A document without them leaves the importing client's
shortcuts as they are when its settings are applied; one with `{}` puts them back to their defaults.

A document whose export did not include the application settings carries none of them, whichever consoles it holds.
The settings that belong to one machine stay out of every document (see "What is not configuration and never travels"
above).

### Versions

The format marker tells a configuration document from any other JSON. The version is a whole number, 2 today, and is
raised only when a change would make a reader written for an older version misread a document — a field dropped, or
given another meaning. Adding a field is not such a change, and a reader ignores a field it does not know; neither is
renaming one whose old name an import still reads, as with `auto_sync_repositories` above. The accounts region was added
the same way, without raising the version: a document exported before it existed carries no accounts. An import refuses
a document whose format it does not recognize, and one whose version is 0 or newer than it reads, before it creates
anything.

A document of an older version is not refused: before reading it, an import upgrades it to the current version one
version at a time, so a file exported before the version was raised stays importable. The upgrade is part of the import
and the file itself is left as it is. Upgrading a version 1 document to version 2 renames the application-wide
`worktrees` under `settings`, and each project's `worktrees`, to `git_workflow`, and turns the boolean `use_worktrees`
in them into `development`: `true` becomes `worktree`, `false` becomes `in_place`, and a project's `null`, following the
application-wide value, stays `null`.

A document also carries, under `exported_by`, the version of Octoboard (its daemon) that exported it. It is there only
for diagnosing a document that misbehaves: an import never reads it, it has no bearing on the version number, and a
document exported before it existed does not carry it.

## Importing

An import is two steps. **Inspecting** reads a document and answers what importing it would meet on the daemon's host;
it changes nothing and runs no `git`. **Applying** takes the same document, together with which of its regions to
import and the choice made for each project and each account, and creates. The consoles and projects are imported
unless the import is told to leave them out; the accounts and the application settings only when it is asked for them.
The document is untrusted input: every path and URL in it is data the daemon's host checks, and nothing in it is acted
on as it stands.

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
default clone directory and a fresh clone; an empty name derives no directory, which leaves only the clone. A project
with nothing offered is left out of the import unless the user points at a directory for it.

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

### Importing accounts

Inspecting a document that carries accounts says, for each of its accounts, agent by agent in the document's order,
what importing it would come to:

- **Existing** — an account of the same agent already has its name, compared trimmed and ignoring letter case. An
  import uses that account as it is and **never changes it**: the document's directory, sharing and turned-off choices
  are not applied to it.
- **New** — it can be created as the document has it.
- **Directory in use** — its directory, as it resolves here, is already another account's of that agent, the default
  account included, or that of an earlier new account of the document, compared as the Add account form compares
  directories (see "Agent config directories" in `docs/product/consoles.md`); the inspection names the account that
  uses it.
- **Cannot be created** — the Add account form would refuse it (see "Adding, editing and removing an account" in
  `docs/product/agent-accounts.md`): its name is blank, is the word "Default", is the name this machine's default
  account of that agent has been given, or is taken by an earlier account of the document; or its directory is not
  absolute once resolved. The inspection gives the form's reason.

A directory is resolved on the daemon's host: `~` and a path starting with `~/` under its home directory, an absolute
path as it is; a bare name is not a directory. A directory that does not exist yet is fine, as it is for the Add account
form. Any account but an existing one can be left out. An account whose directory is in use or cannot be used can be
given **another directory** instead, absolute or starting with `~/`; one whose name is the problem can only be left
out, since no directory would help.

The inspection also says which of the document's added agents this machine has not added (none while this machine's
list is undecided), and, for each agent, the name the document gives its default account and whether an import would
apply it: only when this machine's default account of that agent has no name of its own, and the name is neither blank
nor "Default" and is no account's of that agent, the ones the import would create included.

Applying the accounts comes **before any console is created**, and nothing of it is undone:

1. Every agent of the document's added agents that this machine has not added is appended to its list, in the
   document's order. A list still undecided is decided first, as adding an agent decides it; where it stays undecided,
   nothing is appended.
2. Each account, in the document's order: an existing one is used as it is; one left out is skipped; any other is
   created through the Add account form's checks, in the other directory given for it where there is one. One that
   fails them now — a directory another account has taken since the inspection, say — is recorded as failed with the
   reason, and the import goes on.
3. The accounts created come after the agent's other accounts in its display order, in the document's order. The
   default account keeps its place on this machine: its place in the document is not applied.
4. A created account takes the document's turned-off choice. Where the document says it shares the default
   configuration, sharing is then turned on for it as its switch turns it on (see "Turning sharing on" in
   `docs/product/sharing-the-default-configuration.md`); when that fails, the account stays created and the failure is
   a warning on it.
5. The default account's name is applied under the rule above, checked again at that moment, so a name the default
   account was given meanwhile is kept. Whether the default account is turned off is never applied.

Every change appears in every window as the same change made by hand does. **Nothing is logged in**: the accounts are
created without credentials, and the user logs each one in afterwards (see `docs/product/logging-accounts-in.md`).

### Accounts are rebound by name

Every account reference of the document is bound to the account of the daemon's host that has the same name and belongs
to the agent whose setting the reference sits in, matched **ignoring letter case** — which is how account names are
unique within an agent in the first place (see "Adding, editing and removing an account" in
`docs/product/agent-accounts.md`). When the accounts are imported too, they are created first, so a reference to an
account the import creates binds to it, whichever directory it was given, and falls back only when that account was left
out or failed. A reference that matches no account falls back to that agent's **default account**, pinned rather than
left inheriting, and every such fallback is listed one by one: the inspection says which references would fall back,
the result of an import which ones did. Since no account the import would create exists yet when it inspects, the
inspection lists the references to them as well; the import review tells them apart by what is being imported (see
"Account fallbacks" in `docs/product/configuration-export-and-import-dialogs.md`).

### What applying does

Applying goes region by region: first the accounts, when they are asked for, so that the consoles' and projects'
references can bind to them (see "Importing accounts" above); then the consoles and projects, unless the import is told
to leave them out, in which case no console or project is created and no choice for a project is read; then the
application settings, when they are asked for (see "Application settings are left alone unless they are asked for"
below).

**An import always creates.** Every console of the document becomes a new console, whether or not one of the same name
is already there — console names are not unique, so two of a name stand side by side.

Each project then goes through **exactly the association a project added by hand goes through** (see "Associating a
project" in `docs/product/projects.md`): its directory has to exist, its name, tags and launch settings are normalized
the same way, and a directory already associated with the console being imported into (by the same rule, so the same
directory under another path counts) is not associated a second time, which is what two projects of one console
resolving to the same directory runs into — the second of them is reported as
failed. What the document says about a project — its pin, its tags, its default agent (none stays none, and nothing is
detected from the directory), its account references, its models and efforts, its own git workflow settings — is applied
as it stands. How the project was associated on the exporting machine does not carry over: what is recorded is how the
daemon's host placed it, as a clone or as a local directory.

**An imported project is untrusted like any other.** Folder trust is in no document, so the first session opened under
an imported project goes through its agent's trust confirmation just as one added by hand would, unless its directory
falls under a folder the user has already trusted on the daemon's host (see "The trust permission" in
`docs/product/folder-trust.md`).

Every account, console and project appears as soon as it is created rather than when the import finishes, so a long
import fills the window in as it goes.

### Application settings are left alone unless they are asked for

A document's application settings are applied **only when the import is explicitly asked to apply them**, and are not
applied by default. Automatically pull repositories makes the daemon fast-forward the user's own repositories by itself,
automatically deleting archived sessions makes it delete sessions by itself, automatic account selection makes a later
new session that names no account take the one whose weekly quota resets soonest, automatic switching at the usage
limit ends sessions' processes and moves them to other accounts by itself, starting usage windows automatically spends
a little of the accounts' quota by itself, and the four automatic approvals let agents' requests, account
switches and handoffs through without asking the user, asking questions as choices changes what sessions are told at
launch, and Claude compatibility changes what Codex and Grok Build sessions load at launch, so applying them changes how the daemon's host behaves rather than only adding to what it holds. The inspection
says both what the document sets and what is set now, so what applying would change can be shown before anything is
done. The settings are applied last, through the same path as a change made in Settings — so an import that turns
automatic pulling on runs the same immediate fast-forward pass turning it on by hand does (see "Turning the switch on"
in `docs/product/project-git-status.md`), one that turns the automatic deletion on deletes nothing until the next
scheduled check, as the switch does, one that turns automatic account selection on or off changes no session already
open, one that turns automatic switching at the usage limit on or off changes no session, one that turns the
starting of usage windows on looks over the quota readings already held at once (see "When an
account is kicked" in `docs/product/starting-usage-windows.md`), one that turns an automatic approval on or off changes
no request already waiting, and one that turns asking questions as choices or an agent's Claude compatibility on or off
changes no session already running — and the result of the import reports what was applied. Asking to apply the settings of a document that carries none is
refused rather than quietly ignored, so a request never does less than it was told to.

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
- a choice for a project that cannot be right: it names a project the document does not have, it names one project
  twice, or the directory it points at is neither absolute nor starts with `~/`;
- a choice for an account that cannot be right: it names an account the document does not have, it names one account
  twice, it gives both another directory and leaving the account out, or neither, or the directory it gives is neither
  absolute nor starts with `~/`;
- a request to apply application settings, or to create accounts, that a document does not carry;
- anything creating a console by hand would refuse of any console in the document — an avatar that is not an image
  (`icon_not_an_image`), or one larger than 256 KiB (`icon_too_large`; see "Avatar" in `docs/product/consoles.md`).
  Since a document can carry several consoles, the refusal names the console whose avatar it is (the daemon gives its
  name in `console_name`), and the error alert says which console's avatar is not an image or is larger than the
  limit. The console dialog turns a picked image into one scaled to the limit, so only an import meets either refusal.
  The remedy for each is to give that console, in the Octoboard the configuration came from, an avatar that is an
  image, or a smaller one, then export again and import the new file.

**Past that point nothing is refused and nothing is undone**, because by then earlier accounts, consoles and projects
exist and have already appeared:

- an account that cannot be created is recorded as failed with the reason, sharing that could not be turned on is a
  warning on its account, and the remaining accounts, then the consoles, are imported as usual;
- a console that cannot be created is recorded as failed, every project of it as not attempted, and the next console of
  the document is imported;
- a project that cannot be placed — the recorded path is gone, a clone fails or the directory it would create already
  exists, the directory pointed at is not there, the directory is already associated with that console — is recorded as
  failed with the reason, and the remaining projects are imported as usual;
- application settings that could not be applied are reported as such, and the import stands.

The result says, for every console and every project of the document when the consoles were imported, whether it was
created, left out for want of a choice, failed and why, or never attempted; and for every account of the document when
the accounts were, whether it was created, already existed, was left out or failed and why, with any sharing warning,
which agents were added and which default accounts were named — so the outcome never has to be pieced together from
what appeared in the window.

### How long an import takes

An import that clones can run long. The clones are made one at a time, in the document's order, and each is cut off
after **30 minutes**. There is no way to cancel an import while it runs, and it answers only once the last clone has
finished — which can be as long as all of them together. Everything created before that point is already in the
window.
