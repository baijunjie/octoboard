# 03 Planned restart handover

> Goal: an explicit restart of the daemon hands every terminal over to its successor with its exact screen.
> Completion criteria:
> - Request a planned restart of a daemon started by hand, not following an application, while sessions write output:
>   - every session keeps running, and its screen is exactly as it was, not marked inexact;
>   - the output written during the restart appears once;
>   - the daemon answers the queries in that output.
> - A plain stop still ends every session, as today.
> - After a planned-stop request, a daemon started by hand next restores every screen exactly.
> - A daemon that follows an application refuses the planned-restart request.
> - Under a systemd user service, the planned restart's successor is started by systemd and the sessions survive.

## Technical design

- [ ] A planned-restart request on the daemon's control interface, which makes a planned stop and starts the
      successor on the same address. A daemon that follows an application refuses it.
- [ ] Under systemd the daemon does not start the successor: after its planned stop it exits with an agreed status,
      and the unit's restart rule restarts it on that status, so the successor runs in the unit. A daemon with no
      service manager starts its successor itself.
- [x] A planned-stop request, which makes the planned stop and starts no successor. The desktop update restart of
      milestone 04 sends it.
- [ ] A `restart` command beside start, stop and status, which makes the planned-restart request.
- [x] A planned stop:
  - The daemon tells each holder it is handing over.
  - It saves every terminal's screen, including a half-received escape sequence.
  - The holder marks where the handover falls in each tail, which keeps the history before it.
- [ ] The successor restores each saved screen and replays only the tail written since.
- [x] Query answers produced while replaying a tail are written to the program only after a handover. After a crash
      they would arrive as stray input. A replay never answers a query the holder already answered while no daemon was
      linked (the kinds its welcome lists in `answers_offline`), nor one that is stale.
- [ ] A handover the successor verifies as exact skips the inexact mark and the one-row redraw that every adoption
      makes in milestone 02; a process already marked inexact is never cleared by a handover.

## Implementation plan

- [ ] Add the handover message and screen saving, then restore on start.
- [ ] Add the `restart` command, and route any future server-form update path through it.

## Handoff

- The systemd case (the successor started by the unit and the sessions surviving) is blocked: the daemon exits with
  status 75 after its planned stop and the unit restarts it on that status (`RestartForceExitStatus=75`), but holders
  still run inside the daemon unit's cgroup, which systemd stops with the main process, until milestone 02's systemd
  parts make each holder a user unit of its own.

## Mid-task handoff

**State.** Every technical-design item and both implementation-plan steps are written on `hold/planned-restart`, and
an independent review's changes on top of them on `hold/planned-restart-review`. It compiles and passes clippy on macOS and the
Windows cross-check, and the UI side passes; a first full Rust test run proved the ticked boxes, and the Rust tests
remain to be rerun after the fixes that followed it. The open boxes wait on failed or unreached tests, systemd or
manual runs. There is no server-form update path yet to route through
`restart`.

**Open against the completion criteria.**
- "Exact" is not yet unconditional: a saved screen is exact only for the states the checkpoint reproduces (below); the
  others are saved marked inexact: restored with the tail since the mark, marked, and redrawn (own choice: their content
  beats a rebuild from the tail). The normal screen behind an alternate one, scrollback included, is now saved: the save
  reads the hidden grid by swapping the screens through `alacritty_terminal`'s public `swap_alt` and putting the
  alternate grid back with `grid_mut` (`TerminalSnapshot::normal_behind_alternate`). The one case left out is a width
  change while the alternate screen is shown, until the program enters it again: the reflow may move the hidden cursor
  away from the one entering saved, which entering again in the successor cannot rebuild, and reading it would overwrite
  the live saved cursor; the successor then shows a blank normal screen behind the program. A client's first paint reads
  the hidden normal screen the same way (`TerminalSnapshot::paint`), under the same width-change limit, so a client
  attached while the program is on the alternate screen shows it when the program leaves. Milestone 04's exact-screen
  claim inherits the remaining limits.
- Lossless handover is bounded by the tail: more than 1 MiB written after the mark (2 MiB with a welcome pin) loses
  exact eligibility. Unconditional no-loss would need a disk-backed post-mark spool with acknowledged offsets.
- The systemd case stays blocked (see Handoff), and so does service key delivery: a manager-started successor makes a
  key no client can obtain until the unit's key-delivery TODO is done. Never journal it, put it in `daemon.owner`,
  argv or the environment.

**To pick it up.**
1. Compile: `cargo check -p octoboardd --all-targets`, clippy with `-D warnings`, `pnpm check:windows`. Riskiest spots:
   `holder/server/terminal.rs` (`Terminal::hand_over`, `Handle::block_on` from a blocking thread),
   `holder/client/link.rs` (`hand_over`, the handed-over frame filter, `Sending`, `Adopted`), `terminal/pty.rs` (the
   `ToProgram` answer writer and its fence, `hold_client_changes`, `write_reply`, the owed client replies),
   `terminal/snapshot/{save.rs,unexposed.rs,unended.rs}` (what is left out, the REP re-arm, `blank_normal`),
   `terminal/fan.rs` (the replay window's split chunk), `state/handover_barrier.rs`, `state/restart_screens.rs`,
   `messaging/{outbox.rs,restart_outbox.rs}`, `coordinator/planned_stop.rs` (the `wait_unless` closures, the
   `&dyn Fn` escalation predicate), `state/stop.rs` (`watch::Receiver::wait_for`), `state/process_exit.rs`
   (`stop_all_sessions_after_handover`'s partition and chained handles), `holder/client/mod.rs` (`end_handed_over`,
   `take_links_of`, `part_all`, `HeldRelays`' `Weak<Shared>`, `link_pending` taking `&Arc<Self>`),
   `terminal/shell/mod.rs` (`end_all`'s and `close_all_ending`'s `impl Fn(&ShellProcess)` closures),
   `cli/installation.rs`, `main.rs` (the stop kinds, the cached restart policy, the successor, `taking_signals` and the
   fused termination signal, `stop_all_processes_after_planned`).
2. Tests: `holder::`, `terminal::` (incl. `terminal::snapshot`, `terminal::fan`, `terminal::pty`,
   `terminal::saved_screen`, `terminal::program_status`, `terminal::shell`), `coordinator::planned_stop`,
   `coordinator::adopt`, `coordinator::exit_watch`, `coordinator::lifecycle`, `coordinator::projects`,
   `coordinator::archive_cleanup`, `coordinator::handoff`, `coordinator::account_switch`,
   `coordinator::model_change`, `coordinator::account_sharing`, `coordinator::processes`, `state::handover_barrier`,
   `state::stop`, `state::process_exit`, `state::restart_screens`, `messaging::`, `server::`, `session::interrupt`,
   `cli::`, `instance_lock`, `request_guard`, `protocol::`, `bindings` (the TypeScript `planned_stop` entries were
   written by hand), `layering_tests`, `platform::proc`.
3. By hand on macOS (and Linux once built): `octoboardd start --port N`, sessions and a shell terminal writing output
   (and asking device attributes and the cursor position), `octoboardd restart`: same port and key, every plain screen
   exact and not marked inexact, an alternate-screen program over a non-blank shell exact too (the shell shown exactly
   once the program leaves, in the daemon's screen and in every client), marked inexact only after a width change while
   it was shown, the output written meanwhile shown once, the queries answered once; a session already marked inexact
   stays so; a plain `stop` still ends everything; `POST /planned-stop` then `start` restores exactly; `POST /restart`
   and `restart` refused for a daemon started by the application; a restart through a symlink repointed to a new build
   starts the new build. Windows: `restart` says it is not supported and `POST /restart` answers 409 `no_successor`.

**Decisions, with reasons.** The first set were made without the user; an independent review then kept some and
changed others, and its changes were accepted as the decision. Where the review left a choice, the choice is marked
(own choice).
- `restart` delivers the planned-restart request as `SIGUSR1`, as `stop` delivers its request as a signal: the command
  line never holds the daemon's key. The owner record gains a third token (`application` or `alone`); `restart`
  refuses a daemon that follows an application, and one whose record lacks the token (an older build, which
  `SIGUSR1` would kill). An older CLI cannot read the three-token record: use the installed matching CLI. On Windows,
  where the daemon cannot run in the background, `restart` is unsupported; planned stop is not.
- The control interface carries `POST /restart` (409 with `reason` `follows_application` or `no_successor` when
  refused) and `POST /planned-stop`, beside `POST /shutdown`, and a WebSocket `planned_stop` request. There is no
  WebSocket `restart`. A planned stop is never refused, but accepting it promises nothing about what was saved: a
  caller watches the process exit and the successor's recovery, and an escalation repeats the planned stop rather
  than sending the ordinary shutdown.
- A successor the daemon starts itself gets the same port and the same key, the key through its standard input
  (`--key-stdin`), never its arguments or environment. A successor that fails to start leaves the sessions to the
  holders and the next start, and the old daemon exits with status 1. A successor started by systemd makes a new key
  (blocked on key delivery, above).
- The successor is launched from the installation path, not the resolved image: argv[0] kept unresolved (absolute as
  is, relative against the start-up directory, a bare name through `PATH`), used only when it leads to the same file
  as the running image at start, else the running image's path. `start` launches the background daemon from that
  path too. A repointed symlink or a file replaced at the same path restarts the new build; an entry point that has
  gone fails the restart as above. No restart-executable option was added (own choice: argv[0] identifies the
  entry point for every launcher there is). The running image, resolved once at start
  (`platform::proc::running_image`), stays the build identity and the holders' copy source.
- Leaving the restart to a service manager needs evidence: `SYSTEMD_EXEC_PID` equal to the daemon's pid with a
  well-formed `INVOCATION_ID`, or the explicit `--service-managed` option the maintained unit passes. An inherited
  `INVOCATION_ID` alone no longer counts. The policy is resolved once at start and the refusal and the successor
  choice read the same value. A custom unit passing `--service-managed` must restart on status 75.
- The saved screen is the first paint with the whole scrollback, the program status records as OSC 7501 reports, the
  colour-scheme-updates mode, the title stack, and the bytes of an escape sequence (or UTF-8 character) begun and not
  ended (at most 64 KiB, else the approximate path), plus bytes that re-arm the character a REP would repeat. The save
  marks it inexact when it cannot reproduce the state: an alternate screen over a non-blank normal screen after the
  width changed while it was shown (until it is entered again), a character set other than ASCII (on either screen's
  cursor), tab stops set or cleared since the last reset, a changed palette entry, a cursor outside the scroll region in
  origin mode, a cursor the alternate screen keeps saved while the normal one is shown, a REP that cannot be re-armed. A
  normal screen behind an alternate one is written before the alternate screen is entered: rows, scrollback, cursor and
  pen, with the saved cursor that entering sets; reading it leaves the live terminal unchanged. A normal screen known to
  be blank is not read, and is exact as before. The predecessor's inexact bit is kept. The file carries a format
  version; an unsupported one takes the approximate path.
- A saved screen is used when its token matches the one the holder reports for the terminal's tail, the tail was not cut
  since the handover's mark, and the size matches; the holder's last parting is not consulted. The screens file is no
  longer deleted at start: entries are consumed at their terminals' committed adoption (once per holder, the remaining
  entries rewritten privately), before any replay answer is written, so a crash after that cannot replay answered
  queries; a consumption that cannot be made durable drops the exact path. Entries of pending holders stay for a late
  adoption and are carried over by the next planned stop, which always replaces the file. An entry of a gone holder
  becomes its interrupted session's preview through the same restore path (`SavedScreen::preview`) and is consumed only
  after that preview's write and the holder's disposition are committed (`select_restart_screen`, then
  `expire_restart_screens`); its other entries (shell terminals, sessions needing no preview) are expired then. A
  preview whose write fails is not retried (own choice: a retry needs a marker telling it from the session's own later
  preview, and kept held-terminal records): the session keeps no new preview, as after a failed write at an ordinary
  stop. A screen of a size no screen can have is rendered at 24×80 rather than restored. Malformed files are deleted.
  The API, in order: `load_restart_screens`; `select_exact_screen` / `select_restart_screen` (take nothing);
  `commit_restart_screens` / `expire_restart_screens` (one private rewrite per call); `pending_restart_screens` and
  `restart_screens::replace` for the planned stop.
- After the mark the stopping daemon still writes, until it parts, the answers to queries in output read before the mark
  and the clients' replies to such queries, and nothing else. A terminal is saved only when its reader has ended, its
  answer writer has drained with every write acknowledged by the holder, no reply a client owes to a query left to it is
  outstanding, and no answer or reply was lost since the handover began (all within one 3 s deadline); otherwise it
  takes the approximate path (own choice: a lost answer may leave the program waiting, which the approximate path's
  redraw handles better than a screen claimed exact). An answer to an earlier pre-mark query may reach the program after
  the holder's offline answer to a later one: each is answered once, not necessarily in order.
- A reconnect of the old run after a mark lists the handed-over terminals in its hello (`handed_over`); the holder
  keeps them unlinked, answering offline, with their output in the tail for the successor.
- A replayed tail answers a query only when the output it ends in is at most 1 s old (by the ages the holder keeps
  since the mark) and it is not a kind the holder answers offline; a query not answered is dropped. A replay answers
  nothing when the welcome lists an offline answer this build does not know. Neither applies after a crash.
- A planned stop raises an operation barrier first: from then on it refuses new terminal and lifecycle changes with
  `daemon_shutting_down` at their shared entry points, and it waits up to 5 s for those already admitted, including the
  process ends and branch removals they started (own choice of the bound), whichever transport calls them: shell create
  and close, archiving and deleting sessions, removing a project or a console, the background archive cleanup, account,
  model and sharing changes, handoff, process stops, interrupts, trust confirmation, launches, a `done` report (refused
  whole, so nothing is delivered twice). Read-only requests stay. A step that begins after the wait has run out is
  refused. An operation admitted before the barrier finishes its own inner steps. Client resizes, activation and focus
  updates are held from before the mark; the successor takes the clients' size on reconnect. The copy of
  `daemon_shutting_down` is now operation-neutral.
- The outbox stops writing at the barrier; the seal refuses new messages on every push path and waits up to 3 s for
  drains under way to put back what the handover refused; messages accepted until the queues are sealed are saved after
  the handover to `restart-outbox.json` (private, ordered, stable ids) and queued again by the successor for the
  sessions it adopts with the same launch; new deliveries after the seal are refused as `daemon_shutting_down`. A
  message cut off part-way is never retried whole. The successor reads and deletes the file at start (own choice), so
  its own crash before flushing loses them, as for any queued message; holders linked after adoption ends do not get
  theirs.
- A planned stop leaves the process ledger as a crash would (the successor's reconcile handles it), leaves pending
  holders running without ending them (their terminals are adopted approximately), and with no holders installed is
  an ordinary stop (a planned restart still starts the successor). After a planned parting no terminal of that
  holder reads as exited in the old daemon, and its exit watchers stop once the handover begins.
- The holder's tail keeps the history before a handover's mark, and the welcome says where the mark falls in it
  (`since`): the exact path replays only the output since the mark, while the approximate path still rebuilds from
  the whole tail. A handover the daemon gives up on part-way keeps the screens of the terminals already marked and
  drained; a reader that has not reached its mark is never saved.
- A holder whose tail changed between its welcome and the tail's dispatch closes the link rather than hand over a tail
  that does not match what the welcome said. The tail's front is pinned by one flag between a welcome and its attach,
  so two daemons linking at once may make the later one retry.
- A holder linked late while the terminals are handed over is not adopted by the stopping daemon, and the handover
  marks only the holders of the terminals it listed: a late holder's terminals are kept untouched for the next
  daemon. The pending holders whose screens are carried over are read under the same lock that starts the stop.
- A saved screen of a size the daemon does not support takes the approximate path.
- `request_shutdown` is kept in a watch, so a stop asked before the server waits is not lost; an ordinary stop asked
  after a planned handover has begun escalates it (see "Stop escalation hook" below).
- Stated limits: a lost link while a handover request is in flight can leave a terminal marked by the holder but not
  listed as handed over, which then counts as not handed over and is recovered approximately; a query left to a
  client that never answers it costs exactness until it is stale (5 s, past the 3 s deadline); a message a drain
  puts back after the seal's 3 s wait is only logged; writes a session schedules itself after a drain (the
  submit-key retry, restoring a stashed draft) are outside the barrier, so a stashed draft may not be restored
  across a handover.

**Stop escalation hook.** Written for milestone 04's system end during an update restart (an ordinary stop with
frozen previews, never a continuation of the restart); not compiled or run either.
- Trigger, with no new request: anything that asks for an ordinary stop while a planned stop is asked for or under
  way. The stop asked for only moves from none to planned or ordinary, or from planned to ordinary
  (`state/stop.rs`: `AppState::ordinary_stop_asked`, `planned_stop_asked`, `await_ordinary_stop`). What reaches a
  daemon in a planned stop: the application's `quit` line on standard input, which now asks for the ordinary stop at
  once while a planned stop is asked for or under way (the one transport that works on every host; a quit read just
  after a gone application's 1 s wait still turns the planned stop it asked for ordinary); a termination signal
  (Unix; `main` keeps watching it through every planned stop, the one with no holders included); a `shutdown` request
  on a connection still open. The listener closes once `main` has begun any stop, so a `POST /shutdown` on a new
  connection does not arrive.
- One owner: `main`'s stop sequence. `coordinator::planned_stop::hand_over` returns `Handover::Parted` or
  `Handover::Escalated` and stops where it stands: during its barrier wait (the wait is cut; the barrier stays raised,
  so new admissions are still refused with the handover's wording, but it is never marked passed, so an operation
  admitted before still passes `still_covered` and may still start its process, as in an ordinary stop, which ends it
  once registered), before any terminal is marked (nothing handed over), or after the marks (readers, answers and
  client replies no longer waited for, no further screen saved, the screens saved by then still written as the next
  start's exact screens or previews, the holders not parted from). The marking itself (`Holders::hand_over`, up to
  5 s, every holder at once) and a parting already begun (`part_planned`, up to 3 s, every holder at once) are not
  cut. An escalation seen after the parting still counts until `main` is on its way out; then no successor is
  started, but a `restart-outbox.json` saved meanwhile stays (no adopted session takes it at the next start).
- Cleanup (`main`'s `stop_all_processes_after_planned`): every session's preview is kept first (a terminal still the
  daemon's frozen and written at once, a handed-over one waited for up to its mark), then the sessions are settled and
  marked interrupted and the ledger attributes what the agents started. Then, together: terminals still the daemon's
  are ended as an ordinary stop ends them, and the holders left to the next daemon (`Holders::left_to_successor`:
  those that handed terminals over, were parted from, or were linked to during the planned stop and left to it
  untouched, `HeldRelays::leave_for_successor`) are ended by their verified process (`Holders::end_handed_over`),
  never through their terminals; shell terminals of those holders are forgotten without a word to them
  (`terminal::shell::close_all_ending`). Last the rest go as in an ordinary stop (`Holders::shutdown`,
  `unlinked::interrupt_at_stop`). A pending holder linked, or left to the next daemon, once the escalation has begun
  is ended, not left running. What an exit watcher would do after the end is not needed for a daemon on its way out:
  the sessions are interrupted already and the next start clears the run directory.
- Unchanged: an ordinary stop with no planned stop, and a planned stop with no escalation (it now only remembers the
  holders that handed terminals over, were parted from or were left to the next daemon, and parts from them all at
  once).
- Desktop side (milestone 04's review, `hold/desktop-update-review`, not compiled either): a system end writes the
  quit record and the `quit` line whatever the update restart noted (`note_session_end`), signals a daemon already in
  a planned stop at once on Unix instead of sending `POST /shutdown`, and keeps the restart worker from installing or
  relaunching once it has begun (`exit.rs`, `commit_final_action`).
- TODO, needs a holder-protocol capability that does not exist: a holder ends every program on its own end with a
  kill, not a polite request, and the output since a terminal's mark is lost with it, so a handed-over terminal's
  preview is its checkpoint (the screen up to the mark). Missing: a request, over the predecessor's link or a link
  opened again after a planned parting (whose hello lists `handed_over`), asking the holder to end marked terminals
  politely and hand back each tail since the mark, as a refused holder's ending hands back `Adopted` tails; the
  preview would then be the checkpoint plus that tail through `coordinator::unlinked`'s checkpoint restore helper.
- TODO: sessions claimed for pending holders get no new preview, as in an ordinary stop; their carried screens could
  become previews through `select_restart_screen`, `SavedScreen::preview` and `keep_saved_output`, consumed with
  `expire_restart_screens` once the preview is written. An escalation the system's deadline cuts short before the
  holders are ended no longer leaves them to be adopted: the system end leaves milestone 04's quit record, which the
  next start scopes to every holder the registry lists and ends. The refusal text of the raised barrier still speaks of a
  handover after an escalation.

## Notes for the developer

- **Reusable capabilities**:
  - The frozen-preview snapshot taken before a controlled stop.
  - The holders from milestone 02.
- **Development notes**: builds on daemon-side answering of terminal queries, which is planned separately and has to
  exist first. The `restart` command joins the daemon's command-line start, stop and status commands, which
  have to exist first; both requests and the planned stop do not wait on them. The systemd case needs the daemon
  running as a systemd user service, which does not exist yet. A desktop update is milestone 04.
- **Reference docs**: `docs/product/application-lifecycle.md` ("The daemon on its own"),
  `docs/product/in-app-updates.md`.
