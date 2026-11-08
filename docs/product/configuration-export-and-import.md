# Configuration export and import

Octoboard can lift its configuration out of one machine and build it again on another. What travels is a **configuration
document**, made of up to three **regions**, each chosen on its own: the **consoles and projects** (one console, every
console, or none), the **accounts** — the structure of the agents' accounts and the settings that apply to them, never
anything to log one in with — and the **application settings**, whose sections of Settings are chosen one by one.
Which settings travel in which region is in `docs/product/settings-in-a-configuration.md`. Nothing of what a machine is doing goes with it, so what arrives on the other side is the
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
- **application settings** — present exactly when at least one of their sections was chosen, whichever consoles were,
  and then holding the settings of the sections chosen alone (see "Which settings travel" in
  `docs/product/settings-in-a-configuration.md`);
- **accounts** — present exactly when they were chosen, and then always every account, whichever consoles were.

No region says anything about another: a document that carries the application settings is not therefore one of every
console, and one that carries a single console may carry the settings and the accounts too. An export that would hold
nothing — no console, no section of the application settings and no accounts — is refused (`export_empty`). A request
to the daemon that does not say which consoles exports the one it names, or every console when it names none; one that
does not say which sections of the application settings are included includes every section exactly when every
console is exported, and none otherwise; and one that does not say whether the accounts are included leaves them out.

Within a document the consoles stand in the order they were created and each console's projects by name, so the same
configuration exports the same way every time. Every project of a console travels, whether or not its directory is a
git repository.

### What each console carries

Its name, its avatar image if it has one, its default agent for console sessions, its default agent for project sessions
— none while it follows the one for console sessions — the account it names for each agent, the models and reasoning
efforts it sets for its console sessions and for its project sessions, one pair of each per agent, and its console
session instructions if it has any (see "Console session instructions" in `docs/product/consoles.md`). An import
restores the instructions trimmed, as saving them in the console dialog does, and a document that does not carry them
gives the console none. A document that does not carry a default agent for project sessions gives the console none, so
its project sessions follow its default agent for console sessions. The project session models and efforts are always
in the document, one entry per agent; an agent whose entry leaves a model or effort out has it unset.

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
whose `git_workflow` leaves an item out follows the application-wide value for it, as one giving it `null` does;
a project without a `git_workflow` object cannot be read, and the import is refused. Export and import preserve the
full length of project names and descriptions, including values beyond the editing limits in "Project names" and "The
project's description" in `docs/product/projects.md`; an import neither rejects nor truncates them for their length.

An import treats a project's path as a starting point and never as a place to write (see "Where an imported project
goes" below).

### Project paths

An export either includes full project paths or leaves them out; the export dialog leaves them out unless its checkbox
is checked, and a request to the daemon that does not say which includes them. Either way the document has the same
format and version. The choice is about projects alone: the accounts' directories and the default clone directory are
written whatever it says (see "The accounts" below and "The application settings an export carries" in
`docs/product/settings-in-a-configuration.md`).

In either mode a project's remote URL goes into the document without any credential it carries, removed as a remote
URL is whenever it is shown (see "A credential is never shown" in `docs/product/agent-settings-tools.md`), so a
document never carries one: `https://user:token@example.com/r.git` is written `https://example.com/r.git`.

- **Full paths included** — each project's path is its absolute directory path on the exporting machine, and its remote
  URL is the one recorded for it, whatever kind of URL it is. A project's path, a remote URL that is itself a local
  path or a `file://` URL, and an account directory or a default clone directory outside the home directory are the
  only paths of the exporting machine anywhere in the document, which is the thing to look at before handing the file
  to someone else.
- **Full paths left out** — each project's path is only the last segment of its directory path, its directory name
  (empty for a path with none, such as `/`), and its remote URL is kept only when, without its credential, it may be
  cloned from (see "What may be cloned from" below); any other remote URL, such as a local path or a `file://` URL, is
  left out. Such a document holds no path of the exporting machine but, when it carries the accounts, an account
  directory outside the home directory, and, when it carries the Git section of the application settings, a default
  clone directory outside it.

### How an account reference travels

An account reference — the account a console or a project names for an agent — travels only in a document that carries
the accounts region. **A document without it names no account at all**: every console is written on each agent's
default account, a project that names an account is written as following its console, and a project pinned to the
agent's default account stays pinned to it. Importing such a document therefore binds nothing by name and nothing in it
falls back (see "Accounts are rebound by name" below).

In a document with the accounts region, a reference travels as the account's **name alone**,
and which agent it belongs to is settled by the setting it sits in — the console's Claude Code account names a Claude
Code account. Neither the account's stored id nor its config directory is in the reference: the id means nothing
elsewhere and the directory is the layout of the machine it was on. An import rebinds every reference by name to the
importing machine's own accounts, which include those it creates from the document's accounts (see "Accounts are rebound
by name" below).

A reference to an account that has since been deleted, or that belongs to another agent than the setting it sits in, is
dropped from the document, leaving exactly what deleting that account leaves behind: the console on that agent's
default account, the project back to inheriting its console's reference (see "Agent config directories" in
`docs/product/consoles.md`).

There a project keeps all three states its account setting can be in: inheriting its console's reference, the agent's
default account pinned even where the console names another, or a named account.

### The accounts

The accounts region is the structure of the exporting machine's accounts (see `docs/product/agent-accounts.md`), and
never a credential, a login state or a quota reading. It holds:

- the **added agents**, in their order (see "Added agents" in `docs/product/agent-accounts.md`); left out while the
  exporting machine's list is still undecided, and an import of such a document then adds no agent;
- for each added agent — every agent while the exporting machine's list is undecided — and each other agent that has an
  account the user owns, a default account the user has named or a default account turned off, in the order Claude
  Code, Codex, Grok Build:
  - the **default account**'s name as the user gave it, if any, its place in the agent's display order — how many
    accounts come before it — and whether it is turned off. The default account's directory, which the agent resolves
    on each machine, is not carried;
  - each **account the user owns**, in the agent's display order (see "Reordering accounts" in
    `docs/product/agent-accounts.md`): its name, its directory and whether it is turned off;
- the **agent account settings**, the settings of the Agent accounts section that apply to all of an agent's accounts
  or to every agent (see "The agent account settings an export carries" in
  `docs/product/settings-in-a-configuration.md`).

No account carries its stored id, nor anything of its sharing of the default configuration (see "What is not
configuration and never travels" below).

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
configuration, and are in no document. No console, project or account carries its stored id. Nothing to log an account
in with — credentials, login state — is in any document, so every account an import creates starts not logged in; nor
is the default account's directory. The added agents, the accounts and their order, the turned-off choices and the
default accounts' names travel only in the accounts region (see "The accounts" above). Account quota readings are not
stored at all and are in no document either (see "How a quota is read" in `docs/product/account-quota-readings.md`).

**A setting whose turning on itself changes files outside Octoboard's own directory** (`~/.octoboard`; see "Files
Octoboard owns" in `docs/product/application-lifecycle.md`) **is in no document**, and the user turns it on again by
hand on the importing machine. Today that is an account's sharing of the default configuration, which links the
account's directory to the default account's (see "Turning sharing on" in
`docs/product/sharing-the-default-configuration.md`): an export never writes it, an import never turns it on, and every
account an import creates starts not sharing. A setting that only has Octoboard act on files outside that directory
later, as it runs — pulling repositories automatically, deleting archived sessions, switching an account at its usage
limit, starting usage windows — is not of this kind, and travels like any other (see
`docs/product/settings-in-a-configuration.md`). A provider setting its provider does not declare as carried stays on
its machine too (see "Provider settings" in `docs/product/per-agent-settings.md`); today every provider setting is
declared as carried.

### Versions

The format marker tells a configuration document from any other JSON. The version is a whole number, **3** today, and
is raised only when a change would make a reader written for a released version misread a document — a field dropped,
or given another meaning. Adding a field is not such a change, and a reader ignores a field it does not know. Version
changes and upgrade steps are written only between released versions: a change no release has carried is made in place,
at the current version, and raising the version past a released one adds a step that upgrades a document of that
version.

An import reads **versions 1 to 3**: version 1 is the one the first beta releases wrote, and is upgraded to the current
version as it is read, before anything else looks at it, so the inspection and the import see the same document. It
refuses a document whose format it does not recognize, and one of any other version, older or newer, before it creates
anything; the refusal names the versions it reads. Upgrading a version 1 document:

- moves the agent account settings — automatic account selection, automatic switching at the usage limit, starting
  usage windows, the provider settings and the agent tags — from its application settings into its accounts region,
  where version 2 carries them (see "The agent account settings an export carries" in
  `docs/product/settings-in-a-configuration.md`); a document that carries them but no accounts region is given one
  that holds those settings alone, so they are kept, and are applied when its accounts are imported;
- drops each account's sharing of the default configuration, which an import does not read (see "What is not
  configuration and never travels" above);
- reads what version 1 never carried — the default clone directory, whether a default account is turned off, and the
  agent account settings of a document without application settings — as not said, so an import leaves the importing
  machine's own as they are.

Upgrading a version 2 document drops its application settings' setting for ordering the sidebar by status, which an
import no longer reads: the order of sessions and projects travels instead as the Sorting of the General section (see
"The application settings an export carries" in `docs/product/settings-in-a-configuration.md`).

An export always writes an entry for every agent in `accounts`, `launch` and `project_session_launch`. An import reads
an agent's entry that is left out as unset: a console's account is then the agent's default account, a project's
account inherits its console's reference, and the model and effort are unset. An entry of `accounts`, `launch` or
`project_session_launch` keyed by an agent this build does not support (see `docs/product/supported-agents.md`) is
ignored, as are such an agent's `provider_settings` and `agent_tags` entries (see "The agent account settings an export
carries" in `docs/product/settings-in-a-configuration.md`). A document that names such an agent anywhere else — among the added agents (`added_agents`), as an
account's agent, or as a console's or project's default agent or a console's default agent for project sessions —
cannot be read, and is refused as one (see "What is refused, and what is reported instead" below).

A document also carries, under `exported_by`, the version of Octoboard (its daemon) that exported it. It is there only
for diagnosing a document that misbehaves: an import never reads it and it has no bearing on the version number.

## Importing

An import is two steps. **Inspecting** reads a document and answers what importing it would meet on the daemon's host;
it changes nothing and runs no `git`. **Applying** takes the same document, together with which of its regions to
import, which sections of its application settings and which agents of its accounts, the choice made for each project
and each account, and whether to keep the host's own default clone directory, and creates. The consoles and projects
are imported unless the import is told to leave them out; the accounts, with the agent account settings, and each
section of the application settings only when it is asked for them.
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
default when none of them is. The default clone directory is the one in effect once the import has made its choice
about the document's (see "The default clone directory on import" in `docs/product/settings-in-a-configuration.md`):
the inspection answers the offers under the host's own directory and, where the document carries another one, under
the document's too. A project recorded by its directory name alone (see "Project paths" above) is never
offered the recorded path, since a name is not an absolute path, so it can be offered only the directory under the
default clone directory and a fresh clone; an empty name derives no directory, which leaves only the clone. A project
with nothing offered is left out of the import unless the user points at a directory for it.

**A path written on another operating system.** A document records each project's path as the exporting machine wrote
it. A path that is absolute in the other operating system's style — a Windows path such as `C:\work\app`,
`C:/work/app`, `\\server\share\app` or `\\?\C:\app` read on macOS or Linux, or a path starting with `/` read on
Windows (where `//server/share` is Windows' own form) — names nothing on the daemon's host, so no directory is offered
from it: neither the recorded path nor a directory derived from it, under this machine's default clone directory or
the document's. The inspection marks such a project (`foreign_path`), and asking for its
recorded path or its derived directory anyway is refused for that project as unavailable
(`import_choice_unavailable`). A fresh clone stays offered under the usual rule, and the user can still point at a
directory for the project. A project recorded by its directory name alone, and a path in this host's own style, are
read as above.

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

A URL that carries a credential, in the sense of "A credential is never shown" in
`docs/product/agent-settings-tools.md`, is not accepted either, as the console session's `add_project` refuses one (see
"Adding a project with its settings" there), so a hand-edited document cannot plant a credential in a clone; the
inspection gives such a URL without its credential.

A URL that is not accepted is not offered as a clone, and asking for the clone of such a project anyway is refused for
that project.

### Importing accounts

Inspecting a document that carries accounts says, for each of its accounts, agent by agent in the document's order,
what importing it would come to:

- **Existing** — an account of the same agent already has its name, compared trimmed and ignoring letter case. An
  import uses that account as it is and **never changes it**: the document's directory and turned-off choice are not
  applied to it.
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
form. A directory that is absolute in the other operating system's style (see "A path written on another operating
system" under "Where an imported project goes" above) names nothing on the daemon's host, so it resolves to nothing and
the account cannot be created as the document has it, with the form's reason that the directory is not absolute; the
inspection marks such an account (`foreign_directory`) so the review can say why instead. Any account but an existing
one can be left out. An account whose directory is in use or cannot be used can be given **another directory** instead,
absolute or starting with `~/`; one whose name is the problem can only be left out, since no directory would help.

The inspection also says which of the document's added agents this machine has not added (none while this machine's
list is undecided), and, for each agent, the name the document gives its default account and whether an import would
apply it: only when this machine's default account of that agent has no name of its own, and the name is neither blank
nor "Default" and is no account's of that agent, the ones the import would create included; and, where the document
says whether the default account is turned off, what it says and whether that differs from this machine's.

An import of the accounts can be limited to some of the document's agents; without that it takes every agent the
document carries something for — an entry of accounts, a place among the added agents, or provider settings or tags.
Of an agent left out, nothing is applied: it is not appended to the added agents, its default account's name and
turned-off choice are not applied, none of its accounts is created or used, and its provider settings and tags are not
applied; the three switches of the agent account settings are applied whichever agents are taken. Naming an agent the
document carries nothing for is refused (`import_agent_missing`, naming the agent). The steps below then run for the
agents taken alone, and the progress and the result report only what they applied.

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
4. A created account takes the document's turned-off choice, and does not share the default configuration (see "What
   is not configuration and never travels" above).
5. The default account's name is applied under the rule above, checked again at that moment, so a name the default
   account was given meanwhile is kept. The default account is turned off, or back on, as the document says, where it
   says and that differs from this machine's — for every agent the document lists, so a document whose default account
   of an agent is on turns that agent's default account back on here (see "Turning an account off" in
   `docs/product/agent-accounts.md`).
6. The agent account settings are applied (see "The agent account settings are applied with the accounts" in
   `docs/product/settings-in-a-configuration.md`).

Every change appears in every window as the same change made by hand does. **Nothing is logged in**: the accounts are
created without credentials, and the user logs each one in afterwards (see `docs/product/logging-accounts-in.md`).

### Accounts are rebound by name

Every account reference of the document is bound to the account of the daemon's host that has the same name and belongs
to the agent whose setting the reference sits in, matched **ignoring letter case** — which is how account names are
unique within an agent in the first place (see "Adding, editing and removing an account" in
`docs/product/agent-accounts.md`). When the accounts are imported too, they are created first, so a reference to an
account the import creates binds to it, whichever directory it was given, and falls back only when that account was left
out or failed, or belongs to an agent the import left out — unless an account of that name exists on the host already. A reference that matches no account falls back to that agent's **default account**, pinned rather than
left inheriting, and every such fallback is listed one by one: the inspection says which references would fall back,
the result of an import which ones did. Since no account the import would create exists yet when it inspects, the
inspection lists the references to them as well; the import review tells them apart by what is being imported (see
"Account fallbacks" in `docs/product/configuration-export-and-import-dialogs.md`). A document exported without the
accounts region holds no named reference (see "How an account reference travels" above), so it has no fallback.

### What applying does

Applying goes region by region: first, when the Git section of the application settings is asked for, the document's
default clone directory, unless the import is told to keep the host's own, so that the projects placed under it follow the choice
(see "The default clone directory on import" in `docs/product/settings-in-a-configuration.md`); then the accounts and
the agent account settings, when they are asked for, so that the consoles' and projects' references can bind to the
accounts (see "Importing accounts" above); then the consoles and projects, unless the import is told to leave them out,
in which case no console or project is created and no choice for a project is read; then the rest of the application
settings of the sections asked for (see "Application settings are left alone unless they are asked for" in
`docs/product/settings-in-a-configuration.md`).

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

### What is refused, and what is reported instead

Refused before anything at all is created:

- a document that is not a configuration document, one of a version this Octoboard does not read (see "Versions"
  above), and one that cannot be read;
- a choice for a project that cannot be right: it names a project the document does not have, it names one project
  twice, or the directory it points at is neither absolute nor starts with `~/`;
- a choice for an account that cannot be right: it names an account the document does not have, it names one account
  twice, it gives both another directory and leaving the account out, or neither, or the directory it gives is neither
  absolute nor starts with `~/`;
- a request to apply a section of the application settings that a document does not carry
  (`import_settings_missing`, naming the section), to create accounts it does not carry (`import_accounts_missing`), or
  to import the accounts of an agent it carries nothing for (`import_agent_missing`, naming the agent);
- anything creating a console by hand would refuse of any console in the document — an avatar that is not an image
  (`icon_not_an_image`), or one larger than 256 KiB (`icon_too_large`; see "Avatar" in `docs/product/consoles.md`).
  Since a document can carry several consoles, the refusal names the console whose avatar it is (the daemon gives its
  name in `console_name`), and the error alert says which console's avatar is not an image or is larger than the
  limit. The console dialog turns a picked image into one scaled to the limit, so only an import meets either refusal.
  The remedy for each is to give that console, in the Octoboard the configuration came from, an avatar that is an
  image, or a smaller one, then export again and import the new file.

**Past that point nothing is refused and nothing is undone**, because by then earlier accounts, consoles and projects
exist and have already appeared:

- an account that cannot be created is recorded as failed with the reason, and the remaining accounts, then the
  consoles, are imported as usual;
- agent account settings that could not be applied are reported as such, with the accounts;
- a console that cannot be created is recorded as failed, every project of it as not attempted, and the next console of
  the document is imported;
- a project that cannot be placed — the recorded path is gone, a clone fails or the directory it would create already
  exists, the directory pointed at is not there, the directory is already associated with that console — is recorded as
  failed with the reason, and the remaining projects are imported as usual;
- application settings that could not be applied are reported as such, and the import stands; a default clone
  directory written before them stays written, and the result says so.

The result says, for every console and every project of the document when the consoles were imported, whether it was
created, left out for want of a choice, failed and why, or never attempted; and, when the accounts were imported, for
every account of the agents taken whether it was created, already existed, was left out or failed and why, which
agents were added, which default accounts were named and which were turned off or on, and what came of the agent
account settings — so the outcome never has to be pieced together from what appeared in the window.

### How long an import takes

An import that clones can run long. The clones are made one at a time, in the document's order, and each is cut off
after **30 minutes**, failing its project and leaving no directory behind (see "Associating a project" in
`docs/product/projects.md`). There is no way to cancel an import while it runs, and it answers only once the last clone has
finished — which can be as long as all of them together. Everything created before that point is already in the
window, and where the import stands meanwhile is reported as it goes (see "Progress while an import runs" below).

### Progress while an import runs

An import request may name itself with a token of the client's choosing (`import_id`). For such an import the daemon
reports where each of its items stands while it runs, broadcasting `configuration_import_progress` events that carry the
token, to every client; a client keeps those with its own token. An import that names none is reported on by no event.

- **The items** are the default clone directory when the import writes it, each account the import handles — of the
  agents taken alone —, the agent account settings when the accounts imported carry them, each console, each project
  it places, and the rest of the application settings, of every section applied at once, when they are applied. A project left out is not one, nor an account left out — but an account
  that already exists is, whatever was chosen for it, since it is used as it is.
- **Once nothing can be refused any more**, every item is reported waiting, in the order the import handles them: the
  default clone directory, the accounts, the agent account settings, then each console followed by its projects, then
  the application settings. A request refused before anything is created reports nothing.
- **Each item then moves** to being created as the daemon starts on it — being cloned, for a project that is cloned —
  and ends done, failed (with the same failure the result reports for it), or, for a project whose console could not
  be created, not attempted. A console is done as soon as it is created, its projects then reporting for themselves.
  An account that already exists goes straight to done. A default clone directory that cannot be written fails, and
  the application settings then fail with the same failure without having been worked on.

The events are sent before the reply, but the last of them can reach a client after it; once a client has the reply,
it ignores the later events of that import. A failure the daemon answers with an error after reporting progress
ends the import.
