# Application lifecycle

Octoboard is a macOS desktop application. The agent processes are owned by a background daemon that starts and stops
with it; nothing keeps running once the application is gone.

## Starting up

The daemon is started as part of the application and listens on `127.0.0.1` only, on a port the operating system
assigns. The application is handed that port by the process that started the daemon; it is never fixed and never
guessed.

Two things happen on every daemon start:

- **Every session a previous run left behind becomes *interrupted*.** None of those processes survived, whatever the
  stored status said. Archived sessions are left as they are.
- Per-session scratch space from previous runs is cleared.

**One Octoboard at a time.** A second daemon running against the same `~/.octoboard` would own a second set of agent
processes while sharing one database, so it refuses to start. The application then opens its window anyway, on a
screen that states why the daemon could not start and offers to quit — a failed start never leaves a window with no
explanation, and never leaves a window that cannot be closed.

If the daemon starts but dies later, the application says so and asks the user to restart Octoboard; it does not
silently keep showing a stale session list.

## Losing the daemon connection

A dropped connection is retried automatically a couple of times with a short backoff. While that is happening a
banner says so. Once the attempts are spent, the banner offers a Retry button; nothing retries forever on its own.
Each new connection re-reads the whole state, so nothing has to be replayed by hand.

Failures a user has to know about are shown as dismissible messages, and so are notices about a
session that are not failures — each of those names the project the session runs in, or the console
whose hub it is. A failure raised by a dialog's own action is shown in that dialog instead.

## Quitting

Quitting through the window's close button, `Cmd+Q` or the application menu's Quit all behave the same:

- **While any session has a running process, quitting asks for confirmation.** The message says that quitting
  interrupts those sessions and that each stays resumable next time.
- Once confirmed, every session process is ended and every one of those sessions is left **interrupted**, never
  archived. Clicking one next time resumes it (see "Archiving, interruption and resuming" in
  `docs/product/sessions.md`).
- With no session running, quitting is immediate and asks nothing.
- Quitting does not depend on the daemon answering: if it does not, the application exits anyway after a short wait.

## Crashes and forced termination

A crash of the application, or killing it outright, leaves **the same state** as a confirmed quit: every session
interrupted and resumable.

- The daemon exits by itself once the application is gone, whether or not it was asked to.
- Agent processes do not outlive the daemon: it ends them on its way out, including when it is signalled to stop and
  when it goes down on an internal failure.
- An agent's tool subprocesses go with the agent.
- Should a session's process nonetheless survive, the next start still marks every non-archived session interrupted
  (see "Starting up"), so the session list never claims a session is running that this Octoboard does not own.

## Files Octoboard owns

Everything Octoboard writes for itself lives under `~/.octoboard`:

| Path | Contents |
|---|---|
| `~/.octoboard/octoboard.db` | Consoles, projects, session records, and the report panel pages of every console. |
| `~/.octoboard/consoles/<console id>/` | A console's working directory, where its hub session runs, including the hub instruction file Octoboard generates there (see "The hub's instruction file" in `docs/product/launching-agents.md`). Removed when the console is deleted. |
| `~/.octoboard/run/<session id>/` | Per-session scratch space for what a launch injects. Removed when the session's process is gone, and cleared wholesale on daemon start. |
| `~/.octoboard/daemon.lock` | Enforces one daemon per data directory. |

Nothing is written inside a project directory, and nothing is written into the user's own agent configuration — see
"What Octoboard never modifies" in `docs/product/launching-agents.md`.
