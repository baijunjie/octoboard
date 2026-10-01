# 00 Technical Validation

> Goal: before any product code is written, settle one by one the untested premises the plan depends on, giving each a
> conclusion (feasible / not feasible / feasible with conditions) and the way it was verified.
> Done when: every investigation item below has a conclusion; infeasible items have been adjusted along their fallback and
> written back into the corresponding section of `docs/mvp.md`; and the validation prototype can run the whole chain
> "daemon starts all three agents over a PTY → hooks report status → a message reaches a running session → an MCP tool gets
> called → the frontend renders the terminal over WebSocket".

## Technical design

Prototype scope: the daemon launches all three agents over a PTY; hooks report status; messages are written into a running
session; a minimal MCP server is injected; the frontend renders the terminal with `xterm.js` over WebSocket.
The known command-line flags and the to-be-verified items for each agent's injection mechanism are in the "Agent adapters"
table in section 6 of `docs/mvp.md`.

## Investigation items

Conclusions and the way each was verified are recorded after the item; the parenthesized text is the fallback if it turns out
not to be feasible.

### Common to all agents

- [ ] G1 With the project directory as cwd, the project's own configuration (instruction files, skills, hooks, permissions)
  and the injected content both take effect without overriding each other (change the injection mechanism; project
  configuration must not be overridden)
- [ ] G2 When launching through the login shell, PATH, API keys, and each agent's login state are fully available, including
  when the application is launched from Finder (read the user's shell environment and pass it in explicitly)
- [ ] G3 Status events cover: work started, stopped, permission requested, question asked of the user, resumed after the user
  answered (missing states degrade to not being displayed, or are inferred from terminal output patterns)
- [ ] G4 When a hook fails or the daemon is unreachable, the agent neither hangs nor raises errors that disturb the user
  (hook scripts fail fast and exit silently)
- [ ] G5 The injected MCP server can distinguish identities per session, and the hub and workers see only their own tools
  (use different ports or paths per role)
- [ ] G6 Whether injected arguments must be passed again when resuming an interrupted session with `--resume`, and whether
  they take effect (the adapter reassembles all injected arguments on resume)
- [ ] G7 Using the Stop hook to block a stop and demand a `report` call is feasible; "already reported this turn" and
  "currently waiting for the user" can be determined (do not block; go straight to the fallback report)

### Sending messages to a running session

- [ ] M1 Claude Code / Grok: multi-line text written over the PTY as bracketed paste + Enter is submitted as one complete
  message (switch to single-line escaped text, or look for an official message-injection interface)
- [ ] M2 Behaviour when writing while the session sits at a permission prompt, a multiple-choice question, or mid-execution —
  above all, ruling out being misread as a keypress selection (write only in the "awaiting instructions" state, queue in
  every other state)
- [ ] M3 Codex `codex queue`: when the message is consumed, whether it works while the TUI is running, and whether it
  conflicts with PTY input (fall back to PTY input)

### Per-agent specifics

- [ ] A1 Claude Code: hooks injected via `--settings` merge with, rather than override, the hooks in the project settings
  (they must merge; otherwise change the injection mechanism)
- [ ] A2 Codex: whether a session id can be pre-allocated, and where to obtain it reliably otherwise (match the session
  directory by cwd and start time)
- [ ] A3 Codex: which hook events are supported and in what payload format, and whether they can be injected via `-c` without
  modifying `~/.codex/config.toml` (use an environment variable to point at a separate config directory)
- [ ] A4 Codex: how to append a role description without replacing the default system prompt (write it into the initial task
  prompt)
- [ ] A5 Codex: an MCP server injected via `-c mcp_servers.…` takes effect in the interactive TUI (same as A3)
- [ ] A6 Grok Build: how to inject hooks and MCP, and whether it is possible without modifying project or user-global
  configuration (a separate config directory; failing that, degrade to an ordinary terminal session)
- [ ] A7 Grok Build: how to append a role description (write it into the initial task prompt)

### Terminal and architecture

- [ ] T1 All three agents' TUIs render and behave correctly in `xterm.js` inside WKWebView: full screen, mouse, keyboard
  shortcuts, CJK input methods, window resizing (targeted configuration, e.g. Grok's non-full-screen mode)
- [ ] T2 The latency of daemon → WebSocket → `xterm.js` and the throughput under heavy output (batch frames together, use
  binary frames)
- [ ] T3 After the frontend disconnects and reconnects, the terminal contents are restored from the daemon's cached output
  (replay raw PTY output from a ring buffer)
- [ ] T4 Packaging, signing, and the lifecycle of the daemon as a Tauri sidecar: reliably terminated when the application
  exits, no orphan processes after a crash (the daemon watches its parent process and exits once the parent is gone)

## Notes for developers

- **Key points**: the UI and the daemon interact only over the network protocol, and the prototype obeys that too; agents are
  always launched via `$SHELL -l -c …`; injection must not modify project files or the user's global configuration.
- **Reference**: `docs/mvp.md` sections 4.4 and 6.
