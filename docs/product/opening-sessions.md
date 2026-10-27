# Opening a session

New console session from the console header, the console sessions section's empty row, and the terminal's offer of the
same action open a dialog before anything is launched. New session under a project opens the same dialog for that
project. The sidebar's **+** buttons can also open a session without the dialog, on the values the dialog would open on;
see "Opening a session with defaults" below.

## The session dialog

The dialog is titled "Open session" for a project session and "Open console session" for a console session; the first
sentence of its body names the project, or the console, the session will be opened in.

A project session's dialog opens with two tabs at its head, **New conversation**, selected by default, and **Resume
conversation**: the **Resume conversation** tab starts with a required **Session ID** field alone, and shows the fields
that follow only once the conversation is found; its agent and account are never picked there, and it has no **Report
to** field (see `docs/product/resuming-a-conversation-by-its-session-id.md`). What is typed or picked is kept across a
switch between the tabs, while the Advanced section folds back closed. The dialog for a console session has no tabs, and
neither has the dialog opened from a console session's focus mode, whose session always starts a new conversation.

A dialog without tabs, and the **New conversation** tab, offer these fields in this order:

- **Name** (optional) — the empty field previews the default name: the project's name for a project session, or the
  console's name for a console session, followed by the session label in the UI's current language. Leaving it empty
  uses that default when the session is created, with no numeric suffix; multiple sessions can have the same name. The
  field stays empty until the user enters a custom name. The dialog removes surrounding whitespace from an entered name
  and uses the default when nothing remains; a custom name gets no session label added. Renaming the parent or changing
  the UI's language affects future defaults; existing sessions keep their created names. A task title can still appear
  before either kind of created name, as described in "Displayed names" in `docs/product/session-titles.md`.
- **Agent** — an agent picker joined to an account picker, as in the project dialog. The agent picker lists the agents
  that can be picked, in the order of the added agents, with their icons and names, and otherwise only the one it holds
  (see "Which agents the agent pickers list" in `docs/product/consoles.md`). The field opens on the default resolved in
  "Which agent a session uses" below when that agent can be picked, otherwise on the first agent that can, whenever
  there is one. The account picker offers only that agent's accounts, with its default account first, and is shown even
  when the default account is the agent's only one, offering that account alone. It opens on the account "Which account
  a session uses" below describes. There is no inheritance entry: both pickers settle concrete choices for this session.
  Each account in the picker carries a compact usage bar (see "In an account picker" in
  `docs/product/account-quotas.md`). A used-up account (see "A used-up account" in `docs/product/account-quotas.md`) is
  listed but cannot be picked, with a short note under its name saying it is used up and when it resets. When the
  account the field would open on, or would fall to when the user turns to another agent without having picked one of
  its accounts, is used up, the field takes the first account of the same agent that is not, in the picker's order. When
  the chosen account is used up all the same — every account of the agent is, the agent's only account among them, or a
  reading arrives while the dialog is open — the field says under its pickers, which are marked invalid, that the
  account cannot be used for now, naming the quota it has reached and when it resets, and the dialog cannot be submitted
  until another agent or account is chosen or the account is no longer used up. An account whose sharing of the default
  configuration is being changed, which the daemon refuses a session on meanwhile (see "When a change is carried out" in
  `docs/product/sharing-the-default-configuration.md`), is listed but cannot be picked while the change runs, keeping its
  usage bar and its description but with no note under its name, as the change ends on its own; the field passes over
  it as it passes over a used-up account; and when the chosen account is one all the same, the field says under its
  pickers that its sharing is being changed, to choose another account or wait, and the dialog cannot be submitted until
  then. A turned-off account (see "Turning an account off" in `docs/product/agent-accounts.md`) is not listed, and the
  field passes over it as it passes over a used-up account. Only when the account the field ends on is turned off all the same — every
  account of the agent is unavailable, the account was turned off while the dialog is open, or the **Resume
  conversation** tab settles on it — is it listed, and then it cannot be picked: its usage bar stays and a short note
  under its name reads "Disabled", with the full reason as its description for assistive technology; the field says
  under its pickers that the account is turned off, to choose another account or turn it back on in Settings, and the
  dialog cannot be submitted until then. An account that is turned off is described as turned off, whatever its quota or
  its sharing. An account whose last change of sharing is incomplete can be picked, and submitting on it asks the user
  first (see "Starting a session on an account whose last change is incomplete" in
  `docs/product/sharing-the-default-configuration.md`). Switching agents retains each agent's account choice separately.
  If a remembered account is removed while the dialog is open, that agent stays selected and its account falls back to
  the account the field would open on for that agent (see "Which account a session uses" below); with its last named
  account removed, the picker stays, offering the default account. Switching agents returns focus to the Agent picker.
- **Report to** (project sessions only; a choice, "No console session" by default) — one of the console's console
  sessions that are neither archived nor still *starting*, the most recently started first, each shown beside its
  colour, the same one the session's binding badge will carry (see "The binding badge" in
  `docs/product/sidebar-session-rows.md`), or none. A chosen console session receives this session's reports, instead of
  the session staying outside the orchestration. A console with no such console session shows no choice at all, and the
  session is unbound. A session still starting is not offered because its launch may yet fail and remove it;
  `open_session` refuses one named as the owner (`unknown_session`). A binding can change afterwards — the owner hands
  the session to another, an unbound session binds itself to one, or the owner or the user releases the session, leaving
  it unbound; see `docs/product/changing-session-owners.md`. A session the console session itself starts is always bound
  to it, and one a project session starts is bound to that project session; one started on another session's request is
  bound to nobody; see "Which sessions an owner drives" in `docs/product/hub-orchestration.md`. The choice is offered
  from the project list and the terminal's **New session** offer outside focus mode. Opened from a project's focus mode
  the dialog has no such field and the session is always unbound; opened from a console session's focus mode it has none
  either, and shows a line saying the session reports to that console session instead, which is the binding it gets (see
  `docs/product/focus-mode.md`). Should that fixed console session become unavailable — archived or gone — no session is
  opened, and the dialog turns into a failure modal titled that a session cannot be opened, which identifies the
  unavailable owner, with its name set apart as a chip, and offers only its acknowledgement, which closes the dialog
  (see "The failure modal" in `docs/product/error-alerts.md`). A console session chosen in the optional Report to picker
  becoming unavailable instead returns that choice to no console session, keeping the form open. The **Resume
  conversation** tab has no such field, since a resumed session is never bound (see
  `docs/product/resuming-a-conversation-by-its-session-id.md`).

After these fields, the collapsed **Advanced** section offers the model and reasoning effort for this session. It
initially shows what the session would inherit for the selected agent (see "Which model and reasoning effort a session
uses" below); untouched values keep following it, and changes apply only to this session, including when resumed. The
choices, clearing and account-specific lists are described in "The Advanced section" in
`docs/product/model-and-reasoning-effort.md`.

A console session has no project or Report to field and is never bound to another session. Opening it with the daemon
protocol's `open_session` accepts an explicit agent and account as the dialog does. Its `title` is stored trimmed, and
one that is absent, `null` or blank uses the parent's current name alone: the console's name for a console session, or
the project's name for a project session. The daemon words no default name in the UI's language; the UI supplies its
localized default explicitly when no custom name was entered.

## Starting the session

**Every new session is listed at once as *starting*,** before its agent has been launched, and the launch goes on behind
it (see "The launch environment" in `docs/product/launching-agents.md`). The dialog does not wait for the launch: it
closes as soon as the session is recorded, and the session's terminal says it is starting until the agent is up (see
"Loading" in `docs/product/session-terminal.md`). A refusal that can be known before anything is recorded — an unknown
owner, an unavailable agent or one that is not added, a used-up account, a model the agent does not offer, the refusals
about a conversation to resume (see "Opening a new session on a found conversation" in
`docs/product/resuming-a-conversation-by-its-session-id.md`) — is still shown while the dialog is open, in an error
alert over it, and the dialog stays open. A session an agent opens with `new_session`, and the session an approved
request for one starts (see "On approval" in `docs/product/session-requests.md`), are listed as *starting* the same way,
but the agent's call, or the approval, is answered only once the launch has settled, with the launch's own refusal when
it fails.

The dialog takes no task: once up, a session the user opens by hand is in *awaiting instructions*, sitting at the
agent's prompt, and is given its work by typing into its terminal. Only a session started with `new_session` — by a
console session or a project session — or on another session's request that the user approved is handed an opening
prompt, the brief it is started with (see "Handing out a task: the brief" in `docs/product/hub-orchestration.md` and "On
approval" in `docs/product/session-requests.md`); once up, it is in *working*.

A session in a folder its agent has not been told to trust can first stop on that agent's own trust confirmation, before
it takes up its task or reaches its prompt. A resumed session can stop on it as well; since a session left interrupted
is relaunched only when it is selected or resumed, that is when its confirmation appears. Octoboard presses that
confirmation for the user once they have given permission — for that project, or for a folder its directory lies under,
whichever agent asked — or asks them first; see `docs/product/folder-trust.md`.

If the launch itself fails — the directory cannot be reached, the agent binary is not on the user's `PATH`, the user's
shell environment could not be captured (see "The launch environment" in `docs/product/launching-agents.md`), or, for
Grok Build only, the account the session would hold pins a directory that is not an initialized Grok home (see "Agent
config directories" in `docs/product/consoles.md`) or the launch would run under Grok's own sandbox (see "Grok's own
sandbox" in `docs/product/per-agent-launch-specifics.md`) — the *starting* session is removed again and no session is
left behind. For a session opened from the dialog, or with defaults (see "Opening a session with defaults" below), the
failure is reported in an error alert titled "Could not start the session", whose first line names the session, and
which gives the reason, worded from the failure's own code in the UI's language, with any raw text the cause carried
under **Error details** (see "The error alert" in `docs/product/error-alerts.md`); for a shell environment capture that
ran out of time, the explanation is laid out beneath that line — the command that did not finish and its time limit, the
likely cause, and what to fix. Should the session's record itself fail to be removed, it is left *interrupted* instead,
and the error alert is shown all the same. For `new_session` and an approved session request the failure is reported
through that call instead (see "When an approval cannot be carried out" in `docs/product/session-requests.md` for the
latter). A missing config directory does not fail a brand new session for the other two agents: it has no conversation
to lose, so Claude Code creates the directory itself and Octoboard creates it for Codex.

## When no agent can open a session

**When no agent is both added and installed, the window offers no way to open a session, before any of that** (an agent
counts as installed until it has been determined unavailable; see "Which agents the agent pickers list" in
`docs/product/consoles.md`). Every way into the session dialog is disabled: the console header's **+**, each project
row's **+**, the **+** of either focus mode's header and of each project in a console session's focus mode; the
project's empty row saying there are no sessions yet is plain text rather than a button, in the project list and in a
project's focus mode alike; and no **+** offers to open a session with defaults (see "Opening a session with defaults"
below). Where the window says why:

- when some supported agent is installed, or not yet determined unavailable, but none of those is added, the terminal's
  empty panel, where it would otherwise offer a new session, says that no agent is added and offers **Add agent…** (see
  "A console with nothing in it" in `docs/product/session-terminal.md`), and the console sessions section, with room for
  less, says only that no agent is added;
- when every agent has been determined unavailable, the terminal's empty panel, where it would otherwise offer a new
  session, says that no agent was found and how to make one available, and the console sessions section says only that
  no agent was found on the user's `PATH` (see "The console sessions section and the project list" in
  `docs/product/sidebar.md`).

If this state comes about while the session dialog is open — the last such agent removed or found unavailable meanwhile
— nothing is opened, and the dialog turns into a failure modal titled that a session cannot be opened, saying that no
agent is available to open a session, with nothing but its acknowledgement, which closes the dialog (see "The failure
modal" in `docs/product/error-alerts.md`).

## When the agent or the account cannot open a session

**The daemon refuses to open a session of an agent that is not added.** Whatever opens it — the dialog, a session opened
with defaults, `open_session`, a console or project session's `new_session` (with `resume_agent_session_id` too), or an
approved session request — a new session whose agent, once resolved (see "Which agent a session uses" below), is not on
the list of added agents is refused with `agent_not_added`, and nothing is recorded. The refusal names the agent and
says it is not added, pointing to Settings → Agent accounts; in the window it is shown in an error alert offering **Add
agent…** (see "Recovery and unexpected errors" in `docs/product/error-alerts.md`). A session request is refused that way
before the user is asked, and again on approval should the agent have been removed meanwhile (see
`docs/product/session-requests.md`). Handing a session off to an agent that is not added is refused the same way (see
"What can be handed off" in `docs/product/handing-off-sessions.md`). While the list is undecided (see "Added agents" in
`docs/product/agent-accounts.md`) nothing is refused for it; a list the user has emptied refuses every agent. An agent
that has been determined unavailable is refused as unavailable (`agent_not_available`) rather than as not added. Only a
session being opened is checked: a session of an agent removed since keeps running, and resuming, reopening or
restarting it is never refused for this.

**The daemon refuses to open a session on a turned-off account** (see "Turning an account off" in
`docs/product/agent-accounts.md`). Whatever opens it — the same openings as for a used-up account below — a new session
whose account, once resolved (see "Which account a session uses" below), named or inherited, is turned off is refused
with `account_disabled`, and nothing is recorded; the user's own request is refused too. It is checked before the quota,
so a turned-off account is refused as turned off whatever its quota says. The refusal carries `agent` and `account` (the
account's name; the default account under the name the user gave it, else `Default`). In the window it is shown in an
error alert, in the UI's language, saying the account is turned off, so a session cannot be opened on it; the next step
is to choose another account, or to turn this one back on, and the alert has a **Manage accounts…** button (see
"Recovery and unexpected errors" in `docs/product/error-alerts.md`). An agent's tool call is refused with a reason in
English naming the account and the agent and saying to choose another account or turn it back on in Settings. A session
request is refused that way before the user is asked, and again on approval should the account have been turned off by
then (see `docs/product/session-requests.md`). Only a session being opened is checked: a session already on the account
keeps running, and resuming, reopening or restarting it is never refused for this.

**The daemon refuses to open a session on a used-up account.** Whatever opens it — the dialog, a session opened with
defaults, `open_session`, a console or project session's `new_session` (with `resume_agent_session_id` too), a handoff's
receiving session, or an approved session request — a new session whose account, once resolved (see "Which account a
session uses" below), is used up by the reading the daemon holds at that moment (see "A used-up account" in
`docs/product/account-quotas.md`) is refused with `account_quota_exhausted`, and nothing is recorded; the user's own
request is refused too. The refusal carries `agent`, `account` (the account's name), `window` (the used-up window's name
in English, the same for every agent: `5-hour`, `Weekly`, `Monthly` or `Credits`, or a window's own label when it
carries one), `window_kind` (session, weekly or credit period), `window_minutes` (its length, when known),
`model_weekly` (when the same reading also has a model's own weekly window) and, when the agent gives a reset time,
`resets_at` in milliseconds since the epoch. In the window it is shown in an error alert, in the UI's language, saying
the account cannot run a session because it has reached that limit, the window named as everywhere else in the UI (see
"Quota windows" in `docs/product/account-quotas.md`): Claude Code's "session limit", Codex's "5h limit", Claude Code's
account-wide weekly window as "weekly limit for all models" when the reading also has a model's own weekly window, and
so on; a refusal that does not say what the window is names it as a limit of its `window` name. The alert also says,
when known, how long until it resets, counted from when the alert is shown; the next step is to choose another account
that has not reached its limit, or to try again once the usage limit resets. An agent's tool call is refused with a
reason in English naming the account, the agent and the window (the five-hour window as "session" for Claude Code and
"5h" for Codex, Claude Code's account-wide weekly window as "Weekly · all models" when the reading also has a model's
own weekly window, any other by its `window` name), about how long until it resets when known, and saying to choose
another account or wait for the reset. A session request is refused that way before the user is asked, and again on
approval should the account be used up by then (see `docs/product/session-requests.md`); an owner's handoff the user is
asked to approve is refused for it only on approval, since its form lets the user choose another account (see "The
user's approval of an owner's handoff" in `docs/product/handing-off-sessions.md`). Only a session being opened is
checked: a session already on the account keeps running, and resuming, reopening or restarting it is never refused for
this.

With every agent determined unavailable, a console session's `new_session` tool is refused as well, with a reason it can
report that says the same (see "The console session's tools" in `docs/product/hub-orchestration.md`). Nothing is refused
for an agent being unavailable while availability is still being determined: a session then opens or fails exactly as
described above.

## Opening a session with defaults

While the platform's primary modifier is held — ⌘ on macOS, Ctrl elsewhere, each without the other — every sidebar **+**
that opens the session dialog turns into its quick form: it shows a bolt in place of the plus, and its tooltip and
accessible name say it opens the session **with defaults**. Pressing it then opens the session at once, without the
dialog. These are:

- the console header's **+**, New console session (see "The console header" in `docs/product/sidebar.md`);
- each project row's **+** (see "Project rows" in `docs/product/sidebar-project-rows.md`);
- the header's **+** in a project's focus mode (see "A project's focus mode" in `docs/product/focus-mode.md`);
- in a console session's focus mode, each project's own **+** and the header's **+** menu of projects, where the project
  chosen is opened with defaults (see "A console session's focus mode" in `docs/product/focus-mode.md`).

The empty rows' and the terminal's **New session** and **New console session** offers have no quick form; they always
open the dialog.

A **+** button pressed with the modifier opens with defaults even when the hold had not been noticed before the press. A
project chosen from the console session focus mode's menu opened with the pointer follows whether the modifier is held
at the moment it is chosen. Keyboard creation in focus mode follows "Creating sessions from the keyboard" in
`docs/product/focus-mode.md`. The hold ends when the key is released, when the window loses focus and when the page is
hidden; a modifier already down while the window was in the background is picked up once the pointer moves or a key is
pressed.

The session opened is the one the dialog would open if submitted untouched:

- the localized default name (see **Name** above);
- the agent and account the dialog opens on (see "Which agent a session uses" and "Which account a session uses" below),
  sent as the session's own choice;
- no model or reasoning effort of its own, so both follow the parent;
- a new conversation, never a resumed one;
- **unbound** — from the project list too, where the dialog's **Report to** starts on no console session — except in a
  console session's focus mode, where it is bound to that console session, as the dialog opened there binds it.

It is listed as *starting* and selected, as a session opened from the dialog is, and a launch failure is reported the
same way (see above). A refusal is shown in an error alert titled "Could not open a session". When the console session
it would be bound to is no longer available — archived or gone — nothing is sent, and an error alert titled "Cannot open
a session" says so, with the same message the dialog shows in that case, then the remedy: if that session was archived,
choose **View archive**, then **Reopen** on it; otherwise start a session with **New session**. It is a known failure
(`quick_create_owner_gone`) and asks for no report. The other cases below in which nothing is sent are titled "Cannot
open a session" too. Only one session opens with defaults at a time: further presses are ignored until the daemon has
answered the previous one. If the sidebar has switched to another console by the time the answer arrives, the session is
still opened but not selected.

When no agent is both added and installed, no **+** shows its quick form, and every one of them is disabled (see "When
no agent can open a session" above).

When the account the dialog would open on is used up, turned off, or having its sharing of the default configuration
changed, the session opened with defaults takes another account of the same agent, as the dialog does (see **Agent**
above). When no account of the agent is left that is none of these, and the one it would open on is turned off, nothing
is sent and an error alert says the account is turned off, as the daemon's `account_disabled` refusal does (see "When
the agent or the account cannot open a session" above); when the one it would open on is having its sharing changed
instead, nothing is sent and an error alert says the account's sharing is being changed, as the daemon's
`account_sharing_in_progress` refusal does. When the account it opens on has an incomplete last change of sharing, the
user is asked first, and cancelling opens nothing (see "Starting a session on an account whose last change is
incomplete" in `docs/product/sharing-the-default-configuration.md`). When no account of that agent can open a session,
each being used up or turned off, there is nothing to open with defaults:

- the **+** in its quick form looks disabled but stays focusable and under the pointer; its accessible name stays its
  action, while its tooltip, and its description for assistive technology, say in short that the default agent's
  accounts are unavailable, without naming the agent;
- pressing it — or pressing a **+** with the modifier before the hold was noticed — opens nothing and shows an error
  alert: when the account it would open on is turned off, the alert for a turned-off account above; otherwise one that
  names the button's action and says it is unavailable because none of the default agent's accounts can open a session
  right now, each having reached its usage limit or being turned off, with the advice to choose another agent in the new
  session dialog, or to try again once a usage limit resets or an account is turned back on;
- in the **+** menu of projects in a console session's focus mode, held in its quick form, each such project is
  disabled, with a short note under its name saying the default agent's accounts are unavailable; it stays reachable
  with the arrow keys, and its accessible name ends with that note.

Without the modifier, the same **+** opens the dialog as usual.

## Which agent a session uses

In descending priority:

1. the agent chosen for this session when it was opened;
2. the project's default agent;
3. the console's default agent for project sessions;
4. the console's default agent for console sessions.

A console session has no project, so it uses the agent chosen for that session, or the console's default agent for
console sessions when none is given; the default for project sessions never applies to it. Both console defaults are in
`docs/product/consoles.md`. Console sessions of different agents can run alongside each other in the same console, and
all read the same instruction file, described in "The console session's instruction file" in
`docs/product/launching-agents.md`.

The same order settles the agent everywhere a session's agent is not given outright: the session dialog's initial
choice, a session opened with defaults, and a session of a project associated with "Detect from files" whose directory
named no agent (see "The project's default agent" in `docs/product/projects.md`). A session that a project session
starts itself without naming an agent — with `new_session`, or with `request_session` in the caller's own console — runs
the caller's own agent instead, and so does a console session's request for a console session in its own console; either
falls back to this order only when that agent cannot open a session. A session a console session starts in a project,
and a session request for another console, follow this order (see "The agent and account of a session a session starts"
in `docs/product/hub-orchestration.md`).

A session's agent is fixed for its lifetime. Resuming or reopening a session always relaunches the same agent — session
records belong to a specific agent and cannot be moved across agents.

## Which account a session uses

The account of the session's agent, in descending priority:

1. the account chosen for this session when it was opened;
2. the project's account for that agent, where the project does not leave it to its console — a project set to **the
   agent's default account** ends the chain there, whatever the console names;
3. the console's account for that agent;
4. that agent's default account.

A console session carries no project, so after its own account choice its chain goes straight to the console's
reference. A project leaves the account to the console for every agent it inherits for, which is how every project
starts (see "The project's account, model and reasoning effort" in `docs/product/projects.md`). While **Automatically
select an account** is off (see that section in `docs/product/settings.md`), a session opened without changing the
account picker runs under the project's account for its agent, or the console's, or the agent's default account when
neither pins anything. The session's account is named in its agent icon's tooltip and its accessible session name (see
"Session rows" in `docs/product/sidebar-session-rows.md`), and on the line beside the agent icon of its focus mode card
(see "What both views show" in `docs/product/focus-mode.md`) and of its row in the archive view (see "The archive view"
in `docs/product/archive-view.md`); wherever the agent's name is shown, it carries no account suffix.

While the switch is on, a new session whose account was not named uses the account of that session's agent whose weekly
quota resets soonest, among the accounts that can still open a session. An account the session names still wins,
including an explicit choice of the agent's default account, which does not fall through to a project pin. On
`open_session`, `null` is that explicit choice of the default account, not the absence of a choice. Where an agent's
tool names an account, `default` is the same choice; leaving `account` out, or sending it blank, is not.

Who makes the pick:

- The new-session dialog computes it, shows it in the account picker, and sends it as the session's own choice. The user
  can still pick another account. Until the user does, the account shown follows the dialog's clock (see "A used-up
  account" in `docs/product/account-quotas.md`). The handoff dialog does the same for the receiving session (see
  "Handing off from the window" in `docs/product/handing-off-sessions.md`).
- Opening a session with defaults uses the same account the dialog would open on, and sends it the same way (see
  "Opening a session with defaults" above).
- `new_session` and `request_session`, when `account` is left out or blank, have the daemon make the same pick, in the
  caller's own console and in another console (see "The agent and account of a session a session starts" in
  `docs/product/hub-orchestration.md`).
- The **Resume conversation** tab does not follow the pick. It sends the agent and account that hold the conversation,
  shown locked, when that account can still be selected (see
  `docs/product/resuming-a-conversation-by-its-session-id.md`).
- `handoff_session`, when `account` is left out or blank, has the daemon make the same pick for the receiving session
  (see "Handing off from an owner's tool" in `docs/product/handing-off-sessions.md`).

When the switch is on but no account ranks, the account is the one it would be while the switch is off. The new-session
dialog, the handoff dialog and a session opened with defaults open on the account the project or console resolves to,
then another account of that agent when that one is used up or turned off. For `new_session` and `request_session` the
fallback is in "The agent and account of a session a session starts" in `docs/product/hub-orchestration.md`: in the
caller's own console, the caller's own account only when the session is of the caller's own agent and that account can
still open a session, otherwise the project's for the agent, else the console's; in another console, the project's, else
the console's, and never the caller's. `handoff_session` falls back as `new_session` does in the caller's own console, a
session the caller owns always being in it.

The dialog and the daemon rank the same way:

- Candidates are that agent's accounts that can still open a session. An account is not a candidate when it is used up
  (see "A used-up account" in `docs/product/account-quotas.md`): a five-hour, account-wide weekly, or credit-pool window
  at 100% or more, whose reset is still ahead or absent, drops the whole account. A model-specific weekly limit never
  does that, and it does not drive this pick either. Nor is an account a candidate while it is turned off (see "Turning
  an account off" in `docs/product/agent-accounts.md`), whatever its quota.
- The default account is a candidate ahead of that agent's stored accounts, in the order they are kept.
- The window that ranks is an account-wide weekly window, or a credit pool whose period is about a week (6 to 8 days,
  the ends included), the window "Quota windows" in `docs/product/account-quotas.md` calls **Weekly**. A five-hour
  window, a model-specific weekly window, and a monthly credit pool do not rank. How much of the five-hour window
  remains is not a key.
- The share used must be a finite number below 100, and the reset must be strictly in the future. A reset that has
  passed or is missing does not rank.
- On one account, the soonest such reset wins, then the larger remainder.
- Across accounts, the soonest reset wins, so quota about to reset is spent first; a larger remainder breaks a tie; an
  equal tie keeps the earlier candidate. One percent left that resets in an hour is chosen over ninety percent that
  resets in three days.
- An account used up on its five-hour window is not chosen even when its weekly window resets soon.
- The reading used is the one held for that account under the directory it points at now. A reading taken under another
  directory does not count (see "Which accounts have a quota" in `docs/product/account-quotas.md`).

## Which model and reasoning effort a session uses

A session uses its own model and reasoning effort choices, each independently, or inherits the current defaults for its
agent: a project session from its project, else its console's project session pair; a console session from its console's
console session pair. The inheritance, explicit clearing and persistence rules are in
`docs/product/model-and-reasoning-effort.md`. How the resolved values reach the agent is in "The model and reasoning
effort" in `docs/product/launching-agents.md`.
