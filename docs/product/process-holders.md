# Process holders

The agent sessions and the project shell terminals (see `docs/product/shell-terminals.md`) do not run inside the
daemon. Each runs on a terminal owned by a **process holder**: a separate background process, started by the daemon
from its own build, that is the parent of every program it runs and keeps them running when the daemon is gone. The
daemon speaks to each terminal through its holder. This holds for the desktop form on macOS and Windows, and for a
daemon started on its own (see "The daemon on its own" in `docs/product/server-form.md`), on Linux as on
macOS.

The files holders keep — their registry files, the copies of builds they run from and their logs — are listed in
`docs/product/files-octoboard-owns.md`.

## What a holder runs and when it ends

- The daemon starts a holder of its own build when the first terminal is asked for, and starts another if the one it
  uses is gone. One holder runs many terminals.
- Beside a Codex session's terminal, the holder also runs that session's private app server, started before the
  terminal and ended with it; a holder of an earlier build may not run one (see "Codex's private app server" in
  `docs/product/per-agent-launch-specifics.md`).
- A holder exits once it runs no terminal and is starting none, when no daemon is connected to it or the connected one
  has told it to take no more terminals.
- **A holder's own crash still ends its programs**, the processes in each agent's terminal session with them, as
  "Crashes and forced termination" in `docs/product/application-lifecycle.md` describes for an internal failure. The
  sessions it ran lose their processes as on any exit.
- **A launch stays claimed until its holder has answered.** A session's launch keeps the session claimed, so it can
  never be started twice, until the holder has said whether the program started, even when the request that asked for
  the launch is gone by then. A start that then fails leaves the session *interrupted*; one that succeeded registers
  the session as running.

## Which daemon ends leave the programs running

**An ordinary stop ends everything.** A quit, the daemon's shutdown request, a termination signal,
and the daemon finding the desktop application gone after the application said it was quitting (see "The quit note" in
`docs/product/application-lifecycle.md`) each end every session's process, leaving the session *interrupted*, and every
shell terminal (see "Quitting" in `docs/product/application-lifecycle.md`); the holders are then told the daemon is
going, and exit.

**A planned stop hands everything over.** A planned restart or a planned stop (see "Planned restart and planned stop"
in `docs/product/server-form.md`) — among them the desktop application's restart into an update, and the
daemon finding the desktop application gone with no word of a quit, as after a crash of the application — tells each
holder that the daemon is handing its terminals over, saves each
terminal's screen once it has read all the output the holder sent it up to that point and answered the queries in it,
and parts from every holder,
leaving all of them running, the ones it cannot connect to included. The holder keeps what each program writes from
then on for the next daemon, which takes every terminal over as below, with the screen it saved (see "The screen of a
terminal handed over" below).

**Any other end leaves the programs running in their holders**: a daemon that crashes, goes down on an internal
failure, or is killed outright. The sessions keep working, their agents keep their terminals, and the shell terminals
keep their shells, until a daemon starts and takes them over (see "What a daemon start takes over" below). On Windows,
the system ending the user's session — a sign-out or a shutdown — ends the holders with every other process of the
user's, so nothing is left running for the next start; the desktop application has its daemon make an ordinary stop
first, as far as the time allows (see "When the system ends the session" in `docs/product/application-lifecycle.md`).

## What a daemon start takes over

At every start, before it marks any session interrupted, clears the per-session scratch space or stops anything a
previous run left behind, the daemon connects to every holder still running, all at once, each given **3 seconds** to
answer so that the start never waits long on a holder, and decides, for each terminal it holds:

- **A session's terminal** whose session is on record and is neither archived nor stored as *interrupted* is
  **adopted**. The session keeps running with the status it was stored with; it can be attached to, its tools work
  and its reports arrive, as for a session the daemon started itself. Its scratch space is kept, and the saved terminal
  preview of an earlier process is discarded. A Codex session's private app server, where the holder runs one beside
  the terminal, is linked to again (see "Codex's private app server" in
  `docs/product/per-agent-launch-specifics.md`).
- **A shell terminal** whose project is still on record is **restored** among that project's shell terminals, with its
  directory, worktree and place in the order, and with the name the user gave it, which is kept with the terminal; one
  the user never named shows the shell's name until the shell sets a title again (see "Terminal title" in
  `docs/product/shell-terminals.md`).
- **Any other terminal is ended** — one whose session is archived, stored as interrupted or no longer on record, one
  whose project is gone, a second terminal of a session or shell terminal already taken or already running, or one the
  daemon cannot read. Its program is killed and what it started is stopped.

Then every session no holder runs is marked *interrupted*. Resume sessions on start resumes the interrupted
sessions, never an adopted session, which never stopped (see "Resuming the interrupted sessions at start" in
`docs/product/archiving-and-resuming-sessions.md`). The saved background-process records are reconciled under
"Automatic stopping and its boundaries" in `docs/product/background-processes.md`, leaving the adopted launches'
processes to those launches.

A holder whose process cannot be confirmed to be running and that cannot be connected to is left, with its registry
file, for a later start to try again; the sessions it may run are interrupted like any no holder runs.

### After a quit of the application

A quit of the desktop application, and the system ending the user's session, leaves the quit record,
`~/.octoboard/application-quit`, whenever the application had a daemon of its own that may run holders (see "The quit
note" in `docs/product/application-lifecycle.md`). Every daemon start reads it — whether the desktop application started the
daemon or it was started on its own — before it takes any holder over or starts any terminal, and while it holds the
lock that keeps one daemon per data directory, and ends the holders the quit left with their programs instead of taking
them over:

- **Which holders.** The application's record names no holders. The start makes it name the holders running at that
  moment, those whose registry file is there, and writes it back before it takes anything over; from then on the record
  names only holders that were running when the application quit, so a holder started afterwards is never one of them.
  With no holder running, the file is removed and nothing is ended.
- **What becomes of them.** A holder the record names is never taken over, and no new terminal goes to it: it is ended
  with its programs as a holder outside the compatible range is, whatever its version, so the sessions it ran are
  interrupted once its programs are gone and, while Resume sessions on start is on, resumed (see "A holder that cannot
  be connected to" below). Every other holder is taken over as usual. A named holder whose process the daemon can
  verify as its own but cannot connect to is ended by its process, at the start and at each later try; one whose
  process cannot be verified is never signalled and stays in the record. A holder counts as ended only when it confirms
  that each of its programs has ended; otherwise the daemon ends its process and goes by whether it is gone, its
  sessions staying claimed meanwhile. Each such session's preview is its screen rebuilt from the output its holder kept,
  or the screen a planned stop saved for it, as "The screen of a terminal handed over" below sets out.
- **When the record goes.** A holder leaves the record only once it is confirmed ended or found gone — at that start
  or long after it, by that daemon or a later one — and the file is removed with the last. A daemon stopped or killed
  before then leaves the rest for the next start. The start waits up to about **4 seconds** for these endings before it
  serves anything or starts any terminal; an ending still running then carries on, so the wait bounds only the start,
  never the record. A record the application has written again meanwhile, at a later quit, is left in place. Where
  holders cannot be set up at all, the record is left as it is.

There are limits. A holder the previous daemon had started that had not yet written its registry file when the record
was made to name holders is not among them, and is taken over. A record that cannot be written back keeps naming no
holders: that start ends the holders it found all the same, and the next start reads the record again and ends the
holders running then, those started since included. A quit the application records while a starting daemon is writing
the record back can be overwritten by it.

## A holder that cannot be connected to

A refused connection, a connection that does not answer in time, or one that keeps dropping says nothing of whether a
holder can go on, and one holder runs many terminals. So **a failure to connect never ends a holder**: three cases are
told apart.

- **A holder that has exited** — its process is gone, or it removed its registry file — has ended what it ran. Its
  sessions are interrupted, and the terminals it ran end.
- **A holder confirmed incompatible** — a version outside the range (see "Holders of other builds" below), or a
  greeting that cannot be read — is ended with its programs, and its sessions are interrupted. A start does not wait
  for that: the sessions stay claimed until the holder's programs are gone, and are then interrupted and, while Resume
  sessions on start is on, resumed.
- **A holder that runs but cannot be connected to yet** keeps its programs and its registry file. The daemon keeps
  trying to connect for as long as the holder runs, pausing longer after each try, up to **30 seconds** between tries:
  at a start, the holder is tried once, with **3 seconds** to answer, and if it does not, again a second later in the
  background, the pause doubling from there; a connection that drops while the daemon runs is tried again at once,
  then after half a second, doubling from there.
  Meanwhile:
  - its sessions and shell terminals keep running, unreachable: what is asked of them (input, a resize, a stop, a
    message) fails, and a request a helper waits on still fails after its 30 seconds;
  - the record of each says so, `holder_link: "pending"`, until the holder is connected to again. Octoboard's window
    shows each such session as *Reconnecting* (see "Which status a session shows" in
    `docs/product/session-statuses.md`) and covers the terminal of each such session and shell terminal, which sends
    nothing and keeps nothing typed meanwhile to send later, then attaches again by itself once the holder is
    connected to (see "While the session's process cannot be reached" in `docs/product/session-terminal.md` and
    "While the shell's process cannot be reached" in `docs/product/terminal-panel.md`);
  - each of its sessions **stays claimed**, with the status it was stored with: it is not interrupted, a resume or a
    launch of it is refused, and the processes its agent started are not stopped as a previous run's leftovers;
  - new terminals go to a new holder, and the old one takes no more once it is connected to again.

  A start that finds such a holder knows which sessions it runs from what the daemon records of each launch: which
  holder runs it. A session in a running status that nothing records the launch of may run in it too, so it is
  claimed as well, and only then the start puts off two things until every such holder is connected to or gone:
  resuming the interrupted sessions, which then resumes the sessions interrupted at the start and those claimed for
  want of a record that no holder turned out to run, and stopping what the previous run left behind (the saved
  background-process records are read back, and this run's own processes are handled, as ever). The start never starts
  a second copy of a session on a guess.

  A holder that cannot be reached any more while its terminals are being taken over is treated the same way: none of
  its terminals is taken over or ended then, and the daemon's start does not wait for it.

  Once the holder is connected to, what it runs is taken over as at a start (above), each screen rebuilt and marked
  inexact unless a planned stop saved it (see "The screen of a terminal handed over" below); a session it no longer
  runs is interrupted. Once it is known to have exited, its sessions are interrupted
  and what their agents started is stopped. A shell terminal of such a holder found at a start is listed only once
  the holder is connected to.

- **An ordinary stop ends such a holder too**: the daemon cannot end its programs through it, so it ends the holder's
  process, which ends what the holder runs, and marks its sessions *interrupted*, as for every session.

## The screen of a terminal taken over

A holder keeps the last **1 MiB** of raw output of each terminal it runs (a tail being handed to a daemon while the terminal keeps
printing may hold up to **2 MiB**). A daemon that takes a terminal over, other
than one a planned stop handed over with its screen (see "The screen of a terminal handed over" below), rebuilds its
screen and the replay a client is first painted from that output alone, so the screen is approximate: anything the
program drew before those last bytes is missing.

- The terminal is **marked inexact**: `screen_inexact: true` on the session's record and on the shell terminal's
  record, for as long as its process runs. The record of a process the daemon started itself, with nothing missed,
  never carries it. Octoboard's window shows it as a note mark ("Recovered terminal display") right after the
  session's title in the title bar, and after the tab strip for the shown shell terminal; its explanation says the
  display was rebuilt after a connection interruption and that earlier output and parts of the screen may be
  missing. The mark stays while the flag does, a redraw included. Appearing while the view is open, it is announced
  once, after the window has been attended (visible and focused) for about 1.5 seconds, the wait starting over each
  time the user comes back to the window, so nothing is said while the user is away; an open dialog that hides the rest
  of the window from assistive technology, such as Settings, does not silence it. The announcement is dropped unsaid if,
  before then, the flag clears, the view is hidden (a shell terminal's tab no longer shown) or another session or
  terminal is shown. One that comes, or is still waiting, while the view is covered because its process cannot be
  reached waits until that cover is gone (see "While the session's process cannot be reached" in
  `docs/product/session-terminal.md`). A mark already there when the view is opened or shown is not announced on its
  own. It says only how faithful the screen is, apart from whether the process can be reached: a terminal can carry
  both the mark and the *Reconnecting* presentation at once, each in its own place.
- The program is made to **redraw**: its terminal is resized one row smaller and back (one row larger and back at a
  single row), so a full-screen program paints its whole screen again over the rebuilt one. No client asked for those
  resizes: they count as no client's activity and leave the size the clients decide as it was (see
  `docs/product/terminals-on-several-clients.md`).
- **What that output asked or reported is not acted on again**: the terminal queries in it are not answered, and its
  program status reports (see `docs/product/program-status-reports.md`) move no status, since the daemon that read
  them first may already have; only what the program prints after the takeover does.

## The screen of a terminal handed over

A terminal a planned stop handed over comes back **as it was**, exactly wherever the saved screen can reproduce it. The
stopping daemon saved its screen with the whole scrollback it had kept, the titles the program pushed to restore later,
the character a repeat would draw again, the program's status reports (see `docs/product/program-status-reports.md`),
whether the program asked for colour-scheme updates, and an escape sequence or a character the output had begun and not
finished; the holder kept, apart, what the program wrote from the handover on. The next daemon restores the saved
screen, then takes in only what was written since, as output it reads live:

- The terminal is **not made to redraw**, and is **not marked inexact** unless the saved screen is, or it was marked
  before the handover, which stays.
- **The normal screen behind an alternate one is saved too**: under a full-screen program started from a shell's prompt,
  the shell's screen and scrollback come back with it, in the daemon's screen of the terminal and in the first paint of
  every terminal that attaches, so they are shown exactly as they were once the program leaves its alternate screen (see
  "Output, scrollback and size" in `docs/product/session-terminal.md`).
- **A saved screen is marked inexact when it cannot reproduce the terminal's state**: an alternate screen (a full-screen
  program's) over a normal screen with anything drawn on it or kept in its scrollback, when the terminal's width changed
  while the alternate screen was shown and the program has not entered it again since (the hidden normal screen was
  reflowed, which can separate its cursor from the one the program's entry saved) — a character set other than ASCII,
  tab stops set or cleared since the last reset, a colour of the palette the program changed, a cursor outside the
  scroll region in origin mode, a cursor the alternate screen keeps saved while the normal one is shown, a stack of
  pushed titles it cannot push again, or a character to repeat it cannot draw again. Such a screen is restored all the
  same, with what was written since, marked inexact, and the program is asked to draw it again, as for a screen rebuilt
  from the tail.
- **The output written during the restart appears once**, after the restored screen, neither lost nor shown twice.
- The program status reports in it move the status, as they would have had the stopping daemon read them.
- **Each query is answered once.** The queries in the output before the handover are the stopping daemon's: until it
  parts from the holder it still writes their answers, and the clients' answers to those it left to them, into the
  program. The queries in what was written since **that are still fresh are answered** by the new daemon: those in
  output at most about **1 second** old when it is read, which a program may still be waiting on. A query the holder
  already answered while no daemon was connected (see "Terminal queries while no daemon is connected" below) is not
  answered again, and an older query is dropped, neither answered nor passed to the clients, since a late answer would
  reach the program as typed input. A holder of a later build that answers kinds of query this daemon does not know has
  none of the queries since the handover answered. An answer to a query from before the handover may reach the program
  after the holder's answer to a later one.
- The restored screen leaves out what a client's first paint leaves out (see "Output, scrollback and size" in
  `docs/product/session-terminal.md`).

**A screen that cannot be verified as exact is rebuilt instead**, marked inexact and redrawn, as for any terminal taken
over (see "The screen of a terminal taken over" above). A saved screen is used only when the terminal's holder reports
the same handover, it kept the whole of the output since it — more than 1 MiB written since leaves only the end of it —
and the terminal still has the screen's size. A screen is not saved at all unless, within **3 seconds** of the
handover, the stopping daemon has read the output up to it, every answer to the queries in that output has gone into
the program, no client still owes an answer to such a query, and no answer was lost; nor when the output is in the
middle of an escape sequence longer than **64 KiB**. A screen serves one start: the next daemon takes it out of the saved
screens once its terminal is taken over or ended, before it takes in any of the output since the handover, so that a
start after it never answers those queries a second time; one it cannot take out for good is not used (see
`~/.octoboard/restart-screens.json` in `docs/product/files-octoboard-owns.md`). The screens
of a holder that cannot be connected to at that start are kept until it is connected to, by that daemon or a later one,
a later planned stop carrying them over.

**A saved screen whose holder is gone becomes the session's preview.** Where a planned stop saved a session's screen
and its holder then ended — with the system shutting down, say — the next start, finding the holder gone, or finding
that a holder it could not connect to at the start is gone, keeps that screen as the session's saved terminal preview,
and the session is interrupted with it, as an ordinary stop would have frozen it (see "Interrupted and archived
sessions" in `docs/product/session-terminal.md`). The preview is restored from the saved screen as an adoption restores
it; where it is of a size no screen can have, it is drawn at 24 rows by 80 columns. A holder the daemon ends itself
(see "A holder that cannot be connected to" and "After a quit of the application" above) hands back the output
it kept: where that output holds the whole of what was written since the handover the screen was saved at, the preview
is the saved screen with that output on it; where it holds other output, the preview is rebuilt from that output, as
for a terminal taken over; where it holds none, the preview is the saved screen alone. Nothing in it is answered. A session
already interrupted or archived keeps the preview it had, and a shell terminal has none. A preview that cannot be
written is not tried again: the session is interrupted without a new one, as after a failed write at an ordinary stop.

## Losing and regaining the connection to a holder

While the daemon runs, its connection to a holder can drop with the holder still running. The daemon connects again
at once, and keeps trying while it cannot, as "A holder that cannot be connected to" above describes. What the program
prints meanwhile does not reach the daemon. When the connection is back, each of that holder's terminals is marked
inexact and made to redraw, as above.

## Terminal queries while no daemon is connected

A program asks its terminal some things through its output, such as which terminal it is or where the cursor is, and
waits for the answer; on Windows the bundled console host holds the program until one comes. A connected daemon
answers what it can (see "Answers to the program's queries" in `apps/daemon/PROTOCOL.md`). A holder keeps no screen,
so while no daemon is connected it answers only the queries whose answer needs none, with the daemon's own answer:
**the device attributes** query (`ESC [ c`), where the console host does not answer it itself. It answers a query only
as the program asks it, never one already printed. **The cursor position and every other query that needs the screen
go unanswered** while no daemon is connected, and each holds the program on such a host for about half a second.
Which of the two answers a query is decided by when it is printed, so it is answered once: a query printed while a
daemon is connected is that daemon's, and a daemon never answers a query in the output a holder kept, except in the
output since a planned handover whose screen it restores exactly (see "The screen of a terminal handed over" above).
From a planned handover on, no daemon is connected for a terminal's output, so its holder answers the device
attributes in it.

## Session helpers and the holder

The helpers Octoboard gives a session — its hooks, its MCP server, the git hooks of a project session, and Claude
Code's status line — call the holder that runs the session, not the daemon, with a key the holder issued for that
launch alone. The holder accepts only keys it issued, and passes what the helpers ask on to the connected daemon,
which judges it as it would a call made to itself.

**Reports are kept across a daemon restart, within a bound.** A request that only reports something — a hook's
report, a git ref report, a context-window report — is answered at once, whether or not a daemon is connected. While
none is, the holder keeps the reports as best it can, at most **1024** of them and **16 MiB** of their bodies,
dropping the oldest past either, and hands them to the next daemon in the order they came, once that daemon has taken
its sessions over. Past those bounds the oldest reports are lost.

**A launch's helpers lose the holder once the session has ended.** The holder stops taking a launch's key a few
seconds after the session's process has ended, and exits once it runs nothing more, and the key's file goes with the
session's scratch directory. A background job the session left running then reaches the holder no more. Its git hooks
alone go on, to the running daemon by the record it keeps of itself, and only where the holder refuses the key, nothing
listens on its port, or the key cannot be read — never when the holder answers that it has no daemon (see "The
session's git hooks" in `docs/product/branch-responsible-session.md`). The other helpers report nothing more.

**A request its caller waits on waits for a daemon.** A question — the revert gate's call, an MCP tool call — is
passed on at once to a connected daemon. With none connected, the holder holds it up to **30 seconds**, which covers an
ordinary restart of the daemon, and then fails it as with no daemon running: the revert gate lets the update through
(see "When it cannot decide" in `docs/product/revert-gate.md`), and the MCP tool call fails, telling the agent that no
daemon is running. A question already passed on to a daemon that is then lost fails at once, never sent again: the MCP
tool call fails telling the agent that Octoboard was lost while handling it and that the call may have run.

## Holders of other builds

Holders are per build: the build is the version and a short hash of the daemon's executable, so two builds of one
version are told apart.

- **New terminals go to a holder of the daemon's own build**, as a rule: a start keeps one holder of its build for
  them, and a holder that cannot be connected to for a while is replaced by a new one for them.
- A daemon takes over the holders of builds from the **previous stable release** up to its own version. No stable
  release exists yet, so it takes over a holder of every earlier build. A holder of another build within that range
  keeps the terminals it runs but is given no new ones, and exits once its last terminal ends.
- A holder of a version outside that range, or one whose version or greeting cannot be read, is not taken over: it is
  ended with its programs, and the sessions it ran are interrupted.

## The copy of the build holders run from

A holder, and the helpers of the launches it runs other than a project session's git hooks, run from a copy of their
build under `~/.octoboard/builds/<build>/` (on Windows `%USERPROFILE%\.octoboard\builds\<build>\`, holding `conpty.dll`
and the `OpenConsole.exe` of each architecture beside the executable), not from the installation. So an update can
replace the installed application while sessions run, and on Windows they hold no file of the installation. The copy is
always of the build the daemon runs, even when it is made after the installed executable was replaced, as the server
form's update replaces it under a running daemon (see `docs/product/server-form-updates.md`). A copy is
removed once no holder runs from it. Where the copy cannot be made, the holder runs from the installation instead; that
does not block a Windows update either: the installer renames the files the holder holds aside and the holder runs on
from them (see "Downloading and installing" in `docs/product/in-app-updates.md`).

The git hooks run the installation's own executable instead: a job a session leaves running can outlive the holder by
far, and with it the copy, while the installation is replaced in place by an update. Each runs only for as long as a
git command waits on it, and a Windows update renames a file in use aside as it does for a holder.

## How a holder is detached

- **macOS and Linux**: a holder runs in a session of its own, so nothing sent to the daemon's process group reaches it.
  It stays in the daemon's cgroup on Linux, so a service manager that ends whatever is in the daemon's service ends it
  too.
- **Linux, under a systemd user manager**: a daemon that a user's systemd manager runs as a service starts each holder
  as a transient systemd user unit of its own instead, which nothing that ends the daemon's service ends (the example
  unit is one such service; see "Running as a systemd user service" in `docs/product/server-form.md`). The
  daemon counts as run by one only on both of these, decided once as it starts: systemd names the daemon's own process
  as the one it started for a unit (`SYSTEMD_EXEC_PID` equal to the daemon's pid, with an `INVOCATION_ID`, which
  systemd sets from version 248), and the daemon's cgroup is that of a service, `<name>.service`, directly below its
  user's manager (`user@<uid>.service`) or below slices in it. A daemon started by hand, from a login's terminal or by
  `octoboardd start`, has its holders run in a session of its own as above. The holder's unit:
  - is named `octoboard-holder-<holder id>.service`, described as the application's name followed by "process holder",
    started with `systemd-run --user`, and removed once it has ended, whether or not it failed;
  - when stopped, has only the holder signalled (`KillMode=process`), which ends its programs itself;
  - delegates its cgroup (`Delegate=yes`), in which the groups of the agent launches the holder runs are made (see
    "Which processes belong to a launch" in `docs/product/background-processes.md`);
  - is ordered before the daemon's unit (`Before=`), and since systemd stops units in the reverse of that order, a host
    shutdown or a stop of the user's manager stops the daemon first, which makes its ordinary stop while the programs
    still run;
  - discards its output, runs in the holders' registry directory and in the user manager's environment, none of the
    daemon's being passed, and has a command line naming only the holder's id and the registry directory: no key is in
    its arguments, its properties or the journal.

  When `systemd-run` fails, the holder is started in a session of its own as above, with a warning in the daemon's log,
  and so stays in the daemon's service and ends with it. When `systemd-run` has not returned within **15 seconds**, the
  holder's start fails, and with it the start of the terminal it was for, with no such fallback, since the unit may
  have started and a second holder with the same id must not.
- **Windows**: a holder runs detached, with no console, outside the jobs the daemon is in. Where a job does not let it
  break away, it is started as a child of the desktop application instead; that keeps it running only while the
  application is not in the same job. Where neither is possible it starts inside the daemon's job, and its terminals
  end when that job does.

## When holders cannot be set up

Where the daemon cannot set up holders at all, its terminals run inside the daemon itself. The daemon then ends them
on its way out, an internal failure included, and the next start has nothing to take over: a session whose process
survived anyway is marked interrupted. A planned stop there ends every session as an ordinary stop does.
