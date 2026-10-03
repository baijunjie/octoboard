# Octoboard MVP Development Plan

## Problem and approach

When using agent CLIs across several projects in parallel, the user has to switch back and forth between terminals, dispatch
tasks by hand, and collect the results by hand.
Octoboard is a desktop control board for orchestrating agents across multiple projects: the user gives a request to the
**hub agent**, the hub decides which project it belongs to, starts a native agent session in that project's directory and
hands it the task; when the session is done it reports to the hub, and the hub reports to the user. The user can step into
any session at any time to watch it or take over.

Every session is a native agent CLI process (Claude Code / Codex / Grok Build). Octoboard only adds organization,
orchestration, and observation; it is not tied to any vendor, and the retention and cleanup of session data follows each
agent's own rules.

The full product plan (concepts, interface, orchestration mechanism, data model, MVP scope) lives in `docs/mvp.md`. This
plan only breaks the work into milestones and tracks progress.

## Key design decisions

- **The daemon is split out already in the MVP**: `octoboardd` (a long-running Rust process playing the host role and the
  coordinator role) and the desktop application (a Tauri client) interact only over a network protocol — no Tauri IPC, no
  shared state.
  Rationale: both of the already-decided follow-up features (background operation and remote hosts) depend on that split, so
  doing it now avoids a later rewrite.
- **Never modify project files or the user's global configuration**: all sessions start with the project directory as cwd,
  and Octoboard's capabilities (status hooks, MCP server, role description) are injected additionally through launch
  arguments by the agent adapters.
- **Communication is structured in tool arguments and natural language in content**: agents are not asked to emit JSON in
  their reply text; fields a program must act on travel as tool arguments.
- **Project sessions go to the user directly (raised hand) for permissions and questions, not through the hub.**
- **Launch agents from a snapshot of the user's login + interactive shell environment**: works around macOS applications
  launched from Finder not inheriting PATH and environment variables. A login-only shell proved insufficient during
  milestone 00 — the snapshot is taken per launch, filtered of the daemon's own agent variables, and the agent binary is
  then spawned directly rather than inside a shell.
- **Every project and session carries a `host_id`**: the MVP has only a single local host record, but this avoids a data
  migration when going remote.

## Scope

The MVP is macOS only. Background operation, remote hosts, report panel linkage with conversation rewind, Windows / Linux,
and other agents are all out of scope for this round (the report page's rewind anchor field is kept nonetheless).

## Milestones

- [x] [00 Technical validation](00-technical-validation.md) — wrapped up; kept for the debt it hands to 01 (see its "Landing status")
- [ ] [01 Shell](01-shell.md)
- [ ] [02 Orchestration](02-orchestration.md)
- [ ] [03 Report panel](03-report-panel.md)
- [ ] [04 Polish](04-polish.md)
- [ ] [05 Final confirmation](05-final-confirmation.md) — the checks that need a real build or an external credential, gathered so nothing is left outstanding at the end

Dependency order: the conclusions from 00 determine the implementation details of 01–03, and any item found infeasible must
be written back into the corresponding section of `docs/mvp.md` before the next milestone starts; 01 → 02 → 03 → 04 proceed
in sequence. 05 is last by construction — it holds only the confirmations that cannot be made until a real build exists or
until an external credential is available, each checked against the earlier milestones to be sure it blocks none of them.
