# Account quotas

How much of an account's plan quota is left: the plan an account is on, and for each of the plan's usage windows how
much remains and when it resets. Octoboard shows it for every account of an agent — the agent's default account and
every account the user owns (see `docs/product/agent-accounts.md`) — for Claude Code, Codex and Grok
Build alike. It is shown in Settings' **Agent accounts** section, in the rail's account quota panel (see "The
account quota panel on the rail" below), in a compact indicator for the selected session (see "The selected
session's quota indicator" below) and, in the macOS application, on the menu bar icon and in its quota submenus
(see "The icon's menu" in `docs/product/menu-bar-icon.md`). An account whose quota is used up cannot open a session or
have one switched onto it (see "A used-up account" below).

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

An account whose directory does not exist is not run at all: it is reported as not logged in (see "Quota states"
below), and reading its quota never creates the directory, so one the user deleted stays deleted. The default account
names no directory of its own; for the default Codex account the same state comes from Codex's own refusal of a home
that does not exist. An account whose directory exists but that the agent reports as not signed in is reported as not
logged in too. Neither is a failed reading.

Readings live in the daemon's memory only. They are never stored, are not part of the application settings a console
session can read, and are never exported with the configuration (see "What is not configuration and never travels"
in `docs/product/configuration-export-and-import.md`), so after the daemon restarts every account starts unread
again.

## When quotas are read

- **In the background**, every account is read about every **5 minutes** while the daemon runs. An account whose
  reading fails is tried again after 10, then 20, then 40 minutes, and every 40 minutes after that, until a reading
  succeeds and the interval goes back to 5 minutes. An account that is not logged in, or that the agent says has no
  quota to report, is not a failure, and stays on the 5-minute interval.
- **A new account**, or an agent found available after the daemon started, is read within about 15 seconds.
- **When an account's login ends** (see "Logging an account in" in `docs/product/agent-accounts.md`), its quota is
  read again whatever the interval: at once after a successful login, and within about 15 seconds after any other
  end.
- **Opening the Agent accounts section** of Settings asks for a fresh reading. Such a request reads the accounts whose
  last attempt is older than the interval above, and also, once their last attempt is more than **30 seconds** old,
  the accounts that show no windows because the agent says they are not signed in or have no quota to report, so a
  sign-in done outside Octoboard (in a session of the agent, say) shows up the next time this section, or a menu or
  panel below, is opened. Every
  other account keeps its reading until it is due: one whose directory does not exist, one with windows (out-of-date
  ones included) and one whose reading failed. An account whose reading is already running is not read again, so
  opening the section repeatedly starts little or nothing extra. The readings already held are shown at once either
  way.
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

In Settings, the rail's quota panel and the native menu's account details, windows appear in this order: **5-hour**,
then the account-wide **Weekly**, then a credit pool, then **model-specific weekly** limits. Windows within the
same kind keep the order the agent reported, including the model-specific limits.

The plan is shown as the agent names it, with each word capitalised (`max` as **Max**, `plus` as **Plus**, `pro_lite`
as **Pro Lite**); a name that already has capitals inside it keeps its own casing (**SuperGrok**). Only the plan and
these windows are shown: extra usage, spend, credit balances and pace estimates are not.

## How an account's quota is shown

In Settings and in the rail's panel, the account's plan sits beside its name as a small chip, absent when the agent
does not name one; in Settings a badge saying whether the account is logged in follows it (see "The accounts the user
owns" in `docs/product/agent-accounts.md`). Beneath the account each window has:

- a line reading "<window> N% remaining", where N is what remains of the window as a whole percent from 0 to 100 (a
  window the agent reports as more than fully used shows 0%);
- at the end of the same line, "resets in <countdown>": days and hours from a day away ("2d 3h"), hours and minutes
  from an hour away ("1h 57m"), minutes below that ("12m"), always rounded up to the next minute. It counts down by
  itself, moving at least every 30 seconds, and reads "resets now" once the reset time has passed and a newer reading
  has not arrived yet. A window the agent gives no reset time for has no countdown;
- a bar of what remains, full when nothing is used. The bar has one colour per agent: an orange for Claude Code, a
  blue for Codex, and the text colour for Grok Build, each set off against the bar's track in both light and dark
  appearance.

An account with both a **5-hour** and an account-wide **Weekly** window shows the two as **one meter**: the 5-hour
line and its bar, directly beneath it a thinner bar for the Weekly window, and beneath that the Weekly window's line,
in the muted text colour. Every other window — Grok Build's credit pool, a model-specific weekly limit, a Weekly
window with no 5-hour window beside it — keeps a meter of its own.

In Settings the meters sit two to a row, and one to a row in a window narrower than 640 px; in the rail's account
quota panel they are always one to a row. For assistive technology each bar, the thin Weekly one included,
is named by its window and carries the remaining percent and the countdown as its value, and the drawn lines are not
read separately.

### In an account picker

In the account picker of the session dialog and the handoff dialog (see the **Agent** field in "Opening a session" in
`docs/product/sessions.md`), each account carries a compact, wordless usage bar before its name, in the agent's
colour: what remains of the 5-hour window, with the Weekly window's thin bar beneath it, or, for an account without a
5-hour window, one bar for its Weekly window or its credit pool. An account with nothing to draw — not read yet, not
logged in, unavailable, or whose latest reading failed so that its windows are out of date — shows an empty track. Each
track has a faint edge, so an empty one still shows on any surface; the thin Weekly track's edge is drawn around its
outside, and the track is inset so that this edge ends where the main track's ends do. The bar is not read by
assistive technology; the account's option states what remains of those windows in words instead, or, for a used-up
account, why it cannot be chosen (see "A used-up account" below).

A session row's **Switch account** submenu shows its accounts the same way: the same bar before each account's name,
and the same words for assistive technology as the item's description, while its name stays the account's name (see
"Session rows" in `docs/product/sidebar.md`).

The two dialogs show the account picker even for an agent with only its default account (see the **Agent** field in
"Opening a session" in `docs/product/sessions.md`), so that account carries its usage bar, and its note when used up,
in the picker like any other.

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
| Not logged in: its directory does not exist, or the agent says it is not signed in there | Any windows read before are dropped, and the agent's own message is not shown. In the rail's panel, a single line saying the account is not logged in yet and pointing to logging in to use it stands in place of the windows. In Settings nothing stands beneath the account; its **Not logged in** badge says so (see "The accounts the user owns" in `docs/product/agent-accounts.md`). |

## Why a quota is unavailable

The reason is one of these, in the UI's language:

- the agent's binary was not found on the shell's `PATH`;
- the shell environment the CLI runs in could not be read;
- the CLI gave no reply within 15 seconds;
- the CLI could not be run, stopped without replying, wrote too much output, or refused the request — with its exit
  status and the last line it wrote to its error output, or the error it gave;
- the CLI replied, but not with a quota that could be read;
- the CLI replied that the account has no quota to report, as for an account without a subscription. This one is not
  tried less often the way a failed reading is (see "When quotas are read" above), but windows read before stay, out
  of date, as they do after a failed reading.

## A used-up account

An account is **used up** while the reading held for it has a **5-hour**, account-wide **Weekly** or credit-pool window
that is 100% used or more and whose reset time is still to come or not given. The daemon and the window judge it the
same way:

- A **model-specific weekly limit** never makes an account used up: the account still serves the other models.
- A window whose reset time has passed no longer counts, even before a newer reading arrives.
- An account with **no windows** — never read, still being read, not logged in, or unavailable with nothing read
  before — is never used up.
- Windows kept **out of date** after a failed reading (see "Quota states" above) still count, by what they say.
- It is judged on the reading already held, at the moment it matters: nothing reads the quota afresh for it or waits
  for a reading.
- With several windows used up, the one named is the one that resets last, since the account cannot be used until
  then; one with no reset time counts as resetting last.

A used-up account cannot have a session opened on it, by anyone, the user included (see "Opening a session" in
`docs/product/sessions.md`), and a session cannot be switched onto it (see "When a switch fails" in
`docs/product/switching-session-accounts.md`). A session already running on it keeps running, and resuming,
restarting or reopening a session is never refused for this; Octoboard never moves a session off it on its own.

The window does not offer what the daemon would refuse:

- the session and handoff dialogs list a used-up account but do not let it be picked, open on another account instead,
  and refuse to submit when no account of the chosen agent is left (see the **Agent** field in "Opening a session" in
  `docs/product/sessions.md`);
- opening a session with defaults uses another account of the agent, or is unavailable when none is left (see "Opening
  a session with defaults" in `docs/product/sessions.md`);
- a session row's **Switch account** submenu disables a used-up account (see "Session rows" in
  `docs/product/sidebar.md`), and the switch confirmation refuses one (see "Confirming a switch" in
  `docs/product/switching-session-accounts.md`).

Where it is said in short, under an account's name in a picker or a menu, it is shown on two lines: "Used up", and
beneath it the countdown to the window's reset as in "How an account's quota is shown" above ("resets in 1h 57m"),
with "Used up" alone when the agent gives no reset time. The short form is only drawn; assistive technology reads the
full form, as the account's description. Said in full, it names the account, the window used up (its 5-hour
quota, weekly quota, monthly quota or credits) and when it resets. In an open dialog the judgement follows the clock,
so an account becomes usable again within 30 seconds of its reset time passing, without waiting for a newer reading.
The new-session dialog and the handoff dialog use that same clock when they show the account whose weekly quota
resets soonest (see "Which account a session uses" in `docs/product/sessions.md`): for up to one tick after a weekly
reset, either can still show an account whose reset has passed, and a session opened or handed off from the dialog
while it still shows that account runs on it. A session opened with defaults, and a session an agent starts, are
judged at the moment they are opened.

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

## The selected session's quota indicator

The rail's Account quota button shows the **remaining** quota of the selected session's own agent and account,
including the default account, as a compact indicator beneath its gauge, inside the same 32 px button. The gauge is
16 px with or without an indicator. With one, the gauge is raised from the button's centre — 2 px above one capsule,
4 px above two — and the indicator sits beneath it, so that the two stay together near the button's middle.

The indicator is drawn in the button's text colour, as one or two outlined capsules 18 px wide, centred, each with
fully round ends. What remains of a window is a solid fill from the reading direction's start (the right under a
right-to-left language), its end square where it stops short of the capsule's round end; the rest of the capsule is
hollow inside its outline:

- **Two layers** when the account has both a **5-hour** and an account-wide **Weekly** window: a capsule 4 px tall for
  the 5-hour window and, 1 px beneath it, one 3 px tall for the Weekly window.
- **One capsule** otherwise, 5 px tall.

It follows session selection, a session's account switch and new quota readings; the console's or project's account
preference does not choose this reading.

The macOS menu bar icon shows the same session's quota using the same window selection and hiding rules below.
Its appearance and update timing are described in "The icon's look" in `docs/product/menu-bar-icon.md`.

The windows drawn are those of an account picker's usage bar (see "In an account picker" above): the 5-hour window,
with the Weekly window beneath it when the account has one; for an account without a 5-hour window, one bar for its
Weekly window, or else its credit pool, labelled as in "Quota windows" above. A model-specific weekly limit is never
used. The button's accessible name identifies the agent and account name and, for each window drawn, the window and
its whole remaining percent; the bars are not read separately. The button has no tooltip (see "Tooltips on icon-only
controls" in `docs/product/labels-and-tooltips.md`).

The indicator is absent when no session is selected, its agent is unavailable or its availability is not yet
determined, the account has no reading or no eligible window, or the latest reading failed. A failed reading hides it
even when Settings and the quota panel retain the last good windows as out of date. Without an indicator the button's
accessible name is plain "Account quota". Whether the session's agent is added plays no part: a session whose agent has been removed from
the added agents (see "Reordering and removing an agent" in `docs/product/agent-accounts.md`) keeps its indicator.

An account removed from Settings or belonging to another agent cannot supply the indicator. Sessions retain their
launch configuration directory: if it differs from the named account's current directory, or a session refers to
the default account but retains an explicit directory, the indicator is hidden. An older session without a recorded
launch directory can still show an existing named account's quota.

The button opens the account quota panel as described below, on the tab of the agent whose quota the indicator shows
(see "What the panel shows" below). Selecting a session does not itself request a fresh quota reading; the indicator uses
the readings described in "When quotas are read" above.

## Reading quotas from a session

Every session, of any kind, has the `get_account_quotas` tool, so an agent can check what an account has left before it
picks one for `switch_session_account` or `handoff_session`. It takes an optional `agent` to list only that agent's
accounts, and answers with every account of every agent a session could be opened on, the default account first
(`account` is `default`), whether or not a reading is held for it. An agent that is not installed or not on the added
list lists no accounts; one not detected yet lists its accounts as not read yet. Each entry gives the account's
`account_name`, `plan`, `fetched_at` (milliseconds since the epoch, `null` for an account not read yet), `exhausted`
(whether the account is used up, by the rule in "A used-up account" above) and `exhausted_until` (when the window that
keeps it used up resets, in milliseconds since the epoch; `null` when it is not used up or the agent gives no reset
time), `unavailable` (the reason a reading is missing or could not be refreshed, else `null`) and its `windows`, each
with `kind`, `label`, `used_percent`, `remaining_percent` (100 less the used share, never below 0), `resets_at` and
`window_minutes`. Values are percentages only; no absolute token or money amount is given.

It answers from the readings already held and never waits for a new one, so an entry is as old as its `fetched_at`.

## The account quota panel on the rail

The rail's **Account quota** button, a gauge directly above Settings (see "The rail" in
`docs/product/window-layout.md`), opens a floating panel beside the button, on the content panel's side, that shows
the accounts' quotas without opening Settings.

### What the panel shows

- **One tab per agent that is added and installed**, in the order of the added agents (see "Added agents" in
  `docs/product/agent-accounts.md`), each showing the agent's icon and name, as wide as its content, the tabs
  starting at the strip's start. An agent that is not added, or that has
  been determined unavailable, has no tab; an added agent whose availability is not yet determined has one. Each
  opening starts on the tab of the selected session's agent while the button shows that session's indicator (see "The
  selected session's quota indicator" above) and the agent has a tab, otherwise on the first agent's tab; a tab
  chosen in the panel lasts only until it closes.
- **The selected agent's accounts**: its default account, under the name the user gave it or else "Default" (see "The
  default account" in `docs/product/agent-accounts.md`), then the accounts the user owns. Each shows
  its name, its plan chip and its quota beneath, in the states of "Quota states" above, as "How an account's quota is
  shown" above describes. A name too long for its space fades as in "Names too long for their space" in
  `docs/product/labels-and-tooltips.md`. The panel shows no directories and offers nothing to edit.
- The list of accounts is at most 384 px high, or 60% of the window's height where that is less, and the panel is as
  tall as the tallest agent's list, so choosing a tab with fewer accounts never moves the tab strip from under the
  pointer (which would close a panel opened by hover). Beyond that height a list scrolls and fades at its edges, and
  while it scrolls it is a tab stop of its own (see "Scrolling a surface from the keyboard" in
  `docs/product/scrolling-surfaces.md`).
- **With no agent that is added and installed** there are no tabs, and an empty state stands in their place. When
  some agent is installed, or not yet determined unavailable, but none of them is added, it carries the bot icon, is
  titled "No agent added", with a line saying to add an agent to see its quota there, and offers an **Add agent…**
  button, which closes the panel and opens Settings on its Agent accounts section with the Add agent dialog in front
  (see "Opening and closing Settings" in `docs/product/settings.md`). When every agent has been determined
  unavailable, it carries the gauge icon, is titled "No agent available", with a line saying an agent's quota is
  shown once its command is found on the user's `PATH`, and offers nothing to press.
- At the panel's foot, **Manage accounts…** closes the panel and opens Settings on its Agent accounts section (see
  "Opening and closing Settings" in `docs/product/settings.md`). It is left out while the panel shows "No agent
  added", whose Add agent… button already leads there.

### Opening and pinning the panel

The panel opens in one of two ways:

- **Resting the mouse on the button** for 250 ms floats the panel in; a pointer that only crosses the button shows
  nothing. The panel stays while the mouse is on the button or on the panel, and goes 150 ms after the mouse has left
  both, so crossing the gap between them does not close it. Touch and pen do not open it this way. Floating in moves
  no keyboard focus.
- **Pressing the button** — a click, a tap, Enter or Space — pins the panel open, whether or not it was already
  floating in, and the mouse leaving no longer closes it. Pinned from the keyboard or by assistive technology,
  keyboard focus moves into the panel, onto the selected agent's tab, or, with no tab, onto the panel's first button
  (Add agent…, or Manage accounts…); pinned with a pointer, focus stays where it was, so after a mouse press the
  terminal keeps receiving keystrokes.

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
whether the panel is expanded and which panel it controls.
