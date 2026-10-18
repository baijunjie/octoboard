# Consoles and projects

A **console** groups a set of projects and owns a working directory of its own. A **project** is a directory on a
host, associated with one console. A console is created and deleted by the user; a project is associated either by
the user or by its console session. Most of a console's and its projects' settings can be changed by its console
session as well as by the user (see `docs/product/agent-settings-tools.md`). Sessions that run inside them are described in `docs/product/sessions.md`, and
carrying consoles and the projects under them to another machine in
`docs/product/configuration-export-and-import.md`.

## Consoles

A console carries:

| Field | Set by | Notes |
|---|---|---|
| Name | the user, required | Free text. |
| Default agent for console sessions | the user, required | The agent a console session runs when none is chosen for it. |
| Default agent for project sessions | the user, optional | The agent a project session runs when neither the session nor its project names one; unset, it is the default agent for console sessions. |
| Agent accounts | the user, optional | One per agent: the account that agent's sessions in this console run under, the agent's default account when none is picked; see "Agent config directories" below. |
| Console session model and reasoning effort | the user, optional | One pair of defaults per agent for the console's console sessions, the agent's own default when unset; session overrides and inheritance are in `docs/product/model-and-reasoning-effort.md`. |
| Project session model and reasoning effort | the user, optional | One pair per agent for the console's project sessions, each value taken by a project session whose project and own choice leave it unset; the agent's own default when unset. See `docs/product/model-and-reasoning-effort.md`. |
| Avatar | the user, optional | A custom image shown in place of the default avatar, in a circle; see "Avatar" below. |
| Console session instructions | the user, optional | The user's own instructions for the console's console sessions, added after Octoboard's built-in ones and taking precedence over them, short of what a console session does; see "Console session instructions" below. |
| Working directory | Octoboard | `~/.octoboard/consoles/<console id>/`, created when the console is created. Not settable and not changeable. |

A console has two default agents. The **default agent for console sessions** is what a console session runs unless an
agent is chosen for that session. The **default agent for project sessions** is what a project session runs when
neither the session nor its project names an agent (see "The project's default agent" below). It may be left unset —
"Same as console sessions" — and then it is whatever the default agent for console sessions is at the time, following
it when it changes. A new console starts unset, and so is a console stored before the setting existed. The full order
is in "Which agent a session uses" in `docs/product/sessions.md`.

The console dialog, at most 768 px wide, shows both in one **Default agent** group, as two fields, **Console sessions**
and **Project sessions**, under a note that a project with its own default agent uses that instead. The two fields sit
side by side in two equal columns, one under the other in a window narrower than 768 px; each field's name sits above
its picker, which spans the width of its column. Each field takes one of the three supported agents (Claude Code, Codex,
Grok Build). Console sessions starts on Claude Code in the creation form — or, once Claude Code is determined
unavailable, on the first of the others that is not, so it never starts on an agent that cannot be chosen. Project
sessions starts on its first entry, **Same as console sessions**, which names the agent the Console sessions field
currently holds, with that agent's icon dimmed, and follows that field as it changes; the agents themselves come after
it. Changing one field never moves the other. Which agents the two fields list is in "Agents that are not installed"
below.

Multiple consoles can exist side by side and are independent of each other.

### Agents that are not installed

An agent is **not installed** once Octoboard has determined it unavailable: its binary does not resolve on the user's
login shell `PATH`. Every agent picker — the console dialog's two Default agent fields, the project dialog's Default
agent field and the session dialog's Agent field — leaves such an agent out, with one exception: the value the picker
currently holds. An agent already stored or chosen that is not installed stays shown and selected, labelled as not
installed, and cannot be picked again; it drops out of the list as soon as the picker moves to another entry. Before
availability has been determined, every agent is listed and can be picked. The console dialog's Agent accounts table
follows the same rule (see "Agent config directories" below).

Settings' Add account form is the deliberate exception, and lists every agent (see "Agent accounts" in
`docs/product/settings.md`).

### Avatar

A console is drawn as a round avatar on the rail (see "The console switcher" in `docs/product/sidebar.md`). Without a
custom image it is a network glyph on a coloured fill, one of seven, picked from the console's name: the same name
always gets the same fill, two different names can still share one, and renaming a console can change its fill.
Spaces and invisible characters around the name, and whether accented letters were typed precomposed or combined, do
not change it. Each fill has its own light and dark value (see "What follows the choice" in
`docs/product/appearance.md`).

The console dialog's **Avatar** field is one round control the size of an avatar. It opens the dialog's first row,
beside the Name field and before it (mirrored under a right-to-left language), centred on the Name field's label and
input together; it keeps that place when an error appears under the name. It has no visible label: assistive
technology announces it as a group named "Avatar", and the console's name beside it says whose avatar it is.
While no image is picked it shows an upload glyph, so an optional field shows nothing that looks already filled in;
pressing it opens the file picker. Once an image is picked the control shows the image, and a delete button sits over
its corner: hidden at rest, shown while the mouse pointer is over the control or anything in it has keyboard focus,
and shown outright where the pointer is coarse, since a touch has no hover to reveal it with and tapping the image
opens the file picker instead. Pressing the delete button puts the control back to the upload glyph, from where another
image can be picked. The field does not show what the default avatar would look like: a console with no image is drawn
as described above wherever the console appears, and the dialog does not preview it.

A chosen image is cropped to its centred square and scaled to 128 x 128 pixels in the window before it is saved, so
any size or shape of picture is accepted; a file that is not a readable image is refused in the dialog. As with the
other fields, an avatar that was not touched is left as it is when the console is saved.

### Console session instructions

A console can carry instructions of the user's own for its console sessions — how to work, report or phrase things —
which Octoboard adds after its built-in instructions in the console session's instruction file. The user's text takes
precedence over the built-in instructions wherever the two disagree, with one exception: it cannot change what a console
session does and does not do, so a console session still never changes a project itself. Where the text goes and when a
console session reads it is in "The console session's instruction file" in `docs/product/launching-agents.md`.

The console dialog's **Console session instructions** field is a multi-line text field at the bottom of the dialog's
collapsed Advanced section, below and outside the frame around its agent tabs of model and reasoning effort pickers (see
"The Advanced section" in `docs/product/model-and-reasoning-effort.md`); Enter in it starts a new line rather than
saving the dialog. It shows only the user's own text, never the built-in instructions. Empty, its placeholder says there
are no additional instructions, and a note below it says the text comes after the built-in instructions and takes
precedence over them where the two conflict, and that it applies to new console sessions. The Advanced section's own
note, at its top, speaks only of the models and reasoning efforts.

The text is saved with surrounding whitespace trimmed, and a blank one means none; emptying the field of a console that
had some removes them. There is no length limit, but Codex reads only part of a very long one (see "The console
session's instruction file" in `docs/product/launching-agents.md`). As with the other fields, text that was not changed
is left as it is when the console is saved.

The console session itself can neither read nor change them (see "Who reads and changes what" in
`docs/product/agent-settings-tools.md`).

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

The console dialog sets them in its **Agent accounts** table, one row per agent, each headed by the agent's icon and
name, apart from the Default agent group: every agent's account is shown and set there, whichever agents the console's
sessions default to. A note under it says that every session of an agent in the console uses that agent's account
here, unless a project or the session picks another. The agents' names share one column, their controls starting in
line beside them; in a narrow window each name sits above its control.

- An agent with accounts of its own has a picker of them in its row, its default account first, at most 320 px wide
  rather than spanning the row.
- An agent with none says **Default account** as plain text, followed straight after by a **Manage accounts…** button,
  which wraps under the text where the row is too narrow for both. The button opens
  Settings on its Agent accounts section, above the console dialog, which stays open beneath with its unsaved edits.
  When Settings closes, keyboard focus returns to that agent's row — to its picker, if the agent has gained an account
  meanwhile.
- An agent that is not installed (see "Agents that are not installed" above) has no row, except while the console pins
  a named account for it: that row stays, its agent labelled as not installed, until it is moved to the agent's default
  account. Before availability has been determined every agent has a row.

No account is unpickable for its directory being missing.

- **The default account**, which a new console starts on, is the state of pinning nothing: that agent's sessions use
  whatever the user's login shell exports for the agent's variable, and the agent's own default when it exports none.
- **Any other account** applies its directory to every session of that agent opened in the console afterwards — the
  console session and project sessions alike, including a session whose agent was chosen for that session alone — and
  takes precedence over the value in the user's shell environment (see "The launch environment" in
  `docs/product/launching-agents.md`). A session reads only its own agent's account; sessions of the other agents are
  unaffected. A project can set its own account for an agent — another account, or the agent's default account against
  a console that names one — which its sessions take instead of the console's (see "The project's account, model and
  reasoning effort" below), and a session can also be opened under a different account of its agent than either (see
  "Opening a session" in `docs/product/sessions.md`).

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
  typed;
- it must not be a directory already used by another account of the same agent, or by that agent's default account —
  by the directory the default account currently resolves to, or, until that has been determined or if it cannot be,
  by the agent's usual default from the table above. Directories are compared after the steps above, without resolving
  symlinks; on macOS letter case is ignored, since its default file system is case-insensitive, while elsewhere it
  counts, and the directory is stored with the letter case it was given. Accounts of different agents may use the same
  directory, and leaving an account's directory as it is is never refused.

Existence is **not** checked: a directory that does not exist yet is accepted, and is created on first launch — by
Claude Code itself, and by Octoboard for Codex, which refuses a missing one (Grok Build excepted, see below). What is
stored, and shown for the account, is its expanded absolute path.

**A session keeps the account and the directory it last recorded until the user switches it.** A session takes its
account when it is opened, in the order given in "Which account a session uses" in `docs/product/sessions.md`, and
nothing but the user changes it afterwards: not an edit to the project's or the console's reference, not an edit to the
account's directory. Resuming or reopening it relaunches with that same directory, because the agent keeps the
conversation inside it. The user can move the session to another account of its agent from its action menu; that
copies the conversation into the target's directory and reads that account's configuration, while keeping explicit
model and reasoning effort choices (see `docs/product/switching-session-accounts.md`). A session opened on the default account
holds no directory and resumes under whatever the user's shell exports at that moment.
The Codex account's approval mode is looked up again for each launch; keeping a directory does not freeze that
choice (see "Agent accounts" in `docs/product/settings.md`).

If a session's directory no longer exists when it is launched or resumed, the launch is refused rather than
started — but only for a session that has a conversation on the agent's side to resume. A session with none yet — a
new one, or one opened and never typed into, unless it was opened to resume a conversation (see "Resuming a
conversation started outside Octoboard" in `docs/product/sessions.md`) — launches into the missing directory instead,
and the directory is created as above;
a Codex session whose directory Octoboard cannot create is refused on this same path. This narrower refusal holds for
all three agents, and the message names the agent and the directory and says to recreate it or to point the account at
a different one. Grok Build is refused on this same path whenever its pinned directory is not an initialized Grok
home — one Grok has actually been run against — whether or not there is a conversation to resume, since an empty
directory would keep neither its login nor its conversation; see "Per-agent specifics a user will notice" in
`docs/product/launching-agents.md`.

### Editing a console

The console's action menu item **Console settings** opens the console dialog under that title (see "The console
header" in `docs/product/sidebar.md`). The name, both default agents, every agent's account, every agent's console
session and project session model and reasoning effort, the console session instructions and the avatar can be
changed. The dialog shows the models and reasoning efforts of one agent at a time (see "The Advanced section" in
`docs/product/model-and-reasoning-effort.md`); those set for another agent are kept and still saved. The default agent for project sessions can be put back on Same as console sessions, an account back
on the agent's default account, and the model and the effort back to unset, the agent's own default. Changing a default
agent or an account affects sessions opened afterwards; a session that already exists keeps the agent and the account
and directory it was started with. A change to the default agent for console sessions therefore changes the initial choice in the next
console-session dialog, and a change to the one for project sessions — or to the one for console sessions while
project sessions follow it — the initial choice in the next session dialog of a project with no default agent of its
own, while existing sessions keep the agent they were started with (see "Console sessions and project sessions" in
`docs/product/sessions.md`); a changed model or effort reaches a console session's next launch, or for the project
session pair a project session's, only when that value is inherited (see `docs/product/model-and-reasoning-effort.md`).
The name cannot be cleared: saving an empty name is rejected.

The console session can change the same settings of its own console except the avatar and the console session
instructions (see "The console's own settings: `get_console` and `update_console`" in
`docs/product/agent-settings-tools.md`).

An account, a model and an effort are saved only when they are changed: saving a console with a picker untouched
succeeds regardless of what has happened to the account it refers to since. Removing an account in Settings puts every
console that referred to it back on the agent's default account, and every project that named it back to inheriting
its console's reference.

### Deleting a console

A locked session anywhere in the console, archived sessions included, refuses the whole deletion under
"Locking a session" in `docs/product/sessions.md`.

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

Deleting the **last** console leaves the window with none, which it shows as the welcome screen (see
`docs/product/welcome-screen.md`); the same holds when another client deletes it.

## Projects

A project carries a name, the directory it points at, an optional default agent, how it was associated, the remote
URL of the repository it points at (see "The project's remote URL" below), whether the user has pinned it (see "Order
of projects and sessions" in `docs/product/sidebar.md`), any number of tags, and — one set per agent — what it says
about the account its sessions run under and an optional model and reasoning effort for them (see "The project's
account, model and reasoning effort" below). Every project is bound to a host (see "Hosts" below).

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

### Picked tags

A tag the user has picked and can remove, in a project's Tags field, looks and behaves as follows. The sidebar's
Projects heading shows the tags of a filter in force in the same colours, but there they are not removable and the
rules below do not apply (see "Filtering the project list" in `docs/product/sidebar.md`).

- It is filled with a light accent tint and has accent-coloured text.
- Its label is always one line. A label too long for its space fades out at its cut end and, while the mouse pointer is
  over the tag, runs as a marquee through its whole text (see "Names too long for their space" in
  `docs/product/labels-and-tooltips.md`); its full text is then also the tag's tooltip.
- Its remove button is hidden at rest. It shows while the mouse pointer is over the tag or the tag has keyboard focus,
  laid over the tag's end, the label fading out ahead of it. A label whose end the button then covers counts as cut,
  and its marquee scrolls until that end is clear of the button; a label that ends well short of the button does not
  move.
- At rest, the tag fits its content with no minimum width. While the pointer is over it or it has keyboard focus,
  it is at least 3rem wide so the remove button and its fade leave part of a short label visible.
- The tag takes the direction of its own text, decided by its first letter, whatever the UI language: a tag whose
  first letter is Hebrew, Arabic, Syriac, Thaana or N'Ko runs right to left, any other left to right, and a tag with no
  letter follows the UI language's direction. The remove button and the fade sit at the end the text runs to, so a
  Latin tag under the Arabic UI has its button on the right, and an Arabic tag under the English UI on the left.
- With keyboard focus on it, Backspace or Delete removes it.

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
rules in this section, and a stricter one for a repository URL it clones (see "Adding a project with its settings" in
`docs/product/agent-settings-tools.md`).

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

A name can be given only when a single project is being associated; it then overrides the derived name. A blank name
counts as none given. Otherwise, and for every project a parent-directory association produces, the name is the
directory's own name.

### The project's default agent

A project may carry its own default agent, or leave it unset to follow its console's default agent for project
sessions (see "Consoles" above). The selection order for a new session is in "Which agent a session uses" in
`docs/product/sessions.md`.

The project dialog, both when associating and when editing a project, offers "unset" as an "Inherit from console"
choice listed first and preselected for a new project (a parent directory preselects "Detect from files" instead); it
names the agent the console's default for project sessions currently resolves to — the console's default for console
sessions while the one for project sessions is unset — shown with that agent's icon dimmed, or is a bare "Inherit from
console" when the console is not known. Choosing a specific agent stores it as the project's own default. The agents
listed follow "Agents that are not installed" above.

Associating a project with no default agent given looks at the directory's top level once and stores the agent it is
set up for as the project's own default, shown and editable in Project settings like any chosen one. For a single
directory or a git URL the form has already done this before submitting and shows the result preselected in the field
(a choice made by hand stays only until the path or URL changes, which reads it again); "Inherit from console" chosen
after that means none, and the project follows the console's default for project sessions. For a parent directory
there is no pre-read, and the field offers one more choice, "Detect from files", listed first and selected on arriving
at that source (and given up when the source changes): each repository found is detected on its own at association,
and one that cannot be decided follows the console's default for project sessions. "Inherit from console" keeps its
meaning of following that default with no detection, and choosing an agent applies it to every repository found. The form explains the detect choice under the
field. The same rules apply to what is read: `CLAUDE.md` or `.claude` means Claude Code, `.codex` means Codex, and
`.grok` means Grok Build; `AGENTS.md` or `.agents` mean Codex too, but only when `.grok` is absent, since Grok Build
reads them as well. When no marker is found, or markers of more than one agent are (a directory with both `CLAUDE.md`
and `AGENTS.md`, say), nothing is stored and the project keeps following the console's default for project sessions.
Nothing is stored either when the detected agent is known not to be on the login shell's `PATH`. A default chosen by the user is never
replaced by detection, and this holds for every association source and for projects the console session associates,
which always detect when they give none.

### The project's account, model and reasoning effort

Besides its default agent, a project may carry **per agent** its own account and its own model and reasoning effort for
the sessions opened under it. All three are kept one set per agent, because an account belongs to one agent and model
names and effort levels are each agent's own, and a session reads only the set of its own agent.

- **The account** has three states: **inheriting the console's reference** for that agent, which is the state a new
  project is in; **the agent's default account** — pinning nothing, the state described in "Agent config directories"
  above — which is a choice of its own here, available even where the console names an account; or a **named account**
  of that agent. The last two take the place of the console's for sessions of that agent opened under the project
  afterwards. The order a session resolves its account in, and which sessions reach the project's setting at all, are
  in "Which account a session uses" in `docs/product/sessions.md`, and what an account is in "Agent config
  directories" above.
  Removing an account in Settings puts every project that named it back to inheriting its console's reference, and
  leaves a project on the agent's default account as it is.
- **The model and the reasoning effort**, each left unset, fall back separately to the console's project session
  value for that agent, else to the agent's own default; the console's console session values never apply. See
  `docs/product/model-and-reasoning-effort.md`.

The project dialog, both when associating and when editing a project, joins the account picker to the **Default agent**
field as a single field, the way the session dialog joins its Agent field to one, and holds the same collapsed
**Advanced** section. All of it belongs to the agent the Default agent field currently settles on — the agent picked,
or the agent "Inherit from console" names while the field is on it — so switching that field shows another agent's
set, and what was set for an agent no longer shown is kept and still saved. The account picker lists, in this order,
the inherited entry, named after the account the console resolves to for that agent (the agent's default account when
the console pins none) and marked as coming from the console; the agent's default account, named as it is everywhere
else; and then that agent's own accounts. It is offered only when that agent has an account of its own. A project whose
named account is no longer stored shows the inherited entry, and keeps what it has stored until another entry is
picked.

While a new project on a **parent directory** is set to "Detect from files" neither the account picker nor the Advanced
section is shown, since no agent is settled — each repository found is detected on its own — and anything set in
them before the source changed is not stored.

A project the console session associates takes the accounts, models and efforts its call gives, and otherwise inherits
its console's account for every agent and leaves every model and effort unset. A parent-directory association gives
every project it creates the same values the form, or the call, was filled in with.

A changed account applies to sessions opened afterwards; a changed model or effort reaches a project session's next
launch only when that value is inherited (see `docs/product/model-and-reasoning-effort.md`). Sessions already running
are unaffected by any of the three.

### The project's remote URL

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

### Editing a project

Only the name, the default agent, the account, the model and the reasoning effort, and the tags can be changed,
besides pinning and unpinning the project. The name is changed from the project's Rename or Project settings,
everything else from Project settings, which opens the project dialog under that title (see "Project rows" in
`docs/product/sidebar.md`). The tags field sits between
the name and the default agent: typing in it offers the tags already in use that the project lacks, in a list that
opens below the field and stays open after a pick
so several can be added in a row, and Enter adds what was typed as a tag, or the highlighted suggestion if there is one;
leaving the field adds it too. The tags the project carries show inside the field, ahead of the text input, and wrap
onto more lines as they grow, the field growing with them. Each is a picked tag (see "Picked tags" above), and
removing one, with its remove button or with Backspace or Delete while it has focus, puts the focus in the text input.
Backspace in the empty text input removes the last tag, one per key press (holding it down does not keep removing).
Clicking the field's empty area puts the focus in the text input. The same field is in the dialog that associates a
project, and the tags chosen there go on every project that association creates. The association itself — the source
and the directory — is fixed once the project exists; a project that should point somewhere else is removed and
associated again. The remote URL is not editable either, and saving an edit never overwrites it: Octoboard keeps that
value in step with the repository itself (see "The project's remote URL" above).

The name cannot be cleared: saving an empty name is rejected. The default agent can be cleared, which puts the project
back to following the console's default for project sessions; the account can be put on any of its three states,
inheriting the console's reference among them, and the model and the effort back to unset.

The console session can make the same changes to its console's projects (see "Changing a project: `update_project`" in
`docs/product/agent-settings-tools.md`).

### Removing a project

A locked session of the project, archived sessions included, refuses removal before any process is ended or
folder is moved, under "Locking a session" in `docs/product/sessions.md`.

Removing a project removes the association, and with it the records of that project's sessions, archived ones
included. It also clears what Octoboard recorded about which session is working on which branch in the project's
repository, unless another project still points into that repository (see "What the record is" in
`docs/product/branch-responsible-session.md`). **The directory and everything in it are left as they are**, and so are
the repository's branches and worktrees, unless the user chooses to move the folder to the Trash as well (see "Moving
the project folder to the Trash" below).

Removing a project asks for confirmation. **Sessions of the project that are still running do not block it**:

- With none running, the confirmation offers **Remove**.
- With some running, the confirmation lists them — each with its status glyph, its agent's icon and its title, in the
  sidebar's order (see "Order of projects and sessions" in `docs/product/sidebar.md`) — in its one warning, below its
  options and above its buttons, under a line saying that that many sessions are still running and will be ended, and
  its button reads **End and remove**. The list follows
  sessions starting or stopping while the confirmation is open. Confirming ends each running session's process the way
  archiving does (see "Archiving, interruption and resuming" in `docs/product/sessions.md`), then removes the project
  with all its sessions and the last output Octoboard kept for each. Sessions bound to another session of the project
  are ended before the session they are bound to, the deepest level first, so ending that one is never refused over
  them. A lead session's console session belongs to no project and is left as it is; the lead session's binding to it
  goes with the lead session's record (see "Lead sessions" in `docs/product/hub-orchestration.md`).

The confirmation also carries an option to **also move the project folder to the Trash**, unticked each time the
confirmation opens, with a hint that the whole folder goes, including anything not committed to Git, and that it can
be recovered from the Trash. Ticking it makes the message say the folder is moved to the Trash and turns the button
into **Remove and move to Trash**, or **End, remove and move to Trash** when sessions are running. Ticking it also lists
the worktrees outside the folder, and the branches, that go with it, and offers locked worktrees under a choice of
their own (see "With a project's folder moved to the Trash" in `docs/product/branch-and-worktree-cleanup.md`). While a removal with
the option ticked runs, the button says the folder is being moved to the Trash; while any removal runs, the option
cannot be changed.

Removal is refused while a session of the project is being launched or resumed at that moment, since that session has
no process yet that could be ended, and, with the Trash option ticked, for the folder reasons below. A refusal is shown
in the confirmation and names the reason; nothing is ended, moved or removed.

#### Moving the project folder to the Trash

- The folder is **moved to the system Trash, never deleted outright**, so it can be recovered from there. On macOS,
  Finder's Put Back may not be offered for it; the folder is still in the Trash to be taken back out.
- Running sessions are ended first, then the folder is moved, then the project is removed. **If the move fails, the
  removal fails** with the reason, and the project is kept with its records so the removal can be retried; sessions
  already ended for it stay archived.
- A session started in the project while the folder is being moved is ended as well, and goes with the project.
- Once the project is removed and its sessions' processes have ended, linked worktrees outside the folder that would
  be left without it, and where the repository outlives the folder the branches of the project's sessions, go too
  (which ones depends on whether the repository goes with the folder); a failure there
  does not undo the removal and is reported (see "With a project's folder moved to the Trash" in
  `docs/product/branch-and-worktree-cleanup.md`).
- A folder that no longer exists is skipped, and the project is removed as usual.
- A project directory that is a symbolic link is moved to the Trash as the link, not as the folder it points to.
- It is refused, before any session is ended and with the project kept, for:
  - the home directory, the root of a disk (`/` or a mount point, such as a mounted volume), or a folder that holds
    Octoboard's own data;
  - a folder shared with another project: another project's directory is the same folder, lies inside it or holds
    it, or another project's Git repository lies inside it, as a linked worktree's does when its main repository is
    this folder. The refusal names that other project, which has to be removed first. Symbolic links are resolved for
    this comparison, so two spellings of one folder count as the same folder.

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
