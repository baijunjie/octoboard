# Fact sheets of the five agent CLIs

What Octoboard needs to know about Gemini CLI, OpenCode, GitHub Copilot CLI, Pi and Qwen Code before each becomes a
provider, set against what the three existing providers rely on. The overview and the milestones are in `README.md` in
this directory; each provider milestone turns its own sheet into measured facts in `docs/agent-cli-reference.md`.

## How to read a sheet

Every fact carries where it comes from:

- **Source**: read from the installed package: its code, its bundled docs or its own `--help` and help topics. Holds
  for the installed version only.
- **Docs**: the vendor's web documentation. It may describe a newer version than the one installed.
- **Unmeasured**: nothing was run. No CLI was started as a TUI, no model was called and nothing was signed in for these
  sheets, so everything about what a running session draws, fires or writes is unmeasured until its milestone measures
  it.
- **Needs a login**: can only be measured with a real account, which the user signs in during the milestone.

Installed versions, from `npm root -g` on the development machine on 2026-11-20:

| Agent | Package | Binary | Version | How the package runs |
|---|---|---|---|---|
| Gemini CLI | `@google/gemini-cli` | `gemini` | 0.69.0 | Node, readable bundle, bundled docs |
| OpenCode | `opencode-ai` | `opencode` | 1.19.6 | Bun-compiled executable; code readable only as strings |
| GitHub Copilot CLI | `@github/copilot` | `copilot` | 1.0.114 | Node loader that starts a native executable; code not readable, help topics complete |
| Pi | `@earendil-works/pi-coding-agent` | `pi` | 1.5.2 | Node, bundled docs and type declarations |
| Qwen Code | `@qwen-code/qwen-code` | `qwen` | 0.28.0 | Node, minified chunks, no bundled docs |

All five rewrite their surfaces on upgrade, as the existing three do. A milestone re-checks its sheet against the
version installed when it starts.

## What the existing providers rely on

The shape the generic layer was built around (`docs/agent-cli-reference.md`, "Injecting Octoboard into each agent"):

| Need | Claude Code | Codex | Grok Build |
|---|---|---|---|
| Status | Hooks, payload JSON on stdin naming its event | Hooks, same | Hooks, same |
| Hook injection per launch | `--settings` | `-c hooks.<Event>=…` | A per-session home with its own `hooks/` |
| MCP injection per launch | `--mcp-config` | `-c mcp_servers.octoboard.…` | A block in the per-session `config.toml` |
| Role text | `--append-system-prompt` | `-c developer_instructions=…` | `--rules` |
| Agent session id | Pre-allocated | From the first hook | Pre-allocated |
| Account directory | One variable names it (`CLAUDE_CONFIG_DIR`) | Same (`CODEX_HOME`) | Same (`GROK_HOME`) |
| Sign-in | Subscription OAuth, driven without a terminal | Same | Same |
| Plan quota | Read through the CLI's own channel | Same | Same |
| Records | JSONL files under the account directory | Same | A directory per session |
| MCP tool name | `mcp__<server>__<tool>` | `mcp__<server>__<tool>` | `<server>__<tool>` |

## Comparison of the five

| Need | Gemini CLI | OpenCode | Copilot CLI | Pi | Qwen Code |
|---|---|---|---|---|---|
| Status surface | Hooks, plus a status title | In-process plugin events; no hooks | Hooks, Claude-compatible names offered | OSC 7501 reports, natively; extension events | Hooks with Claude-style names; status title prefix |
| Hook or extension injection per launch | A settings file named by a variable | A config directory or inline config named by a variable | `--plugin-dir` | `--extension <path>` | A settings file named by a variable (unmeasured) |
| MCP injection per launch | Same settings file | Same config | `--additional-mcp-config` | The extension registers the server | `--mcp-config` |
| Role text | No append flag; context file or `SessionStart` hook output | `instructions` in the injected config | A directory of instruction files named by a variable | `--append-system-prompt` | `--append-system-prompt` |
| New session with a given id | `--session-id <uuid>` | No | `--session-id=<uuid>` | `--session-id <id>` | `--session-id <id>` |
| Resume by id | `--resume <uuid>` | `--session <id>` | `--resume=<id>` | `--session <id>` | `--resume <id>` |
| Account directory variable | `GEMINI_CLI_HOME` names the **parent** of `.gemini` | None for data; XDG variables only | `COPILOT_HOME` | `PI_CODING_AGENT_DIR` | `QWEN_HOME` |
| Sign-in model | Google OAuth, or API key, or Vertex | Provider credentials and keys, many at once | GitHub OAuth (web or device code), or token | Provider keys or provider OAuth | Subscription or API key; Qwen OAuth discontinued |
| Plan quota | Per-model daily buckets; no known command | None for API keys | Plan budget, Session and Weekly; no known command | None | Unmeasured |
| Records | JSONL per session, rewritten at times | SQLite database | A directory per session with `events.jsonl` | JSONL per session, a tree | JSONL per session |
| MCP tool name | `mcp_<server>_<tool>` | `<server>_<tool>` (docs) | Unmeasured | `mcp__<server>__<tool>` | `mcp__<server>__<tool>` |
| Permission prompts | Yes | Yes | Yes | None by default | Yes |
| ACP mode | `--acp` | `opencode acp` | `--acp` | No (has `--mode rpc`) | `--acp` |

## Gemini CLI

**Launch and resume** (Source: `--help`)

- Interactive by default; a positional prompt starts the first turn (`-i` does the same explicitly); `-p` is headless.
- `--session-id <uuid>` starts a new session under that id; `--resume <uuid|index|latest>` resumes; the two and
  `--session-file` are mutually exclusive.
- `--model <id>`; no reasoning-effort flag.
- Sessions are filed per project, so a resume has to run in the session's own working directory.

**Records** (Source: code and the package's session management page)

- `~/.gemini/tmp/<project id>/chats/session-<timestamp>-<first 8 of the id>.jsonl`. The project id is a short slug
  that `~/.gemini/projects.json` maps from the project root; older versions used a hash of the path, and startup
  migrates those directories.
- The first line is the session's metadata (`sessionId`, `projectHash`, `startTime`, `lastUpdated`, `kind`); later
  lines are message records and `$set` patches. The file is at times **rewritten whole**, so it is not append-only.
  Subagents write `<parent id>/<id>.jsonl` beside it.
- Sessions older than 30 days are deleted automatically by default (`general.sessionRetention`), so a missing record
  is an ordinary event.

**Status surface** (Source: the package's hooks reference, code)

- Hooks, configured under `hooks` in a settings file, `type: "command"` only, payload JSON on stdin carrying
  `session_id`, `transcript_path`, `cwd`, `hook_event_name` and `timestamp`. Events: `SessionStart` (`source`
  `startup`, `resume` or `clear`), `SessionEnd`, `BeforeAgent` (`prompt`), `AfterAgent` (`prompt`, `prompt_response`,
  `stop_hook_active`), `BeforeTool`, `AfterTool` (`tool_name`, `tool_input`), `Notification` (`notification_type`
  `ToolPermission`), `BeforeModel`, `AfterModel` (per stream chunk), `BeforeToolSelection`, `PreCompress`.
- A hook is synchronous, its default timeout is 60 s, exit status 2 blocks, and anything it prints to stdout must be
  JSON. A hook process gets `GEMINI_SESSION_ID`, `GEMINI_PROJECT_DIR`, `GEMINI_CWD`, `GEMINI_PLANS_DIR` and
  `CLAUDE_PROJECT_DIR`, in an environment Gemini sanitizes first.
- **Hooks from settings do not run in a folder that is not trusted**: the code registers them all as project hooks and
  skips them with "Project hooks disabled because the folder is not trusted".
- The terminal title carries the status by default (`ui.dynamicWindowTitle`): `◇  Ready (<folder>)`,
  `✦  Working… (<folder>)`, `✋  Action Required (<folder>)` and `⏲  Working… (<folder>)`, padded to 80 columns. It
  never carries a conversation title.
- Unmeasured: whether a turn cancelled with Esc fires `AfterAgent`; what fires around its ask-the-user tool
  (`ask_user`); the order of `Notification` against the permission dialog.

**Injection** (Source: the package's configuration reference, code)

- `GEMINI_CLI_SYSTEM_SETTINGS_PATH` names the system settings file, the highest-precedence settings layer;
  `GEMINI_CLI_SYSTEM_DEFAULTS_PATH` names the lowest. Across layers the `hooks.<Event>` arrays are concatenated and
  `mcpServers` is merged by server name, so a per-session file adds Octoboard's hooks and server beside the user's.
  The file at the default path (`/Library/Application Support/GeminiCli/settings.json` and its counterparts) is no
  longer read once the variable is set, so the adapter has to carry its content over.
- MCP: an `mcpServers.octoboard` entry with `command`, `args`, `timeout` (milliseconds) and `trust`.
- Role text: there is no append flag. `GEMINI_SYSTEM_MD` replaces the whole system prompt and is not usable. The
  candidates, in the order to measure them: a context file in a scratch directory brought in through
  `context.includeDirectories` with `context.loadMemoryFromIncludeDirectories`; the `SessionStart` hook's
  `hookSpecificOutput.additionalContext`, which needs a second hook command that prints JSON.

**Configuration directory and accounts** (Source: code)

- `GEMINI_CLI_HOME` replaces the home directory; the CLI creates and uses `.gemini` inside it. No variable names the
  `.gemini` directory itself.
- `GEMINI_CLI_TRUSTED_FOLDERS_PATH` moves the trust store alone.
- Even `gemini --help` writes into `~/.gemini` (`projects.json`).

**Sign-in** (Source: code, bundled docs)

- Auth types: `oauth-personal` (Log in with Google), `gemini-api-key`, `vertex-ai`, `compute-default-credentials`,
  `gateway`; the chosen one is kept in settings (`security.auth.selectedType`), and environment variables can select
  one.
- The Google login is `oauth_creds.json` in `.gemini`, with the signed-in address in `google_accounts.json`. There is
  no `gemini auth` command; sign-in happens in the TUI.
- Needs a login: whether ACP's `authenticate` method drives the Google sign-in without a terminal; whether the
  credential lands in the file or the system keychain on macOS.

**Models and quota** (Source: code, bundled docs)

- `gemini models` (`--output-format json`) and `--list-models` list models. Needs a login: what they print signed out.
- Quota: with a Google login the CLI asks its Code Assist backend for per-model buckets (`modelId`,
  `remainingFraction`, `remainingAmount`, `resetTime`) and shows them in `/stats model` and `/model`; the limits are
  daily request counts. No command prints them, and ACP's prompt response carries token counts only. An API key has no
  quota reading.

**Permissions** (Source: `--help`): `--approval-mode default | auto_edit | yolo | plan`.

**Trust** (Source: bundled docs, code): folder trust is on by default per the settings reference (the bundled Trusted
Folders page still says off). The dialog offers "Trust folder", "Trust parent folder" and "Don't trust" and records in
`trustedFolders.json`. `--skip-trust` and `GEMINI_CLI_TRUST_WORKSPACE=true` trust for one session without recording.

**Other facts** (Source)

- Instruction file: `GEMINI.md` by default (`context.fileName` can name others).
- MCP tools reach the model as `mcp_<server>_<tool>`, single underscores, cut to 63 characters.
- Processes it starts get `GEMINI_CLI=1`.
- Ask-the-user tool: `ask_user`.
- Installed through npm or Homebrew (`gemini-cli`).

**Windows**: unmeasured. The npm install gives a `gemini.cmd` shim; hook commands run through PowerShell there
(Source: code).

## OpenCode

The installed version is 1.19.6. The vendor's current web documentation describes a newer generation with different
config keys (`plugins`, `permissions`), another plugin interface and `/api/…` server routes; the facts below are the
installed version's, with the web's version-1 pages where marked Docs.

**Launch and resume** (Source: `--help`)

- `opencode [project]` starts the TUI; `--prompt <text>` gives the first prompt; `--session <id>` continues a session;
  `--model <provider/model>`; `--agent <name>`. No flag takes an id for a new session.
- `opencode serve` runs a headless server and `opencode attach <url>` attaches a TUI to it; the TUI's own `--port`
  and `--hostname` make its server reachable. `OPENCODE_SERVER_PASSWORD` protects a server with basic auth.

**Records** (Source: strings)

- A SQLite database, `opencode.db`, under the data directory (`~/.local/share/opencode`). There are no per-session
  files. `opencode session list --format json` lists sessions and `opencode export <id>` prints one as JSON.

**Status surface** (Source: strings; Docs: version-1 plugins page)

- No hooks. A plugin is a JavaScript module that OpenCode loads into its own process; it receives every bus event
  through its `event` hook and can act on `tool.execute.before`, `tool.execute.after`, `shell.env` and others. Events
  include `session.created`, `session.status`, `session.idle`, `session.error`, `permission.asked`,
  `permission.replied`, `question.asked`, `question.replied`, `question.rejected`, `message.updated` and
  `message.part.updated`.
- The same events stream from the server's event route, for a session whose server is reachable.
- The terminal title is `OpenCode`, or `OC | <session title>` in a session; it carries no status.
  `OPENCODE_DISABLE_TERMINAL_TITLE` turns it off.

**Injection** (Source: strings)

- `OPENCODE_CONFIG_DIR` names one more configuration directory, read like a project's `.opencode` directory: its
  `opencode.json` and its plugin directory. `OPENCODE_CONFIG_CONTENT` carries a whole config as inline JSON, and
  `OPENCODE_CONFIG` names one more config file. Configs are merged.
- MCP: an `mcp.octoboard` entry. Role text: `instructions`, a list of files. Plugins: the `plugin` list, or files in a
  plugin directory.
- Unmeasured: the precedence between these variables and a user who already sets them; whether the `plugin` list
  takes a file path in this version; the per-server timeout key and its default.

**Configuration directory and accounts** (Source: strings)

- Four directories by the XDG rules: config (`~/.config/opencode`), data (`~/.local/share/opencode`, which holds
  `auth.json` and the database), state and cache. **No variable of OpenCode's own moves the data directory**; only
  `XDG_DATA_HOME` does, and every program the agent starts would inherit it.
- `OPENCODE_AUTH_CONTENT` supplies the credentials as inline JSON in place of `auth.json`.

**Sign-in** (Source: `--help`)

- `opencode auth login [-p <provider>] [-m <method>]`, `auth list`, `auth logout`. One installation holds
  credentials for many providers at once; the model's `provider/model` id picks which is used. Entering a key is an
  interactive prompt.

**Models and quota**: `opencode models [provider]` lists `provider/model` ids (`--refresh` re-reads the catalogue).
No plan quota for API keys; `opencode stats` reports local token and cost totals.

**Permissions** (Source: `--help`, strings): `--auto` approves everything not explicitly denied; finer rules come
from the `permission` config and from `OPENCODE_PERMISSION`.

**Other facts**

- Instruction file: `AGENTS.md` (Docs). OpenCode also reads Claude Code's instructions and skills unless
  `OPENCODE_DISABLE_CLAUDE_CODE` and its companions say otherwise (Source: strings).
- MCP tools reach the model as `<server>_<tool>` (Docs; unmeasured).
- Processes it starts get `AGENT=1`, `OPENCODE=1` and `OPENCODE_PID` (Source: strings).
- The ask-the-user tool sits behind `OPENCODE_ENABLE_QUESTION_TOOL` (Source: strings; unmeasured).
- ACP: `opencode acp`.

**Windows**: unmeasured. The package ships native Windows executables.

## GitHub Copilot CLI

The executable's code is not readable; these facts are from its `--help` and its help topics (`copilot help
<topic>`), which are thorough, and from GitHub's hooks reference where marked Docs.

**Launch and resume** (Source)

- `copilot -i <prompt>` starts interactive mode on a prompt; `--session-id=<uuid>` sets the id of a new session, or
  resumes an existing one; `--resume=<id>` resumes; `--model <id>`; `--reasoning-effort <level>`.
- `--resume` changes to the directory recorded for the session unless `COPILOT_DISABLE_RESUME_AUTO_CD` is set.
- The `copilot` on `PATH` from npm is a Node loader that starts the native executable of the platform package as its
  child and forwards signals to it.

**Records** (Source): `$COPILOT_HOME/session-state/<session id>/` holds `events.jsonl`, `workspace.yaml`, `plan.md`,
`session.db`, `checkpoints/` and `files/`; `session-store.db` indexes sessions. Nothing expires them. When signed in,
history is also synced read-only to the GitHub account unless turned off.

**Status surface** (Source; Docs: hooks reference)

- Hooks: `sessionStart`, `sessionEnd`, `userPromptSubmitted`, `preToolUse`, `permissionRequest`, `postToolUse`,
  `postToolUseFailure`, `agentStop`, `subagentStart`, `subagentStop`, `errorOccurred`, `preCompact` and
  `notification` (`notification_type` `permission_prompt`, `elicitation_dialog`, `agent_idle`, `agent_completed`,
  `shell_completed`, …). Each also answers to a Claude Code name (`SessionStart`, `UserPromptSubmit`, `PreToolUse`,
  `PostToolUse`, `Stop`, …), which selects a payload with Claude Code's field names (`session_id`, `tool_name`,
  `tool_input`); the native payload uses `sessionId`, `toolName`, `toolArgs`.
- Entries are `command` (with `bash`, `powershell` or a shell-free `exec` and `args`), `http` or `prompt`; the default
  timeout is 30 s; `notification` hooks are not waited for; a `preToolUse` command that exits non-zero denies the
  call, and a timeout never does. HTTP hooks to loopback need `COPILOT_HOOK_ALLOW_LOCALHOST=1`.
- Hooks load from policy, the user's `$COPILOT_HOME/hooks/` and `settings.json`, the repository, and plugins.
- The terminal title shows "what the agent is currently working on" (`updateTerminalTitle`,
  `COPILOT_DISABLE_TERMINAL_TITLE`), and the CLI emits OSC 9;4 progress while it works (`terminalProgress`).
- Unmeasured: whether the native payload names its event; what the title reads at the prompt.

**Injection** (Source)

- `--plugin-dir <directory>` loads a plugin for one run; a plugin carries `hooks.json` and more.
  `COPILOT_PLUGIN_DIR_ONLY` is for turning discovery off and is not wanted here.
- `--additional-mcp-config <json | @file>` adds MCP servers for one session, over the user's.
- Role text: `COPILOT_CUSTOM_INSTRUCTIONS_DIRS` names more directories searched for instruction files; the
  `sessionStart` hook's `additionalContext` is the alternative.
- `--allow-tool` rules allow a server's tools without a prompt.

**Configuration directory and accounts** (Source): `COPILOT_HOME` names the directory, default `~/.copilot`:
`settings.json`, `config.json` (trusted folders, signed-in accounts, plugins), `mcp-config.json`, instructions,
`session-state/`. Cached downloads live apart, under `COPILOT_CACHE_HOME`.

**Sign-in** (Source: `copilot login --help`)

- `copilot login`: a browser flow on a desktop, `--device-code` for the device-code flow, `--with-token` to read a
  token from stdin. The token is kept in the system credential store, or in plain text under the home when there is
  none. `COPILOT_GITHUB_TOKEN`, `GH_TOKEN` and `GITHUB_TOKEN` override a stored login.
- Needs a login: how several homes share or separate the stored tokens; where the signed-in account's name is read.

**Models and quota** (Source: `copilot help billing`, `limits`)

- No command lists models; `/model` does in the TUI. ACP is the candidate for listing them.
- Usage is counted in AI credits, or premium requests on legacy billing. `/usage` shows the plan's budget (percent
  used, when it resets) and the Session and Weekly usage limits. `--usage-output-file` writes one session's usage
  only. Needs a login: a way to read the account-wide figures without the TUI.

**Permissions** (Source): `--mode interactive | plan | autopilot`, `--allow-all` (`--yolo`), `--allow-all-tools`,
`--allow-tool` and `--deny-tool` rules; `defaultPermissionMode` in settings.

**Trust** (Source): a folder-trust prompt, remembered in `config.json` (`trustedFolders`); `--trust-folder` covers
prompt mode only. Workspace MCP files and hooks load only once the folder is trusted.

**Other facts** (Source)

- Instruction files: `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, `.github/copilot-instructions.md` and more, from the
  repository root and the working directory. It reads several of the names the other providers use, so a directory
  holding more than one may be read more than once (unmeasured).
- It reads much of a project's Claude Code configuration (`.claude/` skills, agents, commands, hooks).
- Processes it starts get `COPILOT_CLI=1` and `COPILOT_AGENT_SESSION_ID`; hooks also get `COPILOT_PROJECT_DIR`,
  `COPILOT_HOOK_DIR` and `COPILOT_HOOK_DEPTH`.
- Ask-the-user tool: `ask_user` (`--no-ask-user` turns it off).
- Keys: Esc twice interrupts a turn, Ctrl-C stops it at once; Ctrl-S stashes the draft.
- ACP: `--acp`, with opt-in `assistant.turn_start`, `assistant.idle` and `session.idle` notifications.
- Unmeasured: the name an MCP tool reaches the model under.

**Windows**: unmeasured. Hook entries take a `powershell` command beside `bash`.

## Pi

**Launch and resume** (Source: `--help`, bundled docs)

- `pi [messages…]` starts the TUI on the given prompts. `--session-id <id>` opens the working directory's session of
  that id and creates it when there is none, so one flag serves a new session and a resume; `--session <path|id>`
  opens any session.
- `--model <pattern>` (`provider/id`, with an optional `:<thinking>`), `--thinking <level>`,
  `--append-system-prompt <text>`.
- `--mode rpc` keeps a process driven by JSON lines on stdin and stdout; it has no TUI.

**Records** (Source: the package's session format page)

- `~/.pi/agent/sessions/--<path>--/<timestamp>_<session id>.jsonl`, one JSON object a line, the entries a tree by
  `id` and `parentId`. `PI_CODING_AGENT_SESSION_DIR` and `--session-dir` move the store.

**Status surface** (Source: the package's terminal setup and extensions pages)

- **Pi reports its state through OSC 7501, the Program Status Protocol, by itself**, once the terminal answers the
  support query: `working`, `blocked` (a dialog or login waits), `done`, `error`, `idle`. `PI_PROGRAM_STATUS=1` sends
  reports without asking. This is the protocol Octoboard's terminals already answer and turn into a status for an
  agent with no status integration (`docs/product/program-status-reports.md`).
- No hooks. An extension is a TypeScript or JavaScript module loaded into Pi; it can subscribe to `session_start`,
  `agent_start`, `agent_end`, `agent_settled` (the final-idle signal, with `aborted`), `turn_start`, `turn_end`,
  `tool_call`, `tool_execution_end`, `ui_prompt_start`, `ui_prompt_end`, `input` and more, and can send a user
  message, abort a run and set the session's name.

**Injection** (Source)

- `--extension <path>` loads an extension file for one run. An extension can register an MCP server for the session
  (`registerMcpServer`, with `exposure`, `timeout` and the usual `command`, `args`, `env`).
- MCP servers default to an exposure that does not declare their tools to the model; `direct` declares them.
- Role text: `--append-system-prompt`.

**Configuration directory and accounts** (Source): `PI_CODING_AGENT_DIR` names the directory, default `~/.pi/agent`:
`auth.json`, `settings.json`, `mcp.json`, `trust.json`, `sessions/`, extensions.

**Sign-in** (Source: `pi auth --help`)

- `pi auth login <provider> [--api-key] [--no-browser]`, `pi auth logout <provider>`, and `pi auth check [--provider]
  [--json]`, which prints `ready`, `not_ready` or `invalid` and exits 0, 1 or 2. Credentials are in `auth.json`;
  provider keys are also read from the environment.

**Models and quota**: `pi --list-models [search]`. No plan quota.

**Permissions**: none. Pi asks for no approval before a tool call; an extension can gate calls.

**Trust** (Source: the package's security page): a project-trust prompt appears only when the folder holds protected
resources (`.pi/settings.json`, `.pi/mcp.json`, `.pi/extensions`, …). `--approve` and `--no-approve` decide for one
run; decisions are saved in `trust.json`.

**Other facts** (Source)

- Instruction files: `AGENTS.md`, `CLAUDE.md` and `AGENTS.override.md`, read whatever the trust decision.
- MCP tools are `mcp__<server>__<tool>`.
- Processes it starts get `AI_AGENT=pi` and `PI_CODING_AGENT=true`; the commands its shell tools run also get
  `PI_SESSION_ID`, `PI_SESSION_FILE`, `PI_PROVIDER`, `PI_MODEL` and `PI_REASONING_LEVEL`.
- Ask-the-user tool: `ask_question`.
- `pi update` updates Pi itself; the npm package is the install.
- Unmeasured: its terminal title; the keys that interrupt a turn; whether a pasted message submits like the others'.

**Windows**: unmeasured. The package bundles a page on Windows.

## Qwen Code

A fork of Gemini CLI that has since moved toward Claude Code's surface. No docs are bundled; the facts are from
`--help` and the minified code.

**Launch and resume** (Source: `--help`)

- A positional prompt is one-shot by default; `-i <prompt>` runs it and stays interactive. `--session-id <id>` names
  the session; `--resume <id>` resumes; `--model <id>`.
- `--append-system-prompt <text>` and `--mcp-config <json | path>` exist as flags.
- `--json-file <path>` and `--json-fd <n>` write structured events beside the normal TUI, and `--input-file <path>`
  makes the TUI take commands from a file. Unmeasured: what the events are.

**Records** (Source: code): `<Qwen home>/projects/<sanitized working directory>/chats/<session id>.jsonl`, with a
`<session id>.runtime.json` and a `<session id>.ledger.jsonl` beside it.

**Status surface** (Source: code)

- Hooks with Claude Code's event names: `SessionStart`, `SessionEnd`, `UserPromptSubmit`, `PreToolUse`,
  `PostToolUse`, `PostToolUseFailure`, `PermissionRequest`, `PermissionDenied`, `Notification`, `Stop`, `StopFailure`,
  `SubagentStart`, `SubagentStop`, `PreCompact`, `PostCompact` and more.
- The terminal title is `Qwen - <folder>` or the session's name, with a status prefix while it responds (`◐`) and
  while it waits for a confirmation.
- Unmeasured: the payload fields; whether hooks need an approval before they run (`QWEN_CODE_HOOK_APPROVALS_PATH`
  suggests a store of approvals); whether hooks run in an untrusted folder.

**Injection** (Source: code)

- `QWEN_CODE_SYSTEM_SETTINGS_PATH` and `QWEN_CODE_SYSTEM_DEFAULTS_PATH`, as in Gemini CLI. Unmeasured: how hook and
  MCP entries merge across the layers in this fork.
- MCP: `--mcp-config`. Role text: `--append-system-prompt`.

**Configuration directory and accounts** (Source: code): `QWEN_HOME` names the directory itself, default `~/.qwen`.
`QWEN_RUNTIME_DIR` moves runtime state alone; `QWEN_CODE_TRUSTED_FOLDERS_PATH` the trust store.

**Sign-in** (Source: `--help`, code)

- `--auth-type openai | openai-responses | anthropic | qwen-oauth | qwen-subscription | gemini | vertex-ai`. The
  `qwen auth` command is removed and Qwen OAuth is labelled discontinued in the interface; what remains for a Qwen
  account is the subscription and the Coding Plan, beside provider keys.
- Needs a login: which of these the user's Qwen account is, where its credential is kept (`oauth_creds.json` exists
  as a name) and whether any of it can be driven without the TUI.

**Models and quota**: no listing command; ACP is the candidate. Quota: unmeasured.

**Permissions** (Source): `--approval-mode plan | default | auto-edit | auto | yolo`.

**Other facts** (Source)

- Instruction files: `QWEN.md` and `AGENTS.md`.
- MCP tools are `mcp__<server>__<tool>`.
- Processes it starts get `QWEN_CODE_SESSION_ID`, `QWEN_CODE_CLI` and `QWEN_CODE_PROJECT_DIR`. Qwen relaunches
  itself as a child unless `QWEN_CODE_NO_RELAUNCH` is set.
- Ask-the-user tool: `ask_user_question`.
- `qwen serve` is an HTTP daemon with `qwen attach`; ACP: `--acp`.

**Windows**: unmeasured.
