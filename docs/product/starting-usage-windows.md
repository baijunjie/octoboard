# Starting a usage window that has not begun counting

A window that has reset does not begin counting again until the account's next request, and until then the agent reports
it with no reset time: it shows no countdown (see "How an account's quota is shown" in
`docs/product/account-quotas.md`), and the longer it waits, the later its next reset comes. With **Start usage windows
automatically** on (see that section in `docs/product/settings.md`), Octoboard makes that first request itself — a
**kick** — and then reads the account again, so the window starts counting and its countdown shows. A kick uses a little
of the account's quota. The setting is off by default and applies to every agent alike.

## When an account is kicked

An account is kicked after any reading of it (see "When quotas are read" in `docs/product/account-quotas.md`), except
one a session asked for (see below), when all of these hold:

- the setting is on;
- the account is not turned off (see "Turning an account off" in `docs/product/agent-accounts.md`), the default account
  included;
- the reading succeeded: an account that is not logged in, whose directory does not exist, whose latest reading failed
  (even one still showing out-of-date windows) or that has no quota to report is never kicked;
- the reading shows a window with no reset time that is not a **model-specific weekly** limit, which is never kicked:
  the request goes to one model and cannot start another model's limit;
- that window has not been kicked in its current episode.

A reading a session asked for with `get_account_quotas`' `refresh` (see "Reading quotas from a session" in
`docs/product/account-quotas.md`) never kicks: when it shows an account these conditions would kick, the account is
marked due instead, so the background poller reads it again at its next look, within about 15 seconds, and kicks it
after that reading as it would after any other. What a session's request sets off is a reading and nothing more;
spending quota to start a window stays the user's setting, acted on at the readings Octoboard takes itself.

One kick covers every such window of the account at once. A window's **episode** ends only when a reading shows that
window with a reset time again, so a window still without one after its kick, and one whose kick failed, is not kicked
again until it has been seen counting. A failed kick is written to the daemon's log and is not retried; nothing about it
is shown in the window. Episodes live in the daemon's memory with the readings: they start afresh when the daemon
restarts, and an account removed or pointed at another directory loses its own.

Turning the setting on, in Settings or by an import that applies it (see "Application settings are left alone unless
they are asked for" in `docs/product/configuration-export-and-import.md`), looks over the readings already held at once
and kicks every account they call for, without waiting for the next reading; so does turning an account back on while
the setting is on. Turning the setting off starts no further kick; one already running goes on to its end and to the
reading after it.

## How a kick runs

A kick runs the agent's CLI the way a reading does (see "How a quota is read" in `docs/product/account-quotas.md`), from
the home directory, as one request with no terminal: a one-word prompt, a one-line system prompt in place of the agent's
own, as few tools as the CLI lets be left out (none at all for Claude Code and Grok Build), one turn, and, where the CLI
can cap it, a reply of at most 128 output tokens. It uses the agent's cheapest model at its lowest reasoning effort:

| Agent | Model | Reasoning effort |
|---|---|---|
| Claude Code | `haiku`, its alias for the newest Haiku | `low` |
| Codex | `gpt-6-luna` | `low` |
| Grok Build | `grok-4.7` | `low` |

The reply is not read: a kick has done its work when the CLI ends with success. It is stopped after **60 seconds**, and
when the daemon shuts down; none is started once shutdown has begun. A kick runs in the background, so the reading that
called for it is shown at once and the reading after the kick follows when it ends. That reading is skipped when the
daemon is shutting down, or when the account was removed or pointed at another directory meanwhile; when the account's
login ended meanwhile, the login's own reading stands in for it (see "When quotas are read" in
`docs/product/account-quotas.md`). Should the reading after a kick show another window that calls for a kick, the
account is kicked again.

## What a kick leaves behind

A kick leaves out as much of the user's setup as the CLI allows without changing which sign-in is used:

- **Claude Code** keeps no conversation. The user's MCP servers are not started, no hook runs (the user's or a
  plugin's), and skills, slash commands, the user's `CLAUDE.md` and output style are left out. The rest of the user's
  settings still load, since they can decide which credentials are used, and the installed plugins are synchronised as
  for a reading.
- **Codex** writes no session files. It runs without the user's `config.toml`, so its MCP servers, its `notify` program,
  its plugins, apps and hooks do not start. For the same reason an account whose `config.toml` chooses another
  credential store or another model provider cannot be kicked: its kick fails, and is logged as above.
- **Grok Build** leaves a session record in the account's directory, and loads its own configuration, which it offers no
  way to leave out. It does not take in Claude Code's skills, MCP servers, agents, rules or hooks, and it runs in a
  process of its own rather than through a Grok session already running. Besides the kick's own request, it sends two
  small requests of its own to title the session, which no option stops.
