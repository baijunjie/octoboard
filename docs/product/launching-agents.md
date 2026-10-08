# Launching agents

Every session is a native agent CLI process. Octoboard supports three agents — **Claude Code**, **Codex** and
**Grok Build** — and starts each one's own binary, resolved from the user's `PATH`.

## What Octoboard never modifies

This is the guarantee the whole design rests on:

- **Octoboard installs nothing into a project.** It writes no instruction file, no settings file and no hook of its
  own, and changes none of the project's. What the session's agent itself writes while it works is the point of the
  whole thing and is not Octoboard's doing. Removing a project removes an association, never a directory. The one
  thing Octoboard does to a project directory on its own account, the user has to turn on first: with
  **Automatically sync repositories** on, a project's branch is fast-forwarded when it is behind its upstream, which
  moves its working tree (see "Automatically syncing repositories" in `docs/product/project-git-status.md`).
- **The user's own agent configuration is never written to.** Octoboard does not edit `~/.claude.json`,
  `~/.codex/`, `~/.grok/`, a directory chosen as one of a console's agent config directories, or anything else the
  agent reads as the user's global setup, and it never writes a trust decision into any of them.

Where Claude Code stops to ask whether to trust a folder, Octoboard does not touch a file either: it
answers the prompt on Claude Code's own screen, with the keystrokes a person would type — for a project
session only after the user has agreed to that in Octoboard, for a console session in the console's own
working directory without asking. Claude Code then records the answer in its own configuration, as it
does when a person answers. See "Claude Code's workspace-trust prompt" below.

Two settings of theirs are *read* at launch and never written: whether Claude Code has been trusted
with the project's directory, and whether Codex resolves approval requests by itself. Each one
changes what Octoboard can promise for that session (see "Per-agent specifics a user will notice"
below and "The raised hand" in `docs/product/sessions.md`). The Codex setting remains the user's to
change in Codex itself; Claude Code's trust is given by answering its trust prompt.

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
- **A role description** — whether this session is a console session or a project session,
  and the reporting conventions that go with that. The same role decides which tools the session is
  offered.

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

The same capture is what gives a GitHub clone the user's `git` and git credentials, so a clone is refused the same
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
- for every Grok Build session, `GROK_HOME`, set to the session's own Grok home (see "Grok Build" below). A Grok Build
  config directory is not passed as `GROK_HOME` itself.

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
`docs/product/consoles-and-projects.md`). This does not hold up the daemon's start: the application is served
immediately, and the result follows once it lands, typically within a couple of seconds and budgeted the same ten
seconds a launch's own snapshot is. Until it lands every agent reads as *not yet determined*, during which nothing is
refused for being unavailable — a launch whose agent turns out to have no binary is refused by the ordinary path
above, which already covers it. A snapshot that does not complete leaves availability not yet determined for the rest
of that run rather than marking every agent unavailable. With every agent determined unavailable, no session can be
opened at all; see "Opening a session" in `docs/product/sessions.md`.

## Per-agent specifics a user will notice

**Claude Code.** Octoboard assigns the session's id up front, so a session has an agent-side id from the moment it
starts. The flags that would drop the project's own permission rules, hooks or MCP servers are never passed.

When Claude Code has **not been trusted with the project's directory**, it ignores that project's own
`allow` permission rules for the session while still applying its `deny` rules — so the session is
only ever more restrictive, never less. Octoboard tells the user so once, when the session starts,
naming the project it is in: the `allow` rules stay ignored until Claude Code's trust prompt is
answered, and Octoboard answers that prompt once the user has agreed to it (see "Claude Code's
workspace-trust prompt" below). It stays silent unless the user's own configuration says
explicitly that the directory is untrusted, so a configuration it cannot read leaves them alone
rather than warning on every launch. The trust state is read from the global config file this launch's Claude Code
will itself read: `.claude.json` inside the Claude Code config directory in effect — the session's own, or else a
`CLAUDE_CONFIG_DIR` the user's shell exports — and `~/.claude.json` when there is none. A config directory without
that file yet produces no warning; `~/.claude.json` is not consulted in its place.

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
- the trust store: the project's directory is marked trusted in the copy, so the project's own instructions, hooks and
  MCP servers load — without writing a trust decision into the user's own store.

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

## Claude Code's workspace-trust prompt

The first time Claude Code runs in a directory it has not been trusted with, it stops on a screen of
its own asking whether to trust the folder — a two-option list, "No, exit" and "Yes, I trust this
folder", with the cursor starting on "No, exit" — and waits for a person. Until that screen is
answered the session does nothing else and reports nothing (see "What the statuses are derived from"
in `docs/product/sessions.md`). Octoboard recognises the screen and answers it for the user once they
have agreed to that. Codex and Grok Build sessions are unaffected.

What happens when the screen comes up depends on the session:

| Session | What happens |
|---|---|
| Console session | Answered at once, without a dialog and without recording anything: its working directory is the console's own, which holds nothing but the instruction file Octoboard writes there. |
| Project session whose project has the user's consent, or whose directory lies under a trusted folder | Answered at once, without a dialog. |
| Any other project session | The user is asked in a dialog; nothing is sent until they agree. |

**The dialog**, titled "Trust this folder?", opens by saying that Claude Code is asking whether to
trust this folder — for the named session, when the session is known — with the project's full path
set apart below in a monospace block. When "Trust parent folder" is offered, one sentence then says
that it also trusts every project in the folder that holds this one, named by its last two path
components, including projects added there later. A warning callout, tinted and set apart at the
end, says that a trusted folder's `.claude/settings.json` may pre-approve tool permissions. Its
buttons:

- **Trust and continue** — Octoboard answers this session's screen. Only if that succeeded is the
  project's consent recorded, so the project's later Claude Code sessions are answered without a
  dialog; an answer that fails records nothing.
- **Trust parent folder** — shown only when the daemon offers a folder for this project
  (see "Which folder" under "Trusted folders" below); a project with none to offer gets only the other
  two buttons. The button's tooltip gives the full path of the folder offered. Octoboard answers this
  session's screen, and only if that succeeded is that folder added to the trusted folders. The
  project's own consent is not recorded; an answer that fails records nothing. If the folder can no
  longer be offered by the time the button is chosen (see "Which folder" under "Trusted folders"
  below), it is refused before anything is answered: the dialog stays open and shows why, and the user
  can still choose another button.
- **Not now** — also what Escape, the dialog's close button and a click outside it do. Nothing is sent
  and nothing is recorded; the screen stays for the user to answer in the session's terminal. The same
  session is asked about again only if the application reloads its state (a reconnect, or catching up
  after falling behind) while the screen is still up, and a
  later session of the project is asked about again, since the project still has no consent.

Once either trust button has been chosen the dialog closes, whether or not the screen could be
answered — a screen is answered at most once, so trying again from the dialog could not succeed. A
failure is then shown as a toast. Two outcomes differ: the refusal of a folder that can no longer be
offered keeps the dialog open, and a go-ahead for a screen that is no longer waiting — already
answered, whether from another client, in the terminal or by Octoboard itself — closes the dialog
without any message when it was "Trust and continue", since nothing went wrong. For "Trust parent folder" it shows a
message saying the folder was not trusted, because that choice was not carried out.

Prompts are shown one at a time, oldest first; closing one brings up the next. A prompt still waiting
is dropped, without being answered, when its session stops running. When a folder becomes trusted,
the prompts waiting for projects under it are dropped as well, because Octoboard answers those screens
itself; prompts for other projects stay queued. Whenever the application reloads
its state (a connect, a reconnect, or catching up after falling behind), the daemon asks again about every screen still waiting, so a prompt the user never saw
comes back; with no client connected the screen simply waits for the user to answer it in the
terminal. Once one client has answered, a go-ahead from another changes nothing.

**Consent** comes in two forms, and either one is enough for Octoboard to answer without asking:

- **A project's consent** belongs to one project, is stored with it, and covers every later Claude
  Code session in that project, a resume included. It is set only by "Trust and continue" and cannot
  be withdrawn from the application; editing the project leaves it as it is, and removing the project
  removes it with the association. A project starts without it.
- **A trusted folder** covers every project whose directory is that folder or lies anywhere below it,
  in any console, including projects associated after the folder was trusted — by the user or by a
  console session. It is set only by "Trust parent folder", is stored on its own rather than with any
  project, and can be removed in Settings. Its rules are in "Trusted folders" below.

**How the screen is answered.** Octoboard types a Down and then an Enter into the session's terminal,
checking before each key:

- before the Down, that the screen as it was recognised is still showing and the cursor is on "No,
  exit";
- before the Enter, that the cursor has moved to "Yes, I trust this folder" and the terminal has gone
  quiet;
- before either, that nothing else has written into the session's input since the attempt began —
  the user typing in the terminal, or a message Octoboard delivers to the session. An attached
  terminal's own protocol replies, such as focus reports and answers to Claude Code's terminal
  queries, are not counted.

When any check fails the attempt stops without sending the next key and the screen is left for the
user. A session's screen is answered at most once per run, so a failed attempt is not retried; the user
is told in a notice on the session that Octoboard could not answer the screen and that they should
answer it in the terminal.

**Recognition is limited to the start of a session.** The screen is looked for only in a Claude Code
session's own terminal output, and only until the session's first hook report or until it has printed
64 KiB or run for 30 seconds, whichever comes first — the screen is the first thing Claude Code prints,
so the same words appearing later in a session are never taken for it.

### Trusted folders

**What trusting a folder grants.** A trusted folder is not limited to the projects present when it
was trusted. Every project associated under it later is covered as well, by whatever means it was
associated — **including repositories a console session clones or adds into that folder on its own** —
and Octoboard answers Claude Code's trust prompt for each of them without asking. Once that prompt is
answered, Claude Code applies the permission rules and hooks in that project's own
`.claude/settings.json` without asking either. Trusting a folder therefore means trusting whatever
ends up inside it, for as long as the folder stays trusted.

**Which folder.** The folder offered, and the only one that can be trusted from a session's dialog,
is the parent directory of that session's project; the daemon derives it from the project's path, and
no client can name a folder of its own. None is offered — and the "Trust parent folder"
button is not shown — when:

- the project's path is not absolute;
- the parent is the filesystem root, which includes a project directly inside `/`;
- the parent is the user's home directory or any directory that contains it, which includes a
  project directly inside the home directory. This is checked on the paths as written and also on
  the directories themselves, so a symbolic link to the home directory or to a directory above it, or
  a spelling that differs only in letter case on a volume that does not tell case apart, is caught as
  well;
- the home directory cannot be determined.

Such a project can still be trusted on its own with "Trust and continue". The same check is made
again when the button is chosen, and a folder that fails it then is refused before anything is
answered — with the error code `trust_directory_too_broad` when it is the root, the home directory or
a directory containing it, `trust_path_not_absolute` when the project's path is not absolute, and
`trust_home_unknown` when the home directory cannot be determined.

**Which projects it covers.** A project is under a trusted folder when its directory is that folder
or lies below it at any depth. Paths are compared component by component after a purely lexical
clean-up (`.` dropped, `..` folded into the component before it, trailing slashes ignored):

- `/work` covers `/work/app` and `/work/a/b/c`, but not `/work2` or `/workspace/app` — never a text
  prefix;
- symbolic links are not resolved: a project is covered when the path it was associated under lies
  below the folder as written. A project associated through a symbolic link inside a trusted folder is
  therefore covered wherever that link points, so a link inside a trusted folder extends the trust to
  its target; a project associated under a path outside the folder is not covered, however it is
  linked from inside;
- names are compared exactly, so a spelling of the same folder that differs only in letter case is
  not covered;
- only absolute paths are ever covered: a project recorded with a relative path is never under a
  trusted folder. Octoboard records every project it associates with an absolute path (see
  "Associating a project" in `docs/product/consoles-and-projects.md`).

**Trusting a folder answers what is already waiting.** At the moment a folder is added, every trust
screen still waiting in a project under it is answered, including one the user put off with "Not
now"; screens of projects outside it stay as they were and are still asked about. Each of these
answers is checked and reported exactly as described in "How the screen is answered" above.

**The list in Settings.** The Trusted folders section of Settings (see `docs/product/settings.md`)
explains what trusting a folder grants, as above, and lists every trusted folder, sorted by path. A
path too long for the row is cut from its start and fades out there, so the folder's own name stays
visible, and the full path is then the entry's tooltip. Each folder has a Remove button that stops
trusting it at once, without a confirmation. With no folder trusted, the section says so and that a
folder is trusted from a session's trust prompt. Removing a folder leaves every project's own consent as it is and leaves running
sessions alone, a screen already answered included; a trust screen that comes up afterwards in a
project under it, without consent of its own and not under another trusted folder, is asked about
again.

## Agent session data

Conversation history and session records belong to each agent and stay wherever that agent keeps them. Octoboard
never prunes, copies or deletes them; their retention and cleanup follow the agent's own rules.
