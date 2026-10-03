# Sessions

A **session** is one agent CLI process, running with a project's directory as its working directory and rendered in a
terminal the user types into directly. Octoboard is not an agent and not a terminal of its own: it starts the agent's
own binary and shows it as it is. What it adds to each launch, and what it guarantees it does not change, is in
`docs/product/launching-agents.md`.

## Hub sessions and project sessions

| | Working directory | Where it appears |
|---|---|---|
| Hub session | the console's working directory | the console's "Hub" row |
| Project session | the project's directory | under that project |

A console has at most one hub session visible at a time. Clicking the Hub row starts one if there is none, and selects
the existing one otherwise.

A hub session carries no project. It is an ordinary agent session in the console's working
directory — it is given no orchestration tools and no role description, and nothing dispatches work to it.

## The console → project → session menu

The left-hand tree has three levels: console → project → session. Console and project rows expand and collapse;
collapse state is per window and is not stored.

Under each project, sessions whose process is running and sessions that were interrupted are listed directly;
archived sessions are grouped under an "Archive (n)" row that expands on its own. Each session row shows its status,
its title, and a badge naming its agent (Claude Code, Codex, Grok Build).

Selecting a session shows its terminal. Clicking a row deliberately does not move keyboard focus away from the
terminal; a row reached with Tab can be activated with Enter or Space.

## Opening a session

A session is opened under a project with:

- **Agent** — defaulted as below, overridable for this session only.
- **Title** (optional) — defaults to the project's name. A hub session's title defaults to "Hub".
- **Initial task** (optional) — handed to the agent as its initial prompt.

A session opened **with** a task starts in *working*. A session opened **without** one starts in *awaiting
instructions*: it is sitting at the agent's prompt.

If the launch itself fails — the directory cannot be reached, the agent binary is not on the user's `PATH` — no
session appears in the tree and the failure is reported.

### Which agent a session uses

In descending priority:

1. the agent chosen for this session when it was opened;
2. the project's default agent;
3. the console's default agent.

A hub session uses the console's hub agent instead.

A session's agent is fixed for its lifetime. Resuming or reopening a session always relaunches the same agent —
session records belong to a specific agent and cannot be moved across agents.

## Session statuses

| Status | Wire value | Meaning |
|---|---|---|
| Working | `working` | The agent is executing a turn. |
| Waiting for the user | `waiting_user` | The agent is waiting on a permission decision or has asked the user a question through its own ask-the-user tool. |
| Awaiting instructions | `idle` | The process is running and sitting at its prompt. |
| Interrupted | `interrupted` | No process is running, and it did not end by being archived. The session stays in its project's list and can be resumed. |
| Archived | `archived` | Ended on the user's request. Listed in the project's Archive group and can be reopened. |

The first three mean a process is running; the last two mean none is, and both can be resumed.

Transitions:

- Opening a session puts it in *working* or *awaiting instructions*, per the initial task.
- While the process runs, reports from the agent move the session between *working*, *waiting for the user* and
  *awaiting instructions*.
- The process ending for any reason other than archiving — the agent exiting on its own, a crash, the application
  quitting — leaves the session *interrupted*.
- Archiving leaves the session *archived*.
- Resuming an *interrupted* or *archived* session puts it in *awaiting instructions*, whichever of the two it came
  from. A resume that fails to launch leaves the session exactly where it was, archived included.
- An agent's report never moves a session out of *interrupted* or *archived*: whether a stopped session is one or the
  other is Octoboard's own record, not the agent's.

### What the statuses are derived from

Status comes exclusively from hook events Octoboard injects into each agent per launch, never from reading the
terminal's rendered output. A session's end is not taken from a hook either: the process is observed directly.

Where an agent reports nothing, the status simply stays at its last reported value. The known cases, which are
limitations of what the agents expose rather than of this one:

- A turn that ends with the agent asking a question **as plain prose** is indistinguishable from a finished turn on
  all three agents, so such a session reads as *awaiting instructions* rather than waiting for the user.
- When the user cancels an in-flight turn in Claude Code, nothing is reported at all; the session keeps reading as
  *working* until the next prompt is submitted.
- Grok Build's bash mode (`!`) produces no events, so work done through it is invisible to the status.

The *waiting for the user* status is shown on the session's own row only. It is not bubbled up to the project or
console rows, and it raises no system notification or Dock badge.

## Archiving, interruption and resuming

**Archiving** is the user's explicit way to end a session. It ends the agent's process and keeps the session and its
record. The agent is asked to exit first and is killed only if it does not; a kill takes the agent's tool
subprocesses with it. Archiving is available for any session that is not already archived, including an interrupted
one.

**Resuming** happens by selecting an interrupted or archived session, or through its "Resume" action. It relaunches
the same agent in the same directory and reassembles everything Octoboard injects.

- Where the agent has a stored conversation, that conversation is resumed.
- A session that **nobody ever typed into** has no stored conversation on the agent's side; resuming it opens a fresh
  conversation in its place, in the same project and under the same session.
- A resume carries no initial task, so the session comes up at the agent's prompt.
- Resuming a session whose process is already running is refused. The refusal a double-click produces is not surfaced
  to the user.

An interrupted session and an archived session are resumed the same way; the difference is only how the session got
there, and where it appears in the tree.

## Renaming a session

A session's title can be changed at any time, archived sessions included. An empty title is rejected.

## The terminal

The selected session's terminal is live: keystrokes go straight to the agent, exactly as in a system terminal. Mouse
reporting works, so an agent's own mouse-driven TUI is usable. `Ctrl+C` reaches the agent.

- Attaching to a session replays the **most recent 2 MiB** of that session's output, then follows live output. The
  terminal itself keeps 10,000 lines of scrollback.
- A reattach always redraws from the replay rather than appending to what is on screen, so scrollback older than the
  replay window is lost on every reconnect.
- Resizing the window resizes the agent's terminal.
- A client that stops draining output for more than a few seconds is dropped by the daemon rather than letting output
  buffer without bound. The application then reconnects by itself, with a backoff, up to five times; while a running
  session's terminal is disconnected a "Reconnect" button is available as well. A reconnect in the background never
  steals keyboard focus.
- Selecting an interrupted or archived session resumes it rather than attaching; until a process is running, the
  terminal reads "Not running" and offers "Resume".
