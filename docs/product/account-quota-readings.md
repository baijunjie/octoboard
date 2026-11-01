# Account quota readings

How Octoboard reads the account quotas described in `docs/product/account-quotas.md`, for the accounts its "Which
accounts have a quota" section names: how a quota is read, the sign-in email a reading carries, when readings are
taken, what a reading leaves behind and what the daemon's log records of readings.

## How a quota is read

Octoboard asks the agent's own CLI, given the environment a session of that account would get: the login shell's
environment, with the agent's configuration directory set to the account's directory (left as the shell has it for
the default account). It runs from the home directory, with no terminal. The CLI looks after its own sign-in, Keychain
access and token refresh; Octoboard never uses a credential and never contacts a service of its own. It reads the
agent's local sign-in record only to take the account's email from it (see "The account's sign-in email" below). Each
agent is asked over the channel it offers its own front ends:

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
in `docs/product/account-quotas.md`), and reading its quota never creates the directory, so one the user deleted
stays deleted. The default account names no directory of its own; for the default Codex account the same state comes from Codex's own refusal of a home
that does not exist. An account whose directory exists is reported as not logged in only when the agent itself says
it holds no credential of any kind there:

- **Claude Code**: no sign-in token, no API key and no subscription, while on Anthropic's own API;
- **Codex**: the quota request is refused for want of a sign-in, and Codex reports no account while it requires an
  OpenAI sign-in — which includes a home whose only key is `OPENAI_API_KEY` or `CODEX_API_KEY` in the environment;
- **Grok Build**: the quota request is refused for want of authentication and no sign-in method is set as the
  default, or it is refused because the stored credentials are invalid or have expired.

An account that runs on credentials with no plan quota behind them — an API key, Claude Code on Bedrock, Vertex or
Foundry, Codex on a custom or open-source model provider, Grok Build on an xAI API key — is signed in, and reads as
having no quota to report (see "Why a quota is unavailable" in `docs/product/account-quotas.md`), not as not logged
in. A Codex or Grok Build refusal for want of a sign-in that comes without the state telling the two apart is a failed reading. Neither not logged in
nor no quota to report is a failed reading.

Readings live in the daemon's memory only, the sign-in email included. They are never stored, are not part of the
application settings a console session can read, and are never exported with the configuration (see "What is not
configuration and never travels" in `docs/product/configuration-export-and-import.md`), so after the daemon restarts
every account starts unread again.

## The account's sign-in email

Each reading also carries, as `email`, the email the account is signed in with, so that accounts can be told apart. It
is taken from the agent's own sign-in record on this machine, read as a file, whenever the account is read:

| Agent | Where the email is taken from |
|---|---|
| Claude Code | `oauthAccount.emailAddress` in `.claude.json`: the one in the account's directory, and for the default account the file Claude Code itself uses — `.claude.json` in the directory the login shell's `CLAUDE_CONFIG_DIR` names, else `~/.claude.json` |
| Codex | The `email` claim of the ID token (`tokens.id_token`) in the directory's `auth.json`, or, when that claim is missing or not text, the email of its OpenAI profile claim. The token's signature is not checked. |
| Grok Build | The `email` of the entries in the directory's `auth.json`, when every entry that has one gives the same address; none when they differ |

For Codex and Grok Build the directory is the account's own, and for the default account the one the login shell's
environment gives the agent (`CODEX_HOME` or `GROK_HOME`, else the agent's own default directory). The address is
taken trimmed, and only when it is a single line of at most 254 characters.

The email is read whatever the quota reading's outcome, so an account whose quota could not be read still has one. It
is none:

- when the account reads as not logged in (see "Quota states" in `docs/product/account-quotas.md`), whatever a
  leftover record says, and when its directory does not exist;
- for the default account, when the login shell's environment could not be read, since its record's place follows
  that environment; an account the user owns is still read from its own directory then;
- when the record is missing, cannot be read or understood, or holds no email — including a Codex account that keeps
  its sign-in in the system keyring rather than in `auth.json`. This is never a failure: the reading goes on as it
  would without it.

A failed reading that keeps the windows of an earlier one (see "Quota states" in `docs/product/account-quotas.md`)
takes the email of the newer attempt, so the email is always the latest attempt's. Nothing else in the record is kept, and the email is never
written to the daemon's log (where any address the agent prints is masked too; see "What the daemon's log records of
readings" below) or put into an error. Settings shows it in the accounts table and in both account dialogs
(see "The accounts table", "The account details dialog" and "The account settings dialog" in
`docs/product/agent-accounts.md`); nothing else shows it — not the account pickers, the rail's account quota panel,
the menu bar icon or a session's information — and a session is not given it (see
`docs/product/reading-quotas-from-a-session.md`).

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
  last attempt is at least **60 seconds** old (see `docs/product/reading-quotas-from-a-session.md`).
- **After a kick** (see `docs/product/starting-usage-windows.md`), the account is read again once the kick has ended,
  whether or not it succeeded.
- **After a usage limit reset is redeemed**, the account is read again once Codex has been run for it, whatever the
  outcome (see "How a reset is redeemed" in `docs/product/usage-limit-resets.md`).
- At most **four** readings, kicks and redeems of a usage limit reset, together, run at once across the whole daemon,
  and one account is never read twice at the same time. While a kick runs on an account the account is not read either,
  unless its login ends meanwhile.

Each finished reading reaches every connected window straight away. A fresh successful reading can also continue a
turn waiting after the usage limit stopped it while automatic switching is on (see "When no account qualifies" in
`docs/product/automatic-account-switch.md`).

## What reading a quota leaves behind

Each reading starts one process of the agent; none is started once shutdown has begun, and no conversation is started (a
kick does make a request; see "What a kick leaves behind" in `docs/product/starting-usage-windows.md`). Once the agent
has replied, its standard input is closed and it is given up to **5 seconds** to exit by itself, since it may still be
finishing work begun for the request — Claude Code can be writing a refreshed sign-in back to the Keychain then, which
ending it at once could lose. The wait never runs past the reading's 60 seconds, its output still counts towards the
8 MiB, and it ends at once when the daemon begins shutting down. A process still running when the wait ends is ended,
and the reply it gave is read all the same. A process that gives no reply in time, writes too much, or is running when
the daemon shuts down is ended at once. However it ends, the helper processes it started are ended with it. Where the
CLI allows it without changing which sign-in is used, the reading leaves out the user's MCP servers, plugins and hooks:
Claude Code's MCP servers are not started, and Codex's plugins, apps and hooks are turned off for the reading. Claude
Code still loads the user's settings, since they can decide which credentials are used, and Grok Build offers no way to
leave anything out.

Some writes into the account's directory cannot be avoided:

- **Claude Code** synchronises the user's installed plugins and registers the running process under `sessions/`; the
  registration is cleared by a later start of Claude Code.
- **Codex** writes its model cache, its log databases and a temporary directory it clears on its next start.
- **Grok Build** writes its model cache, its session search index and its log.

## What the daemon's log records of readings

The daemon's log (`~/.octoboard/logs/daemon.log`, see "Files Octoboard owns" in
`docs/product/application-lifecycle.md`) is told when an account's readings change, not of every reading: an account
whose readings keep succeeding writes nothing. Kicks and redeems of a usage limit reset are not covered here.

- **An account that goes from signed in to not logged in** is logged as a warning: a reading finds it not logged in
  (see "Quota states" in `docs/product/account-quotas.md`) while the last reading that could tell — one with windows
  or with no quota to report — found it signed in. Failed readings in between do not count either way. The entry gives the reason: for Claude Code
  the fields of the account its `initialize` reply reported that decided it (`tokenSource`, `apiProvider`,
  `apiKeySource`, `subscriptionType`), followed by the error of its usage request when that was refused; for Codex that
  the quota request was refused for want of a sign-in and no account is reported; for Grok Build the refusal of its
  quota request; for an account whose directory does not exist, the reading's own reason. It also gives the failure the
  readings before it ended in, when they were failing.
- **An account that is signed in again** after reading as not logged in is logged as information.
- **A reading that starts failing**, or fails for a different reason than the failed reading before it, is logged as a
  warning, with how long the agent ran and the reason. Further failures for the same reason are not logged. The
  reasons are compared as written to the log.
- **Readings that recover**: an account whose readings were failing and that reads as signed in again is logged as
  information, with the reason it last failed for. Failed readings followed by not logged in are neither a recovery
  nor, unless the account was last seen signed in, a sign-out.
- **A process Octoboard ends itself** is logged as a warning every time, with how long the agent ran and why: it gave
  no reply within 60 seconds, its output went past 8 MiB, the daemon is shutting down, its output could not be read,
  or it was still running when its time to exit after the reply ran out (see "What reading a quota leaves behind"
  above), in which case the entry says how long after the reply it was ended and why. A process that exits by itself,
  before or after replying, is not logged here. It is logged even when the daemon's shutdown was the reason, although
  such a reading is not otherwise recorded.

Each entry names the agent, the account's id (none for the default account) and the agent process's pid (none when
nothing was run). Warnings also carry the end of the agent's error output — the last 8 lines of the last 2 KiB of the
part that is kept (see "How a quota is read" above), joined into one line — and the sign-out and failure warnings carry
up to 3 lines the agent printed besides its reply that it marks as an error or a sign-in status (a `type` of `error`
or `auth_status`, or an `error` field). A line from the agent is cut to 300 characters.

What has been seen of each account is kept in the daemon's memory only, and forgotten when the account is removed or
pointed at another directory. After the daemon starts, an account's first reading is never a change of sign-in, though
a first reading that fails is logged as starting to fail.

Everything the agent wrote passes through redaction before it reaches the log: email addresses, keys and tokens in
the shapes services issue them (`sk-…`, JSON Web Tokens and the like), any run of 32 or more key characters mixing
letters and digits, the word after `Bearer` or `Basic`, and the value after the name of a secret — a word that is or
ends in `token`, `secret`, `password`, `passwd`, `apikey`, `api_key`, `authorization` or `credential` — as in
`"refresh_token": "…"`, `password: …` or `token=…`, are masked. It errs towards masking too much. The agents are not
run with any debug output turned on for this: the log holds only what they print anyway.
