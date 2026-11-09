# 02 Process holders

> Goal: agent processes and shell terminals survive the daemon going down, and keep working with the daemon that
> starts next.
> Completion criteria:
> - Kill the daemon outright and start it again. Every running session stays running and attachable, and its screen
>   is rebuilt, repainted and marked inexact.
> - The adopted sessions' hooks, MCP tools and git gate work with the new daemon, and a hook report made while no
>   daemon was running reaches the new one.
> - A holder of a build outside the compatible range is not adopted: it is ended with its programs, and its sessions
>   are interrupted.
> - A holder of another build within the range receives no new terminals and exits once its last terminal ends.
> - A git gate call made while no daemon runs fails once its deadline passes.
> - A terminal the holder runs but the daemon has no record of is ended.
> - A session no holder runs is interrupted.
> - A holder that runs but refuses connections or keeps dropping the link is not ended: its sessions stay running and
>   claimed, shown as pending, and are taken over once it accepts a link again.
> - A custom shell name is kept with the terminal identity.
> - After a Linux host shutdown and the next start, every session is interrupted, and its preview is its last screen
>   from before the shutdown.

## Technical design

- [x] A holder process, a mode of the daemon binary:
  - It owns the PTYs and is the parent of every program it starts.
  - It reads output and forwards it, keeps a raw tail per terminal, writes input, resizes and signals.
  - It keeps no screen. A bounded streaming recogniser on the output answers, while no daemon is linked, the fixed
    queries that need no screen with the daemon's own answers (the primary device attributes); which side answers a
    query is decided in output order with the link's attach, and the welcome lists what the holder answers.
  - Each program's input is written separately, so a program that stops reading blocks only itself.
- [x] A local socket per holder and a small framed protocol between daemon and holder:
  - From the daemon: spawn, input, resize, signal and release.
  - From the holder: spawned, spawn failed, output, exited and its tails.
  - A program's exit is reported only after its last output has been forwarded.
- [x] The handshake on connecting:
  - The holder answers with an instance id, its build, how it last parted from a daemon (handover or crash) and the
    terminals it runs.
  - Then it sends one tail per terminal.
  - A newer daemon connection takes the holder over from an older one.
- [ ] Adoption on start:
  - Terminals are matched with their sessions and shell terminals.
  - Terminals with no record are ended.
  - Sessions with no terminal are interrupted.
  - A shell terminal comes back with the name the user gave it, which the daemon stores by the terminal's id.
- [ ] A transport failure never ends a holder. An exited holder (no registry file, or its process gone) and one
      confirmed incompatible (a version out of range, a welcome that cannot be read) take the existing end path. One
      that cannot be connected to yet (refused, timed out, short repeated drops) is pending: its process and registry
      file stay, the link is tried again with a capped growing pause while its verified process runs, requests fail at
      their deadlines, and its sessions stay claimed (not interrupted, resumed or cleaned up after) and are shown as
      pending on the wire. The daemon records which holder runs each launch, so a start knows exactly which sessions a
      pending holder runs; a session no record places stays claimed, and resuming waits, while such a holder is left.
      Once it is linked to, its terminals are adopted as at start; an ordinary stop and a confirmed out-of-range build
      may still end it.
- [x] After a crash, each screen is rebuilt from its tail and marked inexact. The program is resized by one row and
      back so a full-screen program repaints.
- [x] A spawn the holder has not confirmed stays claimed and unavailable, and is never started a second time on a
      guess. A late failure clears the claim, and the next start reconciles it against the holder's list.
- [x] A holder whose build differs from the daemon's receives no new terminals and exits when its last one ends. New
      terminals go to a holder of the current build.
- [ ] Holders are detached from the daemon:
  - On macOS, in a session of their own.
  - On Linux, as a systemd user service that kills only its main process when stopped. The unit delegates its cgroup
    subtree, and the holder makes a group per launch inside it, falling back to the process ledger where no writable
    subtree is available. The daemon's unit is ordered to stop before the holders' units, so at a host shutdown the
    daemon makes its ordinary stop, freezing each preview, while the shells are still running. A daemon not run by a
    systemd user manager detaches its holders as on macOS, with the process ledger alone.
  - On Windows, outside the daemon's hidden console and its jobs, as "Process holders on Windows" in
    `docs/architecture.md` describes.
  - Either way, a holder outlives the daemon's process group.
- [ ] The daemon's crash hook, which today kills every PTY process when the daemon panics or aborts, no longer
      reaches programs a holder runs. A holder's own crash still ends its programs.
- [ ] Helpers connect to their holder, not to the daemon:
  - The holder listens on a local address of its own, an HTTP one included, since Claude Code's hooks take a URL.
  - It issues and checks each launch's credentials, and relays helper requests to the connected daemon.
  - While no daemon is connected it keeps the requests and hands them over once one connects, as bounded best-effort
    buffering: reports past 1024 requests or 16 MiB of bodies drop the oldest. A request its caller waits on fails
    once a deadline that covers an ordinary restart passes.
- [x] Holders and their helpers run from a copy of their build outside the installation location, removed once that
      build's last holder exits (see the overview).
- [x] The handshake carries the holder's build, and the daemon adopts only holders from the previous stable release
      up to its own build. It ends any other holder with its programs (see the overview).

## Implementation plan

- [x] Move PTY ownership behind the holder protocol, with the daemon still ending everything on exit, so behavior is
      unchanged.
- [ ] Route the helpers through the holder.
- [ ] Then let holders outlive the daemon, and add adoption and crash recovery.
- [x] Then add the build check and draining.

## Handoff

- On Windows nothing a holder or a helper runs may come from the installation folder, the console host included: a
  running `OpenConsole.exe`, a loaded `conpty.dll` or a running `octoboardd.exe` stops the updater's passive install
  on an Abort/Retry/Ignore box. The per-build copy is built for this; confirm it with a real update over running
  sessions.
- The Windows detach was measured over an OpenSSH session with no desktop logon. Confirm it from a standard user's
  desktop logon, with the daemon started by the desktop application, including the case where breakaway is refused
  and the desktop application is named as the parent.
- The bundled console host holds a program while its terminal queries go unanswered, and with no daemon connected
  only the holder answers, the device attributes alone, though the daemon answers the device attributes and the cursor
  position while no client is attached (`TermFacts::host_waits_on_queries`). Verify on Windows that a long run with
  no daemon keeps progressing and that recovery goes on without a keystroke and without duplicate or late answers; if
  an unanswered cursor position still blocks recovery, a shared parsed terminal state with an exact answer is an
  architecture change to decide. The test harness answered those same two, and still Claude Code drew
  nothing on the bundled host until the first input reached it; the cause is not known. Find it on the Windows VM.
  How queries are answered while a holder has no daemon is decided (see "Mid-task handoff"); revisit it if the cause
  shows a program does not recover once a daemon is linked again.

## Mid-task handoff

**State.** Every technical-design item and every implementation-plan step is written and committed on
`hold/process-holders`, through six rounds of change check, then merged with `integration/update-survival` and changed
after an independent review of the decisions (below). It compiles and passes clippy on macOS and the Windows cross-check, the UI suite passes, and a first full
Rust test run proved the ticked boxes; the Rust tests remain to be rerun after the fixes that followed it. A box is
ticked only when passing tests cover it (a tick means implemented and verified); the open ones wait on failed or
unreached tests, Linux, Windows or manual runs. The product docs,
`docs/architecture.md` ("Process holders", "Process holders on Windows"), `docs/structure/daemon.md`,
`apps/daemon/src/holder/README.md`, `apps/daemon/PROTOCOL.md` (`screen_inexact`, `holder_link`) and the process
ledger's README describe the implemented behavior.

**To pick it up.**
1. Compile: `cargo check -p octoboardd --all-targets`, clippy with `-D warnings`, `pnpm check:windows`. Expect compile
   errors; the riskiest spots are `holder/client/link.rs` (reconnect, pending links, tails across a reconnect),
   `holder/client/mod.rs` (pending holders), `holder/server/terminal.rs` (`Handle::block_on` from terminal threads,
   the offline query recogniser), `terminal/pty.rs` (`move_size` closures handed to the fan) and
   `platform/proc/windows.rs` (`start_detached`).
2. Tests: `holder::`, `coordinator::adopt`, `coordinator::launch`, `coordinator::exit_watch`,
   `coordinator::resume_on_start`, `processes::ledger`, `server::relay`, `server::`, `terminal::`, `session::`,
   `state::`, `store::` (incl. `store::held_terminals`, `store::schema`), `adapter::`, `session_git_hooks`, `mcp::`,
   `request_guard`, `loopback`, `log_file`, `cli::`, `protocol::`, `bindings` (the TypeScript changes to
   `screen_inexact` and `holder_link` were written by hand), `layering_tests`, the `platform::proc` (on Linux the
   cgroup cases) and `platform::term` contract tests; and `node --test apps/desktop/scripts/release-desktop.test.mjs`.
   The UI tests of the recovered-display mark (`TitleBar`, `TerminalPane`, `ShellTerminalPanel`, `ShellTerminalView`,
   the catalog tests) and the gallery quick check for its scenarios run before push.
3. Verify the completion criteria by hand on macOS with the built app: kill the daemon outright (`kill -9`) with an
   agent session and a shell terminal running, let the application start it again, and check each criterion (screen
   rebuilt, repainted, `screen_inexact` and its mark in the title bar and the shell strip; hooks, MCP tools and the git
   gate working; a hook report made while no daemon ran arriving; a gate call with no daemon failing after 30 s; an
   unknown terminal ended; a session no holder runs interrupted; an out-of-range holder ended, by faking its version;
   an in-range holder of another build drained; a holder that refuses connections for a while left running, its
   sessions shown `holder_link: "pending"` and taken over once it answers; a renamed shell keeping its name). Then the
   same on the Windows VM, and the Handoff items above. Also check on macOS that a terminal program's privacy consent
   prompt is still attributed to Octoboard now that programs run under a holder started from the build copy outside
   the app bundle (`docs/product/macos-privacy-permissions.md`).
4. Linux: the non-systemd path is the Unix detach used on macOS and needs the Linux build to verify; a held launch
   joins the cgroup group the daemon made for it, below the holder's own cgroup. The systemd parts (the holder as a
   systemd user service with `Delegate=yes` and a group per launch, the stop order before the holders' units, and the
   Linux shutdown criterion) are not started: they wait for the daemon running on Linux as a systemd user service.

**What the merge with `integration/update-survival` changed.**
- Multi-client 01 (size rule) and 03 (flow control): the fan's rule decides the size; for a held terminal the parsed
  screen is resized under the fan lock and the PTY after it is released (`PtyCore::move_size`, under `resize_lock`),
  so a held resize never waits on the holder under the fan lock, and calls that may wait run off the async worker.
  The repaint nudge goes through `Fan::resize`: it counts as no client's activity, changes no requested size and ends
  at the size it started from. Held output reaches the fan as a local PTY's does.
- Multi-client 04 (the daemon answers queries): while a daemon is linked its parser answers, writing through the held
  backend; replaying a tail answers nothing.
- Hardening 05 (program status): replaying an adopted terminal's tail rebuilds the records but moves no status, since
  the previous daemon already acted on that output; an adopted session's exit settles only after the reports relayed
  during adoption are queued (`AppState::after_adoption`).
- Linux background run (cgroups): the holder joins the group the daemon made for a launch (`Containment::join_launch`)
  instead of the tag alone. A holder adopted by a daemon in another cgroup has its launches followed by tag only.
- Resume-on-start now lists interrupted sessions itself, so adopted sessions, never interrupted, are not resumed.
- Multi-client 01 follow-up (`TermControl::Activate`, `Registration`, `State::reconcile`): `activate`, the
  `Registration`'s deadline settle and its leave on drop resize through `move_size` like the rest, and for a held
  terminal run on a blocking thread (the drop's leave detached, as nothing can wait in a drop). The nudge stays a
  resize-only step through `Fan::resize` that leaves the display and activity alone; both of its resizes are under
  `resize_lock`, which every reconcile takes, and it starts and ends at the size the rule last gave the parsed screen
  (`Fan::screen_size`), not the PTY's, so it also brings a held PTY back after a resize the holder missed while out of
  reach.

**Decisions made without the user (asleep), with reasons, as revised after an independent review.**
- One holder per build is the usual allocation, not an invariant: a start picks one for new terminals and drains the
  others. The link and the helper routes share one loopback TCP listener per holder (a WebSocket `/daemon` link and
  the daemon's helper paths with a per-launch key), found through owner-only registry files under the data
  directory: loopback TCP is needed anyway for Claude Code's HTTP hooks and works the same on every OS. A terminal
  whose output nobody reads can stall the shared link: a stated limit.
- Windows parent when breakaway is refused: the desktop application (the daemon's `--parent-pid`). With no
  application, or when it cannot be opened or named, the holder starts inside the job with a logged warning; its
  terminals then end with the job, and survival is not guaranteed either when the application is in the same outer
  job. Confirming it from a standard user's desktop logon is a pre-release check.
- Every adoption is approximate (tail rebuild, `screen_inexact`, one-row resize), whatever the holder's last parting
  says, since no planned handover exists yet; 03 lets a verified exact handover skip the mark and the redraw and never
  clears a terminal already marked.
- `screen_inexact` is shown as a local note mark ("Recovered terminal display"), not a session status: for an agent
  session right after its title in the title bar (the terminal pane has no title of its own), for a shell terminal in
  the shell panel's strip row after the tab list (a mark inside a tab would nest a button in it), named with the
  terminal's title there. The views' own announcers announce it once when it appears while open.
- A session stored as interrupted whose terminal a holder still runs is not adopted; the terminal is ended, so the
  record stays the truth and a resume cannot meet a running process.
- A late spawn failure whose launching request is gone interrupts the session; a late success registers it running
  without the request's follow-up steps.
- **Changed:** while no daemon is linked, the holder answers primary device attributes only, with the daemon's own
  answer, from a bounded streaming recogniser on its output path (it keeps no screen); it answers only queries read
  while unlinked, and says so in its welcome so a linked daemon does not answer them again. Cursor position and other
  screen-dependent queries keep timing out offline: a stated limit. If the Windows verification shows an unanswered
  cursor-position query still blocks recovery, an exact answer needs a parsed screen in the holder, an architecture
  change to report back.
- The compatible range's floor is `0.0.0` (no stable release exists); the release script refuses a release whose floor
  is not the greatest stable tag below it. The protocol stays compatible within the promised range; the floor moves
  only by the previous-stable rule.
- **Changed:** a transport failure never ends a holder. A holder that is gone (registry file or verified process gone)
  is lost, one confirmed incompatible is ended, and one whose process runs but cannot be linked yet is pending: retried
  with a doubling pause up to 30 s while its file and process last, its sessions kept claimed (not interrupted, not
  resumed, their launches followed) and shown `holder_link: "pending"`, taken over when it answers and interrupted
  once it is known gone. Each launch's holder, terminal and launch ids are stored, so pending sessions are matched
  exactly; while a holder is pending, running sessions with no stored match are claimed too, resume-on-start waits
  until every pending holder has resolved (then resumes only what it would have resumed at start), and the ledger's
  reconcile runs as usual but holds back stopping what the previous run left until then, giving a late-adopted
  launch its recorded processes back. The start tries each holder once, all at once, for 3 s, so it never waits on
  one; a holder ended off the start keeps its sessions claimed until its end is confirmed, and one that cannot be
  ended is treated as pending. A plain quit ends pending holders; an explicit
  stop of a pending session fails like any request to an unreachable holder. A holder linked late is drained. One
  whose identity cannot be verified while something answers on its port is left alone, its sessions interrupted.
  The UI shows `holder_link` per session: a *Reconnecting* status and a cover over that session's or shell's
  terminal that holds its input until the link is back, then reattaches by itself (`docs/product/process-holders.md`).
- **Changed:** a shell terminal's user-given name is stored per terminal and restored on adoption.

## Notes for the developer

- **Reusable capabilities**:
  - The launch tag in each launch's environment.
  - The daemon's `platform::term` and `platform::proc` layers (ConPTY or a Unix pseudoterminal, a job object or a
    process group): a holder process would own them, with their `Pty` and `Containment` APIs as its protocol.
- **Development notes**:
  - Builds on the daemon's first paint on attach: a screen rebuilt after a crash reaches clients through it.
  - The Linux detach and the stop order, with the Linux shutdown criterion, need the daemon running on Linux as a
    systemd user service, which does not exist yet. The rest can be done and verified on macOS, on Linux without
    systemd once the Linux build exists, and on Windows.
  - The per-build copy must keep the binary's code signature on macOS and Windows, and carry the files found next to
    the binary, such as Windows' `conpty.dll` and `OpenConsole.exe` in both of its architecture subdirectories (the
    copy's `conpty.dll` starts the copy's `OpenConsole.exe`).
  - The saved process records are used at start to stop what the previous run left behind. That cleanup has to leave
    alone the launches of adopted sessions.
  - This changes the start-up rules that no process from the previous run is adopted and that sessions' scratch space
    is cleared. It also changes "Agent processes do not outlive the daemon" and the same statement for shell
    terminals. The application-lifecycle, background-process and shell-terminal product docs change with it.
- **Reference docs**: `docs/architecture.md` ("Why the daemon is split out"),
  `docs/product/application-lifecycle.md` ("Starting up", "Crashes and forced termination"),
  `docs/product/background-processes.md`, `docs/product/shell-terminals.md`, `docs/product/launching-agents.md`.
