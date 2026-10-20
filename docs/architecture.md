# Architecture

Why Octoboard is built the way it is: what it is, how it is layered, the technology choices, the constraints that keep
the design open, and the traps found along the way. What the product does is in `docs/product/`; what the agent CLIs do
is in `docs/agent-cli-reference.md`; how the code is laid out is in `docs/project-map.md`. This document keeps only the
reasoning and the constraints, not a restatement of the code.

## Positioning

Octoboard is a control board for orchestrating agents across several projects. In a **console** the user gives a
request to a console session; the console session decides which project it belongs to, starts a dedicated agent
session in that project's directory and hands it the task; the session reports back to the console session, and the
console session reports to the user. The user can step into any session at any time to watch it or take over.

It is neither a new agent nor a new terminal. Every session is a native agent CLI process (Claude Code, Codex, Grok
Build), and Octoboard only adds organization, orchestration and observation on top. It is not tied to any vendor, and
the retention and cleanup of session data follows each agent's own rules.

### Two deployment forms

Octoboard is deployed in one of two forms, and the other documents name them as here:

- **The desktop form**: the macOS application. The window, the daemon and the projects are on one machine, used by
  one person, and the daemon starts and stops with the application.
- **The server form**: the daemon alone, on a machine without a display — a Linux server, Ubuntu say — started and
  kept running there, listening on a port, and reached by its owner from a browser on another machine over the
  network. The machine showing the window and the machine holding the daemon and the projects are two different ones.
  This is what Linux is for: there is no Linux desktop application, and in this form the browser is not a development
  convenience but the only client there is.

Both forms are **one user's**. The server form is the same user reaching their own daemon from elsewhere, not a group
sharing one daemon: the daemon does not tell clients apart, whoever holds a credential for it is taken for its owner,
and a credential lets its holder do everything the owner can — so a credential is to be kept like a password. That is
this version's design boundary, not a gap left for later; what several users would take is in "Why the daemon is
split out".

The server form is **not built yet**. Today the daemon binds to loopback on a port the operating system assigns, has
no credential of any kind, does not serve the UI, and the Linux build is not established. What the form asks of the
design is stated below, where each constraint comes up.

"Server" names where the daemon lives: a machine kept running for others to reach over the network, with nobody
sitting at it. The word *host* is not free for it. A host is any machine that runs sessions — the server is one, and
so is the desktop — and a *remote host* is a different arrangement again: a second host, reached by the daemon the
user is talking to, so that sessions run on a machine other than the daemon's (see "Why the daemon is split out"). The
server form is the reverse: one host, and what is remote is the clients.

## Core concepts

| Concept | Description |
|---|---|
| Console | A management unit for a group of projects, independent of other consoles. It has its own working directory and its own default agent, which its console sessions run and the sessions under its projects fall back to. |
| Host | A machine that runs sessions: the user's own machine in the desktop form, the server in the server form. Only the daemon's own machine exists today; every project and session still carries a `host_id`. |
| Client | A window or browser tab the user operates the daemon through: the desktop application's window, or a browser on any machine that can reach the daemon. Any number of them may be connected at once — the desktop window and a browser tab on a laptop, say — and the daemon does not tell them apart: every client is the same user, sees the same consoles, projects and sessions, and may do everything. |
| Project | A directory on some host, associated with a console. |
| Session | An agent CLI process running in a project directory, created by the user, a console session or an unbound project session. |
| Console session | The agent process running in the console's working directory, given the orchestration tools. |
| Agent adapter | The layer that plugs an agent CLI in: how to launch it, inject capabilities, report status and resume it. |

The daemon defines a single adapter interface and the rest of Octoboard does not know which agent it is driving. An
agent without hook support would degrade to an ordinary terminal session with no status and no automatic reporting.

## Layers

```
   The desktop form                            The server form
┌──── Desktop application (Tauri) ────┐   ┌──── Browser on another machine ──┐
│ menu tree · terminal · report panel │   │ the same UI, for the same user   │
└──────────────────┬──────────────────┘   └────────────────┬─────────────────┘
          on the same machine                       over the network
                   └───────────────┬───────────────────────┘
                                   │ WebSocket
                        ┌──────────▼──────────┐
                        │ Daemon              │
                        │ coordinator + host  │
                        └──────────┬──────────┘
                              agent processes
```

`octoboardd` is a headless, long-running Rust process with two roles:

- **Host role**, present on every machine that runs sessions: manages PTYs and agent processes, receives hook reports,
  serves MCP for local sessions, lists directories, clones repositories, and probes remotes and directories for the
  agent a project is set up for.
- **Coordinator role**, on the machine the clients connect to — the user's own in the desktop form, the server in the
  server form: stores console, project, session and report-page data, executes the console session's orchestration
  requests and routes them to the right host, and handles report delivery and synthesis.

A client — the desktop application's window, or a browser — is only that: it renders terminals, shows status and
forwards input.

The UI is not part of the desktop shell. It is its own package, built once, which the macOS shell bundles and loads and
which also runs in a plain browser; that is what the server form rests on — a Linux server has no shell and offers the
same UI to a browser. To keep one build working in both places, the UI reaches native capabilities (the quit flow and
exit heartbeat, system notifications, the Dock badge, the native window's own appearance and visibility, the window
chrome the top bar must keep clear, the app menu's items and the menu bar icon's menu) only through a platform adapter
chosen at startup, with a Tauri implementation and a browser one; in a browser a capability that is not there means
the feature is absent, not an error (copying instead falls back to the browser's selection copy). It finds the daemon
without Tauri as well: by an address handed to it, or at the origin it was loaded from. Its state is one store fed by
the daemon client's events, readable outside React too. The phone's browser is not a design target — the native mobile
apps have their own UI.

**A browser is a client Octoboard does not control, and it grants two of the capabilities the UI is built on only to a
secure context**: the Notifications API, which the raised hand's notification goes through, and the async Clipboard
API. A page over plain `http` is a secure context only at a loopback address (`localhost`, `127.0.0.1`); at any other
address it is not, so a browser reaching a server over bare `http` never tells anyone that an agent is waiting — and
the whole point of the orchestration is that an agent raises its hand and a person answers. The UI also opens `wss:`
only from an `https:` page, and an `https:` page may not open `ws:` at all. In the server form, then, TLS is a
requirement of the browser before it is anything else, and the UI has to be served from the same `https` origin as the
daemon's WebSocket endpoints.

## Why the daemon is split out

Session processes are owned by the daemon rather than by the window. That is what lets the daemon run with no window
at all — the server form, where there is no application to start or stop it — and, in the desktop form, what would
let it outlive the application (background operation; today it exits with the application there). **Remote hosts** —
a daemon reaching a second host to run sessions there — are **not built yet**, and no workflow for them is settled.
They are not the server form: the server is a single host, and what is remote in that form is the clients.

**Constraint: the UI and the daemon interact only over the network protocol (`apps/daemon/PROTOCOL.md`) — no Tauri
IPC, no shared state.** Hooks and the MCP server of a session point at the daemon on the host the session runs on. The
server form is what makes this a hard constraint rather than a tidy one: a browser on another machine has nothing but
the network.

**Constraint: in the server form, access has to be a credential, not reachability.** Today it is reachability: the
daemon trusts whatever connects and is safe only because it binds to loopback — the access guard (`access.rs`) keeps a
web page on another site out, and does nothing more. That holds in the desktop form, where whoever can connect already
has the machine. In the server form anyone who can reach the port can start agents and type into their terminals, or
open a shell terminal and run any command, as the server's own user, so the daemon as it stands must not be exposed
beyond loopback. What the form needs is authentication on every route, the UI and the WebSocket upgrades included; a
credential per device, so that a device the owner loses or retires can be revoked without re-issuing the rest; and
TLS, for the browser's own reasons above.
None of it exists. A credential identifies a device of the owner's, never a person: the daemon stays one user's, and
every client is that user.

**Known boundary: several users on one daemon are outside this design**, and authentication per person would be the
smallest part of it: every session runs as the daemon's own Unix user, with that user's agent logins, quotas and SSH
keys, and a project is one checkout that two agents would overwrite each other in, so each user would need a working
tree and an operating-system identity of their own; a shared terminal would need an owner of its input; and trust
confirmations, settings and sessions would each need an owner and an authorization. None of it is planned.

## Technology choices

| Layer | Choice | Why |
|---|---|---|
| Daemon | Rust + `tokio` | A single binary that can be deployed straight to a Linux server, which is the whole server form. |
| PTY | `portable-pty` | From the WezTerm project, cross-platform. |
| MCP server | `rmcp` | A stdio child process per session, bridging to the daemon over loopback HTTP; see below. |
| Desktop application | Tauri 2 | The daemon ships as a sidecar; native macOS APIs are callable from the Rust side. |
| Frontend | React + HeroUI + Tailwind CSS + TypeScript, state in a Zustand store | One component library for the whole UI; a vanilla Zustand store can be read and subscribed to outside React. |
| Terminal | `xterm.js` | macOS cannot embed a Terminal.app or iTerm window in another application; PTY + `xterm.js` is the standard approach and terminal compatibility is its job. |
| Report panel | sandboxed iframe (`srcdoc`) + `postMessage` | Independent of the desktop framework. |
| File viewer | `@pierre/diffs` (Shiki on its JavaScript regex engine) | One library for files and diffs; loads offline in the packaged WebView under the app's CSP, no WebAssembly. Behind an application-owned interface. |
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
- **Information between sessions is a channel of its own, because "not an instruction" cannot be enforced.** A message
  is text written into the receiving terminal, so any session that can write to another could steer it. Commanding is
  therefore the only hard-enforced boundary (`send_message`, `archive_session` and `reopen_session` act on the caller's
  own sessions). Sharing information is a separate tool, offered to every session and reaching any running session in
  any console or project (see `docs/product/information-between-sessions.md`); it is delivered under a header naming
  the sender and saying the text is information to weigh, role descriptions tell each session whose work it takes, and
  the receiver's owner is copied so it is never blind to what reaches its sessions. Octoboard makes the distinction
  plain; whether a model keeps to it is the model's call.
- **A session whose work needs somewhere it does not reach asks the user for a session rather than being given more
  reach.** Two alternatives were set aside. Letting a project session drive sessions in other projects directly breaks
  the assumption that an owner and its bound sessions share a project — removing a project would leave dangling
  bindings, and a project's bulk archive delete would reach sessions elsewhere — and hands a session the user opened by
  hand the console's whole reach without asking. Promoting the session itself into a console session cannot work
  either, because a session's role description cannot change on resume: Claude Code replays its appended system prompt
  verbatim and Grok Build stores its `--rules` in the session record, so the session would get a console session's
  tools while still being told it is a project session. What it does instead is bounded. `request_session`, offered to
  every session, asks the user to start a session with no owner — in any project, or a console session — and only the
  user's approval starts it. `new_session` starts a session the caller owns, with no approval, where the caller
  already has reach. A binding changes after launch only by a call of the session concerned: an owner hands a session
  of its own to another owner with `transfer_session`, and an unbound project session binds itself to a console
  session or a project session of its own project with `bind_to_session` (how a session that asked for a console
  session comes to be driven by it). Because the caller's own tool call is what causes the change, the outcome can be
  told to it in the call's result or a message and its fixed role description is left alone. The behaviour of a request
  is in `docs/product/session-requests.md`, and the rules a binding keeps in "Who can own a session" in
  `docs/product/changing-session-owners.md`.
- **A project session goes to the user directly for permissions and questions, not through the console session.**
  Relaying through the console session only creates confusion when several projects ask at once. "Needs a console
  session decision" (the agent calls `report(status: needs_decision)`) and "needs a user decision" (a permission
  prompt or a question to a person, shown as the raised hand) are kept apart.
- **Detecting the raised hand is limited by what the hooks expose**, not by Octoboard: a question asked as plain prose
  is indistinguishable from a finished turn on all three agents, and that is a known gap rather than something to work
  around with terminal-text matching. State is read from hooks, never from rendered output, because that would need a
  VT emulator in the daemon and differs between an agent's own renderers. The one exception is each agent's own
  folder-trust confirmation, which no hook precedes and so can only be recognized in the output.

## Known pitfalls of the Tauri / Rust approach

Each has an established solution unless noted; any outstanding manual verification is stated in its entry.

- **PATH and environment.** A macOS application launched from Finder does not inherit the shell's `PATH`, so `claude`
  and `codex` (under `~/.local/bin` or a node-version manager's shims) cannot be found and API keys are missing. A login
  shell is not enough: on zsh it is `~/.zshrc` that sets those up and a login-only non-interactive shell never reads it.
  The daemon therefore snapshots the environment from a **login + interactive** shell. A launch reuses the last
  snapshot while it is valid (under ten minutes old, every `PATH` directory it found still present, since a version
  manager's `PATH` entry can point at a per-shell-instance directory) and retakes one in the background for the next
  launch, because the login shell's serial chain of execs can take seconds; with none valid it waits for a fresh one.
  The snapshot taken once per daemon start to work out which agents are available (`src/availability.rs`) is the first
  one a launch can reuse. The daemon spawns the agent directly rather than inside a shell.
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
- **The Rust toolchain is pinned, and moves with the stable release.** `rust-toolchain.toml` names the exact version
  every machine and CI builds with, and `rust-version` matches it. Building on one version while users install with a
  newer stable lets a lint that newer Rust denies by default break their build unseen. Raise the pin to the latest
  stable rather than holding a floor: an old floor forces exact pins on whole chains of transitive crates (a crate's
  declared `rust-version` is advisory and often wrong in both directions), and the Tauri crates and their
  `@tauri-apps/*` packages in the UI must move together, since the Tauri CLI refuses to build when the two differ in
  major or minor version.
- **Full-width punctuation from a CJK input method needs a workaround.** A mark such as `？`, which an input method
  emits without a candidate window, is swallowed on the first press by unmodified `xterm.js` 6.0.0 in WKWebView. The library arms
  a "key down seen" flag on every keydown, a bare `Shift` included, and drops the commit while it is set, but WebKit
  delivers the commit before the mark's own keydown. With the pinned version, the terminal's key handler
  restores the flag after a modifier-only keydown (`TerminalController`), reaching into a private field that must be
  rechecked on every `xterm.js` upgrade. Composed CJK text is unaffected.
  The user confirmed the workaround with a real input method on 5.5.0; its private-field contract was rechecked
  against 6.0.0, but that upgrade has not had a separate manual input-method check.
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
- **WebView differences.** Tauri uses the system WebView (WKWebView on macOS). There is no Linux shell to pick one:
  Linux is the server form, reached from whichever browser the user has, which the UI treats as a plain browser like
  any other.
- **End-to-end terminal latency was never measured.** Only the daemon-to-WebSocket path was (well under a
  millisecond); the time from key press to glyph on screen was not.
- **PTY output backpressure.** Under heavy output the daemon's PTY reader must take backpressure from the broadcast
  channel rather than running ahead of it: a reader that always runs ahead fills any bounded channel within a fraction
  of a second and the client gets dropped, and neither framing nor batching addresses that. The size of the daemon's own
  read buffer (4 KiB to 256 KiB) made no measurable difference to throughput, so output frame size is not worth tuning.

## Data model

Entities: console, host, project, session, account, report, report-panel page, and the trusted folders. The shapes are in
`apps/daemon/src/store.rs`; what is worth knowing is the reasoning behind a few fields.

- `Session.id` is Octoboard's own and `agent_session_id` the agent's. They are separate because some agents cannot
  pre-allocate an id.
- `Session.bound_to` is the id of the session a session reports to — a console session, or an unbound project session
  or lead session of the same project — or unset. It is always set to the starting session for a session started
  through `new_session`. Once set it is never cleared, but it can change: `transfer_session` hands a session to
  another owner, and `bind_to_session` binds an unbound project session to an owner. An unbound project session bound
  to a console session becomes a lead session: it keeps the sessions bound to it, so orchestration is at most two
  levels deep. `Session.lead` (stored, never sent to clients) marks such a session, because nothing else in the record
  tells a session bound later from one opened bound, and only the former keeps the unbound project session's role
  description and tools on every launch. A console session itself is never bound, and a session bound to a lead session
  owns no other. Reports are routed by this field rather than by a lookup for "the" console session of a console,
  because a console may hold several at once.
- `Session.colour` and `Session.ordinal` are set only for a console session: a badge colour from a fixed palette,
  assigned on creation and never reused while still in use among the console's other console sessions, and a
  per-console ordinal that is one past the highest ever handed out there — kept on the console record itself
  (`consoles.next_console_session_ordinal`) so deleting the console session that held the highest ordinal does not
  let a later one reuse it.
- `Project.path` is stored absolute and lexically normalized (a relative path is refused), because trusted-folder
  entries are compared with it component by component.
- `Account` is a named config directory of one agent, kept once for the whole application. A console refers to one
  account per agent by id (`claude_account_id` and its two siblings), rather than holding a path directly; `None`
  means that agent's default account, the state of pinning nothing. A project carries the same three fields with one
  state more — inherit its console's reference, the agent's default account, or a named account — since pinning
  nothing is a choice of its own that a project may make against a console that names an account. It travels as a
  tagged value, and in storage the id column still only ever holds an account id, the default state living in a
  separate per-agent flag, so nothing stored can be mistaken for an id. `Session.account_id` is the account the
  session's own agent reads, and `Session.config_dir` is that account's directory at the moment the session was
  opened (`None` for the default account) — both copied onto the session when it is opened, because the agent keeps
  its transcripts there and a resume must find them.
- A console's and a project's **per-agent model and reasoning effort** are the opposite: read from the console or the
  project at every launch rather than copied onto the session, so a changed value reaches the session at its next
  resume. Nothing but the agent interprets them, which is why they are stored as free text per agent rather than as an
  enumeration, and why the models a dialog offers are asked of the agent itself for the account in question, kept only
  as a short-lived cache in the daemon: a list maintained in Octoboard would be a second source of truth that goes
  stale with every agent release. A console's `console_session_agent` is no longer a setting of its own: it is kept
  equal to `default_agent`, is not sent to clients, and survives only so that no existing database has to be
  rewritten.
- Every project and session carries a `host_id` while the host table holds a single local record, so going remote needs
  no data migration. Trusted-folder entries ignore `host_id`, which is harmless while every host is local.
- `Page.console_session_id` is the console session that pushed the page, not its console: a console may hold several
  console sessions, each with its own page history. Deleting a console session deletes its pages in the database, and
  deleting a console therefore deletes all of them.
- `Page.anchor_message_id` records the conversation position a page was pushed at. It is stored and never read; a
  linkage that hid pages after the user rewinds the agent's conversation would read it, but no agent exposes a message
  id to put in it yet.
