# Account quotas

How much of an account's plan quota is left: the plan an account is on, and for each of the plan's usage windows how
much remains and when it resets. Octoboard shows it for every account of an agent — the agent's default account and
every account the user owns (see `docs/product/agent-accounts.md`) — for Claude Code, Codex and Grok Build alike. It is
shown in Settings' **Agent accounts** section, in the rail's account quota panel (see
`docs/product/account-quota-panel.md`), in a compact indicator for the selected session (see "The selected session's
quota indicator" below) and, in the macOS application, on the menu bar icon and in its quota submenus (see "The icon's
menu" in `docs/product/menu-bar-icon.md`). An account whose quota is used up cannot open a session or have one switched
onto it (see "A used-up account" below). Octoboard can also start a window that has reset but not begun counting, at the
user's choice (see `docs/product/starting-usage-windows.md`).

## Which accounts have a quota

Every account of an **available** agent — one whose binary was found on the login shell's `PATH` — is read, the
default account included. The accounts of an agent that is not available, or whose availability is not yet
determined, are not read, and nothing about a quota is shown for them. A reading belongs to the account and to the
directory it points at: removing an account discards its reading at once, and so does pointing it at another
directory, after which it is read afresh from the new one; renaming it keeps the reading.

## How a quota is read

Octoboard asks the agent's own CLI, given the environment a session of that account would get: the login shell's
environment, with the agent's configuration directory set to the account's directory (left as the shell has it for
the default account). It runs from the home directory, with no terminal. The CLI looks after its own sign-in, Keychain
access and token refresh; Octoboard never reads a credential and never contacts a service of its own. Each agent is
asked over the channel it offers its own front ends:

| Agent | What is run | What it is asked |
|---|---|---|
| Claude Code | `claude -p` in stream-JSON input and output mode, keeping no session, with no MCP servers (`--strict-mcp-config` and an empty `--mcp-config`) | Its `get_usage` control request, after `initialize` |
| Codex | `codex app-server`, with its plugins, remote plugins, apps and hooks turned off by `-c features.…=false` overrides | `account/rateLimits/read`, after `initialize` |
| Grok Build | `grok agent --no-leader stdio`, in a process of its own rather than through a Grok already running | `_x.ai/billing`, after `initialize` |

The agents mark these requests as experimental, so a reading that fails or cannot be understood only makes that
account's quota unavailable and affects nothing else. A reading is given 60 seconds and at most 8 MiB on its standard
output, beyond which it fails; its error output fails nothing, and only its first 4 KiB are kept for the reason a
failure gives.

An account whose directory does not exist is not run at all: it is reported as not logged in (see "Quota states"
below), and reading its quota never creates the directory, so one the user deleted stays deleted. The default account
names no directory of its own; for the default Codex account the same state comes from Codex's own refusal of a home
that does not exist. An account whose directory exists is reported as not logged in only when the agent itself says
it holds no credential of any kind there:

- **Claude Code**: no sign-in token, no API key and no subscription, while on Anthropic's own API;
- **Codex**: the quota request is refused for want of a sign-in, and Codex reports no account while it requires an
  OpenAI sign-in — which includes a home whose only key is `OPENAI_API_KEY` or `CODEX_API_KEY` in the environment;
- **Grok Build**: the quota request is refused for want of authentication and no sign-in method is set as the
  default, or it is refused because the stored credentials are invalid or have expired.

An account that runs on credentials with no plan quota behind them — an API key, Claude Code on Bedrock, Vertex or
Foundry, Codex on a custom or open-source model provider, Grok Build on an xAI API key — is signed in, and reads as
having no quota to report (see "Why a quota is unavailable" below), not as not logged in. A Codex or Grok Build refusal
for want of a sign-in that comes without the state telling the two apart is a failed reading. Neither not logged in
nor no quota to report is a failed reading.

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
- **Opening the rail's account quota panel** asks for the same, each time the panel opens, whether the mouse floats it
  in or a tap or the keyboard opens it; a tap on a panel the mouse floated in is not a new opening (see
  `docs/product/account-quota-panel.md`).
- **When a session asks**, with `get_account_quotas`' `refresh`, the accounts it covers are read again once their
  last attempt is at least **60 seconds** old (see "Reading quotas from a session" below).
- **After a kick** (see `docs/product/starting-usage-windows.md`), the account is read again once the kick has ended,
  whether or not it succeeded.
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
"session limit", Codex's "5h limit", "weekly limit" ("weekly limit for all models" where the name above is **Weekly ·
all models**), "Fable weekly limit", "monthly limit", and a credit pool named **Credits** as "credit limit". Every place
that names a window follows this: the meters in Settings and the rail's quota panel, the account pickers' and the Switch
account submenu's descriptions of an account, the selected session's quota indicator, the menu bar icon and its quota
submenus, the used-up errors in dialogs, and the daemon's refusal of a used-up account (see "When the agent or the
account cannot open a session" in `docs/product/opening-sessions.md`). The UI's other languages name the windows by the
same rule, in their own words.

In Settings, the rail's quota panel and the native menu's account details, windows appear in this order: the
five-hour window, then the account-wide weekly window, then a credit pool, then **model-specific weekly** limits.
Windows within the same kind keep the order the agent reported, including the model-specific limits.

The plan is shown as the agent names it, with each word capitalised (`max` as **Max**, `plus` as **Plus**, `pro_lite`
as **Pro Lite**); a name that already has capitals inside it keeps its own casing (**SuperGrok**). Only the plan and
these windows are shown: extra usage, spend, credit balances and pace estimates are not.

## How an account's quota is shown

In Settings and in the rail's panel, the account's plan sits beside its name as a small badge, absent when the agent
does not name one; in Settings a badge in the accounts table's Status column, and beside the name in the account's
account settings dialog, says whether the account is logged in (see "The accounts table" in
`docs/product/agent-accounts.md`). Each window shows:

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

In the rail's panel, and in the account settings dialog, each window has a line reading "<window> N% remaining", with
the countdown at the end of the same line, above its bar.

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

In the rail's panel and the account settings dialog, an account with both a five-hour and an account-wide weekly window
shows the two as **one meter**: the five-hour window's line and its bar, directly beneath it a thinner bar for the
weekly window, and beneath that the weekly window's line, in the muted text colour. Every other window — Grok Build's
credit pool, a model-specific weekly limit, an account-wide weekly window with no five-hour window beside it — keeps a
meter of its own, and the meters sit one to a row. For assistive technology each bar, the thin weekly one included, is
named by its window and carries the remaining percent and the countdown as its value, and the drawn text is not read
separately.

### In an account picker

In the account picker of the session dialog and the handoff dialog (see the **Agent** field in "The session dialog" in
`docs/product/opening-sessions.md`), each account carries a compact, wordless usage bar before its name, in the agent's
colour: what remains of the five-hour window, with the account-wide weekly window's thin bar beneath it, or, for an
account without a five-hour window, one bar for its account-wide weekly window or its credit pool. An account with
nothing to draw — not read yet, not logged in, unavailable, or whose latest reading failed so that its windows are out
of date — shows an empty track. The tracks of a picker's or menu's bars have a hairline edge (a Settings table cell's
have none), and the thin weekly bar is three quarters as tall as the main one, as everywhere a two-layer bar is drawn.
The same bar fills the Usage cell of Settings' accounts table, wider, where out-of-date windows are drawn rather than
the empty track (see "How an account's quota is shown" above). The bar is not read by assistive technology; the
account's option states what remains of those windows in words instead, or, for a used-up account or one that is not
logged in, why it cannot be chosen (see "A used-up account" and "An account that is not logged in" below).

A session row's **Switch account** submenu shows its accounts the same way: the same bar before each account's name,
and the same words for assistive technology as the item's description, while its name stays the account's name (see
"Session rows" in `docs/product/sidebar-session-rows.md`).

The two dialogs show the account picker even for an agent with only its default account (see the **Agent** field in "The
session dialog" in `docs/product/opening-sessions.md`), so that account carries its usage bar, and its note when used
up, in the picker like any other.

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
| Not logged in: its directory does not exist, or the agent says it holds no credential there (see "How a quota is read" above) | Any windows read before are dropped, and the agent's own message is not shown. In the rail's panel, a single line saying the account is not logged in yet and pointing to logging in to use it stands in place of the windows. In Settings the table's Usage cell holds a dash and the account's **Not logged in** badge says why (see "The accounts table" in `docs/product/agent-accounts.md`); its account settings dialog says, in place of the windows, that it is not logged in yet and to log in to use it. |

## Why a quota is unavailable

The reason is one of these, in the UI's language:

- the agent's binary was not found on the shell's `PATH`;
- the shell environment the CLI runs in could not be read;
- the CLI gave no reply within 60 seconds;
- the CLI could not be run, stopped without replying, wrote too much output, or refused the request — for one that
  stopped without replying, with its exit status and the last line of the part of its error output that is kept (its
  first 4 KiB, so for a longer error output a line from that part, possibly cut short), and for a refusal, the error
  it gave, or, for a refusal for want of a sign-in, that it did not say whether it is signed in;
- the CLI replied, but not with a quota that could be read;
- the CLI replied that the account has no quota to report, as for an account without a subscription or one signed in
  with an API key or a provider other than the agent's own plan (see "How a quota is read" above). This one is not
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

A used-up account cannot have a session opened on it, by anyone, the user included (see "When the agent or the account
cannot open a session" in `docs/product/opening-sessions.md`), and a session cannot be switched onto it (see "When a
switch fails" in `docs/product/switching-session-accounts.md`). A session already running on it keeps running, and
resuming, restarting or reopening a session is never refused for this. Octoboard never moves a session off it because
it reads as used up; only a turn that stops on the usage limit can, while the user has turned automatic switching on
(see `docs/product/automatic-account-switch.md`).

The window does not offer what the daemon would refuse:

- the session and handoff dialogs list a used-up account but do not let it be picked, open on another account instead,
  and refuse to submit when no account of the chosen agent is left (see the **Agent** field in "The session dialog" in
  `docs/product/opening-sessions.md`);
- opening a session with defaults uses another account of the agent, or is unavailable when none is left (see "Opening a
  session with defaults" in `docs/product/opening-sessions.md`);
- a session row's **Switch account** submenu disables a used-up account (see "Session rows" in
  `docs/product/sidebar-session-rows.md`), and the switch confirmation refuses one on Confirm, in an error alert (see
  "Confirming a switch" in `docs/product/switching-session-accounts.md`).

Where it is said in short, under an account's name in a picker or a menu, it is shown on two lines: "Limit reached", and
beneath it the countdown to the window's reset as in "How an account's quota is shown" above ("resets in 1h 57m"), with
"Limit reached" alone when the agent gives no reset time. The short form is only drawn; assistive technology reads the
full form, as the account's description. Said in full, it names the account, the limit it has reached (the window named
as in "Quota windows" above) and when it resets. In an open dialog the judgement follows the clock, so an account
becomes usable again within 30 seconds of its reset time passing, without waiting for a newer reading. The new-session
dialog and the handoff dialog use that same clock when they show the account whose weekly quota resets soonest (see
"Which account a session uses" in `docs/product/opening-sessions.md`): for up to one tick after a weekly reset, either
can still show an account whose reset has passed, and a session opened or handed off from the dialog while it still
shows that account runs on it. A session opened with defaults, and a session an agent starts, are judged at the moment
they are opened.

In the rail's account quota panel (see "What the panel shows" in `docs/product/account-quota-panel.md`) a used-up
account stays listed in its place and recedes, so that the accounts still usable stand out: its name is in the muted
text colour, a warning-coloured **Limit reached** badge follows its neutral, bordered plan badge on the name line, its
windows' lines are in the muted text colour, and its bars are a neutral grey instead of the agent's colour. A usable
account looks as usual. Assistive technology reads the badge as part of the account's name line, like the plan badge;
the panel adds no full sentence, and each bar's value still gives the window's countdown (see "How an account's quota is
shown" above).

In Settings' accounts table (see "The accounts table" in `docs/product/agent-accounts.md`) a used-up account's row
recedes the same way: its name in the **Account** column is in the muted text colour, and its **Usage** cell draws the
usage bar in the same neutral grey, followed by a warning-coloured **Limit reached** badge (and by the out-of-date mark
when the windows are out of date). There assistive technology does not read the badge: in place of what remains, the
cell's words say which limit the account has reached and, when known, when it resets ("Reached session limit · resets in
42m"). The account settings dialog does not mark a used-up account.

In the macOS menu bar icon's quota submenus a used-up account's bars are drawn in the same neutral grey (see "The
icon's menu" in `docs/product/menu-bar-icon.md`); no other usage bar turns grey.

The panel and the table judge on the clock as an open dialog does, so the account returns to its usual look within 30
seconds of its reset time passing, without waiting for a newer reading.

## An account that is not logged in

An account is **not logged in** while the last reading held for it, under the directory it points at now, found the
agent not signed in there or its directory missing (the reading's `unavailable` code is `quota_not_signed_in` or
`quota_config_dir_missing`; see "Quota states" above). It is judged on the reading already held, as a used-up account
is. A reading that failed for any other reason — a timeout, a CLI that could not be run — says nothing about the
sign-in, and neither does an account not read yet: such an account can still be chosen and opened on.

An account that is not logged in is treated as clearly unusable:

- **The daemon refuses to open a session on it, hand a session off to it or switch a session onto it**, by anyone, the
  user included, with `account_not_signed_in` (see "When the agent or the account cannot open a session" in
  `docs/product/opening-sessions.md`). A session already on it keeps running, and resuming, restarting or reopening a
  session is never refused for this.
- **Automatic account selection passes over it** (see "Which account a session uses" in
  `docs/product/opening-sessions.md`), and so does a session started by another session where it would otherwise run on
  its caller's own account (see "The agent and account of a session a session starts" in
  `docs/product/hub-orchestration.md`), and so does the automatic account switch (see "Which account it is switched to"
  in `docs/product/automatic-account-switch.md`).
- **The window lists it but does not let it be chosen**: in the account pickers of the session dialog, the handoff
  dialog and the forms in which the user approves a session's request, and in a session row's **Switch account**
  submenu, it is listed,
  disabled, with "Not logged in" under its name and, for assistive technology, the full reason as its description —
  that it is unavailable until it is logged in. A choice that ends on it is marked invalid with the reason that the
  account is not logged in, so a session cannot use it, and to choose another account or log in to it. An account
  that is turned off is described as turned off instead. The console and project dialogs' account pickers do not mark
  it, since a console's or project's account is a lasting preference.
- **Opening a session with defaults passes over it**, as it passes over a used-up account (see "Opening a session with
  defaults" in `docs/product/opening-sessions.md`).

## What reading a quota leaves behind

Each reading starts one process of the agent, which is ended, together with the helper processes it started, once it has
replied, timed out or the daemon shut down; none is started once shutdown has begun, and no conversation is started (a
kick does make a request; see "What a kick leaves behind" in `docs/product/starting-usage-windows.md`). Where the CLI
allows it without changing which sign-in is used, the reading leaves out the user's MCP servers, plugins and hooks:
Claude Code's MCP servers are not started, and Codex's plugins, apps and hooks are turned off for the reading. Claude
Code still loads the user's settings, since they can decide which credentials are used, and Grok Build offers no way to
leave anything out.

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
with the account-wide weekly window beneath it when the account has one; for an account without a five-hour window, one
bar for its account-wide weekly window, or else its credit pool, each named as in "Quota windows" above. A
model-specific weekly limit is never used. The button's accessible name identifies the agent and account name and, for
each window drawn, the window and its whole remaining percent; the bars are not read separately. The button has no
tooltip (see "Tooltips on icon-only controls" in `docs/product/labels-and-tooltips.md`).

The indicator is absent when no session is selected, its agent is unavailable or its availability is not yet determined,
the account has no reading or no eligible window, or the latest reading failed. A failed reading hides it even when
Settings and the quota panel retain the last good windows as out of date. It is absent, too, for an account the user
turned off, unless the selected session is running on it — starting, working, waiting for the user or awaiting
instructions (see "Turning an account off" in `docs/product/agent-accounts.md`). Without an indicator the button's
accessible name is plain "Account usage". Whether the session's agent is added plays no part: a session whose agent has
been removed from the added agents (see "Reordering and removing an agent" in `docs/product/agent-accounts.md`) keeps
its indicator.

An account removed from Settings or belonging to another agent cannot supply the indicator. Sessions retain their
launch configuration directory: if it differs from the named account's current directory, or a session refers to
the default account but retains an explicit directory, the indicator is hidden. An older session without a recorded
launch directory can still show an existing named account's quota.

The button opens the account quota panel described in `docs/product/account-quota-panel.md`, on the tab of the agent
whose quota the indicator shows (see "What the panel shows" in `docs/product/account-quota-panel.md`). Selecting a
session does not itself request a fresh quota reading; the indicator uses the readings described in "When quotas are
read" above.

## Reading quotas from a session

Every session, of any kind, has the `get_account_quotas` tool, so an agent can check what an account has left before it
picks one for `switch_session_account` or `handoff_session`. It takes an optional `agent` to list only that agent's
accounts and an optional `refresh` (below), and answers with every account of every agent a session could be opened on
that the user has not turned off, the default account first (`account` is `default`), whether or not a reading is held
for it. An account the user turned off is not listed, with no exception for the caller's own account (see "Turning an
account off" in `docs/product/agent-accounts.md`); a session finds the account it runs on with `get_session`, which
left without a `session` reads the caller itself (see "Session records" in `docs/product/what-every-session-reads.md`).
An agent that is not installed or not on the added list lists no accounts; one not detected yet lists its accounts as
not read yet. Each entry gives the account's `account_name`, `plan`, `fetched_at` (milliseconds since the epoch, `null`
for an account not read yet), `exhausted` (whether the account is used up, by the rule in "A used-up account" above)
and `exhausted_until` (when the window that keeps it used up resets, in milliseconds since the epoch; `null` when it is
not used up or the agent gives no reset time), `unavailable` (the reason a reading is missing or could not be
refreshed, else `null`), `can_open` and `cannot_open_reason` (below), and its `windows`, each with `kind` (`session` for
the five-hour window, whichever agent's it is; `weekly` for the account-wide weekly window; `weekly_model` for a model's
own weekly window, the model named by `label`; `credit_period` for a credit pool), `label`, `used_percent`,
`remaining_percent` (100 less the used share, never below 0), `resets_at` and `window_minutes`. Values are percentages
only; no absolute token or money amount is given.

- **`can_open`** says whether a session could be opened on the account now. When it is `false`, `cannot_open_reason`
  says why in prose: the account is used up (see "A used-up account" above), it is not logged in (see "An account that
  is not logged in" above), or a change of its sharing of the default configuration is running — the reasons, besides
  being turned off, that automatic account selection passes an account over for. It is `null` when `can_open` is
  `true`.
- **`auto_select_account`**, at the top of the answer beside `accounts`, is the setting of that name (see
  "Automatically select an account" in `docs/product/settings.md`), read-only, so that a session, which may not be able
  to read the settings, knows whether an account it leaves out is chosen for it.

By default it answers from the readings already held and takes no new one, so an entry is as old as its `fetched_at`
and a call spends nothing. With `refresh` set to `true` it first reads again the accounts it lists — every agent's, or
only `agent`'s when that is given — together with those of an agent that is installed but not added, which Octoboard
reads too, never an account the user turned off, and answers once those readings have finished, which can take several
seconds:

- An account is read only when its last attempt, whatever took it — the background poller, a window's request, a kick's
  follow-up reading or an earlier `refresh` — is at least **60 seconds** old. That limit holds whichever session asks
  and however often, and it is the only rule: a failing account's longer retry interval does not hold it back, and an
  account due for a reading for any other reason is not read sooner by the request. Within the limit the held reading
  is returned as it is, with no error.
- An account whose reading is already running is not read again; the call answers with what is held for it.
- The reading is an ordinary one (see "How a quota is read" above): it reaches every window like any other, and counts
  towards the four readings and kicks that may run at once.
- It never kicks (see "When an account is kicked" in `docs/product/starting-usage-windows.md`).
- A `refresh` that is not `true` or `false` is refused.

Each reading runs the agent's CLI, which is why a session's request is limited per account rather than left to the
caller: the tool's description tells the agent to ask only when a reading is too old to decide on.

**A bound session is told to report a low quota.** The role description of a session bound to an owner tells it that
its account's quota can run out while it works: to find the account it runs on with `get_session` (no `session`), look
it up in `get_account_quotas` — where an account the user turned off, possibly its own, is not listed — and, when that
shows it running low, say so in a `report` with the status `needs_decision`, naming the account and what is left,
rather than carry on until it stops, its owner deciding whether to switch it to another account or hand it off. It is
also told that, where the user has turned automatic switching on, Octoboard switches it by itself when a turn stops on
its account's usage limit, tells it so, and that it then carries on there (see
`docs/product/automatic-account-switch.md`).
