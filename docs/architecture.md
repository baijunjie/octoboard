# Architecture

Why Octoboard is built the way it is: what it is, how it is layered, the technology choices, the constraints that keep
the design open, and the traps found along the way. What the product does is in `docs/product/`; what the agent CLIs do
is in `docs/agent-cli-reference.md`; how the code is laid out is in `docs/project-map.md`. This document keeps only the
reasoning and the constraints, not a restatement of the code.

## Positioning

Octoboard is a desktop control board for orchestrating agents across several projects. In a **console** the user gives
a request to a console session; the console session decides which project it belongs to, starts a dedicated agent
session in that project's directory and hands it the task; the session reports back to the console session, and the
console session reports to the user. The user can step into any session at any time to watch it or take over.

It is neither a new agent nor a new terminal. Every session is a native agent CLI process (Claude Code, Codex, Grok
Build), and Octoboard only adds organization, orchestration and observation on top. It is not tied to any vendor, and
the retention and cleanup of session data follows each agent's own rules.

## Core concepts

| Concept | Description |
|---|---|
| Console | A management unit for a group of projects, independent of other consoles. It has its own working directory and its own console session agent. |
| Host | A machine that runs sessions. Only the local machine exists today; every project and session still carries a `host_id`. |
| Project | A directory on some host, associated with a console. |
| Session | An agent CLI process running in a project directory, created by the console session or by the user. |
| Console session | The agent process running in the console's working directory, given the orchestration tools. |
| Agent adapter | The layer that plugs an agent CLI in: how to launch it, inject capabilities, report status and resume it. |

The daemon defines a single adapter interface and the rest of Octoboard does not know which agent it is driving. An
agent without hook support would degrade to an ordinary terminal session with no status and no automatic reporting.

## Layers

```
┌───────────────── Desktop application (Tauri) ─────────────────┐
│ Menu tree · terminal rendering · report panel · notifications │
└───────────────────────────┬───────────────────────────────────┘
                            │ WebSocket
                 ┌──────────▼──────────┐
                 │ Local daemon        │
                 │ coordinator + host  │
                 └──────────┬──────────┘
                       agent processes
```

`octoboardd` is a headless, long-running Rust process with two roles:

- **Host role**, present on every machine that runs sessions: manages PTYs and agent processes, receives hook reports,
  serves MCP for local sessions, lists directories and clones repositories.
- **Coordinator role**, on the user's own machine: stores console, project, session and report-page data, executes the
  console session's orchestration requests and routes them to the right host, and handles report delivery and
  synthesis.

The desktop application is only a client of the daemon: it renders terminals, shows status and forwards input.

The UI is not part of the desktop shell. It is its own package, built once, which the macOS shell bundles and loads and
which also runs in a plain browser; that is what lets a machine without the shell, such as a Linux host, offer the same
UI. To keep one build working in both places, the UI reaches native capabilities (the quit flow and exit heartbeat,
system notifications, the Dock badge, the native window's own appearance, the window chrome the top bar must keep clear
and the app menu's items) only through a platform adapter chosen at startup, with a Tauri implementation and a browser
one; in a browser a capability that is not there means the feature is absent, not an error. It finds the daemon without
Tauri as well: by an address handed to it, or at the origin it was loaded from. Its state is one store fed by the daemon
client's events, readable outside React too. The phone's browser is not a design target — the native mobile apps have
their own UI.

## Why the daemon is split out

Session processes are owned by the daemon rather than by the window. That is what would let the daemon outlive the
application (background operation; today it exits with the application) and lets a remote host run the host role of the
very same daemon, with terminal streams, status events and orchestration commands all on one protocol so the UI need not
tell local from remote. Doing the split up front avoids a later rewrite.

**Constraint: the UI and the daemon interact only over the network protocol (`apps/daemon/PROTOCOL.md`) — no Tauri
IPC, no shared state.** Otherwise going remote or headless breaks. Hooks and the MCP server of a session point at the
daemon on the host the session runs on, so a remote session never has to connect back to the user's machine.

Remote hosts are **not built yet**; the design keeps them possible as follows: The application would install and
start the daemon (host role only) on the remote machine over SSH and tunnel to it, and the coordinator would route
`start_session` to the host role of the daemon on the right host. A remote console session would mean deploying the
whole coordinator remotely and pointing the application at it, with no protocol change. After a disconnect the remote daemon
keeps running on its own, and the coordinator would pull the status events and reports of the gap once the tunnel is
back, which requires the daemon's events to be replayable by sequence number.

## Technology choices

| Layer | Choice | Why |
|---|---|---|
| Daemon | Rust + `tokio` | A single binary that can be deployed straight to a Linux host. |
| PTY | `portable-pty` | From the WezTerm project, cross-platform. |
| MCP server | `rmcp` | A stdio child process per session, bridging to the daemon over loopback HTTP; see below. |
| Desktop application | Tauri 2 | The daemon ships as a sidecar; native macOS APIs are callable from the Rust side. |
| Frontend | React + HeroUI + Tailwind CSS + TypeScript, state in a Zustand store | One component library for the whole UI; a vanilla Zustand store can be read and subscribed to outside React. |
| Terminal | `xterm.js` | macOS cannot embed a Terminal.app or iTerm window in another application; PTY + `xterm.js` is the standard approach and terminal compatibility is its job. |
| Report panel | sandboxed iframe (`srcdoc`) + `postMessage` | Independent of the desktop framework. |
| Storage | SQLite (`rusqlite`, coordinator side) | |

## Orchestration design decisions

- **Capabilities are injected per launch, never by editing the project's or the user's files.** That means arguments,
  an environment variable, or files Octoboard writes itself (for Grok a per-session `GROK_HOME` holding Octoboard's
  `config.toml` and `trusted_folders.toml` copies and `hooks/`). Every session starts with the project directory as its
  cwd, so the project's own instruction file, skills, hooks and permissions apply as they normally would. The mechanism
  differs per agent ("Injecting Octoboard into each agent" in `docs/agent-cli-reference.md`).
- **The MCP server is a child process, not an endpoint the agent dials.** The daemon binary doubles as it
  (`octoboardd mcp`), speaking MCP on stdin/stdout and forwarding each call to the daemon over loopback HTTP. A
  Streamable HTTP endpoint in the daemon was ruled out by two of the three agents: Codex's verified MCP injection takes
  a `command` and `args`, and Grok's SSRF protection rejects plain HTTP and private addresses alike (also why Grok's
  hooks cannot be HTTP). One mechanism for all three beats a bridge for some. The session's identity travels in argv
  because Codex exports nothing to an MCP child and Grok's `{{session_id}}` templating does not work; a token issued per
  launch and dropped with the process means a local process that guesses the port cannot reach the tools, and the
  daemon resolves the calling session from the token rather than from an argument.
- **Structure in tool arguments, natural language in content.** Agents are not asked to emit JSON in reply text, where
  it mixes with prose and code fences; tool arguments are validated against a schema by the agent runtime. Fields a
  program acts on (status, open items) are arguments; what an agent must understand (the brief, a summary) stays prose.
- **Reporting is not forced.** Blocking the stop through the `Stop` hook works (verified on Claude Code and Codex), but
  every gated turn shows an error-styled `Stop hook error` line that `suppressOutput` does not suppress, and the model
  sometimes reads the injected demand as prompt injection and refuses — too costly for a product meant to feel calm. So
  the role description encourages `report`, nothing is blocked, and the daemon synthesizes a report for a session that
  reports to the console session and stops without one. The price is that `status` and `open_items` degrade to prose
  on those turns.
- **A project session goes to the user directly for permissions and questions, not through the console session.**
  Relaying through the console session only creates confusion when several projects ask at once. "Needs a console
  session decision" (the agent calls `report(status: needs_decision)`) and "needs a user decision" (a permission
  prompt or a question to a person, shown as the raised hand) are kept apart.
- **Detecting the raised hand is limited by what the hooks expose**, not by Octoboard: a question asked as plain prose
  is indistinguishable from a finished turn on all three agents, and that is a known gap rather than something to work
  around with terminal-text matching. State is read from hooks, never from rendered output, because that would need a
  VT emulator in the daemon and differs between an agent's own renderers. The one exception is Claude Code's
  workspace-trust screen, which no hook precedes and so can only be recognized in the output.

## Known pitfalls of the Tauri / Rust approach

Each has an established solution unless noted; two are not yet confirmed by hand, as said where they appear.

- **PATH and environment.** A macOS application launched from Finder does not inherit the shell's `PATH`, so `claude`
  and `codex` (under `~/.local/bin` or a node-version manager's shims) cannot be found and API keys are missing. A login
  shell is not enough: on zsh it is `~/.zshrc` that sets those up and a login-only non-interactive shell never reads it.
  The daemon therefore snapshots the environment from a **login + interactive** shell, per launch (a version manager's
  `PATH` entry can point at a per-shell-instance directory), and spawns the agent directly rather than inside a shell.
- **The snapshot must be filtered, not only taken.** The shell passes the daemon's own environment through, so a daemon
  started from inside an agent session would hand that session's identity to every agent it launches. On Claude Code
  that silently turns transcript saving off and makes `--permission-mode` not apply. Stripping the markers does not
  disturb authentication, which lives in the macOS Keychain. The markers are enumerated, never matched by prefix, and
  stripped regardless of which agent is being launched. Build the list by dumping `env` inside a live session of each
  agent and stripping what is there and only there: `GROK_CODE_XAI_API_KEY`, `GROK_HOME`, `CODEX_HOME` and
  `CLAUDE_CODE_USE_BEDROCK` are user settings that a prefix match would drop.
- **Terminal data does not go through Tauri IPC.** High-frequency output would pay serialization overhead, and it would
  break the constraint above. The frontend connects to the daemon over WebSocket directly.
- **Sidecar signing.** The bundled daemon must be signed and notarized with the application or Gatekeeper blocks it.
  Credential access is not a concern: an agent launched from the bundle reaches the user's Keychain login normally,
  because a Keychain ACL is evaluated against the agent binary's own signature rather than its parent's.
- **Rust toolchain floor.** The dependency graph, not Tauri, sets it, and a crate's declared `rust-version` is advisory
  and often wrong in both directions: find a usable version by compiling candidates downward. `reqwest` cannot be used
  below Rust 1.88 (its `idna`/ICU chain), so loopback traffic uses a small hand-rolled HTTP client; Tauri 2 builds on
  older toolchains only with exact pins on a chain of transitive crates. Carry the pins deliberately or raise the
  toolchain; drifting into it by accident costs a day.
- **Full-width punctuation from a CJK input method needs two key presses.** A mark such as `？`, which an input method
  emits without a candidate window, is swallowed on the first press by `xterm.js` 5.5.0 in WKWebView. The library arms
  a "key down seen" flag on every keydown, a bare `Shift` included, and drops the commit while it is set, but WebKit
  delivers the commit before the mark's own keydown. No released version fixes it, so the terminal's key handler
  restores the flag after a modifier-only keydown (`TerminalController`), reaching into a private field that must be
  rechecked on every `xterm.js` upgrade. Composed CJK text is unaffected.
  Not yet confirmed by hand with a real input method.
- **Mouse reports past column 95.** The encoded byte exceeds 127, so `onData` never sees it; only `onBinary` does, as a
  string of raw code units, hence a mask back to bytes rather than a UTF-8 encode. Not yet confirmed by hand: the
  forwarding was read line by line but never exercised with a mouse-aware TUI, and none is reachable from Grok's bash
  mode, which has no controlling TTY.
- **A packaged application needs file-access permission per volume.** The bundled `.app` raises a macOS prompt the
  moment a project lives on an external volume, which `tauri dev` never does, so associating a project has to cope with
  the user declining or not having answered yet.
- **`Ctrl+C` does not reach the terminal on its own.** WKWebView swallows it above `xterm.js` while other modifier
  combinations pass, so the UI intercepts it and forwards `0x03` itself.
- **macOS Quit does not raise Tauri's `ExitRequested`.** Cmd+Q, the application menu's Quit and the Dock's Quit all send
  `terminate:`, and nothing in the Tauri/`tao` stack implements `applicationShouldTerminate:`, so a confirmation hung on
  `ExitRequested` is skipped. The application owns its Quit menu item, and the Dock's Quit can only be caught by
  overriding the application delegate. That selector is also how a logout, restart or shutdown arrives, with nothing to
  tell them apart, so holding a confirmation up in front of a logout is the unavoidable price of asking before the
  Dock's Quit.
- **WebView differences.** Tauri uses the system WebView (WKWebView on macOS). A Linux client would need WebKitGTK's
  support for `xterm.js` WebGL rendering verified, falling back to canvas rendering.
- **End-to-end terminal latency was never measured.** Only the daemon-to-WebSocket path was (well under a
  millisecond); the time from key press to glyph on screen was not.
- **PTY output backpressure.** Under heavy output the daemon's PTY reader must take backpressure from the broadcast
  channel rather than running ahead of it: a reader that always runs ahead fills any bounded channel within a fraction
  of a second and the client gets dropped, and neither framing nor batching addresses that. The size of the daemon's own
  read buffer (4 KiB to 256 KiB) made no measurable difference to throughput, so output frame size is not worth tuning.

## Data model

Entities: console, host, project, session, report, report-panel page, and the trusted folders. The shapes are in
`apps/daemon/src/store.rs`; what is worth knowing is the reasoning behind a few fields.

- `Session.id` is Octoboard's own and `agent_session_id` the agent's. They are separate because some agents cannot
  pre-allocate an id.
- `Session.bound_to` is the id of the console session a session reports to, or unset. It is always set to the
  starting console session for a console-session-started session, and fixed for the session's lifetime; a console
  session itself is never bound. Reports are routed by this field rather than by a lookup for "the" console session
  of a console, because a console may hold several at once (see
  `docs/plans/20261008-console-sessions-and-agent-accounts/02-binding-data-model.md`).
- `Session.colour` and `Session.ordinal` are set only for a console session: a badge colour from a fixed palette,
  assigned on creation and never reused while still in use among the console's other console sessions, and a
  per-console ordinal that is one past the highest ever handed out there — kept on the console record itself
  (`consoles.next_console_session_ordinal`) so deleting the console session that held the highest ordinal does not
  let a later one reuse it.
- `Project.path` is stored absolute and lexically normalized (a relative path is refused), because trusted-folder
  entries are compared with it component by component.
- A console holds an optional config directory per agent, and `Session.config_dir` is the one of the session's own
  agent. The console's value is copied onto a session when it is opened and never changes, because the agent keeps its
  transcripts there and a resume must find them.
- Every project and session carries a `host_id` while the host table holds a single local record, so going remote needs
  no data migration. Trusted-folder entries ignore `host_id`, which is harmless while every host is local.
- `Page.anchor_message_id` records the conversation position a page was pushed at. It is stored and never read; a
  linkage that hid pages after the user rewinds the agent's conversation would read it, but no agent exposes a message
  id to put in it yet.
