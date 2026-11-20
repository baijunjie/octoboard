# Daemon processes and I/O conventions

How daemon code starts and ends processes, does blocking work, handles paths and files, runs git, writes text into a
session, and stays portable across operating systems. General Rust and daemon rules (layering, state, store, timing,
accounts) are in [`daemon.md`](daemon.md); what differs between agents lives in the provider, see
[`provider-neutral-core.md`](provider-neutral-core.md) (the keys, screens and modes below are written for an agent
through its provider, not branched on in generic code). The helpers named here are listed in
`docs/structure/daemon.md`, "Shared entry points".

## Processes

- **Run a spawned process within bounds through `processes/subprocess.rs` (`run_bounded`, `run_exchange`,
  `run_to_file`).** Lead, signal and sweep processes, and hold `Containment` and `ChildHandle`, only through
  `platform::proc`; list pids with `platform::proc_table::pids::all` and read the process table through
  `platform/proc_table/`. A hand-made spawn skips the deadline, the output bound and the sweep.
- **An agent CLI the daemon runs, and everything it left running, is run with `Reach::Session`.** Agent CLIs move
  helpers into process groups of their own, so a group stop leaves them: use `Bounds::agent_query` through `run_bounded`
  (account queries through `adapter::account_cli`, never by hand), or `Bounds { reach: Reach::Session, .. }` for
  `run_exchange`, or build on `platform/proc/` (`lead_session`, `kill_group`, `kill_members`) where it runs on a PTY or
  is read while running. Never write a sweep of your own. A process that is not an agent CLI, such as `git`, keeps
  `Reach::Group`: a session sweep lists every process on the system.
- **Where an agent CLI can start something the user keeps using (a login that opens a browser), sweep its session only
  when the daemon ends it, and just reap one that exits by itself.** An account-CLI run whose job is to hand work to an
  application that must outlive it (`claude --desktop`) uses `AdmittedCommand::run_leaving_descendants`, not
  `Bounds::agent_query`, which would end the application with the CLI.
- **A process is ended only through `platform::proc`'s waiting paths, because on Windows it is gone only once its handle
  is signalled, not once its exit code reads exited.** `end_and_wait` for one process; `kill_leftovers`, or
  `start_kill_leftovers` then `PendingSweep::wait` (off the runtime, outside any lock) for a launch's leftovers; the
  crash kill waits too. Tests ask `test_support::is_running` or `assert_stops_running`, never the exit code. Enforced by
  `a_process_is_ended_on_windows_only_where_it_is_then_waited_for` (`TerminateProcess(` appears only in
  `platform/proc/windows.rs` and `platform/proc/crash/windows.rs`).
- **A shell `trap` for `WINCH` is written only in `test_support/resize.rs`; a script reports a size with
  `test_support::resize::reports_size`.** Bash 5 loses a resize that arrives while a working trap runs. Enforced by
  `a_window_change_trap_is_written_only_in_the_test_support_helper`.
- **The daemon spawns an agent directly, not inside a shell.** A shell in between changes quoting, signals and the
  process tree the sweep depends on.
- **The environment an agent launches with is filtered of agent-session markers, and each provider's descriptor declares
  its own markers.** Generic code strips whatever the descriptors list and names none itself (see
  `docs/conventions/provider-neutral-core.md`, "What breaks the rule"). A daemon started inside an agent session would
  hand that session's identity to every agent it launches. The markers are enumerated, never matched by prefix
  (`GROK_HOME`, `CODEX_HOME` and similar are user settings a prefix would drop), and stripped whichever agent is
  launched. Build each list by dumping `env` in a live session of that agent.
- **A helper process gets the launch key never in its arguments or environment.** Use an owner-only file
  (`key_file.rs`) or the path variable `OCTOBOARD_DAEMON_CREDENTIAL_PATH`; no variable a git hook needs has `KEY`,
  `SECRET` or `TOKEN` in its name, which agents' filters strip. Arguments show in every process listing and agents write
  their environment into files.
- **Code on a per-launch, per-hook-event or per-sweep-item path starts no process if a route without one exists.** Read
  the value from where it is installed (`AgentProvider::installed_version`), serve a cached `env_shell` snapshot, or let
  the other side call the daemon. Where a process is unavoidable, run it off the user's path, cache its result and bound
  how many run at once; one exec can stall for seconds under endpoint-security software, and a login-shell snapshot is a
  serial chain of them.
- **Code run by an `octoboardd` subcommand a person starts from a terminal (`apps/daemon/src/cli/`), and the shared
  helpers it reaches, never take an `env_shell` snapshot.** Read `std::env`; a helper that takes a snapshot is given a
  way to be handed the process's own environment (`UreqFetcher::command_line`). The snapshot's interactive login shell
  is stopped by `SIGTTOU`/`SIGTTIN` on the shared terminal, and the daemon, having none, never shows it.
- **A PTY reader never waits on a client and keeps no bounded channel.** Each client has its own backlog sent as its
  credit window allows; a bounded channel fills within a fraction of a second under heavy output.

## Blocking work and shutdown

- **Blocking filesystem or process work in a handler runs on `tokio::task::spawn_blocking` and is awaited; never
  `block_in_place`.** It panics on the current-thread runtime of a plain `#[tokio::test]`.
- **A pass that runs a blocking task per project, directory or session goes through `blocking_pass::each_blocking`, with
  a deadline per item, not one `spawn_blocking` per item.** The blocking pool is shared with every user-visible path, so
  a few hundred items starve the user's next action. A pass of another shape is held to `blocking_pass::BLOCKING_WIDTH`.
  From a thread with no runtime, `blocking_pass::within_deadline` is the same pass.
- **Work that can run for minutes and that a stop or planned restart should not wait for runs on `std::thread::spawn`,
  not `spawn_blocking`, and checks `AppState::stop_asked` right before each step that cannot be undone.** Once
  `run_daemon` returns, `main` cuts blocking work off after `RUNTIME_SHUTDOWN_WAIT`. The agent CLI update
  (`agents::cli`) is the one exception and no further long `spawn_blocking` work that the stop must wait for is added.
- **Whatever the stop must finish (a record the next daemon reads, the handover) is awaited by `run_daemon` itself.**
  Blocking work whose `JoinHandle` is dropped is not waited for past `RUNTIME_SHUTDOWN_WAIT`.
- **The stop's own steps (ending terminals, reading screens, the sweep) each keep a timeout of their own, but the writes
  the stop must finish get no overall deadline.** Cutting them short on a stalled disk would lose the record the next
  daemon reads.
- **A blocking read that can stall runs on a reader thread of its own with an idle deadline, and the number of abandoned
  readers alive is capped.** A stop check cannot interrupt a blocking read; `pump` in `self_update/fetch.rs` is the
  shape.

## Paths and files

- **Paths on the wire are made and validated only through `platform::names` and `wire_path::RelPath`, and whether the
  host can name a path is asked only where it meets the disk (`RelPath::host_path`).** An absolute host path travels in
  the host's native spelling with the path style announced once in the snapshot; a Windows host shows full paths and
  never `~`.
- **A path is resolved against the disk only through `hostfs::canonical`, and compared as written with
  `hostfs::lexical_key`.** `canonicalize` is an OS token (see Platform differences).
- **A temporary file beside `path` is `platform::names::with_suffix(path, ".partial")`**, never
  `with_extension("partial" | "tmp" | "temp" | "new" | "staging")` or a name keyed by pid, so `starter.plain` and
  `starter.background` never share one. Enforced by
  `a_temporary_file_is_named_after_the_whole_name_of_the_file_it_becomes`.
- **Replace a file whole with `hostfs::replace::replace_file`, and expect failure on Windows.** A file held by a dying
  process refuses a replace or a delete; treat that as a transient, not a broken disk, and let startup cleanup tolerate
  it.
- **A project's file content is read only through `live::read_file` or `live::read_resolved` (`browse::live`), or opened for a
  content search through `live::open_searchable`, never
  `File::open`, `fs::read` or `fs::read_to_string`.** A project holds whatever the user or an agent put there: a link
  out of it, a FIFO whose open blocks, a device that reads forever, a file swapped mid-read. A fixture of plain files
  cannot show the difference.
- **A file the project does not own (agent configuration, home, system-wide, session records) is read as the agent reads
  it, following links, but without blocking:** `O_NONBLOCK`, regular files only, bounded bytes. In an adapter use
  `read_bounded_text` for a file the project keeps and `read_regular_text` for the rest, and
  `files::is_agent_config_dir` to tell the agent's configuration directory. Which reader applies goes by who owns the
  file, not where it lies. Octoboard's own files in its private data directory, which only Octoboard's own processes
  write (the daemon, its holders, the desktop shell), are outside this rule: they are not a file an agent or another
  program provides.
- **A worktree's git and common directories are found through `git::layout` (`git_dir`, `common_dir`), and any other
  file in a git directory is read through `hostfs::open_regular` or `hostfs::read_regular` with a bound.** The files an
  agent can plant there (`.git` pointer, `commondir`, reflogs, `packed-refs`) can block or read forever just the same.
- **Where taking an unreadable file as absent is the unsafe answer, a regular file that is there but refused by these
  readers takes the safe side.** A refusal does not mean the agent cannot read it.
- **Read an agent's records through `records/line_file.rs`, the bounded line reader.**
- **A path built from a value an agent's hook reported (`Session.agent_session_id`, transcript paths) is built as if an
  attacker chose it.** The id must match the agent's exact id shape, the records directory is one the daemon resolved
  itself, only entries at the agent's layout depth count, and no link may lie between them. Any finder of an agent's
  record by id goes through `records::locate::record_paths` or `records::removal::conversation_files_in`, extended if
  short, never a finder of its own. The hook route takes no `Origin` and its key is readable by any local process.
- **A path taken from text an agent could have written is resolved by the daemon at the moment of opening, must lie
  inside a daemon-chosen root after links are resolved, and a file in the user's project is never handed to the host
  opener (it is shown in the viewer).** The opener runs applications, scripts and shortcuts, and what it does with a
  file is up to that machine. The decision is `reach` in `path_links/resolve.rs`; extend it rather than write a second.
- **Resolve a project path for a host action with `browse::scope::resolve_project_path` or
  `resolve_project_directory`.**
- **Deleting, trashing or refusing because a file or directory is missing is judged only from `metadata` or
  `symlink_metadata` failing with `ErrorKind::NotFound`, never `Path::exists`, `is_dir` or `is_file`.** Those answer
  false on any error and an unmounted volume's mount point reads as empty. Any other error is unknown, and unknown
  destroys nothing (`records::Resumability` is the shape). Use `hostfs::require_directory` to check a directory exists.
- **Whether two agent configuration directories are one is decided by comparing `hostfs::dir_key`, and maps are keyed by
  it**; never `==` on the text or a SQL `WHERE config_dir = ?` (exact text only as a "row unchanged since read" guard).
  The default file system on macOS is case-insensitive, so one directory has two spellings.
- **Whether two directories are one, or one lies under another (agent configuration directories aside), is decided by
  device and inode: `hostfs::identity::DirectoryIdentities` and `hostfs::place::Place`**, never by text or
  `canonicalize` (links, case folding, bind mounts and firmlinks defeat both).
- **A path that cannot be resolved counts by what a match does.** Where a match grants (trust) it matches nothing; where
  it protects (refusing a second project) it falls back to the normalised text; where a missed match destroys (Trash,
  worktree removal) it counts as a match. The last decision is made only in `removal_safety/mod.rs`; extend it, do not
  write a second.
- **Directory looks run on blocking threads after the store's lock is released, each with a deadline, and a stuck one is
  recorded in `blocking_pass::StalledPasses`** (`hostfs::place::looked_within`, `identities_within` and
  `blocking_pass::within_deadline` are the shape). A quicker look keeps `Looking::stuck_after` at the full deadline, or
  a merely slow path is recorded as stuck for every decision; a decision that must not be cut short takes `Stuck::Wait`.
- **Move to the Trash through `hostfs::trash::move_to_trash` under `removal_safety`'s rules.**

## Git from the daemon

- **Every `git` the daemon starts goes through `git/run.rs`: one `Runner` per call-site family, from
  `HostTools::get().git_for(Pace)`, with a `Policy` and `Limits` defined as constants beside its users, run through
  `read`, `write` or `write_remote` (or `Runner::command` / `remote_command` then `Runner::run`).** Not `Command::new`,
  `git::env::non_interactive`, or any variable `git/env.rs` sets, at a call site; family-wide options go in the
  `Policy`. The one exempt module is `revert_gate/git.rs`. `UNMONITORED` in `branch_cleanup/mod.rs` is the shape.
- **A `-c` in a write `Policy` reaches the user's hooks that write runs** (through `GIT_CONFIG_PARAMETERS`), so put one
  there only where those hooks may see it; anything they must not see goes elsewhere.
- **The pace is `Pace::Person` for a lookup a person triggers and `Pace::Scheduled` for anything repeating** (a sweep, a
  poll, an MCP tool, background cleanup), which never takes a login shell per call. A project's own repository is
  `project_git::project_git(state, pace)`; browse reads use `IsolatedGit`.
- **A command that may reach a remote is built with `remote_command` or `write_remote` and an `SshConfiguration`.** That
  includes a fast-forward or branch switch in a partial clone. Otherwise the daemon's plain `ssh` overrides the user's
  `core.sshCommand` and a fetch that works from their shell fails here.
- **A git run's deadline is its family's `Limits` (`Limits::deadline`, output ceilings, `Runner::with_limits` for
  another bound), never `Command::output`.** The runner kills the whole process group, which also reaps the `ssh` git
  forked.
- **A git run that only decorates an already-read browse reply gets a deadline of a few seconds defined in
  `browse/budget.rs` (not `GIT_READ_TIMEOUT`); failing or overrunning means "no addition", not a failed reply.**
  Remember repeated asks while their inputs are unchanged, and make every buffer the run holds a constant there, added
  to the reply's reservation (`apps/daemon/PROTOCOL.md`, "Browse budgets").
- **A daemon `git status`, or `git diff` without `--cached`, passes `--ignore-submodules`** (`=dirty` where a moved
  submodule must show, `=all` where submodules are not wanted, `=none` only where the run must match the status
  `git worktree remove` itself asks, as in `git/worktrees.rs`). Without it each submodule adds a whole status to every
  run, and no fixture without submodules shows it.
- **A diff of two versions of a path names each operand `<revision>:<path>` (`:0` for the index) and ends the arguments
  with `--`.** Bare object ids lose the path's attributes, and an operand of that form is also a valid path, so a file
  named like one makes git refuse as ambiguous.
- **A ref handed back to `git` as a revision is written `refs/heads/<branch>` or `refs/remotes/<remote>/<branch>`, taken
  from `%(upstream)`, or named `@{upstream}`.** A short name can resolve to a tag or a branch named `origin/main`. Refs
  listed by `for-each-ref` are matched on the exact `%(refname)`, with one pattern per configured remote and
  `git::remote_branch::split_remote_branch` for short names, since a remote's name may hold a `/`.
- **Paths `git` prints are compared with names read from disk in one Unicode form**: run that `git` with
  `-c core.precomposeUnicode=false`, or normalise both sides. macOS stores some names decomposed.

## Writing text into a session

- **Every new kind of text the daemon writes into a session's input begins with a fixed named constant (`*_OPENING` in
  `messaging/texts.rs`) listed in `OCTOBOARD_OPENINGS` (`records/history.rs`).** History tells Octoboard's text from the
  user's only by how it begins; a missing entry shows as the user's own words and no test fails. A "Message from
  Octoboard" notice is built from `NOTICE_PREFIX` or `notice_prefix!`, never a retyped literal, and an opening that
  reads like user text also gets a fixed first-line phrase matched (`BRANCHES_MARKER`).
- **Text one session, or Octoboard on its behalf, writes into another carries a header saying sender, relation and what
  it is (`docs/product/message-headers.md`).** Passed-along text is quoted with `naming::quote`, a parent's instruction
  goes through `texts::render_instruction`, and a new header is named in `message_headers` in `session_tools/role.rs`
  and the console session's instruction file (a test checks the file). The user's own typing is the only text with no
  header.
- **A key the daemon types into a session on its own is written through `LiveSession::write_keys_if`, with a gate that
  re-checks the target state under the input lock right before each key.** The same key does harm in another agent state
  (an interrupt at an idle prompt exits some agents). Read whether a turn runs only through
  `LiveSession::turn_reading()`; a new kind of key adds its own method there, saying which way it errs. The bytes are
  constants, with no model or message text.
- **One key per write, never joined to a message or to another key, and never repeated blind.** Read what the first did
  (screen, or `AppState::turn_ends` compared with a reading taken before the key) before another press.
- **A wait for the agent's answer to a key is bounded by the agent's pace on the slowest host (measured on Windows ARM64
  under emulation) and ends as soon as what it waits for is seen.**
- **A step that reads or clears the agent's input box holds `AppState::pause_outbox` from before the first key until
  done** (except one typed from inside the queue's own write, like `draft_clear.rs`), and reads the box from
  `screen_text_undimmed`, never a faint-blanking reader of its own.
- **A step that acts on a non-empty box soon after a daemon write waits, bounded, for the box to empty
  (`LiveSession::settled_composer`).** The box still shows the daemon's own message for a moment.
- **A decision on a key, or on writing because the box is empty, is made only from a box read off a whole frame:**
  `wait_for_whole_frame`, then `read_box` (or `composer_for_draft`) under the input lock, refusing, to be retried, where
  the output total is `None`. `composer()` and `settled_composer()` are only a cheap first look.
- **A step whose own keys change what the box holds tells the typing record in the same step as the write, under the
  input lock** (`Draft::cleared_by_octoboard`; `Draft::restored_by_agent` for text the agent puts back). Otherwise later
  messages wait on text the box no longer holds.
- **On the path of a message into a session whose box may hold the user's text, each attempt tries the agent's native
  stash, then the clear, first, and refuses the write (the message stays queued and is retried) where one fails or
  cannot be confirmed.** What the user sees is in `docs/product/draft-preservation.md`, "When the composer cannot be
  cleared".
- **Every wait or hold on that path is bounded by `draft_clear::UNCLEARABLE_DRAFT_WAIT`, kept per message under
  `LiveSession::message_key`, never under its text, per session or per process.** A new refusal or hold gets the same
  bound, and a test of a forced write also sends the next message and asserts it waits out its own grace and bound.
- **A switch that turns such a step off ends once the state that caused it has moved** (`stash_unsettled_at` ends at the
  next prompt sent), never for the rest of the session or process. Where the cause is the agent build itself (a stash
  indicator the provider does not read), that state moves only when the launch ends, so the switch lasts the launch (in
  memory: a daemon that adopts the launch after a restart starts with it on again).
- **The write as the box stands hands back what the box held in exactly one notice** (`message_sent_with_draft` with the
  draft read before any clear, or `draft_discarded`): never lost unseen, never reported twice.
- **Every refusal, start of a wait, discard or write with the user's text logs a `tracing::warn!` naming the session,
  never the text.** The daemon log is the only trace afterwards.
- **A gate on a terminal mode the agent sets (bracketed paste, focus reporting, alternate screen) rests on a real
  capture of the whole start-up output for every place the mode switches on and off, gates on the mode being on as of
  the latest output at the point that counts (not "seen on once"), and is tested with the toggle split across two reads
  each held longer than the watcher's poll interval.** An agent can switch a mode on and straight off while probing the
  terminal.
- **The daemon's own terminal writes are semantic `Input` values (`platform::term`, `Pty::write_input`) that each
  backend encodes.** Never write Win32 input mode sequences: the bundled host then holds a lone Escape until the next
  key.
- **Where the host reads console input records, a paste's Enter is written apart, once the paste is drawn.**

## Platform differences

- **Every OS-specific token in production code appears only under `src/platform/`, with no exceptions and no
  allow-list.** The tokens are `libc::`, `windows_sys::`, `portable_pty::`, `tokio::signal::unix`/`windows`, `std::os`,
  `cfg(target_os | unix | windows)`, a `/bin/sh` literal, mode bits and octal literals, symlink calls and
  `canonicalize`. Enforced by `layering_tests::os_tokens_appear_only_in_the_platform_layer`, which leaves test-only code
  out; the desktop crate keeps its OS code in per-platform modules (`docs/conventions/desktop-shell.md`).
- **Business code branches on a declared fact (`HostFacts`, `TermFacts`, capabilities), never on the OS.** A needed OS
  branch in business code means the platform signature is wrong, and the signature is what gets fixed. The token test
  covers tokens, not facts.
- **A trait is introduced only where a test needs a fake; platform modules are chosen at module level, with no dynamic
  dispatch.**
- **Test-only code that needs a Unix call is gated by `cfg` or kept out of shared tests**, so tests still build on every
  OS (checked by `pnpm check:windows`).
- **Call CoreFoundation only through `src/platform/cf.rs`**, not a second set of declarations.
- **On Windows, read terminal state from the parsed screen, the title event and the hooks where the host renders the
  stream.** The system console host's stream is its repaint: titles collapse, frames between paints are lost and phrases
  are split by cursor moves.
- **A connect to a loopback port that falls back when refused has a deadline well past Windows's 2 to 3+ second
  refusal.** A shorter one times out before the fallback runs and drops what the call carried (git-ref's launch route
  allows 6 seconds).
- **On macOS, host integrations (daemon and desktop shell) reach the system through APIs that need no consent
  (Foundation APIs such as `NSFileManager`, `/usr/bin/open`), never Apple Events** (`osascript`, AppleScript, Scripting
  Bridge, the `trash` crate's default Finder method). The shipped app's Apple Events entitlement exists only for
  programs agents run, and an Apple Event raises a prompt naming Octoboard mid-action. Development hides it, since the
  terminal holds the consent; see `docs/product/macos-privacy-permissions.md`.
