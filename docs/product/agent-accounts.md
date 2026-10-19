# Agent accounts

Settings' **Agent accounts** section (see "Agent accounts" in `docs/product/settings.md`): the agents the user has
added, and every account of each, grouped by agent. The section opens with a paragraph saying what an account is — a
named config directory of an agent, where it keeps its login and conversation history — and that an agent has to be on
the user's `PATH` for its accounts to be usable. Below that paragraph sit the **Add account** and **Add agent** buttons,
then the agent strip with the selected agent's accounts. What an account is and how consoles, projects and sessions use
one are in "Agent config directories" in `docs/product/consoles.md`.

## Added agents

Octoboard keeps a list of the agents the user has **added**, in an order the user sets. It is kept by the daemon with
the other application settings, so every window shows the same list, and it is not part of a configuration export (see
"What is not configuration and never travels" in `docs/product/configuration-export-and-import.md`).

The list decides which agents this section shows, and in what order: the agent strip and the Add account form's agent
picker. Nothing else follows it: the console, project and session dialogs' agent pickers, the rail's account quota
panel and the menu bar icon's quota submenus list agents by whether they are installed, in the order Claude Code,
Codex, Grok Build, added or not.

**How the list starts.** The list is decided once, as soon as Octoboard has determined which agents are installed: it
is filled with every agent that is installed plus every agent the user owns an account of, in the order Claude Code,
Codex, Grok Build. This holds for a fresh installation, which owns no accounts and so starts with the installed agents,
and for an installation from before the list existed, so no account it had disappears from view. From then on only the
user changes it; a list the user has emptied stays empty.

**While the list is undecided** — before Octoboard has determined which agents are installed, and for the whole run
when the user's login shell environment could not be read (until Octoboard next starts) — the section lists every
agent, in the order Claude Code, Codex, Grok Build. The agents then have no menu and cannot be dragged, the strip has
no more-actions button, and the Add agent list adds nothing.

## The agent strip

Each added agent has a tab of its own, in the list's order, each tab showing the agent's icon and name; only the
selected agent's accounts are shown, in its tab's panel. The agent tabs are an underlined strip, set apart from the
list of Settings' sections, and scroll sideways when they do not fit. Once the section is scrolled far enough for the
strip to reach the top of the section's area, at any window width, the strip stays pinned there, opaque, and the
accounts scroll beneath it; the top fade then begins below the strip (see "The edge that hides content fades" in
`docs/product/scrolling-surfaces.md`). The section always opens on the first agent's tab, however it was opened; when
the agent in view is removed, the first agent's tab is shown.

An agent that is not added has no tab, installed or not. An added agent that is not available has a warning icon after
its name in its tab, and each account the user owns of that agent says, under its directory, that it is not usable
until the agent is on `PATH`. The tab carries a tooltip, opened while the pointer is on the tab and while the tab has
keyboard focus, that says the agent is not installed and that its accounts are not usable until it is on `PATH`; the
tab's accessible name is the agent's name marked as not installed. How that tooltip opens and closes is in "Tooltips
on indicators" in `docs/product/labels-and-tooltips.md`. An agent whose availability is not yet determined has no icon,
no tooltip and no such line, and while the list is undecided no tab has the icon or the tooltip.

## Adding an agent

**Add agent** opens a dialog with a line saying that only an agent installed on this machine can be added, and a list
of every agent, in the order Claude Code, Codex, Grok Build, each with its icon, its name and its status:

| Status | What the row offers |
|---|---|
| **Added** | Nothing. An added agent shows as added even when it is not installed. |
| **Installed** | An **Add** button, which puts the agent at the end of the list. |
| **Not installed** | A **How to install** link to the agent's own installation instructions, published by its vendor; Octoboard writes none of its own. |
| **Checking…** (not yet determined) | The Add button, unavailable until the agent is found installed. |

While the list is undecided every Add button is unavailable. The dialog stays open after an agent is added, its row
turning to Added, so several can be added in turn; it closes on its **Close** button. A refused addition — the agent is
not installed, is already added, or which agents are installed is not determined yet — is shown in an error alert
(see `docs/product/error-alerts.md`).

## Reordering and removing an agent

Each added agent has a menu, opened by a right-click on its tab, by the Menu key or Shift+F10 while its tab has
keyboard focus, or from the **More actions** button at the end of the strip, which acts on the agent of the tab in view
and is named for that agent. The menu holds:

- **Move earlier** and **Move later**, which move the agent one place along the list. Earlier and later are in the
  reading direction, so they hold under a right-to-left language too. Move earlier is unavailable for the first agent,
  and Move later for the last;
- **Remove agent**, set apart from the moves.

A tab can also be **dragged** to a new place with a mouse or a pen, when there is more than one agent: the tab follows
the pointer, the tabs it passes make room for it, and releasing it drops it there. Escape, or a cancelled pointer,
ends the drag with nothing moved. A touch on the strip scrolls it instead.

A move or a drop shows the new order at once, and the moved tab keeps keyboard focus. If the move is refused, the order
goes back and the refusal is shown in an error alert.

**Remove agent** asks first. The confirmation says that the agent is taken off the list of added agents and that its
accounts are kept and come back if it is added again. When the agent is still in use, a sentence follows saying that
what is listed still uses the agent and keeps that setting, then a list with one item per kind of user, leaving out a
kind with none: the consoles whose Default agent fields name it, the projects whose Default agent is it, and the
sessions of it that are not archived. Each item gives the count of its kind ("1 console", "3 sessions"); while there
are at most five, the count is followed by a colon and their names, each set apart as a chip ("1 console: Main"),
and beyond five only the count is given. Its confirm button, **Remove**, is styled as destructive. Once the agent is
removed its tab goes, keyboard focus goes back to the selected section's tab in Settings' list of sections, and the
accounts of the agent stay stored untouched. The consoles, projects and sessions that use it keep their setting, and its sessions go on running.

## With no agent added

When the user has removed every agent, the strip is replaced by an empty state titled "No agent added", with a line
saying to add an agent installed on this machine to manage its accounts here. **Add account** is unavailable; **Add
agent** stays.

## The default account

Each agent's panel starts with the agent's **default account**, with the directory it currently resolves to. A "?"
ends its name line, after the plan, and says in its tooltip, or in a popover once pressed, that the default account is
the setup a session runs under when no account is pinned. Its directory cannot be edited, and the default account cannot
be removed. Its actions menu holds **Rename** and **Log in** (see "Logging an account in"
below).

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
the tooltip and accessible name of a session's agent icon in the sidebar, focus mode, the archive view and
confirmations, the menu bar icon's quota submenus, and what a console session's settings tools answer (see "Application
settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md` and
`docs/product/switching-session-accounts.md`).

## The accounts the user owns

The accounts the user owns follow the default account, each with its name, its directory and an actions menu holding
**Edit**, **Log in** and, set apart, **Remove**. Both kinds of directory are shown as in "How paths are shown" in
`docs/product/settings.md`.

Every account of an available agent, the default account included, also shows its plan quota: the plan beside its
name, and beneath the account's row, across its whole width, one line and bar per quota window — or, in its place,
that the quota is still being read, is unavailable, or is out of date. Opening the section asks for a fresh reading of
every account whose last one is due, those of every agent and not only of the tab in view; the readings already held
are shown at once. What is read, when, and how each state looks are in `docs/product/account-quotas.md`.

**Login state.** After the account's name and its plan, a badge says whether the account is logged in, in words and
with a dot, not in colour alone:

| Badge | When |
|---|---|
| **Logged in**, in the success colour | The latest reading of its quota succeeded with at least one quota window. |
| **Not logged in**, in the warning colour | Its quota is in the not-logged-in state (see "Quota states" in `docs/product/account-quotas.md`). The row then shows no quota line beneath it; the badge is all that says so. |
| None | Anything else: the quota not read yet, a reading that failed for another reason, or windows kept from before a newer reading failed. |

## Approval modes

Every Codex account row, including the default account, has an inline approval-mode selector:

- **Follow account configuration** leaves the reviewer to that account's Codex configuration. This is the initial
  choice for both new and existing accounts; automatic review is never enabled just by adding an account.
- **Automatic review** asks Codex to review approval requests itself and may use additional tokens.
- **Ask me** asks Codex to send approval requests to the user.

The choice is saved when selected, separately from editing the account's name or directory. Choices are unavailable
while saving; a failure is shown in an error alert. It applies when a session next starts or resumes, including an account switch;
a running process keeps its current reviewer. A named account's choice is independent of the default account's:
following configuration does not inherit the default account's selector. If a session's named account was deleted,
or it holds a pinned directory without a stored account reference, it follows that original directory's Codex
configuration instead of adopting the default account's choice. Grok Build has no such selector.

These choices are stored by Octoboard. An explicit reviewer overrides Codex's reviewer for that launch without
writing its configuration files or changing its sandbox or approval policy. They remain local; see
"What is not configuration and never travels" in `docs/product/configuration-export-and-import.md`.

Every Claude Code account row, including the default account, has the same kind of selector with Claude Code's four
choices:

- **Follow account configuration** passes nothing, so the account's own `settings.json` decides. This is the initial
  choice for new and existing accounts.
- **Automatic** launches Claude Code in its auto mode, where a classifier approves what it judges safe and blocks the
  rest. Whether auto mode is available to the account, which depends on its plan among other things, is Claude Code's
  own decision; Octoboard only asks for it.
- **Ask me** launches Claude Code so that every tool call not already allowed asks the user.
- **Approve everything** launches Claude Code so that every tool runs without asking.

The choice is saved and applies exactly as Codex's does, a resume and an account switch included, and it stays local
too. **Approve everything** is saved only once the user accepts a destructive confirmation that names the account (or
the default account) and warns that Claude Code will run every tool in that account's sessions without asking,
commands that change or delete files included; its confirm button carries the choice's own name. Cancelling, Escape
included, leaves the choice as it was. If the save fails, the error is shown in the confirmation, which stays open, and
the choice stays unchanged. Because the user has agreed there, Claude Code's own blocking warning for that mode never
appears (see "Per-agent specifics a user will notice" in `docs/product/launching-agents.md`).

## Adding, editing and removing an account

One **Add account** button serves every agent: its form asks for the agent, starting on the agent of the tab in view —
every added agent is listed, in the list's order (every agent while the list is undecided), and can be picked, one that
is not installed included and labelled as not installed, since an account may be set up before its agent is, unlike
the agent pickers elsewhere (see "Agents that are not installed" in `docs/product/consoles.md`) — then a name and a
directory, which can be typed or picked with the directory browser (see "Browsing directories" in
`docs/product/projects.md`). Editing an account offers its name and its directory, never its agent: a directory
belongs to one agent's layout. The directory is checked for being absolute (or starting with `~/`) and for not being
already used by another account of the same agent, the default account included (the full rules are in "Agent config
directories" in `docs/product/consoles.md`); one that does not exist yet is accepted, and the form says that Octoboard
does not require it to exist. A directory already in use is reported, once saving is attempted, under the directory
field, naming the account that uses it — the default account by the name it is shown under; the report goes away as
soon as the directory is changed, typed or picked, or another agent is picked. For a Grok Build account the form adds
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
account, or pointing it at another directory, ends a login running for it.

## Logging an account in

**Log in**, in an account's actions menu, signs the account in with its agent's own CLI, run by the daemon in the
background against the account's config directory (for the default account, the directory the user's login shell gives
the agent), with no terminal and no window. Octoboard never reads or writes the credential itself. Each agent signs in
its own way:

| Agent | What the user sees |
|---|---|
| Claude Code | Octoboard opens the authorization page in the browser. Authorizing there completes the login. |
| Codex | Octoboard creates the account's config directory first if it does not exist, then opens the authorization page in the browser. Only one Codex login runs at a time, across all accounts. |
| Grok Build | Grok Build opens the authorization page in the browser itself, and Octoboard does not open it again. The login dialog shows the code to confirm on that page. |

Octoboard opens the page once, as soon as the agent has given it, and only in the window where the login was started,
whether or not Settings is still open there. Opening it in the browser failing is shown in an error alert with the
address to open by hand.

### The login dialog

Log in opens a modal dialog titled for the agent and the account ("Log in to Codex account Work") and starts the
login at once. While the login runs, the dialog shows a spinner and says that the account is logging in, then that it
is waiting for the user to authorize in the browser — for Grok Build, with the code to confirm instead. Its buttons
are **Open the page again**, while the page's address is known, and **Cancel login**. While the login runs, what the
dialog says is announced to assistive technology as it changes.

**Closing the dialog cancels the login**: Cancel login, Escape and the dialog's close button all end a login that is
still running. Only one login can be followed from Settings at a time, since the dialog is modal; the daemon itself
runs at most one login per account, and one Codex login at a time across all accounts. Log in is unavailable for an
account whose login is running.

When the Agent accounts section opens while a login is already running — after a reload, or started in another window
— the dialog opens on that login (on one of them, when several are running), without starting another, so it can be
followed or cancelled there.

A login ends in one of these:

- **Logged in**: the agent confirms the account is signed in. The dialog closes by itself, the account's quota is read
  again at once, and the account's badge then shows the result (see "The accounts the user owns" above). Assistive
  technology is told once that the account is logged in.
- **Could not log in**: the dialog stays open, says the login to the account did not go through and gives the agent's
  reason. When the CLI said it succeeded but the agent still reports the account not signed in, the reason says so.
- **Timed out**, after 10 minutes of waiting for the user to authorize: the dialog stays open and says the login timed
  out.
- **Cancelled**, including from another window: the dialog closes.
- **Lost**, when the daemon restarts or the connection is re-established while the login runs: the dialog closes.

A login that cannot start is shown the same way as one that could not log in, in the dialog with the daemon's reason:
for example while another Codex account is logging in. A start refused because this account's login is already
running is not shown as a failure; the dialog takes up that login instead. After a failure, a timeout or a refusal,
the dialog offers **Log in again**, which starts a new login in the same dialog, and **Close**.

Whatever the outcome, the account's quota is read again within about 15 seconds (see "When quotas are read" in
`docs/product/account-quotas.md`).
