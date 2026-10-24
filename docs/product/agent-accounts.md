# Agent accounts

Settings' **Agent accounts** section (see "Agent accounts" in `docs/product/settings.md`): the agents the user has
added, and every account of each, grouped by agent. The section opens with a paragraph saying what an account is — a
named config directory of an agent, where it keeps its login and conversation history — and that an agent has to be on
the user's `PATH` for its accounts to be usable. Below that paragraph sit **Automatically select an account** and,
directly under it, **Start usage windows automatically** (see those sections in `docs/product/settings.md`), switches
for the whole application, not controls on one agent's tab. They stay there while agents are listed, while the
added-agent list is still undecided, and while no agent is added. Below the switches sits the agent strip, which ends
in the **Add agent** and **Add account** buttons, with the selected agent's accounts under it. What an account is and how consoles, projects and sessions use
one are in "Agent config directories" in `docs/product/consoles.md`; signing an account in from this section is in
`docs/product/logging-accounts-in.md`.

## Added agents

Octoboard keeps a list of the agents the user has **added**, in an order the user sets. It is kept by the daemon with
the other application settings, so every window shows the same list, and it is not part of a configuration export (see
"What is not configuration and never travels" in `docs/product/configuration-export-and-import.md`).

The list decides which agents this section shows, and in what order: the agent strip and the Add account form's agent
picker. Every other place that lists agents follows it too, in its order: the console, project, session and handoff
dialogs' agent pickers and the console dialog's per-agent sections (see "Which agents the agent pickers list" in
`docs/product/consoles.md`), the rail's account quota panel (see "What the panel shows" in
`docs/product/account-quotas.md`) and the menu bar icon's quota submenus (see "The icon's menu" in
`docs/product/menu-bar-icon.md`); so does whether a new session of an agent can be opened, or a session handed off to
it, at all — from the window or by an agent's tool (see "Opening a session" in `docs/product/sessions.md`). Only this section's Add agent dialog and the welcome screen's supported CLI
tools list (see "What the welcome screen shows" in `docs/product/welcome-screen.md`) list an agent that was never
added.

**How the list starts.** The list is decided once, as soon as Octoboard has determined which agents are installed: it
is filled with every agent that is installed plus every agent the user owns an account of, in the order Claude Code,
Codex, Grok Build. This holds for a fresh installation, which owns no accounts and so starts with the installed agents,
and for an installation from before the list existed, so no account it had disappears from view. From then on only the
user changes it; a list the user has emptied stays empty.

**While the list is undecided** — before Octoboard has determined which agents are installed, and for the whole run
when the user's login shell environment could not be read (until Octoboard next starts) — the section lists every
agent, in the order Claude Code, Codex, Grok Build. The agents then have no menu and no actions button and cannot be
dragged, and the Add agent list adds nothing; the Add agent and Add account buttons are still there. Meanwhile every other place that follows the list treats
every agent as added, in that same order.

## The agent strip

Each added agent has a tab of its own, in the list's order, each tab showing the agent's icon and name and as wide as
its content, the tabs starting at the strip's start; only the selected agent's accounts are shown, in its tab's panel.
The agent tabs are an underlined strip, set apart from the list of Settings' sections, and scroll sideways when they do
not fit. Once the section is scrolled far enough for the
strip to reach the top of the section's area, at any window width, the strip stays pinned there, opaque, and the
accounts scroll beneath it; the top fade then begins below the strip (see "The edge that hides content fades" in
`docs/product/scrolling-surfaces.md`). The section always opens on the first agent's tab, however it was opened; when
the agent in view is removed, the first agent's tab is shown.

Each tab carries, at its end, its own small actions button (⋮), named "Actions for" and the agent, which opens the
agent's menu (see "Reordering and removing an agent" below). It is shown while its tab is wholly in view in the strip,
and while it holds keyboard focus. After the strip's last tab, at the strip's end, stand two icon-only buttons,
**Add agent** and **Add account**, each named by its tooltip (see "Adding an agent" and "Adding, editing and removing
an account" below). While a tab is dragged, and while the tabs move into their new places after a drag or a reorder,
every tab's actions button fades out, except the one holding keyboard focus, which goes with its tab; they fade back
in at their new places once the tabs have settled. A press on a tab that does not move the pointer fades nothing.

An agent that is not added has no tab, installed or not. An added agent that is not available has a warning icon after
its name in its tab, and each account the user owns of that agent says, under its directory in its account settings
dialog, that it is not usable until the agent is on `PATH`. The tab carries a tooltip, opened while the pointer is on
the tab and while the tab has keyboard focus, that says the agent is not installed and that its accounts are not usable
until it is on `PATH`; the tab's accessible name is the agent's name marked as not installed. How that tooltip opens and
closes is in "Tooltips on indicators" in `docs/product/labels-and-tooltips.md`. An agent whose availability is not yet
determined has no icon, no tooltip and no such line, and while the list is undecided no tab has the icon or the tooltip.

## Adding an agent

**Add agent** opens a dialog with a line saying that only an agent installed on this machine can be added, and a list
of every agent, in the order Claude Code, Codex, Grok Build, each with its icon and name, followed by a separate
status badge in every state:

| Status | What the row offers |
|---|---|
| **Added** | Nothing. An added agent shows as added even when it is not installed. |
| **Installed** | An **Add** button, which puts the agent at the end of the list. |
| **Not installed** | A **How to install** link to the agent's own installation instructions, published by its vendor; Octoboard writes none of its own. |
| **Checking…** (not yet determined) | The Add button, unavailable until the agent is found installed. |

While the list is undecided every Add button is unavailable. The dialog stays open after an agent is added, its row
turning to Added, so several can be added in turn; it closes on its **Close** button.

The dialog also opens by itself, in front of Settings, when Settings is opened or navigated from one of the window's
**Add agent…** buttons (see "Opening and closing Settings" in `docs/product/settings.md`): Settings shows this section
and the dialog opens over it, holding keyboard focus. It opens once per such request; switching to another section of
Settings and back does not open it again. When the user closes a dialog opened this way, keyboard focus
goes to the **Add agent** button at the end of the agent strip, when the strip is there. A later request for Settings to show a section without this dialog
closes the dialog, however it was opened, and focus goes to that section's tab. A refused addition — the agent is not
installed, is already added, or which agents are installed is not determined yet — is shown in an error alert (see
`docs/product/error-alerts.md`).

## Reordering and removing an agent

Each added agent has a menu, opened by a right-click on its tab, by the Menu key or Shift+F10 while its tab has
keyboard focus, or from the actions button at the end of its tab (see "The agent strip" above), so no tab need be
selected first. The menu holds:

- **Move earlier** and **Move later**, which move the agent one place along the list. Earlier and later are in the
  reading direction, so they hold under a right-to-left language too. Move earlier is unavailable for the first agent,
  and Move later for the last;
- **Remove agent**, set apart from the moves.

A tab can also be **dragged** to a new place with a mouse or a pen, when there is more than one agent: the tab follows
the pointer, the tabs it passes make room for it, and releasing it drops it there. Escape, or a cancelled pointer,
ends the drag with nothing moved. A touch on the strip scrolls it instead.

A move or a drop shows the new order at once, and keyboard focus stays where it was: on the moved tab, or on its
actions button when the move was chosen from that button's menu. If the move is refused, the order
goes back and the refusal is shown in an error alert.

**Remove agent** asks first. The confirmation says that the agent is taken off the list of added agents and that its
accounts are kept and come back if it is added again. When the agent is still in use, a sentence follows saying that
what is listed still uses the agent and keeps that setting, then a list with one item per kind of user, leaving out a
kind with none: the consoles whose Default agent fields name it, the projects whose Default agent is it, and the
sessions of it that are not archived. Each item gives the count of its kind and says how that kind uses the agent: the
consoles and projects items say that the agent is their default agent ("2 projects have it as their default agent"),
and the sessions item says that the sessions use it ("3 sessions use it"). While there are at most five, the item is
followed by a colon and their names, each set apart as a chip ("1 console has it as its default agent: Main", "1 session
uses it: Fix build"), and beyond five only the item without names is given. Its confirm button, **Remove**, is styled as destructive. Once the agent is
removed its tab goes, keyboard focus goes back to the selected section's tab in Settings' list of sections, and the
accounts of the agent stay stored untouched. The consoles, projects and sessions that use it keep their setting, and its sessions go on running.

## With no agent added

When the user has removed every agent, the strip is replaced by an empty state titled "No agent added", with a line
saying to add an agent installed on this machine to manage its accounts here, and an **Add agent** button. There is
no Add account button. The two switches, **Automatically select an account** and **Start usage windows automatically**, stay above
it.

## The default account

Each agent's accounts table starts with the agent's **default account**; its account settings dialog shows the directory
it currently resolves to. A "?" follows that directory there and says in its popover (see "The help tip" in
`docs/product/labels-and-tooltips.md`) that this is the agent's own default configuration directory, which new
sessions use when no account is chosen and whose configuration other accounts can share. Its directory cannot be edited,
and the default account cannot be removed, but it can be turned off like any other account (see "Turning an account off"
below). It has no sharing switch: its directory is the one other accounts share. Its actions menu holds **Settings**,
**Rename** and **Log in**; where else **Log in** appears is in "Where Log in appears" in
`docs/product/logging-accounts-in.md`.

**Rename** opens a dialog, titled for the default account of that agent, with a single name field holding the name the
user gave it, empty when there is none, and a note that leaving it empty uses the standard name. The name is stored
trimmed. A blank name, or the word "Default" in any letter case, clears it, and the default account is again shown under
its name in the current language. The name cannot be one an account the user owns of the same agent already has,
compared trimmed and ignoring letter case; that is reported under the name field. The name is kept by the daemon and is
not part of a configuration export (see "What is not configuration and never travels" in
`docs/product/configuration-export-and-import.md`).

The name the user gave is what the default account is called everywhere it is named: in this section, the rail's
account quota panel, the console, project and session dialogs' account pickers (see "Agent config directories" in
`docs/product/consoles.md` for how the console dialog names it), a session row's Switch account submenu,
the agent icon's tooltip and the accessible name of a session in the sidebar, the archive view and confirmations, a
focus mode card's model line, agent icon tooltip and accessible name, an archive view row's model line, the agent
icon's tooltip and the accessible name of focus mode's archived rows, the menu bar icon's quota submenus, and what a console session's settings tools answer (see "Application
settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md` and
`docs/product/switching-session-accounts.md`).

## The accounts the user owns

The accounts the user owns follow the default account, each with its name, its directory and an actions menu holding
**Settings**, **Edit** and **Log in** and, set apart, **Remove**; where else **Log in** appears is in "Where Log in
appears" in `docs/product/logging-accounts-in.md`. Both kinds of directory are shown as in "How paths are shown" in
`docs/product/settings.md`.

## The accounts table

Each agent's tab panel holds, for Claude Code and Codex, the agent's **Permission approval** row (see "Approval modes"
below), then one table of the agent's accounts, named for the agent ("Claude Code accounts"): a row per account, the
default account first, then the accounts the user owns in the order they are kept. Its columns:

| Column | What a row shows |
|---|---|
| **Account** | The account's name and, when the agent names one, its plan as a badge. A name too long for its space fades as in "Names too long for their space" in `docs/product/labels-and-tooltips.md`. |
| **Status** | The account's login state (below) and, for an account known to be signed out, the **Log in** icon button (see `docs/product/logging-accounts-in.md`). |
| **Usage** | The account's compact usage bar, the same as an account picker's, with no numbers: the five-hour window with the account-wide weekly window's thinner bar beneath it, or one bar for the account-wide weekly window or the credit pool (see "In an account picker" in `docs/product/account-quotas.md`); assistive technology reads what remains of those windows in words. While the first reading is on its way it says the quota is being read. With no current reading it holds a dash, followed by the "Quota unavailable" mark when the reading failed with nothing read before, or by the out-of-date mark when the windows held are the last good ones (see "Quota states" in `docs/product/account-quotas.md`); otherwise assistive technology reads the dash as "No usage reading". Model-specific windows are left to the account settings dialog. |
| **Disable** | The account's Disable switch (see "Turning an account off" below). The column's heading carries a "?" help tip. |
| No heading | The account's actions menu button (see "The default account" and "The accounts the user owns" above). |

**Login state.** A badge in the Status column says whether the account is logged in, in words and with a dot, so the
state is not carried by colour alone: **Logged in**, in the success colour, once a reading of its quota has been taken
— the latest reading succeeded, or the windows of an earlier one are still held after a newer reading failed; **Not
logged in**, in the warning colour, while its quota is in the not-logged-in state (see "Quota states" in
`docs/product/account-quotas.md`). While neither is known — the quota not read yet, or a reading that failed with
nothing read before — there is nothing.

**In a narrow window** the table keeps its columns' widths: once the section is narrower than they need, the table
scrolls sideways, and while it does it is a tab stop of its own, named for the agent's accounts as scrollable (see
"Scrolling a surface from the keyboard" in `docs/product/scrolling-surfaces.md`). The actions column stays pinned at
the table's end edge meanwhile, with a hairline at its start, and a control that keyboard focus moves to is scrolled
clear of it. The table draws no fade at its sideways edges.

Opening the section asks for a fresh reading of the quotas, covering the accounts of every agent and not only of the
tab in view; the readings already held are shown at once.

## The account settings dialog

**Settings**, the first entry of an account's actions menu, with a gear icon, opens a modal dialog of everything about
that one account.
Its heading is the account's name, followed by its plan badge, its login state — the badge as in the table, or, while
that is not known, "Checking…" while the first reading is on its way and "Unknown" otherwise — the "Quota unavailable"
mark when its reading failed with nothing read before, and the **Log in** icon button for an account known to be signed
out. Under the heading stands the account's directory; for the default account a "?" help tip follows it (see "The
default account" above). For an account the user owns of an agent that is not available, the dialog's body opens with
a line saying it is not usable until the agent is on `PATH`. Then, one under another:

- **Usage** — the account's quota drawn as in the rail's account quota panel: the five-hour and account-wide weekly
  windows as one meter, every other window, model-specific ones included, as a meter of its own, one to a row. How
  long ago the windows were read, with the out-of-date mark when it applies, ends the heading. It says the quota is
  being read while the first reading is on its way, says for an account that is not logged in that it is not logged
  in yet and to log in to use it, and holds a dash when there is no reading to show otherwise. How windows are named
  and drawn is in "How an account's quota is shown" in `docs/product/account-quotas.md`.
- The account's settings, as rows like Settings' own, a name with a line under it and the control at the row's end;
  each row's line says what its switch does in the position it is in, and changes with it: **Share default
  configuration**, for an account the user owns (see "Sharing the default configuration" below), then, last,
  **Disable account** (see "Turning an account off" below).

The dialog has no footer: it closes on its close button, on Escape and on a press outside it, and keyboard focus then
returns to the account's actions menu button in the table. It follows the account as it changes, from any window, and
closes by itself when the account is removed.

## Turning an account off

Every account, the default account included, has a **Disable** switch, off to begin with, in the table's Disable column
and, as the **Disable account** row, last in its account settings dialog. Turning the switch on turns the account off.
Its accessible name is "Disable" (or "Disable account") and the account. The column heading's "?" help tip says,
whatever the switches' positions, what being off means: the account is not shown in the account choices and new sessions
will not use it, while running sessions are unaffected. The **Disable account** row's line follows its switch: while it
is off, that once it is on the account is left out of account choices and new sessions won't use it; while it is on,
that the account is turned off, with the same consequences; both add that running sessions are unaffected. The account
stays out of use for new work until the user turns the switch off again: nothing turns an account back on by itself,
after a time or otherwise.

While an account is turned off:

- **No session is opened on it, handed off to it or switched onto it**, by anyone, the user included: the daemon
  refuses with `account_disabled` (see "Opening a session" in `docs/product/sessions.md`, "What can be handed off" in
  `docs/product/handing-off-sessions.md` and "When a switch fails" in `docs/product/switching-session-accounts.md`).
- **Automatic account selection passes over it** (see "Which account a session uses" in `docs/product/sessions.md`),
  and so does a session started by another session where it would otherwise run on its caller's own account (see "The
  agent and account of a session a session starts" in `docs/product/hub-orchestration.md`).
- **The window's account choices leave it out**: the session and handoff dialogs' account pickers do not list it and
  open on another account (see the **Agent** field in "Opening a session" in `docs/product/sessions.md`), a session
  opened with defaults takes another account, or is unavailable when none is left (see "Opening a session with
  defaults" in `docs/product/sessions.md`), and a session row's **Switch account** submenu does not list it (see
  "Session rows" in `docs/product/sidebar-session-rows.md`). Where a choice already holds it, it stays listed:
  - in the session and handoff dialogs, when the account the field ends on is turned off, it is listed, marked
    **Disabled**, but cannot be picked, and the dialog cannot be submitted on it;
  - in the Switch account submenu, a session's own account stays listed, checked, whether or not it is turned off;
  - in the console and project dialogs' account pickers, the account the console or project named when the dialog
    opened, and the one picked in it now, stay listed, marked **Disabled**, and can still be picked, since a console's
    or a project's account is a lasting preference (see "Agent config directories" in `docs/product/consoles.md` and
    "The project's account, model and reasoning effort" in `docs/product/projects.md`). A console or project whose
    account it is keeps it; only opening a session on it is refused.
- **No usage window is started on it automatically** (see "When an account is kicked" in
  `docs/product/account-quotas.md`).

The sessions already running on it are not affected, and resuming, restarting or reopening a session on it is never
refused for this. Its quota goes on being read, and the rail's account quota panel and the menu bar icon's quota
submenus list it as usual. Its row in the accounts table looks like any other, but for its switch.

**The switch** saves at once, on its own, and shows the daemon's recorded choice, which every window shares, moving
when it changes. A refusal is shown in an error alert, the switch keeping its position; a press while the previous one
is still unanswered is ignored. A client turns an account off or on with the protocol request `set_account_disabled`.
Turning an account back on while **Start usage windows automatically** is on starts at once any window its held reading
shows has not begun counting, as turning that setting on does.

Whether an account is turned off is kept by the daemon on this machine and is not part of a configuration export (see
"What is not configuration and never travels" in `docs/product/configuration-export-and-import.md`). An agent can read
it — `get_settings` marks each account that is turned off — but no tool turns an account off or on (see "Application
settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md`).

## Sharing the default configuration

Every account the user owns has a **Share default configuration** row in its account settings dialog, above **Disable
account** (see "The account settings dialog" above): the name with a "?" help tip, a line under it, and the switch at
the row's end. The line follows the switch: while it is off, "Turning this on links the account's configuration and
conversation history to the default account's directory."; while it is on, "The account's configuration and conversation
history are linked to the default account's directory." The default account has none. The switch's accessible name
identifies the account. A change is carried out at once or not at all (see "When a change is carried out" in
`docs/product/sharing-the-default-configuration.md`), so the switch is either free to use or unavailable, and says
which. Unavailable, it looks disabled but stays focusable and is marked unavailable for assistive technology; a press on
it is ignored, and what is shown under the row's line says why.

A help tip beside the row's name says, one line each, that configuration (skills, plugins, settings and more) and
conversation history are linked to the default account's directory, what is never shared for the selected agent, and
what happens to conversations when sharing is turned off; and, while the account has idle sessions, that changing it
first ends that many idle sessions of the account and resumes them afterwards. The rules are in "What is shared" and "Turning sharing off" in
`docs/product/sharing-the-default-configuration.md`.

The switch saves independently of the account's name or directory. It shows the daemon's recorded choice, which changes
as soon as a change starts, so during a change it shows the change's target. While the last change is incomplete — it
failed, or the daemon stopped during it — and no change runs, the switch instead shows the position it had before that
change, the opposite of the recorded choice: turning it asks again for the recorded choice, which the daemon carries out
as the retry (see "The sharing choice" in `docs/product/sharing-the-default-configuration.md`). From the switch, an
incomplete change can therefore only be tried again in the same direction; it cannot be turned back until a retry
succeeds. The one exception is turning sharing on that failed because the default account's directory has moved since
sharing was turned on (the cause `default_moved`), which asking again cannot mend: the switch then shows the recorded
choice, on, and turning it off, then on again, shares the new directory. Every window shows the same state, from the
account's sharing state (see "The account's sharing state" in `docs/product/sharing-the-default-configuration.md`). What
stands under the row's line is the switch's description for assistive technology:

| State | The switch | Under it |
|---|---|---|
| Free, off or on, with or without idle sessions of the account | Can be turned | Nothing beyond the row's line, which says what turning it on does or that the account shares; the idle sessions a change would end are in the help tip. |
| Sessions of the account working | Unavailable | That sharing cannot be changed while that many of the account's sessions are working, and to wait for them to finish. Which sessions count as working is in "When a change is carried out" in `docs/product/sharing-the-default-configuration.md`. |
| The account being signed in | Unavailable | That sharing cannot be changed while the account is being signed in. With sessions running too, both lines are shown. |
| A change running | Unavailable | A progress bar (below). |
| The last change incomplete | Its position before that change (on, after a moved default directory) | An amber alert (below). |

Text typed into an idle session's terminal but not yet sent is not seen, and is lost when the change ends that session
(see "When a change is carried out" in `docs/product/sharing-the-default-configuration.md`); the switch does not warn
of it.

A reason the account is in use stays under the row's line alongside a progress bar or the alert. The progress bar and
the alert stand under the row, across its width. The switch is also
unavailable while this window's own request for a change is still unanswered, and shows the recorded choice meanwhile.

**While a change runs**, a progress bar stands under the row, labelled with the direction, turning sharing on or
off, and following the change's phases (see "When a change is carried out" in
`docs/product/sharing-the-default-configuration.md`). While it waits for another account's sharing change to finish,
while it ends the idle sessions, and while it resumes them, the bar is indeterminate and says which in place of a
count. While the entries change, it shows how many have been processed out of the total, as its value too, with the
entry being processed named under it as a code chip; until the first entry, while a background quota or model check
of the account finishes, it is indeterminate and says it is waiting to start. The direction is announced when the
change starts, together with the phase when that is not the entries, and each later phase is announced as it begins,
except entering the entries phase, which keeps what was last said.

**When the change ends** the result is shown at the switch at once. A change that completed leaves nothing under the
row. One that did not complete — whether it failed just now or the daemon was stopped during it — is shown under
the row as one compact amber alert (see "Failures shown on the screen" in `docs/product/error-alerts.md`):

- a title saying that turning sharing on (or off) could not finish, or, after a daemon restart, when nothing is known
  of the failure, that the last attempt to turn it on (or off) did not finish;
- for a failure, one sentence saying that some files or folders could not be changed. The title and this sentence are
  the only part of the alert announced to assistive technology, and only when the failure follows a change this view
  saw running, not for one already there when the section opens;
- then what to do: turn the switch on (or off) again to try once more, or, after a moved default directory, turn it
  off, then on again. The alert has no retry button of its own;
- a **Details** button, present only when there is something behind it, opening a popover that holds, in order: why,
  in plain language, when that is known — one line per cause, naming the entries it hit compactly, the first by name
  and the rest as a count ("You do not have permission to change `skills/` and 1 more."), a conversation by the name of
  a session that records it, each line going on with what to do about its cause; or, when no entry is listed, the
  reason the change as a whole failed with what to do about it, when it is a named one (see "When a change is carried
  out" in `docs/product/sharing-the-default-configuration.md`); then, with idle sessions of the account, the line
  saying that changing sharing ends and resumes them, since a retry does too; then each entry with what it was left
  as and the raw diagnostic, or, with no entry listed, the one raw reason the change failed.

The causes and what each covers are in "When a change is carried out" in
`docs/product/sharing-the-default-configuration.md`. The alert stays while the account's last change is incomplete
and no change runs; the failure's details are dropped once the next change records its choice, so a retry refused
or stopped before that keeps them. When the alert goes while keyboard focus is inside it, focus moves to the switch. A failure is shown only there, not in an error alert, the
window that asked for the change included. A refused request (an account in use, a change already running) is shown
in an error alert, the switch keeping its position.

While the account's last change is incomplete and no change runs, starting a session on it from the window asks the
user first (see "Starting a session on an account whose last change is incomplete" in
`docs/product/sharing-the-default-configuration.md`).

## Approval modes

How Claude Code and Codex handle permission requests is **one setting per agent**, applied to every account of that
agent, the default account included. It is set in the agent's tab, in the **Permission approval** row above the
accounts table: the row's name with a "?" help tip, a line saying that it applies to every account of the agent, and a
selector at the row's end. The help tip explains how the agent handles permission requests, that **Follow account
configuration** leaves it to each account's own configuration and, for Codex, that automatic review may use
additional tokens. Grok Build has no such row.

Codex's choices:

- **Follow account configuration** leaves the reviewer to each account's own Codex configuration. This is the initial
  choice; automatic review is never enabled just by adding an account.
- **Automatic review** asks Codex to review approval requests itself and may use additional tokens.
- **Ask me** asks Codex to send approval requests to the user.

Claude Code's choices:

- **Follow account configuration** passes nothing, so each account's own `settings.json` decides. This is the initial
  choice.
- **Automatic** launches Claude Code in its auto mode, where a classifier approves what it judges safe and blocks the
  rest. Whether auto mode is available to an account, which depends on its plan among other things, is Claude Code's
  own decision; Octoboard only asks for it.
- **Ask me** launches Claude Code so that every tool call not already allowed asks the user.
- **Approve everything** launches Claude Code so that every tool runs without asking.

The choice is saved when selected; the choices are unavailable while saving, and a failure is shown in an error alert.
It applies when a session of any account of the agent next starts or resumes, including an account switch, and a
legacy pinned directory with no account reference included; a running process keeps what it was started with. An
explicit choice overrides the agent's own setting for that launch without writing its configuration files, and for
Codex without changing its sandbox or approval policy.

**Approve everything** is saved only once the user accepts a destructive confirmation that names the agent and warns
that every tool will run without asking in every account of that agent, commands that change or delete files
included; its confirm button carries the choice's own name. Cancelling, Escape included, leaves the choice as it was.
If the save fails, the error is shown in the confirmation, which stays open, and the choice stays unchanged. Because the
user has agreed there, Claude Code's own blocking warning for that mode never appears (see "Per-agent specifics a user
will notice" in `docs/product/launching-agents.md`).

The choices are stored by Octoboard and stay on this machine (see "What is not configuration and never travels" in
`docs/product/configuration-export-and-import.md`). Versions of Octoboard that kept a choice per account are moved over
once: each agent's setting takes the choice its default account had, and the other accounts' choices are dropped.

## Adding, editing and removing an account

**Add account**, the icon button at the end of the agent strip, opens a form that asks for the agent, starting on the
agent of the tab in view —
every added agent is listed, in the list's order (every agent while the list is undecided), and can be picked, one that
is not installed included and marked with a Not installed badge after its name, since an account may be set up before its agent is, unlike
the agent pickers elsewhere (see "Which agents the agent pickers list" in `docs/product/consoles.md`) — then a name and a
directory, which can be typed or picked with the directory browser (see "Browsing directories" in
`docs/product/projects.md`). Editing an account offers its name and its directory, never its agent: a directory
belongs to one agent's layout. The directory is checked for being absolute (or starting with `~/`) and for not being
already used by another account of the same agent, the default account included (the full rules are in "Agent config
directories" in `docs/product/consoles.md`); one that does not exist yet is accepted, and the form says that it
need not exist yet. A directory already in use is refused by the daemon once saving is attempted, and the refusal is
shown in an error alert naming the account that uses it — the default account by the name it is shown under — with
the form left open and what was entered kept. For a Grok Build account the form adds
that the directory has to be a Grok home that Grok has already been run against, which is checked when a session
launches.

Every account is named by the user, and the name is required: it is stored trimmed, and one left blank is refused. A
name has to be unique within its agent, compared trimmed and ignoring letter case; accounts of different agents may
share a name. A name already taken by another account of the same agent, or by the default account — the name it is
shown under, which is the name the user gave it or else its name in the current language — is reported under the name
field. So is the English word "Default", which is reserved for the default account in every language.

Removing an account asks first and says that consoles referring to it go back to the agent's default account and that
sessions already open are not affected. A project referring to it is put back to inheriting its console's reference at
the same time (see "The project's account, model and reasoning effort" in `docs/product/projects.md`). Removing an
account, or pointing it at another directory, ends a login running for it. An account that shares the default
configuration, or whose last change of it is incomplete, has its sharing turned off first instead, under the same rule
as a change of the switch, its idle sessions ended and resumed around it: refused while sessions of the account are
working or it is being signed in, and while a
change of its sharing runs (see "Removing or repointing a sharing account" in
`docs/product/sharing-the-default-configuration.md`).
