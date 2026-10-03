# 01 Shell

> Goal: the console / project / session three-level menu and manual sessions work; sessions can be interrupted and resumed,
> archived and reopened; the split daemon / application architecture is in place.
> Done when: a console can be created by hand, projects associated (local directory / parent directory / GitHub clone), a
> session opened under a project with any agent and operated normally in the terminal; the project's own configuration
> (skills, permissions) takes effect inside the session; after the application exits sessions are marked "interrupted" and
> clicking one next time resumes it through `--resume`; archiving and reopening work.

## Landing status

Wrapped up. The daemon and the desktop application both exist, the three-level menu drives real agent sessions, and the
validation prototype is deleted. This document is kept only because of the debt below.

### How the final form differs from the plan

- **The adapters inject the status hooks only.** The MCP server and the role description are the capabilities the hub's
  orchestration announces, and their content is 02's. One consequence has to be planned around rather than discovered:
  Claude Code records an appended system prompt on a conversation's first request and replays it verbatim afterwards, so
  a session opened in this milestone can never be given a role later — 02 starts new sessions rather than resuming these.
- **The agent's own session id is always a freshly minted UUID**, never Octoboard's session id. Both pre-allocating CLIs
  refuse an id that already has a stored conversation, so reusing one would make relaunching a session that was opened
  and never typed into fail with an opaque launch error.
- **`Session.has_conversation` was added to the data model**, which `docs/mvp.md` section 10 does not carry. Without it a
  resume cannot tell "reopen the agent's stored conversation" from "there is nothing stored to reopen": `claude --resume`
  on a session nobody ever typed into fails outright, and a session the user opened and left is the common case of that.
- **Grok's folder trust is Octoboard's own copy, not the `--trust` flag.** The farm copies the user's trust store and
  adds the project to the copy, so Octoboard never writes a trust decision into the user's `~/.grok` on their behalf.
- **Three protocol rules the plan did not anticipate**: no request field may be named `id` (the request envelope's id
  shares one flat object with the request's fields, so a field of that name silently shadowed it and made a delete act on
  the request id); `open_session` answers with the session it started, since a broadcast carries no request id and the
  client otherwise cannot tell which new session is its own; and an `error` carries an optional machine-readable `code`,
  because the one failure a client must recognise — its own double click — was otherwise only distinguishable by prose.
- **Deleting a project or a console takes its session records with it** (the database cascades). The plan says only that
  the association is removed and the directory untouched; both remain true.
- **The application owns its Quit menu item.** macOS's Cmd+Q, application menu and Dock all send `terminate:`, which
  raises no Tauri exit event at all, so the plan's "show a confirmation on exit" is not reachable from the framework's
  own default menu.

### Deliberately left transitional layers

None. What is missing is marked where it will be built rather than stubbed: `TODO(milestone 02)` in the three adapters
and `adapter/mod.rs` (MCP server, role description), in `coordinator.rs` (the hub's instruction file in a console's
working directory), and in the UI's `StatusIcon` and `Sidebar` (the raised hand's bubbling and notifications, and an
archived hub session that the tree cannot currently show); `TODO(milestone 04)` in `adapter/codex.rs` (seeding Codex's
hook-trust hashes, to drop the two warning lines its bypass flag costs) and in the application's menu (the Dock's Quit,
which needs an application-delegate override).

### Debt handed to later milestones

| Debt | Who takes it |
|---|---|
| MCP server and role description in all three adapters; sessions opened in this milestone cannot be re-roled on Claude Code | 02 |
| The hub's instruction file in the console working directory, named for the console's hub agent | 02 |
| `waiting_user` is tracked and shown as a plain state; the raised hand's bubbling to project and console rows, and the system notification, are not built | 02 |
| An archived hub session is unreachable in the tree — nothing archives a hub today, but 02's automatic archiving could | 02 |
| Claude Code's untrusted-workspace warning is not surfaced; the only channel is the terminal's own output, since the agent runs on a PTY | 02 |
| Codex's `approvals_reviewer` / `auto_review` setting is not read, so Octoboard could raise a hand Codex will never show | 02 |
| `send_message` is implemented in the daemon and specified in the protocol, but nothing in the UI calls it | 02 |
| Codex's hook-trust hashes, and the Dock's Quit | 04 |

### How it was verified, and what was not

Verified against the real CLIs (Claude Code 2.1.274, Codex 0.160.0, Grok Build 1.0.46) and in the real window:

- A session launches and renders for all three agents; Octoboard's hooks fire and move the status on Claude Code and
  Grok; **the project's own `.claude/settings.json` hook fires in the same session as Octoboard's**, and `grok inspect`
  through the per-session `GROK_HOME` reports the project trusted, its instruction files loaded, Octoboard's 11 hooks
  plus the project's own, and the user's skills and permissions unchanged.
- Archiving, reopening, `shutdown` leaving sessions interrupted, resuming after a daemon restart, the single-instance
  lock, and all three project sources (a directory, a parent directory's git repositories, a GitHub clone).
- In the window, driven as a user would: creating a console from its dialog (Enter submits it), the action menus,
  associating a project through the directory browser the daemon serves, Escape closing only the topmost layer with a
  picker open over a form, opening a session and having its terminal attach and render the agent's TUI, typing into that
  terminal (including through a CJK input method's composition), archiving a session into its project's Archive group,
  reopening it from there, a session that ends on its own becoming interrupted with a Resume offered, resizing the window
  past 160 columns, and a second instance showing the daemon-failed-to-start screen with its reason and a Quit button.
- By the user, by hand: `Ctrl+C` reaching the agent, `Cmd+Q` raising the exit confirmation, and typing CJK text.

Not verified:

- **Mouse reporting past column 95**, which is what forwarding `xterm.js`'s `onBinary` exists for. The forwarding is
  there and was read line by line, but no agent UI available here reacts to a click in a way that would prove the report
  arrived — Claude Code's composer does not position its cursor by click — and a scripted GUI probe can only ever
  confirm a positive, never a negative.
- **The reconnect banner's states**, which need the daemon to drop a client mid-session.
- **Turn-level status.** No model turn was ever run, so `working` during a turn, `waiting_user` at a permission prompt,
  Grok's two `Stop` fires and Codex's `Interrupt` are implemented from the payloads milestone 00 captured but have not
  been exercised against a live agent. Codex's hooks in particular fire nothing until the first prompt submission, so
  its status mapping is entirely unexercised.
- **A packaged build.** Everything was run from `tauri dev`; packaging, signing and Gatekeeper are milestone 04's and
  05's.

## Technical design

**daemon (`octoboardd`)**

- [x] Host role: PTY and agent process management, directory listing, repository cloning
- [x] Coordinator role: persist console / project / session data, route to hosts
- [x] External protocol: WebSocket (terminal streams, status events, and management commands all share one protocol), bound
  to localhost only
- [x] Storage: SQLite (coordinator side)
- [x] A single agent adapter interface: launch, pre-allocate / obtain a session id, inject, report status, resume. Implement
  three adapters — Claude Code, Codex, Grok Build — choosing injection mechanisms per the conclusions from 00; an agent
  without hook support degrades to an ordinary terminal session with no status display. What the adapters inject in this
  milestone is the status hooks; the MCP server and the role description are the capabilities milestone 02 announces, and
  are left to it
- [x] The adapter tolerates `agent_session_id` being unknown: Codex cannot pre-allocate one, and in its interactive TUI the
  thread is created lazily on the first prompt submission, so a session the user opens without a task has no id until they
  type (Claude Code and Grok both accept `--session-id`)
- [x] The PTY reader takes backpressure from whatever it feeds. In the prototype it always ran ahead, which filled a bounded
  channel within a fraction of a second under heavy output and dropped the client. Output *frame size* is not the lever —
  sweeping it from 4 KiB to 256 KiB changed nothing measurable
- [x] Terminating an agent **must not signal a process group by pid**. Milestone 00's prototype did, and once a session's
  pid had been recycled by the OS the signal landed on an unrelated group: it killed the spawn helper of the editor the
  daemon had been launched from, which left that process unable to start any child at all — its own terminal included —
  until it was restarted. Signal the process itself, and keep a handle rather than a pid; if a group kill is genuinely
  needed, it must be gated on the process being known-alive, not merely registered
- [x] Session teardown lets the agent exit cleanly where it can. Repeatedly killing Claude Code mid-startup during
  milestone 00 tripped its own `fullscreenAutoDisabled` counter and left it rendering in its degraded renderer on that
  machine; agents keep this kind of state about themselves

**Data model**

- [x] `Console { id, name, workdir, hub_agent, default_agent, created_at }`
- [x] `Host { id, name, kind: local|ssh, ssh_config? }`, a single local record in the MVP
- [x] `Project { id, console_id, host_id, name, path, default_agent?, source: local|parent|github, remote_url? }`
- [x] `Session { id, agent, agent_session_id, console_id, project_id?, host_id, role: hub|worker, origin: hub|user, title,
  status, started_at, ended_at }`, where `status` is one of working / waiting_user / idle / interrupted / archived; `id` is
  generated by Octoboard and `agent_session_id` is stored separately to accommodate agents that cannot pre-allocate an id
- [x] Agent selection priority: hub override > project default > console default; the user can pick one too when opening a
  session manually, defaulting to the latter two; reopening an archived session keeps its original agent

**Desktop application (Tauri 2 + React + TypeScript)**

- [x] The three-level menu (console → project → session), sessions labelled with their agent, an "Archive" group under each
  project
- [x] The center terminal (`xterm.js`) connects to the daemon directly over WebSocket, not through Tauri IPC
- [x] The terminal pane is owned by a single object holding the active session, the socket's lifecycle, the connection
  status and terminal focus together, with one transition that sets all four. Milestone 00's prototype split them across
  separate variables and produced two defects from it: clicking anything moved focus off the terminal so input stopped
  reaching the agent while output kept flowing, and a superseded socket's `close` event stamped its status over the new
  connection's `open`
- [x] The UI intercepts `Ctrl+C` itself and forwards `0x03`: WKWebView swallows it, while other modifier combinations
  pass through
- [x] The UI forwards `xterm.js`'s `onBinary` events as well as `onData`. Mouse reports are not UTF-8: past column 95 a
  coordinate byte exceeds 127 and never reaches `onData` at all, so without this the TUI's mouse handling silently stops
  working on any reasonably wide window
- [x] Session state icons: working / awaiting instructions / interrupted / archived (the raised hand lands in 02)
- [x] Creating, editing, and deleting consoles; associating projects: a single directory, a parent directory (git
  repositories beneath it discovered automatically), a GitHub URL (cloned, then associated)
- [x] Associating a project on a volume the application has no file access to must degrade gracefully: a packaged
  application raises a macOS per-volume prompt the development build never shows, and the user may decline it or leave it
  unanswered. Projects will be scattered across volumes, so this is a normal path, not an edge case
- [x] The daemon starts and stops with the application as a sidecar

## Implementation

- Launch: the daemon spawns each agent binary directly with the project directory as cwd, using an environment snapshotted
  per launch from `$SHELL -l -i -c 'env -0'` and filtered of the daemon's own agent variables; injected arguments are
  assembled by the adapter. See "Known pitfalls of the Tauri / Rust approach" in `docs/mvp.md` for why a login-only shell is
  not enough and what the filter is for
- Exit: if any session is in progress when the application exits, show a confirmation; once confirmed, terminate all session
  processes and the daemon. Terminated sessions are not archived but marked "interrupted"; a crash behaves the same way
- Resume: clicking an "interrupted" session makes the adapter use `--resume` (`codex resume` for Codex) and reassemble all
  injected arguments
- Archive: the user archives manually, which ends the process; clicking an archived item reopens it

## Notes for developers

- **Key points**: the retention and cleanup of session data follows each agent's own rules — Octoboard does not manage it;
  no Tauri IPC channel may exist between the UI and the daemon.
- **Reference**: `docs/mvp.md` sections 2, 3, 6, 8, and 10.
