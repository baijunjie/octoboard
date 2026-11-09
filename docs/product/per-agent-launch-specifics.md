# Per-agent launch specifics

What each agent's launch does differently, and what a user will notice of it. What every launch injects, the launch
environment and what Octoboard never modifies are in `docs/product/launching-agents.md`.

## How each agent is given the injection

What "What is injected on every launch" in `docs/product/launching-agents.md` lists, and the user's choices carried with
it, reaches each agent through that agent's own launch arguments, the environment, or, for Grok Build, files in the
per-session Grok home; never through the project's or the user's own files. All of it is passed again on every launch,
a resume and an account switch included. Octoboard's MCP server is registered under the name `octoboard` with every
agent.

| What | Claude Code | Codex | Grok Build |
|---|---|---|---|
| The session's id | `--session-id <id>` on a new session; `--resume <id>` on a resume | None on a new session (see "Codex" below); `codex resume <id>` on a resume | `--session-id <id>` on a new session; `--resume <id>` on a resume |
| Status hooks | In the JSON passed with `--settings`, which Claude Code merges with the settings files rather than replacing them, so the project's own hooks still run | One `-c hooks.<event>=…` override per event, each run asynchronously; hook trust is handled as described in "Codex" below | A `hooks/` directory of Octoboard's own in the per-session Grok home (see "The per-session Grok home" below) |
| Octoboard's MCP server | `--mcp-config` | `-c mcp_servers.octoboard.command=…` and `-c mcp_servers.octoboard.args=[…]` | An `[mcp_servers.octoboard]` table in the per-session copy of `config.toml` |
| Running Octoboard's tools without asking | An `allow` rule, `mcp__octoboard`, in the `--settings` JSON | `-c mcp_servers.octoboard.default_tools_approval_mode="approve"` | Nothing is passed |
| The agent's time limit on a call to Octoboard's tools | None is set | `-c mcp_servers.octoboard.tool_timeout_sec=660` | `tool_timeout_sec = 660` in that table |
| The role description | `--append-system-prompt`, added to Claude Code's own system prompt | `-c developer_instructions=…`, a developer message added beside Codex's own prompt, which stays as it is | `--rules`, appended to Grok's system prompt |
| The model and reasoning effort | `--model`, `--effort` | `-c model=…`, `-c model_reasoning_effort=…` | `--model`, `--reasoning-effort` |
| An explicit permission choice (see "Approval modes" in `docs/product/per-agent-settings.md`) | `--permission-mode`: `auto` for **Auto**, `default` for **Manual**, `acceptEdits` for **Accept edits**, `plan` for **Plan**, `bypassPermissions` for **Bypass permissions** | `-c default_permissions=…`, `-c approval_policy=…` and `-c approvals_reviewer=…` select a complete preset (see "Codex" below); a resume with Codex's private app server applies it to the conversation through the server instead (see "Codex's private app server" below) | `--permission-mode`: `default` for **Ask**, `auto` for **Auto**, `bypassPermissions` for **Always-approve** |
| Claude compatibility while on (see "Claude compatibility" in `docs/product/per-agent-settings.md`) | Not applicable | A section appended to the `developer_instructions` text, and Claude Code's MCP servers as `-c mcp_servers.<name>.…` overrides (see "Claude compatibility for Codex" in `docs/product/compatibility-between-agents.md`) | `GROK_CLAUDE_*_ENABLED` in the environment, and, where Claude Code reads another global config file than `$HOME/.claude.json`, Claude Code's MCP servers in the per-session copy of `config.toml` (see "Claude compatibility for Grok Build" in `docs/product/compatibility-between-agents.md`) |
| Codex compatibility while on (see "Codex compatibility" in `docs/product/per-agent-settings.md`) | Codex's MCP servers in `--mcp-config`, Codex's skills and custom prompts as a plugin passed with `--plugin-dir`, and a section appended to the `--append-system-prompt` text (see "Codex compatibility for Claude Code" in `docs/product/compatibility-between-agents.md`) | Not applicable | Codex's MCP servers and a `[skills] paths` entry in the per-session copy of `config.toml`, and a section appended to the `--rules` text (see "Codex compatibility for Grok Build" in `docs/product/compatibility-between-agents.md`) |
| Anything else | The status line, and under **Bypass permissions** the acceptance of Claude Code's warning, in the `--settings` JSON | The overrides listed under "Codex" below | `--no-leader` (see "Grok's leader mode" below) |

The 660 seconds are Octoboard's own 10-minute limit on a forwarded call plus a minute (see "The time limit on
Octoboard's tools" below and the Codex list below for why). A value the session leaves to the agent's default — no
model, no effort, **Follow account configuration** for Claude Code and Grok Build or **Custom (config.toml)** for Codex
— passes nothing for it, except that Claude Code may be passed a model then (see "The default model" in
`docs/product/model-and-reasoning-effort.md`). The variables a launch sets for each agent, `CLAUDE_CONFIG_DIR`,
`CODEX_HOME`, `GROK_HOME` and `GROK_THEME` among them, are in "The launch environment" in
`docs/product/launching-agents.md`.

A new session's initial task is passed as the agent's prompt argument. On a resume only Claude Code is given a prompt
argument, and only when Octoboard has an instruction or a prompt to continue for it; Codex and Grok Build are given none
on a resume, and what Octoboard has for them is written into the session instead (see "Messages held until a session
can take them" in `docs/product/message-delivery.md`).

## Claude Code

Octoboard assigns the session's id up front, so a session has an agent-side id from the moment it starts; a session
opened to resume a conversation has that conversation's id. The flags that would drop the project's own permission
rules, hooks or MCP servers are never passed. Octoboard's server takes the name `octoboard` among the session's MCP
servers, so a project's own MCP server of that name is not started in a Claude Code session Octoboard runs. Its status
line is Octoboard's, which runs the user's own in its place; the footer differences that brings for a user without one
are in "Claude Code's status line" in `docs/product/launching-agents.md`.

When Claude Code has **not been trusted with the project's directory**, it ignores that project's own `allow` permission
rules for the session while still applying its `deny` rules — so the session is only ever more restrictive, never less.
Octoboard tells the user so once, when the session starts, with the notice `provider.claude.workspace_untrusted`, naming the project it is in: the `allow` rules stay ignored
until Claude Code's trust confirmation is answered, and Octoboard presses that confirmation once the user has agreed to
it (see `docs/product/folder-trust.md`). It stays silent unless the user's own configuration says explicitly that the
directory is untrusted, so a configuration it cannot read leaves them alone rather than warning on every launch; and it
stays silent when a directory above the session's is recorded as trusted, which Claude Code honours without asking while
leaving the directory's own record saying untrusted. The trust state is read from the global config file this launch's
Claude Code will itself read: `.claude.json` inside the Claude Code config directory in effect — the session's own, or
else a `CLAUDE_CONFIG_DIR` the user's shell exports — and `~/.claude.json` when there is none. A config directory
without that file yet produces no warning; `~/.claude.json` is not consulted in its place.

An explicit approval mode chosen for Claude Code in Settings is passed on every launch of any of its accounts, a resume
included, taking precedence over a saved mode. Following account configuration passes none, so Claude Code decides from
its native configuration and resume rules, including their conditions for restoring a saved mode. Under **Bypass
permissions**, every launch also tells Claude Code that its own warning for that mode has been accepted, so the blocking
screen it would otherwise raise on a config directory's first use of the mode never appears; this holds for that launch
alone, and nothing is written into the account's configuration files. The choices are in "Approval modes" in
`docs/product/per-agent-settings.md`.

Octoboard's own tools are pre-approved for the session on every launch, a resume included, under every approval mode,
following the account's configuration included, so a session's calls to them, a console session's orchestration
included, raise no Claude Code permission prompt. The launch carries one `allow` permission rule naming Octoboard's MCP
server, `mcp__octoboard`; like the acceptance of the warning above, it holds for that launch alone and nothing is
written into any settings file. The rule covers that one server's tools alone: every other MCP server, whatever its
name, Bash, file edits and every other tool keep the approval mode in effect. It adds to the user's and the project's
own `allow` rules rather than replacing them, and a `deny` or `ask` rule of the user's or the project's that names one
of Octoboard's tools still wins over it — a `deny` takes that tool away from the session altogether. Under **Auto**,
Octoboard's tools are therefore not put to Claude Code's auto-mode classifier either. What a tool itself refuses, and
the requests Octoboard puts to the user before acting on a session's behalf (see `docs/product/session-requests.md`),
are unaffected.

## Codex

Octoboard does not assign Codex a session id in advance. A resumed session has its conversation's id from the start;
a new session learns its agent-side id from the first prompt submitted in it, so one opened without a task has none
until then; with Codex's private app server an automatic message can be that first prompt, without the user having to
submit the draft in the composer first. A session of Codex 0.166.0 or later runs with a private app server of its own
(see "Codex's private app server" below). Any other session, and one whose server could not be started, runs as one
terminal process, `codex` (`codex resume <id>` on a resume), whose own command line carries every override Octoboard
passes, all of them again on a resume.

On a Windows daemon host, where the `codex` found on the `PATH` is a package manager's shim, Octoboard runs the native
Codex program (`codex.exe`) the installed package ships instead of that shim, for the session as for its account's
reads and sign-in. It finds the package through each package manager's install layout:

- npm: beside its shims in the global prefix, or in a project's `node_modules` for a shim in its `node_modules\.bin`;
- pnpm: under `global\<n>\node_modules` below its shims, the newest `<n>` first, with Codex's platform package also
  looked for beside the store directory the package's link leads to;
- Yarn classic: shims in `Yarn\bin`, packages in `Yarn\Data\global\node_modules`;
- Bun: the program shim `.bun\bin\codex.exe`, packages in `.bun\install\global\node_modules`;
- Volta: shims in `Volta\bin`, the package under `Volta\tools\image\packages`.

Only a `codex.exe` built for an architecture the machine runs is taken, whichever architecture Octoboard's daemon
itself is built for: on an x64 machine only an x64 build; on an ARM64 machine the ARM64 build, or, where the install
has only the x64 build, that one, run emulated. A shim whose package ships no such program, and a script shim (`.cmd`,
`.bat`, `.ps1`) whose package is in none of these layouts, are refused with `provider.codex.native_program_missing`,
whose message says Codex's install is missing its native program and to reinstall Codex: the launcher script is never
run in the native program's place. A `codex.exe` found in none of these layouts is run as it is.

The launch conditions that are visible:

- project trust is not passed: in a git repository Codex has not been told to trust, its own folder-trust confirmation
  appears, and Octoboard presses it under the user's permission (see `docs/product/folder-trust.md`);
- Codex's check for a newer version at startup is turned off for that launch (`check_for_update_on_startup=false`), so
  that its update prompt does not come up ahead of the trust confirmation; the user's own setting is left as it is;
- the items of Codex's terminal title are set for that launch to its activity, the thread's name and the project's name
  (`tui.terminal_title=["activity","thread-name","project-name"]`), overriding the user's and a trusted project's own
  `tui.terminal_title` inside Octoboard's terminal. Codex's first title is what tells Octoboard its session has started,
  and its activity title is what raises the hand for an action Codex waits on (see "Messages held until a session can
  take them" in `docs/product/message-delivery.md` and "The raised hand" in `docs/product/session-statuses.md`), so a
  configuration that draws no title would otherwise hold the session's messages for good;
- a resume starts in the session's own working directory without asking (`tui.resume_cwd="current"`), instead of Codex
  offering to switch to the directory the conversation last ran in;
- Codex's prompt to try a newer model is suppressed for every model that the models cache (`models_cache.json`) in the
  Codex home that launch uses lists an upgrade for (`notice.model_migrations`), and nothing is passed when that cache is
  missing, unreadable or lists none; its nudge to switch to another model as a rate limit nears is hidden
  (`notice.hide_rate_limit_model_nudge=true`). Each of these screens, like the working-directory choice above, would
  take a message written into the session as its answer; one that comes up all the same holds the session's messages
  until the user has answered it (see "Messages held until a session can take them" in
  `docs/product/message-delivery.md`). Like the title items and the working directory, these hold for that launch alone;
  nothing is written into the user's configuration;
- the review Codex would otherwise raise for Octoboard's injected hooks is bypassed for that launch
  (`--dangerously-bypass-hook-trust`), a resume included, which can produce two warning lines in the session's own
  output; a resume with the private app server, where Codex does not honour that bypass, trusts Octoboard's hooks as
  described in "Codex's private app server" below. Nothing is recorded as trusted in the user's files. The bypass
  covers hook review only; it does not weaken the sandbox or the approval policy.
- Octoboard's own tools are pre-approved for the session, so a console session's orchestration calls raise no approval
  dialog. This covers Octoboard's tools alone; every other tool, the sandbox and the approval policy are untouched.
- Codex's own time limit on a call to Octoboard's tools (300 s by default) is raised to 660 seconds, above Octoboard's
  own limit on a forwarded call (the 10 minutes in "Waiting for the answer" of `docs/product/session-requests.md`), so
  that a call that waits for the user, such as a session request, can return its result. This is the limit on
  Octoboard's tools alone; no other tool's limit changes.
- a session of a non-default Codex account keeps the OAuth tokens of its MCP servers in its own Codex home
  (`-c mcp_oauth_credentials_store="file"`, so they land in `$CODEX_HOME/.credentials.json`), whether it is started,
  resumed, switched onto the account or handed off to it, and whatever the account's sharing state. Codex's default
  store on macOS is the keychain, under a key made from the server's name and URL alone, not from `CODEX_HOME`, so
  without the override every account would sign in to a server as one identity and refresh one token uncoordinated;
  and the store cannot be set in the account's `config.toml`, which a sharing account has in common with the default
  account. A session of the default account is passed nothing and keeps Codex's default store, as `codex` run in a
  terminal does. `codex` run by hand under a non-default account's home does not carry the override and reads the
  default account's keychain entry instead. A token already in the keychain is not moved: a non-default account signs
  in to the server again, which it can do from its Connectors and MCP dialog. Reading the account's MCP servers carries
  the same override (see "How a Codex account is read" in `docs/product/account-mcp-servers-and-connectors.md`), and so does
  signing the account in to or out of a server from Octoboard (see "Signing an account in to and out of an MCP server"
  in the same doc).

Automatic messages go into Codex's own queue on a launch with the private app server, leaving the composer untouched
(see "Messages through Codex's own queue" in `docs/product/message-delivery.md`). On any other launch they reach Codex
through its terminal, like any other agent's: Octoboard uses no stash of Codex's to set a draft aside, so they take the
guarded terminal delivery in "Messages held until a session can take them" in `docs/product/message-delivery.md`.

An explicit Codex permission preset chosen in Settings applies on every launch of any of its accounts, a resume
included. Each is passed as three per-launch configuration overrides:

| Choice | `default_permissions` | `approval_policy` | `approvals_reviewer` |
|---|---|---|---|
| Ask for approval (`user`) | `:workspace` | `on-request` | `user` |
| Approve for me (`auto_review`) | `:workspace` | `on-request` | `auto_review` |
| Full access (`full_access`) | `:danger-full-access` | `never` | `user` |

The built-in profile replaces the account's own permission profile and its legacy sandbox roots and network settings for
that launch. The preset also replaces permissions saved with a resumed conversation. The **Custom (config.toml)** choice
passes none of these overrides and leaves Codex's native new-session and resume behavior intact. Its meaning and
lifetime rules are in "Approval modes" and "Provider settings" in `docs/product/per-agent-settings.md`.

For status reporting under Custom, whether Codex reviews approval requests itself is read from `config.toml` in the
Codex home that launch uses: the session's Codex config directory, else a `CODEX_HOME` the user's shell exports, else
`~/.codex`. A trusted project's own `.codex/config.toml`, which Codex itself layers over that file, is not consulted for
this status hint. Actual action prompts are observed separately through Codex's terminal activity title.

### Codex's private app server

**Which launches have one.** Every launch of Codex 0.166.0 or later, a resume included. The version is read from the
install: the `codex-package.json` of a standalone release or the `package.json` of the
`@openai/codex` package, found in one of the few directories above the program; on a Windows daemon host, where
neither is found, Codex's own `--version` answer. A version with a suffix after `-` or `+` (a platform package's)
counts by the release before it, and a metadata file that cannot be read for a version is passed over. An older
version, and an install whose version cannot be told this way (a Homebrew one carries no such metadata), get the
ordinary launch: the terminal alone.

**How the session runs.** Octoboard first starts `codex app-server` for the session, then the terminal, which attaches
to it (`--remote`). Both are given the launch's `-c` overrides (the hooks, Octoboard's MCP server, the role
description, the model and effort, the permission preset, the title and notice settings listed above), the same
environment and the same working directory. The terminal is also told the session's working directory (`--cd`), so
Codex's folder-trust confirmation appears as it does at an ordinary launch, and Octoboard presses it under the user's
permission as usual (see `docs/product/folder-trust.md`). On a resume:

- the terminal is given none of the permission preset's overrides, since Codex refuses permission overrides when
  resuming a remote conversation. An explicit preset is applied to the conversation through the server before the
  terminal attaches, so it takes precedence over the conversation's saved choice as it does at an ordinary resume;
  under **Custom (config.toml)** the saved choice stays;
- Codex does not honour the hook-trust bypass, so Octoboard trusts only its own generated hook command, by the
  fingerprint Codex gives it, through overrides of that server start alone. It records no trust in the user's files
  and grants none to the user's other hooks. Hook trust does not weaken the sandbox or the approval policy.

**Where the server listens.** On macOS and Linux, a Unix socket in a private directory under `/tmp`, removed with the
server. On a Windows daemon host, a loopback WebSocket port, which any local process can reach, guarded by a random
capability token: the server is given only the token's SHA-256, and the terminal reads the token from an environment
variable of its own, `OCTOBOARD_CODEX_REMOTE_TOKEN`.

**Requests of the server's.** Approvals and the other requests the server sends a client are answered in the terminal
alone: Octoboard's own connections to the server, which carry its messages, leave them unanswered. A turn that runs on a
conversation Octoboard had the server load again (see "Messages through Codex's own queue" in
`docs/product/message-delivery.md`) while the terminal shows another task, and a turn a message started before a
resumed session's terminal attached, send their approval requests to Octoboard's connection while it is open, which
never answers them; the request then waits. Codex's agent command center lists that task as "Needs input", and opening
it shows the approval in the terminal, where it can be answered (measured on Codex 0.166.0).

**Its lifetime.** The server belongs to the running session. When the terminal exits, the server is ended. When the
server exits by itself, the terminal is ended too, as if the agent had exited by itself rather than been ended by
Octoboard, since it would otherwise wait for the server to come back for good. The server is in the launch's process
containment and leads a process session of its own (on a Windows daemon host, a job of its own inside the launch's
job); what that means for the background-process lists is in "Automatic stopping and its boundaries" in
`docs/product/background-processes.md`.

**When it cannot be used.** If the server does not start listening within 15 seconds, exits first, or cannot be
initialised, or, on a resume, the hook trust or the permission preset cannot be applied, the session is launched the
ordinary way: the terminal alone, with every override on its command line, and its automatic messages take the guarded
terminal delivery. Nothing queued for the session is lost by it. When a single message cannot go into Codex's queue,
see "Messages through Codex's own queue" in `docs/product/message-delivery.md`.

## Grok Build

An explicit permission mode chosen in Settings applies to every account on fresh launches and resumes through
`--permission-mode`: Ask uses `default`, Auto uses `auto`, and Always-approve uses `bypassPermissions`. Following
account configuration passes no flag and leaves the mode to Grok Build's native configuration and resume rules. When the
native auto-mode feature is unavailable, Grok Build falls back to Ask. The launch neither edits the source configuration
nor writes a permission mode back from the session's copy, even when the source configuration is shared through a
symlink. The choices and confirmations are in "Approval modes" in `docs/product/per-agent-settings.md`.

Always-approve leaves deny rules and hooks active; a session can still need the user's attention. It does not disable
Grok Build's sandbox or bypass Octoboard's refusal of an incompatible sandboxed launch, described below. Octoboard
passes no separate grant to pre-approve its own tools for Grok Build.

Grok Build is available on a Windows daemon host as on macOS. It is installed there with x.ai's PowerShell installer
(`irm https://x.ai/cli/install.ps1 | iex`), and its `grok` binary is looked up on the launch environment's `PATH` like
every agent's (see "The launch environment" in `docs/product/launching-agents.md`). A Windows host that makes no file
links still runs Grok Build sessions (see "On a host without file links" below).

### The per-session Grok home

A Grok session runs against a per-session Grok home that Octoboard builds afresh on every launch from a **source home**:
the session's Grok Build config directory when it holds one, else a `GROK_HOME` the user's shell exports, else
`~/.grok`. The per-session home links back to the source home's entries, so the login and the session records are the
source home's own, and a session Octoboard started stays resumable from the user's own `grok` command run against that
same home — for a console's Grok Build config directory, `GROK_HOME=<that directory> grok`. Each link points at the
source entry's fully resolved target. When the source entry is itself a link, that target is the final file or
directory, not the link, so the session reaches it through one link. For an account that shares the default
configuration, a save from the session of a linked shared file — `AGENTS.md`, `pager.toml` or `disabled-hooks` — updates
the default directory's resolved file and leaves the account's own link in place. A dangling source link has nothing to
resolve to, and the session's link points at that source entry.

Some of its entries are **copies** taken at launch rather than links. Two are files whose copying has consequences worth
knowing:

- the Grok configuration file: an edit to the source file while a session is running is not picked up by that session.
  With a plain source `config.toml`, settings the session saves are lost when the session ends. With a source
  `config.toml` that is a link to an existing file — an account sharing the default configuration, or any home whose
  `config.toml` the user has linked elsewhere, such as into a dotfiles repository — selected settings are carried into
  the file the link leads to after the process exits and before the per-session home is removed, leaving the link in
  place: `ui.theme`, `plugins.enabled`, `plugins.disabled`, and `models.default`. Only values that differ from the copy
  taken at launch are carried; each plugin list is carried whole, an empty one included, but removing a key does not
  remove it from the linked file. A model equal to the model passed at launch is excluded, including when the user
  explicitly selects it again. Reasoning effort, Octoboard's MCP entry, what a compatibility switch adds (see
  `docs/product/compatibility-between-agents.md`) and every other setting stay in the copy. The
  next launch reads the source file afresh.

  The link must still lead to the same file it led to at launch: a link removed, broken or redirected to another file
  receives nothing. Sessions exiting under the same daemon merge only what they changed into the current file: changes
  to different settings survive, and for the same setting — a plugin list counting as one — the last exit wins. A
  session that changed nothing does not restore an earlier value. A simultaneous save from an external Grok process or
  editor can still race with this. A failure to write is logged, and the per-session home is still removed, so the
  settings it would have carried are lost; so are they if the Octoboard daemon itself stops while the session is
  running.
- the trust store, `trusted_folders.toml`: copied as it is, with nothing added, so a folder the user has not trusted
  Grok with shows Grok's own trust confirmation. The one entry Grok writes into the copy when that confirmation is
  accepted is the exception to its being lost: it is carried into the source home's store (see "Carrying Grok Build's
  trust record" in `docs/product/folder-trust.md`).

Grok's built-in bundles and its sandbox files — `bundled/`, `bundles/`, `hooks-paths` and `sandbox.toml` — are copies
taken on every launch as well, a directory copied whole, whichever of them the source home holds. A bundle update Grok
downloads during a session lands in Octoboard's copy and is not kept; the next launch copies whatever the user's own
`grok` has fetched since.

Three entries of the source home are left out altogether. Its `hooks/` directory is not linked: the per-session home's
`hooks/` holds Octoboard's status hooks alone, so hooks the user keeps in the Grok home's own `hooks/` directory do not
run in a session Octoboard starts, while a project's own hooks still do (see "A project's hooks and instruction files"
below). Its `requirements.toml` and `managed_config.toml` are not carried either (see "Grok's own sandbox" below).

On a Windows daemon host, a SQLite database the source home holds is a copy even where the host makes file links, so
what a session writes into it is not kept.

### On a host without file links

Where the daemon's host makes no file links — Windows with Developer Mode off and Octoboard not elevated — a Grok Build
session still runs, against a per-session Grok home built differently from the one above:

- the source home's directories are still linked, the session records' `sessions/` among them, so a session stays
  resumable from the user's own `grok`;
- its other files are copies taken at launch, the session's own, and what the session writes into them is not kept;
  lock files and temporary files are left out, for Grok to create its own;
- a directory copied into the home (`bundled/`, `bundles/`) leaves out only Grok's own lock and temporary files: a
  bare `.lock`, a `<file>.lock` beside the `<file>` it guards (`auth.json.lock`), `.tmp` followed by six letters or
  digits (`.tmp2tPTzx`), and `.<number>.<number>.tmp` (`.59335.0.tmp`). Files that only look alike are copied: a
  `uv.lock`, `Cargo.lock`, `yarn.lock` or `bun.lock` with no file of its own name beside it, and a name starting
  `.tmpl`;
- `auth.json` is left out as well: the launch sets `GROK_AUTH_PATH` to the source home's `auth.json`, so Grok uses the
  user's own login file rather than a copy of it. A refresh of an expired token in the session is written to that
  file, with Grok's `auth.json.lock` created beside it, as it is through the link on other hosts, and a sign-in in the
  session is expected to be written there too, so the user's own `grok` stays signed in and never holds a refresh
  token the session has spent. Where the launch environment already holds
  `GROK_AUTH_PATH`, that value is left as it is;
- the Grok configuration file and the trust store are copies under the rules above: with a plain source `config.toml`,
  settings saved in the session stay in the session's copy, and the trust entry Grok writes when its confirmation is
  accepted is carried into the user's own `trusted_folders.toml`.

A host that makes file links builds the home as described above.

### Grok's own sandbox

**A Grok Build launch that would run under Grok's own sandbox is refused** before the per-session home is built, with
the code `provider.grok.sandbox_unsupported`, whose message names the sandbox profile and what sets it. The sandbox lets Grok
write only to its own home, the working directory and temporary directories, judged by where links really lead, and the
per-session home's login and session records are links back into the source home, so a sandboxed session could not even
create its conversation. The alert's remedy is to turn the sandbox off where it is set, or to have whoever manages that
setting do so; a conversation begun under the sandbox keeps it, so it is to be continued in Grok itself. A profile of
`off` is no sandbox. Which profile applies is read as Grok layers it. The system-wide files below are read from
`/etc/grok` on macOS and Linux only: Grok Build has no system-wide configuration directory on Windows, so neither is read
there, and a file under `\etc\grok` on the daemon's current drive refuses nothing.

- `[sandbox] profile` in the system-wide `/etc/grok/requirements.toml` is a pin that takes precedence over everything:
  any other profile there refuses, and `off` there allows the launch whatever else is set;
- on a resume, the profile Grok saved with the conversation (its `summary.json`'s `sandbox_profile`), or a
  `GROK_SANDBOX` in the launch environment (see "The launch environment" in `docs/product/launching-agents.md`), refuses
  when either is not `off`, since Grok restores a saved profile and refuses a resume that asks for a different one; a
  resume whose conversation has no saved profile is judged like a new session;
- otherwise the first of these that sets a profile decides: `GROK_SANDBOX` in the launch environment, then `[sandbox]
  profile` in the source home's `config.toml`, then the fleet default in the system-wide
  `/etc/grok/managed_config.toml`.

Not read: macOS managed preferences for Grok, so a sandbox set only there is not refused up front and the session fails
inside Grok instead; and the source home's own `requirements.toml` and `managed_config.toml`, which are left out of the
per-session home, so Grok never loads them for an Octoboard session.

### Grok's leader mode

**Every Grok Build launch passes `--no-leader`** — a new session, a resume, and the relaunches of an account switch and
a model change alike — so the session's agent runs in the session's own process even where the Grok configuration turns
leader mode on (`[cli] use_leader = true`). The session neither starts a shared Grok leader process nor connects to one
already running. Under leader mode the terminal would be only a client of that leader, and stopping the session would
kill the leader outright mid-turn, so Grok would neither end the turn nor shut the session down cleanly; with
`--no-leader`, stopping the session ends the agent's work through Grok's own shutdown. Leader mode is not available
in a session Octoboard starts.

### The time limit on Octoboard's tools

Octoboard's own MCP server is declared in that copy of the configuration file, and its entry sets **Grok Build's own
time limit on a call to Octoboard's tools**, to 660 seconds, above Octoboard's own limit on a forwarded call (the 10
minutes in "Waiting for the answer" of `docs/product/session-requests.md`), so that a call which waits for the user,
such as a session request, is ended by Octoboard's limit and not by Grok's — Grok cancels a call it gives up on, and a
cancellation withdraws such a request. The limit is set on Octoboard's own entry alone; no other MCP server of the
user's is touched, and the injected MCP entry is never carried into the user's own configuration.

### Grok Build's theme

**Grok Build's theme follows the session terminal's appearance.** Grok's own default theme is a dark one, which would
paint dark bands across a terminal in the light appearance, so every Grok Build launch sets `GROK_THEME`:

| The launch | `GROK_THEME` |
|---|---|
| asked for from a window in the light appearance | `grokday` |
| asked for from a window in the dark appearance | `groknight` |
| with no window behind it | `auto`, which has Grok follow the operating system's appearance |

A window asks for a launch when it opens a session (with the new-session dialog or with defaults), resumes one, switches
one to another account or hands one off, and it sends the appearance its Light, Dark or System choice has resolved to at
that moment (see "Light, dark and follow the system" in `docs/product/appearance.md`). No window is behind a launch an
agent asks for through Octoboard's tools (opening or reopening a session, handing one off, switching one to another
account), a session opened, reopened or resumed by an approved session request, or an archived owner relaunched on the
way while a session is resumed.

The appearance is taken at each launch and not kept with the session: a running Grok session keeps the theme it was
launched with when the window's appearance changes later, and its next launch picks the theme afresh. `GROK_THEME` takes
precedence over the theme set in the Grok configuration file at startup, and does not save a theme into that file. An
explicit `/theme` saves the chosen theme in the session copy; when the source `config.toml` is linked, it is carried
into the linked file on exit under the rules in "The per-session Grok home" above, even when it is the theme the
appearance picked. The next launch applies the terminal's appearance again. Claude Code and Codex are told nothing of
the appearance.

### A Grok Build config directory

A Grok Build config directory must **already be a Grok home, one Grok has been run against**. Only entries that already
exist in the source home are linked; for an empty directory, Grok creates its login and session records inside the
per-session home, which is discarded with the session's process, so neither the login nor the conversation persists.
Octoboard does not check this when the directory is set — a user pointing it at a directory they are about to create
should not be stopped. It is checked at **launch** instead, and a Grok Build session pinned to a directory that is not
an initialized Grok home is refused there (`provider.grok.home_not_initialized`), naming the directory and what is wrong with it; a session on the default
account is never held to this, since that directory is the user's own existing setup.

### A project's hooks and instruction files

Grok Build loads **a project's own hooks only inside a git repository**: it locates a project by walking up for a `.git`
directory, and in a directory without one it loads none of the project's hooks. Its instruction files need no repository
— in a directory that is not one, Grok reads those in the working directory alone — but, like the project's hooks and
MCP servers, they load only once the folder is trusted (see `docs/product/folder-trust.md`). That is how a Grok console
session reads the console's instruction file (see "The console session's instruction file" in
`docs/product/launching-agents.md`).
