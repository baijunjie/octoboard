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
server. The CLI modes other processes start are `hook`, `git-ref`, `gate-hook`, `status-line` and `mcp`:
their dispatch is `src/cli/mod.rs`, and their code is `src/cli/` (`hook`, `status-line`), `src/session_git_hooks/`
(`git-ref`, `gate-hook`) and `src/mcp/stdio.rs` (`mcp`). The routes are in `src/server/`. Each mode takes the running
daemon's port and the launch key (`--port`, `--key`):

- `hook` — posts an agent's hook payload from stdin to `POST /hook/:session`.
- `git-ref` — posts created branches and moved refs a session's `reference-transaction` hook saw.
- `gate-hook` — run by a session's `reference-transaction` and `pre-push` dispatchers: asks the daemon whether the
  revert gate applies, runs it in-process, and exits with its status (3 rejects).
- `status-line` — the status line the Claude Code adapter injects: posts the context window to
  `POST /context-window/:session` and runs the user's own status line with the same input.
- `mcp` — the MCP server each adapter registers for a session: speaks MCP on stdin/stdout and forwards tool calls to
  `POST /mcp/:token`.

## Directory map

`src/` has one directory per domain, plus a few helpers at its root: `blocking_pass/` (the width of a pass of blocking
per-entity work, `each_blocking` for an async caller, `blocking_pass/within.rs`'s `within_deadline` for a caller with no
runtime, and, in `blocking_pass/stalled.rs`, `StalledPasses`, the blocking work that outran its deadline and whose
thread is still running), `git_workflow.rs` (the one place the application-wide git workflow values and a project's
overrides are combined: `effective`; the settings types are in `protocol/`), `host_tools.rs` (`HostTools`, the seam
tests substitute stand-ins behind, `git` among them: the one place a project's git gets the user's `git`, which
`isolated_git` hands to browse reads), `instance_lock.rs` (one daemon per data directory), `interface_language.rs` (the
languages the interface is offered in, mirroring the UI's `i18n/languages.ts`), `log_file.rs` (the rotated
`daemon.log`), `loopback.rs` (the std-only HTTP client to the daemon's own loopback address that the CLI modes, the git
hook modes and the MCP stdio bridge call the running daemon with), `paths.rs` (where Octoboard keeps its files under the
data directory), `text.rs` (`trimmed_or_none`, free text as stored), `time.rs` (the clock, `now_millis` and `now_secs`,
and the RFC 3339, compact and readable timestamps) and `wire_path.rs` (`RelPath`, the canonical wire form of a path or
branch name). Test-only files (`tests.rs`, `*_tests.rs`, `test_support/` and `store/test_support.rs`, and the
`layering_tests/` directory) sit beside the code they test, and `apps/daemon/testdata/` holds the captured agent screens
and records tests replay.

### Wire and server

- `src/server/` — the router of `PROTOCOL.md`: `control.rs` the control socket, `terminal.rs` the terminal socket,
  `callbacks.rs` the hook, git-ref, revert-gate, context-window and MCP routes, `access.rs` the Host/Origin/key guard.
- `src/protocol/` — Rust types of the wire protocol, one file per concern, re-exported as `crate::protocol`; `codes.rs`
  is the `Code` registry (a code can be made only there), `errors.rs` the one mapping from a failure to an `error`
  reply, `version.rs` the protocol generation and the daemon's version the `hello` frame states, `bindings.rs` the test
  that renders the types (and the generation) into `packages/ui/src/protocol.generated.ts`.
- `src/cli/` — the command line and the modes above.

### Coordinator role

- `src/coordinator/` — what each control-socket request does to the stored records and which host-role work it triggers:
  `mod.rs` the dispatcher, then one file per request family (`consoles`, `projects`, `open`, `resume`, `launch`,
  `restart` (what the restarts of a session's process share: `run` drives the stop and relaunch of an account switch and
  a model change, the other helpers also serve a sharing change), `bindings`, `lifecycle` (archiving, deleting, and a
  session's own archive when its work is finished: `lifecycle/finished.rs`), `handoff`, `account_switch`,
  `model_change`, `account_sharing`, `accounts`, `settings`, `pages`, `processes` (listing, stopping and keeping the
  process trees a client sees), `messages`, `auto_switch` (moving a session off an account whose usage limit stopped a
  turn; lower layers reach it through `AppState`'s hooks), `validate` for the checks of request text fields), plus
  `console_workdir.rs` (a console's working directory: its instruction file written into it, and its removal),
  `exit_watch.rs` (the exit watcher every launch starts, and `unregister_live`) and `archive_cleanup.rs` (a daemon
  task).
- `src/session_request/` — a session asking the user to approve something (`request_session`, `request_resume_session`,
  account switch, handoff): `mod.rs` the wait, `registry.rs` the requests the daemon holds, `request.rs` what a request
  is, `change.rs` the account switch and handoff variant, `carry_out.rs` what an approval does, `outcome.rs` the wording
  of the outcome.
- `src/messaging/` — every message Octoboard writes into sessions: `delivery.rs` (gates, queueing, what became of each
  message), `outbox.rs` (the per-session ordered queue), `write_queue.rs` (`AppState`'s queuing, flushing and holds
  over it), `reporting.rs`, `sharing.rs`, `naming.rs` (how a message names a session, a place, a quoted body),
  `owner_notices.rs` (every notice to an owner), `texts.rs` (fixed texts in Octoboard's own name).
- `src/handoff/` — the handoff package built for a session another takes over: build, render and file. The
  `handoff_session` operation is `coordinator/handoff.rs`.
- `src/branch_origin/` — which session is responsible for a git branch: the record (`record.rs`, `target.rs`), the
  reconcile against the repository's branches (`reconcile.rs`), forks (`forks.rs`), handover (`handover.rs`), the
  listing, and `gate.rs` with `KeyedGates`, the keyed mutex branch cleanup shares.
- `src/branch_cleanup/` — cleaning up branches and worktrees bound to archived or deleted sessions.
- `src/removal_safety/` — which folder may be moved to the Trash or which linked worktree removed without taking away
  what belongs to something else; `project.rs` is the part for removing a project's folder.
- [`src/session_git_hooks/`](../../apps/daemon/src/session_git_hooks/README.md) — the git hooks of a project session and
  the `git-ref` and `gate-hook` modes.
- `src/revert_gate/` — the revert gate engine run in-process by `gate-hook`: `local.rs` decides a ref update, `push.rs`
  a push, `report.rs` words a rejection, `content.rs` the content criteria, `git.rs` the gate's own `git`.
- `src/config_document/` — export and import of the configuration document that carries state between devices
  (`import/projects.rs` places an imported project).

### Storage and state

- `src/store/` — the coordinator's SQLite `Store`: `schema.rs` (the current schema, the `MIGRATIONS` that bring an older
  database up to it — none yet, version 1 being the baseline — and the checks on a file before it is opened;
  `schema/v{N}.sql` keeps each past version's schema for the migration tests once there is one), `rows.rs`, one file
  per table family, and `failure.rs`,
  the store's own `Result` and `StoreError`, which codes the SQLite failures to read or write Octoboard's database the
  user can fix (read-only data folder, full disk, a file that cannot be opened or read) as `store_write_failed`.
- `src/state/` — `AppState` (`mod.rs`) with one file per concern of live state (running sessions, claims, status,
  titles, turns, sharing status, git status, requests, availability, hooks to the automatic account switch, forgetting
  a record), plus `bindings.rs` (the read-only walks over which session is bound to which) and `refusals.rs` (the
  locked and archived refusals every operation on a session shares).
- `src/agents/` — what the daemon knows about each agent CLI apart from launching it: availability, models, launch
  settings, `layout.rs` (where each agent keeps its files, one `AgentLayout` each), `codex_app_server.rs` (Codex's
  app-server handshake and start arguments), `json_rpc.rs` (reading JSON-RPC replies, shared by Codex's app server and
  Grok's ACP stdio), `detect.rs` (which agent a project is set up for, from the marker files at its top level).

### Agent accounts

- `src/accounts/` — accounts: `quotas/` (plan quota read from the agent's CLI, poller, probes, judgement), `login/`
  (background sign-in), `dirs.rs`, `relocate.rs` (copying a session's record into another account's directory for a
  switch), `codex_threads.rs` (Codex's thread state database, shared by the switch's copy and the shared configuration),
  `trash.rs`, `choice/` (which agent and account a session opens with: `mod.rs` resolving a session's own, `started.rs`
  one a session starts for another, `automatic.rs` automatic selection and the order of an agent's accounts,
  `standing.rs` whether and why an account cannot open one, `refusals.rs` the refusals of one that cannot).
- [`src/accounts/shared_config/`](../../apps/daemon/src/accounts/shared_config/README.md) — an account sharing its
  agent's default configuration directory.

### Host work: git and filesystem

- `src/git/` — the one layer for running `git` and reading repository layout from disk: `env.rs` the settings every
  command shares, `run.rs` building and running (`Runner`), `layout.rs` and `repo_files.rs` where a repository's files
  are and what they say, `reflog.rs` a branch's creation entry, `worktrees.rs` worktrees and their changes,
  `isolated.rs` the reads isolated from the user's environment (with their deadline and stderr ceiling),
  `remote_branches.rs` the lookup of a branch's remote-tracking branch and of the remote default branch, and the small
  readings of git's text shared by several families (`object_id.rs`, `remote_branch.rs`, `tree_entries.rs`).
- `src/project_git/` — git work on a project's own repository: header, status, push, switch, restore, remote, the ref
  watch (`watch.rs`), the re-read after a session's git moves refs (`ref_moves.rs`), clone; `debounced.rs` is the
  scheduler `watch.rs` and `ref_moves.rs` share.
- `src/browse/` — project-browsing reads for the control socket and the MCP read tools; `mod.rs`'s `serve` is the entry,
  with `budget.rs` (including `ChangeBudget`, the one cut of a change listing) and `lane.rs` the limits every read is
  held to (the upstream-lookup budgets live in `git/remote_branches.rs`). `scope.rs` resolves the project, directory and
  worktree a request works in (and a path for a host action: `resolve_project_path`, `resolve_project_directory`,
  `project_live_roots`); `changes.rs` lists the worktree's changes and `change_read.rs` reads one change for them, for
  `compare.rs` and for `commits.rs`; `content.rs` encodes a body for the wire.
- `src/hostfs/` — host filesystem work: bounded reads, regular-file reads and appends, device/inode identity, listing,
  whole-file replacement, Trash, small entry steps (`modified`, `remove_any`, `is_same_path`) and `dir_key`.
- `src/host_open/`, `src/path_links/` — opening the file manager, default application or terminal (every launcher runs
  through one bounded runner); probing and opening the paths a terminal shows, where `path_links/resolve.rs` is the one
  place that decides what a link may reach and what is only shown rather than launched.
- `src/env_shell/` — the user's real shell environment agents are launched with, and resolving a binary on it.

### Running sessions

- `src/terminal/` — PTY processes and what clients see of them (`pty.rs` the descriptor, input, reader thread and child
  lifecycle, `fan.rs` the output fan-out of replay ring, parsed screen and attached clients, `ptyio.rs`, `ringbuf.rs`,
  snapshots, titles and the spinner glyphs they start with, reply routing, `input_class.rs` input that is only the
  terminal's own protocol traffic, `plain_text.rs` output without its escape sequences, `shell/` project shell
  terminals).
- `src/session/` — one running agent process (`LiveSession`), input and message-write gates, `term/` launch and writing
  a message into the process (and `UncertainSubmission`, the error of a write whose outcome is unknown), `draft.rs`,
  `swap.rs`, `input_box.rs`, `dialogs.rs`, `trust/` folder-trust confirmations, `interrupt/`, `title_activity.rs` (what
  a terminal title says about a turn), `title_interrupt.rs` (a cancel that left no other trace, read off the title),
  `wait.rs` (polling a condition, `wait_for`, and waiting for a session's output to go quiet, `wait_quiet`).
- `src/titles/` — the title a session shows, from the agent's records, the terminal title and the first prompt.
- [`src/processes/ledger/`](../../apps/daemon/src/processes/ledger/README.md) — attributing, listing, stopping, keeping
  and carrying over the processes an agent launch started.
- `src/processes/native/` — the native reads of the system's process table (list, start time, parent and children, a
  descriptor's path, sockets, working directory), one reader per platform; shared by the ledger's `table.rs` and
  `terminal/shell/busy.rs`.
- `src/processes/` — the rest: `subprocess.rs` and `leader.rs` (running a
  spawned process within bounds, leading a session of its own), `pids.rs`, `crash_cleanup.rs`.

### Agent records, hooks, adapters and MCP

- `src/hooks/` — reading an agent hook payload, the per-session ordered queue of events, applying an event.
- `src/records/` — an agent's own on-disk session records; `mod.rs` is the index. `locate.rs` finds them (the hook's
  hint, the account directories walked, `record_paths` for every record of a session), `codex_rollout_files.rs` holds
  Codex's rollout file-name and segment-chain rules, `removal.rs` says which paths a deletion removes, `resumability.rs`
  judges whether a conversation can be resumed, `carried.rs` the record a switch of accounts carries and what travels
  with it, `switch_continue.rs` whether a relaunched session is sent a prompt to carry on (its text is
  `messaging/texts.rs`); then the one collector that persists what the records show,
  conversation and history reading, and `watch/` (the poll mechanism in `mod.rs`, `claude_transcript.rs` the Claude Code
  rejection and interrupt watch, `codex_rollout.rs` the Codex failed-turn watch); every pass over a record goes through
  `line_file.rs`.
- `src/adapter/` — one adapter per agent CLI behind a single interface (`claude/`, `codex/`, `grok/`); `mod.rs` holds
  the shared launch/resume flow, `hooks.rs` the hook URL, command and script, and `account_cli.rs` the account
  directories, commands and short queries (running an agent CLI as an account) (`codex/claude_mcp.rs` is Codex's use of
  Claude Code's MCP servers). Other per-agent differences live where they are used (screens and dialogs in `session/`
  and `titles/`, hook payloads in `hooks/`, records in `records/`, sign-in, quota and account switching in `accounts/`,
  tool naming, the instruction file and role text in `session_tools/`, launch and account switch in `coordinator/`,
  model catalogues in `agents/models.rs` and detecting a project's agent in `agents/detect.rs`).
- `src/session_tools/` — the static description of the Octoboard MCP tools and what sessions are told of them:
  `catalogue/` the tool catalogue (`mod.rs` which tools a role sees, then one file per tool family and `schemas.rs`
  the shared schema builders), `role.rs` the role description, `console_instructions.rs` the console session's
  instruction text (written to the console's directory by `coordinator/console_workdir.rs`), and `Owner` and
  `CALL_TIMEOUT` in `mod.rs`.
- `src/mcp/` — the Octoboard MCP server a session drives Octoboard with, over the catalogue in `src/session_tools/`:
  - `dispatch.rs` checks and runs one call through the same coordinator and reporting functions the control socket uses.
  - `tools/` has one file per tool or small group of related tools, named for it — for example `sessions.rs`,
    `messaging.rs`, `archive.rs`, `bindings.rs`, `handoff.rs`, `handover/` (branches and process trees), `settings.rs`,
    `pages.rs`, `account_switch.rs`, `session_model.rs`, `history.rs`, `agents.rs` and `models.rs`, and the reads
    (`reads.rs`, `project_changes.rs`, `project_branches.rs`, `session_branches.rs`). Two files are shared wording and
    helpers rather than tools: `relaunch.rs` (what the tools that relaunch a session tell the caller) and
    `project_browse.rs` (which project a repository read may touch, and running its browse request).
  - `lookup.rs` reads the arguments of a call (including the per-agent ones) and finds the records they name,
    `resolve_account` and `page_bounds` among them; `views.rs` describes a session, project or console back to the
    agent and holds the result fields tools share.
  - `stdio.rs` is the `mcp` mode and `test_support.rs` the tests' shared fixtures.

## Dependency direction

Enforced by the test `layering_tests::every_top_level_module_depends_only_downward_or_as_listed` in
`src/layering_tests/` (`cargo test -p octoboardd layering_tests`). Its `LAYERS` and `UPWARD` constants
(`layering_tests/mod.rs`; the scanner is `layering_tests/scan.rs`) are the source of truth; the two tables below keep in
step with them, and an edge that must point upward is added to `UPWARD` and to the allow-list table together, with its
reason.

The rule: every top-level module sits in one layer, and a module may use its own layer and the layers below it. An edge
(a `crate::` path in production code, in a `use` or inline, from the module the file sits in to the module the path
names) may point to a higher layer only if `UPWARD` lists it, with the items of the target it covers. `state`
(`AppState` holds the live state of every module) may name the types of any module outside the entry layer (`cli`,
`server`) and their associated items (`Outbox::new`) whatever the layer; the functions and constants it reaches upward
need an item entry. The crate root's `APP_NAME` may be named by any module (`ROOT_ITEMS`). The test fails on a new
upward edge, on a module in no layer or two layers, on a layer that names a module `main.rs` does not declare, on an
item of `main.rs` that is not in `ROOT_ITEMS`, on a `.rs` file the walk of `mod` declarations from `main.rs` does not
reach (it would go unchecked), on an `UPWARD` entry that is not an upward edge between layered modules, and on a stale
entry that no production code uses any more. Test code is not scanned: a file reached only through a `#[cfg(test)]`
module, a file under `#![cfg(test)]`, and any item, variant, match arm, field or statement under a `cfg` predicate that
holds only in tests (`cfg(test)`, `cfg(all(test, …))`, `cfg(any(test, all(test, …)))`) are left out. The scan is
lexical: comments and literals are blanked; a name a `use` brings in is followed through the file, but a child module's
`use super::*` inheriting a parent's import is not; a glob import counts as the import of its module; what a macro
expands to is not seen; methods and fields reached through a value are not paths and are not seen, so an entry for a
type admits everything the type exposes; and a test-only field of a tuple struct or variant whose type holds a comma
inside angle brackets (`HashMap<K, V>`) is cut short at that comma, so the rest of its type is read as production code,
which can only add an edge, never hide one.

### Layers, lowest first

| Layer | Modules |
|---|---|
| foundation | `blocking_pass`, `interface_language`, `loopback`, `text`, `time`, `wire_path` |
| wire | `protocol` |
| host | `env_shell`, `git`, `host_open`, `hostfs`, `instance_lock`, `log_file`, `paths`, `processes` |
| storage | `git_workflow`, `store` |
| state | `state` |
| workspace | `branch_cleanup`, `branch_origin`, `browse`, `host_tools`, `path_links`, `project_git`, `removal_safety`, `revert_gate`, `terminal` |
| agent sessions | `accounts`, `adapter`, `agents`, `handoff`, `hooks`, `messaging`, `records`, `session`, `session_git_hooks`, `session_tools`, `titles` |
| control | `config_document`, `coordinator`, `mcp`, `session_request` |
| entry | `cli`, `server` |

### Allowed upward edges

| From | To | Covers | Why |
|---|---|---|---|
| `state` | `accounts` | `shared_config::in_use`, `shared_config::runs_under`, `shared_config::sharing_in_progress` | the sharing locks and claims `AppState` holds decide what an account's sharing change covers by `shared_config`'s own rules, and refuse with its error |
| `state` | `handoff` | `package_file::remove_in` | forgetting a session removes its handoff package; `remove_in` is std-only file work |
| `state` | `project_git` | `header::apply_header` | the git status `AppState` holds takes a fresh header the way `project_git` reads it |
| `state` | `titles` | `project_names`, `resolve` | a session's title is resolved where `AppState` publishes it |
| `messaging` | `coordinator` | `lifecycle::archive_finished`, `lifecycle::Finish`, `lifecycle::Finished`, `lifecycle::Unfinished`, `lifecycle::unfinished` | `messaging::reporting` archives a session that reported it is done, and checks beforehand that it may be, through the coordinator's archive of a finished session |

## Where new code goes

- A new control-socket request: its types in `src/protocol/` (and `PROTOCOL.md`), its handler under `src/coordinator/`
  reached from the dispatcher in `mod.rs`; regenerate `packages/ui/src/protocol.generated.ts` by running the
  `protocol::bindings` test with `OCTOBOARD_WRITE_BINDINGS=1` (the same test fails when the file is stale). It follows
  the conventions in `PROTOCOL.md`'s "What every client implements" section (naming, tri-state update fields, wire paths,
  and what raises the protocol generation).
- A new code the daemon names a failure or notice by: a `pub const` of the matching module in `src/protocol/codes.rs`
  (the only place a `Code` is made); the UI needs its record and wording in the same change.
- A new MCP tool: the catalogue in `src/session_tools/catalogue/`, its dispatch in `src/mcp/dispatch.rs` and the tool
  in a file of `src/mcp/tools/`; it calls the coordinator or reporting function the control socket uses rather than
  reimplementing it.
- A new table or column: raise `SCHEMA_VERSION` in `src/store/schema.rs`, append the step from the previous version to
  `MIGRATIONS`, keep the previous schema as `src/store/schema/v{N}.sql` (the migration tests build a database from every
  past one), and edit the file of its table family. A database at an older version is backed up as
  `octoboard.db.pre-v{N}` and migrated; `PRAGMA application_id` marks Octoboard's databases. A plain boolean application
  setting is one line of the `bool_settings!` table in `src/store/settings.rs`.
- Store code returns the store's `Result` (`src/store/failure.rs`), and a rusqlite error must reach `?` as itself:
  wrapping it into `anyhow` first (`.context()`, `anyhow::Error::from`) skips the classification. A caller outside the
  store that returns a store result as `anyhow` unwraps it with `Ok(store.x()?)`. The classification is in the store
  and not where `error_parts` builds the reply on purpose: the daemon also opens the agents' own SQLite files (Codex's
  state database), whose failures must not be reported as Octoboard's data folder.
- State derived at run time from sessions, claims and processes belongs on `AppState` (`src/state/`), not in a stored
  record.
- Anything specific to one agent CLI belongs in `src/adapter/<agent>/` or where `src/agents/layout.rs` names; code that
  works for any agent reads the descriptor's fields.
- Tests sit beside the code they test (`tests.rs`, `*_tests.rs`); shared fixtures come from `src/test_support/`
  (`mod.rs` re-exports `scratch.rs`, `app.rs`, `stand_in.rs`, `processes.rs`, `git.rs` for repositories, `sessions.rs`
  and `owner_team.rs`, and holds `code_of` and `param_of`, the code and a param of a coded error).

## Shared entry points

New code uses these rather than a second implementation. None is enforced by a check unless stated.

| Need | Entry point |
|---|---|
| Run `git` in a user's repository or against their remote | `src/git/run.rs`: a family's `Runner` (`UserGit` with the call site's `Policy` and `Limits`; `read`, `write`, and `write_remote` for a command that may reach a remote) and `GitError`, the one error type. A command that reaches a remote is built with `remote_command` so the user's own ssh applies (`src/git/env.rs`). Get the user's `git` from `HostTools::git`. Browse reads use `IsolatedGit` (`src/git/isolated.rs`). The revert gate's own `src/revert_gate/git.rs`, which runs inside the user's hook, is the one exception |
| Replace a file whole | `hostfs::replace::replace_file` (`src/hostfs/replace.rs`) |
| Read from the disk within a bound | `hostfs::read_bounded` (with cancel), `read_regular` / `open_regular` for files the project does not own (`src/hostfs/mod.rs`); a project's files go through `browse::live` |
| Read an agent's record | `src/records/line_file.rs`, the bounded line reader (also what record watches use) |
| Tell directories apart; move to Trash | `src/hostfs/identity.rs` (device and inode); `hostfs::trash::move_to_trash` under `src/removal_safety/`'s rules; account directories by `hostfs::dir_key` |
| List every process id | `processes::pids::all` (only `crash_cleanup.rs` keeps its own listing, which must not allocate) |
| Run a spawned process within bounds | `src/processes/subprocess.rs` (`run_bounded`, `run_exchange`, `Bounds`) |
| Run an agent's CLI as an account for a short query | `src/adapter/account_cli.rs`: `query` (admits the run, held to `Bounds::agent_query`), `AdmittedCommand::run`, and `run_query` only where the account's directory is already held (a sign-in); `last_stderr_line` words a failure |
| An account's directory | `adapter::account_dir` (the pinned directory, else the agent's default; `src/adapter/account_cli.rs`) |
| Where an agent keeps its files; Codex's app-server handshake; reading a JSON-RPC reply | `src/agents/layout.rs`; `src/agents/codex_app_server.rs`; `src/agents/json_rpc.rs` |
| Codex's thread state database (its location, repointing rows, carrying or forgetting threads) | `src/accounts/codex_threads.rs` |
| Look up a known record or fail with its `unknown_*` error | `Store::require_project`, `require_console`, `require_account`, `require_session` |
| Whether an agent is on the added list (every agent is while the list is undecided) | `Store::agent_is_added` (`src/store/agents.rs`) |
| Clean a project's or an agent's tag list before storing it | `normalise_tags` (`src/coordinator/validate.rs`) |
| Word what became of a delivery | `Delivery::note` (`src/messaging/delivery.rs`) |
| Word a notice to an owner, or a session's role after its parent changed | `src/messaging/owner_notices.rs` (`tell_owner_*`, `parent_changed_notice`, `new_owner_notice`, `handed_over_to_you`, `handed_over_to_owner`, `first_report_on_bind`, `role_after_owner_change`, `duties_under_new_parent`) |
| Name a session or put a title on one line in a model-facing message | `src/messaging/naming.rs` (`label`, `named_session`, `one_line`, `quote`) |
| Which agent and account a session started for another opens with; the accounts of an agent as a listing shows them; why one cannot open a session | `src/accounts/choice/` (`started.rs`: `choice_for_started_session`; `automatic.rs`: `candidate_accounts`; `standing.rs`: `cannot_open`) |
| Refuse an operation on a locked or an archived session | `state::refusals::ensure_unlocked`, `refuse_if_archived` (`src/state/refusals.rs`) |
| Walk which sessions are bound to which (an owner's sessions, its team, a group, owned counts) | `src/state/bindings.rs` (`bound_sessions`, `team`, `group_in`, `owned_counts`) |
| Serialize adding projects to one console | `AppState::claim_project_association` (`src/state/claims.rs`) |
| Word what a session taking over another's work is told | `TAKEOVER_OPENING` and `takeover_prompt` (`src/messaging/texts.rs`); the handoff package itself is `src/handoff/` |
| Archive a session whose work is finished (an owner archiving itself, a `done` report) | `coordinator::lifecycle::archive_finished` with its `Finished` rule (`src/coordinator/lifecycle/finished.rs`) |
| Strip escape sequences from terminal output | `terminal::plain_text::strip_escapes` |
| Share the steps of the two handover tools | `Parties` and friends in `src/mcp/tools/handover/mod.rs` |
| Change a session's model and effort; resume checks | `coordinator::model_change::set_session_model`; `coordinator::resume::check_can_resume` and `locate_conversation` |
| Drop a live session | `coordinator::exit_watch::unregister_live`, which runs `AppState::remove_live` and `after_live_removed` around withdrawing the session's request |
| Re-read projects when a session's git moves refs | `project_git::ref_moves::moved` |
| Cap a pass of blocking per-entity work | `blocking_pass::each_blocking` from async code; from a thread with no runtime, a pass waited for only until a deadline, with a stuck key skipped while it is stuck: `blocking_pass::within_deadline` (the async sibling for paths is `hostfs::place::looked_within`) |
| Read the system's process table | `src/processes/native/` (`ledger/table.rs` and `terminal/shell/busy.rs` read through it) |
| Resolve a project path for a host action, or judge what a terminal link may reach | `browse::scope::resolve_project_path` / `resolve_project_directory`; `path_links::resolve::reach`, which holds the refusal rules |
| Check a directory exists and can be looked at | `hostfs::require_directory` (absence only from NotFound; any other failure is `directory_unreadable`) |
| Call the running daemon from another process (CLI modes, git hook modes, the MCP stdio bridge) | `src/loopback.rs` (`post`, `connect`, `exchange`; a non-2xx answer is an error whose status `error_status` gives) |
| Store free text trimmed, blank as unset | `text::trimmed_or_none` |
| Read the clock; format a timestamp | `src/time.rs` (`now_millis`, `now_secs`, the RFC 3339, compact and readable stamps) |
| Poll a condition, or wait for a session's output to go quiet | `src/session/wait.rs` (`wait_for`, `wait_quiet`) |
| Resolve the user's shell environment and binaries | `src/env_shell/` (`resolve_binary`) |
| Name a failure or notice on the wire | `Code` in `src/protocol/codes.rs`; failure to `error` reply through `src/protocol/errors.rs` |

The wire contract with the UI is held by two checks: `protocol::bindings` (the generated TypeScript must match the Rust
types) and the UI's `daemonCodes.test.ts` (every code the daemon declares has a record and wording in the UI).
