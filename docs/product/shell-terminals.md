# Shell terminals

A shell terminal is a plain shell the daemon runs for a project, beside the project's agent sessions: the user's own
login shell on a terminal of its own, with no agent behind it. It is not the system terminal that "Open in" opens
(see `docs/product/project-terminal.md`), and it is not a session.

Octoboard's window shows a project's shell terminals as tabs in the Terminal panel (see
`docs/product/terminal-panel.md`). They exist in the daemon, and any client of the daemon can start one, type into it,
rename it and close it; what that means for who can reach the daemon is in "Who can reach the daemon" in
`docs/product/application-lifecycle.md`.

## Who owns a shell terminal

**The daemon owns its shell terminals, not the client that started them.** A terminal lives on after every client
view of it is closed, after a window is closed while the daemon keeps running, and across a lost and restored
connection. A client that connects is given every shell terminal the daemon holds, running or exited, oldest first,
and attaching to one replays its scrollback before live output follows, the same way a session's terminal is
attached to.

Shell terminals are live state: they are held in memory only, never saved, and none survives the daemon.

A project may have any number of shell terminals. Each belongs to one project; a console has none of its own.

## Starting a shell terminal

**Where it starts.** A shell terminal starts in the project's directory, or, when a worktree of the project's
repository is chosen, in the project's place in that worktree. The start is refused when the project does not exist
(including one removed while the shell was starting, whose terminal is then ended), when the chosen worktree is not
one of the project's repository's, when the project has no place in that worktree, when the path to that place goes
through a symbolic link anywhere below the worktree's root, when what is at the directory's path is not a folder, and
when the directory cannot be reached for another reason. In Octoboard's window a refusal of the directory raises an
error alert whose remedy follows why it was refused (see "Recovery and unexpected errors" in
`docs/product/error-alerts.md`). The directory is recorded with its symbolic links resolved. After that the shell goes
wherever the user takes it; the recorded directory does not follow.

**What runs.** The shell is the user's login shell, `$SHELL` (`/bin/zsh` when that is unset), started as a login shell
(`-l`).

**Its environment** is the environment an agent launch takes: the captured environment of the user's login +
interactive shell, reused under the same rules and filtered the same way (see "The launch environment" in
`docs/product/launching-agents.md`). Unlike an agent launch, **a capture that fails does not refuse the shell**: it
starts instead in the daemon's own environment, less the variables that mark a process as running inside an agent
session, and loads the user's profile itself as any login shell does. Either way the terminal is declared as
`xterm-256color`, and nothing else is set over that environment: none of the variables Octoboard sets for an agent
session (its session id, the daemon's port, the git hook path, an agent's config directory or home) is set.

## Terminal title

A shell terminal's title follows the title the shell sets with OSC 0 or OSC 2, read by the same rules as a session's
terminal title: surrounding whitespace is trimmed, the leading activity glyphs are dropped (see "Leading activity
glyphs" in `docs/product/session-titles.md`), and a title holding a control character is ignored. Before the shell
sets a title, and whenever what it sets is blank under those rules, the title is the shell's name, such as `zsh`.

**Renaming** gives the terminal the user's own title, stored trimmed; a blank title is refused. From then on the
shell's titles no longer change it.

## Busy shells

A running shell is **busy** while it runs something: the terminal's foreground process group is not the shell's own
(a program started in the foreground), or a process the shell started, or one of that process's descendants, still has
the terminal open as its stdin, stdout or stderr, which covers a job left running in the background, such as a server.
A process whose descriptors the daemon is not allowed to inspect (one run with `sudo`, say) makes the shell busy,
and so does a shell with more descendants than one check will look at, because what cannot be ruled out is asked
about. A helper the prompt keeps beside the shell with all three streams redirected away from the terminal
(powerlevel10k's `gitstatusd`) does not count, nor does a job started the same way (`</dev/null >log 2>&1 &`); a job
with only its output redirected (`npm run dev > dev.log 2>&1 &`) still holds the terminal as its stdin, so it counts. A
shell sitting at its prompt with nothing started is not busy, and neither is one that has exited. The daemon checks
this twice a second while the shell runs and carries the result on the terminal's record.

## When the shell exits on its own

A shell that exits by itself (`exit`, a crash) does not take its terminal with it: the terminal stays, marked exited
and not busy, with its final screen, until it is closed. Attaching to an exited terminal replays its output and then
ends the attachment; it takes no input. A client attached at the moment the shell exits is let go once the shell's
last output has reached it.

## Closing a shell terminal

Closing a terminal ends its shell if it still runs and removes the terminal; an exited terminal is removed the same
way. Every client attached to it is let go.

A shell still running is sent `SIGHUP`, the hangup a terminal closing sends, which an interactive shell passes on to
its jobs. A shell still there 3 seconds later is killed together with its process group. Then the terminal itself is
hung up, so a job still on it — one a shell that exited left running, or one sent off with `nohup` or `disown` — loses
its terminal. Nothing of a closed terminal is kept, its output included. Closing answers once the shell is gone.

The same ending happens, without anyone being asked, to:

- **every shell terminal of a project that is removed** (see `docs/product/removing-projects.md`), and of every project
  of a console that is deleted (see "Deleting a console" in `docs/product/consoles.md`). A running shell never holds
  either up;
- **every shell terminal when the daemon exits**: on a quit, and on a termination signal when the daemon runs on its
  own (see "Quitting" and "The daemon on its own" in `docs/product/application-lifecycle.md`).

If the daemon goes down on an internal failure, each shell is killed with its process group — not its whole terminal
session, as an agent is (see "Crashes and forced termination" in `docs/product/application-lifecycle.md`) — and the
jobs still on its terminal lose it as the daemon goes.
