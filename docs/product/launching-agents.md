# Launching agents

Every session is a native agent CLI process. Octoboard supports three agents — **Claude Code**, **Codex** and
**Grok Build** — and starts each one's own binary, resolved from the user's `PATH`.

## What Octoboard never modifies

This is the guarantee the whole design rests on:

- **Octoboard installs nothing into a project.** Outside the Git operations below, it writes no instruction file,
  no settings file and no hook of its own, and changes none of the project's. What the session's agent itself writes
  while it works is the point of the
  whole thing and is not Octoboard's doing. Removing a project removes an association and leaves the directory
  alone, unless the user chooses to move the folder to the system Trash with it (see "Moving the project folder to
  the Trash" in `docs/product/consoles-and-projects.md`).
  Octoboard changes a project's files only through an authorized fast-forward, **Switch branch** or
  **Restore file changes**, and does not push except at the user's word. A branch that is
  behind its upstream and has no commits of its own is fast-forwarded, which moves its working tree — on its own
  once **Automatically pull repositories** has been turned on, or for one project when the user chooses **Pull
  current branch** (see "Automatically pulling repositories" and "Pulling one project by hand" in
  `docs/product/project-git-status.md`). Separately, **Push current branch** pushes that one branch. That updates
  the remote and, in the repository, the remote-tracking refs `git` updates for that push; when the branch had no
  upstream, it records one. It does not edit the project's files and does not move the working tree (see "Pushing
  the current branch" in `docs/product/project-git-actions.md`). **Restore file changes**, after confirmation,
  discards staged and unstaged tracked changes in the project's own worktree, restoring files to `HEAD` without
  moving the branch or changing commits (see "Restoring file changes" in `docs/product/project-git-actions.md`).
  **Switch branch** moves the project's own worktree to an existing local branch when chosen, preserving local
  changes Git can carry safely (see "Switching the local branch" in `docs/product/project-git-actions.md`).
  A check's fetch of remote refs is separate:
  it does not move the working tree and it does not push (see "What one check does" in
  `docs/product/project-git-status.md`).
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

Claude Code's folder trust and Codex's approval reviewer affect what Octoboard can promise for a session (see
"Per-agent specifics a user will notice" below and "The raised hand" in `docs/product/sessions.md`). Their
configuration files are read at launch and never rewritten. Claude Code's trust is given by answering its trust
confirmation. Codex's reviewer can be overridden for an account in Settings; see "Agent accounts" in
`docs/product/settings.md`.

Everything Octoboard adds is injected **per launch** and disappears with the process. In consequence, a project's own
configuration keeps working exactly as it does outside Octoboard: its instruction file, its skills, its hooks and its
permission rules all take effect, because the session runs with the project's directory as its working directory and
nothing suppresses the agent's normal discovery of them. A hook the project itself defines and Octoboard's own hook
both run, in the same session.

## What is injected on every launch

Three things, and nothing else, besides the variables a project session's agent is given for its git (see "The launch
environment" below):

- **Status hooks** — the events the session statuses in `docs/product/sessions.md` are derived from.
- **Octoboard's own MCP server** — the tools described in `docs/product/hub-orchestration.md`. It is
  a child process of the agent, started fresh per session, and each session's tools are reachable
  only by that session. The agent's own and the project's own MCP servers keep working alongside it.
- **A role description** — whether this session is a console session or a project session, and the reporting conventions
  that go with that. A console session's is short: it is the console session, it decomposes, dispatches, follows up and
  summarizes and does not change a project itself, and the rest is in its instruction file, which may end with the
  user's own instructions, which take precedence over the rest of the file wherever they disagree but never change
  what it does and does not do (see "The console session's instruction file" below). A project session bound to a console session is told that console session is waiting on its
  result and how to `report` to it; one bound to a project session is told which project session, by its id, dispatched it and
  that it reports there; either is told that its owner can hand it to another session, which Octoboard then tells it in
  a message. One nobody owns — opened by hand without a console session to report to, or started on another session's
  request — is told that no session dispatched it, that nobody but the user owns it, and not to call `report`; that it
  may start sessions in its own project and drive them, which report back to it; and that if it binds itself to an
  owner, that call's result tells it what changes. A lead session, bound to a console session only after it was opened,
  is given that same unbound project session's text on every launch, as at its first (see "Lead sessions" in
  `docs/product/hub-orchestration.md`). Every project session's text also sets out how it relates to other sessions:
  `start_session` for a session of its own where it has it, `transfer_session` and `bind_to_session` where it has them,
  `request_session` for a session that is nobody's, and `list_sessions` and `share_info` for information. The text is
  written from who the session reports to when it launches, and says that what Octoboard tells it of a later change of
  owner replaces what it says (see "When a binding changes while a session runs" in
  `docs/product/hub-orchestration.md`). The role and the session's binding as it stands decide which tools the session
  is offered, so an unbound project session still has `report`, and calling it is refused while it is unbound (see
  "Reporting" and "The unbound project session's tools" in `docs/product/hub-orchestration.md`).

The injected hooks are built to be invisible. They never steer the agent, print nothing (with the one exception
below), never fail the turn, and carry a short timeout (3 seconds) so a daemon that is unreachable costs a turn a
fraction of a second rather than stalling it. A Claude Code that can post hooks itself (2.1.63 or later) is given the daemon's address instead of a
command, so its hooks start no process; an older one, or one whose version Octoboard cannot tell from where it is
installed, gets the command. So does one whose settings restrict the addresses hooks may post to
(`allowedHttpHookUrls`, in the user's, the project's or the managed settings files) to a list that leaves out the
daemon's loopback address, since Claude Code would skip such hooks silently; a restriction delivered by other means
than those files is not seen. The one exception to printing nothing is the address form with a daemon that cannot be
reached or does not answer in time: Claude Code shows its own non-blocking notice about the failed hook, where the
command form stays silent.

**A session's role is fixed for its lifetime.** Two of the three agents record the injected role text
into the conversation on its first turn and replay it on every resume afterwards, so changing a
session's role later is not possible: a session that belongs on the other side of the orchestration
is a new session. For the same reason, a session opened to resume a conversation started outside Octoboard (see
"Resuming a conversation started outside Octoboard" in `docs/product/sessions.md`) may not take up its role
description with Claude Code or Grok Build, which keep what that conversation recorded; the status hooks and
Octoboard's MCP server are injected as on any resume.

A resume reassembles the full injection, because all three agents resolve hooks and MCP servers from the launch
arguments every time and a resume that omitted them would leave a session nobody can observe and nothing to report
with.

The model and reasoning effort resolved for the session are passed on the launch as well (see "The model and
reasoning effort" below), as is an explicit Codex account approval reviewer (see "Agent accounts" in
`docs/product/settings.md`). These are the user's choices carried to the agent, separate from the injected capabilities.

### The console session's instruction file

Octoboard writes one instruction file into the console's working directory, `AGENTS.md`, with `CLAUDE.md` beside it as
a relative symbolic link to it; a regular `CLAUDE.md` found there is replaced by the link. Claude Code reads
`CLAUDE.md`, Codex reads `AGENTS.md`, and Grok Build reads both and loads the linked pair once — a console's working
directory is not a git repository, and Grok reads the instruction files there once the folder is trusted, which
Octoboard's press of a console session's trust confirmation sees to (see `docs/product/folder-trust.md`). So console
sessions of every agent, running together or not, read the same text, and the file names Octoboard's tools without any
agent's prefix. Each console session's role description points it to the file by the name its own agent reads.

The file holds the console session's role — decompose, dispatch, follow up, summarize, and make no change to a project
itself, reading one being how it checks what its sessions report (see `docs/product/hub-orchestration.md`) — and how to
use the orchestration and settings tools. When the console has console session instructions of the user's (see
"Console session instructions" in `docs/product/consoles-and-projects.md`), they are the file's last section,
held between a `<user_instructions>` and a `</user_instructions>` tag under a heading of the file's own.
That heading's paragraph says the tags hold the user's own text, that any heading inside them belongs to that text and
not to the file's sections, and that the console session follows the user's text wherever it disagrees with the
built-in part, except that it cannot change what the console session does and does not do — the console session still
never changes a project itself. The text is kept as written, except that a closing `</user_instructions` of its own, in
any letter case, has its `<` written as `&lt;`, so it cannot end the user's part early. A console without any has no
such section.

The file is written:

- **when the console is created** — a console whose file cannot be written is not created;
- **after an edit to the console is saved** — a failure to write it is logged and leaves the saved edit standing, and
  the next console session launch writes the file;
- **before every console session launch**, a resume and an account switch included — a failure to write it fails the
  launch.

Edits made to the file by hand therefore do not survive the next write. None of these writes recreates a missing
console working directory: a console session launch into one is refused instead.

An edit to the console's console session instructions reaches every agent's console sessions begun after it. A Claude
Code or Codex console session resumed after it reads it too (on Codex 0.142.5, only after a compaction), while a Grok Build
console session never does within its life, resumed or not: only a new one does. Codex reads at most 32 KiB of a
project's instruction files by default (its own `project_doc_max_bytes` setting), so a very long text can be cut off
for Codex console sessions; Octoboard sets no limit on the text itself.

## The model and reasoning effort

The session's model and reasoning effort are resolved under the inheritance and override rules in
`docs/product/model-and-reasoning-effort.md`. Each resolved value is passed as that agent's own launch argument, on
every launch, a resume and an account switch included, and nothing else about the launch changes. A value left to
the agent's own default produces no launch argument for that value.

Octoboard checks neither value against anything at launch. Which models and which effort levels exist is the agent's
own business — the dialogs that set them offer what the agent itself answers, and the agent is what answers for a
value it does not know anyway, in the session's own output, like any other complaint it has about its arguments. What
the three do differs, and is worth knowing before a value is set:

- **Claude Code** silently lowers an effort the chosen model does not support down to `high`, rather than refusing it.
- **Codex** refuses an effort the chosen model does not advertise, with an error of its own naming that model.
- **Grok Build** takes both as flags. Because its session runs against a per-session copy of the Grok configuration
  file, neither value, and no change the session makes to them while it runs, reaches the user's own Grok
  configuration (see "Per-agent specifics a user will notice" below).

## The launch environment

The agent is launched directly, not through a shell, with an environment captured from the user's **login +
interactive** shell. That is what makes agents installed under a version manager or in `~/.local/bin` findable, and
it is what carries the user's API keys and other settings into the session. Starting that shell can take seconds, so
a launch does not wait for one when it can avoid it: it uses the most recent capture Octoboard holds (the daemon's own
at start-up, or one taken since) while that capture is under ten minutes old and every `PATH` directory it found still
exists, and, when that capture is more than a minute old, captures afresh in the background for the launches after
it. A change to the user's shell configuration is therefore picked up without a restart, though not necessarily by
the very next launch: in steady use by the one after it, and in any case once the capture in hand is ten minutes old.
When there is no usable capture the launch waits for a fresh one, as below.

A new session is listed before any of that has happened: it is recorded and shown as *starting* immediately, and the
launch — the environment capture, the injected files and the agent process — carries on behind it, so a slow capture
holds up nothing the user sees. What a *starting* session allows, and what happens to it when the launch is refused for
any reason named below, is in "Opening a session" and "Session statuses" in `docs/product/sessions.md`.

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
- for a project session, a `core.hooksPath` entry through `GIT_CONFIG_COUNT`, together with `OCTOBOARD_SESSION_ID` and
  `OCTOBOARD_DAEMON_PORT`, which point every git in the agent's process tree at Octoboard's hook dispatchers; those
  still run the user's own git hooks, and report the branches that git creates (see "The session's git hooks" and
  "What attribution needs" in `docs/product/branch-creator.md`). A console session gets none of them.

The agent binary is resolved from that same snapshot's `PATH`, which is that login + interactive shell's `PATH`, not the
one a terminal happens to have. So when the shell lists an old install of an agent before a newer one, the old one is
launched. Seen with Codex: a Finder-style launch resolved an older Homebrew build ahead of the current one in
`~/.local/bin`. That Codex was older than the async hook support Octoboard injects (inferred from its "skipping async
hook" warning; the session ended on an API error before status could be observed), and such a Codex cannot supply hooks
(see "Codex" in `docs/agent-cli-reference.md`). The cause is the user's `PATH` order, not Octoboard.

Which directory a session holds, and the refusal of a launch whose directory no longer exists, are described in "Agent
config directories" in `docs/product/consoles-and-projects.md`.

**What a session may do under macOS's privacy rules is Octoboard's, not the agent's.** macOS attributes a privacy
request made by a session's process — taking a screenshot, driving another application's interface — to Octoboard, so
the session acts under Octoboard's own grants and a grant the user gave some other application does not count. The two
permissions this concerns are granted ahead of time in Settings; see "Permissions" in `docs/product/settings.md`.

**Whether an agent is installed at all is worked out once, not per launch.** Right after the daemon starts it takes
one login-shell snapshot of its own — the same kind described above — resolves each agent's binary against that
snapshot's `PATH`, and reads what each agent's default account resolves to from it (see "Agent config directories" in
`docs/product/consoles-and-projects.md`). A binary that resolves is all "available" means: whether an account's
directory holds a login is never checked, since a user may sign in through an API key from their shell, a credential
helper or an organization's gateway, none of which shows in the directory. This does not hold up the daemon's start:
the application is served immediately, and the result follows once it lands, typically within a couple of seconds
and budgeted the same ten seconds a launch's own snapshot is. That snapshot is also the one the first launch can
reuse. Until it lands every agent reads as *not yet
determined*, during which nothing is refused for being unavailable — a launch whose agent turns out to have no binary
is refused by the ordinary path above, which already covers it. A snapshot that does not complete leaves availability
not yet determined for the rest of that run rather than marking every agent unavailable. With every agent determined
unavailable, no session can be opened at all; see "Opening a session" in `docs/product/sessions.md`.

## Per-agent specifics a user will notice

**Claude Code.** Octoboard assigns the session's id up front, so a session has an agent-side id from the moment it
starts; a session opened to resume a conversation has that conversation's id. The flags that would drop the project's own permission rules, hooks or MCP servers are never passed.

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
submission — so a session opened without a task has no agent-side id until the user types something, unless it was
opened to resume a conversation, whose id it has from the start. The launch
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
- Codex's own time limit on a call to Octoboard's tools (300 s by default) is raised above Octoboard's own limit on a
  forwarded call (the 10 minutes in "Waiting for the answer" of `docs/product/session-requests.md`), so that a call
  that waits for the user, such as a session request, can return its result. This is the limit on Octoboard's tools
  alone; no other tool's limit changes.

An explicit Codex approval reviewer chosen for the account in Settings takes precedence on every launch, a resume
included. When the account follows its configuration, whether Codex resolves approval requests by itself is read
from `config.toml` in the Codex home that launch uses: the session's Codex config directory, else a `CODEX_HOME` the
user's shell exports, else `~/.codex`. The account choices and their lifetime rules are in "Agent accounts" in
`docs/product/settings.md`.

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

Octoboard's own MCP server is declared in that copy of the configuration file, and its entry sets **Grok Build's own
time limit on a call to Octoboard's tools** above Octoboard's own limit on a forwarded call (the 10 minutes in
"Waiting for the answer" of `docs/product/session-requests.md`), so that a call which waits for the user, such as a
session request, is ended by Octoboard's limit and not by Grok's — Grok cancels a call it gives up on, and a
cancellation withdraws such a request. The limit is set on Octoboard's own entry alone; no other MCP server of the
user's is touched, and the user's own configuration is untouched because only the copy is written.

A Grok Build config directory must **already be a Grok home, one Grok has been run against**. Only entries
that already exist in the source home are linked; for an empty directory, Grok creates its login and session records
inside the per-session home, which is discarded with the session's process, so neither the login nor the conversation
persists. Octoboard does not check this when the directory is set — a user pointing it at a directory they are about
to create should not be stopped. It is checked at **launch** instead, and a Grok Build session pinned to a directory
that is not an initialized Grok home is refused there, naming the directory and what is wrong with it; a session on
the default account is never held to this, since that directory is the user's own existing setup.

Grok Build loads **a project's own hooks only inside a git repository**: it locates a project by walking up for a
`.git` directory, and in a directory without one it loads none of the project's hooks. Its instruction files need no
repository — in a directory that is not one, Grok reads those in the working directory alone — but, like the project's
hooks and MCP servers, they load only once the folder is trusted (see `docs/product/folder-trust.md`). That is how a
Grok console session reads the console's instruction file (see "The console session's instruction file" above).

## Agent session data

Conversation history and session records belong to each agent and stay wherever that agent keeps them. Their
retention and cleanup follow the agent's own rules, except for the automatic and opt-in moves to the system Trash
when deleting sessions described in "Moving the agent's own records to the Trash" in
`docs/product/deleting-sessions.md`. Octoboard copies a session's record only when the
user switches that session to another account of the same agent: the session's own record, and no other, is added to the
target account's directory so the conversation can continue there. Copying leaves the record in the account it came
from as well, so switching back needs no second copy; and a record Octoboard copied is the agent's own from then on,
retained and cleaned up by the agent's rules like any other.
