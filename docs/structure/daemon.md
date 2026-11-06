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
their dispatch is `src/cli/mod.rs`, and their code is `src/cli/hook.rs` (`hook`), a provider's `StatusLine`
(`status-line`; `src/cli/mod.rs` only finds the provider's and hands over, Claude Code's is
`src/adapter/claude/status_line.rs`), `src/session_git_hooks/` (`git-ref`, `gate-hook`) and `src/mcp/stdio.rs` (`mcp`). The routes are in `src/server/`. Each mode takes the running
daemon's port and the launch key (`--port`, `--key`):

- `hook` — posts an agent's hook payload from stdin to `POST /hook/:session`.
- `git-ref` — posts created branches and moved refs a session's `reference-transaction` hook saw.
- `gate-hook` — run by a session's `reference-transaction` and `pre-push` dispatchers: asks the daemon whether the
  revert gate applies, runs it in-process, and exits with its status (3 rejects).
- `status-line` — the status line a provider that declares `Capabilities::status_line` injects (`--provider <id>`):
  posts the context window to `POST /context-window/:session` and runs the user's own status line with the same input.
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
data directory), `text.rs` (`trimmed_or_none`, free text as stored, and `without_paste_wrapper`, a prompt with the wrapper an agent puts around a paste taken off), `time.rs` (the clock, `now_millis` and `now_secs`,
and the RFC 3339, compact and readable timestamps) and `wire_path.rs` (`RelPath`, the canonical wire form of a path or
branch name). Test-only files (`tests.rs`, `*_tests.rs`, `test_support/` and `store/test_support.rs`, and the
`layering_tests/` directory) sit beside the code they test, and `apps/daemon/testdata/` holds the captured agent screens
and records tests replay.

### Wire and server

- `src/server/` — the router of `PROTOCOL.md`: `control.rs` the control socket, `terminal.rs` the terminal socket,
  `callbacks.rs` the hook, git-ref, revert-gate, context-window and MCP routes, `access.rs` the Host/Origin/key guard.
- `src/protocol/` — Rust types of the wire protocol, one file per concern, re-exported as `crate::protocol`; `codes.rs`
  is the `Code` registry (a core code can be made only there; a provider's code only through `Code::of_provider` from
  its declaration), `errors.rs` the one mapping from a failure to an `error` reply (with `CodedError::require_offered`,
  which refuses an operation a provider does not offer as `provider_unsupported`), `version.rs` the protocol generation and the daemon's version the `hello` frame states, `bindings.rs` the test
  that renders the types (and the generation) into `packages/ui/src/protocol.generated.ts`.
- `src/cli/` — the command line and the modes above; `hook.rs` is the `hook` mode.

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
  over it), `reporting.rs`, `capacity_retry.rs` (the retries of a Codex turn that stopped at its model's capacity; its
  waits live in `AppState::capacity_retries`), `sharing.rs`, `naming.rs` (how a message names a session, a place, a
  quoted body),
  `owner_notices.rs` (every notice to an owner), `texts.rs` (fixed texts in Octoboard's own name).
- `src/handoff/` — the handoff package built for a session another takes over: build, render and file. The
  `handoff_session` operation is `coordinator/handoff.rs`.
- `src/branch_origin/` — which session is responsible for a git branch: the record (`record.rs`, `target.rs`), the
  reconcile against the repository's branches (`reconcile.rs`), forks (`forks.rs`), transfer (`transfer.rs`), the
  listing, and `gate.rs` with `KeyedGates`, the keyed mutex branch cleanup shares.
- `src/branch_cleanup/` — cleaning up branches and worktrees bound to archived or deleted sessions.
- `src/removal_safety/` — which folder may be moved to the Trash or which linked worktree removed without taking away
  what belongs to something else; `project.rs` is the part for removing a project's folder.
- [`src/session_git_hooks/`](../../apps/daemon/src/session_git_hooks/README.md) — the git hooks of a project session and
  the `git-ref` and `gate-hook` modes.
- `src/revert_gate/` — the revert gate engine run in-process by `gate-hook`: `local.rs` decides a ref update, `push.rs`
  a push, `report.rs` words a rejection, `content.rs` the content criteria, `git.rs` the gate's own `git`.
- `src/config_document/` — export and import of the configuration document that carries state between devices
  (`import/projects.rs` places an imported project, `import/progress.rs` broadcasts the `configuration_import_progress` events of a running import); provider settings travel as the explicit choices of the settings
  their descriptor marks `exported`.

### Storage and state

- `src/store/` — the coordinator's SQLite `Store`: `schema.rs` (the current schema, the `MIGRATIONS` that bring an older
  database up to it and the checks on a file before it is opened; `schema/v{N}.sql` keeps each released past version's
  schema for the migration tests once there is one), `rows.rs`, one file per table family (`settings.rs` the application
  settings, `provider_settings.rs` the user's explicit choices of each provider's settings and the value in effect of
  every one), and `failure.rs`, the store's own `Result` and `StoreError`, which codes the SQLite failures to read or
  write Octoboard's database the user can fix (read-only data folder, full disk, a file that cannot be opened or read)
  as `store_write_failed`.
- `src/state/` — `AppState` (`mod.rs`) with one file per concern of live state (running sessions, claims, status,
  titles, turns, sharing status, git status, requests, availability, hooks to the automatic account switch, forgetting
  a record), plus `bindings.rs` (the read-only walks over which session is bound to which) and `refusals.rs` (the
  locked and archived refusals every operation on a session shares).
- `src/agents/` — what the daemon knows about each agent CLI apart from launching it: availability, `models.rs` (the
  `ModelCatalogue` interface a provider lists its models through, with the cache, the command runner and the checks
  around the lists), launch settings, `json_rpc.rs` (reading JSON-RPC replies, shared by Codex's app server and
  Grok's ACP stdio), `detect.rs` (which agent a project is set up for, from the marker files at its top level).

### Agent accounts

- `src/accounts/` — accounts: `ops.rs` (`AccountOps`, what `AgentProvider::accounts` gives: a provider's quota probe,
  kick, sign-in email, reset credits, sign-in flow, shareable configuration items and plugin registries, thread
  registry), `quotas/` (plan quota read from the agent's CLI, poller, judgement, kicks, reset credits; `probe.rs`
  the `QuotaProbe` interface with the conversation it runs, `reset_credit.rs` the `ResetCredits` interface), `login/`
  (background sign-in; `flows.rs` the `LoginFlow` interface with the process it runs and, for a provider whose sign-in only one login can use, the code a second is refused with), `dirs.rs`, `relocate.rs` (copying a session's record into another account's directory for a
  switch), `thread_registry.rs` (the `ThreadRegistry` interface of a provider's thread database, shared by the switch's copy
  and the shared configuration, with `RowError`, the failure of one row carrying a cause every provider's registry shares; Codex's is `adapter/codex/threads.rs`),
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
  a message into the process (and `UncertainSubmission`, the error of a write whose outcome is unknown;
  `PrivateServer` and `MessageTransport`, a provider's own server a session's messages go through instead of its
  terminal, and `AfterExit`, the work a launch leaves for once its process has exited: `LaunchPlan::after_exit`), `draft.rs`,
  `swap.rs`, `input_box.rs`, `dialogs.rs`, `trust/` folder-trust confirmations (`carry.rs` the `TrustCarrier` interface of a provider that records the trust in
  a per-session home), `interrupt/`, `title_activity.rs` (what
  a terminal title says about a turn), `title_interrupt.rs` (a cancel that left no other trace, read off the title),
  `wait.rs` (polling a condition, `wait_for`, and waiting for a session's output to go quiet, `wait_quiet`).
  `screen.rs` is the `ScreenReader` trait, how one provider's terminal screens, input box and title read, which
  `AgentProvider::screen` gives (`NoScreen` is the default that recognises nothing; `user_action_title` is the title
  text a provider shows when only the user can act); the modules above keep only what is
  done with a reading (the dialogs are kept as `DialogPattern` and `DialogKind`), and read it through the session's
  provider.
- `src/titles/` — the title a session shows, from the agent's records, the terminal title and the first prompt.
- [`src/processes/ledger/`](../../apps/daemon/src/processes/ledger/README.md) — attributing, listing, stopping, keeping
  and carrying over the processes an agent launch started.
- `src/processes/native/` — the native reads of the system's process table (list, start time, parent and children, a
  descriptor's path, sockets, working directory), one reader per platform; shared by the ledger's `table.rs` and
  `terminal/shell/busy.rs`.
- `src/processes/` — the rest: `subprocess.rs` and `leader.rs` (running a
  spawned process within bounds, leading a session of its own), `pids.rs`, `crash_cleanup.rs`.

### Agent records, hooks, adapters and MCP

- `src/providers/` — the provider registry, the facts about each agent provider that do not depend on running it. `mod.rs`
  holds the registry (`REGISTRY`, one line per provider), `AgentId` (re-exported by `protocol`, generated for clients as `Agent`: a `Copy`
  handle on a registered provider's id that only the registry makes, so an unregistered id is refused when
  deserialized; `AgentId::ALL` lists them in registry order and each provider's file declares its own constant),
  `ProviderDescriptor` (id, names, install page, binary,
  layout, the markers that detect a project set up for it, instruction file, tool-name prefix, ask-question tool,
  `Capabilities` (status line, reset credits, per-session trust, configuration sharing, the delivery quirks),
  `quota_windows` (`QuotaWindowNaming`, which windows the provider has and what its own interface calls them),
  `model_display` (`ModelDisplay`, how its model ids are shown), whether other providers can be made compatible
  with it, its `settings` schema and its error and notice `codes`) and `descriptors()`
  in the registry's order. A `SettingDescriptor` declares one user setting of the provider: a choice (some choices
  needing confirmation; unset follows the account's own configuration) or a switch with a default rule, whether a
  configuration export carries it (`exported`), whether the agents' `get_settings` shows it (`shown_to_agents`), and
  which provider's configuration it makes the provider read (`compatibility_with`); `ProviderSettings` and
  `SettingValue` are the values in effect, and the provider reads its own from `LaunchSpec.settings`. A `ProviderCode`
  is a code only that provider raises, named `provider.<id>.<code>` and declared as a `pub const` of its file; `claude.rs`, `codex.rs` and `grok.rs` are one descriptor each, and `test_provider.rs` is the provider only the tests
  register (its behavior is `src/adapter/test_provider.rs`); `layout.rs` is the `AgentLayout` struct (where an agent keeps its
  files) and its lookup; each provider's own `LAYOUT` and the file-name constants only its logic uses are in its file. It is a layer of its own below `protocol`,
  so every layer can read a descriptor; `protocol/providers.rs` serves them to clients as `ProviderInfo` in `snapshot`.
- `src/hooks/` — the `HookInterpreter` trait each provider reads its hook events with (including which event starts a
  conversation and which prompts are the agent's own completion notices or self-sent), what is common to every
  provider's payloads, the per-session ordered queue of events, and applying an event through the session's provider's
  interpreter (`apply.rs`, which also has the provider's record store arm the record watch the interpreter asks
  for).
- `src/records/` — an agent's own on-disk session records, read the same way for every provider; `mod.rs` is the
  index. `store.rs` is the `RecordStore` trait each provider answers through (`AgentProvider::records`) everything that
  depends on how it names, files and writes its records, and on how it watches them (whether it recognises its record
  names, whether a missing record settles anything, the record a hook payload hints at, unarchiving, whether a record's
  head moves during a turn, and how the text a user typed reads out of a line). `locate.rs` finds them (the
  hook's hint, the account directories walked, `record_paths` for every record of a session), `removal.rs` says which
  paths a deletion removes, `resumability.rs` judges whether a conversation can be resumed, `carried.rs` the record a
  switch of accounts carries and what travels with it, `switch_continue.rs` whether a relaunched session is sent a
  prompt to carry on (its text is `messaging/texts.rs`); then the one collector that persists what the records show,
  conversation and history reading, and `watch/` (the poll mechanism every record watch runs on, and `back_at_prompt`,
  putting a session back at its prompt after a watch finding); every pass over a
  record goes through `line_file.rs`.
- `src/adapter/` — the behavior of each provider behind one trait, `AgentProvider` (its descriptor, the launch plan,
  refusing an account that cannot be launched, the installed version, reading the titles it shows, its hook
  interpreter, its record store, its screen reader, its accounts (`accounts()`, `AccountOps`), its model catalogue
  (`catalogue()`, `ModelCatalogue`), its status line (`status_line()`, `StatusLine`, defined in `mod.rs`) and what it
  offers a provider made compatible with it (`compatibility_source()`, `CompatibilitySource`, defined in `mod.rs`: the
  MCP servers and the instructions to read the rest of its configuration; `compatibility_source_of` resolves which
  source a provider's switches select)), registered by
  `adapter::provider(agent)` and implemented by `ClaudeProvider`, `CodexProvider` and `GrokProvider` in `claude/`,
  `codex/` and `grok/` (each provider's `hooks.rs` there is its `HookInterpreter`, and `records.rs` its
  `RecordStore`, with `conversation.rs` and `switch_continue.rs` its readings of a conversation and of how its last
  turn ended, and `screen.rs` its `ScreenReader`; `claude/stash.rs` is Claude Code's stash keybinding gate; `claude/compatibility.rs` (with
  `compatibility.md`, the instruction text) is Claude Code's `CompatibilitySource`;
  `claude/transcript_watch.rs` is the Claude Code rejection and interrupt watch, `claude/status_line.rs` its
  `StatusLine`, `codex/rollout_watch.rs`
  the Codex failed-turn watch and `codex/rollout_files.rs` Codex's rollout file-name and segment-chain rules;
  `models.rs` is a provider's `ModelCatalogue`, and `accounts.rs` its `AccountOps` with the parts `quota.rs`
  (`QuotaProbe`), `login.rs` (`LoginFlow`) and `shared_config.rs` (the items it shares); Codex also has
  `reset_credit.rs` (`ResetCredits`), `threads.rs` (its `ThreadRegistry`), `unarchive.rs` (its unarchiving of a rollout),
  `app_server.rs` (the app-server start and handshake every conversation with `codex app-server` shares) and `transport.rs` with `private_server.rs` (the
  `PrivateServer` its launch plan carries), and Grok `trust.rs` (its `TrustCarrier`)); `mod.rs` holds the shared launch/resume flow, `hooks.rs` the hook URL, command and script, and `account_cli.rs` the account
  directories, commands and short queries (running an agent CLI as an account) (`codex/claude_mcp.rs` only translates a
  compatibility source's MCP servers into Codex's launch overrides). The instruction file, tool-name prefix, ask-question tool and detection markers are read from the
  descriptors by `session_tools/`, `coordinator/console_workdir.rs` and `agents/detect.rs`.
- `src/session_tools/` — the static description of the Octoboard MCP tools and what sessions are told of them:
  `catalogue/` the tool catalogue (`mod.rs` which tools a role sees, then one file per tool family and `schemas.rs`
  the shared schema builders), `role.rs` the role description, `console_instructions.rs` the console session's
  instruction text (written to the console's directory by `coordinator/console_workdir.rs`), and `Owner` and
  `CALL_TIMEOUT` in `mod.rs`.
- `src/mcp/` — the Octoboard MCP server a session drives Octoboard with, over the catalogue in `src/session_tools/`:
  - `dispatch.rs` checks and runs one call through the same coordinator and reporting functions the control socket uses.
  - `tools/` has one file per tool or small group of related tools, named for it — for example `sessions.rs`,
    `messaging.rs`, `archive.rs`, `bindings.rs`, `handoff.rs`, `transfer/` (branches and process trees), `settings.rs`,
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
| providers | `providers` |
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
  (the only place a core `Code` is made); the UI needs its record and wording in the same change. A code only one
  provider raises is a `ProviderCode` const in `src/providers/<id>.rs`, listed in that descriptor's `codes`; its record
  and wording go in the provider's presentation module in the UI.
- A new provider: its descriptor in `src/providers/<id>.rs` and its behavior in `src/adapter/<id>/`, plus one line in
  each registry (`REGISTRY` in `src/providers/mod.rs`, `PROVIDERS` in `src/adapter/mod.rs`); everything keyed by provider
  (`protocol::PerAgent`, the store's per-provider tables) covers the new one without a change, and the UI adds its
  presentation module (see the UI structure doc).
- A new setting of one provider: declared in the provider's descriptor (`settings`) and read by the provider from
  `LaunchSpec.settings`; the store, the wire (`provider_settings`, `set_provider_setting`), the configuration document,
  the MCP `get_settings` tool and the UI's provider settings follow the schema, so none of them changes.
- A new MCP tool: the catalogue in `src/session_tools/catalogue/`, its dispatch in `src/mcp/dispatch.rs` and the tool
  in a file of `src/mcp/tools/`; it calls the coordinator or reporting function the control socket uses rather than
  reimplementing it.
- A new table or column: version changes and migration code are written only between released versions. While the schema
  version has not been released, change `SCHEMA` in `src/store/schema.rs` in place, with no version bump or new step (a
  step that already brings a released version up to it is amended instead). Changing the schema of a released version
  raises `SCHEMA_VERSION`, appends the step from the released version to `MIGRATIONS`, and keeps that version's schema
  as `src/store/schema/v{N}.sql` (the migration tests build a database from every past one). Either way, edit the file
  of its table family. A database at an older version is backed up as `octoboard.db.pre-v{N}` and migrated; `PRAGMA
  application_id` marks Octoboard's databases. What belongs to one provider of a console or a project is a row of a
  child table keyed by the owner and the provider (`console_providers`, `console_launches`, `project_providers`), not a
  column; rows of a provider this build does not register are kept and left out of loads. A plain boolean application
  setting is one line of the `bool_settings!` table in `src/store/settings.rs`.
- Store code returns the store's `Result` (`src/store/failure.rs`), and a rusqlite error must reach `?` as itself:
  wrapping it into `anyhow` first (`.context()`, `anyhow::Error::from`) skips the classification. A caller outside the
  store that returns a store result as `anyhow` unwraps it with `Ok(store.x()?)`. The classification is in the store
  and not where `error_parts` builds the reply on purpose: the daemon also opens the agents' own SQLite files (Codex's
  state database), whose failures must not be reported as Octoboard's data folder.
- State derived at run time from sessions, claims and processes belongs on `AppState` (`src/state/`), not in a stored
  record.
- What is specific to one provider lives in the provider: its descriptor (`src/providers/<id>.rs`) for its facts and
  declared capabilities, and its behavior (`src/adapter/<id>/`), including its accounts (quota, sign-in, reset
  credits, shared configuration, thread registry), model catalogue, status line and trust carrying. Code that works
  for any provider reads the descriptor's fields and branches on the capabilities it declares, or calls the
  provider's interface, never on which provider it is.
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
| A provider's fact (names, install page, binary, instruction file, tool-name prefix, detection markers, declared capabilities) | its descriptor: `AgentId::descriptor` and `providers::descriptors` (`src/providers/mod.rs`) |
| Where an agent keeps its files | the descriptor's `layout` (`AgentLayout`, `src/providers/layout.rs`; each provider's value is its `LAYOUT` in `src/providers/<id>.rs`) |
| A provider's settings, and the codes only it raises | the descriptor's `settings` and `codes` (`src/providers/mod.rs`); stored by `src/store/provider_settings.rs` |
| Refuse an operation a provider does not offer (sign-in, kick, reset credits, configuration sharing) | `CodedError::require_offered` (`src/protocol/errors.rs`), giving `provider_unsupported` |
| What a provider offers one made compatible with it (MCP servers, instructions) | `CompatibilitySource` (`src/adapter/mod.rs`), reached through `adapter::compatibility_source_of`; Claude Code's is `src/adapter/claude/compatibility.rs` |
| A value for every provider (a setting per provider, a model and effort per provider) | `protocol::PerAgent<T>` (`src/protocol/agents.rs`), indexed by `AgentId` and serialised in registry order |
| A provider's behavior (launch plan, installed version, titles, terminal screens and dialogs, accounts, model catalogue, status line) | `adapter::provider(agent)` giving the `AgentProvider` (`src/adapter/mod.rs`); its accounts through `AccountOps` (`src/accounts/ops.rs`), its models through `ModelCatalogue` (`src/agents/models.rs`) |
| Codex's app-server handshake; reading a JSON-RPC reply | `src/adapter/codex/app_server.rs`; `src/agents/json_rpc.rs` |
| Codex's thread state database (its location, repointing rows, carrying or forgetting threads) | `src/adapter/codex/threads.rs` (through `ThreadRegistry`, `src/accounts/thread_registry.rs`) |
| Look up a known record or fail with its `unknown_*` error | `Store::require_project`, `require_console`, `require_account`, `require_session` |
| Whether an agent is on the added list (every agent is while the list is undecided) | `Store::agent_is_added` (`src/store/agents.rs`) |
| Clean a project's or an agent's tag list before storing it | `normalise_tags` (`src/coordinator/validate.rs`) |
| Word what became of a delivery | `Delivery::note` (`src/messaging/delivery.rs`) |
| Word a notice to an owner, or a session's role after its parent changed | `src/messaging/owner_notices.rs` (`tell_owner_*`, `parent_changed_notice`, `new_owner_notice`, `transferred_to_you`, `transferred_to_owner`, `first_report_on_bind`, `role_after_owner_change`, `duties_under_new_parent`) |
| Name a session or put a title on one line in a model-facing message | `src/messaging/naming.rs` (`label`, `named_session`, `one_line`, `quote`) |
| Which agent and account a session started for another opens with; the accounts of an agent as a listing shows them; why one cannot open a session | `src/accounts/choice/` (`started.rs`: `choice_for_started_session`; `automatic.rs`: `candidate_accounts`; `standing.rs`: `cannot_open`) |
| Refuse an operation on a locked or an archived session | `state::refusals::ensure_unlocked`, `refuse_if_archived` (`src/state/refusals.rs`) |
| Walk which sessions are bound to which (an owner's sessions, its team, a group, owned counts) | `src/state/bindings.rs` (`bound_sessions`, `team`, `group_in`, `owned_counts`) |
| Serialize adding projects to one console | `AppState::claim_project_association` (`src/state/claims.rs`) |
| Word what a session taking over another's work is told | `TAKEOVER_OPENING` and `takeover_prompt` (`src/messaging/texts.rs`); the handoff package itself is `src/handoff/` |
| Archive a session whose work is finished (an owner archiving itself, a `done` report) | `coordinator::lifecycle::archive_finished` with its `Finished` rule (`src/coordinator/lifecycle/finished.rs`) |
| Strip escape sequences from terminal output | `terminal::plain_text::strip_escapes` |
| Share the steps of the two transfer tools | `Parties` and friends in `src/mcp/tools/transfer/mod.rs` |
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
