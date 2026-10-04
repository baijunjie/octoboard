# Launching agents

Every session is a native agent CLI process. Octoboard supports three agents — **Claude Code**, **Codex** and
**Grok Build** — and starts each one's own binary, resolved from the user's `PATH`.

## What Octoboard never modifies

This is the guarantee the whole design rests on:

- **No file inside a project is written, changed or deleted.** Not an instruction file, not a settings file, not a
  hook. Removing a project removes an association, never a directory.
- **The user's own agent configuration is never written to.** Octoboard does not edit `~/.claude.json`,
  `~/.codex/`, `~/.grok/` or anything else the agent reads as the user's global setup, and it never records a trust
  decision on the user's behalf.

Two settings of theirs are *read* at launch and never written: whether Claude Code has been trusted
with the project's directory, and whether Codex resolves approval requests by itself. Each one
changes what Octoboard can promise for that session (see "Per-agent specifics a user will notice"
below and "The raised hand" in `docs/product/sessions.md`); both decisions remain the user's to make
in the agent itself.

Everything Octoboard adds is injected **per launch** and disappears with the process. In consequence, a project's own
configuration keeps working exactly as it does outside Octoboard: its instruction file, its skills, its hooks and its
permission rules all take effect, because the session runs with the project's directory as its working directory and
nothing suppresses the agent's normal discovery of them. A hook the project itself defines and Octoboard's own hook
both run, in the same session.

## What is injected on every launch

Three things, and nothing else:

- **Status hooks** — the events the session statuses in `docs/product/sessions.md` are derived from.
- **Octoboard's own MCP server** — the tools described in `docs/product/hub-orchestration.md`. It is
  a child process of the agent, started fresh per session, and each session's tools are reachable
  only by that session. The agent's own and the project's own MCP servers keep working alongside it.
- **A role description** — whether this session is a console's hub or a project worker, and the
  reporting conventions that go with that. The same role decides which tools the session is offered.

The injected hooks are built to be invisible. They never steer the agent, never print anything, never fail the turn,
and carry a short timeout (3 seconds) so a daemon that is unreachable costs a turn a fraction of a second rather than
stalling it.

**A session's role is fixed for its lifetime.** Two of the three agents record the injected role text
into the conversation on its first turn and replay it on every resume afterwards, so changing a
session's role later is not possible: a session that belongs on the other side of the orchestration
is a new session.

A resume reassembles the full injection, because all three agents resolve hooks and MCP servers from the launch
arguments every time and a resume that omitted them would leave a session nobody can observe and nothing to report
with.

### The hub's instruction file

A hub whose agent reads an instruction file gets one generated in the console's working directory:
`CLAUDE.md` for a Claude Code hub, `AGENTS.md` for a Codex hub. It holds the hub's role — decompose,
dispatch, follow up, summarize, and do not modify project code itself — and how to use the
orchestration tools. A **Grok Build hub gets no such file**, because Grok reads no instructions in a
directory that is not a git repository and a console's working directory is not one; its whole role
travels in a launch flag instead.

The file is written when the console is created, rewritten when the console's hub agent changes, and
refreshed immediately before every hub launch — so an edit the user makes to it does not survive the
next launch. Only the file for the console's current hub agent is kept: one left behind by a previous
agent is removed, because every agent matches instruction filenames by exact spelling and some read
more than one of them.

## The launch environment

The agent is launched directly, not through a shell, with an environment captured from the user's **login +
interactive** shell at the moment of the launch. That is what makes agents installed under a version manager or in
`~/.local/bin` findable, and it is what carries the user's API keys and other settings into the session. The snapshot
is taken per launch, so a change to the user's shell configuration takes effect on the next session rather than
requiring a restart.

**Capturing it is bounded at ten seconds, and a capture that does not complete refuses the launch.** Octoboard
refuses rather than starting an agent with a half-built environment, because a session missing the user's `PATH` or
their keys fails later in ways that do not point back here. Three outcomes are refused, and the message names which
one it was:

- the capture ran out of time — a shell startup file is blocked on something, or a background process it started is
  still holding the shell's output open;
- the shell exited with an error, in which case its own diagnostic output is included;
- the shell exited cleanly without having produced a complete environment dump.

The same capture is what gives a GitHub clone the user's `git` and git credentials, so a clone is refused the same
way (see "Associating a project" in `docs/product/consoles-and-projects.md`).

Two groups of variables are removed from that snapshot:

- the snapshot shell's own terminal variables, which would otherwise make the agent downgrade its renderer, colours
  and mouse handling; the session's terminal is declared as `xterm-256color` instead;
- the variables that mark a process as *running inside an agent session*. Without this, an Octoboard started from
  inside an agent session would hand that session's identity to every agent it launches, which silently changes their
  behaviour. Authentication is unaffected — credentials live in the macOS Keychain, not in these variables.

## Per-agent specifics a user will notice

**Claude Code.** Octoboard assigns the session's id up front, so a session has an agent-side id from the moment it
starts. The flags that would drop the project's own permission rules, hooks or MCP servers are never passed.

When Claude Code has **not been trusted with the project's directory**, it ignores that project's own
`allow` permission rules for the session while still applying its `deny` rules — so the session is
only ever more restrictive, never less. Octoboard tells the user so once, when the session starts,
naming the project it is in; accepting the trust prompt in Claude Code itself is the only fix, and Octoboard
does not take that decision for them. It stays silent unless the user's own configuration says
explicitly that the directory is untrusted, so a configuration it cannot read leaves them alone
rather than warning on every launch.

**Codex.** Codex cannot be given a session id in advance, and its conversation is created lazily on the first prompt
submission — so a session opened without a task has no agent-side id until the user types something. Two launch
conditions are visible:

- the project's directory is marked trusted for that one invocation, so no folder-trust dialog appears; nothing is
  persisted to the user's configuration by it;
- the review Codex would otherwise raise for Octoboard's own hooks is bypassed, which costs two warning lines in the
  session's own output on **every** launch, a resume included. The bypass covers hook review only; it does not weaken
  the sandbox or the approval policy. These two lines are a standing cost, not a defect waiting to be fixed: Codex
  gates a hook behind a trust hash taken over the hook's own command, and that command is the session's own
  per-session script, so no pre-captured hash could ever match it.
- Octoboard's own tools are pre-approved for the session, so a hub's orchestration calls raise no
  approval dialog. This covers Octoboard's tools alone; every other tool, the sandbox and the
  approval policy are untouched.

**Grok Build.** A Grok session runs against a per-session Grok home that links back to the user's real one, so login
state is shared and a session Octoboard started stays resumable from the user's own `grok` command. Two of its files
are per-launch **copies** rather than links, with consequences worth knowing:

- the Grok configuration file: an edit the user makes to it while a session is running is not picked up by that
  session, and a setting the session persists (changing its reasoning effort, for instance) lands in Octoboard's copy
  and is lost when the session ends. Both are re-read from the user's file on the next launch.
- the trust store: the project's directory is marked trusted in the copy, so the project's own instructions, hooks and
  MCP servers load — without writing a trust decision into the user's own store.

Grok Build additionally **requires the project to be a git repository**: it locates a project by walking up for a
`.git` directory, and in a directory without one it loads neither the project's instructions nor the project's hooks.
A console's working directory is not a repository, so a Grok hub session reads no instruction file from it and takes
its role through a launch flag instead (see "The hub's instruction file" above).

## Agent session data

Conversation history and session records belong to each agent and stay wherever that agent keeps them. Octoboard
never prunes, copies or deletes them; their retention and cleanup follow the agent's own rules.
