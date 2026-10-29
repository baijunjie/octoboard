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
| Status hooks | In the JSON passed with `--settings`, which Claude Code merges with the settings files rather than replacing them, so the project's own hooks still run | One `-c hooks.<event>=…` override per event, each run asynchronously, together with `--dangerously-bypass-hook-trust` | A `hooks/` directory of Octoboard's own in the per-session Grok home (see "The per-session Grok home" below) |
| Octoboard's MCP server | `--mcp-config` | `-c mcp_servers.octoboard.command=…` and `-c mcp_servers.octoboard.args=[…]` | An `[mcp_servers.octoboard]` table in the per-session copy of `config.toml` |
| Running Octoboard's tools without asking | An `allow` rule, `mcp__octoboard`, in the `--settings` JSON | `-c mcp_servers.octoboard.default_tools_approval_mode="approve"` | Nothing is passed |
| The agent's time limit on a call to Octoboard's tools | None is set | `-c mcp_servers.octoboard.tool_timeout_sec=660` | `tool_timeout_sec = 660` in that table |
| The role description | `--append-system-prompt`, added to Claude Code's own system prompt | `-c developer_instructions=…`, a developer message added beside Codex's own prompt, which stays as it is | `--rules`, appended to Grok's system prompt |
| The model and reasoning effort | `--model`, `--effort` | `-c model=…`, `-c model_reasoning_effort=…` | `--model`, `--reasoning-effort` |
| An explicit approval choice (see "Approval modes" in `docs/product/per-agent-settings.md`) | `--permission-mode`: `auto` for **Automatic**, `default` for **Ask me**, `bypassPermissions` for **Approve everything** | `-c approvals_reviewer=…`: `auto_review` for **Automatic review**, `user` for **Ask me** | Not applicable |
| Claude compatibility while on (see "Claude compatibility" in `docs/product/per-agent-settings.md`) | Not applicable | A section appended to the `developer_instructions` text, and Claude Code's MCP servers as `-c mcp_servers.<name>.…` overrides (see "Claude compatibility for Codex" below) | `GROK_CLAUDE_*_ENABLED=true` in the environment (see "Claude compatibility for Grok Build" below) |
| Anything else | The status line, and under **Approve everything** the acceptance of Claude Code's warning, in the `--settings` JSON | The overrides listed under "Codex" below | Nothing else |

The 660 seconds are Octoboard's own 10-minute limit on a forwarded call plus a minute (see "The time limit on
Octoboard's tools" below and the Codex list below for why). A value the session leaves to the agent's default — no
model, no effort, **Follow account configuration** — passes nothing for it. The variables a launch sets for each agent,
`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GROK_HOME` and `GROK_THEME` among them, are in "The launch environment" in
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
Octoboard tells the user so once, when the session starts, naming the project it is in: the `allow` rules stay ignored
until Claude Code's trust confirmation is answered, and Octoboard presses that confirmation once the user has agreed to
it (see `docs/product/folder-trust.md`). It stays silent unless the user's own configuration says explicitly that the
directory is untrusted, so a configuration it cannot read leaves them alone rather than warning on every launch; and it
stays silent when a directory above the session's is recorded as trusted, which Claude Code honours without asking while
leaving the directory's own record saying untrusted. The trust state is read from the global config file this launch's
Claude Code will itself read: `.claude.json` inside the Claude Code config directory in effect — the session's own, or
else a `CLAUDE_CONFIG_DIR` the user's shell exports — and `~/.claude.json` when there is none. A config directory
without that file yet produces no warning; `~/.claude.json` is not consulted in its place.

An explicit approval mode chosen for Claude Code in Settings is passed on every launch of any of its accounts, a resume
included, as the permission mode Claude Code starts in; following each account's configuration passes none, so the
account's `settings.json` decides. Under **Approve everything**, every launch also tells Claude Code that its own
warning for that mode has been accepted, so the blocking screen it would otherwise raise on a config directory's first
use of the mode never appears; this holds for that launch alone, and nothing is written into the account's configuration
files. The choices are in "Approval modes" in `docs/product/per-agent-settings.md`.

Octoboard's own tools are pre-approved for the session on every launch, a resume included, under every approval mode,
following the account's configuration included, so a session's calls to them, a console session's orchestration
included, raise no Claude Code permission prompt. The launch carries one `allow` permission rule naming Octoboard's MCP
server, `mcp__octoboard`; like the acceptance of the warning above, it holds for that launch alone and nothing is
written into any settings file. The rule covers that one server's tools alone: every other MCP server, whatever its
name, Bash, file edits and every other tool keep the approval mode in effect. It adds to the user's and the project's
own `allow` rules rather than replacing them, and a `deny` or `ask` rule of the user's or the project's that names one
of Octoboard's tools still wins over it — a `deny` takes that tool away from the session altogether. Under
**Automatic**, Octoboard's tools are therefore not put to Claude Code's auto-mode classifier either. What a tool itself
refuses, and the requests Octoboard puts to the user before acting on a session's behalf (see
`docs/product/session-requests.md`), are unaffected.

## Codex

Codex cannot be given a session id in advance, and its conversation is created lazily on the first prompt submission —
so a session opened without a task has no agent-side id until the user types something, unless it was opened to resume a
conversation, whose id it has from the start. The launch conditions that are visible:

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
- the review Codex would otherwise raise for Octoboard's own hooks is bypassed, which costs two warning lines in the
  session's own output on **every** launch, a resume included. The bypass covers hook review only; it does not weaken
  the sandbox or the approval policy. These two lines are a standing cost, not a defect waiting to be fixed: Codex gates
  a hook behind a trust hash taken over the hook's own command, and that command is the session's own per-session
  script, so no pre-captured hash could ever match it.
- Octoboard's own tools are pre-approved for the session, so a console session's orchestration calls raise no approval
  dialog. This covers Octoboard's tools alone; every other tool, the sandbox and the approval policy are untouched.
- Codex's own time limit on a call to Octoboard's tools (300 s by default) is raised to 660 seconds, above Octoboard's
  own limit on a forwarded call (the 10 minutes in "Waiting for the answer" of `docs/product/session-requests.md`), so
  that a call that waits for the user, such as a session request, can return its result. This is the limit on
  Octoboard's tools alone; no other tool's limit changes.

An explicit Codex approval reviewer chosen for Codex in Settings takes precedence on every launch of any of its
accounts, a resume included. When the setting follows each account's configuration, whether Codex resolves approval
requests by itself is read from `config.toml` in the Codex home that launch uses: the session's Codex config directory,
else a `CODEX_HOME` the user's shell exports, else `~/.codex`. A trusted project's own `.codex/config.toml`, which Codex
itself layers over that file, is not consulted for this. The choices and their lifetime rules are in "Approval modes" in
`docs/product/per-agent-settings.md`.

### Claude compatibility for Codex

While Codex's Claude compatibility is on (see "Claude compatibility" in `docs/product/per-agent-settings.md`), every
launch, a resume and an account switch included, gives Codex two things; while it is off, it gives neither. Nothing is
written into any of Claude Code's or Codex's files for it.

**A "Claude Code compatibility" section** is appended to the developer instructions Octoboard already passes (after the
role description). It names the user-level Claude Code configuration directory — the directory of Claude Code's default
account as the launch's shell environment resolves it: a `CLAUDE_CONFIG_DIR` the user's shell exports, else
`~/.claude` — and the project-level one, the `.claude/` directory at the repository root and in any subdirectory the
task touches. It tells Codex, before planning or changing any file, to read at both levels what the task needs: the
settings files and the plugins they enable, the `CLAUDE.md` files with their `@` imports (and `CLAUDE.local.md` and
`.claude/CLAUDE.md`), skills, commands, agents and rules, and the documentation the entry files point to; that Claude
Code's MCP servers have already been added, so it is not to set them up itself; to apply `permissions.deny`,
`permissions.ask` and blocking hooks in full by intent, never lifted by agreement in the conversation, to carry out
other hooks by intent at the moment they would run, and not to use permission allowlists to get around those
restrictions; not to modify `.claude/` or create Codex configuration unless asked; that system, developer and user
instructions and the applicable `AGENTS.md` outrank the section, except that a Claude-side restriction holds against a
conflicting user instruction; and to say in one sentence which kinds of Claude rules it adopted and carry on. Codex
follows this as instructions: Octoboard does not check what it reads or how it follows it.

**Claude Code's MCP servers** for the session's working directory are passed as `-c mcp_servers.<name>.…` overrides.
They are gathered as Claude Code gathers them, by server name, a later scope replacing an earlier one:

| Scope | Where it is read | Taken when |
|---|---|---|
| User | `mcpServers` at the top of Claude Code's global config file: `.claude.json` inside a `CLAUDE_CONFIG_DIR` the user's shell exports, else `~/.claude.json` | Always |
| Project | `mcpServers` of each `.mcp.json` from the filesystem root down to the working directory, the nearest one winning | Only once approved: `enableAllProjectMcpServers` is true, or the server is listed in `enabledMcpjsonServers`, and it is not listed in `disabledMcpjsonServers`, in any of the user-level `settings.json` (in the user-level directory above), the working directory's `.claude/settings.local.json` and the working directory's entry under `projects` in the global config file. The project's shared `.claude/settings.json` approves nothing, so a repository cannot approve its own servers; managed settings are not read, so a server approved only there stays off |
| Local | `mcpServers` of the working directory's entry under `projects` in the global config file | Always |

A server that the working directory's entry lists in `disabledMcpServers` is dropped from every scope. Plugins'
servers are not passed. A server is also left out when:

- its name is `octoboard` (Octoboard's own), or is not made only of letters, digits, `-` and `_`;
- a server of the same name is defined in the `config.toml` of the Codex home the launch uses, or in a project
  `.codex/config.toml` — the working directory's and each one above it up to the repository root (the nearest directory
  holding `.git`; the working directory's alone outside a repository). Codex's own definition then stands. When one of
  these files exists but cannot be read or parsed, no Claude Code server is passed at all;
- it uses a transport other than stdio and streamable HTTP (`"type": "http"`) — SSE, for one — or has a
  `headersHelper`, or a field of the wrong type, or no `command` (stdio) or `url` (HTTP);
- a `${VAR}` in it has no `:-default` and `VAR` is not set in the launch's shell environment;
- its `url` has a `${VAR}` that the environment fills, carries credentials (`user:pass@`) or has a query string;
- an `env` entry has a name that cannot be a variable's, or an `env` or header value holds a NUL byte;
- an `env` entry names `TERM`, a `GIT_CONFIG_*` or an `OCTOBOARD_*` variable, at any value;
- an `env` entry names a variable that steers Codex itself — `CODEX_*`, `OPENAI_*`, `RUST_LOG`, `RUST_BACKTRACE`,
  `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` and `NO_PROXY` in either letter case, `SSL_CERT_FILE`, `SSL_CERT_DIR`,
  `NODE_OPTIONS`, `DYLD_*`, `LD_*` — at a value other than the one the launch already gives it;
- an `env` entry would give a variable a value other than the one the user's shell, the launch or a server passed
  before it gives it.

A server left out is left out whole, and the others are still passed; nothing in the window says which were left out.

**What reaches Codex's command line.** A stdio server's `command`, `args` and `cwd`, with their variables expanded, and
an HTTP server's `url` are on Codex's own argv, which other users of the machine can list; `command` and `args` become
the server's own argv when Codex starts it anyway, as under Claude Code. Header values and `env` values are kept off
it: each header value is put in the Codex process's environment under a variable of Octoboard's own,
`OCTOBOARD_CLAUDE_MCP_<n>_<m>_TOKEN`, which the server's `env_http_headers` names; each `env` entry is passed by name
through the server's `env_vars`, and an entry the user's shell does not already hold at that value is set, under its
own name, on the Codex process as a whole rather than on the server alone.

## Grok Build

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
  explicitly selects it again. Reasoning effort, Octoboard's MCP entry and every other setting stay in the copy. The
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
taken on every launch as well, whichever of them the source home holds. A bundle update Grok downloads during a session
lands in Octoboard's copy and is not kept; the next launch copies whatever the user's own `grok` has fetched since.

Three entries of the source home are left out altogether. Its `hooks/` directory is not linked: the per-session home's
`hooks/` holds Octoboard's status hooks alone, so hooks the user keeps in the Grok home's own `hooks/` directory do not
run in a session Octoboard starts, while a project's own hooks still do (see "A project's hooks and instruction files"
below). Its `requirements.toml` and `managed_config.toml` are not carried either (see "Grok's own sandbox" below).

### Grok's own sandbox

**A Grok Build launch that would run under Grok's own sandbox is refused** before the per-session home is built, with
the code `grok_sandbox_unsupported`, whose message names the sandbox profile and what sets it. The sandbox lets Grok
write only to its own home, the working directory and temporary directories, judged by where links really lead, and the
per-session home's login and session records are links back into the source home, so a sandboxed session could not even
create its conversation. The alert's remedy is to turn the sandbox off where it is set, or to have whoever manages that
setting do so; a conversation begun under the sandbox keeps it, so it is to be continued in Grok itself. A profile of
`off` is no sandbox. Which profile applies is read as Grok layers it:

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

### Claude compatibility for Grok Build

Grok Build has Claude Code support of its own, for Claude Code's skills, rules, agents, MCP servers and hooks. While
Grok Build's Claude compatibility is on (see "Claude compatibility" in `docs/product/per-agent-settings.md`), every
launch, a resume and an account switch included, sets `GROK_CLAUDE_SKILLS_ENABLED`, `GROK_CLAUDE_RULES_ENABLED`,
`GROK_CLAUDE_AGENTS_ENABLED`, `GROK_CLAUDE_MCPS_ENABLED` and `GROK_CLAUDE_HOOKS_ENABLED` to `true`, which outrank a
`false` for any of them in Grok's configuration file or the user's shell. While it is off, none of them is set by
Octoboard, and Grok follows its own configuration and whatever the user's shell exports. Nothing is written into the
Grok configuration for it.

### A Grok Build config directory

A Grok Build config directory must **already be a Grok home, one Grok has been run against**. Only entries that already
exist in the source home are linked; for an empty directory, Grok creates its login and session records inside the
per-session home, which is discarded with the session's process, so neither the login nor the conversation persists.
Octoboard does not check this when the directory is set — a user pointing it at a directory they are about to create
should not be stopped. It is checked at **launch** instead, and a Grok Build session pinned to a directory that is not
an initialized Grok home is refused there, naming the directory and what is wrong with it; a session on the default
account is never held to this, since that directory is the user's own existing setup.

### A project's hooks and instruction files

Grok Build loads **a project's own hooks only inside a git repository**: it locates a project by walking up for a `.git`
directory, and in a directory without one it loads none of the project's hooks. Its instruction files need no repository
— in a directory that is not one, Grok reads those in the working directory alone — but, like the project's hooks and
MCP servers, they load only once the folder is trusted (see `docs/product/folder-trust.md`). That is how a Grok console
session reads the console's instruction file (see "The console session's instruction file" in
`docs/product/launching-agents.md`).
