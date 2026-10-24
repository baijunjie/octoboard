# Projects

A **project** is a directory on a host, associated with one console (see `docs/product/consoles.md`). A project is
associated either by the user or by its console session. Most of a project's settings can be changed by its console
session as well as by the user (see `docs/product/agent-settings-tools.md`). Sessions that run inside projects are
described in `docs/product/sessions.md`, and carrying projects to another machine with their consoles in
`docs/product/configuration-export-and-import.md`.

A project carries a name, an optional description (see "The project's description" below), the directory it points at,
an optional default agent, how it was associated, the remote URL of the repository it points at (see "The project's
remote URL" below), whether the user has pinned it (see "Order of projects and sessions" in
`docs/product/order-of-projects-and-sessions.md`), any number of tags (see `docs/product/project-tags.md`), and — one
set per agent — what it says about the account its sessions run under and an optional model and reasoning effort for
them (see "The project's account, model and reasoning effort" below). Every project is bound to a host (see "Hosts"
below).

A project also carries whether the user has given Octoboard permission to press any agent's trust confirmation for
its directory without asking. A project starts without that permission, including one the console session associates;
it is given only by "Trust and continue" in the dialog Octoboard shows when a session of the project stops on its
agent's trust confirmation, and it is not part of the project's editable fields. Separately from any project, the user
can trust a whole folder, which covers every project under it — including any associated there later, by the user or
by the console session — without giving any of them that permission; a trusted folder is not stored with a project, so
editing or removing a project does not affect it (see "Trusted folders" in `docs/product/folder-trust.md`). The rules
are in "The trust permission" in `docs/product/folder-trust.md`.

## Associating a project

There are three sources:

| Source | Input | Result |
|---|---|---|
| A single directory | a path | That one directory becomes a project. The path must exist and be a directory. |
| A parent directory | a path | Every git repository **directly beneath** that path becomes its own project. Only one level down is scanned; a checkout nested deeper belongs to the repository above it. Associating nothing is an error: if no git repository is found directly beneath the path, the request is refused and names the directory. |
| A git repository URL | a repository URL plus a parent directory | The repository, from any git remote and not only GitHub, is cloned into a new directory beneath the parent, and the clone is then associated. The parent directory is filled in with the default clone directory (see below), which can be changed in the form or left blank to use that directory anyway. |

For a single directory, the form asks the daemon about the path as soon as it is entered or picked (after a short pause
when typed): the path must be an existing directory, and its agent is read and preselected in the Default agent field,
which stays changeable; see "The project's default agent". While the answer for the current path is outstanding the
form says it is checking, and the fields below the path (name, description, tags, default agent) and the submit button
are disabled; a refusal such as "not a directory" is shown in an alert directly under the path field and keeps them
disabled. Only the latest path's answer counts, and a changed path is read again and its result preselected anew.

For a git URL, the form first checks the repository: once the URL stops changing, the daemon fetches the remote's tip
commit without file contents or history into a scratch directory, which proves the remote can be read with the user's
credentials and shows its top-level names. Every field below the URL (clone into, name, description, tags, default
agent) and the submit button stay disabled until that check has succeeded for the URL as it stands, an empty one included; while it
runs the form says it is checking, and a failure is shown in an alert directly under the URL field, saying the
repository could not be read with git's own reason behind Details, and keeps them disabled. Neither alert opens an error alert (see "Failures shown on the screen" in
`docs/product/error-alerts.md`). A success enables them and preselects the detected agent. The check is bounded by a
timeout, and its scratch directory is removed when it ends.

A path may be entered by hand or picked with the directory browser. A leading `~` is expanded to the home directory
of the host the daemon runs on. The path must then be absolute: a relative path is refused, for every source. It is
recorded lexically normalised — `.` components dropped, each `..` folded into the component before it, no trailing
slash — without resolving symbolic links, so a path through a link is kept as written. A project's recorded path is
what a trusted folder is compared against (see "Trusted folders" in `docs/product/folder-trust.md`).

The console session can associate a project itself, from the same three sources and under all the
rules in this section, and a stricter one for a repository URL it clones (see "Adding a project with its settings" in
`docs/product/agent-settings-tools.md`).

The **default clone directory** is where a clone lands when no parent directory is named. It is `~/Projects` (expanded
to the home directory) until the user sets one in Settings' Git section (see "Default clone directory" in
`docs/product/settings.md`), and follows the same path rules as any other path here. The form starts its "Clone into"
field with it, and the console session's `add_project` falls back to it when it gives no `path` for a clone; a path is
still required for the other two sources. The form keeps the local directory and clone parent directory separately:
changing sources and returning retains each entered value. The directory is created if it does not exist.

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

## Project names

A name can be given only when a single project is being associated — a single directory, a git URL, or a parent
directory beneath which exactly one git repository is found; it then overrides the derived name. A blank name counts as
none given. Otherwise, and for every project a parent-directory association produces when it finds more than one
repository, the name is the directory's own name.

In a new project's dialog, the empty Name field previews the name derived from the current directory or repository
URL. For a directory, the preview follows the host's home expansion and lexical path normalization, including the
filesystem root; for a repository, it follows the clone's directory name. If a usable name cannot yet be derived, it
identifies the directory or repository name as the default instead. Changing the source, path or URL updates the hint
without replacing a name the user has typed. The hint never fills the field: submitting it empty still asks for the
derived name. An existing project's Name is required and shows an example when cleared.

An explicit name is trimmed and limited to 100 characters, counted as Unicode scalar values: a Chinese character or
an ordinary emoji counts as one; a sequence of several code points counts as several. Directory-derived names keep
their full length. Text ignored for a parent-directory association that finds several repositories is not checked.

The project dialog has a Name field when associating a single directory or a git URL, and hides it while the source is
a parent directory: a parent-directory association made from the dialog gives no name, so every project it creates is
named after its directory. Only the console session can give a name to a parent-directory association (see "Adding a
project with its settings" in `docs/product/agent-settings-tools.md`), and it applies under the rule above.

## The project's description

A project may carry a description: free text, over multiple lines, saying what the project is for. It is stored trimmed
and limited to 2,000 characters, counted as for names in "Project names" above; a blank one means the project has none.
A project starts with none unless its association gives one.

The project dialog shows it as the **Description** field right after the name, both when associating and when editing a
project, with a note under it saying what it is for and that agent sessions read it; empty, it shows a placeholder
saying there is no description. While the dialog keeps the fields below the path or URL disabled (see "Associating a
project" above), it is disabled too.

A description belongs to one project, so it applies under the same rule as a name (see "Project names" above): the
dialog hides the field while the source is a parent directory, as it hides the Name field, and a parent-directory
association that finds more than one repository stores none on any of them, whatever was given.

Agent sessions read it only as data, from the project's settings that the MCP tools return (see "A project's settings"
in `docs/product/agent-settings-tools.md`): the console session for every project of its console, a project session for
its own project. Octoboard writes it into no instruction file and into no text a session is launched or messaged with.

## The project's default agent

A project may carry its own default agent, or leave it unset to follow its console's default agent for project sessions
(see `docs/product/consoles.md`). The selection order for a new session is in "Which agent a session uses" in
`docs/product/sessions.md`.

The project dialog, both when associating and when editing a project, offers "unset" as a "Same as console" choice
listed first and preselected for a new project (a parent directory preselects "Detect from files" instead); a badge after
it names the agent the console's default for project sessions currently resolves to — the console's default for console
sessions while the one for project sessions is unset — with that agent's icon shown dimmed, or it is a bare "Same as
console" when the console is not known. Choosing a specific agent stores it as the project's own default. The agents listed
follow "Which agents the agent pickers list" in `docs/product/consoles.md`.

Associating a project with no default agent given looks at the directory's top level once and stores the agent it is
set up for as the project's own default, shown and editable in Project settings like any chosen one. For a single
directory or a git URL the form has already done this before submitting and shows the result preselected in the field
(a choice made by hand stays only until the path or URL changes, which reads it again); "Same as console" chosen
after that means none, and the project follows the console's default for project sessions. For a parent directory
there is no pre-read, and the field offers one more choice, "Detect from files", listed first and selected on arriving
at that source (and given up when the source changes): each repository found is detected on its own at association, and
one that cannot be decided follows the console's default for project sessions. "Same as console" keeps its meaning
of following that default with no detection, and choosing an agent applies it to every repository found. For this source
a line under the field says that an agent chosen here applies to every repository, and a "?" beside the field's label
explains the detect choice: what it looks for in each repository — `CLAUDE.md`, `AGENTS.md` and agent folders such as
`.claude` or `.codex` — and that a repository it cannot decide on inherits from the console. The same rules apply to
what is read: `CLAUDE.md` or `.claude` means Claude Code, `.codex` means Codex, and `.grok` means Grok Build;
`AGENTS.md` or `.agents` mean Codex too, but only when `.grok` is absent, since Grok Build reads them as well. When no
marker is found, or markers of more than one agent are (a directory with both `CLAUDE.md` and `AGENTS.md`, say), nothing
is stored and the project keeps following the console's default for project sessions. Nothing is stored either when the
detected agent is known not to be on the login shell's `PATH`. A default chosen by the user is never replaced by
detection, and this holds for every association source and for projects the console session associates, which always
detect when they give none.

## The project's account, model and reasoning effort

Besides its default agent, a project may carry **per agent** its own account and its own model and reasoning effort for
the sessions opened under it. All three are kept one set per agent, because an account belongs to one agent and model
names and effort levels are each agent's own, and a session reads only the set of its own agent.

- **The account** has three states: **inheriting the console's reference** for that agent, which is the state a new
  project is in; **the agent's default account** — pinning nothing, the state described in "Agent config directories" in
  `docs/product/consoles.md` — which is a choice of its own here, available even where the console names an account; or
  a **named account** of that agent. The last two take the place of the console's for sessions of that agent opened
  under the project afterwards. The order a session resolves its account in, and which sessions reach the project's
  setting at all, are in "Which account a session uses" in `docs/product/sessions.md`, and what an account is in "Agent
  config directories" in `docs/product/consoles.md`. Removing an account in Settings puts every project that named it
  back to inheriting its console's reference, and leaves a project on the agent's default account as it is.
- **The model and the reasoning effort**, each left unset, fall back separately to the console's project session
  value for that agent, else to the agent's own default; the console's console session values never apply. See
  `docs/product/model-and-reasoning-effort.md`.

The project dialog, both when associating and when editing a project, joins the account picker to the **Default agent**
field as a single field, the way the session dialog joins its Agent field to one, and holds the same collapsed
**Advanced** section. All of it belongs to the agent the Default agent field currently settles on — the agent picked,
or the agent "Same as console" names while the field is on it — so switching that field shows another agent's
set, and what was set for an agent no longer shown is kept and still saved. The account picker lists, in this order,
the inherited entry, "Same as console", with a badge naming the account the console resolves to for that agent (the
agent's default account when the console pins none); the agent's default account, named as it is everywhere
else; and then that agent's own accounts. It is offered only when that agent has an account of its own. A project whose
named account is no longer stored shows the inherited entry, and keeps what it has stored until another entry is
picked. A turned-off account — the default account or one of the agent's own (see "Turning an account off" in
`docs/product/agent-accounts.md`) — is left out of the picker, except the one the project named when the dialog opened
and the one picked in it now: as in the console dialog, since a project's account is a lasting preference, that one
stays listed and pickable, with a short note "Disabled" under its name and, as its description for assistive
technology, that sessions cannot open on it until it is turned back on. "Same as console" carries no such mark, its
badge naming the console's account as usual. A project that names a turned-off account keeps it; the sessions that
would run on it are refused until then.

While a new project on a **parent directory** is set to "Detect from files" neither the account picker nor the Advanced
section is shown, since no agent is settled — each repository found is detected on its own — and anything set in
them before the source changed is not stored.

A project the console session associates takes the accounts, models and efforts its call gives, and otherwise inherits
its console's account for every agent and leaves every model and effort unset. A parent-directory association gives
every project it creates the same values the form, or the call, was filled in with.

A changed account applies to sessions opened afterwards; a changed model or effort reaches a project session's next
launch only when that value is inherited (see `docs/product/model-and-reasoning-effort.md`). Sessions already running
are unaffected by any of the three.

## The project's remote URL

A project records the git remote URL of the repository its directory holds, for **every** source, not only for one
associated from a URL. It is Octoboard's own record of where that repository came from; the user types it only as the
input of a git association, it appears in no project form, and it is not among the fields a project's settings can
change.

**What is recorded.** The URL as the repository's own configuration writes it, before any rewrite the machine's git
configuration would apply to it, since such a rewrite is a setting of the machine and not of the repository. The
remotes are read across git's whole configuration — system, global, local and worktree — the same set `git remote`
and `git fetch` see, so a remote that only the user's global configuration defines is found here as well.

**Which remote.** `origin`'s URL; failing that, the URL of the remote the current branch's upstream belongs to;
failing that, the first remote the configuration lists (system before global before local before worktree).
`origin` comes first so that the recorded value says where the working tree came from and does not move as the user
switches branches. The last fallback has a
rare consequence: a repository with no `origin`, whose current branch has no upstream, and which has a remote of its
own while the user's global configuration also defines one, records the global one's URL.

**No URL is a normal state.** A directory that is not a git repository, and a repository with no remote configured,
have no recorded URL, and neither is an error — the project is associated and its git status checked the same way. A
read that cannot be made at all leaves whatever is recorded alone, and never refuses an association.

**When it is read.** Associating a project reads it from the directory — for a single directory, and for each
repository a parent directory yields. A git association instead keeps the URL the request was given to clone from, and
takes the configured one at the next read. Afterwards Octoboard maintains the value itself: every project's directory
is re-read once when the daemon starts, and each git status check of a project records what it reads (see "What one
check does" in `docs/product/project-git-status.md`). So a repository whose remote the user changes outside Octoboard
— `git remote set-url`, a renamed or moved repository — is followed, and a corrected value reaches every open window
at once, as any other change to a project does.

**When it is kept and when it is cleared.** A directory that cannot be seen — missing, on a volume that is not
mounted, or replaced by something that is not a directory — keeps the recorded URL, since none of that says the
repository stopped being one. It is cleared only when the directory is there and is no longer a git repository, or is
a repository with no remote left.

## Editing a project

Only the name, the description, the default agent, the account, the model and the reasoning effort, and the tags can be
changed, besides pinning and unpinning the project. The name is changed from the project's Rename or Project settings,
everything else from Project settings, which opens the project dialog under that title (see "Project rows" in
`docs/product/sidebar-project-rows.md`). The description field follows the name, and the tags field sits between the
description and the default agent: typing in it offers the tags already in use that the project lacks, in a list that
opens below the field and stays open after a pick so several can be added in a row, and Enter adds what was typed as a
tag, or the highlighted suggestion if there is one; leaving the field adds it too. The tags the project carries show
inside the field, ahead of the text input, and wrap onto more lines as they grow, the field growing with them. Each is a
picked tag (see "Picked tags" in `docs/product/project-tags.md`), and removing one, with its remove button or with
Backspace or Delete while it has focus, puts the focus in the text input. Backspace in the empty text input removes the
last tag, one per key press (holding it down does not keep removing). Clicking the field's empty area puts the focus in
the text input. The same field is in the dialog that associates a project, and the tags chosen there go on every project
that association creates. The association itself — the source and the directory — is fixed once the project exists; a
project that should point somewhere else is removed (see `docs/product/removing-projects.md`) and associated again. The
remote URL is not editable either, and saving an edit never overwrites it: Octoboard keeps that value in step with the
repository itself (see "The project's remote URL" above).

The name cannot be cleared: saving an empty name is rejected. The description can be emptied, which leaves the project
with none. New or changed text follows the limits in "Project names" and "The project's description" above, counted
after trimming. The name and description fields, including Rename, show the current count and limit from 90% of the
limit onward. Submitting overlong text shows an error at the field, keeps the input and its counter, and saves nothing;
text is never truncated.

An existing overlong name or description can be left unchanged while other settings are edited. Resubmitting the same
text after trimming is also accepted and preserves its stored text, including any surrounding whitespace; changing it
requires meeting the limit. Configuration import also preserves overlong values (see "What each project carries" in
`docs/product/configuration-export-and-import.md`).

The default agent can be cleared, which puts the project
back to following the console's default for project sessions; the account can be put on any of its three states,
inheriting the console's reference among them, and the model and the effort back to unset.

The console session can make the same changes to its console's projects (see "Changing a project: `update_project`" in
`docs/product/agent-settings-tools.md`).

## Browsing directories

The directory browser lists directories only — a project is a directory — and hides entries whose name begins with a
dot. Each entry is flagged with whether it is a git repository. Entries are sorted case-insensitively by name. The
listing always comes from the host the projects live on, never from the application's own filesystem view.

In the browser, the listed directory's subdirectories are one list, with a parent-directory entry at its top except at
the filesystem root — muted, marked with an up-arrow icon and labelled as the parent directory, which is also what it is
announced as to assistive technology — each directory marked with a folder icon, and a "git" badge at the end of each
entry that is a git repository. Clicking an entry, or pressing Enter on it, lists that directory. The list is a single
Tab stop: the arrow keys, Home and End move between its entries. A directory with no subdirectories shows an entry
saying so, in the compact empty-state row (see "The compact row" in `docs/product/empty-states.md`), which does nothing and
is skipped by the arrow keys. When the browser is opened on a path that does not exist (such as a default clone
directory not created yet), it lists the nearest ancestor that does exist instead (a `~/` path goes no higher than the
home directory), so there is always somewhere to navigate from; any other failure is reported as described next.

A listing can fail, and that is a normal path rather than an edge case: a packaged application is granted file access
per volume by macOS, and the user may decline the prompt or leave it unanswered. The failure is shown in the browser
itself, in an alert under the path field saying the directory could not be listed, with the reason — naming the path,
and the hint that per-volume access has to be granted — and **Try again**, which asks for the same directory again; it
opens no error alert. Whatever was listed before stays on screen so the browser can be navigated back out of, and the
alert goes once a listing succeeds. The same failure can surface when a session is opened: a project directory the daemon
cannot reach is reported as such rather than silently starting the agent somewhere else.

## Hosts

Every project and every session is bound to a host. Only one host exists, the local machine, listed as
"This machine". Hosts cannot be added or edited.
