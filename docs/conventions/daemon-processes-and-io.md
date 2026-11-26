# Daemon processes and I/O conventions

How daemon code starts and ends processes, does blocking work, handles paths and files, runs git, writes text into a
session, and stays portable across operating systems. General daemon rules are in [`daemon.md`](daemon.md); the keys,
screens and modes below are written for an agent through its provider
([`provider-neutral-core.md`](provider-neutral-core.md)). The helpers named here, and the source tests that hold the
daemon to its OS-token and source rules, are in `docs/structure/daemon.md`.

## General principles

- **Every process, read of outside data, wait on another program and git run has a deadline, and every output it
  gathers a cap.** One stalled program, disk or agent must not hold a request or a lock; the writes the stop must
  finish are the daemon's own and are not cut short (see "Blocking work and shutdown").
- **Project files, agent files and the values an agent reports are treated as attacker-shaped.** A project or an agent
  can put a link, a FIFO, a device or a crafted id where a plain file or name is expected, and no fixture of plain files
  shows the difference.

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
  when the daemon ends it, and just reap one that exits by itself.**
- **A process is ended only through `platform::proc`'s waiting paths, and a test asks whether it still runs through
  `test_support::is_running` or `assert_stops_running`, never its exit code**: on Windows a process is gone only once
  its handle is signalled, not once its exit code reads exited. A launch's leftovers are waited for off the runtime,
  outside any lock (`start_kill_leftovers` then `PendingSweep::wait`).
- **The daemon spawns an agent directly, not inside a shell.** A shell in between changes quoting, signals and the
  process tree the sweep depends on.
- **Free text the daemon puts on an agent's command line (a first task) is placed where the CLI cannot read it as an
  option or a subcommand**: after a bare `--`, or joined to its flag (`--prompt=<text>`), as that CLI was measured to
  take it; a CLI that still dispatches a subcommand after `--` (Claude Code) gets the argument shaped by its provider. A
  task can start with `-` (a list) or be one word that names a subcommand (`update`), and the CLI's parser then takes it
  for flags or runs the subcommand, and the task is lost while the daemon counts it as sent.
- **The environment an agent launches with is stripped of every provider's agent-session markers, which are enumerated,
  never matched by prefix.** A daemon started inside an agent session would hand that session's identity to every agent
  it launches, whichever agent is launched; `GROK_HOME`, `CODEX_HOME` and similar are user settings a prefix would drop.
  Everything a launch sets on the agent's process over the snapshot goes through `env_shell::session_environment_with`,
  which records it, and every reading of an
  inherited environment that is to stand for the user's goes through `env_shell::outside_session`, which puts each
  recorded variable back by value; only Octoboard's own markers, the snapshot shell's terminal variables and the
  variables a provider declares as its session's are filtered by name. A value an agent's CLI sets for its commands
  under a name users also set is declared by the provider with its exact value and recorded the same way, never filtered
  by name; the daemon makes its own environment what `env_shell::outside_session` gives of it, once at start and only
  when that environment was inherited from a session (`env_shell::forget_inherited_session`), which a helper mode of the
  executable never does.
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

## Blocking work and shutdown

- **Blocking filesystem or process work in a handler runs on `tokio::task::spawn_blocking` and is awaited.**
- **A pass that runs a blocking task per project, directory or session goes through `blocking_pass::each_blocking`, with
  a deadline per item, not one `spawn_blocking` per item.** The blocking pool is shared with every user-visible path, so
  a few hundred items starve the user's next action. A pass of another shape is held to `blocking_pass::BLOCKING_WIDTH`.
  From a thread with no runtime, `blocking_pass::within_deadline` is the same pass.
- **Work that can run for minutes and that a stop or planned restart should not wait for runs on `std::thread::spawn`,
  not `spawn_blocking`, and checks `AppState::stop_asked` right before each step that cannot be undone.** Once
  `run_daemon` returns, `main` cuts blocking work off after `RUNTIME_SHUTDOWN_WAIT`. The stop waits for only two kinds of
  long task: the agent CLI update (`agents::cli`), and a step that changes the user's own files, which cut short would
  leave them half-changed (the account-sharing change on disk, `accounts::shared_config::on_disk`). Such a step is
  refused once a stop is asked, and awaited once begun; no other kind is added.
- **Whatever the stop must finish (a record the next daemon reads, the handover) is awaited by `run_daemon` itself.**
  Blocking work whose `JoinHandle` is dropped is not waited for past `RUNTIME_SHUTDOWN_WAIT`.
- **The stop's own steps (ending terminals, reading screens, the sweep) each keep a timeout of their own, but the writes
  the stop must finish get no overall deadline.** Cutting them short on a stalled disk would lose the record the next
  daemon reads.
- **A blocking read that can stall runs on a reader thread of its own with an idle deadline, and the number of abandoned
  readers alive is capped.** A stop check cannot interrupt a blocking read.

## Paths and files

- **Paths on the wire are made and validated only through `platform::names` and `wire_path::RelPath`, and whether the
  host can name a path is asked only where it meets the disk (`RelPath::host_path`).** An absolute host path travels in
  the host's native spelling with the path style announced once in the snapshot; a Windows host shows full paths and
  never `~`.
- **A path is resolved against the disk only through `hostfs::canonical`, and compared as written with
  `hostfs::lexical_key`.**
- **Replace a file whole with `hostfs::replace::replace_file`, and expect failure on Windows.** A file held by a dying
  process refuses a replace or a delete; treat that as a transient, not a broken disk, and let startup cleanup tolerate
  it.
- **A project's file content is read only through `live::read_file` or `live::read_resolved` (`browse::live`), or opened
  for a content search through `live::open_searchable`, never `File::open`, `fs::read` or `fs::read_to_string`.** A
  project holds whatever the user or an agent put there: a link out of it, a FIFO whose open blocks, a device that reads
  forever, a file swapped mid-read. A fixture of plain files cannot show the difference.
- **A file the project does not own (agent configuration, home, system-wide, session records) is read as the agent reads
  it, following links, but without blocking:** `O_NONBLOCK`, regular files only, bounded bytes. In an adapter use
  `read_bounded_text` for a file the project keeps and `read_regular_text` for the rest, and
  `files::is_agent_config_dir` to tell the agent's configuration directory. Which reader applies goes by who owns the
  file, not where it lies. Octoboard's own files in its private data directory, which only Octoboard's own processes
  write (the daemon, its holders, the desktop shell), are outside this rule: they are not a file an agent or another
  program provides. This rule is about reading; whether Octoboard may write a file it does not own is
  [P9](principles.md).
- **A worktree's git and common directories are found through `git::layout` (`git_dir`, `common_dir`), and any other
  file in a git directory is read through `hostfs::open_regular` or `hostfs::read_regular` with a bound.** The files an
  agent can plant there (`.git` pointer, `commondir`, reflogs, `packed-refs`) can block or read forever just the same.
- **Where taking an unreadable file as absent is the unsafe answer, a regular file that is there but refused by these
  readers takes the safe side.** A refusal does not mean the agent cannot read it.
- **A path built from a value an agent's hook reported (`Session.agent_session_id`, transcript paths) is built as if an
  attacker chose it.** The id must match the agent's exact id shape, the records directory is one the daemon resolved
  itself, only entries at the agent's layout depth count, and no link may lie between them. Any finder of an agent's
  record by id goes through `records::locate::record_paths` or `records::removal::conversation_files_in`, extended if
  short, never a finder of its own. The hook route takes no `Origin` and its key is readable by any local process.
- **A path taken from text an agent could have written is resolved by the daemon at the moment of opening, a click never
  hands a file to the host opener (it is shown in the viewer), and what the system could launch is never opened.** A
  temp root's contents must be the daemon user's and closed to others, and a network or device path, outside the
  project's own roots, as written or where a symbolic link on the way leads, is refused before that place is touched
  (looking at it makes the host connect out). The opener runs applications, scripts and shortcuts, and what it does
  with a file is up to that machine. The decision is `reach` in `path_links/resolve.rs`; extend it rather than write a
  second.
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
  `blocking_pass::within_deadline` are the shape).

## Git from the daemon

- **Every `git` the daemon starts goes through `git/run.rs`: one `Runner` per call-site family, from
  `HostTools::get().git_for(Pace)`, with a `Policy` and `Limits` defined as constants beside its users, run through
  `read`, `write` or `write_remote` (or `Runner::command` / `remote_command` then `Runner::run`).** Not `Command::new`,
  `git::env::non_interactive`, or any variable `git/env.rs` sets, at a call site; family-wide options go in the
  `Policy`. The one exempt module is `revert_gate/git.rs`. `UNMONITORED` in `branch_cleanup/mod.rs` is the shape.
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

How a message gets past the user's text in the agent's input box is `docs/product/draft-preservation.md`, and how the
code does it is in the module docs of `session/draft_clear.rs`, `session/composer.rs` and `session/paste_apart.rs`.

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
  constants, with no model or message text. Its wait for the agent's answer is bounded by the agent's pace on the slowest
  host (Windows ARM64 under emulation) and ends as soon as what it waits for is seen.
- **One key per write, never joined to a message or to another key, and never repeated blind.** Read what the first did
  (screen, or `AppState::turn_ends` compared with a reading taken before the key) before another press. The one joined
  write is keys an interaction reader chooses together (`NextKey::PressAll`) because the screen between them shows
  nothing to read; they are gated and waited on as one key.
- **Every refusal, start of a wait, discard or write with the user's text logs a `tracing::warn!` naming the session,
  never the text.** The daemon log is the only trace afterwards.
- **The daemon's own terminal writes are semantic `Input` values (`platform::term`, `Pty::write_input`) that each
  backend encodes.** Never write Win32 input mode sequences: the bundled host then holds a lone Escape until the next
  key.

## Platform differences

- **Business code branches on a declared fact (`HostFacts`, `TermFacts`, capabilities), never on the OS.** A needed OS
  branch in business code means the platform signature is wrong, and the signature is what gets fixed; the OS-token
  test catches tokens, not a branch on a fact that only restates the OS.
- **A trait is introduced only where a test needs a fake; platform modules are chosen at module level, with no dynamic
  dispatch.**
- **Test-only code that needs a Unix call is gated by `cfg` or kept out of shared tests**, so tests still build on every
  OS (checked by `pnpm check:windows`).
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
