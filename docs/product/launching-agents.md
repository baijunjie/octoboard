# Launching agents

Every session is a native agent CLI process. Octoboard supports three agents — **Claude Code**, **Codex** and
**Grok Build** — and starts each one's own binary, resolved from the user's `PATH`.

## What Octoboard never modifies

This is the guarantee the whole design rests on:

- **Octoboard installs nothing into a project.** It writes no instruction file, no settings file and no hook of its
  own, and changes none of the project's. What the session's agent itself writes while it works is the point of the
  whole thing and is not Octoboard's doing. Removing a project removes an association, never a directory. The one
  thing Octoboard does to a project directory on its own account happens only at the user's word: a project's branch
  is fast-forwarded when it is behind its upstream, which moves its working tree — on its own once **Automatically
  sync repositories** has been turned on, or for one project when the user chooses its **Sync repository** (see
  "Automatically syncing repositories" and "Syncing one project by hand" in `docs/product/project-git-status.md`).
- **The user's own agent configuration is never written to, with two narrow exceptions.** Octoboard does not edit
  `~/.claude.json`, `~/.codex/`, `~/.grok/`, an account's config directory, or anything else the agent reads as the
  user's global setup, and it never makes a trust decision of its own in any of them. The first exception is switching
  a session to another account (see "Switching a session's account" in `docs/product/sessions.md`): it copies the one
  conversation record of that one session into the target account's directory, adding it there or replacing an
  earlier copy of the same record. It builds the copy in a transient `.octoboard-switch` directory at the root of the
  target directory, which it removes again, so a failure never leaves a half-written record among the agent's own. It
  touches no settings file of the agent's, none of the other records in either directory, and it leaves the original
  where it was. The second is Grok Build's trust store: once Grok's own trust confirmation has been accepted, the one
  entry Grok wrote into its session's copy of the store is carried, unchanged, into the user's own (see "Carrying Grok
  Build's trust record" in `docs/product/folder-trust.md`). Creating a Codex account's directory that does not exist
  yet, empty, before its first launch is not an exception: Codex refuses to start without it, and nothing of the
  user's is in it.

Where an agent stops to ask whether to trust a folder, Octoboard decides nothing on its behalf: it presses the
agent's own confirmation, with the keystrokes a person would type — for a project session only after the user has
given Octoboard permission, for a console session in the console's own working directory without asking — and the
agent records the trust in its own configuration, as it does when a person answers. See
`docs/product/folder-trust.md`.

Two settings of theirs are *read* at launch and never written: whether Claude Code has been trusted
with the project's directory, and whether Codex resolves approval requests by itself. Each one
changes what Octoboard can promise for that session (see "Per-agent specifics a user will notice"
below and "The raised hand" in `docs/product/sessions.md`). The Codex setting remains the user's to
change in Codex itself; Claude Code's trust is given by answering its trust confirmation.

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
- **A role description** — whether this session is a console session or a project session, and the reporting conventions
  that go with that. A project session bound to a console session is told that console session is waiting on its result
  and how to `report` to it; one bound to a project session is told which project session, by its id, dispatched it and
  that it reports there. One the user opened without a console session to report to is told that it was opened directly,
  that nobody is waiting on it, and not to call `report`, and that it may start sessions in its own project and drive
  them, which report back to it. A lead session, bound to a console session only after it was opened, is given that
  same unbound project session's text and tools on every launch, as at its first (see "Lead sessions" in
  `docs/product/hub-orchestration.md`). All of this follows from what is fixed for the session's lifetime, so the text
  never changes under it. The role and whether the session was opened bound decide which tools the session is offered,
  so an unbound project session still has `report`, and calling it is refused while it is unbound (see "Reporting" and
  "The unbound project session's tools" in `docs/product/hub-orchestration.md`).

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

### The console session's instruction file

A console session whose agent reads an instruction file gets one generated in the console's working
directory: `CLAUDE.md` for a Claude Code console session, `AGENTS.md` for a Codex console session. It
holds the console session's role — decompose, dispatch, follow up, summarize, and do not modify
project code itself — and how to use the orchestration tools. A **Grok Build console session gets no
such file**, because Grok reads no instructions in a directory that is not a git repository and a
console's working directory is not one; its whole role travels in a launch flag instead.

The file is written when the console is created, rewritten when the console's console session agent
changes, and refreshed immediately before every console session launch — so an edit the user makes to
it does not survive the next launch. Only the file for the console's current console session agent is
kept: one left behind by a previous agent is removed, because every agent matches instruction
filenames by exact spelling and some read more than one of them.

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

The same capture is what gives a git clone the user's `git` and git credentials, so a clone is refused the same
way (see "Associating a project" in `docs/product/consoles-and-projects.md`).

Two groups of variables are removed from that snapshot:

- the snapshot shell's own terminal variables, which would otherwise make the agent downgrade its renderer, colours
  and mouse handling; the session's terminal is declared as `xterm-256color` instead;
- the variables that mark a process as *running inside an agent session*. Without this, an Octoboard started from
  inside an agent session would hand that session's identity to every agent it launches, which silently changes their
  behaviour. Authentication is unaffected — credentials live in the macOS Keychain, not in these variables.

Besides the terminal type, the variables set over the snapshot are:

- for a session that holds a config directory of its agent, the agent's own variable: `CLAUDE_CONFIG_DIR` for Claude
  Code, `CODEX_HOME` for Codex, each set to that directory and replacing any value the user's shell exports. A session
  that holds none gets the snapshot's own value, if there is one, unchanged; Octoboard sets `CODEX_HOME` for nothing
  else;
- for every Grok Build session, `GROK_HOME`, set to the session's own Grok home (see "Per-agent specifics a user will
  notice" below). A Grok Build config directory is not passed as `GROK_HOME` itself.

The agent binary is resolved from that same snapshot's `PATH`, which is that login + interactive shell's `PATH`, not the
one a terminal happens to have. So when the shell lists an old install of an agent before a newer one, the old one is
launched. Seen with Codex: a Finder-style launch resolved an older Homebrew build ahead of the current one in
`~/.local/bin`. That Codex was older than the async hook support Octoboard injects (inferred from its "skipping async
hook" warning; the session ended on an API error before status could be observed), and such a Codex cannot supply hooks
(see "Codex" in `docs/agent-cli-reference.md`). The cause is the user's `PATH` order, not Octoboard.

Which directory a session holds, and the refusal of a launch whose directory no longer exists, are described in "Agent
config directories" in `docs/product/consoles-and-projects.md`.

**Whether an agent is installed at all is worked out once, not per launch.** Right after the daemon starts it takes
one login-shell snapshot of its own — the same kind described above — resolves each agent's binary against that
snapshot's `PATH`, and reads what each agent's default account resolves to from it (see "Agent config directories" in
`docs/product/consoles-and-projects.md`). A binary that resolves is all "available" means: whether an account's
directory holds a login is never checked, since a user may sign in through an API key from their shell, a credential
helper or an organization's gateway, none of which shows in the directory. This does not hold up the daemon's start:
the application is served immediately, and the result follows once it lands, typically within a couple of seconds
and budgeted the same ten seconds a launch's own snapshot is. Until it lands every agent reads as *not yet
determined*, during which nothing is refused for being unavailable — a launch whose agent turns out to have no binary
is refused by the ordinary path above, which already covers it. A snapshot that does not complete leaves availability
not yet determined for the rest of that run rather than marking every agent unavailable. With every agent determined
unavailable, no session can be opened at all; see "Opening a session" in `docs/product/sessions.md`.

## Per-agent specifics a user will notice

**Claude Code.** Octoboard assigns the session's id up front, so a session has an agent-side id from the moment it
starts. The flags that would drop the project's own permission rules, hooks or MCP servers are never passed.

When Claude Code has **not been trusted with the project's directory**, it ignores that project's own
`allow` permission rules for the session while still applying its `deny` rules — so the session is
only ever more restrictive, never less. Octoboard tells the user so once, when the session starts,
naming the project it is in: the `allow` rules stay ignored until Claude Code's trust confirmation is
answered, and Octoboard presses that confirmation once the user has agreed to it (see
`docs/product/folder-trust.md`). It stays silent unless the user's own configuration says
explicitly that the directory is untrusted, so a configuration it cannot read leaves them alone
rather than warning on every launch; and it stays silent when a directory above the session's is
recorded as trusted, which Claude Code honours without asking while leaving the directory's own
record saying untrusted. The trust state is read from the global config file this launch's Claude Code
will itself read: `.claude.json` inside the Claude Code config directory in effect — the session's own, or else a
`CLAUDE_CONFIG_DIR` the user's shell exports — and `~/.claude.json` when there is none. A config directory without
that file yet produces no warning; `~/.claude.json` is not consulted in its place.

**Codex.** Codex cannot be given a session id in advance, and its conversation is created lazily on the first prompt
submission — so a session opened without a task has no agent-side id until the user types something. The launch
conditions that are visible:

- project trust is not passed: in a git repository Codex has not been told to trust, its own folder-trust
  confirmation appears, and Octoboard presses it under the user's permission (see `docs/product/folder-trust.md`);
- Codex's check for a newer version at startup is turned off for that launch (`check_for_update_on_startup=false`),
  so that its update prompt does not come up ahead of the trust confirmation; the user's own setting is left as it is;
- the review Codex would otherwise raise for Octoboard's own hooks is bypassed, which costs two warning lines in the
  session's own output on **every** launch, a resume included. The bypass covers hook review only; it does not weaken
  the sandbox or the approval policy. These two lines are a standing cost, not a defect waiting to be fixed: Codex
  gates a hook behind a trust hash taken over the hook's own command, and that command is the session's own
  per-session script, so no pre-captured hash could ever match it.
- Octoboard's own tools are pre-approved for the session, so a console session's orchestration calls
  raise no approval dialog. This covers Octoboard's tools alone; every other tool, the sandbox and
  the approval policy are untouched.

Whether Codex resolves approval requests by itself is read from `config.toml` in the Codex home that launch uses: the
session's Codex config directory, else a `CODEX_HOME` the user's shell exports, else `~/.codex`.

**Grok Build.** A Grok session runs against a per-session Grok home that Octoboard builds afresh on every launch from a
**source home**: the session's Grok Build config directory when it holds one, else a `GROK_HOME` the user's shell
exports, else `~/.grok`. The per-session home links back to the source home's entries, so the login and the
session records are the source home's own, and a session Octoboard started stays resumable from the user's own `grok`
command run against that same home — for a console's Grok Build config directory, `GROK_HOME=<that directory> grok`.
Two of its files are **copies** taken at launch rather than links, with consequences worth knowing:

- the Grok configuration file: an edit the user makes to it while a session is running is not picked up by that
  session, and a setting the session persists (changing its reasoning effort, for instance) lands in Octoboard's copy
  and is lost when the session ends. Both are re-read from the source home on the next launch.
- the trust store, `trusted_folders.toml`: copied as it is, with nothing added, so a folder the user has not trusted
  Grok with shows Grok's own trust confirmation. The one entry Grok writes into the copy when that confirmation is
  accepted is the exception to its being lost: it is carried into the source home's store (see "Carrying Grok
  Build's trust record" in `docs/product/folder-trust.md`).

A Grok Build config directory must **already be a Grok home, one Grok has been run against**. Only entries
that already exist in the source home are linked; for an empty directory, Grok creates its login and session records
inside the per-session home, which is discarded with the session's process, so neither the login nor the conversation
persists. Octoboard does not check this when the directory is set — a user pointing it at a directory they are about
to create should not be stopped. It is checked at **launch** instead, and a Grok Build session pinned to a directory
that is not an initialized Grok home is refused there, naming the directory and what is wrong with it; a session on
the default account is never held to this, since that directory is the user's own existing setup.

Grok Build additionally **requires the project to be a git repository**: it locates a project by walking up for a
`.git` directory, and in a directory without one it loads neither the project's instructions nor the project's hooks.
A console's working directory is not a repository, so a Grok console session reads no instruction file from it and
takes its role through a launch flag instead (see "The console session's instruction file" above).

## Agent session data

Conversation history and session records belong to each agent and stay wherever that agent keeps them. Octoboard
never prunes or deletes them; their retention and cleanup follow the agent's own rules. It copies one only when the
user switches that session to another account of the same agent: the session's own record, and no other, is added to the
target account's directory so the conversation can continue there. Copying leaves the record in the account it came
from as well, so switching back needs no second copy; and a record Octoboard copied is the agent's own from then on,
retained and cleaned up by the agent's rules like any other.
