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
- [x] Adoption on start:
  - Terminals are matched with their sessions and shell terminals.
  - Terminals with no record are ended.
  - Sessions with no terminal are interrupted.
  - A shell terminal comes back with the name the user gave it, which the daemon stores by the terminal's id.
- [x] A transport failure never ends a holder. An exited holder (no registry file, or its process gone) and one
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
- Holders are detached from the daemon, and a holder outlives the daemon's process group:
  - [x] On macOS, in a session of their own.
  - [ ] On Linux, as a systemd user service that kills only its main process when stopped. The unit delegates its
        cgroup subtree, and the holder makes a group per launch inside it, falling back to the process ledger where no
        writable subtree is available. The daemon's unit is ordered to stop before the holders' units, so at a host
        shutdown the daemon makes its ordinary stop, freezing each preview, while the shells are still running. A
        daemon not run by a systemd user manager detaches its holders as on macOS, with the process ledger alone.
  - [ ] On Windows, outside the daemon's hidden console and its jobs, as "Process holders on Windows" in
        `docs/architecture.md` describes.
- [ ] The daemon's crash hook, which today kills every PTY process when the daemon panics or aborts, no longer
      reaches programs a holder runs. A holder's own crash still ends its programs.
- [x] Helpers connect to their holder, not to the daemon:
  - The holder listens on a local address of its own, an HTTP one included, since Claude Code's hooks take a URL.
  - It issues and checks each launch's credentials, and relays helper requests to the connected daemon.
  - While no daemon is connected it keeps the requests and hands them over once one connects, as bounded best-effort
    buffering: reports past 1024 requests or 16 MiB of bodies drop the oldest (the bounds covered by `server::relay`,
    run at push). A request its caller waits on fails once a deadline that covers an ordinary restart passes.
- [x] Holders and their helpers run from a copy of their build outside the installation location, removed once that
      build's last holder exits (see the overview).
- [x] The handshake carries the holder's build, and the daemon adopts only holders from the previous stable release
      up to its own build. It ends any other holder with its programs (see the overview).

## Implementation plan

- [x] Move PTY ownership behind the holder protocol, with the daemon still ending everything on exit, so behavior is
      unchanged.
- [x] Route the helpers through the holder.
- [x] Then let holders outlive the daemon, and add adoption and crash recovery, on macOS.
- [ ] The same on Linux and Windows, with the crash hook (see the open technical-design items).
- [x] Then add the build check and draining.

## Handoff

- On Windows nothing a holder or a helper runs may come from the installation folder, the console host included: a
  running `OpenConsole.exe`, a loaded `conpty.dll` or a running `octoboardd.exe` stops the updater's passive install
  on an Abort/Retry/Ignore box. The per-build copy is built for this; confirm it with a real update over running
  sessions.
- The Windows detach was measured over an OpenSSH session with no desktop logon. Confirm it from a standard user's
  desktop logon, with the daemon started by the desktop application, including the case where breakaway is refused
  and the desktop application is named as the parent.
- In a test harness that answered the device attributes and the cursor position, Claude Code drew nothing on the
  bundled console host until the first input reached it; the cause is not known. Find it on the Windows VM from a
  desktop logon. How queries are answered while a holder has no daemon is decided (see "Design decisions"); revisit
  it if the cause shows a program does not recover once a daemon is linked again.

## Design decisions

Made without the user, with reasons, as revised after an independent review.
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
  says, except a handover that 03 verifies as exact, which skips the mark and the redraw and never clears a terminal
  already marked.
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
  screen-dependent queries keep timing out offline: a stated limit. On the Windows VM (over SSH) a program writing 600
  cursor-position queries through a 46 s outage on the bundled console host kept progressing and recovered once, with
  no input, so the limit does not block recovery there.
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

## Mid-task handoff

**State.** Everything is implemented. On 2026-11-10 the macOS items were verified by hand with real builds: the daemon
killed outright and restarted by hand and by the application, with rebuilt, repainted screens marked inexact and the
recovered-display mark in the title bar and the shell strip; hooks, MCP tools and the git gate through the holder's
relay, a hook report made with no daemon arriving, a gate call with no daemon failing after 30 s; an unknown terminal
ended; a session no holder runs interrupted; an out-of-range holder ended; an in-range holder of another build drained;
a holder refusing links, and one dropping them repeatedly, left running and shown pending, then taken over; a renamed
shell keeping its name. On the Windows VM, over SSH only: crash adoption with the renamed shell, hooks and MCP after
adoption, offline report buffering, the holder outside the daemon's job, and the unanswered cursor-position run above.
Evidence, on the verification Mac: `/tmp/octoboard-manual-20261110-67e2/REPORT.md` and
`/tmp/octoboard-gui-20261110-67e2/REPORT.md` (macOS), `/tmp/windows-verification-evidence/REPORT.md` (Windows). The
scripted suites (`holder::`, `coordinator::adopt`, `coordinator::resume_on_start`, `server::relay`,
`store::held_terminals`, the `platform::proc` contract tests and the UI tests of the recovered-display mark) run at
push.

**Remaining, each a real run:**
1. The crash hook, on macOS (not run on 2026-11-10; no test covers a held program's absence from the crash cleanup):
   with an agent session and a shell running under a holder, `kill -ABRT` the daemon, whose abort handler runs the
   crash cleanup: the held programs keep running and the next daemon adopts them. Then `kill -ABRT` the holder: its
   programs end.
2. The macOS privacy prompt (`docs/product/macos-privacy-permissions.md`): from the packaged application, with holders
   running from the build copy outside the bundle, make a program in a held terminal raise a fresh consent prompt (a
   permission the verification application has not been granted or denied, such as screen recording through
   `CGRequestScreenCaptureAccess`), and check that the prompt names Octoboard. On 2026-11-10 a held program's
   responsible process was the application, but macOS raised no prompt, which proves nothing about its attribution.
3. Windows, from an interactive desktop logon, blocked on the sign-in password of the VM user `bjj`. The VM's working
   directory `C:\Users\bjj\windows-verification` keeps the source, the build and the isolated data. Signed in as a
   standard user, with the daemon started by the desktop application: the holders run outside the daemon's jobs and
   survive it, and where breakaway is refused the desktop application is the holder's parent (Handoff); Claude Code
   on the bundled console host draws before any input (Handoff); a real update over running sessions installs with no
   Abort/Retry/Ignore prompt (Handoff). Also the completion criteria not yet run on Windows at all (they need no
   desktop and may run over SSH first): the git gate after adoption, and a gate call with no daemon failing after 30 s;
   an out-of-range holder ended; an in-range holder of another build drained; an unknown terminal ended; a session no
   holder runs interrupted; a holder refusing links, or dropping them repeatedly, left running, shown pending and taken
   over.
4. Linux without systemd, blocked on Docker, which is off: with the Linux build in a container, the Unix detach, and a
   held launch joining the cgroup group the daemon made for it below the holder's own cgroup (a writable cgroup v2
   subtree, as Linux daemon run milestone 01's notes describe).
5. Linux with systemd, blocked on the daemon running on Linux as a systemd user service, which does not exist yet:
   the holder as a systemd user service with `Delegate=yes` and a group per launch, the daemon's unit stopping before
   the holders' units, and the Linux host shutdown criterion.
