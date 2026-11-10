# Account MCP servers and connectors

The daemon can read, for one Claude Code, Codex or Grok Build account, which MCP tools the account has: the **platform
connectors** its platform account holds (claude.ai connectors for Claude Code, ChatGPT Apps for Codex; Grok Build's cannot
be listed) and the **local MCP servers** configured for it, each with its state. This reading is the account's **MCP inventory**. It is taken only when a client
asks for it, through the control request `read_account_mcp_inventory`; the daemon never reads it by itself, and never
holds or broadcasts it. The window reads and shows it in a dialog of the account's own (see "The Connectors and MCP
dialog" below). For a Codex account the daemon can also sign the account in to, and out of, a local MCP server that
uses OAuth (see "Signing an account in to and out of an MCP server" below).

Accounts differ here even when they share the default configuration (see
`docs/product/sharing-the-default-configuration.md`): a platform connector is connected on the platform's side, under
the platform account the account is signed in with, and a local server's definition can be shared while its sign-in
stays the account's own.

## Requesting an MCP inventory

The request names the agent, the account (an account id; none for the agent's default account) and whether to refresh
(off by default). It is answered with `account_mcp_inventory_read`, carrying the inventory, once the agent's CLI has
answered. Since the agent connects to every server to report its state, an answer can take seconds. The CLI steps share
a 60-second read budget, including Claude Code's initial sign-in check and Grok Build's initial listing. The whole
reading has a 90-second deadline, including obtaining the shell environment and waiting for the account's directory. At
that deadline the daemon answers with `timed_out` and cancels the CLI, without waiting for it to stop. An unexpected
stop of the reading is answered as a failure. Slow readings and unexpected failures are recorded in the daemon's
diagnostics, with the elapsed time and, for the overall deadline, whether the wait was for the shell or the CLI.

- **Which agents can be read** is served to every window with the agents, as the capability `mcp_inventory`: Claude
  Code, Codex and Grok Build can be read. A request for an agent that cannot be read is answered as
  `unsupported_agent` (see "When an MCP inventory cannot be read" below).
- **Refused** only when the account is not an account of the agent (`unknown_account`). Every other failure is an
  answer: an inventory whose two lists are empty and which says why it could not be read (see "When an MCP inventory
  cannot be read" below).
- **One reading per account at a time.** A request for an account already being read joins that reading and gets the
  same inventory, whatever refresh either asked for. Readings of different accounts run independently.
- **Refresh** asks the agent to refetch what it caches rather than report what it holds. Only Codex caches anything
  here (see "How a Codex account is read" below); for Claude Code and Grok Build it changes nothing.
- **Nothing is written.** A reading never writes the account's configuration, MCP servers or sign-ins.

## What an MCP inventory holds

Each inventory names the agent and the account and says when it was read. Then it holds:

- **Platform connectors**, each with the agent's own id for it, its name, its state — connected, not connected or
  unknown — and, for Codex, the names of the plugins that bring it (always none for Claude Code). Grok Build has none to
  list: its gateway connectors cannot be read from its CLI.
- **Local MCP servers**, configured by the user or brought by a plugin, each with:
  - its name;
  - its origin: the URL of a remote server, none for a server that is a local command;
  - the plugin that brings it, none for one the user configured;
  - its sign-in state: signed in, needs sign-in, not needed (a local command, or a server that takes no sign-in) or
    unknown;
  - its runtime state: running, failed or unknown, with the reason it failed when the agent gives one;
  - how many tools it offers, where the agent reports that (Codex, and Grok Build for a server that started);
  - whether the account signs in to it with OAuth through the agent's own login command, which Octoboard can run for it
    (see "Signing an account in to and out of an MCP server" below): for Codex see "How a Codex account is read"
    below; never for Claude Code or Grok Build.

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

First `codex mcp list --json`, run the same way and given 15 seconds, says which servers are turned off, whether in
the account's configuration or by the plugin that brings them; those are left out of the inventory, as `codex mcp list`
shows them as disabled while the app server lists them like any other. Only each server's name and whether it is
enabled are read. A listing that fails or cannot be understood turns no server off, and the reading goes on.

Then the app server is asked, after `initialize`:

- `account/read`: an account Codex reports as signed out — no account while it requires an OpenAI sign-in — reads as
  not signed in. An account that needs no OpenAI sign-in is read as any other.
- `app/installed` gives the platform connectors: every App the account has installed, in the order Codex gives them,
  which is the order of the Apps in Codex's own Apps settings. An App is connected when Codex reports it turned on and
  callable (it has a tool the model may use), not connected when it is turned off, and unknown otherwise. An installed
  Codex that does not answer this method leaves the connectors empty rather than failing the reading.
- `mcpServerStatus/list` gives the local MCP servers, with their sign-in and runtime states and their tools. The
  `codex_apps` server is not listed as a server, as its Apps are the connectors; nor is a server Codex reports as
  disabled. Codex reports a runtime state only for a server a session has started, which a reading has none of; without
  one, a server whose tools Codex listed is running, as it had to start to give them, one whose tools could not be
  listed is failed with the reason, and any other is unknown. A server that needs authentication to start reads as
  needing sign-in, with its runtime state unknown. An error answer to this method fails the reading.
  A server signs in with OAuth exactly when Codex reports its authentication as OAuth (`oAuth`) or as not logged in
  (`notLoggedIn`); a server with a bearer token, one that takes no sign-in, and an authentication status Octoboard does
  not know do not.

When the account has installed Apps, a second app server is then asked `app/read` for them, at most 100 to a request
and without their tools: it gives each App's name and the plugins that bring it. An App it does not describe, or a
Codex that does not answer it, keeps the name Codex's tool catalogue carries for it (else its id) and no plugins.

At most 1000 servers are listed, with no further request for more. With refresh on, Codex is asked to refresh the
installed Apps' tools first; otherwise it reports what it last fetched, so an App connected or disconnected on the
platform's side can be missing or stale until a reading is refreshed. The reading does not ask `app/list`, Codex's
catalogue of every App there is: it answers only once Codex has fetched the whole catalogue, page by page, which every
app server it starts does afresh, and which takes half a minute or more.

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

The two commands share the read budget. For the listing, Octoboard sets Claude Code's per-server connection timeout,
`MCP_TIMEOUT`, to 10,000 milliseconds, keeping a positive value of 10,000 or less already set in the login shell.
A server that takes longer to connect is listed as failed by Claude Code. This setting applies to the inventory
listing's process.

The statuses `claude mcp list` prints map onto the states:

| Claude Code's status | A connector | A local server |
|---|---|---|
| Connected | connected | running; sign-in not needed for a local command, unknown for a remote server |
| Needs authentication | not connected | needs sign-in, runtime unknown |
| Failed to connect | unknown | failed, with the reason the line gives; sign-in not needed for a local command, unknown for a remote server |
| Any other | unknown | sign-in and runtime unknown |

Claude Code reports no tool count, and has nothing to refetch. No Claude Code server is one Octoboard can sign in to:
its sign-in is Claude Code's own prompt.

## How a Grok Build account is read

Octoboard runs Grok Build's CLI in the environment of a quota reading (see "How a quota is read" in
`docs/product/account-quota-readings.md`), with the account's directory as `GROK_HOME` (left as the shell has it for the
default account), run from the home directory, so a project's own `.grok/config.toml` and `.mcp.json` take no part (a
`.mcp.json` in the home directory itself would, as the one of the directory it runs from):

1. `grok mcp list --json`, given 15 seconds: the servers configured in the account's `config.toml`, a disabled one
   left out (the protocol has no disabled state, and listing it would read as configured and usable). A URL gives a
   remote server's origin; a command gives none. The listing also carries each server's headers and
   environment variables, which are not read. A listing that exits with a nonzero status or cannot be understood fails
   the reading.
2. `grok mcp doctor --json`: starts every enabled server and runs its checks, which takes a few seconds at most for a
   server that answers. It exits with status 1 when a server fails, with its report on standard output all the same;
   a report that cannot be understood leaves every state unknown rather than failing the reading. A server the report
   has and the listing lacks, which Grok takes from other tools' configuration (Claude Code's `~/.claude.json`, a
   `.mcp.json` in the home directory), is
   listed after the others.

The two commands share the read budget, so the doctor receives the time left after the listing.
Neither command talks to a leader process, so one account's reading never reaches another's through Grok's shared
leader socket. Both create Grok's `docs` and `logs` directories in a home that lacks them.

| What the report says of a server | Its states |
|---|---|
| Healthy | running, with the tools it offers; sign-in not needed for a local command, unknown for a remote server |
| A check failed because the server asked for authentication | needs sign-in, runtime unknown |
| Any other failed check | failed, with that check's name and what it said; sign-in as above |
| Not in the report (the report does not give the server's state) | runtime unknown; sign-in as above |

A server of a plugin is not marked as one. No Grok Build server is one
Octoboard can sign in to: Grok's sign-in is the `/mcps` modal of its terminal interface, and its CLI has no login
command. A Grok Build configuration with a syntax error reads as no servers, as Grok itself reports.

## When an MCP inventory cannot be read

An inventory that could not be read carries one reason, from a closed set clients word themselves, optionally with a
detail (what the agent or the daemon said):

| Reason | When |
|---|---|
| `not_signed_in` | The agent says the account is not signed in (see "How a Codex account is read" and "How a Claude Code account is read" above) |
| `cli_missing` | The agent is not available, or its CLI cannot be started |
| `timed_out` | The CLI exhausted the shared 60-second read budget, an initial command exceeded its own shorter limit (15 seconds for `claude auth status` and `grok mcp list`), or the whole reading reached the 90-second deadline |
| `unsupported_agent` | The agent has no way of listing its MCP servers: none of the supported agents at present |
| `failed` | Anything else, with a detail saying what: the login shell's environment could not be read, the account's directory does not exist, the CLI failed, or the reading was cut short by the daemon shutting down |

A run of the agent's CLI is stopped as every short run of an agent's CLI is, the daemon's shutdown included (see "How a
run of an agent's CLI is stopped" in `docs/product/account-quota-readings.md`). An account that is turned off (see
"Turning an account off" in `docs/product/agent-accounts.md`) is read like any other.

A reading waiting for a change of the account's shared configuration still answers at the overall deadline. Its
background wait can remain until that change releases the directory, when the cancellation takes effect.

## Signing an account in to and out of an MCP server

For a local MCP server the account signs in to with OAuth (see "What an MCP inventory holds" above), the daemon can run
the agent's own command to sign the account in to the server or out of it, through the control requests
`sign_in_account_mcp_server` and `sign_out_account_mcp_server`. Each names the agent, the account (an account id; none
for the agent's default account) and the server, by the name the inventory lists it under. Only Codex has such
commands: a request for another agent, Claude Code and Grok Build included, is refused with `provider_unsupported` (operation `mcp_sign_in`).

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

## The Connectors and MCP dialog

**Connectors and MCP**, an entry of an account's actions menu in Settings' Agent accounts section, with a plug icon,
right after **Settings** (see "The default account" and "The accounts the user owns" in
`docs/product/agent-accounts.md`), opens a modal dialog showing the account's MCP inventory. The entry is there only
for an agent that can be read (see "Requesting an MCP inventory" above); an account of any other agent has no entry,
and nothing is read for it.

The dialog is titled "Connectors and MCP", whichever account it is opened on. The first line of its body is the
account's name, with the email it is signed in with under it as in the account details dialog (see "The account
details dialog" in `docs/product/agent-accounts.md`); its plan, login state, directory and usage are left to that
dialog. The inventory follows. The dialog has no footer, closes, follows the account, tells of a login that succeeds
and closes by itself when the account is removed as the account details dialog does, and hands keyboard focus back
the same way.

**Reading.** The inventory is read, with refresh off, each time the dialog opens. Until the first answer, the body
shows one spinner beside text saying that it is reading the account's connectors and MCP servers. After 15 seconds
that text says the reading is still running and that the agent can take about a minute. The dialog remains usable.

Once an answer has arrived, the account's name line ends with a **Refresh** icon button, which reads again with
refresh on (see "Requesting an MCP inventory" above). During a further reading, the last answer stays shown and the
name line says it is refreshing, beside the pending Refresh button, whose busy state is announced to assistive
technology. The dialog makes one reading at a time. A press of Refresh or an alert's **Try again** while one runs does
nothing, and Try again is unavailable meanwhile.

The window stops waiting after 100 seconds and shows the reading as timed out with **Try again**, even if the daemon
has not answered. A late answer can replace this timeout while the dialog stays open, until a newer reading has
started. An answer that arrives after the dialog has closed is dropped.

Before the Refresh button, while a successful inventory is shown and no further reading runs, the name line says how
long ago it was read, in minutes, hours or days, or that it was just updated when less than a minute has passed.

**What it shows.** The wide dialog shows two groups, **Platform connectors** then **Local MCP servers**, each with a
heading distinct from its explanatory text. An agent with no connector platform and no connectors, Grok Build, has only
the second group. Each nonempty group is a table with aligned state columns. A table scrolls horizontally within its
group when the dialog is too narrow for its columns. A group with no entries says there are none in a compact empty row
(see "The compact row" in `docs/product/empty-states.md`). Tab and Shift+Tab reach the tables' buttons in document
order, including sign-in controls and error marks. WebKit's Option+Tab and Option+Shift+Tab make the same moves.

- **The platform connectors table** has **Name**, **Status** and **Details** columns. Each connector has a state badge
  showing connected, not connected or unknown, green for connected. For Codex, Details names the plugins that bring it.
- **The platform connectors group's heading** ends with a link to the platform's connector settings, opened in the
  browser: **Manage on claude.ai** for Claude Code (`https://claude.ai/customize/connectors`), **Manage on ChatGPT** for
  Codex (`https://chatgpt.com/plugins`). Under the group a line says that, to connect or disconnect connectors, the user
  logs in to that platform with the account this one is signed in with. An agent the window has no presentation of its
  own for (see "How an agent is named and marked" in `docs/product/supported-agents.md`) has neither.
- **The local MCP servers table** has **Name**, **Sign-in**, **Runtime** and **Details** columns, plus **Actions** when
  any server offers sign-in controls. Sign-in shows **Signed in** (green), **Needs sign-in** (amber), or muted text for
  **Not needed** and **Unknown**. Runtime shows **Running** (green), **Failed** (red) or muted **Unknown**. The two
  states are independent. A failed runtime whose agent gave a reason has a separate error mark beside its badge.
  The mark opens that raw diagnostic in a block with a Copy action. Details holds the remote URL, plugin and tool
  count where known, each on its own line. Plugin names in either table are marked as references.
- **A Codex server the account signs in to with OAuth** has a **Sign out** button in Actions while it reads as
  signed in and a **Sign in** button otherwise (see "Signing in and out from the dialog" below). The name announced
  for each button names the server. Servers without this capability have no sign-in controls.
- **A Claude Code server that needs sign-in** has a line in Details saying to run `/mcp` in a session of this account
  to sign it in, since Octoboard cannot sign Claude Code's servers in itself.

**When it cannot be read.** An inventory that could not be read is shown, in place of the two groups, as an alert in
the dialog, red, worded by its reason (see "When an MCP inventory cannot be read" above):

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

### Signing in and out from the dialog

**Sign in** starts a sign-in of the account to the server, and **Sign out** a sign-out (see "Signing an account in to
and out of an MCP server" above), at once: Sign out asks for no confirmation, since signing in again undoes it. While
either runs, its button shows as pending and cannot be pressed. During a sign-in, a line in the entry's Details cell
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
