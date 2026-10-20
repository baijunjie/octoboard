# Sessions

A **session** is one agent CLI process, running in a console's or a project's working directory and rendered in a
terminal the user types into directly. Octoboard is not an agent and not a terminal of its own: it starts the agent's
own binary and shows it as it is. What it adds to each launch, and what it guarantees it does not change, is in
`docs/product/launching-agents.md`.

## Console sessions and project sessions

| | Working directory | Where it appears |
|---|---|---|
| Console session | the console's working directory | the console sessions section |
| Project session | the project's directory | under that project |

**A console may hold any number of console sessions running at once.** Opening, resuming or reopening
one never checks what else is running in the console, and the sidebar lists every one of them that is
not archived, each as its own row, in the console sessions section above the project list (see "The
console sessions section and the project list" in `docs/product/sidebar.md`).

A console session's row follows a session row exactly — the same status glyph, its agent's icon, its title, a pin button
when pinned, and selecting it resumes it when it is interrupted, as selecting an interrupted session's row does. Its
action menu offers **Lock**/**Unlock**, **Pin**/**Unpin**, **Rename**, **Copy session ID** (see "Session rows" in
`docs/product/sidebar.md`), **Colour** (see "The binding badge" in `docs/product/sidebar.md`), **Focus mode** (see
`docs/product/focus-mode.md`), **Switch account**, **Hand off** and **Archive** — **Delete** in its place while the
console session has never been used (see "Deleting an unused session" in `docs/product/deleting-sessions.md`); there is
no Resume item, for the same reason. A console session's title is renameable, like a project session's; its default is
under **Name** in "Opening a session" below.

The user archives a console session from its own row's menu, as any session is archived; the console session can also
archive itself once every session under it is archived (see "A session archiving itself" in
`docs/product/hub-orchestration.md`). Archiving it moves it among the console's archived console sessions; these are not
reached from any one row, but from the console's own menu, which offers **Archived console sessions** — a submenu of the
console's newest archived console sessions, each of which can be selected, and the way to the archive view of all of
them (see "The console header" in `docs/product/sidebar.md`). The console header's **New console session** button opens
the session dialog described in "Opening a session" below — whereas resuming or reopening a console session keeps its
recorded agent and account; only switching accounts changes its account (see "Agent config directories" in
`docs/product/consoles.md` and `docs/product/switching-session-accounts.md`).

A console session carries no project; it runs in the console's working directory. It is given
Octoboard's orchestration tools and dispatches work to sessions in the console's projects — see
`docs/product/hub-orchestration.md`. Because it belongs to no project, archived console sessions
belong to the console rather than to any project.

A project session bound to a console session carries that console session's **binding badge**
wherever it is listed, and the console session's own row shows the same colour (see "The binding
badge" in `docs/product/sidebar.md`).

An **unbound** project session — one the user opened by hand without choosing a console session under **Report to**
(see "Opening a session" below), or one started on another session's request that the user approved — is given a
narrower set of the orchestration tools, scoped to its own project: it can start sessions there, which are bound to it
and report to it, and drive them (see "The unbound project session's tools" in `docs/product/hub-orchestration.md`). A
session it starts carries no binding badge and cannot start sessions of its own. A session's **owner** is the session
it is bound to: a console session or a project session (an unbound session's own owner is the user). An owner can hand
a session of its own to another owner, and an unbound project session can bind itself to one; bound to a console
session while keeping the sessions it started, it is a **lead session**. Neither asks the user, and orchestration stays
at most two levels deep (see "Lead sessions" in `docs/product/hub-orchestration.md` and
`docs/product/changing-session-owners.md`).
Every session, of whatever kind, can also list the other running sessions, in every console and project, and share
information with any of them; it takes work only from its owner, and what comes from another session is information to
weigh (see `docs/product/information-between-sessions.md`). Every session can also ask the user
to start a session that is nobody's, in any project or a console session, which reports to the user (see
`docs/product/session-requests.md`).

Selecting a session also gives the window's right pane, beside the terminal, to the session's own owner: a console
session, whose **report panel** it holds (see `docs/product/report-panel.md`), or a project session's project, whose
**Files panel** it holds (see `docs/product/project-pane.md`). When the right pane shows something else is in "What the right pane shows" in
`docs/product/window-layout.md`.

## Where sessions are listed

Sessions are listed in the sidebar, one console at a time: its console sessions in the console
sessions section, a project session under its project. Archived sessions are not listed among the
others; they are reached from their project's action menu or, for a console session, the console header's
action menu, and from the archive view. Everything about the sidebar — its rows and menus, the order sessions
are listed in, pinning, the archive view and how selecting a session works — is in
`docs/product/sidebar.md`; focus mode is in `docs/product/focus-mode.md`.

## Opening a session

New console session from the console header, the console sessions section's empty row, and the terminal's offer of
the same action open a dialog before anything is launched. New session under a project opens the same dialog for that
project. The sidebar's **+** buttons can also open a session without the dialog, on the values the dialog would open
on; see "Opening a session with defaults" below.

A project session's dialog opens with two tabs at its head, **New conversation**, selected by default, and **Resume
conversation**, each holding the whole form: the **Resume conversation** tab starts with a required **Session ID**
field and has no **Report to** field (see "Resuming a conversation started outside Octoboard" below). What is typed or
picked is kept across a switch between the tabs, while the Advanced section folds back closed. The dialog for a console
session has no tabs, and neither has the dialog opened from a console session's focus mode, whose session always starts
a new conversation.

Both dialogs offer these fields in this order:

- **Name** (optional) — the empty field previews the default name: the project's name for a project session, or the
  console's name for a console session, followed by the session label in the UI's current language. Leaving it empty
  uses that default when the session is created, with no numeric suffix; multiple sessions can have the same name.
  The field stays empty until the user enters a custom name. The dialog removes surrounding whitespace from an
  entered name and uses the default when nothing remains; a custom name gets no session label added. Renaming the
  parent or changing the UI's language affects future defaults; existing sessions keep their created names. A task
  title can still appear before either kind of created name, as described in "Displayed names" in
  `docs/product/session-titles.md`.
- **Agent** — an agent picker joined to an account picker, as in the project dialog. The agent picker lists the
  agents that can be picked, in the order of the added agents, with their icons and names, and otherwise only the one
  it holds (see "Which agents the agent pickers list" in `docs/product/consoles.md`). The field opens on the default
  resolved in "Which agent a session uses" below when that agent can be picked, otherwise on the first agent that can,
  whenever there is one.
  The account picker offers only that agent's accounts, with its default account first, and is shown even when the
  default account is the agent's only one, offering that account alone. It opens on the account resolved from the
  project and console for that agent (see "Which account a session uses" below); a console session starts from its
  console's account for the selected agent.
  There is no inheritance entry: both pickers settle concrete choices for this session.
  Each account in the picker carries a compact usage bar (see "In an account picker" in
  `docs/product/account-quotas.md`). A used-up account (see "A used-up account" in `docs/product/account-quotas.md`)
  is listed but cannot be picked, with a short note under its name saying it is used up and when it resets. When the
  account the field would open on, or would fall to when the user turns to another agent without having picked one
  of its accounts, is used up, the field takes the first account of the same agent that is not, in the picker's order.
  When the chosen account is used up all the same — every account of the agent is, the agent's only account among
  them, or a reading arrives while the dialog is open — a danger callout at the top of the
  dialog's body names the account, the quota it has used up and when it resets, and the dialog cannot be submitted
  until another agent or account is chosen or the account is no longer used up.
  Switching agents retains each agent's account choice separately. If a remembered account is removed while the dialog
  is open, that agent stays selected and its account falls back to its project's and console's current resolution,
  or its console's alone for a console session; with its last named account removed, the picker stays, offering the
  default account. Switching agents returns focus to the Agent picker.
- **Report to** (project sessions only; a choice, "No console session" by default) — one of the console's console
  sessions that are neither archived nor still *starting*, the most recently started first, each shown beside its colour, the same one the session's binding badge
  will carry (see "The binding badge" in `docs/product/sidebar.md`), or none. A chosen console session receives this
  session's reports, instead of the session staying outside the orchestration. A console with no such console session
  shows no choice at all, and the session is unbound. A session still starting is not offered because its launch may
  yet fail and remove it; `open_session` refuses one named as the owner (`unknown_session`). A binding is never undone;
  it changes afterwards only when the owner hands the session to another, or when an unbound session binds itself to
  one. A session the console session itself starts is always bound to it, and one a project session starts is bound
  to that project session; one started on another session's request is bound to nobody; see "Which sessions an owner
  drives" in
  `docs/product/hub-orchestration.md`. The choice is offered from the project list and the terminal's **New session** offer outside focus
  mode. Opened from a project's focus mode the dialog has no such field and the session is always unbound; opened from a console session's focus mode it
  has none either, and shows a line saying the session reports to that console session instead, which is the binding
  it gets (see `docs/product/focus-mode.md`). Should that console session be no longer available — archived or gone —
  the dialog says so in an error callout at the top of its body and cannot be submitted. The **Resume conversation** tab has no such field, since a resumed
  session is never bound (see "Resuming a conversation started outside Octoboard" below).

After these fields, the collapsed **Advanced** section offers the model and reasoning effort for this session. It
initially shows what the session would inherit for the selected agent (see "Which model and reasoning effort a session
uses" below); untouched values keep following it, and changes apply only to this session, including when resumed.
The choices, clearing and account-specific lists are described in "The Advanced section" in `docs/product/model-and-reasoning-effort.md`.

A console session has no project or Report to field and is never bound to another session. Opening it with the daemon
protocol's `open_session` accepts an explicit agent and account as the dialog does. Its `title` is stored trimmed, and
one that is absent, `null` or blank uses the parent's current name alone: the console's name for a console session,
or the project's name for a project session. The daemon has no UI language; the UI supplies its localized default
explicitly when no custom name was entered.

**Every new session is listed at once as *starting*,** before its agent has been launched, and the launch goes on
behind it (see "The launch environment" in `docs/product/launching-agents.md`). The dialog does not wait for the
launch: it closes as soon as the session is recorded, and the session's terminal says it is starting until the agent
is up (see "Loading" in `docs/product/session-terminal.md`). A refusal that can be known before anything is recorded —
an unknown owner, an unavailable agent or one that is not added, a used-up account, a model the agent does not offer, the refusals about a conversation to resume
below — is still shown in the dialog, which stays open. A session an agent opens with `new_session`, and the session
an approved request for one starts (see "On approval" in `docs/product/session-requests.md`), are listed as *starting*
the same way, but the agent's call, or the approval, is answered only once the launch has settled, with the launch's
own refusal when it fails.

The dialog takes no task: once up, a session the user opens by hand is in *awaiting instructions*, sitting at the
agent's prompt, and is given its work by typing into its terminal. Only a session started with `new_session` — by a
console session or a project session — or on another session's request that the user approved is handed an opening
prompt, the brief it is started with (see "Handing out a task: the brief" in `docs/product/hub-orchestration.md` and
"On approval" in `docs/product/session-requests.md`); once up, it is in *working*.

A session in a folder its agent has not been told to trust can first stop on that agent's own trust confirmation,
before it takes up its task or reaches its prompt. A resumed session can stop on it as well; since a session left
interrupted is relaunched only when it is selected or resumed, that is when its confirmation appears. Octoboard presses
that confirmation for the user once they have given permission — for that project, or for a folder its directory lies
under, whichever agent asked — or asks them first; see `docs/product/folder-trust.md`.

If the launch itself fails — the directory cannot be reached, the agent binary is not on the user's `PATH`, the user's
shell environment could not be captured (see "The launch environment" in `docs/product/launching-agents.md`), or, for
Grok Build only, the account the session would hold pins a directory that is not an initialized Grok home (see "Agent
config directories" in `docs/product/consoles.md`) — the *starting* session is removed again and no session is left
behind. For a session opened from the dialog, or with defaults (see "Opening a session with defaults" below), the
failure is reported in an error alert that names the session and gives the reason, worded from the failure's own code in
the UI's language; for a shell environment capture that ran out of time, the title names the session alone and the
explanation is laid out beneath it — the command that did not finish and its time limit, the likely cause, and what to
fix. Should the session's record itself fail to be removed, it is left *interrupted* instead, and the error alert is
shown all the same. For `new_session` and an approved session request the failure is reported through that call instead
(see "When an approval cannot be carried out" in `docs/product/session-requests.md` for the latter). A missing config
directory does not fail a brand new session for the other two agents: it has no conversation to lose, so Claude Code
creates the directory itself and Octoboard creates it for Codex.

**When no agent is both added and installed, the window offers no way to open a session, before any of that** (an
agent counts as installed until it has been determined unavailable; see "Which agents the agent pickers list" in
`docs/product/consoles.md`). Every way into the session dialog is disabled: the console header's **+**, each project
row's **+**, the **+** of either focus mode's header and of each project in a console session's focus mode; the
project's empty row saying there are no sessions yet is plain text rather than a button, in the project list and in
a project's focus mode alike; and no **+** offers to open a session with defaults (see "Opening a session with
defaults" below). Where the window says why:

- when some supported agent is installed, or not yet determined unavailable, but none of those is added, the
  terminal's empty panel, where it would otherwise offer a new session, says that no agent is added and offers **Add
  agent…** (see "A console with nothing in it" in `docs/product/session-terminal.md`), and the console sessions
  section, with room for less, says only that no agent is added;
- when every agent has been determined unavailable, the terminal's empty panel, where it would otherwise offer a new
  session, says that no agent was found and how to make one available, and the console sessions section says only
  that no agent was found on the user's `PATH` (see "The console sessions section and the project list" in
  `docs/product/sidebar.md`).

If this state comes about while the session dialog is open — the last such agent removed or found unavailable
meanwhile — the dialog closes at once, with nothing opened, and an error alert says that no agent is available to open
a session; that alert offers nothing but its acknowledgement (see "Recovery and unexpected errors" in
`docs/product/error-alerts.md`).

**The daemon refuses to open a session of an agent that is not added.** Whatever opens it — the dialog, a session
opened with defaults, `open_session`, a console or project session's `new_session` (with
`resume_agent_session_id` too), or an approved session request — a new session whose agent, once resolved (see "Which
agent a session uses" below), is not on the list of added agents is refused with `agent_not_added`, and nothing is
recorded. The refusal names the agent and says it is not added, pointing to Settings → Agent accounts; in the window it
is shown in an error alert offering **Add agent…** (see "Recovery and unexpected errors" in
`docs/product/error-alerts.md`). A session request is refused that way before the user is asked, and again on
approval should the agent have been removed meanwhile (see `docs/product/session-requests.md`). Handing a session off
to an agent that is not added is refused the same way (see "What can be handed off" in
`docs/product/handing-off-sessions.md`). While the list is undecided (see "Added agents" in
`docs/product/agent-accounts.md`) nothing is refused for it; a list the user has emptied refuses every agent. An agent
that has been determined unavailable is refused as unavailable (`agent_not_available`) rather than as not added. Only
a session being opened is checked: a session of an agent removed since keeps running, and resuming, reopening or
restarting it is never refused for this.

**The daemon refuses to open a session on a used-up account.** Whatever opens it — the dialog, a session opened with
defaults, `open_session`, a console or project session's `new_session` (with `resume_agent_session_id` too), a
handoff's receiving session, or an approved session request — a new session whose account, once resolved (see "Which
account a session uses" below), is used up by the reading the daemon holds at that moment (see "A used-up account" in
`docs/product/account-quotas.md`) is refused with `account_quota_exhausted`, and nothing is recorded; the user's own
request is refused too. The refusal carries `agent`, `account` (the account's name), `window` (the used-up window's
name: `5-hour`, `Weekly`, `Monthly` or `Credits`) and, when the agent gives a reset time, `resets_at` in milliseconds
since the epoch. In the window it is shown in an error alert, in the UI's language, saying the account cannot run a
session because it has used up that quota and, when known, how long until it resets, counted from when the alert is
shown; the next step is to choose another account that still has quota, or to try again once the quota resets. An
agent's tool call is refused with a reason in English naming the account, the agent and the window, about how long
until it resets when known, and saying to choose another account or wait for the reset. A session request is refused
that way before the user is asked, and again on approval should the account be used up by then (see
`docs/product/session-requests.md`). Only a session being opened is checked: a session already on the account keeps
running, and resuming, reopening or restarting it is never refused for this.

With every agent determined unavailable, a console session's `new_session` tool is refused as well, with a reason it
can report that says the same (see "The console session's tools" in `docs/product/hub-orchestration.md`). Nothing is
refused for an agent being unavailable while availability is still being determined: a session then opens or fails
exactly as described above.

### Opening a session with defaults

While the platform's primary modifier is held — ⌘ on macOS, Ctrl elsewhere, each without the other — every sidebar
**+** that opens the session dialog turns into its quick form: it shows a bolt in place of the plus, and its tooltip
and accessible name say it opens the session **with defaults**. Pressing it then opens the session at once, without
the dialog. These are:

- the console header's **+**, New console session (see "The console header" in `docs/product/sidebar.md`);
- each project row's **+** (see "Project rows" in `docs/product/sidebar.md`);
- the header's **+** in a project's focus mode (see "A project's focus mode" in `docs/product/focus-mode.md`);
- in a console session's focus mode, each project's own **+** and the header's **+** menu of projects, where the
  project chosen is opened with defaults (see "A console session's focus mode" in `docs/product/focus-mode.md`).

The empty rows' and the terminal's **New session** and **New console session** offers have no quick form; they always
open the dialog.

A **+** button pressed with the modifier opens with defaults even when the hold had not been noticed before the press.
A project chosen from the console session focus mode's menu follows whether the modifier is held at the moment it is
chosen. The hold ends when the key is released, when the window loses focus and when the page is hidden; a modifier
already down while the window was in the background is picked up once the pointer moves or a key is pressed.

The session opened is the one the dialog would open if submitted untouched:

- the localized default name (see **Name** above);
- the agent and account the dialog opens on (see "Which agent a session uses" and "Which account a session uses"
  below), sent as the session's own choice;
- no model or reasoning effort of its own, so both follow the parent;
- a new conversation, never a resumed one;
- **unbound** — from the project list too, where the dialog's **Report to** starts on no console session — except in a
  console session's focus mode, where it is bound to that console session, as the dialog opened there binds it.

It is listed as *starting* and selected, as a session opened from the dialog is, and a launch failure is reported the
same way (see above). A refusal is shown in an error alert. When the console
session it would be bound to is no longer available — archived or gone — nothing is sent, and an error alert says so,
with the same message the dialog shows in that case. Only one session opens with defaults at a time: further presses
are ignored until the daemon has answered the previous one. If the sidebar has switched to another console by the time
the answer arrives, the session is still opened but not selected.

When no agent is both added and installed, no **+** shows its quick form, and every one of them is disabled (see
"Opening a session" above).

When the account the dialog would open on is used up, the session opened with defaults takes another account of the
same agent, as the dialog does (see **Agent** above). When every account of that agent is used up, there is nothing
to open with defaults:

- the **+** in its quick form looks disabled but stays focusable and under the pointer; its accessible name stays its
  action, while its tooltip, and its description for assistive technology, say in short that the default agent's
  quota is used up, without naming the agent;
- pressing it — or pressing a **+** with the modifier before the hold was noticed — opens nothing and shows an error
  alert that names the button's action and says it is unavailable because the default agent's accounts have all used
  up their quota, with the advice to choose another agent in the new session dialog, or to try again once the quota
  resets;
- in the **+** menu of projects in a console session's focus mode, held in its quick form, each such project is
  disabled, with a short note under its name saying the default agent's quota is used up; it stays reachable with
  the arrow keys, and its accessible name ends with that note.

Without the modifier, the same **+** opens the dialog as usual.

### Which agent a session uses

In descending priority:

1. the agent chosen for this session when it was opened;
2. the project's default agent;
3. the console's default agent for project sessions;
4. the console's default agent for console sessions.

A console session has no project, so it uses the agent chosen for that session, or the console's default agent for
console sessions when none is given; the default for project sessions never applies to it. Both console defaults are
in `docs/product/consoles.md`. Console sessions of different agents can run alongside each
other in the same console, and all read the same instruction file, described in "The console session's instruction
file" in `docs/product/launching-agents.md`.

The same order settles the agent everywhere a session's agent is not given outright: the session dialog's initial
choice, a session opened with defaults, `new_session` without an `agent`, a session request without one once it is
approved (see `docs/product/session-requests.md`), and a session of a project associated with "Detect from files" whose
directory named no agent (see "The project's default agent" in `docs/product/projects.md`).

A session's agent is fixed for its lifetime. Resuming or reopening a session always relaunches the same agent —
session records belong to a specific agent and cannot be moved across agents.

### Which account a session uses

The account of the session's agent, in descending priority:

1. the account chosen for this session when it was opened;
2. the project's account for that agent, where the project does not leave it to its console — a project set to **the
   agent's default account** ends the chain there, whatever the console names;
3. the console's account for that agent;
4. that agent's default account.

A console session carries no project, so after its own account choice its chain goes straight to the console's
reference. A project leaves the account to the console for every agent it inherits for, which is how every project
starts (see "The project's account, model and reasoning effort" in `docs/product/projects.md`). A session opened without
changing the account picker therefore runs under the project's account for its agent, or the console's, or the agent's
default account when neither pins anything. The session's account is identified by its agent icon's tooltip and its
accessible session name, without an account suffix in visible session metadata (see "Session rows" in
`docs/product/sidebar.md`).

A session that makes no choice of its own — one a console session or a project session starts with its own
`new_session` tool, or one started on another session's request, without an `account` — takes the project's
setting. The dialog that opens a session by hand always makes a choice,
opening on the account the rest of the chain resolves to (the project's setting for the agent, else the console's, else
the agent's default account) and sending that as the session's own choice, so a session opened there runs under that
account unless the user picks another in the account picker.

### Which model and reasoning effort a session uses

A session uses its own model and reasoning effort choices, each independently, or inherits the current defaults for
its agent: a project session from its project, else its console's project session pair; a console session from its
console's console session pair. The inheritance, explicit clearing and persistence
rules are in `docs/product/model-and-reasoning-effort.md`. How the resolved values reach the agent is in "The model
and reasoning effort" in `docs/product/launching-agents.md`.

### Resuming a conversation started outside Octoboard

A project session can be opened to continue a conversation the user started with the agent outside Octoboard — Claude
Code, Codex or Grok Build run in a terminal — instead of starting a new one. The session dialog's **Resume
conversation** tab, beside the default **New conversation** tab at the head of the dialog, starts with a required
**Session ID** field for the conversation's session ID — the one the agent's own resume command takes. The tabs are
offered for a project session only, from the project list, the terminal's **New session** offer and a project's focus
mode alike: the dialog for a console session has none, and neither has the dialog opened from a console session's
focus mode, whose session always reports to that console session. A session started with `new_session` never resumes one. In the daemon protocol, `open_session` takes the ID
as `resume_agent_session_id`; blank is read as absent.

The tab's other fields apply to a resumed session as to a new one: its name, its agent and account, and the model and
reasoning effort in the Advanced section, which are passed to the agent when it resumes the conversation. The ID is
sent only when the dialog is submitted from the **Resume conversation** tab; one typed there and then left for the
**New conversation** tab is not.

- **A resumed session is a plain, unbound project session.** The **Resume conversation** tab has no **Report to**
  field and the session is opened unbound; the **New conversation** tab still shows the choice made there before. `open_session` refuses a request that names both a conversation to resume and a session to report to
  (`conflicting_fields`).
- **The ID must be a UUID.** The field takes the hyphenated form the agents show (such as
  `123e4567-e89b-12d3-a456-426614174000`), in either letter case. An empty field, or anything else, stops the submit
  and is reported under the field — asking for the session ID of the conversation to resume when it is empty, and
  giving an example ID otherwise — and focus moves to the field. From then on the message follows what is
  typed. `open_session` takes any spelling of a UUID, trimmed, and keeps it lowercase and hyphenated; anything else is
  refused (`resume_id_invalid`).
- **A conversation already in Octoboard is refused.** No two sessions share one conversation: an ID that a session
  already records as its agent's conversation, live or archived, is refused. A session of the same project is named,
  and the user is told to resume or reopen that session instead (`conversation_in_session`); a session outside the
  project — of another project, or a console session — is only said to be outside this project
  (`conversation_outside_project`). The check is made again once the new session is recorded, so two requests for the
  same conversation cannot both succeed.
- **The conversation must be in the session's account directory.** It is looked for in the config directory of the
  account the session will use (see "Which account a session uses" above): the account's own directory, or for the
  default account the directory the agent defaults to under the user's login shell environment (see "Agent config
  directories" in `docs/product/consoles.md`). With no record of it there, the open is refused, naming the
  agent and the directory but not the ID, which is in the field above (`conversation_to_resume_not_found`). A
  conversation started in a terminal is
  therefore found only when the session's account resolves to the same config directory that terminal's agent used.
  For the default account, a login shell environment that could not be captured refuses the open as well (see "The
  launch environment" in `docs/product/launching-agents.md`).
- Every refusal comes before anything is recorded or launched: no session appears, and the reason is shown in an error alert or under the affected field. `open_session` also refuses an ID for a console session (`resume_for_console_session`).
- **The refusals about the conversation are shown under the Session ID field**: an ID
  that is not a conversation ID (`resume_id_invalid`), a conversation already in a session (`conversation_in_session`,
  `conversation_outside_project`) and one not found (`conversation_to_resume_not_found`). The field is marked invalid
  and takes focus. The refusal goes when the dialog is submitted again, and as soon as the ID, the agent or the account
  is changed, since that asks about a different conversation; it is not shown on the **New conversation** tab. Any
  other failure of the open is shown in an error alert.

A session opened this way starts by resuming that conversation, and comes up at the agent's prompt like any session
opened from the dialog. It **has a conversation from the moment it is opened**, not from its first turn: interrupted
before the user has typed anything, it resumes that conversation rather than starting a fresh one; it is never unused,
so it is archived rather than deleted (see "Deleting an unused session" in `docs/product/deleting-sessions.md`);
switching its account copies that conversation; and its relaunch is refused when its pinned config directory has gone (see "Archiving, interruption and
resuming" in `docs/product/archiving-and-resuming-sessions.md`). Octoboard's status hooks and tools are injected as on any resume, so its status is tracked and its
tools are there; whether the role description takes effect in it is in "What is injected on every launch" in
`docs/product/launching-agents.md`.

## Session statuses

| Status | Wire value | Glyph | Meaning |
|---|---|---|---|
| Starting | `starting` | a muted arc turning | A newly opened session — from the dialog, by an agent's `new_session`, or by an approved session request — whose agent is still being launched; it has no process yet. It becomes *awaiting instructions*, or *working* for a session started with a brief, once the process is up, and is removed again if the launch fails (see "Opening a session"). |
| Working | `working` | an accent-coloured dot pulsing a fading copy of itself outward | The agent is executing a turn. |
| Waiting for the user | `waiting_user` | a raised hand that waves now and then | The agent is waiting on a permission decision, including its folder-trust confirmation, or has asked the user a question through its own ask-the-user tool; or Octoboard is asking the user on the agent's behalf while the agent's call waits, which is a session request (see "Waiting for the answer" in `docs/product/session-requests.md`). |
| Awaiting instructions | `idle` | a green speech bubble | The process is running and sitting at its prompt. |
| Running in background | `idle`, with a non-empty `background_tasks` | the *working* glyph as a hollow green ring: a green ring pulsing a fading copy of itself outward | The process is sitting at its prompt, but its last turn ended with work still running in the background. Claude Code sessions only; see "Running in background" below. |
| Interrupted | `interrupted` | a pause sign | No process is running, and it did not end by being archived. The session stays in its project's list (a console session, in the console sessions section) and can be resumed. |
| Archived | `archived` | an archive box | Ended by being archived (see "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`). No longer listed among its project's sessions (a console session, no longer in the console sessions section); reached through the archive (see "Archived sessions" in `docs/product/archiving-and-resuming-sessions.md`) and can be reopened. |

*Working*, *waiting for the user*, *awaiting instructions* and *running in background* mean a process is running, and
*starting* means one is about to be; the last two mean none is, and both can be resumed.

The glyph stands for the status wherever one is shown — the sidebar's rows, focus mode's cards, the top bar's
breadcrumb and a confirmation's list of the sessions it affects. In a confirmation, archived sessions omit the
status glyph; the warning identifies them as archived. The waving hand and the outward pulse stand still
where the system asks for reduced motion, leaving the working glyph a plain accent-coloured dot and the *running in
background* glyph a plain hollow green ring; working and awaiting instructions still differ by shape as well as by
colour. The starting arc stands still there too and differs from the rest by shape.
*Running in background* is the *working* glyph drawn as a hollow ring in *awaiting instructions*' green: it is told
apart from *working*'s filled dot by shape as well as by colour, and from *awaiting instructions* by shape.

**How the status reaches a user who does not read the glyph** differs by placement:

- On a **sidebar row** and on a **focus mode's card** the glyph adds nothing of its own for assistive technology: the
  row's own accessible name says the status, and a card shows it in words beside the glyph.
- On the **top bar's breadcrumb** and in a **confirmation's list of the sessions it affects** nothing around the glyph
  says the status, so the glyph itself is named after it.
- Every placement where no status text sits beside the glyph gives it a **tooltip** saying the status in the same
  words — the **sidebar's rows**, the **top bar's breadcrumb** and a **confirmation's list** (see "Tooltips on
  indicators" in `docs/product/labels-and-tooltips.md`). Focus mode's cards say the status in words already and
  carry none.

Transitions:

- Opening a session puts it in *starting*; once its process is up it moves to *working* when it is handed an opening
  prompt (a session started with `new_session`, or on another session's request that the user approved), and to
  *awaiting instructions* otherwise, unless the agent's own first report has already moved it on (see "Opening a
  session"). A launch that fails removes the session.
- While the process runs, reports from the agent move the session between *working*, *waiting for the user* and
  *awaiting instructions* — except while a folder-trust confirmation, Codex action prompt or session request holds
  it at *waiting for the user* (see "What the statuses are derived from" below). A Claude Code session that would be
  *awaiting instructions* reads as *running in background* while it has work left running there (see "Running in
  background" below).
- The process ending for any reason other than archiving — the agent exiting on its own, a crash, the application
  quitting — leaves the session *interrupted*.
- Archiving leaves the session *archived*. User-initiated archiving of an owner also ends and archives the sessions under it;
  agent-initiated archiving is refused while any descendant has a process running (see "Archiving, interruption and
  resuming" in `docs/product/archiving-and-resuming-sessions.md`).
- Resuming an *interrupted* or *archived* session puts it in *awaiting instructions*, whichever of the two it came
  from. A resume that is refused or fails to launch leaves the session exactly where it was, archived included. Resuming a session
  whose owner is *archived* first does the same to its archived owners, the topmost first.
- Switching a session to another account of its agent ends and relaunches its process without archiving it: it reads
  as *interrupted* while the process is down and comes back in *awaiting instructions*, and a switch that fails leaves
  it *interrupted* on the account it had (see `docs/product/switching-session-accounts.md`).
- An agent's report never moves a session out of *interrupted* or *archived*: whether a stopped session is one or the
  other is Octoboard's own record, not the agent's.

### What the statuses are derived from

Status comes from hook events Octoboard injects into each agent per launch, with the exceptions below. Only the
session's own events count: those of a subagent it runs never change its status, on any agent, and on Claude Code
neither do those of its internal helper agents. Five things besides those events bear on a running session's status,
and only these five.

- While a **Claude Code** session is *working* or *waiting for the user*, Octoboard also reads that session's own
  transcript file — the machine-readable record the agent keeps of the conversation, still not its rendered output —
  because neither a turn the user cancels nor a declined prompt is reported by any hook event; see "Cancelling a
  Claude Code turn" and "Declining a Claude Code prompt or question" below.
- While a **Codex** session has a turn open, Octoboard also reads that session's own rollout file — the record Codex
  keeps of the session, again not its rendered output — because a turn that ends in an error is reported by no hook
  event at all; see "A Codex turn that ends in an error" below.
- A **Codex** terminal title showing `Action Required` holds the session at *waiting for the user*, including
  a prompt for one of Codex's subagents. Both phases of its blinking indicator keep the hand up; other hook reports
  cannot lower it while that title remains. See "The raised hand" below for its limits.
- A recognised folder-trust confirmation that needs the user holds its session at *waiting for the user*, even
  before the agent's first hook report. Its raised hand follows the confirmation rather than the dialog; see
  "What happens when a confirmation comes up" in `docs/product/folder-trust.md`.
- A session request — a `request_session` call — waiting for the user's answer holds
  its session at *waiting for the user* for the whole wait, whatever the agent's own events say meanwhile; when the
  request ends the session goes back to where its agent was last seen to be (see "Waiting for the answer" in
  `docs/product/session-requests.md`).

A session's end is not taken from a hook either: the process is observed directly.

Where an agent reports nothing, the status simply stays at its last reported value. The known cases, which are
limitations of what the agents expose rather than of this one:

- A turn that ends with the agent asking a question **as plain prose** is indistinguishable from a finished turn on
  all three agents, so such a session reads as *awaiting instructions* rather than waiting for the user.
- Grok Build's bash mode (`!`) fires no tool or turn events, so work done through it never shows in the status; Grok's
  turn-end backstop was seen to follow about a minute later in one run (see "When a session does not report" in
  `docs/product/reporting-to-owners.md`), but it finds the session already awaiting instructions.
- A question the agent asks through **its own ask-the-user tool** is reported by Claude Code and Grok
  Build but not by Codex, which has no such event; a Codex session asking that way reads as *working* unless its
  terminal title signals `Action Required`.
- Work left running in the background is followed for **Claude Code only**. A Grok Build or Codex session whose turn
  ends with work still running reads as *awaiting instructions*, never *running in background*.

Part of the Grok Build case above is Octoboard's own attribution rule rather than a limit of what the agent exposes.
Grok Build's turn-end backstop carries no turn id, so the daemon attributes it by clock. An ending that names its own
turn, or a cancellation, arms one expected backstop (a flag, not a count), and the next clock-attributed signal spends
it whether or not a turn is open. When an ending's own backstop never arrives, the flag stays armed, so a later turn
whose only ending is its backstop would have that backstop discarded: the session would keep reading as working and,
if it is bound to an owner, no report would be synthesized for that turn. Whether Grok produces that sequence in
practice was not checked (in one run an ending followed within a second by a new prompt got no backstop).

### Running in background

A Claude Code turn can end while work it started is still running in the background — a shell command sent there
(by the user, or by Claude Code when it runs past the tool's time limit), a Monitor, an MCP tool call still waiting, or
a background agent. Claude Code lists that work as the turn ends, and as its background subagents start and stop
between turns, and the session then reads as *running in background* instead of *awaiting instructions* until none of
it is left. On the wire it is still `idle`; what sets it apart is the session's `background_tasks`, one entry per piece
of work: `id`, and `type` and `description` where Claude Code gives them.

**How the list is kept:**

- Every turn end Claude Code reports carries the complete list of what is still running, and replaces the one before;
  a turn end listing nothing empties it. A turn that ends in an error empties it too.
- When a piece of work finishes, Claude Code tells the session so in a turn of its own, naming it; that work leaves the
  list, and the session reads as *working* for that turn as for any other.
- Between turns, while the session is at its prompt, a background subagent's own starts and stops keep the list
  current, since a subagent can wake to its own background work without the session taking a turn. A subagent that
  starts or resumes joins the list; one that stops leaves it, and the rest of the list becomes everything Claude Code
  reports as still running across the whole session at that moment. A subagent's activity never changes the
  session's status: while one works between turns the session keeps reading as *running in background*, not
  *working*. Inside a turn these starts and stops change nothing, the turn's end listing everything. Only a named
  subagent counts; Claude Code's internal helper agents are never listed. A subagent listed because it started
  carries its description only if an earlier report had named it.
- A subagent stop that would leave nothing running takes effect only after about **2 seconds**, so one subagent
  handing over to the next does not make the session flash to *awaiting instructions* in between; a turn end, a
  subagent starting, or another subagent stop within that time decides the list instead.
- The list goes when the session's process ends — archived, interrupted, switched to another account or handed off —
  and the work goes with the process, as it does when the agent exits on its own (see "Archiving, interruption and
  resuming" in `docs/product/archiving-and-resuming-sessions.md`).
- It is given only while the session is at its prompt: in any other status the session shows that status and no
  list, and what was last listed shows again if it comes back to its prompt without a new turn end.
- Clearing the conversation with `/clear` does not end work running in the background (measured on Claude Code
  2.1.303), so the session keeps reading as *running in background*.

**In every other respect it is a session awaiting instructions.** It takes a message straight away (see "Messages held
until a session can take them" in `docs/product/message-delivery.md`), and it can be handed off (see "What can be
handed off" in `docs/product/handing-off-sessions.md`). Unlike one awaiting instructions with nothing left running,
archiving it asks for confirmation first (see "Archiving, interruption and resuming" in
`docs/product/archiving-and-resuming-sessions.md`), quitting the application asks too (see "Quitting" in
`docs/product/application-lifecycle.md`), and a turn that ends this way is not reported to its owner (see "When a
session does not report" in `docs/product/reporting-to-owners.md`).

**It has its own glyph and name** wherever a session's status is shown or said — its row, focus mode's card, the top
bar's breadcrumb, a confirmation's list of sessions, the row's accessible name and the glyph's tooltip. **For project
ranking and activity summaries, it counts as working**: the order of projects in the sidebar, a console's badge on the
rail and a collapsed project's activity marker (see "Order of projects and sessions", "The console switcher" and
"Project rows" in `docs/product/sidebar.md`), and a switch strip chip (see "The switch strip" in
`docs/product/focus-mode.md`). Session lists give it a separate place before working sessions, following
"Order of projects and sessions" in `docs/product/sidebar.md`. The menu bar icon's menu lists it in a group of its own,
after **Working** (see "The icon's menu" in `docs/product/menu-bar-icon.md`).

An owner's tools give the list with every session they describe (see "The console session's tools" in
`docs/product/hub-orchestration.md`).

### The raised hand

*Waiting for the user* is the raised hand, and it is made findable rather than left on the session's
own row:

- The session's row shows the raised hand. Its project row shows one while the project is collapsed,
  and its console's avatar on the rail carries one, the rail showing every console at once, so a
  waiting session can be found whatever the sidebar is showing (see "The console switcher" and
  "Project rows" in `docs/product/sidebar.md`). In a console session's focus mode, a console session's
  chip in the switch strip shows one too while it or a session bound to it is waiting (see "The switch
  strip" in `docs/product/focus-mode.md`).
- A system notification fires once as a session enters that state, naming the session by its title
  and the project it runs in — or the console whose console session it is. A session that is answered and later
  waits again notifies again. No notification permission is asked for: the application posts the
  notification directly, and macOS showed no permission prompt. The banner was seen while Octoboard
  was not the frontmost application; while it was frontmost, no banner was seen. A notification that
  cannot be shown is not reported as having failed — the sidebar's own marker carries the same signal.
- The Dock badge carries how many sessions are waiting, counted across every console, and clears
  when none is. While the window is closed into the background Octoboard has no Dock icon; the count
  is still kept and shows once it returns to the Dock (see "Closing the window" in
  `docs/product/application-lifecycle.md`). The menu bar icon's menu lists the waiting sessions too
  (see "The icon's menu" in `docs/product/menu-bar-icon.md`), and the icon itself carries an orange dot while
  any is waiting (see "The icon's look" in `docs/product/menu-bar-icon.md`).
- The rail carries the same count, as a raised hand and the number, shown only while at least one
  session is waiting. Pressing it selects the next waiting session after the selected one — console by
  console in the order the consoles were created, its console sessions before its projects' sessions,
  and both in the sidebar's order (see "Order of projects and sessions" in `docs/product/sidebar.md`) —
  and wraps from the last back to the first; when the selected session is not waiting, it selects the
  first. The sidebar follows the session it selects (see "Selecting a session" in
  `docs/product/sidebar.md`).

**Where the hand is the agent's own, the user answers in the session's terminal**, and the status leaves *waiting for
the user* on the agent's next event or when its Codex action title clears — or, where the answer was a decline and no
event follows, on the decline showing up in the agent's own record of the conversation, which only Claude Code
sessions are read for (see "Declining a Claude Code prompt or question" below). Nobody can answer for them: a session
driving others — a console session, or a
project session that started sessions — is told to leave such a session alone, and a message addressed to it is held
until the user is done — see "Messages held until a session can take them" in `docs/product/message-delivery.md`.

**A granted tool permission prompt reported through hooks lowers the hand only once the call it let through has
finished.** The user answering *yes* is itself reported by no event, so the next event the session produces is normally
the end of that tool call — the hand
stays up from the moment the prompt appeared until the call is done, which is a moment for a file write and minutes for
a build or a test run. For that whole time the session reads as *waiting for the user* everywhere the hand is shown, it
counts in the rail's waiting count and the Dock badge, and a message addressed to it goes on being held. There is no
earlier signal to read: the delay is in what the agents report, not in how often Octoboard looks. Measured on Codex
0.160.0 and on Claude Code 2.1.274; Grok Build was not measured. A prompt the user *declines* is not delayed this way:
the decline ends the turn, and the hand comes down right after the answer.

A hand raised for a folder-trust confirmation can be answered in its trust dialog or the terminal; see "What happens
when a confirmation comes up" in `docs/product/folder-trust.md`.

A hand Octoboard raised for a waiting session request is answered in that request's dialog instead, so a message
addressed to such a session is not held (see "Waiting for the answer" in `docs/product/session-requests.md`).

Where Codex's effective approval mode enables **automatic approval review** (`auto_review`) — from the account's
explicit choice or its own configuration (see "Approval modes" in `docs/product/agent-accounts.md`) — a permission event
alone raises no hand: requests Codex resolves itself keep the session reading as *working*. If a manual prompt
does reach the terminal, its `Action Required` title raises the hand, including a prompt labelled as belonging to
a Codex subagent. Messages addressed to the session are held while that signal is present. When Codex changes the
title back after the answer, the session returns to its last hook-reported status and queued messages are released
if that status permits delivery, without waiting for another hook event.

The title does not override a hook-reported wait after it clears: ordinary approvals whose last hook status is
still *waiting for the user* keep the hand up until the tool finishes, as described above. This detection also
requires Codex's activity title: disabling terminal titles, or customising `tui.terminal_title` to omit activity
or its blinking indicator, removes the signal. Octoboard does not change the user's Codex configuration to enable it.

### Declining a Claude Code prompt or question

A Claude Code session with its hand up that is then **declined** — the permission prompt answered No
or dismissed with Esc, or the agent's own ask-the-user question cancelled — reports nothing at all:
no hook event of any kind follows, while the agent is already back at an empty prompt. So for this
one case Octoboard reads the session's own transcript file, the record Claude Code keeps of the
conversation, and moves the session to *awaiting instructions* once the decline appears there,
normally within about a second of the user answering. Both ways a Claude Code session raises its hand
are covered: a permission prompt and its own ask-the-user question.

What follows from that move is nothing special to this path: the raised hand comes down wherever the
sidebar showed it, the Dock badge count drops, and a message queued
for the session while its hand was up is released (see "Messages held until a session can take them"
in `docs/product/message-delivery.md`). A later prompt or question in the same session raises the
hand and notifies afresh, as any other does.

- The agent prints that the turn was interrupted, but its process is still running, so the session is
  *awaiting instructions* and not *interrupted*.
- No turn end was reported, so no report is synthesised for the session's owner for that turn either (see "When a
  session does not report" in `docs/product/reporting-to-owners.md`).
- **Claude Code sessions only.** Codex and Grok Build report a decline through their own hook events,
  and the transcript read here is Claude Code's own format; neither is watched this way.
- Only a hand that is currently up is recovered this way. A turn cancelled while the session reads
  as *working* is recovered by its own watch — see "Cancelling a Claude Code turn" below.
- Only the decline Claude Code records as the user's own entry counts. The transcript also logs
  each request sent to the model, replaying the conversation so far — earlier declines included;
  a decline replayed inside one does not lower the hand.
- Where an ask-the-user question is left not by declining it but by asking to chat about it instead,
  the turn in fact carries on; Octoboard reads that as a decline too, so the session reads as
  *awaiting instructions* for a moment until the agent's next event puts it back to *working*.
- If the record cannot be read — the pending-decision event named no transcript file, or the file is
  unreadable — nothing lowers the hand, and the session keeps it up until something else moves its
  status.
- A decline is noticed the same way while a session request holds that session's hand up (see
  "Waiting for the answer" in `docs/product/session-requests.md`). The hand stays up until the
  request ends, since it is the request's own; the session is then *awaiting instructions*, where the
  decline left its agent.

**What this costs to keep working.** The transcript is a file format Claude Code owns and rewrites on
upgrade, and a decline is recognised by two fixed marker strings inside it; the behaviour was
measured against Claude Code 2.1.274 and 2.1.286, and the hand was seen coming down in the running
application against 2.1.289. A release that renames those markers breaks this
silently and completely — there is no error, nothing is reported as having failed, and the only
symptom is a declined prompt leaving the raised hand up with nothing to lower it. Nothing Octoboard
can observe by itself tells that apart from a user who simply has not answered yet.

### Cancelling a Claude Code turn

A Claude Code session reading *working* whose turn the user **cancels** — Esc pressed while the
model is thinking, writing or running a tool — reports nothing: the cancelled turn fires no hook
event, while the agent is already back at its prompt. So Octoboard reads the session's own
transcript file while it works, and moves the session to *awaiting instructions* once the interrupt
Claude Code records there appears, normally within about a second of the cancel. Claude Code records
it as a user entry reading `[Request interrupted by user]` (cancelled while the model thinks or
writes) or `[Request interrupted by user for tool use]` (cancelled while a tool runs).

- **When the transcript is read.** The watch starts each time a hook event moves the session into
  *working* — a submitted prompt, or any other event that finds it not already working, such as
  the hand coming down on an answered prompt — and stops once the session leaves *working* some
  other way, raises its hand, or its process ends.
- **Only this turn's cancel counts.** Only entries written after the watch started are read, and an
  interrupt counts only if it is timestamped no earlier than a few seconds before the watch started,
  so an earlier turn's interrupt, and one in the history of a resumed or copied conversation, does
  not. An interrupt replayed inside a logged request does not count either, for the reason given in
  "Declining a Claude Code prompt or question" above. Each submitted prompt retires the previous
  turn's watch before its own turn opens, so an interrupt found late never ends the turn after it.
- The agent's process is still running, so the session is *awaiting instructions* and not
  *interrupted*.
- No turn end was reported, so no report is synthesised for the session's owner for that turn (see
  "When a session does not report" in `docs/product/reporting-to-owners.md`).
- **Claude Code sessions only.** Codex and Grok Build report a cancelled turn through their own hook
  events, and the transcript read here is Claude Code's own format; neither is watched this way.
- A cancel while the session's hand is up is a decline, covered by "Declining a Claude Code prompt
  or question" above.

Where it is not recovered, the session keeps reading *working* until its next prompt is submitted:

- the hook event that moved the session into *working* named no transcript file, or the file cannot
  be read;
- the interrupt entry carries no timestamp Octoboard can read;
- the interrupt reached the transcript before the hook event that moved the session into *working*
  reached Octoboard — hook events are delivered asynchronously — in which case it can be missed;
- an event from the cancelled turn reaches Octoboard after the move, reporting the session as
  *working* again.

**What this costs to keep working.** As with a decline, the transcript is a file format Claude Code
owns and rewrites on upgrade, and a cancel is recognised by the two fixed interrupt strings above;
they were seen in real transcripts of Claude Code 2.1.296, 2.1.302 and 2.1.303. A release that
renames them breaks this silently — there is no error and nothing is reported as having failed —
and the only symptom is a cancelled turn reading *working* until the next prompt.

### A Codex turn that ends in an error

A Codex turn that **ends in an error** — the account's usage limit reached, for one — is reported by
no hook event at all: Codex has no failure event and no notification event, so nothing follows the
turn's prompt submission, while the agent is already back at its prompt. So for this one case
Octoboard reads the session's own rollout file, the record Codex keeps of the session, and moves the
session to *awaiting instructions* once the turn appears there as ended with an error — normally
within about a second of the prompt, and so up to about half a second behind Codex's own return to
its prompt. Every turn of a session is covered, its first included.

- The agent's process is still running, so the session is *awaiting instructions* and not
  *interrupted*.
- The turn is reported on: a session bound to an owner has a report synthesised for it with status
  *failed*, carrying Codex's own error text and the usual line saying the session stopped without
  reporting (see "When a session does not report" in `docs/product/reporting-to-owners.md`).
- **Codex sessions only.** Claude Code and Grok Build report a failed turn through their own hook
  events, and the file read here is Codex's own format; neither is watched this way.
- A turn Codex ends normally, and one the user cancels, are reported by Codex's own hook events, and
  nothing is read from the rollout for them.
- If the record cannot be read — the prompt submission named no rollout file or no turn, or the
  turn's end never appears there — nothing moves the session, and it keeps reading *working* until
  something else does.

**What this costs to keep working.** The rollout is a file format Codex owns and rewrites on upgrade,
and a failed turn is recognised by the shape of the record Codex writes for the turn's end; the
behaviour was measured against Codex 0.160.0. A release that changes that shape breaks this silently
— there is no error and nothing is reported as having failed — and the only symptom is a session
reading *working* while Codex sits at its prompt.

## Locking a session

Locking is the user's protection against archiving or deleting a session. Every session starts unlocked,
including sessions already stored without a lock setting. The lock is shared by every client, survives a daemon
restart, and stays set when the session is resumed or reopened. Locking does not change its position in a list,
its status, or its binding. The controls are described in "Session lock controls" in `docs/product/sidebar.md`.
Agents have no tool to lock or unlock a session.

An archive or deletion that includes a locked session is refused with `session_locked`, identifying that session,
and changes nothing. This covers the session itself, an owner's sessions at both levels, deleting archived sessions
in bulk, removing a project, and deleting a console. A locked descendant blocks archiving or deleting its owner,
even when the descendant is already archived. A bulk operation is refused as a whole rather than skipping locks.
Unlock the protected session before retrying. A completion report still reaches its owner but does not archive
a protected session or owner (see "Automatic archiving" in `docs/product/reporting-to-owners.md`).

While an archive or deletion is in progress, changing any session's lock is refused with `session_lock_busy`;
retry after that operation finishes. A failed lock change leaves the stored lock unchanged.

## Archiving, interruption and resuming

Archiving a session, an interruption, resuming and reopening a dormant session, when its conversation can no longer be
resumed, and how long archived sessions are kept are described in `docs/product/archiving-and-resuming-sessions.md`.

## Renaming a session

A session that is not archived offers **Rename** in its action menu; an archived session's menu has no Rename item.
The rename field contains the created name alone; an empty name is rejected. The daemon's `rename_session` request
and an owner's `rename_session` tool still accept archived sessions. An agent's terminal title remains separate
(see "Displayed names" in `docs/product/session-titles.md`).

## The terminal

The selected session's terminal — what its area shows with no session selected, typing into it, its output and
scrollback, reconnecting and loading, a dormant session's saved preview, links in its output, and what it offers while
a console has nothing in it — is described in `docs/product/session-terminal.md`.
