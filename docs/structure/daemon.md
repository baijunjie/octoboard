# Daemon structure (`apps/daemon/`)

`octoboardd`: a headless Rust binary (edition 2021) that plays two roles in one process. The host role owns PTYs, agent
processes and project shell terminals and works on the machine's files and repositories; the coordinator role stores
consoles, projects, sessions, report pages and agent accounts in SQLite, routes requests to the host role and carries
the stored configuration between devices. Every client reaches it only through the WebSocket/HTTP protocol specified in
[`PROTOCOL.md`](../../apps/daemon/PROTOCOL.md); the product behavior is described in the product docs, not here.

Paths in this doc that do not start with `apps/` or `packages/` are relative to `apps/daemon/`.

Stack (`apps/daemon/Cargo.toml`): tokio, axum 0.7 (HTTP and WebSocket), rusqlite with bundled SQLite, `portable-pty` and
`alacritty_terminal` (PTYs and the parsed screen), `rmcp` (the MCP server), clap (CLI modes), `notify` (repository ref
watching), serde. The crate is a member of the root Cargo workspace beside the desktop shell's Tauri crate.

## Entry points

`src/main.rs` hands a named CLI mode to `cli::run`; otherwise it takes the instance lock, opens the store, binds
`127.0.0.1` on an OS-assigned port, generates the per-launch key, prints the handshake line (port and key) and runs the
server. The CLI modes other processes start are `hook`, `git-ref`, `gate-hook`, `revert-gate`, `status-line` and `mcp`:
their dispatch is `src/cli/mod.rs`, and their code is `src/cli/` (`hook`, `status-line`), `src/session_git_hooks/`
(`git-ref`, `gate-hook`), `src/revert_gate/` (`revert-gate`) and `src/mcp/stdio.rs` (`mcp`). The routes are in
`src/server/`. Each mode takes the running daemon's port and the launch key (`--port`, `--key`) except `revert-gate`:

- `hook` — posts an agent's hook payload from stdin to `POST /hook/:session`.
- `git-ref` — posts created branches and moved refs a session's `reference-transaction` hook saw.
- `gate-hook` — run by a session's `reference-transaction` and `pre-push` dispatchers: asks the daemon whether the
  revert gate applies, runs it in-process, and exits with its status (3 rejects).
- `revert-gate` — runs the revert gate directly with guarded branches, target branches and fork points as arguments;
  exits 3 to reject a change that undoes work already merged into a guarded branch.
- `status-line` — the status line the Claude Code adapter injects: posts the context window to
  `POST /context-window/:session` and runs the user's own status line with the same input.
- `mcp` — the MCP server each adapter registers for a session: speaks MCP on stdin/stdout and forwards tool calls to
  `POST /mcp/:token`.

## Directory map

`src/` has one directory per domain, plus a few helpers at its root: `blocking_pass.rs` (the width of a pass of blocking
per-entity work, `each_blocking`, and, in `blocking_pass/stalled.rs`, `StalledPasses`, the blocking work that outran its
deadline and whose thread is still running), `git_workflow.rs` (git workflow settings and `effective`, the one function
that combines application and project values), `host_tools.rs` (`HostTools`, the seam tests substitute stand-ins
behind), `instance_lock.rs` (one daemon per data directory), `interface_language.rs` (the languages the interface is
offered in, mirroring the UI's `i18n/languages.ts`), `log_file.rs` (the rotated `daemon.log`), `loopback.rs` (the
std-only HTTP client to the daemon's own loopback address that the CLI modes, the git hook modes and the MCP stdio
bridge call the running daemon with), `paths.rs` (where Octoboard keeps its files under the data directory),
`project_trash.rs` and `trash_safety.rs` (which folders may be moved to the Trash), `rfc3339.rs` (timestamps) and
`wire_path.rs` (`RelPath`, the canonical wire form of a path or branch name). Test-only files (`tests.rs`, `*_tests.rs`,
`test_support*`, and the `layering_tests/` directory) sit beside the code they test, and `apps/daemon/testdata/` holds
the captured agent screens and records tests replay.

### Wire and server

- `src/server/` — the router of `PROTOCOL.md`: `control.rs` the control socket, `terminal.rs` the terminal socket,
  `callbacks.rs` the hook, git-ref, revert-gate, context-window and MCP routes, `access.rs` the Host/Origin/key guard.
- `src/protocol/` — Rust types of the wire protocol, one file per concern, re-exported as `crate::protocol`; `codes.rs`
  is the `Code` registry (a code can be made only there), `errors.rs` the one mapping from a failure to an `error`
  reply, `bindings.rs` the test that renders the types into `packages/ui/src/protocol.generated.ts`.
- `src/cli/` — the command line and the modes above.

### Coordinator role

- `src/coordinator/` — what each control-socket request does to the stored records and which host-role work it triggers:
  `mod.rs` the dispatcher, then one file per request family (`consoles`, `projects`, `open`, `resume`, `launch`,
  `restart`, `bindings`, `lifecycle`, `account_switch`, `model_change`, `account_sharing`, `accounts`, `settings`,
  `pages`, `messages`, `account_choice` for which agent and account a session opens with, `validate` for the checks of
  request text fields), plus `exit_watch.rs` (the exit watcher every launch starts, and `unregister_live`) and
  `archive_cleanup.rs` (a daemon task).
- `src/session_request/` — a session asking the user to approve something (`request_session`, `request_resume_session`,
  account switch, handoff): pending requests, the wait, the carrying out.
- `src/messaging/` — every message Octoboard writes into sessions: `delivery.rs` (gates, queueing, what became of each
  message), `outbox.rs` (the per-session ordered queue), `reporting.rs`, `sharing.rs`, `owner_notices.rs` (every notice
  to an owner), `texts.rs` (fixed texts in Octoboard's own name).
- `src/handoff/` — the handoff package built for a session another takes over, and the `hand_off_session` operation.
- `src/branch_origin/` — which session is responsible for a git branch (records, handover, listing, reflog reads).
- `src/branch_cleanup/` — cleaning up branches and worktrees bound to archived or deleted sessions.
- [`src/session_git_hooks/`](../../apps/daemon/src/session_git_hooks/README.md) — the git hooks of a project session and
  the `git-ref` and `gate-hook` modes.
- `src/revert_gate/` — the revert gate engine behind `revert-gate` and in-process `gate-hook`.
- `src/config_document/` — export and import of the configuration document that carries state between devices.

### Storage and state

- `src/store/` — the coordinator's SQLite `Store`: `schema.rs`, `rows.rs`, one file per table family.
- `src/state/` — `AppState` (`mod.rs`) with one file per concern of live state (running sessions, claims, status,
  titles, turns, sharing status, git status, requests, availability, write queue facade, forgetting a record).
- `src/agents/` — what the daemon knows about each agent CLI apart from launching it: availability, models, launch
  settings, `layout.rs` (where each agent keeps its files, one `AgentLayout` each), `codex_app_server.rs`.

### Agent accounts

- `src/accounts/` — accounts: `quotas/` (plan quota read from the agent's CLI, poller, probes, judgement), `login/`
  (background sign-in), `cli_run.rs`, `dirs.rs`, `relocate.rs`, `trash.rs`, `switch_continue.rs`, `auto_switch.rs`
  (automatic continuation of turns stopped by a usage limit).
- [`src/accounts/shared_config/`](../../apps/daemon/src/accounts/shared_config/README.md) — an account sharing its
  agent's default configuration directory.

### Host work: git and filesystem

- `src/git/` — the one layer for running `git` and reading repository layout from disk: `env.rs`, `run.rs`, `layout.rs`,
  `repo_files.rs`, `worktree_list.rs`, `isolated.rs`, `branch_git.rs`.
- `src/project_git/` — git work on a project's own repository: header, status, push, switch, restore, remote, the ref
  watch (`watch.rs`), the re-read after a session's git moves refs (`ref_moves.rs`), clone.
- `src/browse/` — project-browsing reads for the control socket and the MCP read tools; `mod.rs`'s `serve` is the entry,
  with `budget.rs` and `lane.rs` the limits every read is held to.
- `src/hostfs/` — host filesystem work: bounded reads, regular-file reads, device/inode identity, listing, whole-file
  replacement, Trash.
- `src/host_open/`, `src/path_links/` — opening the file manager, default application or terminal; probing and opening
  the paths a terminal shows.
- `src/env_shell/` — the user's real shell environment agents are launched with, and resolving a binary on it.

### Running sessions

- `src/terminal/` — PTY processes and what clients see of them (`pty.rs`, `ptyio.rs`, `ringbuf.rs`, snapshots, titles,
  reply routing, `shell/` project shell terminals).
- `src/session/` — one running agent process (`LiveSession`), input and message-write gates, `term/` launch, `draft.rs`,
  `swap.rs`, `input_box.rs`, `dialogs.rs`, `trust/` folder-trust confirmations, `interrupt/`.
- `src/titles/` — the title a session shows, from the agent's records, the terminal title and the first prompt.
- [`src/processes/ledger/`](../../apps/daemon/src/processes/ledger/README.md) — attributing, listing, stopping, keeping
  and carrying over the processes an agent launch started.
- `src/processes/` — the rest: `requests/` (client-facing process requests), `subprocess.rs` and `leader.rs` (running a
  spawned process within bounds, leading a session of its own), `pids.rs`, `crash_cleanup.rs`.

### Agent records, hooks, adapters and MCP

- `src/hooks/` — reading an agent hook payload, the per-session ordered queue of events, applying an event.
- `src/records/` — an agent's own on-disk session records: finding them, the one collector that persists what they show,
  conversation and history reading, record watches; every pass over a record goes through `line_file.rs`.
- `src/adapter/` — one adapter per agent CLI behind a single interface (`claude/`, `codex/`, `grok/`); `mod.rs` holds
  the shared launch/resume flow. Other per-agent differences live where they are used (screens and dialogs in `session/`
  and `titles/`, hook payloads in `hooks/`, records in `records/`, sign-in, quota and account switching in `accounts/`,
  tool naming, the instruction file and role text in `mcp/`, launch and account switch in `coordinator/`, model
  catalogues in `agents/models.rs` and detecting a project's agent in `hostfs/listing.rs`).
- `src/mcp/` — the Octoboard MCP server a session drives Octoboard with: `mod.rs` the tool catalogue and which tools a
  role sees, `exec/` dispatch of one call through the same coordinator and reporting functions the control socket uses,
  then one file per tool family (reads, project changes and branches, handoff and handover, bindings, settings, pages,
  `stdio.rs` the `mcp` mode).

## Dependency direction

Enforced by the test `layering_tests::every_top_level_module_depends_only_downward_or_as_listed` in
`src/layering_tests/` (`cargo test -p octoboardd layering_tests`). Its `LAYERS` and `UPWARD` constants
(`layering_tests/mod.rs`; the scanner is `layering_tests/scan.rs`) are the source of truth; the two tables below keep in
step with them, and an edge that must point upward is added to `UPWARD` and to the allow-list table together, with its
reason.

The rule: every top-level module sits in one layer, and a module may use its own layer and the layers below it. An edge
(a `crate::` path in production code, in a `use` or inline, from the module the file sits in to the module the path
names) may point to a higher layer only if `UPWARD` lists it, with the part of the target it covers: types only, named
as a module path ending in a type, or the listed items. `state` (`AppState` holds the live state of every module) may
name the types of any module outside the entry layer (`cli`, `server`) and their associated items (`Outbox::new`)
whatever the layer; the functions and constants it reaches upward need an item entry. The crate root's `APP_NAME` may be
named by any module (`ROOT_ITEMS`). The test fails on a new upward edge, on a module in no layer or two layers, on a
layer that names a module `main.rs` does not declare, on an item of `main.rs` that is not in `ROOT_ITEMS`, on a `.rs`
file the walk of `mod` declarations from `main.rs` does not reach (it would go unchecked), on an `UPWARD` entry that is
not an upward edge between layered modules, and on a stale entry that no production code uses any more. Test code is not
scanned: a file reached only through a `#[cfg(test)]` module, a file under `#![cfg(test)]`, and any item, variant, match
arm, field or statement under a `cfg` predicate that holds only in tests (`cfg(test)`, `cfg(all(test, …))`,
`cfg(any(test, all(test, …)))`) are left out. The scan is lexical: comments and literals are blanked; a name a `use`
brings in is followed through the file, but a child module's `use super::*` inheriting a parent's import is not; a glob
import counts as the import of its module; what a macro expands to is not seen; methods and fields reached through a
value are not paths and are not seen, so a types entry admits everything those types expose; and a test-only field of a
tuple struct or variant whose type holds a comma inside angle brackets (`HashMap<K, V>`) is cut short at that comma, so
the rest of its type is read as production code, which can only add an edge, never hide one.

### Layers, lowest first

| Layer | Modules |
|---|---|
| foundation | `blocking_pass`, `interface_language`, `loopback`, `rfc3339`, `wire_path` |
| wire | `protocol` |
| host | `env_shell`, `git`, `host_open`, `hostfs`, `instance_lock`, `log_file`, `paths`, `processes` |
| storage | `git_workflow`, `store` |
| state | `state` |
| workspace | `branch_cleanup`, `branch_origin`, `browse`, `host_tools`, `path_links`, `project_git`, `project_trash`, `revert_gate`, `terminal`, `trash_safety` |
| agent sessions | `accounts`, `adapter`, `agents`, `handoff`, `hooks`, `messaging`, `records`, `session`, `session_git_hooks`, `titles` |
| control | `config_document`, `coordinator`, `mcp`, `session_request` |
| entry | `cli`, `server` |

### Allowed upward edges

| From | To | Covers | Why |
|---|---|---|---|
| `protocol` | `accounts` | types | a sharing failure goes on the wire in the types account sharing reports it in |
| `protocol` | `config_document` | types | the configuration document and its import's findings go on the wire as the types export and import build |
| `protocol` | `git_workflow` | types | the Git workflow settings and overrides go on the wire as the types `git_workflow` resolves |
| `protocol` | `processes` | types | a process tree goes on the wire as the ledger's own `ProcessTree` |
| `protocol` | `titles` | types | a session record carries the titles read off its agent's records as `RecordTitles` |
| `git` | `browse` | `budget::GIT_READ_TIMEOUT`, `budget::GIT_STDERR` | `IsolatedGit` is bounded by browse's read budgets, which keep every read limit in one place |
| `processes` | `state` | types | `processes::requests` answers the clients' process requests off `AppState` |
| `processes` | `store` | `LOCAL_HOST_ID` | `processes::requests` names the local host the way stored records do |
| `store` | `accounts` | `dirs::account_dir_key` | an account update compares directories the way `accounts` does (case folded on macOS) to tell a repoint from a respelling |
| `store` | `titles` | types | a session row stores `RecordTitles`, the titles read off the agent's records |
| `state` | `accounts` | `shared_config::in_use`, `shared_config::runs_under`, `shared_config::sharing_in_progress` | the sharing locks and claims `AppState` holds decide what an account's sharing change covers by `shared_config`'s own rules, and refuse with its error |
| `state` | `handoff` | `package_file::remove_in` | forgetting a session removes its handoff package; `remove_in` is std-only file work |
| `state` | `project_git` | `header::apply_header` | the git status `AppState` holds takes a fresh header the way `project_git` reads it |
| `state` | `session` | `term::FRAGMENT_HAZARD` | the notice for a dropped write queue explains the hazard in the words the session's own write uses |
| `state` | `titles` | `project_names`, `resolve` | a session's title is resolved where `AppState` publishes it |
| `accounts` | `coordinator` | `account_choice`, `account_switch`, `launch::LaunchEdit`, `open::report_failed_launch`, `restart::record_needs_shell_env` | `accounts::auto_switch` answers a usage limit with the coordinator's account switch and launch, the path a switch the user asks for takes |
| `adapter` | `mcp` | `Owner`, `SERVER_KEY`, `role`, `stdio::CALL_TIMEOUT` | an adapter writes the agent's MCP server entry and role text from mcp's server key, role descriptions and call timeout |
| `handoff` | `coordinator` | `bindings`, `lifecycle`, `open` | a handoff opens the receiving session and archives the original through the coordinator's own open and lifecycle |
| `handoff` | `mcp` | `qualified_tool_name` | the text handed to the receiving session names MCP tools as its agent sees them |
| `messaging` | `coordinator` | `bindings`, `lifecycle` | `messaging::reporting` acts on a worker's final report, archiving its team, through the coordinator's bindings and lifecycle |
| `messaging` | `mcp` | `qualified_tool_name`, `tools_for` | the notices sessions are sent name MCP tools as each agent sees them |
| `session` | `mcp` | `Owner` | a launch carries whom the worker reports to, which its MCP role text is written from |

## Where new code goes

- A new control-socket request: its types in `src/protocol/` (and `PROTOCOL.md`), its handler under `src/coordinator/`
  reached from the dispatcher in `mod.rs`; regenerate `packages/ui/src/protocol.generated.ts` by running the
  `protocol::bindings` test with `OCTOBOARD_WRITE_BINDINGS=1` (the same test fails when the file is stale).
- A new code the daemon names a failure or notice by: a `pub const` of the matching module in `src/protocol/codes.rs`
  (the only place a `Code` is made); the UI needs its record and wording in the same change.
- A new MCP tool: the catalogue in `src/mcp/mod.rs` and its dispatch under `src/mcp/exec/` or a tool file; it calls the
  coordinator or reporting function the control socket uses rather than reimplementing it.
- A new table or column: `src/store/schema.rs` and the file of its table family; a plain boolean application setting is
  one line of the `bool_settings!` table in `src/store/settings.rs`.
- State derived at run time from sessions, claims and processes belongs on `AppState` (`src/state/`), not in a stored
  record.
- Anything specific to one agent CLI belongs in `src/adapter/<agent>/` or where `src/agents/layout.rs` names; code that
  works for any agent reads the descriptor's fields.
- Tests sit beside the code they test (`tests.rs`, `*_tests.rs`); fixtures built on repositories use helpers from
  `test_support`.

## Shared entry points

New code uses these rather than a second implementation. None is enforced by a check unless stated.

| Need | Entry point |
|---|---|
| Run `git` in a user's repository or against their remote | `src/git/run.rs`: `UserGit` with a call site's `Policy`, `run` and `run_checked`, `GitError`. Browse reads use `IsolatedGit` (`src/git/isolated.rs`). The revert gate's own `src/revert_gate/git.rs`, which runs inside the user's hook, is the one exception |
| Replace a file whole | `hostfs::replace::replace_file` (`src/hostfs/replace.rs`) |
| Read from the disk within a bound | `hostfs::read_bounded` (with cancel), `read_regular` / `open_regular` for files the project does not own (`src/hostfs/mod.rs`); a project's files go through `browse::live` |
| Read an agent's record | `src/records/line_file.rs`, the bounded line reader (also what record watches use) |
| Tell directories apart; move to Trash | `src/hostfs/identity.rs` (device and inode); `hostfs::trash::move_to_trash` under `src/trash_safety.rs`'s rules; account directories by `accounts::dirs::account_dir_key` |
| List every process id | `processes::pids::all` (only `crash_cleanup.rs` keeps its own listing, which must not allocate) |
| Run a spawned process within bounds | `src/processes/subprocess.rs` (`run_bounded`, `run_exchange`, `Bounds`) |
| Run an agent's CLI as an account for a short query | `src/accounts/cli_run.rs` `admitted_command`, held to `Bounds::agent_query` |
| An account's directory | `adapter::account_dir` (the pinned directory, else the agent's default) |
| Where an agent keeps its files; Codex's app-server handshake | `src/agents/layout.rs`; `src/agents/codex_app_server.rs` |
| Look up a known record or fail with its `unknown_*` error | `Store::require_project`, `require_console`, `require_account`, `require_session` |
| Word what became of a delivery | `Delivery::note` (`src/messaging/delivery.rs`) |
| Word a notice to an owner, or a session's role after its parent changed | `src/messaging/owner_notices.rs` (`tell_owner_*`, `role_after_owner_change`, `duties_under_new_parent`) |
| Share the steps of the two handover tools | `Parties` and friends in `src/mcp/handover.rs` |
| Change a session's model and effort; resume checks | `coordinator::model_change::set_session_model`; `coordinator::resume::check_can_resume` and `locate_conversation` |
| Drop a live session | `coordinator::exit_watch::unregister_live`, which runs `AppState::remove_live` and `after_live_removed` around withdrawing the session's request |
| Re-read projects when a session's git moves refs | `project_git::ref_moves::moved` |
| Cap a pass of blocking per-entity work | `blocking_pass::each_blocking` |
| Call the running daemon from another process (CLI modes, git hook modes, the MCP stdio bridge) | `src/loopback.rs` (`post`, `connect`, `exchange`) |
| Resolve the user's shell environment and binaries | `src/env_shell/` (`resolve_binary`) |
| Name a failure or notice on the wire | `Code` in `src/protocol/codes.rs`; failure to `error` reply through `src/protocol/errors.rs` |

The wire contract with the UI is held by two checks: `protocol::bindings` (the generated TypeScript must match the Rust
types) and the UI's `daemonCodes.test.ts` (every code the daemon declares has a record and wording in the UI).
