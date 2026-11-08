# Account MCP servers and connectors

The daemon can read, for one Claude Code or Codex account, which MCP tools the account has: the **platform connectors**
its platform account holds (claude.ai connectors for Claude Code, ChatGPT Apps for Codex) and the **local MCP servers**
configured for it, each with its state. This reading is the account's **MCP inventory**. It is taken only when a client
asks for it, through the control request `read_account_mcp_inventory`; the daemon never reads it by itself, and never
holds or broadcasts it. The window reads and shows it in an account's details dialog (see "The Connectors and MCP
section" below). For a Codex account the daemon can also sign the account in to, and out of, a local MCP server that
uses OAuth (see "Signing an account in to and out of an MCP server" below).

Accounts differ here even when they share the default configuration (see
`docs/product/sharing-the-default-configuration.md`): a platform connector is connected on the platform's side, under
the platform account the account is signed in with, and a local server's definition can be shared while its sign-in
stays the account's own.

## Requesting an MCP inventory

The request names the agent, the account (an account id; none for the agent's default account) and whether to
refresh (off by default). It is answered with `account_mcp_inventory_read`, carrying the inventory, once the agent's
CLI has answered. Since the agent connects to every server to report its state, an answer can take seconds, and up to
a minute (a minute and a quarter for Claude Code, whose sign-in is checked first).

- **Which agents can be read** is served to every window with the agents, as the capability `mcp_inventory`: Claude
  Code and Codex can be read, Grok Build cannot. A request for an agent that cannot be read is answered as
  `unsupported_agent` (see "When an MCP inventory cannot be read" below).
- **Refused** only when the account is not an account of the agent (`unknown_account`). Every other failure is an
  answer: an inventory whose two lists are empty and which says why it could not be read (see "When an MCP inventory
  cannot be read" below).
- **One reading per account at a time.** A request for an account already being read joins that reading and gets the
  same inventory, whatever refresh either asked for. Readings of different accounts run independently.
- **Refresh** asks the agent to refetch what it caches rather than report what it holds. Only Codex caches anything
  here (see "How a Codex account is read" below); for Claude Code it changes nothing.
- **Nothing is written.** A reading never writes the account's configuration, MCP servers or sign-ins.

## What an MCP inventory holds

Each inventory names the agent and the account and says when it was read. Then it holds:

- **Platform connectors**, each with the agent's own id for it, its name, its state — connected, not connected or
  unknown — and, for Codex, the names of the plugins that bring it (always none for Claude Code).
- **Local MCP servers**, configured by the user or brought by a plugin, each with:
  - its name;
  - its origin: the URL of a remote server, none for a server that is a local command;
  - the plugin that brings it, none for one the user configured;
  - its sign-in state: signed in, needs sign-in, not needed (a local command, or a server that takes no sign-in) or
    unknown;
  - its runtime state: running, failed or unknown, with the reason it failed when the agent gives one;
  - how many tools it offers, where the agent reports that (Codex only);
  - whether the account signs in to it with OAuth through the agent's own login command, which Octoboard can run for it
    (see "Signing an account in to and out of an MCP server" below): for Codex see "How a Codex account is read"
    below; never for Claude Code.

Every state comes from what the agent itself reports. A state value the agent reports that Octoboard does not know is
read as unknown; it never fails the reading.

## How a Codex account is read

Octoboard runs `codex app-server` for the account, as for a quota reading (see "How a quota is read" in
`docs/product/account-quota-readings.md`): the login shell's environment, the account's directory as `CODEX_HOME` (left
as the shell has it for the default account), run from the home directory. It differs from a quota reading in what it
leaves on:

- **Apps and plugins stay on**, since Apps reach Codex through its `codex_apps` server and plugins bring Apps and
  servers of their own; the remote plugin catalogue sync and hooks are still turned off for the reading.
- **A non-default account keeps its own MCP sign-ins**: its app server is started with
  `-c mcp_oauth_credentials_store="file"`, as that account's sessions are (see "Codex" in
  `docs/product/per-agent-launch-specifics.md`), so the sign-in states reported are the ones in its own home rather than
  the default account's keychain entries. The default account's app server is given no override.

Codex is asked, after `initialize`:

- `account/read`: an account Codex reports as signed out — no account while it requires an OpenAI sign-in — reads as
  not signed in. An account that needs no OpenAI sign-in is read as any other.
- `app/list` and `app/installed` give the platform connectors. Codex's catalogue names every App there is, so an App is
  listed only when the account can reach it, a plugin brings it, or it is installed. Its state follows whether Codex
  reports it accessible: accessible is connected, not accessible is not connected, no answer is unknown. An installed
  Codex that does not answer one of these two methods leaves that part empty rather than failing the reading.
- `mcpServerStatus/list` gives the local MCP servers, with their sign-in and runtime states and their tools. The
  `codex_apps` server is not listed as a server: its Apps are the connectors. A server that needs authentication to
  start reads as needing sign-in, with its runtime state unknown. An error answer to this method fails the reading.
  A server signs in with OAuth exactly when Codex reports its authentication as OAuth (`oAuth`) or as not logged in
  (`notLoggedIn`); a server with a bearer token, one that takes no sign-in, and an authentication status Octoboard does
  not know do not.

At most 1000 Apps and 1000 servers are listed. Codex caches its App catalogue per home for up to an hour, so an App
connected on the platform's side can be missing from a reading until it is refreshed: with refresh on, Codex is asked
to refetch the catalogue and the installed Apps.

## How a Claude Code account is read

Octoboard runs Claude Code's CLI in the environment of a quota reading (see "How a quota is read" in
`docs/product/account-quota-readings.md`), with the account's directory as `CLAUDE_CONFIG_DIR` (left as the shell has it
for the default account):

1. `claude auth status`, given 15 seconds: an account it reports as not logged in reads as not signed in, and an
   answer that cannot be understood fails the reading.
2. `claude mcp list`, which connects to every configured server and prints one line per server. A line whose name
   starts with `claude.ai ` is a platform connector, named by the rest of it; every other line is a local server, a name
   of the form `plugin:<plugin>:<server>` naming the plugin that brings it. A server reached at a URL is listed with
   that URL as its origin. A listing that exits with a nonzero status fails the reading.

The statuses `claude mcp list` prints map onto the states:

| Claude Code's status | A connector | A local server |
|---|---|---|
| Connected | connected | running; sign-in not needed for a local command, unknown for a remote server |
| Needs authentication | not connected | needs sign-in, runtime unknown |
| Failed to connect | unknown | failed, with the reason the line gives; sign-in not needed for a local command, unknown for a remote server |
| Any other | unknown | sign-in and runtime unknown |

Claude Code reports no tool count, and has nothing to refetch. No Claude Code server is one Octoboard can sign in to:
its sign-in is Claude Code's own prompt.

## When an MCP inventory cannot be read

An inventory that could not be read carries one reason, from a closed set clients word themselves, optionally with a
detail (what the agent or the daemon said):

| Reason | When |
|---|---|
| `not_signed_in` | The agent says the account is not signed in (see the two sections above) |
| `cli_missing` | The agent is not available, or its CLI cannot be started |
| `timed_out` | A run of the agent's CLI did not answer within 60 seconds (15 for `claude auth status`) |
| `unsupported_agent` | The agent has no way of listing its MCP servers: Grok Build |
| `failed` | Anything else, with a detail saying what: the login shell's environment could not be read, the account's directory does not exist, the CLI failed, or the reading was cut short by the daemon shutting down |

A run of the agent's CLI is stopped as every short run of an agent's CLI is, the daemon's shutdown included (see "How a
run of an agent's CLI is stopped" in `docs/product/account-quota-readings.md`). An account that is turned off (see
"Turning an account off" in `docs/product/agent-accounts.md`) is read like any other.

## Signing an account in to and out of an MCP server

For a local MCP server the account signs in to with OAuth (see "What an MCP inventory holds" above), the daemon can run
the agent's own command to sign the account in to the server or out of it, through the control requests
`sign_in_account_mcp_server` and `sign_out_account_mcp_server`. Each names the agent, the account (an account id; none
for the agent's default account) and the server, by the name the inventory lists it under. Only Codex has such
commands: a request for another agent is refused with `provider_unsupported` (operation `mcp_sign_in`).

**How it runs.** For Codex the command is `codex mcp login` or `codex mcp logout`, with the server's name after `--`.
It runs in the background, with no terminal and no window, in the environment a launch of the account has: the login
shell's environment, the account's directory as `CODEX_HOME` (left as the shell has it for the default account), run
from the home directory. A non-default account's run carries `-c mcp_oauth_credentials_store="file"`, as its sessions
and its inventory reading do (see "Codex" in `docs/product/per-agent-launch-specifics.md`), so the token is stored in,
or removed from, the account's own home. The default account's run has no override and uses Codex's default store, as
`codex mcp login` run in a terminal does. For a sign-in Codex opens the browser itself; Octoboard opens nothing and
sends the CLI no input.

**One run per account and server.** A request for the action already running for the same server of the same account
joins that run and gets the same outcome; a request for the other action is refused with `mcp_server_auth_busy`. Runs
for other servers or other accounts are independent.

**Refused before anything runs**, besides `provider_unsupported` and `mcp_server_auth_busy`: an account that is not an
account of the agent (`unknown_account`), an agent that is not available (`agent_not_available`), a login shell
environment that could not be read in time (`shell_environment_timeout`), the agent's CLI not found on the shell's
`PATH` (`binary_not_found`), and an account whose directory is being moved to the Trash (`account_being_removed`).

**How a run ends.** The request is answered with `account_mcp_server_auth_ended` once the CLI has ended, which for a
sign-in can take up to ten minutes. Its outcome is one of:

| Outcome | When |
|---|---|
| `succeeded` | The CLI exited with status 0. A sign-out of a server that holds no sign-in succeeds too |
| `cancelled` | The sign-in was cancelled (below), or the account was pointed at another directory or removed meanwhile; its CLI is killed |
| `timed_out` | A sign-in still running after 10 minutes, or a sign-out after 60 seconds; its CLI is killed |
| `failed` | The CLI exited with another status or could not be started, or the account's directory does not exist. It carries what the CLI said: its standard error with escape sequences stripped and, past 4096 characters, cut to 4095 followed by `…`; when that is empty, the last line of its standard output; else the exit status |

The daemon does not read the inventory again after a run; the client does, once it has the answer.

**When the asking connection drops.** A run is not tied to the window's connection that asked for it. If that connection
closes while the CLI runs (the window reloads, or the client reconnects, for instance after the daemon dropped a client
that stopped reading), the CLI keeps running to its end and its sign-in or sign-out still takes effect; the answer is
sent to no one. The window that asked gets no outcome and shows the request as failed; a request for the same action on
a new connection while the run is still going joins it and gets its outcome.

**Cancelling.** The control request `cancel_account_mcp_sign_in`, naming the agent, the account and the server, ends a
running sign-in and kills its CLI; the sign-in is then answered as `cancelled`. It is refused with
`mcp_sign_in_not_running` when no sign-in of that server runs for the account. A sign-out cannot be cancelled.

**The account's directory during a run.** A run, sign-in or sign-out, counts as a sign-in of the account: a change of
its sharing is refused while it runs, and a run that starts while a change runs waits for it (see "When a change is
carried out" in `docs/product/sharing-the-default-configuration.md`). Pointing the account at another directory, or
removing it, ends its runs; a removal that moves the directory to the Trash also waits for them (see "Logins and agent
reads during the removal" in `docs/product/removing-accounts.md`). The daemon's exit kills the CLIs still running. The
runs are held in memory only.

## The Connectors and MCP section

An account's details dialog (see "The account details dialog" in `docs/product/agent-accounts.md`) ends, for an agent
that can be read, with a **Connectors and MCP** section showing the account's MCP inventory. For an agent that cannot
be read the section is absent and nothing is read.

**Reading.** The inventory is read, with refresh off, each time the dialog opens; nothing is kept from an earlier
opening. The section's heading ends with a **Refresh** icon button, which reads it again with refresh on (see
"Requesting an MCP inventory" above). The dialog makes one reading at a time: a press of Refresh, or of an alert's **Try
again**, while one runs does nothing, and Try again is unavailable meanwhile. While a reading runs the Refresh button
shows as pending, announced to assistive technology as busy, and the last answer stays shown; before the first answer
the section says, beside a spinner, that it is reading the account's connectors and MCP servers. The reading never holds
the dialog up: the rest of it is shown and usable meanwhile. An answer that arrives after the dialog has closed is
dropped.

Before the Refresh button, the heading says how long ago the inventory shown was read, as the dialog's Usage heading
does: "Updated just now" under a minute, otherwise "Updated 3 minutes ago" (in hours or days once it is that old). It is
absent while nothing has been read and while the last answer is a failure: an inventory that could not be read, or a
request that was refused or did not reach the daemon.

**What it shows.** A read inventory is shown as two groups, **Platform connectors** then **Local MCP servers**, each
entry's name on a line with its state badges after it, and what else is known of it in a muted line under it. A group
with no entries says there are none ("No platform connectors", "No local MCP servers") in a compact empty row (see "The
compact row" in `docs/product/empty-states.md`).

- **A platform connector** has one badge for its state: **Connected** (green), **Not connected** or **State unknown**.
  For Codex, the line under it names the plugins that bring it.
- **The platform connectors group's heading** ends with a link to the platform's connector settings, opened in the
  browser: **Manage on claude.ai** for Claude Code (`https://claude.ai/customize/connectors`), **Manage on ChatGPT** for
  Codex (`https://chatgpt.com/plugins`). Under the group a line says that, to connect or disconnect connectors, the user
  logs in to that platform with the account this one is signed in with. An agent the window has no presentation of its
  own for (see "How an agent is named and marked" in `docs/product/supported-agents.md`) has neither.
- **A local MCP server** has a badge for its sign-in state — **Signed in** (green) or **Needs sign-in** (amber), none
  when sign-in is not needed or unknown — and one for its runtime state — **Running** (green), or for a failed server a
  danger status mark reading **Failed**, whose popover says it failed to connect, with the reason the agent gave under
  that where it gave one (see "Failures, warnings and notes marked at their point" in `docs/product/error-alerts.md`);
  none when the runtime state is unknown. The line under it holds the URL of a remote server, the plugin that brings it,
  and, for Codex, how many tools it offers. A plugin's name, there and under a connector, is marked as a reference.
- **A Codex server the account signs in to with OAuth** has, at the end of its name line, a **Sign out** button while
  it reads as signed in and a **Sign in** button otherwise (see "Signing in and out from the section" below). No other
  entry has either: not a platform connector, a server with a bearer token, a local command, a plugin's server that
  does not sign in with OAuth, nor any Claude Code entry. The name announced for each button names the server.
- **A Claude Code server that needs sign-in** has a further line under it saying to run `/mcp` in a session of this
  account to sign it in: Octoboard cannot sign Claude Code's servers in itself.

**When it cannot be read.** An inventory that could not be read is shown, in place of the two groups, as an alert in
the section, red, worded by its reason (see "When an MCP inventory cannot be read" above):

| Reason | What it says |
|---|---|
| `not_signed_in` | The account is not logged in, and its connectors and MCP servers can be read once it is; log in, then try again |
| `cli_missing` | The agent is not available: its CLI could not be found or started; install it or make sure it runs, then try again |
| `timed_out` | The reading timed out: the agent did not answer in time; make sure the servers it connects to are reachable, then try again |
| `unsupported_agent` | The agent cannot list its MCP servers or connectors |
| `failed` | The connectors and MCP servers could not be read; try again in a moment |

The reason's detail, where there is one, is behind the alert's **Details**. Every reason but `unsupported_agent` offers
**Try again**, which reads again with refresh on. A request that is refused, or that does not reach the daemon, is shown
as a red alert saying the connectors and MCP servers could not be read, with the failure's own message, and Try again
where reading again can help. Either alert is announced to assistive technology as it appears, and again when a further
reading fails in the same words (see "Failures shown on the screen" in `docs/product/error-alerts.md`); when the first
reading answers so soon after the dialog opened that the dialog's own announcement would drown it, the alert is
announced a moment later. When a further reading brings the inventory in place of the alert, keyboard focus that was in
the alert, or had fallen to the page, goes to the Refresh button. When an answer replaces what held keyboard focus by
something else — one failure's alert by another's, or the lists by an alert when a further reading fails — focus goes
to the new alert's Try again, or to Refresh when it has none.

### Signing in and out from the section

**Sign in** starts a sign-in of the account to the server, and **Sign out** a sign-out (see "Signing an account in to
and out of an MCP server" above), at once: Sign out asks for no confirmation, since signing in again undoes it. While
either runs, its button shows as pending and cannot be pressed. During a sign-in, a line under the entry's name line
says, beside a spinner, that it is waiting for the user to sign in in the browser, which Codex has opened; the line is
announced to assistive technology as it appears. A **Cancel** button comes before Sign in meanwhile, its announced name
naming the server: it ends the sign-in and its CLI, and shows as pending until the daemon has answered. When the sign-in
ends and Cancel goes, keyboard focus on it moves to the entry's Sign in or Sign out button. A sign-out has neither line
nor Cancel. Each server's run is its own; readings, and runs for other servers, go on meanwhile.

How a run ends shows as follows:

- **Succeeded**: the inventory is read again with refresh on, after the reading under way when there is one, and the
  entry's button stays pending until that reading has answered; the entry then shows the server's new state. Assistive
  technology is told, naming the server, that the account signed in to it or out of it: a moment after the answer, so
  that it is not lost in a focus move, or, when the user was out of the window at the time, a moment after they come
  back.
- **Cancelled**: the entry goes back to its buttons, and nothing is shown.
- **Failed, timed out or refused**: an error alert (see "The error alert" in `docs/product/error-alerts.md`), titled
  that the account could not be signed in to the MCP server, or signed out of it. For a run that failed or timed out its
  body says, naming the server, that the sign-in or sign-out did not finish — for a sign-in that timed out, that it was
  not finished in the browser in time — with what to do under it: to make sure the server is reachable, then sign in or
  out again, or, for a sign-in that timed out, to sign in again and finish in the browser in time; what the CLI said is
  in the alert's details. A refusal, or a request that does not reach the daemon, shows the failure's own message.
- **Connection lost before the answer**: a request whose connection to the daemon closes before its answer arrives is
  shown as a refusal is, an error alert with the failure's own message. The dialog takes the run as over: the entry's
  button is usable again and nothing is read again, although the CLI may still be running in the daemon (see "When the
  asking connection drops" above); the next reading shows the state it left, and a press of the button while the run
  still goes joins it.

A cancel that is refused or does not reach the daemon is shown in an error alert titled that the sign-in could not be
cancelled, even once the dialog has closed, except when the sign-in had already ended as the cancel went out, which
shows nothing: the sign-in's own ending follows.

**Closing the dialog does not end a run.** A sign-in goes on in the daemon until it ends, and a failure, timeout or
refusal still raises its error alert in the window it was started from; a success then reads nothing, since the next
opening reads the inventory anew. The dialog does not know of a run it did not start: opened again while a sign-in
runs, it shows the entry with Sign in as usual, and pressing it joins the running sign-in, its waiting line and Cancel
appearing.
