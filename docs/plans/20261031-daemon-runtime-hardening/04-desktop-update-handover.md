# 04 Desktop updates keep sessions running

> Goal: restarting the desktop application to apply an update hands every session over to the new version's daemon
> instead of interrupting it, on macOS and Windows; an application crash leaves them running too.
> Completion criteria: install an update and restart into it while sessions are working, with a shell terminal open.
> - Every session and shell terminal keeps running in the new version, with its exact screen. Nothing becomes
>   interrupted.
> - The adopted sessions' hooks, MCP tools and git gate work with the new daemon.
> - New sessions run under a holder of the new build. The old holder exits once its last terminal ends.
> - Kill the application outright while sessions work. On the next launch every session is still running.
> - A plain quit, with no update, still ends every session as today, and so does the quit of a window that stopped
>   responding.
> - A system shutdown asks for no confirmation, and on the next start every session is interrupted with its last
>   screen as its preview.
> - The update restart asks for confirmation only when some session would be interrupted, and names those sessions.

## Technical design

- [ ] The update restart sends milestone 03's planned-stop request rather than the ordinary shutdown. On macOS the
      application bundle has already been replaced, so the old daemon makes its planned stop and the new
      application's daemon adopts the holders. On Windows the installer runs only after that planned stop.
- [x] The restart's escalation repeats the planned-stop request rather than the ordinary shutdown, and waits long
      enough for the screens to be saved. A daemon killed in the end leaves its sessions to crash recovery.
- [x] Every quit tells the daemon it is quitting before the application exits, without waiting for an answer, the
      quits that do not wait for the daemon included. A daemon that finds the application gone after that makes an
      ordinary stop.
- [x] The daemon finding the application gone makes the planned stop rather than an ordinary stop, so an
      application crash leaves the sessions to the next launch.
- [ ] A system shutdown, restart or logout is told apart from a quit: it asks for no confirmation, and the daemon
      makes its ordinary stop, freezing each preview, before the system goes on. On Windows the application passes
      the system's end-session notice on to the daemon, as far as the time the system allows.
- [x] Where the planned stop has saved a screen and the holders then die with the system, the session interrupted at
      the next start shows that screen as its preview.
- [x] The update restart's confirmation appears only when some session would be interrupted: a holder outside the
      compatible range. It names those sessions.
- [ ] The new application starts its own daemon as today. That daemon adopts the holders through milestone 02's
      adoption, including a holder of an older build in the compatible range.
- [ ] The new build's database migration runs while sessions are adopted. Records of adopted sessions stay valid
      across it.
- [ ] The adopted sessions' hooks, MCP bridge and git gate run from their build's copy and keep working through
      their holder's relay (milestone 02), while the installation is replaced.

## Implementation plan

- [ ] Route the update restart and the application's loss through the planned stop on macOS, then on Windows.
- [ ] Change the update restart's confirmation, the quit paths and the system shutdown path.

## Notes for the developer

- **Reusable capabilities**: the update restart flow, with its confirmed quit and its daemon stop; the daemon
  instance lock; the per-user port file in the data directory.
- **Development notes**:
  - Today an update restart is a confirmed quit. The application asks the daemon to shut down, follows up after
    2 seconds with a direct request or a termination signal, and kills it after 5 seconds. On macOS the daemon is
    never started again before the application restarts.
  - Milestone 02 changes the start-up rules for adopted sessions.
  - Logout, restart and shutdown arrive on macOS through the same application delegate selector as a quit. On
    Windows the system ends the daemon at sign-out or shutdown with no notice of its own.
  - The application-lifecycle and in-app-update product docs change with this milestone.
- **Reference docs**: `docs/product/in-app-updates.md` ("Downloading and installing"),
  `docs/product/application-lifecycle.md` ("Starting up", "Quitting"), `docs/architecture.md` ("Why the daemon is
  split out").

## Mid-task handoff

**State.** Every technical-design item and both implementation-plan steps are written, first on `hold/desktop-update`,
then revised by the independent review on `hold/desktop-update-review` (decisions 2 to 5 changed, defects D1 to D9
fixed, the desktop side of milestone 03's stop escalation hook wired). The Rust
compiles and passes clippy on macOS and the Windows cross-check, and the UI side passes; a first full Rust test run
proved the ticked boxes, and the Rust tests remain to be rerun after the fixes that followed it. The open boxes wait on
Windows runs, manual runs or failed tests. 04 reaches 03's saved screens only in
`coordinator::unlinked::keep_screens_as_previews`, which selects a gone holder's screen
(`AppState::select_restart_screen`), renders it through the shared restore path (`SavedScreen::preview`), and consumes
it (`expire_restart_screens`) only once its preview is written and the holder's disposition is committed; its tests
build screens with `SavedScreen::of_bytes`.

**What was built.**
- Desktop shell (`sidecar/{mod.rs,stop.rs,quit_note.rs}`, `exit.rs`, `lifecycle/macos.rs`): `stop_daemon` takes the
  stop's kind. The update restart (and on Windows the installer it starts) sends `POST /planned-stop`, repeats it once
  if it could not be sent, never signals, and kills only after a 30 s grace; a killed daemon leaves its sessions to
  crash recovery. The daemon's planned stop is nominally about 31.5 s at worst after 03's barriers (barrier 5, hook
  settle 0.5, a link reopened and settled 15, the handover's answer 5, the readers 3, parting from every holder at once
  3), a sum of deadlines rather than a measured maximum; it begins at the UI's request, so the kill comes 32 to about
  37 s after it (more where recoveries are waited for first): a thin margin, past which the sessions are left to crash
  recovery.
  Every plain quit first leaves the quit record `~/.octoboard/application-quit` (`{"version":1}`, written atomically),
  when the application had a daemon of its own that may hold holders (one was up, or one was killed while still
  running; not one that exited on its own before its startup line, as with the instance lock held by another daemon),
  and then writes the line `quit` to the daemon's standard input once (`ChildSlot`, `QuitNote`), also to a daemon a
  recovery or the first start stores after the quit; a successful write does not stand in for the record. The UI calls
  the native `begin_quit` before its `shutdown` request, so the record exists before the daemon starts stopping. The
  update restart marks the note skipped. A system end (macOS logout, restart or shutdown through the delegate within
  5 s; Windows `RunEvent::Exit` with no exit let through, within 4 s) never asks and is an ordinary stop: it writes the
  record and the `quit` line even over a skipped note (`note_session_end`), which escalates a planned stop already
  under way; it then sends `POST /shutdown`, after the termination signal (Unix) when a planned stop is under way, and
  kills at the end of the budget. The restart worker sends no further planned-stop request once the note is written,
  and the daemon makes a planned stop asked for after the quit line ordinary (`state/stop.rs`). Once a session end
  has begun, the restart's and the installing quit's workers neither install nor relaunch (`commit_final_action`,
  serialized against `begin_session_end`). The macOS Dock's Quit during a restart or installing quit is turned back.
- Daemon: with `--parent-pid` a thread reads standard input (`state/application_note.rs`); the application gone after
  its `quit` line makes an ordinary stop, otherwise a planned stop. Every start, with or without `--parent-pid`,
  reconciles the quit record under the instance lock before adoption (`holder::quit_targets`): an unscoped record is
  scoped to the holders the registry lists (holder id and verified identity) and written back; a target leaves the
  record only once it is confirmed ended or gone, and the file goes with the last one. Targeted holders are refused
  and never become current, so new work gets a fresh holder; one that runs but cannot be linked to is ended by its
  verified process, at start and on each retry; one whose identity cannot be verified is never signalled and stays a
  target. A refused holder reports itself gone only when every terminal's end was confirmed by the holder
  (`HeldTerminal::end` returns it); otherwise it is ended by its process and judged by whether it is gone, and the
  sessions stay claimed meanwhile. The start waits up to about 4 s for these endings before the startup line; the wait
  bounds only the start, not the record. Previews are rebuilt from the holders' tails, or the saved screen plus the
  output since the handover mark, or the saved screen alone. `handover_check { holder_floor, target_version }`
  (`coordinator/handover_check.rs`) answers `known` (both ends readable), the running sessions and shell terminals a
  planned stop followed by a daemon adopting holders from the floor up to the target version would not carry over, and
  the shell terminals of pending holders not carried over (`unreached_shell_terminals`, busy state unknown).
- Release: the update manifest carries `holder_floor` (the release commit's `FLOOR`); the shell passes it, with the
  update's version, to the UI.
- UI: the update restart always asks `handover_check` (2 s at most) and confirms when any listed session (idle,
  waiting, working or pending alike), a listed busy shell terminal or an unreached shell terminal would be
  interrupted, a listed shell the window does not hold counting as busy; a known empty answer restarts without asking.
  With no answer or an unknown one it warns for every session with a live process and every busy shell (and the
  unreached shells an unknown answer listed), in uncertain copy. `lifecycle/ExitConfirmDialog.tsx` names the
  listed sessions (four and a plural "and N more" row past five, ids the window cannot name counted in it, or a message
  stating the count when none can be named), says
  terminals that cannot be reached may end too, and says everything else keeps running only for a known answer with
  no idle listed shell and no unreached one. Restart now is pending meanwhile. It sends `planned_stop`.

**To pick it up.**
1. Compile both crates (`cargo check --all-targets`, clippy `-D warnings`, `pnpm check:windows`). Riskiest spots:
   desktop `sidecar/quit_note.rs` (`announce`'s borrows of the child slot, the record write under the slot lock),
   `sidecar/stop.rs` (`const` asserts, `session_end_budget`, `escalate_planned_session_end`), `sidecar/mod.rs`
   (`may_hold_holders`, the `Exited` path taking the child out), `exit.rs` (`commit_final_action`, cfg'd
   `SESSION_END_WAIT`); daemon `holder/quit_targets.rs`, `holder/client/mod.rs` (the
   target policy in `adopt_entry`, `take_over`, `link_pending`, the moves in `adopt_existing`'s ending thread, `refuse`,
   `pending_holders`), `holder/client/held.rs` (`end` returning whether the end is confirmed, `ask_reap`), `main.rs`
   (the reconcile, `wait_for_quit_endings`), `coordinator/handover_check.rs`.
2. Tests: daemon `holder::quit_targets`, `holder::client` (incl. `holder::client::adoption_tests`; the quit-target
   tests now depend on a real reap answer from the test holder), `state::application_note`,
   `tests::the_parent_watch_asks_for_the_stop_with_a_dead_log`, `coordinator::handover_check`, `coordinator::adopt`,
   `coordinator::unlinked`, `holder::`, `state::`, `protocol::`, `bindings` (the TypeScript `handover_check` entries
   were written by hand; diff the regenerated file), `server::`, `cli::`, `layering_tests`; desktop
   `sidecar::quit_note`, `sidecar::stop`, `exit::`, `updater::`; `node --test apps/desktop/scripts/*.test.mjs`; UI
   `ExitConfirmDialog`, `useAppExit`, `updateStore`, `UpdateNotice`, `UpdatesSetting`, `platform/tauri`, `catalog`,
   `ErrorBoundary`, `StartupScreen`, `fixtureDaemon` (its settings case fails on the base too) and the gallery quick
   check for `updates-restart-*`.
3. Not covered by unit tests (in-process test holders share the test's pid, which `end_process` refuses): ending a
   targeted holder by its process, an unconfirmed terminal end, and `link_pending` ending a target; exercise them by
   hand.
4. By hand on macOS with two built versions: install the newer over the older while an agent session works and a
   shell terminal runs, restart from the update notice: no confirmation, every session and shell running in the new
   version with its exact screen, hooks, MCP tools and git gate working, new sessions under a holder of the new build,
   the old holder exiting with its last terminal. An idle session whose holder is faked out of range (floor raised in
   the manifest): the restart names it. Kill the application (`kill -9`): the next launch finds every session running.
   A plain quit, a quit through the stopped-responding escape hatch, and a quit while the daemon is down or still
   starting interrupt every session; kill the daemon right after a quit's note and relaunch: the sessions are still
   ended; start `octoboardd` on its own after a quit: it ends the record's holders too; a holder that cannot be linked
   to after a quit is ended by its process. Log out with sessions working, also during an update restart's planned
   stop: no prompt, every session interrupted at the next start with its last screen as preview, no relaunch. Then on
   the Windows VM: sign-out and shutdown within the time Windows allows, also overlapping an update restart (no
   installer started after the session end began); the installer running after the planned stop with holders still
   running from the build copy.

**Decisions made without the user (asleep), with reasons.**
- The quit note travels over the daemon's standard input: a pipe write lands even when the daemon does not answer and
  costs no wait. It is a notice only: the durable intent is the quit record, written for every plain quit and every
  system end before the note, and kept until each targeted holder is confirmed ended. Child spawns keep explicit
  standard input so none competes for this pipe.
- The record is unscoped when the desktop writes it (`{"version":1}`; an empty file reads the same) and scoped by the
  first daemon start that reads it, before any holder of its own exists; every start reconciles it, so an unscoped
  record never meets holders started after the quit. Stated limits: a holder launched by the previous daemon that had
  not yet written its registry file is not a target; a record that cannot be rewritten stays unscoped and is scoped
  again at the next start; a quit written while a starting daemon scopes the record can be overwritten by that scoping;
  with holders that could not be set up the record is left as it is. A quit leaves no record when the application never
  had a daemon that may hold holders (its daemon exited on its own before its startup line), so a desktop whose daemon
  lost the instance lock to another one does not end that daemon's sessions at its next start.
- A macOS logout stops the daemon by blocking the delegate's answer (within 5 s) rather than `TerminateLater`. A system
  end is an ordinary stop also during an update restart: it escalates the planned stop (03's hook, main the single
  cleanup owner) instead of waiting for it, and wins over the installer and the relaunch; a planned stop asked for
  after the quit line is made ordinary, so a late planned-stop request cannot undo the session end.
- The new build's holder floor and version reach the old application through the update manifest; compatibility is the
  inclusive range from the floor to the target version, and a missing or unreadable end means unknown, which warns
  conservatively in uncertain copy. A pending holder is judged by its registry file's version; its shell terminals are
  listed from the held-terminal records, their busy state unknown.
- An update restart confirms for every session it would interrupt, idle ones included, while a plain quit keeps asking
  only about working or waiting sessions and busy shells. An affected shell at its prompt does not ask alone, but keeps
  the dialog from saying everything else keeps running.
- The planned-stop grace stays 30 s, as an escalation budget: with the waits before it the kill comes 32 to about 37 s
  after the planned stop begins, against a nominal 31.5 s, so exact screens are not guaranteed at the extreme.
- No migration change was needed: the schema migration runs at `Store::open`, before adoption, and its steps only add;
  the quit record is a versioned private file, not a table.
