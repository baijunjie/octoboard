# Consoles and projects

A **console** groups a set of projects and owns a working directory of its own. A **project** is a directory on a
host, associated with one console. A console is created and managed by the user; a project is associated either by
the user or by its console session. Sessions that run inside them are described in
`docs/product/sessions.md`.

## Consoles

A console carries:

| Field | Set by | Notes |
|---|---|---|
| Name | the user, required | Free text. |
| Console session agent | the user | The agent the console's console sessions run. |
| Default agent | the user | The fallback agent for sessions opened under this console's projects. |
| Agent accounts | the user, optional | One per agent: the account that agent's sessions in this console run under, the agent's default account when none is picked; see "Agent config directories" below. |
| Avatar | the user, optional | A custom image shown in place of the default avatar, in a circle; see "Avatar" below. |
| Working directory | Octoboard | `~/.octoboard/consoles/<console id>/`, created when the console is created. Not settable and not changeable. |

Both agent fields take one of the three supported agents (Claude Code, Codex, Grok Build) and default to Claude Code in
the creation form — or, once Claude Code is determined unavailable, to the first of the others that is not, so neither
starts on an agent that cannot be chosen. Both rows list all three every time, whether or not the machine has each one
installed: once Octoboard has determined an agent unavailable (its binary does not resolve on the user's login shell
`PATH`), that row names it rather than leaving it out, labels it as not installed, and will not let it be chosen — it is
shown rather than hidden so the reason it cannot be picked is on screen, and it is still reached with the arrow keys
and announced as disabled. Before that determination has landed every
agent is offered normally; an agent already chosen before it was found unavailable is left exactly as it is.

Multiple consoles can exist side by side and are independent of each other.

### Avatar

A console is drawn as a round avatar on the rail (see "The console switcher" in `docs/product/sidebar.md`). Without a
custom image it is a network glyph on a coloured fill, one of seven, picked from the console's name: the same name
always gets the same fill, two different names can still share one, and renaming a console can change its fill.
Spaces and invisible characters around the name, and whether accented letters were typed precomposed or combined, do
not change it. Each fill has its own light and dark value (see "What follows the choice" in
`docs/product/appearance.md`). The console dialog's **Avatar** field shows the current avatar, following the name as it
is typed, with **Choose image** to pick an image file from disk and **Remove image** to go back to the default. A chosen
image is cropped to its centred square and scaled to 128 x 128 pixels in the window before it is saved, so any
size or shape of picture is accepted; a file that is not a readable image is refused in the dialog. As with the other
fields, an avatar that was not touched is left as it is when the console is saved.

### Agent config directories

A console refers to one **account** per agent — a named config directory kept once for the whole application rather
than typed into each console. Accounts are added, renamed, repointed and removed in the Agent accounts section of
Settings (see `docs/product/settings.md`); the console dialog only picks among them.
Each agent's config directory is where that agent keeps its own configuration, login state and conversation history:

| Agent | What the directory is | The agent's usual default |
|---|---|---|
| Claude Code | what Claude Code itself takes from `CLAUDE_CONFIG_DIR` | `~/.claude` |
| Codex | what Codex itself takes from `CODEX_HOME` | `~/.codex` |
| Grok Build | the Grok home, what Grok itself takes from `GROK_HOME` | `~/.grok` |

How each agent is pointed at its directory is in "The launch environment" and "Per-agent specifics a user will notice"
in `docs/product/launching-agents.md`.

The console dialog shows one picker for each agent currently selected as the console's console session agent or
default agent — one row or two, in the order Claude Code, Codex, Grok Build — labelled with that agent's name and
listing that agent's accounts, its default account first. An account can therefore be picked only for an agent selected
in one of those two fields. A reference already stored for an agent that is not currently selected is kept as it is: it
is neither shown nor saved by the dialog, and it still applies to that agent's sessions, such as those of a project
whose own default agent it is. To see or change it, select that agent as the console session or default agent again.
A picker lists accounts whether or not the agent is installed, and no account is unpickable for its directory being
missing.

- **The default account**, which a new console starts on, is the state of pinning nothing: that agent's sessions use
  whatever the user's login shell exports for the agent's variable, and the agent's own default when it exports none.
- **Any other account** applies its directory to every session of that agent opened in the console afterwards — the
  console session and project sessions alike, including a session whose agent was chosen for that session alone — and
  takes precedence over the value in the user's shell environment (see "The launch environment" in
  `docs/product/launching-agents.md`). A session reads only its own agent's account; sessions of the other agents are
  unaffected. A session can also be opened under a different account of its agent than the console's (see "Opening a
  session" in `docs/product/sessions.md`).

For Claude Code, an account pointing at `~/.claude` is not the same as the default account: whenever a directory is
set, Claude Code reads its global config file from `.claude.json` inside that directory instead of from
`~/.claude.json`. For this reason the default account is never offered as a second, pinned account of its own — it
already covers it.

For Grok Build, the directory does not have to already be a Grok home when it is set; whether it is one is checked
later, at launch, rather than here. What happens then is in "Per-agent specifics a user will notice" in
`docs/product/launching-agents.md`.

An account's directory is checked when the account is added or edited, in Settings, and a failure is shown in the form
with nothing saved:

- surrounding whitespace is trimmed;
- a leading `~` or `~/` is expanded to the home directory of the host the daemon runs on;
- the result must be an absolute path;
- it is normalized lexically — a trailing `/`, `.` components and `..` components are resolved; a symlink is kept as
  typed.

Existence is **not** checked: a directory that does not exist yet is accepted, and is created on first launch — by
Claude Code itself, and by Octoboard for Codex, which refuses a missing one (Grok Build excepted, see below). What is
stored, and shown for the account, is its expanded absolute path.

**A session keeps the account and the directory it last recorded until the user switches it.** A session takes its
account when it is opened — the one picked for it, else the console's reference for its own agent, else the default
account — and nothing but the user changes it afterwards: not an edit to the console's reference, not an edit to the
account's directory. Resuming or reopening it relaunches with that same directory, because the agent keeps the
conversation inside it. The user can move the session to another account of its agent from its action menu; that
copies the conversation into the target's directory and carries the session into that account's whole setup, not only
its login (see "Switching a session's account" in `docs/product/sessions.md`). A session opened on the default account
holds no directory and resumes under whatever the user's shell exports at that moment.

If a session's directory no longer exists when it is launched or resumed, the launch is refused rather than
started — but only for a session that has a conversation on the agent's side to resume. A new session, and one that
was opened but never had a turn, launches into the missing directory instead, and the directory is created as above;
a Codex session whose directory Octoboard cannot create is refused on this same path. This narrower refusal holds for
all three agents, and the message names the agent and the directory and says to recreate it or to point the account at
a different one. Grok Build is refused on this same path whenever its pinned directory is not an initialized Grok
home — one Grok has actually been run against — whether or not there is a conversation to resume, since an empty
directory would keep neither its login nor its conversation; see "Per-agent specifics a user will notice" in
`docs/product/launching-agents.md`.

### Editing a console

The name, the console session agent, the default agent and the accounts the dialog shows can be changed, and an account
can be put back on the agent's default account. Changing an agent or an account affects sessions opened afterwards; a
session that already exists keeps the agent and the account and directory it was started with. A change to the console
session agent therefore shows in the next console session started from the console sessions section, while the console
sessions already running keep the agent they were started with (see "Console sessions and project sessions" in
`docs/product/sessions.md`).

An account is saved only when it is changed: saving a console with a picker untouched succeeds regardless of what has
happened to the account it refers to since. Removing an account in Settings puts every console that referred to it back
on the agent's default account.

### Deleting a console

Deleting a console asks for confirmation, and the Delete button stays disabled until the word for "delete" in the
current language — "DELETE" in English, and in any language not yet translated (see "What is translated so far" in
`docs/product/language.md`), "删除" in Simplified Chinese — has been typed into the confirmation's field. What is typed
is shown in capitals and compared ignoring case and surrounding spaces.

Deleting a console is **refused while any of its sessions still has a running process**, or is being launched or
resumed at that moment — those must be archived (or the application restarted, which interrupts them) first. The
error names the reason.

When it goes through, deleting a console also deletes every project association under it, every session record
belonging to it, archived sessions included, with the last output Octoboard kept for each, and every page its console
sessions pushed to the report panel, and removes the console's own working directory under `~/.octoboard`. No project
directory is touched.

## Projects

A project carries a name, the directory it points at, an optional default agent, how it was associated, the remote
URL it was cloned from (for a git association), whether the user has pinned it (see "Order of projects and
sessions" in `docs/product/sidebar.md`), and any number of tags. Every project is bound to a host (see "Hosts" below).

A project also carries whether the user has given Octoboard permission to press any agent's trust confirmation for
its directory without asking. A project starts without that permission, including one the console session associates;
it is given only by "Trust and continue" in the dialog Octoboard shows when a session of the project stops on its
agent's trust confirmation, and it is not part of the project's editable fields. Separately from any project, the user
can trust a whole folder, which covers every project under it — including any associated there later, by the user or
by the console session — without giving any of them that permission; a trusted folder is not stored with a project, so
editing or removing a project does not affect it (see "Trusted folders" in `docs/product/folder-trust.md`). The rules
are in "The trust permission" in `docs/product/folder-trust.md`.

### Tags

A tag is a short free-form label the user puts on a project to group projects, and the sidebar's filter narrows the
project list by them (see "Filtering the project list" in `docs/product/sidebar.md`). There is no list of tags to
create or manage: the first time a tag is used, the user types it in a project's settings, and once that project is
saved it is offered for other projects of the same console. The tags offered are simply the distinct ones the
console's projects carry, so a tag no project carries any more is gone from the choices. A project can carry as many
tags as the user likes. A tag is trimmed, an empty one is dropped, and one that matches another of the project's tags
ignoring case is dropped, the first spelling staying.

### Associating a project

There are three sources:

| Source | Input | Result |
|---|---|---|
| A single directory | a path | That one directory becomes a project. The path must exist and be a directory. |
| A parent directory | a path | Every git repository **directly beneath** that path becomes its own project. Only one level down is scanned; a checkout nested deeper belongs to the repository above it. Associating nothing is an error: if no git repository is found directly beneath the path, the request is refused and names the directory. |
| A git repository URL | a repository URL plus a parent directory | The repository, from any git remote and not only GitHub, is cloned into a new directory beneath the parent, and the clone is then associated. The parent directory is filled in with the default clone directory (see below), which can be changed in the form or left blank to use that directory anyway. |

For a single directory, the form asks the daemon about the path as soon as it is entered or picked (after a short pause
when typed): the path must be an existing directory, and its agent is read and preselected in the Default agent field,
which stays changeable; see "The project's default agent". While the answer for the current path is outstanding the
form says it is checking, and the fields below the path (name, tags, default agent) and the submit button are disabled;
a refusal such as "not a directory" is shown under the path field and keeps them disabled. Only the latest path's
answer counts, and a changed path is read again and its result preselected anew.

For a git URL, the form first checks the repository: once the URL stops changing, the daemon fetches the remote's tip
commit without file contents or history into a scratch directory, which proves the remote can be read with the user's
credentials and shows its top-level names. Every field below the URL (clone into, name, tags, default agent) and the
submit button stay disabled until that check has succeeded for the URL as it stands, an empty one included; while it
runs the form says it is checking, and a failure is shown under the URL field with git's own reason and keeps them
disabled. A success enables them and preselects the detected agent. The check is bounded by a timeout, and its scratch
directory is removed when it ends.

A path may be entered by hand or picked with the directory browser. A leading `~` is expanded to the home directory
of the host the daemon runs on. The path must then be absolute: a relative path is refused, for every source. It is
recorded lexically normalised — `.` components dropped, each `..` folded into the component before it, no trailing
slash — without resolving symbolic links, so a path through a link is kept as written. A project's recorded path is
what a trusted folder is compared against (see "Trusted folders" in `docs/product/folder-trust.md`).

The console session can associate a project itself, from the same three sources and under all the
rules in this section (see "The console session's tools" in `docs/product/hub-orchestration.md`).

The **default clone directory** is where a clone lands when no parent directory is named. It is `~/Projects` (expanded
to the home directory) until the user sets one in Settings' General section (see "General" in
`docs/product/settings.md`), and follows the same path rules as any other path here. The form starts its "Clone into"
field with it, and the console session's `add_project` falls back to it when it gives no `path` for a clone; a path is
still required for the other two sources. The directory is created if it does not exist.

For a git association:

- The target directory name is taken from the URL's last segment with any `.git` suffix removed. Both
  `https://host/owner/repo(.git)` and `git@host:owner/repo(.git)` are understood.
- If that directory already exists, the request is refused rather than cloning into or over it.
- The clone runs with the user's own shell environment, so it uses the `git` on the user's `PATH` and their git
  configuration and credentials. Capturing that environment is bounded the same way a session launch's is, and a
  capture that does not complete refuses the clone (see "The launch environment" in
  `docs/product/launching-agents.md`).
- A clone can take minutes; other state updates keep flowing while it runs.

A directory that is already associated with the same console is skipped rather than associated twice. Associating a
parent directory again after new repositories have appeared beneath it therefore adds only the new ones. If every
directory found is already associated, the request is refused and says so.

### Project names

A name can be given only when a single project is being associated; it then overrides the derived name. Otherwise, and
for every project a parent-directory association produces, the name is the directory's own name.

### The project's default agent

A project may carry its own default agent, or leave it unset to inherit the console's. The selection order for a new
session is in "Which agent a session uses" in `docs/product/sessions.md`.

The project dialog, both when associating and when editing a project, offers "unset" as an "Inherit from console"
choice listed first and preselected for a new project (a parent directory preselects "Detect from files" instead); it
names the agent the console's default currently resolves to, shown with that agent's icon dimmed, or is a bare "Inherit
from console" when the console is not known. Choosing a specific agent stores it as the project's own default.

Associating a project with no default agent given looks at the directory's top level once and stores the agent it is
set up for as the project's own default, shown and editable in Project settings like any chosen one. For a single
directory or a git URL the form has already done this before submitting and shows the result preselected in the field
(a choice made by hand stays only until the path or URL changes, which reads it again); "Inherit from console" chosen
after that means none, and the project inherits the console's default. For a parent directory there is no pre-read, and
the field offers one more choice, "Detect from files", listed first and selected on arriving at that source (and given
up when the source changes): each repository found is detected on its own at association, and one that cannot be
decided follows the console's default. "Inherit from console" keeps its meaning of following the console's default with
no detection, and choosing an agent applies it to every repository found. The form explains the detect choice under the
field. The same rules apply to what is read: `CLAUDE.md` or `.claude` means Claude Code, `.codex` means Codex, and
`.grok` means Grok Build; `AGENTS.md` or `.agents` mean Codex too, but only when `.grok` is absent, since Grok Build
reads them as well. When no marker is found, or markers of more than one agent are (a directory with both `CLAUDE.md`
and `AGENTS.md`, say), nothing is stored and the project keeps inheriting the console's default. Nothing is stored
either when the detected agent is known not to be on the login shell's `PATH`. A default chosen by the user is never
replaced by detection, and this holds for every association source and for projects the console session associates,
which always detect when they give none.

### Editing a project

Only the name, the default agent and the tags can be changed, besides pinning and unpinning the project. The name is
changed from the project's Rename or Project settings, the default agent and the tags from Project settings (see
"Project rows" in `docs/product/sidebar.md`). The tags field sits between the name and the default agent: typing in it
offers the tags already in use that the project lacks, in a list that opens below the field and stays open after a pick
so several can be added in a row, and Enter adds what was typed as a tag, or the highlighted suggestion if there is one;
leaving the field adds it too. The tags the project carries show inside the field, ahead of the text input, in a
light accent tint with accent-coloured text, and wrap onto more lines as they grow, the field growing with them; each
has a remove button, and removing one puts the focus in the text input. Backspace in the empty text input removes the last tag, one per key press (holding it down does not
keep removing). Clicking the field's empty area puts the focus in the text input. The same field is in the dialog that associates a project, and the tags chosen there go on every
project that association creates. The association itself — the source, the directory, the remote URL — is fixed once the
project exists; a project that should point somewhere else is removed and associated again.

The name cannot be cleared: saving an empty name is rejected. The default agent can be cleared, which puts the project
back to inheriting the console's default.

### Removing a project

Removing a project **never touches the directory or anything in it**. It removes the association, and with it the
records of that project's sessions, archived ones included.

Removing a project asks for confirmation. **Sessions of the project that are still running do not block it**:

- With none running, the confirmation offers **Remove**.
- With some running, the confirmation lists them — each with its status glyph, its agent's icon and its title, in the
  sidebar's order (see "Order of projects and sessions" in `docs/product/sidebar.md`) — under a warning that that
  many sessions are still running and will be ended, and its button reads **End and remove**. The list follows
  sessions starting or stopping while the confirmation is open. Confirming ends each running session's process the way
  archiving does (see "Archiving, interruption and resuming" in `docs/product/sessions.md`), then removes the project
  with all its sessions and the last output Octoboard kept for each. Sessions bound to another session of the project
  are ended before the session they are bound to, so ending that one is never refused over them.

Removal is refused only while a session of the project is being launched or resumed at that moment, since that
session has no process yet that could be ended; the error names the reason, and nothing is ended or removed.

## Browsing directories

The directory browser lists directories only — a project is a directory — and hides entries whose name begins with a
dot. Each entry is flagged with whether it is a git repository. Entries are sorted case-insensitively by name. The
listing always comes from the host the projects live on, never from the application's own filesystem view.

In the browser, the listed directory's subdirectories are one list, with a parent-directory entry at its top except at
the filesystem root — muted, marked with an up-arrow icon and labelled as the parent directory, which is also what it is
announced as to assistive technology — each directory marked with a folder icon, and a "git" tag at the end of each
entry that is a git repository. Clicking an entry, or pressing Enter on it, lists that directory. The list is a single
Tab stop: the arrow keys, Home and End move between its entries. A directory with no subdirectories shows an entry
saying so, which does nothing. When the browser is opened on a path that does not exist (such as a default clone
directory not created yet), it lists the nearest ancestor that does exist instead (a `~/` path goes no higher than the
home directory), so there is always somewhere to navigate from; any other failure is reported as described next.

A listing can fail, and that is a normal path rather than an edge case: a packaged application is granted file access
per volume by macOS, and the user may decline the prompt or leave it unanswered. The failure is reported with the path
and with the hint that per-volume access has to be granted; whatever was listed before stays on screen so the browser
can be navigated back out of. The same failure can surface when a session is opened: a project directory the daemon
cannot reach is reported as such rather than silently starting the agent somewhere else.

## Hosts

Every project and every session is bound to a host. Only one host exists, the local machine, listed as
"This machine". Hosts cannot be added or edited.
