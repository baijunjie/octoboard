# Account quotas

How much of an account's plan quota is left: the plan an account is on, and for each of the plan's usage windows how
much remains and when it resets. Octoboard shows it for every account of an agent — the agent's default account and
every account the user owns (see "Agent accounts" in `docs/product/settings.md`) — for Claude Code, Codex and Grok
Build alike. It is shown in Settings' **Agent accounts** section.

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

The account's plan sits beside its name as a small chip, absent when the agent does not name one. Beneath the account
each window has:

- a line reading "<window> N% remaining", where N is what remains of the window as a whole percent from 0 to 100 (a
  window the agent reports as more than fully used shows 0%);
- at the end of the same line, "resets in <countdown>": days and hours from a day away ("2d 3h"), hours and minutes
  from an hour away ("1h 57m"), minutes below that ("12m"), always rounded up to the next minute. It counts down by
  itself, moving at least every 30 seconds, and reads "resets now" once the reset time has passed and a newer reading
  has not arrived yet. A window the agent gives no reset time for has no countdown;
- a bar of what remains, full when nothing is used. The bar has one colour per agent: an orange for Claude Code, a
  blue for Codex, and the text colour for Grok Build, each set off against the bar's track in both light and dark
  appearance.

The windows sit two to a row, and one to a row in a window narrower than 640 px. For assistive technology each bar
is named by its window and carries the remaining percent and the countdown as its value, and the drawn line above it
is not read separately.

## Quota states

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
