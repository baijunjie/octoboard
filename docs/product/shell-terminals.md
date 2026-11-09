# Shell terminals

A shell terminal is a plain shell the daemon runs for a project, beside the project's agent sessions: the user's own
login shell on a terminal of its own, with no agent behind it. It is not the system terminal that System terminal
under "Open in" opens (see `docs/product/project-terminal.md`), and it is not a session.

Octoboard's window shows a project's shell terminals as tabs in the Terminal panel (see
`docs/product/terminal-panel.md`). They exist in the daemon, and any client of the daemon can start one, type into it,
rename it and close it; what that means for who can reach the daemon is in "Who can reach the daemon" in
`docs/product/application-lifecycle.md`.

## Who owns a shell terminal

**The daemon owns its shell terminals, not the client that started them.** Hiding a client view does not end a terminal;
explicitly closing the Terminal panel can close idle terminals when its application setting is on (see "Closing the
panel" in `docs/product/terminal-panel.md`). A terminal otherwise lives on after every client view is closed, after a
window is closed while the daemon keeps running, and across a lost and restored connection. A client that connects is
given every shell terminal the daemon holds, running or exited, oldest first,
and attaching to a running one starts from a first paint of its screen and scrollback before live output follows, the
same way a session's terminal is attached to (see "Output, scrollback and size" in `docs/product/session-terminal.md`);
attaching to an exited one is in "When the shell exits on its own" below.

Shell terminals are live state: they are held in memory, and only the name the user gave one is saved, until the
terminal is closed or its project removed. A shell terminal's shell runs in a process holder, not in the daemon (see
`docs/product/process-holders.md`): an ordinary stop of the daemon ends every shell terminal (see "Closing a shell
terminal" below), but a daemon that goes down without stopping — a crash, an internal failure, a kill — or makes a
planned stop (see "Planned restart and planned stop" in `docs/product/application-lifecycle.md`) leaves the shells
running, and the next daemon start restores each terminal whose project is still on record, under "What a daemon start
takes over" in `docs/product/process-holders.md`. A restored terminal keeps its directory, worktree, place in the order
and the name the user gave it, and its first paint is rebuilt from the output its holder kept (see "The screen of a
terminal taken over" in the same document), or, after a planned stop, is the screen it had, restored as the stopping
daemon saved it (see "The screen of a terminal handed over" in the same document). While its holder runs but cannot be connected to, the terminal
is kept, its shell running unreachable, and its record says so (see "A holder that cannot be connected to" in the same
document).

A shell terminal and the processes in its terminal session are excluded from the agent background-process lists and
cleanup (see "Automatic stopping and its boundaries" in `docs/product/background-processes.md`). Their lifecycle stays
with the terminal under the rules below.

A project may have any number of shell terminals. Each belongs to one project; a console has none of its own.

## Starting a shell terminal

**Where it starts.** A shell terminal starts in the project's directory, or, when a worktree of the project's
repository is chosen, in the project's place in that worktree. When a folder below that place is named as well, it
starts in that folder instead: the folder is named and checked as the paths File manager and Default application act on
are (see "A row's actions" in `docs/product/project-pane.md`), so one that does not exist is refused with
`file_not_found` and one that leads outside that place with `outside_scope`; one that is there but is not a folder is
refused with `path_not_a_directory`. A symbolic link that leads to a folder inside that place is followed, and the
shell starts in the folder it led to when the start was made. The start is refused when the project does not exist
(including one removed while the shell was starting, whose terminal is then ended), when the chosen worktree is not
one of the project's repository's, when the project has no place in that worktree, when the path to that place goes
through a symbolic link anywhere below the worktree's root, when what is at the directory's path is not a folder, and
when the directory cannot be reached for another reason. In Octoboard's window a refusal of the directory raises an
error alert whose remedy follows why it was refused (see "Recovery and unexpected errors" in
`docs/product/error-alerts.md`). The directory is recorded with its symbolic links resolved. After that the shell goes
wherever the user takes it; the recorded directory does not follow.

**What runs.** The shell is the user's login shell, `$SHELL` (`/bin/zsh` when that is unset), started as a login shell
(`-l`). On a Windows daemon host it is the first of PowerShell 7 (`pwsh.exe`), Windows PowerShell (`powershell.exe`)
and `cmd.exe` found on the `PATH`, `cmd.exe` when none is, started with no arguments. A PowerShell 7 installed from the
Microsoft Store is started too, and what it runs stays among the shell's own processes, for the busy reading and the
close below.

**Its environment** is the environment an agent launch takes: the captured environment of the user's login +
interactive shell — on a Windows daemon host the environment built from the system's and the user's variables —
reused under the same rules and filtered the same way (see "The launch environment" in
`docs/product/launching-agents.md`). Unlike an agent launch, **a capture that fails does not refuse the shell**: it
starts instead in the daemon's own environment, less the variables that mark a process as running inside an agent
session, and loads the user's profile itself as any login shell does. Either way the terminal is declared as
`xterm-256color`, and nothing else is set over that environment: none of the variables Octoboard sets for an agent
session (its session id, the daemon's port, the git hook path, an agent's config directory or home) is set.

## Terminal title

A shell terminal's title follows the title the shell sets with OSC 0 or OSC 2, read by the same rules as a session's
terminal title: surrounding whitespace is trimmed, the leading activity glyphs are dropped (see "Leading activity
glyphs" in `docs/product/session-titles.md`), and a title holding a control character is ignored. Before the shell
sets a title, and whenever what it sets is blank under those rules, the title is the shell's name, such as `zsh`
(`pwsh.exe` on Windows).

**Renaming** gives the terminal the user's own title, stored trimmed; a blank title is refused. From then on the
shell's titles no longer change it. The name is kept with the terminal: a daemon start that restores the terminal
from its holder shows it again, and no title the shell set before or sets after takes its place. It is forgotten when
the terminal is closed or its project removed.

## Busy shells

A running shell is **busy** while it runs something: the terminal's foreground process group is not the shell's own
(a program started in the foreground), or a process the shell started, or one of that process's descendants, still has
the terminal open as its stdin, stdout or stderr, which covers a job left running in the background, such as a server.
A process whose descriptors the daemon is not allowed to inspect (one run with `sudo`, say) makes the shell busy,
and so does a shell with more descendants than one check will look at, or one whose descendants could not be listed,
because what cannot be ruled out is asked about. A helper the prompt keeps beside the shell with all three streams redirected away from the terminal
(powerlevel10k's `gitstatusd`) does not count, nor does a job started the same way (`</dev/null >log 2>&1 &`); a job
with only its output redirected (`npm run dev > dev.log 2>&1 &`) still holds the terminal as its stdin, so it counts. A
shell sitting at its prompt with nothing started is not busy, and neither is one that has exited. The daemon checks
this twice a second while the shell runs and carries the result on the terminal's record.

**On a Windows daemon host the reading is coarser.** A Windows console has no foreground job to ask about, and its
processes hold no terminal device to look for, so the shell is busy while any process it started, directly or through
another, is still running, whatever its streams are redirected to — a helper the prompt keeps beside the shell
included — other than the console hosts the terminal runs on. When the shell's processes cannot be listed, the shell
reads as busy, as on every host: what cannot be ruled out is asked about.

## When the shell exits on its own

A shell that exits by itself (`exit`, a crash) does not take its terminal with it: the terminal stays, marked exited
and not busy, with its final screen, until it is closed. Attaching to an exited terminal shows a paint of that final
screen and then ends the attachment; it takes no input. The paint is drawn from the daemon's parsed copy of the
terminal as a running terminal's first paint is (see "Output, scrollback and size" in
`docs/product/session-terminal.md`), with the scrollback the client asks for and its wrapped lines joined, so they
reflow to the client's width. A client attached at the moment the shell exits is let go once the output still waiting
for it has reached it.

## Closing a shell terminal

Closing a terminal ends its shell if it still runs and removes the terminal; an exited terminal is removed the same
way. Every client attached to it is let go once the output still waiting for it has been sent.

A shell still running is sent `SIGHUP`, the hangup a terminal closing sends, which an interactive shell passes on to
its jobs. A shell still there 3 seconds later is killed together with its process group. Then the terminal itself is
hung up, so a job still on it — one a shell that exited left running, or one sent off with `nohup` or `disown` — loses
its terminal. On a Windows daemon host the hangup is the closing of the shell's console, which asks every process on it
to end, as closing a console window does; a shell still there 3 seconds later is ended. Nothing of a closed terminal
is kept, its output included. Closing answers once the shell is gone.

A close can be conditional on the terminal being idle. The daemon checks whether the shell is busy at that close,
under "Busy shells" above, rather than trusting its periodically refreshed record. A busy shell, or one whose idle
state cannot be established, is kept without asking. An idle shell or an exited shell is closed normally. A terminal
already gone counts as a successful close. This is the close used by "Closing the panel" in
`docs/product/terminal-panel.md`.

The same ending happens, without anyone being asked, to:

- **every shell terminal of a project that is removed** (see `docs/product/removing-projects.md`), and of every project
  of a console that is deleted (see "Deleting a console" in `docs/product/consoles.md`). A running shell never holds
  either up;
- **every shell terminal when the daemon stops**: on a quit, and on a termination signal when the daemon runs on its
  own (see "Quitting" and "The daemon on its own" in `docs/product/application-lifecycle.md`). A daemon that goes down
  without stopping, or makes a planned stop, ends none (see "Who owns a shell terminal" above); once a planned stop has
  begun, starting or closing a shell terminal is refused with `daemon_shutting_down`, and what is typed into a shell
  terminal once it is handed over is dropped (see "While the terminals are handed over" in
  `docs/product/application-lifecycle.md`). An ordinary stop asked for during a planned stop ends every shell terminal
  after all (see "An ordinary stop during a planned stop" in the same document).

If the process the shell terminals run in goes down on an internal failure — their holder, or the daemon where they run
in it (see "When holders cannot be set up" in `docs/product/process-holders.md`) — each shell is killed with its process
group, not its whole terminal session, as an agent is (see "Crashes and forced termination" in
`docs/product/application-lifecycle.md`), and the jobs still on its terminal lose it as that process goes.
