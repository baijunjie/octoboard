# Octoboard MVP Plan

## 1. Positioning

Octoboard is a desktop control board for orchestrating agents across multiple projects. In the **console**, the user gives a
request to a hub agent; the hub agent decides which project the task belongs to, starts a dedicated agent session in that
project's directory, and hands it the task. When the project session finishes it reports back to the hub, and the hub reports
to the user. The user can switch to any session at any time to watch it or take over directly.

It is neither a new agent nor a new terminal: every session is a native agent CLI process (Claude Code, Codex, Grok Build,
etc.). Octoboard only adds a layer of organization, orchestration, and observation on top. It is not tied to any agent
vendor, and the retention and cleanup of session data follows each agent's own rules.

## 2. Core Concepts

| Concept | Description |
|---|---|
| Console | A management unit for a group of projects. Multiple consoles can exist, independent of each other. Each console has its own working directory and its own hub agent. |
| Host | A machine that runs sessions. The MVP has only the local machine; remote hosts can be added later (section 9). |
| Project | A directory on some host, associated with a console. Sources: a single directory / a parent directory (git repositories beneath it are discovered automatically) / a GitHub URL (cloned, then associated). |
| Session | An agent CLI process running in a project directory. It can be created by the hub or manually by the user. |
| Hub Session | The agent process running in the console's working directory, equipped with the orchestration tools Octoboard provides. |
| Agent Adapter | The layer that plugs an agent CLI into Octoboard: how to launch it, inject capabilities, report status, and resume sessions (section 6). |

Navigation is a three-level menu: console → project → session. Opening any menu item shows the terminal of the corresponding
session.

## 3. Interface

```
┌──────────────────┬───────────────────────────────┬──────────────────────┐
│ Console A     ▾  │                               │                      │
│  ◉ Hub           │                               │                      │
│  ▸ project-alpha │      Terminal (xterm.js)      │   Report panel       │
│     ⟳ Fix login  │                               │   hub session only   │
│     ✋ New ask    │                               │   ◀ history ▶        │
│     ⏸ Refactor   │                               │                      │
│     ▸ Archive(12)│                               │                      │
│  ▸ project-beta  │                               │                      │
│ Console B     ▸  │                               │                      │
└──────────────────┴───────────────────────────────┴──────────────────────┘
```

- Session states in the left-hand tree are listed in 5.7. The raised-hand state bubbles up to the project and console nodes
  so it stays visible when they are collapsed; it also fires a system notification and shows a count on the Dock icon.
- Each session is labelled with the agent it uses (Claude / Codex / Grok icon).
- Each project has an "Archive" group listing its archived sessions; clicking one reopens it.
- Center: the terminal of the selected session. The user can type into it directly, exactly as if operating the agent CLI in
  a system terminal.
- Right: the report panel, shown only in the hub session (section 7).

## 4. Architecture

### 4.1 Layers

```
┌───────────────── Desktop application (Tauri) ─────────────────┐
│ Menu tree · terminal rendering · report panel · notifications │
└───────────────────────────┬───────────────────────────────────┘
                            │ WebSocket (direct locally / SSH tunnel for remote)
                 ┌──────────┴──────────┐
         ┌───────▼────────┐    ┌───────▼────────┐
         │ Local daemon   │◀──▶│ Remote daemon  │  (after the MVP)
         │ coordinator +  │    │ host role only │
         │ host           │    │                │
         └───────┬────────┘    └───────┬────────┘
            agent processes       agent processes
```

`octoboardd` is a headless, long-running Rust process that plays two roles:

- **Host role** (present on every machine that runs sessions): manages PTYs and agent processes, receives hook reports,
  serves MCP for local sessions, lists directories, and clones repositories. It talks to concrete agents through agent
  adapters.
- **Coordinator role** (only on the user's own machine): stores console / project / session / report-page data, executes the
  hub's orchestration requests and routes them to the right host, and handles report delivery and synthesis.

The desktop application is just a client of the daemon: it renders terminals, displays status, and forwards user input.

### 4.2 Why the daemon is split out in the MVP

Both of the already-decided follow-up features depend on this split. Doing it in the MVP avoids a later rewrite:

- **Background operation**: session processes are owned by the daemon, so the daemon can keep running when the application
  exits, and the application simply reconnects next time it opens. In the MVP the daemon exits together with the application
  (section 8) — only a switch is missing.
- **Remote hosts**: a remote host runs the host role of the very same daemon. The local coordinator connects to it over an
  SSH tunnel; terminal streams, status events, and orchestration commands all use the same protocol, and the UI does not
  distinguish local from remote.

Constraint: the UI and the daemon may only interact over the network protocol — no Tauri IPC, no shared state. Otherwise
going remote breaks.

### 4.3 Technology Choices

| Layer | Choice | Notes |
|---|---|---|
| daemon | Rust + `tokio` | A single binary that can be deployed straight to a remote Linux host. |
| PTY | `portable-pty` | From the WezTerm project, cross-platform. |
| MCP server | `rmcp` (official Rust SDK) | Streamable HTTP, bound to localhost only. |
| Desktop application | Tauri 2 | The daemon ships as a sidecar with the application; native macOS APIs can be called directly from the Rust side when needed. |
| Frontend | React + TypeScript | — |
| Terminal | `xterm.js` | macOS does not allow embedding a Terminal.app / iTerm window into another application. PTY + xterm.js is the standard approach (VS Code and others do it), and terminal compatibility is xterm.js's job. |
| Report panel | sandboxed iframe (`srcdoc`) + `postMessage` | Independent of the desktop framework. |
| Storage | SQLite (`rusqlite`, coordinator side) | — |

### 4.4 Known pitfalls of the Tauri / Rust approach

Each of these has an established solution, except where a bullet says otherwise:

- **PATH and environment variables**: a macOS application launched from Finder does not inherit the user's shell PATH, so
  `claude` / `codex` (commonly installed under `~/.local/bin` or an nvm directory) cannot be found, and API keys and other
  environment variables are missing too. A **login** shell is not enough: on a zsh machine `~/.zshrc` is what puts
  `~/.local/bin` and a node-version manager's shims on `PATH`, and a login-only non-interactive zsh never reads it. The
  daemon therefore snapshots the environment from a **login + interactive** shell (`$SHELL -l -i -c 'env -0'`) and spawns
  each agent binary directly with that environment, rather than running the agent inside a shell. The snapshot is taken per
  launch, because a node-version manager's `PATH` entry can point at a per-shell-instance directory.
- **The snapshot must be filtered, not just taken.** The shell that produces it inherits the daemon's own environment and
  passes it straight through, so a daemon that was itself started from inside an agent session leaks that session's
  variables into every agent it spawns. With Claude Code this is not cosmetic: inheriting `CLAUDE_CODE_CHILD_SESSION` and
  `CLAUDE_CODE_SESSION_ID` silently turns transcript saving off and makes `--permission-mode` not apply, with no error
  anywhere; the `CLAUDE_CODE_BRIDGE_*` credential bridge belongs in the same strip list, though the observed effects are
  the other two's. Stripping them does **not** disturb authentication — with only the markers removed, `claude auth
  status` still reports a logged-in account, because the credentials live in the macOS Keychain rather than in the
  environment. Worth stating, because a login failure is the obvious thing to blame the filter for, and re-adding the
  variables to fix it would reintroduce the original bug. The daemon must strip those markers from the snapshot —
  regardless of which agent it is launching, since a daemon running inside one agent's session can spawn another.
  **Enumerate them; do not match on a name prefix.** The markers and the user's own settings share the same prefixes and
  cannot be told apart by name: `GROK_CODE_XAI_API_KEY`, `GROK_HOME`, `CODEX_HOME` and `CLAUDE_CODE_USE_BEDROCK` are all
  user settings, and dropping the first of them stops a key-authenticated session from starting at all, with a failure
  that looks like a login problem — while a Claude Code session also exports `CLAUDE_PID` and `CLAUDE_EFFORT`, which no
  obvious prefix catches. Build the list by dumping `env` inside a live session of each agent and stripping what is there
  and only there.
- **Terminal data does not go through Tauri IPC**: high-frequency terminal output would pay serialization overhead over IPC
  and would violate the constraint in 4.2. The frontend connects to the daemon over WebSocket directly.
- **Sidecar signing**: when the daemon binary is bundled with the application it must be signed and notarized along with it,
  or Gatekeeper will block it. **Partly unverified** — no Developer ID was available, so Gatekeeper
  admission on another machine is untested. Credential access is not a concern: an agent launched from the built bundle
  reached the user's Keychain login normally, because a Keychain ACL is evaluated against the agent binary's own
  signature rather than its parent's.
- **Rust toolchain floor**: the dependency graph, not Tauri itself, sets the floor. `reqwest` cannot be used below Rust
  1.88 (its `idna`/ICU chain requires it) — for loopback-only traffic a small hand-rolled HTTP client avoids the problem
  entirely — and Tauri 2 builds on older toolchains only with exact version pins on a chain of transitive crates whose own
  declared `rust-version` is higher. Either carry those pins deliberately or raise the toolchain; drifting into it by
  accident costs a day.
- **Full-width punctuation from a CJK input method needs two key presses**: a mark such as `？`, which an input method
  emits directly without a candidate window, is swallowed on the first press in `xterm.js` 5.5.0 inside WKWebView, where a
  native application takes it on the first. Composed CJK *text*, which goes through candidate conversion, is unaffected.
  The decision is to keep the framework's default composition handling rather than write one, so the fix has to come from
  the terminal library; until then the key is pressed twice, or the input method switched.
- **A packaged application needs file-access permission per volume**: the bundled `.app` raised a macOS prompt the moment
  a session's project directory lived on an external volume, which `tauri dev` never does. Projects will be scattered
  across volumes, so associating one has to cope with the user declining, or with the prompt not having been answered yet.
- **`Ctrl+C` does not reach the terminal on its own**: inside WKWebView it is swallowed above `xterm.js`, while other
  modifier combinations pass through. Since it is the most-used key in a terminal, the UI must intercept it explicitly and
  forward `0x03` itself.
- **WebView differences**: Tauri uses the system WebView (WKWebView on macOS). This has no impact on a macOS-only MVP; when
  Linux support is added later, WebKitGTK's support for xterm.js WebGL rendering needs to be verified, falling back to canvas
  rendering if necessary.

## 5. Orchestration

Key premise: every session starts with the project directory as its cwd, so the project's own agent configuration
(its instruction file, skills, hooks, permission settings, and so on) takes effect exactly as it normally would — note
each agent reads a different instruction filename, see section 6.
Octoboard's capabilities are **injected additionally** through launch arguments only; project files are never modified. The
injection mechanism differs per agent and is the adapter's responsibility (section 6).

### 5.1 Injected capabilities

When a session starts, the adapter injects three things:

- **Status hooks**: report events such as working / stopped / waiting for the user to the daemon on that host.
- **The Octoboard MCP server**: the orchestration and reporting tools (5.2).
- **A role description**: tells the agent whether it is the hub or a project worker, plus the reporting conventions.

Both hooks and MCP point at the daemon on the **host the session runs on** (localhost), so remote sessions never need to
connect back to the user's own machine.

### 5.2 Octoboard MCP tools

Tokens are issued per session, and different tools are exposed depending on the role.

**Available to the hub session:**

| Tool | Purpose |
|---|---|
| `list_projects` | List this console's projects (name, host, path, description, active sessions). |
| `add_project` | Associate a directory, or clone a GitHub repository and associate it. |
| `start_session(project, brief, agent?)` | Start a session in a project and hand it a task (the `brief` structure is described in 5.3). `agent` overrides the agent for this one session; when omitted, the project's default agent is used. Returns a session id. |
| `send_message(session, text)` | Append an instruction to a session. Delivered immediately when the session is idle *or* mid-turn — every agent queues it itself and consumes it when the turn ends — and held by the daemon only while a modal dialog is up or the session's state is unknown. |
| `get_session(session)` | Query the status and a summary of recent output. |
| `archive_session(session)` | End the session process and archive it (for the hub to wrap things up explicitly). |
| `list_archived(project)` / `reopen_session(session, text?)` | Query archived sessions; reopen an old session to continue work. |
| `show_page(html)` | Push a page of HTML to the report panel (section 7). |

**Available to project sessions:**

| Tool | Purpose |
|---|---|
| `report(summary, status, open_items[])` | Report to the hub. `status`: `done` / `failed` / `needs_decision`; `open_items` lists unfinished items. |

### 5.3 Communication conventions

Communication between the hub and project sessions is **structured in tool-call arguments and natural language in content**:

- Agents are not asked to emit JSON in their reply text. JSON in prose easily gets mixed with explanatory text, code fences,
  or formatting errors, making parsing unreliable; tool arguments, by contrast, are validated against a schema by the agent
  runtime and are inherently decidable.
- Fields a program must act on (status, unfinished items) go into structured arguments; content an agent must understand
  (task description, result summary) stays natural language, leaving the agent room to express itself.

**Handing out a task (hub → project)**, the `brief` fields:

| Field | Description |
|---|---|
| `goal` | The outcome to achieve. |
| `context` | Background and relevant leads. |
| `acceptance` | Criteria for being done. |
| `constraints` | Constraints (what must not be touched, whether committing / pushing is allowed, and so on). |

The daemon renders the `brief` into an initial prompt using a fixed template, omitting the corresponding section for each
missing field.

**Reporting (project → hub)** is simply the arguments of `report`: `summary` is natural language, while `status` and
`open_items` are structured.

**Reporting is not forced.** Blocking the stop through the Stop hook does work — it was verified end to end on both Claude
Code and Codex, and both "already reported this turn" and "waiting for the user" can be determined reliably enough — but the
cost is unacceptable for a product meant to feel calm: every gated turn shows the user an error-styled `Stop hook error`
line that `suppressOutput` does not suppress, and the model sometimes reads the injected demand as a prompt-injection
attempt and refuses outright.

So the `report` tool stays and the role description encourages calling it, but nothing is blocked. When a hub-dispatched
session comes to a stop without having called `report` this turn, the daemon synthesises the report from the `Stop` hook's
`last_assistant_message` and treats the status as `needs_decision`, leaving the judgement to the hub. What is given up is
that the structured fields (`status`, `open_items`) degrade to prose on turns where the session did not call the tool
itself.

Several conditions qualify that synthesis, all found during validation:

- **Grok fires `Stop` twice per session** — once per turn with `reason: "end_turn"` and again at teardown with
  `reason: "shutdown"`. Filter on the reason, or every Grok session ends with a phantom report. `SIGTERM` is what produces
  that graceful teardown, so it is how a Grok session should be stopped.
- **Grok's bash mode (`!`) bypasses hooks and permissions entirely** — no tool events, and a deny rule does not stop it.
  Anything the daemon infers from tool hooks is blind to it.
- **`Stop` and `StopFailure` are mutually exclusive on Claude Code.** A turn that ends in an API error fires only
  `StopFailure`, so a daemon keying "finished" on `Stop` alone leaves that session looking busy forever. Register both.
  `StopFailure` does not mean the session died — in the interactive TUI it stays alive at the prompt.
- **A tool call rejected by Claude Code's own pre-execution guard fires only `PostToolBatch`**, skipping `PreToolUse`,
  `PostToolUse` and `PostToolUseFailure`, so a daemon pairing pre with post misses those calls entirely.
- **A Claude Code `Stop` carrying a non-empty `background_tasks` means paused, not finished.** Synthesising there would
  report an unfinished turn as a result.
- **On Grok some turns produce no stop event at all** (bash mode, builtin slash commands, cancel-and-send, rewinds). The
  backstop is `Notification` with `idle_prompt`, and it was confirmed: a turn cancelled before its first token emitted no
  stop event of any kind, and `idle_prompt` was the session's only turn-end signal. It fires about 60 s after the turn
  ends and carries **no turn id**, so the daemon can only attribute it by session and clock, and it needs at least one
  turn to have ended — a session that only ran a slash command never emits it.

### 5.4 Reporting and automatic archiving

- A project session calls `report` after finishing a round of work (see 5.3 for what happens when it does not).
- **Automatic archiving condition**: `status = done` with an empty `open_items` → once the report has been delivered to the
  hub, the daemon ends the session process and archives it.
- Any other case (unfinished items, failure, a decision needed) → the session stays in the "awaiting instructions" state,
  waiting for the hub to `send_message` and continue, or to be archived explicitly by the hub or the user.
- Reports are written into the hub session as a user message, on the same terms as any other write into a running
  session: delivered when the hub is idle *or* mid-turn, and held only while a modal dialog is up in it or its state is
  unknown.

### 5.5 Waiting for the user (raised hand)

When a project session hits a permission prompt or wants to ask the user something, it **goes to the user directly, not
through the hub** — relaying questions through the hub only creates confusion when several projects ask at once.

- The session shows a raised-hand icon in the menu, bubbled up to its project and console nodes, and fires a system
  notification.
- The user opens the session and answers or grants permission right in the terminal; after the answer the hooks
  automatically return the state to working.
- Through `get_session` the hub can see "waiting for the user"; meanwhile it must not nag or re-dispatch, and messages bound
  for that session queue until the user is done.
- "Needs a hub decision" and "needs a user decision" are kept apart: the former is the agent actively calling
  `report(status: needs_decision)` and belongs to the hub; the latter is a permission prompt or the agent asking a person
  directly, and goes through the raised hand.

**How much of this the hooks can actually see**, established per agent during validation:

- **A pending permission prompt is detectable on all three** — Claude Code's `PermissionRequest`, Grok's `Notification`
  with `permission_prompt`, Codex's `PermissionRequest`. Codex's was observed firing *before* the dialog reached the user; for
  the other two the event was captured but its ordering against the dialog was not established.
- **A question asked through the agent's own ask-the-user tool is detectable on Claude Code and Grok** but **not on
  Codex**, which has no question or notification event at all.
- **A question asked as plain prose is detectable on none of them.** The turn simply ends, and no payload field
  distinguishes it from a finished turn. Those sessions show as idle rather than raising a hand; that is a known gap, not
  something to work around with terminal-text matching.
- **Clearing the raised hand is inferred, not signalled.** No agent has an event for "the user answered"; the daemon
  opens the state on the permission or question event and closes it on the next tool-resolution or stop event for the same
  turn.
- **A user cancellation is observable only on Codex.** Its `Interrupt` fires on Esc or Ctrl-C during an in-flight turn,
  carries the cancelled turn's own `turn_id`, and is mutually exclusive with `Stop`. Claude Code emits **nothing at all**
  when the user interrupts — not in the thinking phase and not mid-tool. No timeout is needed to recover, though: the
  session's **next `UserPromptSubmit`** is a reliable signal that the previous turn is over, and is what should close the
  dangling `PreToolUse`. Only the interrupted turn is silent — the following turn reports normally and tool events pair
  as usual, so the session does not become unreliable. The cost is that until the user types again that session reads as
  working in the menu, which is stale rather than wrong, and the interrupted tool's own child process is already gone —
  Claude Code kills it, it just says nothing. Note also that on Codex a *declined*
  approval aborts the turn and fires `Interrupt` too, so cancel and decline are told apart by whether a
  `PermissionRequest` went unresolved just before it.
- **The user's own agent configuration can remove the prompt entirely.** Codex's `approvals_reviewer` defaults are
  per-user, and with `auto_review` an approval request is resolved by Codex itself: the hook fires, no modal is ever
  shown, and the tool proceeds. Octoboard would raise a hand nobody needs to answer. The adapter should read that setting
  and not promise a raised hand the agent will never surface.

### 5.6 The hub session

- Working directory: `~/.octoboard/consoles/<id>/` on the coordinator's host. Octoboard generates hub-specific instruction
  files there (role, orchestration principles, report format), named for the agent the hub
  uses.
- The hub is only responsible for decomposing, dispatching, following up, and summarizing — it does not modify project code
  itself. That constraint is written into its instructions.
- The hub and each project session may use different agents, e.g. Claude for the hub and Codex for one project.
- The user can also bypass the hub and open a session manually under any project. Manual sessions do not report to the hub by
  default, but "include in hub" can be checked.

### 5.7 Session states

| State | Menu icon | Meaning |
|---|---|---|
| Working | ⟳ loading | The agent is executing. |
| Raised hand | ✋ | Waiting for the user to grant permission or answer. |
| Awaiting instructions | — | Stopped, waiting for the next instruction from the hub or the user. |
| Interrupted | ⏸ | The process ended abnormally (application exit, crash). It stays in the session list; clicking it resumes. |
| Archived | inside the Archive group | The task is wrapped up and the process has ended; it can be reopened. |

## 6. Agent adapters

The daemon defines a single adapter interface, and the rest of Octoboard is unaware of specific agents. The MVP implements
three adapters: Claude Code, Codex, and Grok Build.

| Capability | Claude Code | Codex | Grok Build |
|---|---|---|---|
| Launch with an initial task | `claude "<task>"` | `codex "<task>"` | `grok "<task>"` |
| Pre-allocate a session id | `--session-id <uuid>` | **Not possible.** Take it from the `SessionStart` hook payload; in the interactive TUI the thread is created lazily on the first prompt submission, so a session opened without a task has no id until the user types | `--session-id <uuid>` (new sessions only) |
| Inject hooks | `--settings <json-string-or-path>`, verified to merge with the project settings | `-c 'hooks.<Event>=[{hooks=[{type="command",command=…,timeout=3,async=true}]}]'`, one per event, plus the hook-trust step | A per-session `GROK_HOME` whose `hooks/` directory is Octoboard's and whose other entries symlink to the real `~/.grok` |
| Inject MCP | `--mcp-config <json>` (never with `--strict-mcp-config`) | `-c 'mcp_servers.octoboard.command=…'` + `-c 'mcp_servers.octoboard.args=[…]'` + `-c 'mcp_servers.octoboard.default_tools_approval_mode="auto"'` | An `[mcp_servers.octoboard]` block appended to the `config.toml` copy inside that `GROK_HOME` |
| Inject a role description | `--append-system-prompt` (recorded once per conversation and replayed on resume, so it cannot be changed later) | `-c 'developer_instructions="…"'` — adds a developer message, leaving the rest of the prompt byte-identical. Not `-c instructions=`, which replaces the system prompt | `--rules "…"` — appends to the system prompt and persists into the session record |
| Send a message to a running session | PTY input: `ESC[200~` + text + `ESC[201~` + `CR` (one logical write, but see "Writing into a running session" below — it must be a non-blocking retry loop) | `codex queue --thread <session id>`, or the same PTY input | PTY input, same sequence |
| Resume a session | `claude --resume <id>` | `codex resume <id>` | `grok --resume <id>` |
| Identity seen by the injected MCP server | `CLAUDE_CODE_SESSION_ID` is set automatically | argv or an explicit `env` table — no `CODEX_*` variables reach the child | `GROK_SESSION_ID` is set automatically; `{{session_id}}` templating does *not* work |

- Adding a new agent only requires implementing one adapter; an agent without hook support degrades to an ordinary terminal
  session with no status display and no automatic reporting.
- **Each agent reads a different instruction filename, and matches it by exact spelling.** Measured on a case-sensitive
  volume: Claude Code reads `CLAUDE.md` and `CLAUDE.local.md` and — contrary to what the plan originally assumed — **does
  not read `AGENTS.md` at all**, and cannot be made to: the plugin that would support it is not registered in this build,
  so neither a per-launch setting nor a persistent one turns it on. Where an agent must be shown a project's existing
  `AGENTS.md`, the verified route is `--append-system-prompt-file`, which lands it in the system prompt rather than the
  instruction-file block, leaving discovery and precedence to us. Codex reads `AGENTS.md`; Grok reads all of `AGENTS.md`, `Agents.md`,
  `CLAUDE.md`, `Claude.md` and `CLAUDE.local.md`. **No agent accepts an all-lowercase name.** So the file the hub writes
  into a console's working directory has to be named for the agent that console uses, and on a case-sensitive volume Grok
  will load *every* matching spelling present in a directory rather than just the first.
  **Grok additionally needs a git root**: it locates a project by walking up for a `.git` directory, and in a directory
  without one it reads no project instructions *and* no project hooks at all. A console's working directory is not a
  repository, so **a Grok hub's role description cannot be delivered as an instruction file there** — it has to come from
  `--rules`, which the adapter injects anyway. The alternative, making each console working directory a git root, buys
  nothing else and is not worth it.
- The injection, messaging and resume mechanisms in the table were exercised against Claude Code 2.1.274, Codex 0.160.0
  and Grok Build 1.0.46; every cell has a verification record; the conclusions and the way each was verified are in the milestone 00 validation document. The three
  injection mechanisms turned out to be quite different in shape, and each carries a condition the adapter must satisfy:
  - **Claude Code** injects cleanly through flags. Two flags must *never* be passed: `--setting-sources` (it silently drops
    the project's own permission rules and hooks) and `--strict-mcp-config` (it silently drops the project's and the user's
    MCP servers). The injected MCP server's key must not collide with one the project defines, or the project's definition
    is silently never spawned.
  - **Grok Build** has no flag for hooks or MCP, and its `GROK_CONFIG` / `GROK_CONFIG_PATH` overlay accepts only allowlisted
    keys, so both silently drop. The working mechanism is a per-session `GROK_HOME` pointed at an Octoboard-owned directory
    where every entry is a symlink back to the real `~/.grok` except a copied `config.toml` carrying the MCP block and an
    Octoboard `hooks/` directory. `auth.json` and `sessions/` stay symlinks so login state is shared and sessions stay
    resumable from the user's own `grok`.
  - **Workspace trust gates the *project's* own configuration on two of the three**, which is the exact failure the
    "project configuration must not be overridden" rule exists to prevent — and it fails silently. On Grok, an untrusted
    folder makes the project's `AGENTS.md`, its hooks and its MCP servers simply not load; trust lives inside
    `GROK_HOME`, so `trusted_folders.toml` must be symlinked in or every project looks untrusted, and there is an
    undocumented `--trust` flag. Grok additionally needs a recognised **git** workspace root — project hooks did not load
    in a trusted non-git directory. On Claude Code, an untrusted workspace makes the project's `allow` rules be ignored
    (with an explanatory line on stderr) while its `deny` rules still apply, so the session is only ever more restrictive;
    trust lives in `~/.claude.json`, which Octoboard must not write, so the adapter should detect that stderr line and
    surface it to the user.
  - **Codex** injects through repeated `-c` overrides, with two extras: the project must be marked trusted in the same way
    (`-c 'projects={"<canonical cwd>"={trust_level="trusted"}}'`), and hooks are gated behind a persisted trust hash —
    without it an interactive session raises a blocking review modal and a headless one **hangs indefinitely**. The flag
    `--dangerously-bypass-hook-trust` clears that at the cost of two warning lines per launch; seeding `hooks.state` with
    captured hashes is the warning-free alternative.
- **Resume re-injects everything.** For all three agents the hooks and the MCP server are resolved from the launch
  arguments every time and are lost on a resume that omits them — silently, leaving an unobserved session. The role
  description is the exception and behaves differently per agent: Grok persists `--rules` into the session record, while
  Claude Code records its appended system prompt on the conversation's first request and replays it verbatim, so a
  *changed* role text is ignored on resume. Treat a session's role as immutable for its lifetime.
- **Hook scripts must fail silently and fast on all three.** Every agent surfaces a failing hook to the user, and a hook
  with no timeout blocks the turn for its full duration. Octoboard owns the hook scripts: exit 0 unconditionally, write
  nothing to stderr, set a short explicit timeout (Codex clamps some events to 3 s), mark them asynchronous where the agent
  supports it, and give the daemon call a hard deadline. Note Grok's HTTP hooks cannot reach the daemon at all — its SSRF
  protection rejects both plain HTTP and private addresses — so hooks must be `command` type and talk to the daemon
  themselves.
- Injection must not modify project files, nor the user's global configuration. If an agent can only be configured through a
  config file, prefer an environment variable or flag of the "use this config directory" kind.
- **Writing into a running session** was the most fragile part of the design and is now settled for all three agents. The
  sequence is `ESC[200~`, the text with LF separators, `ESC[201~`, then `CR`; multi-line text arrives as a single message,
  with no delay needed between the paste and the Enter. Four rules come with it, each of which fails silently or
  destructively if ignored:
  - **A message starting with `/` is executed as a slash command**, even inside a bracketed paste — prefix a single space.
  - **A macOS PTY master accepts only about 1022 bytes before `EAGAIN`** when the child is not draining, so the write must
    be a non-blocking partial-write-and-retry loop in slices, never a blocking `write_all` on the daemon's event loop.
  - **Nothing may be written while a modal dialog is up.** The paste itself is discarded, but the trailing `CR` confirms
    whatever option is highlighted — at Claude Code's trust dialog that exits the session, and at Grok's approval modal it
    would select "always-approve". Codex is the exception: a paste at its modal changes nothing. Never send bare keys
    either; Grok and Codex treat digits as confirm hotkeys.
  - **The gate must come from hook-reported state, not from the terminal.** None of the agents signal their modal state
    through terminal modes, and matching rendered footer text needs a VT emulator in the daemon and differs between an
    agent's own renderers. If the hook state is missing or stale, do not write.

### 6.1 Which agent gets used

In descending priority:

1. The agent the hub specified in `start_session` (for that session only).
2. The project's default agent.
3. The console's default agent.

The user can likewise pick an agent when opening a session manually, defaulting to 2 and 3.
Reopening an archived session keeps that session's original agent — session records belong to a specific agent and cannot be
resumed across agents.

## 7. Report panel

- The hub calls `show_page(html)` to push a page and the panel refreshes to the newest one. Every page is archived and the
  history can be paged back and forth.
- Pages run in a sandboxed iframe (separate origin, CSP restricting outbound requests) exposing a single bridge method,
  `octoboard.submit(data)`.
- Form submission: the data passed to `submit` is sent to the hub session as a user message, annotated with its source page.
- History pages are read-only: forms and `submit` are disabled while viewing history.
- **Rewind linkage (after the MVP)**: each page records the conversation position (message id) it was pushed at. After the
  user rewinds inside the agent, the current conversation chain is compared and pages whose anchor is no longer on the chain
  are marked "rewound" and hidden. The MVP keeps the data field but implements no linkage.

## 8. Lifecycle

- **MVP**: the daemon starts with the application. On exit, if any session is still in progress, a confirmation dialog
  appears; once confirmed, all session processes and the daemon are terminated. Terminated sessions are **not archived** —
  they stay where they are in the "interrupted" state, and clicking one next time resumes it through the adapter
  (`--resume`). An application crash behaves the same way.
- **Later: background operation**: the daemon keeps running when the application exits and sessions carry on working. When
  the application opens again it reconnects, and terminals restore their contents from the daemon's output buffer.

## 9. Remote hosts (after the MVP; the architecture already allows for it)

- The user adds a host (SSH configuration) in the application. The application installs / starts `octoboardd` (host role) on
  the remote machine over SSH and sets up a tunnel, which the coordinator then connects through.
- Projects are bound to hosts; `start_session` is routed by the coordinator to the daemon on the right host.
- The hub session runs locally by default. If the user wants the hub remote as well, the coordinator is deployed remotely as
  a whole and the application simply connects to the remote one — the protocol does not change.
- Disconnection: the remote daemon runs independently, so sessions are unaffected; when the tunnel comes back the coordinator
  pulls the status events and reports from the disconnected period. This requires the daemon's events to be replayable by
  sequence number.

## 10. Data model

```
Console   { id, name, workdir, hub_agent, default_agent, created_at }
Host      { id, name, kind: local|ssh, ssh_config? }
Project   { id, console_id, host_id, name, path, default_agent?,
            source: local|parent|github, remote_url? }
Session   { id, agent, agent_session_id, console_id, project_id?, host_id,
            role: hub|worker, origin: hub|user, title,
            status: working|waiting_user|idle|interrupted|archived,
            started_at, ended_at }
Report    { id, session_id, summary, status, open_items, created_at }
Page      { id, console_id, html, anchor_message_id, created_at }
```

- `Session.id` is generated by Octoboard and `agent_session_id` is the agent's own session id. They are kept separate to
  accommodate agents that cannot pre-allocate an id.
- In the MVP `Host` holds a single local record, but every project and session still carries a `host_id` so no data migration
  is needed when going remote.

## 11. MVP scope

**Included:**

1. The split daemon / application architecture (single machine).
2. Three adapters — Claude Code, Codex, Grok Build — plus per-project default agents and per-session hub overrides (6.1).
3. Creating, editing, and deleting consoles; associating projects (local directory, parent directory, GitHub clone).
4. The three-level menu, session states (including raised hand and interrupted), system notifications; opening and typing
   into session terminals; archiving and reopening.
5. The full set of hub MCP tools (5.2); `report` for project sessions; report delivery and synthesis; conditional automatic
   archiving.
6. The report panel: pushing, paging through history, form submission, read-only history.
7. Exit confirmation, with interrupted sessions preserved and resumable.
8. macOS only.

**Not included (later versions):**

- Background operation (section 8).
- Remote hosts (section 9).
- Report panel linkage with conversation rewind.
- Windows / Linux desktop.
- Other agents.

## 12. Milestones

| Stage | Deliverable | Verification |
|---|---|---|
| M0 Technical validation | The daemon runs all three agents over a PTY; hooks deliver status; messages reach a running session; MCP tools get called; the frontend renders a terminal over WebSocket | Every validation item has a conclusion |
| M1 Shell | The console / project / session three-level menu, manual sessions, interruption recovery, archiving and reopening | Project configuration (skills, permissions) takes effect inside sessions |
| M2 Orchestration | Hub MCP tools, report delivery and synthesis, automatic archiving, raised hand | The hub completes one full "dispatch → execute → report → archive → summarize" loop; several projects raise their hand at once; the hub and projects use different agents; the hub overrides the agent for one session |
| M3 Report panel | `show_page`, history, form round-trip | After a form submission the hub continues correctly |
| M4 Polish | Status details, exit flow, packaging and signing | — |
| M5 Final confirmation | The checks that need a real build or an external credential | Each has a recorded outcome; nothing is left outstanding |

## 13. Validation status

Every premise this plan rested on has been settled against Claude Code 2.1.274, Codex 0.160.0 and Grok Build 1.0.46. A
throwaway prototype ran the full chain **on Claude Code** — PTY launch, hooks reporting status, a message written into a
running session, an injected MCP tool being called, and the terminal rendered over WebSocket inside a Tauri window — and
launched all three agents over a PTY. Grok's and Codex's hook and MCP injection was settled by direct probes against those
CLIs rather than through the prototype, which runs them in the degraded no-injection mode.

The conclusions are folded into the sections they affect rather than kept as a separate list — the launch mechanism in 4.4,
the reporting mechanism in 5.3, and the per-agent injection mechanisms, their conditions and their traps in section 6.

Nothing left open can change a design decision. Two things could not be settled during validation, and both are carried
by the plan's final milestone rather than left loose:

- **Grok's `StopFailure`.** The only hook event of the three agents never captured from a live session; its payload shape
  comes from Grok's own documentation. Provoking it means pointing Grok's chat endpoint at a server that returns an
  error, which redirects an authenticated client's traffic and so needs the user's explicit authorisation.
- **The end-to-end terminal latency the user actually perceives.** The daemon-to-WebSocket path measures well under a
  millisecond, but the rendering step on top of it can only be instrumented once the real application exists.

Separately, **Developer ID signing and notarization** were never exercised, for want of a Developer ID — that is milestone
04's own completion criterion rather than a loose end, and it leaves open only whether Gatekeeper admits the bundle on
another machine. Credential access is not at stake: an agent launched from the built bundle reached the user's Keychain
login normally.

One measurement did change a design decision and is recorded here because it constrains the implementation: under heavy
output the daemon's PTY reader must take backpressure from the broadcast channel rather than running ahead of it. A reader
that always runs ahead fills any bounded channel within a fraction of a second and the client gets dropped — that
particular failure is not addressed by framing or batching — and neither is anything else. Sweeping the daemon's own read
buffer, which sets the size of every frame it sends, from 4 KiB to 256 KiB made no measurable difference to throughput at
either of two very different source rates, so output frame size is not a lever worth tuning.
