# Consoles

A **console** groups a set of projects and owns a working directory of its own. A console is created and deleted by
the user. Most of a console's settings can be changed by its console session as well as by the user (see
`docs/product/agent-settings-tools.md`). The projects under a console are described in `docs/product/projects.md`,
sessions that run inside them in `docs/product/sessions.md`, and carrying consoles and the projects under them to
another machine in `docs/product/configuration-export-and-import.md`.

A console carries:

| Field | Set by | Notes |
|---|---|---|
| Name | the user, required | Free text. |
| Default agent for console sessions | the user, required | The agent a console session runs when none is chosen for it. |
| Default agent for project sessions | the user, optional | The agent a project session runs when neither the session nor its project names one; unset, it is the default agent for console sessions. |
| Agent default accounts | the user, optional | One per agent: the account that agent's sessions in this console run under, the agent's default account when none is picked; see "Agent config directories" below. |
| Console session model and reasoning effort | the user, optional | One pair of defaults per agent for the console's console sessions, the agent's own default when unset; session overrides and inheritance are in `docs/product/model-and-reasoning-effort.md`. |
| Project session model and reasoning effort | the user, optional | One pair per agent for the console's project sessions, each value taken by a project session whose project and own choice leave it unset; the agent's own default when unset. See `docs/product/model-and-reasoning-effort.md`. |
| Avatar | the user, optional | A custom image shown in place of the default avatar, in a circle; see "Avatar" below. |
| Console session instructions | the user, optional | The user's own instructions for the console's console sessions, added after Octoboard's built-in ones and taking precedence over them, short of what a console session does; see "Console session instructions" below. |
| Working directory | Octoboard | `~/.octoboard/consoles/<console id>/`, created when the console is created. Not settable and not changeable. |

A console has two default agents. The **default agent for console sessions** is what a console session runs unless an
agent is chosen for that session. The **default agent for project sessions** is what a project session runs when neither
the session nor its project names an agent (see "The project's default agent" in `docs/product/projects.md`). It may be
left unset — "Same as console sessions" — and then it is whatever the default agent for console sessions is at the time,
following it when it changes. A new console starts unset, and so is a console stored before the setting existed. The
full order is in "Which agent a session uses" in `docs/product/sessions.md`.

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

## Agents that are not installed

An agent is **not installed** once Octoboard has determined it unavailable: its binary does not resolve on the user's
login shell `PATH`. Every agent picker — the console dialog's two Default agent fields, the project dialog's Default
agent field and the session dialog's Agent field — leaves such an agent out, with one exception: the value the picker
currently holds. An agent already stored or chosen that is not installed stays shown and selected, labelled as not
installed, and cannot be picked again; it drops out of the list as soon as the picker moves to another entry. Before
availability has been determined, every agent is listed and can be picked. The console dialog's Agent default accounts
section follows the same rule (see "Agent config directories" below).

Settings' Add account form is the deliberate exception, and lists every added agent, installed or not (see "Adding,
editing and removing an account" in `docs/product/agent-accounts.md`).

## Avatar

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

## Console session instructions

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
precedence over them where the two conflict, and that it applies to new console sessions.

The text is saved with surrounding whitespace trimmed, and a blank one means none; emptying the field of a console that
had some removes them. There is no length limit, but Codex reads only part of a very long one (see "The console
session's instruction file" in `docs/product/launching-agents.md`). As with the other fields, text that was not changed
is left as it is when the console is saved.

The console session itself can neither read nor change them (see "Who reads and changes what" in
`docs/product/agent-settings-tools.md`).

## Agent config directories

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

The console dialog sets them in its **Agent default accounts** section, one row per agent, each headed by the agent's
icon and name, apart from the Default agent group: every agent's account is shown and set there, whichever agents the
console's sessions default to. A note under the section's title says that every session of an agent in the console
uses that agent's account here, unless a project or the session picks another. The agents' names share one column,
their controls starting in line beside them; in a narrow window each name sits above its control.

- Every agent's row has a picker of its accounts, the default account first, at most 320 px wide rather than spanning
  the row. An agent with no account of its own still has the picker, with the default account as its one option:
  under the name the user gave the default account (see "The default account" in `docs/product/agent-accounts.md`),
  else as "Default account".
- Beside the section's title, where a "?" help tip would sit, an icon-only **Manage accounts** button (a key) opens
  Settings on its Agent accounts section, above the console dialog, which stays open beneath with its unsaved edits.
  Its tooltip and the name announced for it are both "Manage accounts". When Settings closes, keyboard focus returns
  to the button.
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
  unaffected. A project can set its own account for an agent — another account, or the agent's default account against a
  console that names one — which its sessions take instead of the console's (see "The project's account, model and
  reasoning effort" in `docs/product/projects.md`), and a session can also be opened under a different account of its
  agent than either (see "Opening a session" in `docs/product/sessions.md`).

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

**A session keeps the account and the directory it last recorded until it is switched to another account.** A session
takes its account when it is opened, in the order given in "Which account a session uses" in `docs/product/sessions.md`,
and nothing but a switch of its account changes it afterwards: not an edit to the project's or the console's reference,
not an edit to the account's directory. Resuming or reopening it relaunches with that same directory, because the agent
keeps the conversation inside it. The user can move the session to another account of its agent from its action menu,
and an owner can move a session of its own with the `switch_session_account` tool; that copies the conversation into the
target's directory and reads that account's configuration, while keeping explicit model and reasoning effort choices
(see `docs/product/switching-session-accounts.md`). A session opened on the default account holds no directory and
resumes under whatever the user's shell exports at that moment. The Codex or Claude Code account's approval mode is
looked up again for each launch; keeping a directory does not freeze that choice (see "Approval modes" in
`docs/product/agent-accounts.md`).

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

## Editing a console

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

## Deleting a console

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
