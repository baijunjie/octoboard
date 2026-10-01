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
  hub's orchestration requests and routes them to the right host, and handles report queueing and delivery.

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

All of these have established solutions and none of them is a blocker:

- **PATH and environment variables**: a macOS application launched from Finder does not inherit the user's shell PATH, so
  `claude` / `codex` (commonly installed under `~/.local/bin` or an nvm directory) cannot be found, and API keys and other
  environment variables are missing too. The daemon therefore always launches agents through the user's login shell
  (`$SHELL -l -c …`).
- **Terminal data does not go through Tauri IPC**: high-frequency terminal output would pay serialization overhead over IPC
  and would violate the constraint in 4.2. The frontend connects to the daemon over WebSocket directly.
- **Sidecar signing**: when the daemon binary is bundled with the application it must be signed and notarized along with it,
  or Gatekeeper will block it.
- **WebView differences**: Tauri uses the system WebView (WKWebView on macOS). This has no impact on a macOS-only MVP; when
  Linux support is added later, WebKitGTK's support for xterm.js WebGL rendering needs to be verified, falling back to canvas
  rendering if necessary.

## 5. Orchestration

Key premise: every session starts with the project directory as its cwd, so the project's own agent configuration
(`CLAUDE.md` / `AGENTS.md`, skills, hooks, permission settings, and so on) takes effect exactly as it normally would.
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
| `send_message(session, text)` | Append an instruction to a session (delivered immediately when idle, queued when busy or waiting for the user). |
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

**Forced reporting**: when a hub-dispatched session comes to a stop without having called `report` this turn and without
being in the waiting-for-user state, the daemon blocks the stop through the Stop hook and prompts the session to call
`report` (at most once per turn, to prevent loops). If it still does not call it, the daemon falls back to the session's last
reply as the report and treats the status as `needs_decision`, leaving the judgement to the hub. Agents that cannot block a
stop go straight to the fallback.

### 5.4 Reporting and automatic archiving

- A project session calls `report` after finishing a round of work (see 5.3 for what happens when it does not).
- **Automatic archiving condition**: `status = done` with an empty `open_items` → once the report has been delivered to the
  hub, the daemon ends the session process and archives it.
- Any other case (unfinished items, failure, a decision needed) → the session stays in the "awaiting instructions" state,
  waiting for the hub to `send_message` and continue, or to be archived explicitly by the hub or the user.
- Reports are written into the hub session as a user message. If the hub is busy they queue and are delivered once it stops.

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

### 5.6 The hub session

- Working directory: `~/.octoboard/consoles/<id>/` on the coordinator's host. Octoboard generates hub-specific instruction
  files there (role, orchestration principles, report format), as `CLAUDE.md` or `AGENTS.md` depending on the agent the hub
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
| Pre-allocate a session id | `--session-id <uuid>` | To be verified; if unsupported, take it from the startup hook payload | `--session-id <uuid>` |
| Inject hooks | `--settings <json>`, merged with the project settings | Hooks are supported (`features.hooks` is already stable), injected via `-c`; the exact events and format are to be verified | Hooks are supported, but no command-line injection flag was found; the injection mechanism is to be verified |
| Inject MCP | `--mcp-config <json>` | `-c mcp_servers.octoboard.…` | MCP is supported; the injection mechanism is to be verified |
| Inject a role description | `--append-system-prompt` | To be verified (candidate: overriding the instruction-related config via `-c`) | Only `--system-prompt-override` exists (a full replacement, so unusable); an append mechanism is to be verified, with the fallback being to write it into the initial task |
| Send a message to a running session | PTY input (bracketed paste + Enter) | `codex queue --thread <id> --message <text>`; fall back to PTY input if it does not meet the need | PTY input |
| Resume a session | `claude --resume <id>` | `codex resume <id>` | `grok --resume <id>` |

- Adding a new agent only requires implementing one adapter; an agent without hook support degrades to an ordinary terminal
  session with no status display and no automatic reporting.
- The "to be verified" entries above are tracked in the investigation checklist in section 13. Every command-line flag listed
  has been confirmed to exist in the local `--help` output (Claude Code, Codex 0.159.2, Grok Build 1.0.46), but none has been
  exercised end to end yet.
- Injection must not modify project files, nor the user's global configuration. If an agent can only be configured through a
  config file, prefer an environment variable or flag of the "use this config directory" kind.
- Writing to the PTY of Claude Code / Grok is the most fragile part of the MVP; multi-line text and the behaviour while a
  session sits at a permission prompt both need hands-on testing.

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
5. The full set of hub MCP tools (5.2); `report` for project sessions; queued report delivery; conditional automatic
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
| M0 Technical validation | The daemon runs all three agents over a PTY; hooks deliver status; messages reach a running session; MCP tools get called; the frontend renders a terminal over WebSocket | Every item in the section 13 checklist has a conclusion |
| M1 Shell | The console / project / session three-level menu, manual sessions, interruption recovery, archiving and reopening | Project configuration (skills, permissions) takes effect inside sessions |
| M2 Orchestration | Hub MCP tools, queued reporting, automatic archiving, raised hand | The hub completes one full "dispatch → execute → report → archive → summarize" loop; several projects raise their hand at once; the hub and projects use different agents; the hub overrides the agent for one session |
| M3 Report panel | `show_page`, history, form round-trip | After a form submission the hub continues correctly |
| M4 Polish | Status details, exit flow, packaging and signing | — |

## 13. M0 investigation checklist

The goal of M0 is to settle, one by one, the premises the plan depends on that have not been tested yet — before any product
code is written. Each item needs a conclusion (feasible / not feasible / feasible with conditions) and the way it was
verified. For items that turn out not to be feasible, the plan is adjusted along the listed fallback and written back into
the corresponding section.

### 13.1 Common to all agents

| # | Question to settle | Impact | Fallback if not feasible |
|---|---|---|---|
| G1 | When started with the project directory as cwd, do the project's own configuration (instruction files, skills, hooks, permissions) and the injected content both take effect without overriding each other | Premise of section 5 | Change the injection mechanism; project configuration must not be overridden |
| G2 | When launching through the login shell, are PATH, API keys, and each agent's login state fully available (including when the application is launched from Finder) | 4.4 | Read the user's shell environment and pass it in explicitly |
| G3 | Can status events cover: work started, stopped, permission requested, question asked of the user (e.g. `AskUserQuestion`), resumed after the user answered | 5.5, 5.7 | Missing states degrade to not being displayed, or are inferred from terminal output patterns |
| G4 | When a hook fails or the daemon is unreachable, does the agent hang or raise errors that disturb the user | Stability | Hook scripts must fail fast and exit silently |
| G5 | Can the injected MCP server distinguish identities per session (token passed via URL / header), and do the hub and workers see only their own tools | 5.2 | Use different ports or paths per role |
| G6 | When resuming an interrupted session (`--resume`), do the injected arguments need to be passed again, and do they take effect | Section 8 | The adapter reassembles all injected arguments on resume |
| G7 | Is using the Stop hook to block a stop and demand a `report` call feasible; how can "already reported this turn" and "currently waiting for the user" be determined | 5.3 | Do not block; go straight to the fallback report |

### 13.2 Sending messages to a running session

| # | Question to settle | Impact | Fallback if not feasible |
|---|---|---|---|
| M1 | Claude Code / Grok: is multi-line text written over the PTY as bracketed paste + Enter submitted as one complete message | 5.4 report delivery, `send_message` | Switch to single-line escaped text, or look for an official message-injection interface |
| M2 | What happens when writing while the session sits at a permission prompt, a multiple-choice question, or mid-execution (being misread as a keypress selection is the biggest risk) | Same as above | The daemon only writes in the "awaiting instructions" state and queues in every other state |
| M3 | Codex `codex queue`: when is the message consumed (at the end of the current turn?), does it work while the session runs in the TUI, and does it conflict with PTY input | Section 6 | Fall back to PTY input |

### 13.3 Per-agent specifics

| # | Agent | Question to settle | Fallback if not feasible |
|---|---|---|---|
| A1 | Claude Code | Are hooks injected via `--settings` merged with, or do they override, the hooks in the project's `.claude/settings*.json` | They must merge; otherwise use a different injection mechanism |
| A2 | Codex | Can a session id be pre-allocated; if not, where can the session id be obtained reliably after launch (hook payload / session file) | Match the session directory by cwd and start time |
| A3 | Codex | Which hook events are supported and in what payload format, and can they be injected via `-c` without modifying `~/.codex/config.toml` | Use an environment variable to point at a separate config directory |
| A4 | Codex | How to append a role description without replacing the default system prompt | Write it into the initial task prompt |
| A5 | Codex | Does an MCP server injected via `-c mcp_servers.…` take effect in the interactive TUI | Same as A3 |
| A6 | Grok Build | How to inject hooks and MCP (no relevant command-line flags found), and whether it is possible without modifying project or user-global configuration | A separate config directory; failing that, Grok degrades to an ordinary terminal session |
| A7 | Grok Build | How to append a role description (`--system-prompt-override` replaces everything, so it is unusable) | Write it into the initial task prompt |

### 13.4 Terminal and architecture

| # | Question to settle | Impact | Fallback if not feasible |
|---|---|---|---|
| T1 | Do all three agents' TUIs render and behave correctly in `xterm.js` inside WKWebView: full-screen mode, mouse, keyboard shortcuts, CJK input methods, window resizing | Section 3 | Targeted configuration (e.g. Grok's non-full-screen mode); adjust xterm.js options if necessary |
| T2 | The latency of daemon → WebSocket → `xterm.js` and the throughput under heavy output | 4.2 | Batch frames together, use binary frames |
| T3 | After the frontend disconnects and reconnects, can the terminal contents be restored from the daemon's cached output | Prerequisite for background operation in section 8 | Record raw PTY output in a ring buffer and replay it |
| T4 | Packaging, signing, and the lifecycle of the daemon as a Tauri sidecar (reliably terminated when the application exits, no orphan processes after a crash) | 4.4, section 8 | The daemon watches its parent process and exits once the parent is gone |
