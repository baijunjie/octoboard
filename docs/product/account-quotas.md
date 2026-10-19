# Account quotas

How much of an account's plan quota is left: the plan an account is on, and for each of the plan's usage windows how
much remains and when it resets. Octoboard shows it for every account of an agent — the agent's default account and
every account the user owns (see "Agent accounts" in `docs/product/settings.md`) — for Claude Code, Codex and Grok
Build alike. It is shown in Settings' **Agent accounts** section, in the rail's account quota panel (see "The
account quota panel on the rail" below) and, in the macOS application, in the quota submenus of the menu bar icon's
menu (see "The icon's menu" in `docs/product/menu-bar-icon.md`).

## Which accounts have a quota

Every account of an **available** agent — one whose binary was found on the login shell's `PATH` — is read, the
default account included. The accounts of an agent that is not available, or whose availability is not yet
determined, are not read, and nothing about a quota is shown for them. A reading belongs to the account and to the
directory it points at: removing an account discards its reading at once, and so does pointing it at another
directory, after which it is read afresh from the new one; renaming it keeps the reading.

## How a quota is read

Octoboard asks the agent's own CLI, run the way a session of that account would be run: in the login shell's
environment, with the agent's configuration directory set to the account's directory (left as the shell has it for
the default account). The CLI looks after its own sign-in, Keychain access and token refresh; Octoboard never reads a
credential and never contacts a service of its own. The agents mark this way of asking as experimental, so a reading
that fails or cannot be understood only makes that account's quota unavailable and affects nothing else.

Readings live in the daemon's memory only. They are never stored, are not part of the application settings a console
session can read, and are never exported with the configuration (see "What is not configuration and never travels"
in `docs/product/configuration-export-and-import.md`), so after the daemon restarts every account starts unread
again.

## When quotas are read

- **In the background**, every account is read about every **5 minutes** while the daemon runs. An account whose
  reading fails is tried again after 10, then 20, then 40 minutes, and every 40 minutes after that, until a reading
  succeeds and the interval goes back to 5 minutes.
- **A new account**, or an agent found available after the daemon started, is read within about 15 seconds.
- **Opening the Agent accounts section** of Settings asks for a fresh reading of the accounts whose last attempt is
  older than the interval above; accounts read more recently keep their reading, so opening the section repeatedly
  starts nothing extra. The readings already held are shown at once either way.
- **Opening the menu bar icon's menu**, in the macOS application, asks for the same, each time the menu begins to
  open (see "The icon's menu" in `docs/product/menu-bar-icon.md`).
- **Opening the rail's account quota panel** asks for the same, each time the panel opens, whether it floats in or
  is pinned; pinning a panel that is already floating in is not a new opening (see "The account quota panel on the
  rail" below).
- At most **four** readings run at once across the whole daemon, and one account is never read twice at the same
  time.

Each finished reading reaches every connected window straight away.

## Quota windows

What each agent reports, and what each window is called:

| Agent | Windows |
|---|---|
| Claude Code | **5-hour**, **Weekly**, and one weekly window per model that has a weekly limit of its own, named after the model (for example "Fable weekly") |
| Codex | **5-hour** and **Weekly** |
| Grok Build | One credit pool, named after its current period: **Weekly** for a period of about a week (6 to 8 days), **Monthly** for one of about a month (28 to 31 days), and **Credits** when the period is anything else or not given |

The plan is shown as the agent names it, with each word capitalised (`max` as **Max**, `plus` as **Plus**, `pro_lite`
as **Pro Lite**); a name that already has capitals inside it keeps its own casing (**SuperGrok**). Only the plan and
these windows are shown: extra usage, spend, credit balances and pace estimates are not.

## How an account's quota is shown

In Settings and in the rail's panel, the account's plan sits beside its name as a small chip, absent when the agent
does not name one. Beneath the account each window has:

- a line reading "<window> N% remaining", where N is what remains of the window as a whole percent from 0 to 100 (a
  window the agent reports as more than fully used shows 0%);
- at the end of the same line, "resets in <countdown>": days and hours from a day away ("2d 3h"), hours and minutes
  from an hour away ("1h 57m"), minutes below that ("12m"), always rounded up to the next minute. It counts down by
  itself, moving at least every 30 seconds, and reads "resets now" once the reset time has passed and a newer reading
  has not arrived yet. A window the agent gives no reset time for has no countdown;
- a bar of what remains, full when nothing is used. The bar has one colour per agent: an orange for Claude Code, a
  blue for Codex, and the text colour for Grok Build, each set off against the bar's track in both light and dark
  appearance.

In Settings the windows sit two to a row, and one to a row in a window narrower than 640 px; in the rail's account
quota panel they are always one to a row. For assistive technology each bar
is named by its window and carries the remaining percent and the countdown as its value, and the drawn line above it
is not read separately.

## Quota states

This is what Settings and the rail's panel show; the menu bar icon's menu differs as "The icon's menu" in
`docs/product/menu-bar-icon.md` says.

| State | What the account shows |
|---|---|
| Its agent is not available, or not yet determined | Nothing about a quota, no plan either. |
| Not read yet | "Reading quota…" in place of the windows. |
| Read | The plan and the windows. |
| Never read successfully, and the last reading failed | "Quota unavailable" and the reason (see "Why a quota is unavailable" below). |
| Read before, but the latest reading failed | The last good plan and windows stay, with a line above them saying they are out of date and how long ago they were last read ("last read 12 minutes ago"), and the reason the newer reading failed. The next successful reading clears it. |

## Why a quota is unavailable

The reason is one of these, in the UI's language:

- the agent's binary was not found on the shell's `PATH`;
- the shell environment the CLI runs in could not be read;
- the CLI gave no reply within 15 seconds;
- the CLI could not be run, stopped without replying, wrote too much output, or refused the request — with its exit
  status and the last line it wrote to its error output, or the error it gave;
- the CLI replied, but not with a quota that could be read;
- the CLI replied that the account has no quota to report, as for an account without a subscription.

## What reading a quota leaves behind

Each reading starts one process of the agent, which is ended once it has replied or timed out; no conversation is
started. Where the CLI allows it without changing which sign-in is used, the
reading leaves out the user's MCP servers, plugins and hooks: Claude Code's MCP servers are not started, and Codex's
plugins, apps and hooks are turned off for the reading. Claude Code still loads the user's settings, since they can
decide which credentials are used, and Grok Build offers no way to leave anything out.

Some writes into the account's directory cannot be avoided:

- **Claude Code** synchronises the user's installed plugins and registers the running process under `sessions/`; the
  registration is cleared by a later start of Claude Code.
- **Codex** writes its model cache, its log databases and a temporary directory it clears on its next start.
- **Grok Build** writes its model cache, its session search index and its log.

## The account quota panel on the rail

The rail's **Account quota** button, a gauge directly above Settings (see "The rail" in
`docs/product/window-layout.md`), opens a floating panel beside the button, on the content panel's side, that shows
the accounts' quotas without opening Settings.

### What the panel shows

- **One tab per available agent**, in the order the agents are listed elsewhere, each showing the agent's icon and
  name. An agent that is not available, or whose availability is not yet determined, has no tab. Each opening starts
  on the first agent's tab.
- **The selected agent's accounts**: its default account, named "Default", then the accounts the user owns. Each shows
  its name, its plan chip and its quota beneath, in the states of "Quota states" above, as "How an account's quota is
  shown" above describes. A name too long for its space fades as in "Names too long for their space" in
  `docs/product/labels-and-tooltips.md`. The panel shows no directories and offers nothing to edit.
- The list of accounts is at most 384 px high, or 60% of the window's height where that is less, and the panel is as
  tall as the tallest agent's list, so choosing a tab with fewer accounts never moves the tab strip from under the
  pointer (which would close a panel opened by hover). Beyond that height a list scrolls and fades at its edges, and
  while it scrolls it is a tab stop of its own (see "Scrolling a surface from the keyboard" in
  `docs/product/scrolling-surfaces.md`).
- **With no available agent** there are no tabs: an empty state titled "No agent available" stands in their place,
  with a line saying an agent's quota is shown once its command is found on the user's `PATH`.
- At the panel's foot, **Manage accounts…** closes the panel and opens Settings on its Agent accounts section (see
  "Opening and closing Settings" in `docs/product/settings.md`).

### Opening and pinning the panel

The panel opens in one of two ways:

- **Resting the mouse on the button** for 250 ms floats the panel in; a pointer that only crosses the button shows
  nothing. The panel stays while the mouse is on the button or on the panel, and goes 150 ms after the mouse has left
  both, so crossing the gap between them does not close it. Touch and pen do not open it this way. Floating in moves
  no keyboard focus.
- **Pressing the button** — a click, a tap, Enter or Space — pins the panel open, whether or not it was already
  floating in, and the mouse leaving no longer closes it. Pinned from the keyboard or by assistive technology,
  keyboard focus moves into the panel, onto the selected agent's tab; pinned with a pointer, focus stays where it
  was, so after a mouse press the terminal keeps receiving keystrokes.

### Closing the panel

The panel closes, floating or pinned:

- on **Escape**, which is the panel's own while it is open: it reaches neither the agent in the terminal nor a
  drawer or a floating pane under the panel, which stay open;
- on a **press outside** the button and the panel, which also does whatever it does where it lands;
- when a **modal dialog or a menu opens** by any other way, such as Settings from the application menu;
- when **keyboard focus moves out of it** — Tab past its last control, Shift+Tab before its first, or F6.

A pinned panel also closes on **another press of the button**.

On closing, keyboard focus goes back to what held it when the panel opened — the button, after a keyboard pin —
unless the close itself put focus somewhere else: a press on something that takes focus leaves it there, Tab and
Shift+Tab out of the panel go on to the control after or before the one that held focus when it opened, and F6 goes
on as described in "Moving focus between regions with F6" in `docs/product/moving-focus-between-regions.md`.

### The panel is not modal

The panel does not take the window around it the way a menu or popover does (see "What an open dialog, menu or
popover covers" in `docs/product/window-layout.md`): while it is open the rest of the window stays usable, and the
window's keyboard shortcuts, Back and Forward, `F6` and the floating panes go on working. It is no stop of its own in
the `F6` cycle. Assistive technology is told it is a non-modal dialog named "Account quota", and the button says
whether the panel is expanded and which panel it controls. The button's tooltip is held back while the panel is open
(see "Tooltips on icon-only controls" in `docs/product/labels-and-tooltips.md`).
