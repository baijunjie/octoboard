# 04 Desktop updates keep sessions running

> Platform scope: macOS and Windows.
>
> Goal: restarting the desktop application to apply an update hands every session over to the new version's daemon
> instead of interrupting it, on macOS and Windows; an application crash leaves them running too.
> Completion criteria: install an update and restart into it while sessions are working, with a shell terminal open.
> - Every session and shell terminal keeps running in the new version, with its exact screen. Nothing becomes
>   interrupted. The exact screen has one stated exception: a screen whose width changed while a program
>   showed the alternate screen comes back marked inexact.
> - The adopted sessions' hooks, MCP tools and git gate work with the new daemon.
> - New sessions run under a holder of the new build. The old holder exits once its last terminal ends.
> - Kill the application outright while sessions work. On the next launch every session is still running.
> - A plain quit, with no update, still ends every session as today, and so does the quit of a window that stopped
>   responding.
> - A system shutdown asks for no confirmation, and on the next start every session is interrupted with its last
>   screen as its preview.
> - The update restart asks for confirmation only when some session would be interrupted, and names those sessions.

## Technical design

- The update restart sends the planned-stop request rather than the ordinary shutdown:
  - [x] On macOS the application bundle has already been replaced, so the old daemon makes its planned stop and the
        new application's daemon adopts the holders.
  - [x] On Windows the installer runs only after that planned stop. Real NSIS process-start events place the installer
        after the daemon's successful planned stop. Evidence: `/tmp/windows-desktop-evidence/beta42-installer-events.json`
        and `beta42-desktop.log`.
- [x] The restart's escalation repeats the planned-stop request rather than the ordinary shutdown, and waits long
      enough for the screens to be saved. A daemon killed in the end leaves its sessions to crash recovery.
- [x] Every quit tells the daemon it is quitting before the application exits, without waiting for an answer, the
      quits that do not wait for the daemon included. A daemon that finds the application gone after that makes an
      ordinary stop.
  - [x] On macOS, a quit while the sidecar is still starting leaves a quit record that ends every leftover session
        at the next start. Evidence: `/tmp/octoboard-remaining-20261110-7fd0/starting-quit.json`.
- [x] The daemon finding the application gone makes the planned stop rather than an ordinary stop, so an
      application crash leaves the sessions to the next launch.
- [ ] A system shutdown, restart or logout is told apart from a quit: it asks for no confirmation, and the daemon
      makes its ordinary stop, freezing each preview, before the system goes on. On Windows the application passes
      the system's end-session notice on to the daemon, as far as the time the system allows.
  - [x] Windows: real sign-out and shutdown completed the daemon's ordinary stop with exit 0 and retained previews;
        an update restart overlapping sign-out kept the preview and started no installer or automatic relaunch.
        Evidence: `/tmp/windows-desktop-evidence/signout-fixed-preview.json`, `system-shutdown-samples.jsonl`,
        `system-shutdown-preview.json` and `overlap44-installer-and-relaunch-check.json`.
  - [ ] macOS: real logout, shutdown and their overlap with the update restart (see the remaining list).
- [x] Where the planned stop has saved a screen and the holders then die with the system, the session interrupted at
      the next start shows that screen as its preview.
- [x] The update restart's confirmation appears only when some session would be interrupted: a holder outside the
      compatible range. It names those sessions.
- [x] The new application starts its own daemon as today. That daemon adopts the holders through milestone 02's
      adoption, including a holder of an older build in the compatible range.
- [x] The new build's database migration runs at `Store::open`, before holders are adopted, and an additive
      migration leaves their session and held-terminal records valid. Verified on macOS with a signed localhost
      beta.3→beta.6 update built from main and a scratch v3→v4 empty-table step; all held rows and program PIDs
      survived, with Codex queue delivery and shell input working.
      Evidence: `/tmp/octoboard-remaining-20261110-7fd0/REPORT.md` (cross-schema update records).
- [x] The adopted sessions' hooks, MCP bridge and git gate run from their build's copy and keep working through
      their holder's relay (milestone 02), while the installation is replaced (verified on macOS, and on Windows,
      where the installer met no file in use: `/tmp/windows-desktop-evidence/REPORT.md`).

## Implementation plan

- [x] Route the update restart and the application's loss through the planned stop on macOS.
- [x] The same on Windows. Evidence: `/tmp/windows-desktop-evidence/REPORT.md` (signed NSIS updates and application kill).
- [x] Change the update restart's confirmation and the quit paths.
- [ ] Change the system shutdown path.

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

## Design decisions

Made without the user, with reasons.

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
  end is an ordinary stop also during an update restart: it escalates the planned stop (the stop-escalation hook,
  main the single cleanup owner) instead of waiting for it, and wins over the installer and the relaunch; a planned stop asked for
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
- No migration change was needed: the schema migration runs at `Store::open`, before adoption, and its steps add
  columns and tables and change the settings table, rewriting no session or held-terminal row; the quit record is a
  versioned private file, not a table.

### What was built

- Desktop shell (`sidecar/{mod.rs,stop.rs,quit_note.rs}`, `exit.rs`, `lifecycle/macos.rs`): `stop_daemon` takes the
  stop's kind. The update restart (and on Windows the installer it starts) sends `POST /planned-stop`, repeats it once
  if it could not be sent, never signals, and kills only after a 30 s grace; a killed daemon leaves its sessions to
  crash recovery. The daemon's planned stop is nominally about 31.5 s at worst after its barriers (barrier 5, hook
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

## Mid-task handoff

**State.** Everything is implemented. On 2026-11-10, on macOS, with real signed localhost updates from beta.3 to
beta.4 and from beta.4 to beta.5 installed through the updater, every session and the shell terminal kept running with
an unmarked screen, the adopted sessions' hooks, MCP tools and git gate worked, a new shell ran under the beta.5 holder
and the beta.3 holder exited after its last terminal; the raised-floor restart named the sessions it would interrupt
and Cancel kept them, while a compatible restart asked nothing. Killing the application left every session running at
the next launch; a plain quit and the stopped-responding escape hatch ended every session; a quit while the daemon was
down, and a quit record with a holder that refused links (whose terminals' ends the holder therefore could not
confirm), had the next start end the leftover holder by its process.
Evidence, on the verification Mac: `/tmp/octoboard-gui-20261110-67e2/REPORT.md` (with
`/tmp/octoboard-manual-20261110-67e2/REPORT.md`), and `/tmp/windows-verification-evidence/REPORT.md` for the Windows
VM, where only the daemon's planned stop and start ran. The scripted suites (`holder::quit_targets`,
`holder::client`, `state::application_note`, `coordinator::handover_check`, `coordinator::unlinked`, desktop
`sidecar::`, `exit::`, `updater::`, UI `ExitConfirmDialog`, `useAppExit`, `updateStore`) run at push.

On 2026-11-10, main `035746de6` also passed macOS startup-time quit and a real signed cross-schema update.
The scratch next build added only an empty-table v3→v4 migration and verification metadata/version overrides; it
was never committed. Evidence: `/tmp/octoboard-remaining-20261110-7fd0/REPORT.md`.

**Windows desktop follow-up, 2026-11-10.** A temporary standard user's interactive logon verified application-kill
survival and recovery, a plain quit, the native repeated-quit escape hatch with an actual stalled JavaScript event
loop, and two signed local-feed NSIS updates over working Codex, Claude and shell terminals. The second update
preserved previously exact screens without an inexact mark; process-start events place the installer after the
successful planned stop, and an independent installation of the same payload matches all seven shipped files.
Evidence: `/tmp/windows-desktop-evidence/REPORT.md`, `app-kill-offline-survival.json`, `plain-quit-relaunch.json`,
`unresponsive-quit-relaunch.json`, `beta42-after-session.json`, `beta42-installer-events.json` and `reference42-compare.json`.

A real sign-out exposed the daemon being terminated before the shell's shutdown request reached it. The desktop
now sets its shutdown order before the daemon's default order; the repeated real sign-out ended all observed
processes within 2213 ms, with daemon exit 0 and the interrupted session's last preview retained. The exact Windows
API test passed. Evidence: `/tmp/windows-desktop-evidence/signout-fixed-system-result.json`, `signout-fixed-desktop.log`,
`signout-fixed-preview.json` and `test-desktop-priority.log`. Ordinary quits also exposed a holder left with unreleased
terminal records; its generation-checked ordinary-parting cleanup passed three precise Windows regressions and a
real ordinary quit, leaving no holder or agent behind. Evidence: `fixed-holder-quit-processes-after.json` and
`fixed-holder-quit-preview.json` under the same evidence root.

Real Windows shutdown also passed: the observed product processes ended within 2452 ms, daemon exit 0, and after
booting and signing in again the interrupted session retained its 1678-byte last preview. For the update/sign-out
overlap, a proxy delayed only the holder's handover request; the real system notification reached the app while its
planned stop was under way. The four-second escalation ended the daemon at its deadline, preserving the preview;
no installer or automatic application restart started before the next manual launch. The downloaded beta.44 update
was left for the next start. Evidence: `/tmp/windows-desktop-evidence/system-shutdown-desktop.log`,
`system-shutdown-preview.json`, `overlap44-desktop.log`, `update-signout-overlap-preview.json` and
`overlap44-installer-and-relaunch-check.json`. The normal sign-out and shutdown completed normally; the deliberately
delayed overlap exercised the bounded kill fallback. No sign-in blocker remains.

**Remaining, each a real run:**
1. macOS: log out and separately shut down with sessions working, also during an update restart's planned stop: no
   confirmation, no relaunch, and at the next start every session interrupted with its last screen as its preview.
   This was not run on the verification machine, since it would end that machine's own session. All Windows items
   above are verified; final fixture cleanup and VM state are recorded in `/tmp/windows-desktop-evidence/REPORT.md`.
