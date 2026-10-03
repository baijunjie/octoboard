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

Everything Octoboard adds is injected **per launch** and disappears with the process. In consequence, a project's own
configuration keeps working exactly as it does outside Octoboard: its instruction file, its skills, its hooks and its
permission rules all take effect, because the session runs with the project's directory as its working directory and
nothing suppresses the agent's normal discovery of them. A hook the project itself defines and Octoboard's own hook
both run, in the same session.

## What is injected on every launch

The injection is **status hooks only** — the events the session statuses in `docs/product/sessions.md` are derived
from. Nothing else is added to the agent: no tools, no extra instructions, no system-prompt text.

The injected hooks are built to be invisible. They never steer the agent, never print anything, never fail the turn,
and carry a short timeout (3 seconds) so a daemon that is unreachable costs a turn a fraction of a second rather than
stalling it.

A resume reassembles the full injection, because all three agents resolve hooks from the launch arguments every time
and a resume that omitted them would leave a session nobody can observe.

## The launch environment

The agent is launched directly, not through a shell, with an environment captured from the user's **login +
interactive** shell at the moment of the launch. That is what makes agents installed under a version manager or in
`~/.local/bin` findable, and it is what carries the user's API keys and other settings into the session. The snapshot
is taken per launch, so a change to the user's shell configuration takes effect on the next session rather than
requiring a restart.

Two groups of variables are removed from that snapshot:

- the snapshot shell's own terminal variables, which would otherwise make the agent downgrade its renderer, colours
  and mouse handling; the session's terminal is declared as `xterm-256color` instead;
- the variables that mark a process as *running inside an agent session*. Without this, an Octoboard started from
  inside an agent session would hand that session's identity to every agent it launches, which silently changes their
  behaviour. Authentication is unaffected — credentials live in the macOS Keychain, not in these variables.

## Per-agent specifics a user will notice

**Claude Code.** Octoboard assigns the session's id up front, so a session has an agent-side id from the moment it
starts. The flags that would drop the project's own permission rules, hooks or MCP servers are never passed.

**Codex.** Codex cannot be given a session id in advance, and its conversation is created lazily on the first prompt
submission — so a session opened without a task has no agent-side id until the user types something. Two launch
conditions are visible:

- the project's directory is marked trusted for that one invocation, so no folder-trust dialog appears; nothing is
  persisted to the user's configuration by it;
- the review Codex would otherwise raise for Octoboard's own hooks is bypassed, which prints two warning lines at the
  top of every session. The bypass covers hook review only; it does not weaken the sandbox or the approval policy.

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
A console's working directory is not a repository, so a Grok hub session reads no instruction file from it.

## Agent session data

Conversation history and session records belong to each agent and stay wherever that agent keeps them. Octoboard
never prunes, copies or deletes them; their retention and cleanup follow the agent's own rules.
