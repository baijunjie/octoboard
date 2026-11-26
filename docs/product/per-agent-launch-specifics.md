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
| Status hooks | In the settings passed with `--settings`, a JSON file Octoboard writes for the launch in the session's scratch directory, owner-only since the hooks' address carries the key (see `docs/product/who-can-reach-the-daemon.md`); Claude Code merges them with the settings files rather than replacing them, so the project's own hooks still run | One `-c hooks.<event>=…` override per event, each run asynchronously; hook trust is handled as described in "Codex" below | A `hooks/` directory of Octoboard's own in the per-session Grok home (see "The per-session Grok home" below) |
| The cross-project guard, for a project session launched while **Keep sessions to their own project** is on (see "The edit check" in `docs/product/keeping-sessions-to-their-own-project.md`) | A second `PreToolUse` group in the `--settings` file, beside the status hooks', matching Claude Code's file-edit tools (listed in "Which calls it checks" there): a synchronous command line, run through the shell Claude Code runs commands through (Git Bash on Windows), that runs `octoboardd guard-hook --list <list>` and then exits 0 whatever it did | A second `PreToolUse` group in the same `-c hooks.PreToolUse=…` override, matching `apply_patch` and run synchronously, whose command is a per-session script in the session's scratch directory (see `docs/product/files-octoboard-owns.md`), that runs the same mode and exits 0; its trust on a resume with the private app server is described in "Codex's private app server" below | A second `PreToolUse` group in Octoboard's hooks file in the per-session Grok home, matching Grok's edit tools under all their names, whose command is that per-session script |
| Octoboard's MCP server | `--mcp-config` | `-c mcp_servers.octoboard.command=…` and `-c mcp_servers.octoboard.args=[…]` | An `[mcp_servers.octoboard]` table in the per-session copy of `config.toml` |
| Running Octoboard's tools without asking | An `allow` rule, `mcp__octoboard`, in the `--settings` file | `-c mcp_servers.octoboard.default_tools_approval_mode="approve"` | Nothing is passed |
| The agent's time limit on a call to Octoboard's tools | None is set | `-c mcp_servers.octoboard.tool_timeout_sec=660` | `tool_timeout_sec = 660` in that table |
| The role description | `--append-system-prompt`, added to Claude Code's own system prompt | `-c developer_instructions=…`, a developer message added beside Codex's own prompt, which stays as it is | `--rules`, appended to Grok's system prompt |
| The model and reasoning effort | `--model`, `--effort` | `-c model=…`, `-c model_reasoning_effort=…` | `--model`, `--reasoning-effort` |
| An explicit permission choice (see "Approval modes" in `docs/product/per-agent-settings.md`) | `--permission-mode`: `auto` for **Auto**, `default` for **Manual**, `acceptEdits` for **Accept edits**, `plan` for **Plan**, `bypassPermissions` for **Bypass permissions** | `-c default_permissions=…`, `-c approval_policy=…` and `-c approvals_reviewer=…` select a complete preset (see "Codex" below); a resume with Codex's private app server applies it to the conversation through the server instead (see "Codex's private app server" below) | `--permission-mode`: `default` for **Ask**, `auto` for **Auto**, `bypassPermissions` for **Always-approve** |
| Claude compatibility while on (see "Claude compatibility" in `docs/product/per-agent-settings.md`) | Not applicable | A section appended to the `developer_instructions` text, and Claude Code's MCP servers as `-c mcp_servers.<name>.…` overrides (see "Claude compatibility for Codex" in `docs/product/compatibility-between-agents.md`) | `GROK_CLAUDE_*_ENABLED` in the environment, and, where Claude Code reads another global config file than `$HOME/.claude.json`, Claude Code's MCP servers in the per-session copy of `config.toml` (see "Claude compatibility for Grok Build" in `docs/product/compatibility-between-agents.md`) |
| Codex compatibility while on (see "Codex compatibility" in `docs/product/per-agent-settings.md`) | Codex's MCP servers in `--mcp-config`, Codex's skills and custom prompts as a plugin passed with `--plugin-dir`, and a section appended to the `--append-system-prompt` text (see "Codex compatibility for Claude Code" in `docs/product/compatibility-between-agents.md`) | Not applicable | Codex's MCP servers and a `[skills] paths` entry in the per-session copy of `config.toml`, and a section appended to the `--rules` text (see "Codex compatibility for Grok Build" in `docs/product/compatibility-between-agents.md`) |
| Simple shell commands while on (see "Simple shell commands" in `docs/product/per-agent-settings.md`) | One instruction appended to the `--append-system-prompt` text, after the Codex compatibility section when that is on | Not applicable | Not applicable |
| Anything else | The status line, and under **Bypass permissions** the acceptance of Claude Code's warning, in the `--settings` file | The overrides listed under "Codex" below | `--no-leader` (see "Grok's leader mode" below) |

| Agent | Hook or status injection | MCP tools and role description |
|---|---|---|
| Antigravity CLI | Hooks in the launch’s private configuration and native OSC 7501 status reports. | The launch’s MCP file and global rule file in its private home. |
| GitHub Copilot CLI | A plugin passed with `--plugin-dir`. | `--additional-mcp-config` and an instructions directory for the role. |
| OpenCode | A plugin named in the launch’s inline configuration. | The same configuration supplies the MCP server and references the role file. |
| Qwen Code | Hooks in the launch’s system settings. | `--mcp-config` and `--append-system-prompt`. |
| Pi | An extension loaded for the launch, together with program status reports. | The extension supplies the Octoboard tools. `--append-system-prompt` names the launch’s role file. |

A launch started without Octoboard's additions (see "The plain launch" in `docs/product/launching-without-octoboards-additions.md`) passes
none of the above, only this:

| Agent | Resume | Model | A new session's task |
|---|---|---|---|
| Claude Code | `--resume <id>` | `--model` | the prompt argument, after a bare `--`, shaped as below; on a resume, the instruction or prompt to continue Octoboard has for it, the same way |
| Codex | `codex resume <id>` | `-c model=…` | after a bare `--` |
| Grok Build | `--resume <id>` | `--model` | after a bare `--` |
| Antigravity CLI | `--conversation <id>` | `--model` | the session's first message |
| GitHub Copilot CLI | `--resume=<id>` | `--model` | `--interactive=<task>` |
| OpenCode | `--session <id>` | `--model` for OpenCode 1.x only; OpenCode 2 is passed no model | the session's first message |
| Qwen Code | `--resume <id>` | `--model` | `--prompt-interactive=<task>` |
| Pi | `--session <id>`, after `--session-dir <dir>` where the record is in a directory an earlier launch kept (see "Pi" below) | `--model` | after a bare `--`; a task that starts with `@` as the session's first message |

The 660 seconds are Octoboard's own 10-minute limit on a forwarded call plus a minute (see "The time limit on
Octoboard's tools" below and the Codex list below for why). A value the session leaves to the agent's default — no
model, no effort, **Follow account configuration** for Claude Code and Grok Build or **Custom (config.toml)** for Codex
— passes nothing for it, except that Claude Code may be passed a model then (see "The default model" in
`docs/product/model-and-reasoning-effort.md`). The variables a launch sets for each agent, `CLAUDE_CONFIG_DIR`,
`CODEX_HOME`, `GROK_HOME` and `GROK_THEME` among them, are in "The launch environment" in
`docs/product/launching-agents.md`.

A new session's initial task is passed as the agent's prompt argument, exactly as it was given, last on the command
line, after the flags and a bare `--`, so a task that begins with `-` is taken as the prompt and never as an option.
Octoboard removes no invisible characters from it: Claude Code removes the ones it treats as invisible itself and sends
the prompt at once. A task that is nothing but whitespace once its control and invisible characters are ignored is no
task: nothing is passed, and the session comes up at its prompt rather than working. On a resume only Claude Code is
given a prompt argument, placed the same way, and only when Octoboard has an instruction or a prompt to continue for it;
Codex and Grok Build are given none on a resume, and what Octoboard has for them is written into the session instead
(see "Messages held until a session can take them" in `docs/product/message-delivery.md`).

For Claude Code, a prompt that is one command-like word — nothing but ASCII letters, digits, `-` and `_` — is passed
with a single leading space, which the model sees: Claude Code runs an argument that names one of its subcommands even
after the `--`, so such a word, a new session's task or a resume's instruction alike, would otherwise run that command
rather than start a turn.

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

A model and an effort chosen for the session are passed to every Claude Code release, with no version checked first. A
release that does not take one of the options Octoboard passes refuses it and exits before it draws anything; Octoboard
then starts it once more as a plain terminal would, with the conversation, the model and the task alone, and tells the
user with the standing notice `launch_retried_plain` (there is no in-between launch for Claude Code). A launch whose
hooks never arrive — the user's or a managed configuration switching hooks off, say — is found without them 30 seconds
after it starts: the messages Octoboard holds for it go in once its input box is drawn with no trust confirmation up,
and the standing notice `hooks_not_received` says so until a hook arrives. Claude Code runs no hook while its trust
confirmation is up, so the 30 seconds do not run while it is: they start once it has gone, and a confirmation left
unanswered raises nothing.

## Codex

Octoboard does not assign Codex a session id in advance. A resumed session has its conversation's id from the start;
a new session learns its agent-side id from the first prompt submitted in it, so one opened without a task has none
until then; with Codex's private app server an automatic message can be that first prompt, without the user having to
submit the draft in the composer first. Every session started with Octoboard's additions runs with a private app server
of its own, whatever Codex's version (see "Codex's private app server" below). One whose server could not be started
runs as one terminal process, `codex` (`codex resume <id>` on a resume), whose own command line carries every override
Octoboard passes, all of them again on a resume. A launch that Codex ends as it starts is started again with less (see
"When Codex exits as it starts" below).

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
`.bat`, `.ps1`) whose package is in none of these layouts, start Codex as a plain terminal would, through the shim
itself and without Octoboard's additions, which the shim's command line cannot carry; a new session's task is then
given as its first message. The banner `launch_enhancements_unavailable` names the reason,
`provider.codex.native_program_missing`: Codex's install is missing its native program, and reinstalling Codex starts
it with Octoboard's additions again. A `codex.exe` found in none of these layouts is run as it is.

The launch conditions that are visible:

- project trust is not passed: in a git repository Codex has not been told to trust, its own folder-trust confirmation
  appears, and Octoboard presses it under the user's permission, or, in a console session, without asking (see
  `docs/product/folder-trust.md`);
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

The user's `shell_environment_policy` is left as it is. Codex's default excludes are off unless the user sets
`ignore_default_excludes = false`; where they apply, Codex removes every variable whose name holds `KEY`, `SECRET` or
`TOKEN` from the commands its model runs; no variable Octoboard gives a project session's git has such a name (see "The
session's git hooks" in `docs/product/branch-responsible-session.md`), so git and Octoboard's git hooks keep working in
those commands. A policy that removes more, such as an `include_only` list leaving out `OCTOBOARD_SESSION_ID`, leaves
the session's git running the user's own hooks with no report and no revert gate; one that leaves out
`GIT_CONFIG_PARAMETERS` leaves it running without Octoboard's hooks at all.

Automatic messages go into Codex's own queue on a launch with the private app server, leaving the composer untouched
(see "Messages through Codex's own queue" in `docs/product/message-delivery.md`). On any other launch they reach Codex
through its terminal, like any other agent's: Octoboard uses no stash of Codex's to set a draft aside, so past the
grace a draft in its composer is cleared with Ctrl-U before the message is written, as on the other agents (see
`docs/product/draft-preservation.md`), under the holds in "Messages held until a session can take them" in
`docs/product/message-delivery.md`.

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

**Which launches have one.** Every launch with Octoboard's additions, a resume included, whatever Codex's version, and
an install whose version cannot be read included: no release is known to have introduced the server, 0.166.0 being
only the first one measured with it. A release that cannot run it is caught where it fails: a server that does not come
up gives the ordinary launch (see "When it cannot be used" below), and a terminal that refuses to attach exits as it
starts and is started again without it (see "When Codex exits as it starts" below). A launch whose terminal goes to a
process holder of an earlier build that cannot run the server (see "Holders of other builds" in
`docs/product/process-holders.md`) gets the ordinary launch, the terminal alone; so does a reduced launch without
Octoboard's hooks, below.

**Who runs it.** The process holder that runs the session's terminal (see `docs/product/process-holders.md`) runs the
server too, beside the terminal, so the server outlives the daemon as the terminal does. Where holders cannot be set up
and the terminal runs inside the daemon (see "When holders cannot be set up" in `docs/product/process-holders.md`), the
daemon runs the server itself, and it ends with the daemon as the terminal does. Either way Octoboard's own
connections, which carry its messages, go to the server's endpoint directly, not through the holder.

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
- Codex does not honour the hook-trust bypass, so Octoboard trusts only its own generated hook commands — the status
  hooks' and, where it is injected, the cross-project guard's — by the fingerprint Codex gives each, through overrides
  of that server start alone. It records no trust in the user's files and grants none to the user's other hooks. Hook trust does not weaken the sandbox or the approval policy.

**Where the server listens.** On macOS and Linux, a Unix socket in a private directory under `/tmp`, removed with the
server. On a Windows daemon host, a loopback WebSocket port, which any local process can reach, guarded by a random
capability token: the server is given only the token's SHA-256, and the terminal reads the token from an environment
variable of its own, `OCTOBOARD_CODEX_REMOTE_TOKEN`. A holder that runs the server makes the token itself and hands it to
the daemon over its connection to the daemon; a daemon that runs the server makes it. The variable is the one secret an
agent's environment holds: Codex's terminal takes the token only from a variable it is named
(`--remote-auth-token-env`), never from a file. Only the terminal has it; the server, which runs the agent's commands
and writes its shell snapshots, does not, and the token opens that session's server alone.

**Requests of the server's.** Approvals and the other requests the server sends a client are answered in the terminal
alone: Octoboard's own connections to the server, which carry its messages, leave them unanswered. A turn that runs on a
conversation Octoboard had the server load again (see "Messages through Codex's own queue" in
`docs/product/message-delivery.md`) while the terminal shows another task, and a turn a message started before a
resumed session's terminal attached, send their approval requests to Octoboard's connection while it is open, which
never answers them; the request then waits. Codex's agent command center lists that task as "Needs input", and opening
it shows the approval in the terminal, where it can be answered (measured on Codex 0.166.0).

**Its lifetime.** The server belongs to the running session, and runs as long as its terminal does. When the terminal
exits, the server is ended. When the server exits by itself, the terminal is ended too, as if the agent had exited by
itself rather than been ended by Octoboard, since it would otherwise wait for the server to come back for good. A
holder that runs both does this itself, whether or not a daemon is connected to it, and the server ends with the
holder however the holder ends, killed outright included. A server left running by a holder of an earlier build that
ended without it is stopped by the next daemon start as part of what that launch left behind (see "Automatic stopping
and its boundaries" in `docs/product/background-processes.md`). The server is in the launch's
process containment and leads a process session of its own (on a Windows daemon host, a job of its own inside the
launch's job); what that means for the background-process lists is in "Automatic stopping and its boundaries" in
`docs/product/background-processes.md`.

**Across a daemon restart.** Where a holder runs the server, every daemon end that leaves the programs running in their
holders (see "Which daemon ends leave the programs running" in `docs/product/process-holders.md`) — a crash, a planned
restart such as `octoboardd restart` or the desktop application's restart into an update — leaves the server running
with the session, and what is already in Codex's own queue keeps running meanwhile. The daemon that takes the
session's terminal over (see "What a daemon start takes over" in `docs/product/process-holders.md`) links to the server
again, and the session's messages go through Codex's own queue as before (see "Messages through Codex's own queue" in
`docs/product/message-delivery.md`). A server that exited while no daemon was connected has had its terminal ended by
the holder, and the session ends as if its agent had exited by itself. A session taken over with no server beside its
terminal — an ordinary launch, or one a holder of an earlier build ran — or with one at an endpoint the daemon cannot
use has its automatic messages take the guarded terminal delivery.

**When it cannot be used.** If the server does not start listening within 15 seconds, exits first, or cannot be
initialised, or, on a resume, the hook trust or the permission preset cannot be applied, the session is launched the
ordinary way: the terminal alone, with every override on its command line, and its automatic messages take the guarded
terminal delivery. Nothing queued for the session is lost by it. When a single message cannot go into Codex's queue,
see "Messages through Codex's own queue" in `docs/product/message-delivery.md`.

### When Codex exits as it starts

A Codex launch that ends by itself as it starts, before it has drawn anything and before a key was typed into it, is
started again once with less, according to what Codex wrote as it exited (measured on Codex 0.167.1):

- **A setting's value Codex does not take.** Codex refuses a value of a configuration key it does not know (`unknown
  variant`), naming the key, and a `default_permissions` that names a built-in profile it does not know (`Error loading
  configuration: default_permissions refers to unknown built-in profile …`). Where it is one Octoboard set for the
  launch, the launch is started again without that one override, and the banner `provider.codex.setting_left_out` names
  the key. Where it is one of the permission preset's three keys, all three are left out, and the banner
  `provider.codex.permissions_left_out` says the permission approval chosen in Octoboard is not applied; Codex's own
  configuration decides what it may do. A key Octoboard did not set is the user's own, and the launch goes on to the
  plain one.
- **Anything else**, an option Codex does not take among it: the launch is started again without the review bypass,
  Octoboard's hooks and the private app server, which a release may not take as a whole, and with every other
  addition. The banner `provider.codex.hooks_left_out` says so: the session's status follows what Codex reports through
  its terminal, which it does by itself, edits to another project's files are not blocked, and its prompts can be
  answered only in the terminal. Its automatic messages take the guarded terminal delivery. A new session started
  this way has no hook to report its conversation's id, so the banner also carries `conversation_id_unknown`: its next
  start opens a new conversation.

A key Codex does not know at all is passed over without a word, so a release that lacks one of Octoboard's settings
starts as usual without it. When the second launch exits as it starts too, Codex is started a last time as a plain
terminal would, and when that exits too the session ends with Codex's own message on screen.

A launch with Octoboard's hooks expects its first hook with its first turn: Codex runs none before the first prompt is
submitted, on a new conversation and on a resume alike. A launch whose first turn brings no hook within half a minute
is from then on read as a launch without them, its status following what Codex reports through its terminal.

## Grok Build

An explicit permission mode chosen in Settings applies to every account on fresh launches and resumes through
`--permission-mode`: Ask uses `default`, Auto uses `auto`, and Always-approve uses `bypassPermissions`. Following
account configuration passes no flag and leaves the mode to Grok Build's native configuration and resume rules. When the
native auto-mode feature is unavailable, Grok Build falls back to Ask. The launch neither edits the source configuration
nor writes a permission mode back from the session's copy, even when the source configuration is shared through a
symlink. The choices and confirmations are in "Approval modes" in `docs/product/per-agent-settings.md`.

Always-approve leaves deny rules and hooks active; a session can still need the user's attention. It does not disable
Grok Build's sandbox, and a launch under the sandbox starts without Octoboard's additions, described below. Octoboard
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
  **A theme, a plugin switch or the default model changed in a session reaches the file the source `config.toml` links
  to, as Grok would have saved it there; with a plain source `config.toml`, it is lost with the session**, as is every
  other setting the session saves. Any change made in the session counts, whoever made it. A source `config.toml` is
  linked for an account sharing the default configuration, or in any home whose `config.toml` the user has linked
  elsewhere, such as into a dotfiles repository; the link must lead to an existing file. The settings are carried after
  the process exits and before the per-session home is removed, leaving the link in place, and they are exactly
  `ui.theme`, `plugins.enabled`, `plugins.disabled` and `models.default`. Only values that differ from the copy taken at
  launch are carried; each plugin list is carried whole, an empty one included, but removing a key does not remove it
  from the linked file. The plugin lists are not carried from a session whose per-session home has a plugin directory of
  its own (see the paragraphs on installed plugins below). A model equal to the model passed at launch is excluded,
  including when the user explicitly selects it again. Reasoning effort, Octoboard's MCP entry, what a compatibility
  switch adds (see `docs/product/compatibility-between-agents.md`) and every other setting stay in the copy. The next
  launch reads the source file afresh.

  The link must still lead to the same file it led to at launch: a link removed, broken or redirected to another file
  receives nothing. Sessions exiting under the same daemon merge only what they changed into the current file: changes
  to different settings survive, and for the same setting — a plugin list counting as one — the last exit wins. A
  session that changed nothing does not restore an earlier value. A simultaneous save from an external Grok process or
  editor can still race with this. A failure to write is logged, and the per-session home is still removed, so the
  settings it would have carried are lost; so are they if the Octoboard daemon itself stops while the session is
  running.
- the trust store, `trusted_folders.toml`: for a project session copied as it is, with nothing added, so a folder the
  user has not trusted Grok with shows Grok's own trust confirmation. The one entry Grok writes into the copy when that
  confirmation is accepted is the exception to its being lost: it is carried into the source home's store (see
  "Carrying Grok Build's trust record" in `docs/product/folder-trust.md`). For a console session the copy is given an
  entry for the console's directory, so Grok does not ask, and nothing is carried (see "Console sessions' trust
  confirmations" in `docs/product/folder-trust.md`).

Grok's built-in bundles and its sandbox files — `bundled/`, `bundles/`, `hooks-paths` and `sandbox.toml` — are copies
taken on every launch as well, a directory copied whole, whichever of them the source home holds. A bundle update Grok
downloads during a session lands in Octoboard's copy and is not kept; the next launch copies whatever the user's own
`grok` has fetched since.

**A plugin installed in a session is the user's, and its registry entry is made to name it through the source home.**
Grok installs a plugin into `installed-plugins/<name>` under its home and records it in
`installed-plugins/registry.json`. The per-session home's `installed-plugins/` is a link to the source home's, so the
plugin's files and its registry entry land in the source home's own directory, but Grok writes the entry's path through
the per-session home, which is removed with the session. Left so, the user's own `grok` still lists the plugin, cannot
update one from a git source and may no longer load it. Octoboard therefore rewrites those paths in the source home's
registry:

- after the session's process exits and before the per-session home is removed (after the settings above are carried),
  every entry whose path lies under this session's per-session home is pointed at the same path under the source home's
  `installed-plugins/`, spelled `<source home>/installed-plugins/<name>`, as Grok records it when run against the source
  home;
- before each launch, before Grok starts, entries an earlier process left under this session's own per-session home,
  and entries under the per-session home of another session that no longer exists or now leads to another plugin
  directory (a daemon that stopped without seeing that session exit), are pointed at the directory the plugins are in,
  by its resolved path. An entry reaches a registry only through a per-session home whose link led to that registry's
  directory, whichever account the session ran as, so that directory is where the plugin's files are; this session's
  own spelling of it is not used for another session's entry. A per-session home that still leads to this directory is
  a running session's, and its entries are left for its own exit. A session resumed on another account gets its
  per-session home built anew under the same path: what an earlier process of it left in the first account's registry
  is then under a home that leads elsewhere, and is settled at the next Grok Build launch from that account.

Only the `path` of an entry recorded under a per-session home changes; every other entry is left as it is. A registry
with nothing to rewrite is not written at all and stays byte for byte as it was: a missing one is not created, and one
that is not JSON is left alone. A registry reached through a link is written where the link leads, and the link stays. A
file a rewrite cut off by the daemon's end left staged beside the registry (`.registry.json.octoboard-<process id>`) is
removed before the next rewrite, once the process that wrote it is gone and the file is more than fifteen minutes old,
and no file of another name is touched. A failure is logged and holds up neither the launch nor the removal of the
per-session home. Grok's own processes do not wait for Octoboard: the registry is replaced only while it still holds
what Octoboard read, yet a plugin install another Grok process saves at that very moment can be lost. For an account
that shares the default configuration, the path so written runs through the account's directory, and is then pointed at
the default directory's own like any such path (see "Plugin registries kept leading into the default directory" in
`docs/product/sharing-the-default-configuration.md`). Whether the plugin is switched on is a setting in `config.toml`,
carried back only as described above for that file.

This applies to every Grok Build account, the default one included, whenever the source home has an
`installed-plugins/` directory, as a home Grok 1.0.62 or later has run against does. Where it has none, the per-session
home gets a plugin directory of its own: a plugin installed in the session, its files and its registry, are the
session's and are lost with it, and nothing is written to the source home's registry. From such a session the plugin
switches (`plugins.enabled` and `plugins.disabled`) are not carried into a linked `config.toml` either, so no switch is
left there for a plugin that went with the session; a switch the user changed in that session for a plugin of the
source home's own is not carried with them. The theme and the default model are carried as ever.

Three entries of the source home are left out altogether. Its `hooks/` directory is not linked: the per-session home's
`hooks/` holds Octoboard's own hooks alone (its status hooks and, where it is injected, the cross-project guard), so
hooks the user keeps in the Grok home's own `hooks/` directory do not run in a session Octoboard starts, while a project's own hooks still do (see "A project's hooks and instruction files"
below). Its `requirements.toml` and `managed_config.toml` are not carried either (see "Grok's own sandbox" below).

On a Windows daemon host, a SQLite database the source home holds is a copy even where the host makes file links, so
what a session writes into it is not kept.

The per-session home is `grok-home` in the session's scratch directory, `~/.octoboard/run/<session id>/` (see
`docs/product/files-octoboard-owns.md`). Its copy of `config.toml`, which holds the user's whole Grok configuration with
Octoboard's MCP entry added, is the user's alone whatever the source file's mode (on Windows through the data
directory's access control list). The MCP entry's command line names the file the session's key is in and the session's
MCP token, never a key (see `docs/product/who-can-reach-the-daemon.md`). With a real Grok session this has been checked
on macOS only; on Windows it has not yet been checked that `icacls` shows only the owner and SYSTEM on the copy and that
a second, non-administrator user, logged on as itself, cannot open it.

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
  accepted in a project session is carried into the user's own `trusted_folders.toml`.

A host that makes file links builds the home as described above.

### Grok's own sandbox

**A Grok Build launch that would run under Grok's own sandbox starts bare**: no per-session home is built, and Grok runs
against the account's own home as it would in a plain terminal, with only the conversation to resume, the model and the
task on its command line (a bare launch). The terminal banner says the
session started without Octoboard's additions, with `provider.grok.sandbox_unsupported` as the reason, naming the
sandbox profile and what sets it. The sandbox lets Grok write only to its own home, the working directory and temporary
directories, judged by where links really lead, and the per-session home's login and session records are links back
into the source home, so a sandboxed session could not even create its conversation there. The banner's remedy is to
turn the sandbox off where it is set, or to have whoever manages that setting do so, for sessions to have Octoboard's
additions. A conversation begun under the sandbox keeps it, so its resumes start bare too. A profile of `off` is no
sandbox. Which profile applies is read as Grok layers it. The system-wide files below are read from `/etc/grok` on
macOS and Linux only: Grok Build has no system-wide configuration directory on Windows, so neither is read there, and a
file under `\etc\grok` on the daemon's current drive counts for nothing.

- `[sandbox] profile` in the system-wide `/etc/grok/requirements.toml` is a pin that takes precedence over everything:
  any other profile there starts the launch bare, and `off` there gives it the per-session home whatever else is set;
- on a resume, the profile Grok saved with the conversation (its `summary.json`'s `sandbox_profile`), or a
  `GROK_SANDBOX` in the launch environment (see "The launch environment" in `docs/product/launching-agents.md`), starts
  it bare when either is not `off`, since Grok restores a saved profile and refuses a resume that asks for a different one; a
  resume whose conversation has no saved profile is judged like a new session;
- otherwise the first of these that sets a profile decides: `GROK_SANDBOX` in the launch environment, then `[sandbox]
  profile` in the source home's `config.toml`, then the fleet default in the system-wide
  `/etc/grok/managed_config.toml`.

Not read: macOS managed preferences for Grok, so a sandbox set only there does not start the launch bare and the session
fails inside Grok instead; and the source home's own `requirements.toml` and `managed_config.toml`, which are left out of the
per-session home, so Grok never loads them for an Octoboard session.

### Grok's leader mode

**Every Grok Build launch passes `--no-leader`** — a new session, a resume, and the relaunches of an account switch and
a model change alike — so the session's agent runs in the session's own process even where the Grok configuration turns
leader mode on (`[cli] use_leader = true`). The session neither starts a shared Grok leader process nor connects to one
already running. Under leader mode the terminal would be only a client of that leader, and stopping the session would
kill the leader outright mid-turn, so Grok would neither end the turn nor shut the session down cleanly; with
`--no-leader`, stopping the session ends the agent's work through Grok's own shutdown. Leader mode is not available
in a session Octoboard starts.

`grok --help` does not list the flag, and Octoboard passes it whatever the installed version. A release that refuses it
exits as it starts, printing that it does not take the argument; the session is then started again with everything else
of Octoboard's but `--no-leader`, and its terminal banner says (`provider.grok.leader_not_overridden`) that where the Grok
configuration turns leader mode on, stopping the session cuts off a running turn without Grok's own shutdown. Any other
option Grok refuses as it starts, or any other exit as it starts, starts it again bare, with nothing of Octoboard's on
its command line.

A Grok Build launch with Octoboard's hooks expects its first hook as it starts: Grok runs `SessionStart` within about a
second of the launch, on a new conversation and on a resume alike, before anything is written into it (measured on
Grok Build 1.0.67). One with none within 30 seconds, not counting the time its trust confirmation is up, is from then
on read as a launch without hooks (see "When the hooks never arrive" in `docs/product/launching-without-octoboards-additions.md`). The draft
stash is used whatever Grok Build's version (see `docs/product/draft-preservation.md`).

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

A window asks for a launch when it opens a session (with the new-session dialog or with defaults), resumes one, changes
one's model, switches one to another account or hands one off, and it sends the appearance its Light, Dark or System
choice has resolved to at that moment (see "Light, dark and follow the system" in `docs/product/appearance.md`). No
window is behind a launch an agent asks for through Octoboard's tools (opening or reopening a session, handing one off,
switching one to another account, changing one's model), a session opened, reopened or resumed by an approved session
request, a session the daemon resumes on its own as it starts (see "Resume sessions on start" in
`docs/product/general-settings.md`), or an archived owner relaunched on the way while a session is resumed.

The appearance is taken at each launch and not kept with the session: a running Grok session keeps the theme it was
launched with when the window's appearance changes later, and its next launch picks the theme afresh. `GROK_THEME` takes
precedence over the theme set in the Grok configuration file at startup, and does not save a theme into that file. An
explicit `/theme` saves the chosen theme in the session copy; when the source `config.toml` is linked, it is carried
into the linked file on exit under the rules in "The per-session Grok home" above, even when it is the theme the
appearance picked. The next launch applies the terminal's appearance again. Claude Code and Codex are told nothing of
the appearance at launch; in every agent's session, it also counts towards the colours the session's terminal answers a
program's colour queries with (see "The colours and sizes a program reads" in `docs/product/terminal-queries.md`).

### A Grok Build config directory

A Grok Build config directory must **already be a Grok home, one Grok has been run against**. Only entries that already
exist in the source home are linked; for an empty directory, Grok creates its login and session records inside the
per-session home, which is discarded with the session's process, so neither the login nor the conversation persists.
Octoboard does not check this when the directory is set — a user pointing it at a directory they are about to create
should not be stopped. It is checked at **launch** instead: a Grok Build session pinned to a directory that is not an
initialized Grok home starts bare against that directory, as `GROK_HOME=<directory> grok` would in a plain terminal, so
Grok signs in and keeps its conversation there, and the terminal banner gives `provider.grok.home_not_initialized`,
naming the directory, as the reason the session has no Octoboard additions. Moving a session to such an account is still
refused before anything is copied into it, with the same code. A session on the default account is never held to this,
since that directory is the user's own existing setup.

### A project's hooks and instruction files

Grok Build loads **a project's own hooks only inside a git repository**: it locates a project by walking up for a `.git`
directory, and in a directory without one it loads none of the project's hooks. Its instruction files need no repository
— in a directory that is not one, Grok reads those in the working directory alone — but, like the project's hooks and
MCP servers, they load only once the folder is trusted (see `docs/product/folder-trust.md`). That is how a Grok console
session reads the console's instruction file (see "The console session's instruction file" in
`docs/product/launching-agents.md`).

## Windows and Linux hosts

The launch uses the daemon host's installation and configuration, including for a remote window. These are
platform differences for the agents listed below. Claude Code, Codex and Grok Build’s platform-specific launch rules are in their sections above.

| Agent | Windows | Linux |
|---|---|---|
| Antigravity CLI | The native install uses a private launch directory. When the host cannot link files, eligible files are copied into that directory. The CLI's first-run Terms screen waits for the user's answer. | The native install uses a private launch directory and a shell to run the injected hook. |
| GitHub Copilot CLI | The npm install and Windows block-border composer are supported. Account login uses Credential Manager when available. | Account login uses Secret Service when available. When the CLI asks to save its token in plain text, the login dialog waits for the user's answer. |
| OpenCode | The npm install launches the bundled executable. A cold first launch can take longer while OpenCode sets up its plugins and downloads ripgrep. | A cold first launch can take longer while OpenCode installs its dependencies. |
| Qwen Code | The npm install launches through Node, and the injected hooks use PowerShell. The system settings directory is under `C:\ProgramData\qwen-code`. | The injected hooks run through the POSIX shell, and system settings are read from `/etc/qwen-code/settings.json`. |
| Pi | The launch extension supplies turn status when the console host relies on rendered output. A daemon interrupt sends Escape. | Pi supplies native turn status. The launch extension runs the injected hook and provides message delivery as on Windows. |

Qwen Code's injected status hooks apply to the outer Octoboard launch on both platforms. A nested Qwen Code
started by a shell tool or MCP server cannot report its status as that launch.

For sign-in on either host, see "How a quota is read" in `docs/product/account-quota-readings.md` and "How each
agent signs in" in `docs/product/logging-accounts-in.md`.

## GitHub Copilot CLI

- **An option a Copilot release does not take.** Copilot refuses an option it does not know and exits at once, before
  it draws anything, and its message does not tell one of Octoboard's options from another. Octoboard has no reduced
  launch for it: such a launch is started again bare, with only the conversation, the model and the task, and the
  terminal's banner says that the session runs without Octoboard's additions.
- **When its hooks are expected.** Copilot runs its first hook with the session's first prompt, not as it starts, so
  an idle session waits for no hook. Once a prompt has gone in and no hook follows within the bound, the session is
  read as one without hooks and the banner says so.
- **The draft stash** is used from 1.0.60 on, the release whose changelog made Ctrl-S the stash key, and when the
  installed version cannot be read.

## Qwen Code

A system settings file that is there but cannot be read as settings no longer stops a launch. The session starts
as Qwen Code would in a plain terminal, without Octoboard's additions, so Qwen Code reads and reports that file
itself; the banner names the file, says to repair or remove it, and the next launch after that has the additions
again. On Windows, when Node cannot be found beside the npm launcher or on `PATH`, the session likewise starts
through the npm launcher itself without the additions. A Qwen Code release that no longer takes one of Octoboard's
options exits as it starts; the session is then started again without the additions, with the banner saying so.

A Qwen Code session's first hook comes with its first turn, so a session that sits idle raises no notice. When a
turn starts and no hook arrives within the bound, the session is read as one without hooks and the banner says so.
A launch found with Qwen Code's hooks switched off by the user says so from its start and waits for none.

## Antigravity CLI

A launch uses its own copy of the hooks, MCP server configuration and global rule file. Changes a session makes
to those copies or to its enabled plugins end with the launch. To keep these changes, make them in `agy` run
directly. A launch can create the native `antigravity-cli` and `config/projects` directories under `~/.gemini`
when they are missing. Model listing runs in a private directory and leaves the user's home as it was.

When Octoboard cannot prepare its additions, the session still starts, as `agy` does in a plain terminal, and the
terminal's banner says why: this version of `agy` does not take the option that names the launch's directory, a
hooks, MCP server or global rule file cannot be used, or the configuration is still in an earlier version's layout.
Only an account with a directory of its own is refused, since `agy` has one sign-in whatever directory it is given.

A session started that way runs `agy` against the user's own `~/.gemini`, so `agy` migrates an earlier layout itself
as it starts, and the next launch has Octoboard's additions again. If a hooks or MCP configuration file exists in both
layouts, that migration replaces this version's file with the earlier one and keeps no copy; the banner names both
files, and what the newer one held has to come from a backup. To avoid that, choose which file to keep and run `agy`
directly before launching. Model listing never migrates: from an unmigrated configuration it lists nothing and says
why.

If `agy` exits as it starts because it refuses one of the settings Octoboard launches it with, Octoboard starts it
again without that setting and the banner names it (`provider.antigravity.launch_setting_not_taken`); the rest of
Octoboard's additions stay. Any other exit at start starts it again as in a plain terminal.

A launch with Octoboard's hooks expects its first hook with its first turn: nothing fires as `agy` starts, and every
turn of a new or a resumed conversation runs `PreInvocation` at its first model request (measured on 1.5.14). A user
settings file that `agy` cannot read as settings, which `agy` then runs without, is told in the banner for the rest of
the launch (`provider.antigravity.settings_not_read`), naming the file to repair.

## OpenCode

Octoboard launches any OpenCode version; it was measured on 1.19.6, 2.3.2, 2.4.0 and 2.4.1, and a newer one is
not refused for being unmeasured. A version of a minor not measured shows a standing notice that some Octoboard
features may be unavailable for the session; the terminal works as usual. The major version picks how a session is
launched: a version from 2.0.0 on, or one that cannot be read, is launched as OpenCode 2. For session messages, see
"Messages through OpenCode's private server" in `docs/product/message-delivery.md`.

- **A migration of OpenCode's own still pending.** When OpenCode 2 has yet to import the first generation's
  credentials or terminal settings, the session starts bare, as in a plain terminal, and the banner says why
  (`launch_enhancements_unavailable`, its reason `provider.opencode.migration_required`): OpenCode migrates as it
  starts, and the next start or resume of the session has Octoboard's additions again.
- **Resuming on OpenCode 2.** Before the launch Octoboard asks OpenCode whether it still has the conversation. Only
  OpenCode's own answer that it has no such conversation refuses the resume (`conversation_unresumable`), since
  OpenCode 2 would otherwise start a new conversation under that id. Any other outcome (no database where it is looked
  for, a pending migration, a query that timed out or answered something unreadable) resumes anyway, and tells the
  user with the standing `conversation_record_not_found`, a bare launch as well as one with Octoboard's additions.
  OpenCode 1.x answers a conversation it no longer has itself as it starts.
- **An option it does not know.** OpenCode exits at once and prints its usage, naming no option, so a launch it
  refuses is started again bare.
- **When its events are expected.** Its plugin forwards nothing as OpenCode starts; the first event comes with the
  first prompt submitted, on a new conversation and on a resume alike.
- **Its plugin.** Octoboard's plugin turns itself off rather than fail if OpenCode's plugin interface changed;
  OpenCode itself goes on past a plugin that fails to load and shows nothing. The session then has no events: once its
  first prompt has gone in without one, the banner says that none arrive (`hooks_not_received`).
- **Its private server.** When the server Octoboard starts for a session does not come up, the terminal is launched on
  its own and messages go through it.

## Pi

When a Pi release no longer takes one of the options Octoboard adds, or cannot load Octoboard's extension, Pi
exits as it starts. Octoboard then starts it again without its additions: the conversation, the model and the task
only. The terminal shows a notice that the session runs without Octoboard's additions. If Octoboard's extension
loads but cannot set itself up under a changed extension interface, Pi keeps running without it. The session then
shows a notice that no hooks arrive.

A launch with Octoboard's extension expects its first hook as Pi starts: the extension forwards Pi's `session_start`
once Pi is past its project-trust confirmation, before anything is written into it.

A session keeps every records directory its launches selected through `PI_CODING_AGENT_SESSION_DIR`. A conversation
whose record is in one of them resumes from it, with `--session-dir <that directory>` before `--session <id>`, since
`--session` alone looks only in the directory the current environment selects (measured on Pi 1.5.2), even after the
variable has changed or been removed; that holds for a plain launch too. Such a record counts as the session's when
judging whether its conversation can still be resumed, and is moved to the Trash with the session (see "When a
conversation can no longer be resumed" in `docs/product/archiving-and-resuming-sessions.md` and "Moving the agent's own
records to the Trash" in `docs/product/deleting-sessions.md`). A record in none of them is looked for where a plain
terminal's run would look.
