# Account quotas

How much of an account's plan quota is left: the plan an account is on, and for each of the plan's usage windows how
much remains and when it resets. Octoboard shows it for every account of an agent — the agent's default account and
every account the user owns (see `docs/product/agent-accounts.md`) — for Claude Code, Codex and Grok
Build alike. It is shown in Settings' **Agent accounts** section, in the rail's account quota panel (see "The
account quota panel on the rail" below), in a compact indicator for the selected session (see "The selected
session's quota indicator" below) and, in the macOS application, on the menu bar icon and in its quota submenus
(see "The icon's menu" in `docs/product/menu-bar-icon.md`). An account whose quota is used up cannot open a session or
have one switched onto it (see "A used-up account" below). Octoboard can also start a window that has reset but not
begun counting, at the user's choice (see "Starting a window that has not begun counting" below).

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
- **Once a window resets**, the account is read again whatever the interval: when a window held for it, out-of-date
  ones included, has a reset time that passed after its last attempt began, it is read about **30 seconds** after
  that reset time, giving the service a moment to roll the window over. A reset is acted on once: a reading taken for
  it that fails leaves the account on its interval and retries as above.
- **A new account**, or an agent found available after the daemon started, is read within about 15 seconds.
- **When an account's login ends** (see `docs/product/logging-accounts-in.md`), its quota is
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
- **Opening the rail's account quota panel** asks for the same, each time the panel opens, whether the mouse floats it in or a tap or the keyboard opens it; a
  tap on a panel the mouse floated in is not a new opening (see "The account quota panel on the rail" below).
- **When a session asks**, with `get_account_quotas`' `refresh`, the accounts it covers are read again once their
  last attempt is at least **60 seconds** old (see "Reading quotas from a session" below).
- **After a kick** (see "Starting a window that has not begun counting" below), the account is read again once the
  kick has ended, whether or not it succeeded.
- At most **four** readings and kicks, together, run at once across the whole daemon, and one account is never read
  twice at the same time. While a kick runs on an account the account is not read either, unless its login
  ends meanwhile.

Each finished reading reaches every connected window straight away.

## Quota windows

What each agent reports, and what each window is called. A window is named the way the agent's own interface names
it, so the same kind of window can carry a different name for different agents:

| Agent | Windows |
|---|---|
| Claude Code | The **five-hour window**, called **Session**; the **account-wide weekly window**, called **Weekly · all models** when the same reading also has a model's own weekly window, and **Weekly** when it has none; and one weekly window per model that has a weekly limit of its own, named after the model (for example "Fable weekly") |
| Codex | The **five-hour window**, called **5h**, and the **account-wide weekly window**, called **Weekly** |
| Grok Build | One credit pool, named after its current period: **Weekly** for a period of about a week (6 to 8 days), **Monthly** for one of about a month (28 to 31 days), and **Credits** when the period is anything else or not given |

Inside a sentence — a used-up account's reason, an alert — a window is named the same way, as a limit: Claude Code's
"session limit", Codex's "5h limit", "weekly limit" ("weekly limit for all models" where the name above is
**Weekly · all models**), "Fable weekly limit", "monthly limit", and a credit pool named **Credits** as "credit limit".
Every place that names a window follows this: the meters in Settings and the rail's quota panel, the account pickers'
and the Switch account submenu's descriptions of an account, the selected session's quota indicator, the menu bar
icon and its quota submenus, the used-up errors in dialogs, and the daemon's refusal of a used-up account (see "Opening a
session" in `docs/product/sessions.md`). The UI's other languages name the windows by the same rule, in their own
words.

In Settings, the rail's quota panel and the native menu's account details, windows appear in this order: the
five-hour window, then the account-wide weekly window, then a credit pool, then **model-specific weekly** limits.
Windows within the same kind keep the order the agent reported, including the model-specific limits.

The plan is shown as the agent names it, with each word capitalised (`max` as **Max**, `plus` as **Plus**, `pro_lite`
as **Pro Lite**); a name that already has capitals inside it keeps its own casing (**SuperGrok**). Only the plan and
these windows are shown: extra usage, spend, credit balances and pace estimates are not.

## How an account's quota is shown

In Settings and in the rail's panel, the account's plan sits beside its name as a small badge, absent when the agent
does not name one; in Settings a badge in the accounts table's Status column, and beside the name in the account's
account settings dialog, says whether the account is logged in (see "The accounts table" in `docs/product/agent-accounts.md`).
Each window shows:

- what remains of the window, as a whole percent from 0 to 100 (a window the agent reports as more than fully used
  shows 0%);
- when it resets, "resets in <countdown>": days and hours from a day away ("2d 3h"), hours and minutes from an hour
  away ("1h 57m"), minutes below that ("12m"), always rounded up to the next minute. It counts down by itself, moving
  at least every 30 seconds, and reads "resets now" once the reset time has passed and a newer reading has not arrived
  yet. A window the agent gives no reset time for has no countdown;
- a bar of what remains, full when nothing is used. The bar has one colour per agent: an orange for Claude Code, a
  blue for Codex, and the text colour for Grok Build, each set off against the bar's track in both light and dark
  appearance. In the rail's panel, in Settings' accounts table and in the menu bar icon's quota submenus a used-up
  account's bars are a neutral grey instead (see "A used-up account" below).

Settings shows an account's quota in two places (see "The accounts table" and "The account settings dialog" in
`docs/product/agent-accounts.md`):

- the accounts table's **Usage** cell holds the account's compact usage bar, the one an account picker shows (see "In
  an account picker" below), with no numbers, and assistive technology reads what remains of its windows in words.
  Windows kept after a newer reading failed are drawn the same way, followed by the out-of-date mark. With no window to
  draw — none read, not logged in, or a failed reading with nothing read before — the cell holds a dash, followed by
  the failure mark when the reading failed (see "Quota states" below). A used-up account's cell is marked as "A used-up
  account" below says;
- the account's settings dialog draws its quota as the rail's panel does, every window included.

In the rail's panel, and in the account settings dialog, each window has a line reading "<window> N% remaining", with the
countdown at the end of the same line, above its bar.

How long ago the windows shown were read is shown in small muted text — at the end of the **Usage** heading in an
account's settings dialog in Settings (the table does not show it), at the end of its name line in the rail's panel
(which shows no directories), and at the end of its header in the menu bar icon's quota submenu: "Updated just now"
under a minute, otherwise "Updated 3 minutes ago" (in hours or days once it is that old). It is shown whenever the
account has windows to show, including windows kept after a newer reading failed, which a warning mark after it says are
out of date (see "Quota states" below; the submenu says so in a note under its header instead).

**A window whose reset time has passed** is shown as **100% remaining**, its bar full, until a newer reading arrives,
while its countdown reads "resets now". This holds wherever a quota is drawn or stated: Settings, the rail's panel, the
selected session's indicator and its accessible name, the account pickers' and the Switch account submenu's usage bars
and their descriptions, and, in the macOS application, the menu bar icon, its quota submenus and their opening lines.
Each notices the reset passing on its own clock: within 30 seconds in Settings, the rail's panel and an open dialog,
at the reset time itself on the rail's button, as of its opening in the Switch account submenu, and in the menu bar
as "The icon's menu" in `docs/product/menu-bar-icon.md` says.

In the rail's panel and the account settings dialog, an account with both a five-hour and an account-wide weekly window shows
the two as **one meter**: the five-hour window's line and its bar, directly beneath it a thinner bar for the weekly window, and beneath
that the weekly window's line, in the muted text colour. Every other window — Grok Build's credit pool, a
model-specific weekly limit, an account-wide weekly window with no five-hour window beside it — keeps a meter of its
own, and the meters sit one to a row. For assistive technology each bar, the thin weekly one included, is named by its window and carries the remaining percent and the countdown as its value, and the
drawn text is not read separately.

### In an account picker

In the account picker of the session dialog and the handoff dialog (see the **Agent** field in "Opening a session" in
`docs/product/sessions.md`), each account carries a compact, wordless usage bar before its name, in the agent's
colour: what remains of the five-hour window, with the account-wide weekly window's thin bar beneath it, or, for an
account without a five-hour window, one bar for its account-wide weekly window or its credit pool. An account with
nothing to draw — not read yet, not logged in, unavailable, or whose latest reading failed so that its windows are out
of date — shows an empty track. The tracks of a picker's or menu's bars have a hairline edge (a Settings table cell's have none), and the thin weekly bar is three quarters as tall as the main one, as everywhere a two-layer bar is drawn. The
same bar fills the Usage cell of Settings' accounts table, wider, where out-of-date windows are drawn rather than the
empty track (see "How an account's quota is shown" above). The bar is not read by
assistive technology; the account's option states what remains of those windows in words instead, or, for a used-up
account, why it cannot be chosen (see "A used-up account" below).

A session row's **Switch account** submenu shows its accounts the same way: the same bar before each account's name,
and the same words for assistive technology as the item's description, while its name stays the account's name (see
"Session rows" in `docs/product/sidebar-session-rows.md`).

The two dialogs show the account picker even for an agent with only its default account (see the **Agent** field in
"Opening a session" in `docs/product/sessions.md`), so that account carries its usage bar, and its note when used up,
in the picker like any other.

## Quota states

This is what Settings and the rail's panel show; the menu bar icon's menu differs as "The icon's menu" in
`docs/product/menu-bar-icon.md` says.

| State | What the account shows |
|---|---|
| Its agent is not available, or not yet determined | Nothing about a quota, no plan either; in Settings, a dash in the table's Usage cell and in the account settings dialog's Usage. |
| Not read yet | "Reading usage…" in place of the windows, in the table's Usage cell and in the account settings dialog. |
| Read | The plan, the windows and how long ago they were read. |
| Never read successfully, and the last reading failed | No windows, and a red failure mark named "Usage unavailable" on the account's name line — in Settings, after the dash in the table's Usage cell and on the account settings dialog's first line, after the account's name and its badges; it opens a popover with the reason (see "Why a quota is unavailable" below, and "Failures, warnings and notes marked at their point" in `docs/product/error-alerts.md` for how the popover opens). |
| Read before, but the latest reading failed | The last good plan and windows stay, and "Updated …" still says how long ago they were read, followed by an amber warning mark named as out of date and when they were last read ("Out of date: last read 12 minutes ago"); it opens a popover with the reason the newer reading failed. The next successful reading clears it. In Settings the account settings dialog shows them so; the table's Usage cell draws them as its bar, followed by the out-of-date mark. |
| Not logged in: its directory does not exist, or the agent says it is not signed in there | Any windows read before are dropped, and the agent's own message is not shown. In the rail's panel, a single line saying the account is not logged in yet and pointing to logging in to use it stands in place of the windows. In Settings the table's Usage cell holds a dash and the account's **Not logged in** badge says why (see "The accounts table" in `docs/product/agent-accounts.md`); its account settings dialog says, in place of the windows, that it is not logged in yet and to log in to use it. |

## Why a quota is unavailable

The reason is one of these, in the UI's language:

- the agent's binary was not found on the shell's `PATH`;
- the shell environment the CLI runs in could not be read;
- the CLI gave no reply within 60 seconds;
- the CLI could not be run, stopped without replying, wrote too much output, or refused the request — with its exit
  status and the last line it wrote to its error output, or the error it gave;
- the CLI replied, but not with a quota that could be read;
- the CLI replied that the account has no quota to report, as for an account without a subscription. This one is not
  tried less often the way a failed reading is (see "When quotas are read" above), but windows read before stay, out
  of date, as they do after a failed reading.

## A used-up account

An account is **used up** while the reading held for it has a five-hour, account-wide weekly or credit-pool window
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
  `docs/product/sidebar-session-rows.md`), and the switch confirmation refuses one on Confirm, in an error alert (see
  "Confirming a switch" in `docs/product/switching-session-accounts.md`).

Where it is said in short, under an account's name in a picker or a menu, it is shown on two lines: "Limit reached", and
beneath it the countdown to the window's reset as in "How an account's quota is shown" above ("resets in 1h 57m"),
with "Limit reached" alone when the agent gives no reset time. The short form is only drawn; assistive technology reads
the full form, as the account's description. Said in full, it names the account, the limit it has reached (the window
named as in "Quota windows" above) and when it resets. In an open dialog the judgement follows the clock,
so an account becomes usable again within 30 seconds of its reset time passing, without waiting for a newer reading.
The new-session dialog and the handoff dialog use that same clock when they show the account whose weekly quota
resets soonest (see "Which account a session uses" in `docs/product/sessions.md`): for up to one tick after a weekly
reset, either can still show an account whose reset has passed, and a session opened or handed off from the dialog
while it still shows that account runs on it. A session opened with defaults, and a session an agent starts, are
judged at the moment they are opened.

In the rail's account quota panel (see "What the panel shows" below) a used-up account stays listed in its place and
recedes, so that the accounts still usable stand out: its name is in the muted text colour, a neutral **Limit
reached** badge follows its plan badge on the name line, its windows' lines are in the muted text colour, and its bars
are a neutral grey instead of the agent's colour. A usable account looks as usual. Assistive technology reads the badge
as part of the account's name line, like the plan badge; the panel adds no full sentence, and each bar's value still
gives the window's countdown (see "How an account's quota is shown" above).

In Settings' accounts table (see "The accounts table" in `docs/product/agent-accounts.md`) a used-up account's row
recedes the same way: its name in the **Account** column is in the muted text colour, and its **Usage** cell draws the
usage bar in the same neutral grey, followed by a neutral **Limit reached** badge (and by the out-of-date mark when the
windows are out of date). There assistive technology does not read the badge: in place of what remains, the cell's
words say which limit the account has reached and, when known, when it resets ("Reached session limit · resets in
42m"). The account settings dialog does not mark a used-up account.

In the macOS menu bar icon's quota submenus a used-up account's bars are drawn in the same neutral grey (see "The
icon's menu" in `docs/product/menu-bar-icon.md`); no other usage bar turns grey.

The panel and the table judge on the clock as an open dialog does, so the account returns to its usual look within 30
seconds of its reset time passing, without waiting for a newer reading.

## Starting a window that has not begun counting

A window that has reset does not begin counting again until the account's next request, and until then the agent
reports it with no reset time: it shows no countdown (see "How an account's quota is shown" above), and the longer it
waits, the later its next reset comes. With **Start usage windows automatically** on (see that section in
`docs/product/settings.md`), Octoboard makes that first request itself — a **kick** — and then reads the account again,
so the window starts counting and its countdown shows. A kick uses a little of the account's quota. The setting is off
by default and applies to every agent alike.

### When an account is kicked

An account is kicked after any reading of it (see "When quotas are read" above), except one a session asked for (see
below), when all of these hold:

- the setting is on;
- the account is not turned off (see "Turning an account off" in `docs/product/agent-accounts.md`), the default
  account included;
- the reading succeeded: an account that is not logged in, whose directory does not exist, whose latest reading failed
  (even one still showing out-of-date windows) or that has no quota to report is never kicked;
- the reading shows a window with no reset time that is not a **model-specific weekly** limit, which is never kicked:
  the request goes to one model and cannot start another model's limit;
- that window has not been kicked in its current episode.

A reading a session asked for with `get_account_quotas`' `refresh` (see "Reading quotas from a session" below) never
kicks: when it shows an account these conditions would kick, the account is marked due instead, so the background
poller reads it again at its next look, within about 15 seconds, and kicks it after that reading as it would after any
other. What a session's request sets off is a reading and nothing more; spending quota to start a window stays the
user's setting, acted on at the readings Octoboard takes itself.

One kick covers every such window of the account at once. A window's **episode** ends only when a reading shows that
window with a reset time again, so a window still without one after its kick, and one whose kick failed, is not kicked
again until it has been seen counting. A failed kick is written to the daemon's log and is not retried; nothing about
it is shown in the window. Episodes live in the daemon's memory with the readings: they start afresh when the daemon
restarts, and an account removed or pointed at another directory loses its own.

Turning the setting on, in Settings or by an import that applies it (see "Application settings are left alone unless
they are asked for" in `docs/product/configuration-export-and-import.md`), looks over the readings already held at
once and kicks every account they call for, without waiting for the next reading; so does turning an account back on
while the setting is on. Turning the setting off starts no further kick; one already running goes on to its end and to
the reading after it.

### How a kick runs

A kick runs the agent's CLI the way a reading does (see "How a quota is read" above), from the home directory, as one
request with no terminal: a one-word prompt, a one-line system prompt in place of the agent's own, as few tools as the
CLI lets be left out (none at all for Claude Code and Grok Build), one turn, and, where the CLI can cap it, a reply of
at most 128 output tokens. It uses the agent's cheapest model at its lowest reasoning effort:

| Agent | Model | Reasoning effort |
|---|---|---|
| Claude Code | `haiku`, its alias for the newest Haiku | `low` |
| Codex | `gpt-6-luna` | `low` |
| Grok Build | `grok-4.7` | `low` |

The reply is not read: a kick has done its work when the CLI ends with success. It is stopped after **60 seconds**, and
when the daemon shuts down; none is started once shutdown has begun. A kick runs in the background, so the reading
that called for it is shown at once and the reading after the kick follows when it ends. That reading is skipped when
the daemon is shutting down, or when the account was removed or pointed at another directory meanwhile; when the
account's login ended meanwhile, the login's own reading stands in for it (see "When quotas are read" above). Should
the reading after a kick show another window that calls for a kick, the account is kicked again.

### What a kick leaves behind

A kick leaves out as much of the user's setup as the CLI allows without changing which sign-in is used:

- **Claude Code** keeps no conversation. The user's MCP servers are not started, no hook runs (the user's or a
  plugin's), and skills, slash commands, the user's `CLAUDE.md` and output style are left out. The rest of the user's
  settings still load, since they can decide which credentials are used, and the installed plugins are synchronised as
  for a reading.
- **Codex** writes no session files. It runs without the user's `config.toml`, so its MCP servers, its `notify`
  program, its plugins, apps and hooks do not start. For the same reason an account whose `config.toml` chooses
  another credential store or another model provider cannot be kicked: its kick fails, and is logged as above.
- **Grok Build** leaves a session record in the account's directory, and loads its own configuration, which it offers
  no way to leave out. It does not take in Claude Code's skills, MCP servers, agents, rules or hooks, and it runs in a
  process of its own rather than through a Grok session already running. Besides the kick's own request, it sends two
  small requests of its own to title the session, which no option stops.

## What reading a quota leaves behind

Each reading starts one process of the agent, which is ended once it has replied, timed out or the daemon shut down;
none is started once shutdown has begun, and no conversation is started (a kick does make a request; see "What a kick
leaves behind" above). Where the CLI allows it without changing which sign-in is used, the reading leaves out the
user's MCP servers, plugins and hooks: Claude Code's MCP servers are not started, and Codex's plugins, apps and hooks
are turned off for the reading. Claude Code still loads the user's settings, since they can decide which credentials
are used, and Grok Build offers no way to leave anything out.

Some writes into the account's directory cannot be avoided:

- **Claude Code** synchronises the user's installed plugins and registers the running process under `sessions/`; the
  registration is cleared by a later start of Claude Code.
- **Codex** writes its model cache, its log databases and a temporary directory it clears on its next start.
- **Grok Build** writes its model cache, its session search index and its log.

## The selected session's quota indicator

The rail's Account usage button shows the **remaining** quota of the selected session's own agent and account,
including the default account, as a compact indicator beneath its gauge, inside the same 32 px button. The gauge is
16 px with or without an indicator. With one, the gauge is raised from the button's centre — 2 px above one capsule,
4 px above two — and the indicator sits beneath it, so that the two stay together near the button's middle.

The indicator is drawn in the button's text colour, as one or two outlined capsules 18 px wide, centred, each with
fully round ends. What remains of a window is a solid fill from the reading direction's start (the right under a
right-to-left language), its end square where it stops short of the capsule's round end; the rest of the capsule is
hollow inside its outline:

- **Two layers** when the account has both a five-hour and an account-wide weekly window: a capsule 4 px tall for
  the five-hour window and, 1 px beneath it, one 3 px tall for the weekly window.
- **One capsule** otherwise, 5 px tall.

It follows session selection, a session's account switch and new quota readings, and fills a window's capsule once
its reset time passes (see "How an account's quota is shown" above); the console's or project's account preference
does not choose this reading.

The macOS menu bar icon shows the same session's quota using the same window selection and hiding rules below.
Its appearance and update timing are described in "The icon's look" in `docs/product/menu-bar-icon.md`.

The windows drawn are those of an account picker's usage bar (see "In an account picker" above): the five-hour window,
with the account-wide weekly window beneath it when the account has one; for an account without a five-hour window,
one bar for its account-wide weekly window, or else its credit pool, each named as in "Quota windows" above. A model-specific weekly limit is never
used. The button's accessible name identifies the agent and account name and, for each window drawn, the window and
its whole remaining percent; the bars are not read separately. The button has no tooltip (see "Tooltips on icon-only
controls" in `docs/product/labels-and-tooltips.md`).

The indicator is absent when no session is selected, its agent is unavailable or its availability is not yet
determined, the account has no reading or no eligible window, or the latest reading failed. A failed reading hides it
even when Settings and the quota panel retain the last good windows as out of date. Without an indicator the button's
accessible name is plain "Account usage". Whether the session's agent is added plays no part: a session whose agent has been removed from
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
accounts and an optional `refresh` (below), and answers with every account of every agent a session could be opened on, the default account first
(`account` is `default`), whether or not a reading is held for it. An agent that is not installed or not on the added
list lists no accounts; one not detected yet lists its accounts as not read yet. Each entry gives the account's
`account_name`, `plan`, `fetched_at` (milliseconds since the epoch, `null` for an account not read yet), `exhausted`
(whether the account is used up, by the rule in "A used-up account" above) and `exhausted_until` (when the window that
keeps it used up resets, in milliseconds since the epoch; `null` when it is not used up or the agent gives no reset
time), `unavailable` (the reason a reading is missing or could not be refreshed, else `null`) and its `windows`, each
with `kind` (`session` for the five-hour window, whichever agent's it is; `weekly` for the account-wide weekly window;
`weekly_model` for a model's own weekly window, the model named by `label`; `credit_period` for a credit pool),
`label`, `used_percent`, `remaining_percent` (100 less the used share, never below 0), `resets_at` and
`window_minutes`. Values are percentages only; no absolute token or money amount is given.

By default it answers from the readings already held and takes no new one, so an entry is as old as its `fetched_at`
and a call spends nothing. With `refresh` set to `true` it first reads again the accounts it lists — every agent's, or
only `agent`'s when that is given — and answers once those readings have finished, which can take several seconds:

- An account is read only when its last attempt, whatever took it — the background poller, a window's request, a kick's
  follow-up reading or an earlier `refresh` — is at least **60 seconds** old. That limit holds whichever session asks
  and however often, and it is the only rule: a failing account's longer retry interval does not hold it back, and an
  account due for a reading for any other reason is not read sooner by the request. Within the limit the held reading
  is returned as it is, with no error.
- An account whose reading is already running is not read again; the call answers with what is held for it.
- The reading is an ordinary one (see "How a quota is read" above): it reaches every window like any other, and counts
  towards the four readings and kicks that may run at once.
- It never kicks (see "When an account is kicked" above).
- A `refresh` that is not `true` or `false` is refused.

Each reading runs the agent's CLI, which is why a session's request is limited per account rather than left to the
caller: the tool's description tells the agent to ask only when a reading is too old to decide on.

## The account quota panel on the rail

The rail's **Account usage** button, a gauge directly above Settings (see "The rail" in
`docs/product/window-layout.md`), opens a floating panel beside the button, on the content panel's side, that shows
the accounts' quotas without opening Settings. The panel is drawn on the same surface as a tooltip (see "How a tooltip
behaves" in `docs/product/labels-and-tooltips.md`), at its own width, with its content laid out to its edges.

### What the panel shows

- **One tab per agent that is added and installed**, in the order of the added agents (see "Added agents" in
  `docs/product/agent-accounts.md`), each showing the agent's icon and name, as wide as its content, the tabs
  starting at the strip's start. An agent that is not added, or that has
  been determined unavailable, has no tab; an added agent whose availability is not yet determined has one. Each
  opening starts on the tab of the selected session's agent while the button shows that session's indicator (see "The
  selected session's quota indicator" above) and the agent has a tab, otherwise on the first agent's tab; a tab
  chosen in the panel lasts only until it closes. When it opens on the selected session's agent this way, that tab's
  list is also scrolled so that the session's account — the default account for a session that names none — is the
  first whole row in view, clear of the list's top fade, as far as the list can scroll; the row is not marked in any
  other way. Otherwise a list opens at its top.
- **The selected agent's accounts**: its default account, under the name the user gave it or else "Default" (see "The
  default account" in `docs/product/agent-accounts.md`), then the accounts the user owns. Each shows
  its name, its plan badge and its quota beneath, in the states of "Quota states" above, as "How an account's quota is
  shown" above describes; a used-up account recedes, with a **Limit reached** badge after its plan badge (see "A
  used-up account" above). The name and its plan badge keep to one line, and a name too long for its space fades
  before the plan badge, as in "Names too long for their space" in `docs/product/labels-and-tooltips.md`; what follows
  them on the name line — the Limit reached badge, a failure mark, how long ago the reading was taken — moves to the
  next line when it does not fit beside them. The panel shows no directories and offers nothing to edit.
- The list of accounts is at most 384 px high, or 60% of the window's height where that is less, and the panel is as
  tall as the tallest agent's list, so choosing a tab with fewer accounts never moves the tab strip from under the
  pointer (which would close a panel opened by hover). Beyond that height a list scrolls and fades at its edges, and
  while it scrolls it is a tab stop of its own (see "Scrolling a surface from the keyboard" in
  `docs/product/scrolling-surfaces.md`).
- **With no agent that is added and installed** there are no tabs, and an empty state stands in their place. When
  some agent is installed, or not yet determined unavailable, but none of them is added, it carries the bot icon, is
  titled "No agent added", with a line saying to add an agent to see its usage there, and offers an **Add agent…**
  button, which closes the panel and opens Settings on its Agent accounts section with the Add agent dialog in front
  (see "Opening and closing Settings" in `docs/product/settings.md`). When every agent has been determined
  unavailable, it carries the gauge icon, is titled "No agent available", with a line saying an agent's usage is
  shown once its command is found on the user's `PATH`, and offers nothing to press.
- At the panel's foot, **Manage accounts…** closes the panel and opens Settings on its Agent accounts section (see
  "Opening and closing Settings" in `docs/product/settings.md`). It is left out while the panel shows "No agent
  added", whose Add agent… button already leads there.

### Opening the panel

The panel opens by the input the device has, as a status mark's popover does (see "Failures, warnings and notes marked
at their point" in `docs/product/error-alerts.md`):

- **Resting the mouse on the button** for 250 ms floats the panel in; a pointer that only crosses the button shows
  nothing. The panel stays while the mouse is on the button or on the panel, and goes 150 ms after the mouse has left
  both, so crossing the gap between them does not close it. Touch and pen do not open it this way. Floating in moves
  no keyboard focus, and the window's shortcuts go on working beside it.
- **A mouse click on the button opens nothing and pins nothing**: where there is a mouse there is the hover. A click on
  the button of a panel the mouse floated in leaves it as it is.
- **A tap** (touch or pen), **assistive technology** and **Enter or Space** open it with keyboard focus moved into the
  panel, onto the selected agent's tab, or, with no tab, onto the panel's first button (Add agent…, or Manage
  accounts…). A panel opened this way is modal, as a menu is: the window's shortcuts and F6 wait until it closes,
  and a tap on a floating panel turns it into this form.

**While the mouse is on the button** and the button shows the selected session's account (see "The selected
session's quota indicator" above), the open panel makes that account stand out: every other account's row, on every
agent's tab, is dimmed by a veil of the panel's own colour, the same veil the sidebar lays over inactive projects. The
dimming lasts from then until the mouse enters the panel or the panel closes. Keyboard focus on the button does not
bring it. It is a passing cue, separate from a used-up account's look (see "A used-up account" above): the button's
account keeps its used-up look undimmed when it is used up, and another used-up account is dimmed on top of its own
look. Dimmed text falls below the usual contrast minimum, by decision, since the cue is lifted as soon as the mouse
enters the panel.

### Closing the panel

The panel closes:

- on **Escape**, which then goes no further: it reaches neither the agent in the terminal nor a drawer or a floating
  pane under the panel, which stay open. A panel the mouse floated in holds no focus, so it leaves the key alone
  while focus is in a terminal (the agent's or a shell's) or in a text field holding text, where the key does what it
  does there and the panel stays, and while an input method is composing. A panel opened by a tap or the keyboard
  holds focus and takes the key. With a mark's popover open inside the panel (see below), the key is that popover's
  where the popover would take it — keyboard focus is in it, or the mouse opened it and focus is not in a terminal or
  a text field holding text — which closes the popover alone, the panel staying open;
- on a **press outside** the button and the panel, which also does whatever it does where it lands (for a panel
  opened by a tap or the keyboard, the press is swallowed, as a menu's);
- when a **modal dialog or a menu opens** by any other way, such as Settings from the application menu.

After it closes by Escape or a press outside, a mouse still resting on the button does not open it again until it has
left the button and come back.

An account's failure or warning mark (see "Quota states" above) opens its popover from inside the panel, by the mouse
resting on it or a tap, and that popover counts as part of the panel: a press inside it, its own opening, and the mouse
crossing from the panel into it do not close the panel. How it opens and closes is in "Failures, warnings and notes
marked at their point" in `docs/product/error-alerts.md`; which of it and the panel Escape closes is in the Escape item
above.

On closing, keyboard focus goes back to what held it when the panel opened — the button, after a keyboard opening —
unless the close itself put focus somewhere else. A panel floated in by the mouse moved no focus, so closing it moves
none.

### The panel is not modal while the mouse has floated it in

A panel the mouse floated in does not take the window around it the way a menu or popover does (see "What an open
dialog, menu or popover covers" in `docs/product/window-layout.md`): while it is open the rest of the window stays
usable, and the window's keyboard shortcuts, Back and Forward, `F6` and the floating panes go on working. It is no
stop of its own in the `F6` cycle. Assistive technology is told it is a non-modal dialog named "Account usage", and
the button says whether the panel is expanded and which panel it controls. A panel opened by a tap or the keyboard is
modal, as a menu is: focus is inside it, an outside press is swallowed, and the window's shortcuts and `F6` wait until
it closes.
