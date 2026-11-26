# Launching agents

Every session is a native agent CLI process. Octoboard starts the selected agent’s binary, resolved from the user’s `PATH`. The eight agents and their order are listed in `docs/product/supported-agents.md`.
What each agent's launch does differently is in `docs/product/per-agent-launch-specifics.md`.

## What Octoboard never modifies

Antigravity CLI model listing uses a private directory. Starting a session can create Antigravity CLI’s standard
`antigravity-cli` and `config/projects` directories under `~/.gemini`. Octoboard starts no migration of that directory.
See "Antigravity CLI" in `docs/product/per-agent-launch-specifics.md` for how to prepare an earlier layout.

Octoboard leaves the user's own system — the agents' configuration and everything else outside Octoboard's own
directories — as it finds it, apart from what this section lists. What Octoboard does inside a project the user added
and in its repository — the project's files, git operations, a session's branches and worktrees — is development work,
outside that promise; the first point below says what Octoboard itself does there.

- **Octoboard installs nothing into a project.** Outside the Git operations below, it writes no instruction file,
  no settings file and no hook of its own, and changes none of the project's. What the session's agent itself writes
  while it works is the point of the
  whole thing and is not Octoboard's doing. Removing a project removes an association and leaves the directory
  alone, unless the user chooses to move the folder to the system Trash with it (see "Moving the project folder to
  the Trash" in `docs/product/removing-projects.md`).
  In the project's own worktree, Octoboard changes files only through an authorized fast-forward, a merge the user
  confirmed, **Switch branch** or **Restore file changes**, and does not push except at the user's word. A branch that
  is behind its upstream and has no commits of its own is fast-forwarded, which moves its working tree — on its own once
  **Automatically pull repositories** has been turned on (see "Automatically pulling repositories" in
  `docs/product/project-git-status.md`), or for one project when the user chooses **Pull current branch**. A branch that
  has diverged from its upstream is changed only by Pull current branch, and only once the user confirms: its upstream
  is merged in, which creates a merge commit on the branch and moves its working tree (see "Pulling the current branch"
  in `docs/product/project-git-actions.md`). Separately, **Push current branch** pushes that one branch. That updates
  the remote and, in the repository, the remote-tracking refs `git` updates for that push; when the branch had no
  upstream, it records one. It does not edit the project's files and does not move the working tree (see "Pushing the
  current branch" in `docs/product/project-git-actions.md`). **Restore file changes**, after confirmation, discards
  staged and unstaged tracked changes in the project's own worktree, restoring files to `HEAD` without moving the branch
  or changing commits (see "Restoring file changes" in `docs/product/project-git-actions.md`). **Switch branch** moves
  the project's own worktree to an existing local branch when chosen, preserving local changes Git can carry safely (see
  "Switching the local branch" in `docs/product/project-git-actions.md`). A check's fetch of remote refs is separate: it
  does not move the working tree and it does not push (see "What one check does" in
  `docs/product/project-git-status.md`). Archiving or deleting sessions removes the branches they worked on and the
  linked worktrees that have them checked out, where git can do so safely (see
  `docs/product/branch-and-worktree-cleanup.md`).
- **What Octoboard gives an agent reaches it on each launch, and only there.** Its hooks, its MCP server, the role
  description and the user's choices it carries (see "What is injected on every launch" below) are passed as launch
  arguments, environment variables, or files Octoboard writes in its own directory, `~/.octoboard` (see "How each agent
  is given the injection" in `docs/product/per-agent-launch-specifics.md`). Octoboard does not edit `~/.claude.json`,
  `~/.codex/`, `~/.grok/`, an account's config directory, or anything else the agent reads as the user's global setup,
  adds no hook of its own to the user's settings, and makes no trust decision of its own in any of them other than a
  console's own directory, whose trust the agent then records as it does for any answer (see below). Nothing a
  session changes in the files Octoboard wrote for its launch is carried back into the user's own, except Grok Build's
  trust entry and Grok Build's settings saved in a session, both listed below.
- **Octoboard's own writes into the user's agent configuration are the operations listed below**, each with what sets
  it off.

**What an agent writes for itself is not Octoboard's change.** An agent's CLI writes as it does in the user's own use
of it: its conversation records, its caches and logs, a login's credentials, and the trust it records when its own
trust confirmation is answered. So do the commands of an agent's CLI that Octoboard runs as part of what the user asked
for, the agent doing what the user asked: an account's login (see `docs/product/logging-accounts-in.md`), an MCP
server's sign-in or sign-out (see "Signing an account in to and out of an MCP server" in
`docs/product/account-mcp-servers-and-connectors.md`), a CLI update (see "Updating" in
`docs/product/agent-cli-updates.md`), and, before a conversation is resumed, `codex unarchive` for a Codex conversation
Codex itself archived and stopping a Claude Code background session that holds the conversation (both in "Archiving,
interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`).

**Octoboard's own background runs of an agent's CLI** — reading an account's quota, starting a usage window and the
other short runs listed in "How a run of an agent's CLI is stopped" in `docs/product/account-quota-readings.md` — leave
behind what "What reading a quota leaves behind" in `docs/product/account-quota-readings.md` and "What a kick leaves
behind" in `docs/product/starting-usage-windows.md` describe. Grok Build's quota reading, model listing and kick run in
a directory of Octoboard's own and leave the account's `config.toml` as it was (see "Where Grok Build's readings run"
in `docs/product/account-quota-readings.md`). In a config directory Claude Code has not run in before, Claude Code's
quota reading, `claude mcp list` and `claude agents --json` carry out Claude Code's own first-run set-up there, which
changes its configuration files as its next launch by the user would have (see "Claude Code's first-run set-up" in
`docs/product/account-quota-readings.md`). Codex's quota reading, model listing and MCP inventory change no
configuration file of Codex's.

Where an agent stops to ask whether to trust a folder, Octoboard decides nothing about a project on its behalf. In a
project session it presses the agent's own confirmation, with the keystrokes a person would type, only after the user
has given Octoboard permission, and the agent records the trust in its own configuration, as it does when a person
answers. A console session runs in a directory of Octoboard's own, whose trust is Octoboard's to give: Claude Code's
and Codex's confirmations there are pressed without asking, and the agent records the trust as for any answer — an
entry naming the console's directory in Claude Code's trust list, or, only where a git repository holds that
directory, an entry for the repository's root in Codex's `config.toml` — which stays after the console is deleted and
after Octoboard is uninstalled. Grok Build is trusted there for the launch alone, recording nothing in the user's trust
list. See "Console sessions' trust confirmations" in `docs/product/folder-trust.md`.

A macOS run on 2026-11-24 opened consoles on signed-in accounts in a local build with its own app identifier and
daemon data directory. Without keyboard input, Claude Code 2.1.330 reached its composer and its `.claude.json`
gained `hasTrustDialogAccepted = true` for the newly created console directory. The terminal observer first saw
the composer after the confirmation had already cleared. Grok Build 1.0.92 reached its composer without a trust
confirmation and its console entry was present in the session's copy of `trusted_folders.toml`. The source
`trusted_folders.toml`, Grok Build's `config.toml` and login store, and Claude Code's settings files stayed
byte for byte unchanged. The agents also wrote their ordinary runtime state, including Claude Code's startup
and project session metadata, and Grok Build's session files and tip cursor.

**The operations that write into the user's agent configuration**, each with what it writes and where:

- **Switching a session's account** — the user's **Switch account**, after its confirmation; an owner's switch, once
  the user approves it or under **Automatically approve account switches**, which the user turned on; or the automatic
  switch at the usage limit, while the user has **Automatically switch accounts at the usage limit** on (see
  `docs/product/switching-session-accounts.md` and `docs/product/automatic-account-switch.md`). It copies that one
  session's conversation — its record and what the agent keeps of the conversation beside it — into the target
  account's directory, adding it there or replacing an earlier copy, by way of a transient `.octoboard-switch`
  directory at the root of the target directory, so a failure never leaves a half-written item among the agent's own.
  For a Codex session it also sets the `rollout_path` the target account's thread database keeps of that thread to the
  carried head, where it leads to another file, and changes nothing else in that database: Codex resumes the rollout
  the database names, so a target account that held the thread earlier would otherwise resume an older branch of it.
  If that cannot be done, the switch fails before the session is relaunched. The switch touches no settings file of
  the agent's and nothing of other sessions in either directory, and leaves the originals where they were. What is
  copied for each agent, and how a failed copy is undone, are in "Carrying the conversation" in
  `docs/product/switching-session-accounts.md`.
- **Sharing the default configuration** — the account's **Share default configuration** switch in Settings. Turning
  it on writes into both directories: in the sharing account's, the shareable entries are replaced by links into the
  default account's directory and its own configuration is set aside as backups; in the default directory, the
  account's conversation history is moved in, its prompt histories (`history.jsonl`) and Codex's thread-name index
  (`session_index.jsonl`) are appended to the default directory's, its Codex thread rows are carried into the default
  directory's thread database, added or pointed at the account's rollouts (not on Windows, where that database is not
  shared), and a directory the default directory lacks is created there, empty. Before each of the account's sessions
  launches, the links are repaired under the same rules. The paths the default directory's plugin registries record
  through the account are pointed at the default directory's own after each of the account's sessions ends, before each
  of its launches, as the daemon stops and once at daemon start (see "Plugin registries kept leading into the default
  directory" in `docs/product/sharing-the-default-configuration.md`). Turning sharing off removes the links and
  restores the backups, points what the default directory's Codex thread database and plugin registries record through
  the account at the default directory's own, and moves the conversations of the account's own Octoboard sessions back
  into the account, the shared thread database forgetting the threads moved out and their names copied into the
  account's `session_index.jsonl`; every other conversation stays in the default directory. The detail of each is in
  `docs/product/sharing-the-default-configuration.md`.
- **Grok Build's trust entry for a project's folder** — once Grok's own trust confirmation has been accepted in a
  project session: pressed by Octoboard after the user agreed in the trust dialog or under the trust permission the
  user gave, or answered by the user in the session's terminal. The one entry Grok wrote into its session's copy of
  the trust store is carried, unchanged, into `trusted_folders.toml` in the session's source Grok home, every other
  entry there left as it is (see "Carrying Grok Build's trust record" in `docs/product/folder-trust.md`). Nothing is
  carried for a console session, whose directory is trusted in the session's own copy of the store alone, for that
  launch (see "Console sessions' trust confirmations" there).
- **Grok Build's settings and plugin installs saved in a session** — a session whose source `config.toml` is a link to
  a file: a theme, plugin switch or default model changed in the session is written, when the process exits, to the
  file the link leads to, as Grok itself would have saved it, and the link is left in place; the plugin switches are
  left out where the session's plugins were its per-session home's own and went with it. A session whose source
  Grok home has an `installed-plugins/` directory: a plugin installed in the session is recorded by Grok in that
  directory's `registry.json` through the session's per-session home, and the `path` of each such entry is rewritten
  to the source home's own `installed-plugins/` — when the process exits, and, for entries an earlier process of the
  session or a session whose per-session home no longer exists left, before each launch — every other entry and the
  rest of the registry left as they are (both in "The per-session Grok home" in
  `docs/product/per-agent-launch-specifics.md`).
- **Moving the agent's own records to the Trash** — deleting one session, for the sessions it removes that have no
  conversation yet and whose records, read first, hold no prompt sent to them; a deletion whose confirmation's option
  to move the records is ticked, for every session it removes. The agent's own records of those sessions are moved
  from the accounts' directories into the system Trash. No other deletion moves a used session's records, even when the agent itself no longer has the conversation: the automatic
  deletion of archived sessions whose conversation is gone, and reopening a session whose archived owner's conversation
  is gone, leave them where they are. Which records are taken, and from which accounts, is in "Moving the agent's own
  records to the Trash" in `docs/product/deleting-sessions.md`.

Creating a Codex account's directory that does not exist yet, empty, before its first launch or sign-in is not one of
these: Codex refuses to start without it, and nothing of the user's is in it.

Outside the agents' configuration, the desktop application and the server form put these in place:

- at the user's word: the command-line link, made by **Install** and removed by **Remove** (see "Command-line tool" in
  `docs/product/general-settings.md`); the login entry, macOS's login item or the Windows `Run` value, added and
  removed by **Launch at login** (see `docs/product/launch-at-login.md`); and, through the server form's install
  script, the `octoboardd` executable in its install directory and, with `--service`, the systemd user unit in the
  user's systemd configuration directory and lingering for the user, turned on with `loginctl enable-linger` (see
  "Installing it as a service" in `docs/product/installing-the-server-form.md`); and a key of this computer's own on a
  remote daemon's host, one line in the remote user's authorized keys, added after a password or key-file sign-in only
  once the owner answers **Add key** to the window's question, and taken back from the host, with its private half from
  the credential store, when it is turned off on the device's page, the saved daemon is removed, or its host, port or
  user changes (see "Device keys" in
  `docs/product/remote-daemons.md`);
- with the server form's updates: `octoboardd.previous` beside its executable (see
  `docs/product/files-octoboard-owns.md`);
- by itself: the Windows taskbar's jump list of the remote daemons (see "Where remote daemons are reached from" in
  `docs/product/remote-daemons.md`).

Claude Code's folder trust and Codex's permission preset affect what Octoboard can promise for a session (see
`docs/product/per-agent-launch-specifics.md` and "The raised hand" in `docs/product/session-statuses.md`). Their
configuration files are read at launch and never rewritten. Claude Code's trust is given by answering its trust
confirmation. For agents that declare permission choices, Settings can override the choice for every account of the agent; see "Approval modes" in `docs/product/per-agent-settings.md`.

An agent may retain native session settings in its saved conversation: Codex's Custom permission choice preserves
saved permissions on resume (see "Approval modes" in `docs/product/per-agent-settings.md`). A project's own
configuration still loads as it does outside Octoboard: its instruction file, skills, hooks and permission rules take
effect, because the session runs with the project's directory as its working directory and nothing suppresses the
agent's normal discovery of them. A hook the project itself defines and Octoboard's own hook both run, in the same
session.

**What an uninstall leaves.** The injection needs no uninstall: none of it was ever written into the user's or an
agent's files, so an agent run on its own after Octoboard is gone starts without Octoboard's hooks, MCP server or role
description. What the operations above wrote stays as it was left, the agent's or the user's own from then on. What
stays of everything this section lists once Octoboard is uninstalled — in Octoboard's own directories, in the agents'
files and elsewhere on the system — with where it is and how to remove it by hand, is in
`docs/product/what-remains-after-uninstalling.md`.

## What is injected on every launch

The following capabilities, and for Claude Code a status line besides (see "Claude Code's status line" below), plus
the launch environment described below:

- **Status hooks** — the events the session statuses in `docs/product/session-statuses.md` are derived from.
- **Octoboard's own MCP server** — the tools described in `docs/product/hub-orchestration.md`. It is a child process of
  the agent, started fresh per session, and each session's tools are reachable only by that session. The agent's own and
  the project's own MCP servers keep working alongside it. Claude Code and Codex are told to run its tools without
  asking (see "Claude Code" and "Codex" in `docs/product/per-agent-launch-specifics.md`).
- **A role description** — whether this is a console or project session, and, for a project session, its coordinator or
  worker role and separate reporting relationship. Every project session's description opens with its responsibility for
  the current project's tasks under the project's own instructions, conventions and configuration. It then explains its
  reporting relationship; an independent session's coordination capabilities follow in a separate section. The
  description includes the corresponding reporting conventions. A console session's is short: it is the console session,
  it decomposes requests on its console's projects, dispatches them, follows up and summarizes, does itself any work
  that touches no project, and does not change a project itself, and the rest is in its instruction file, which may end
  with the user's own instructions, which take precedence over the rest of the file wherever they disagree but never let
  it change a project itself (see "The console session's instruction file" below). A worker bound to a project session
  is told which project session, by its id, dispatched it and that it reports there, and a worker bound to a console
  session that the console session is waiting on its result; either is told that its owner can hand it to another
  session, which Octoboard then tells it in a message. A coordinator with a console parent is told the same of that
  console session and how to `report` to it, besides its coordination capabilities, from its first launch when the
  console session opened it, and from the first launch after the binding when it was bound later (see "Coordinators with
  a parent session" in `docs/product/hub-orchestration.md`). One nobody owns — opened by hand without a console session
  to report to, or started on another session's request — is told that no session dispatched it, that nobody but the
  user owns it, and not to call `report`; that it may start sessions in its own project and drive them, which report
  back to it; and that if it binds itself to an owner, that call's result tells it what changes. The text of a worker,
  or of a coordinator with a console parent, also tells it how to find its own account and to report when its quota runs
  low (see `docs/product/listing-agents-from-a-session.md`). Every project session's text also sets out how it relates
  to other sessions: `open_session` for a session of its own where it has it, together with when to start one and how to
  choose the session a task goes to (see "Choosing the session a task goes to" in `docs/product/hub-orchestration.md`),
  `handoff_session`, `switch_session_account`, `transfer_session`, `release_session` and `bind_to_session` where it has
  them, `request_session` for a session that is nobody's, `request_resume_session` for resuming a conversation the user
  asked for, `list_agents` for the agents a session could be opened on, with their tags, and what each account's quota
  has left, the reads every session has whatever its kind — `get_session`, `read_session_history`, `list_consoles`,
  `list_projects` and `list_archived_sessions` — with the rule that a session not its own is to be left alone whatever
  it can read of it, `list_agent_models` to check before naming a model or effort, `list_sessions` and `inform_session`
  for information, `list_session_branches` for where the branches a session is responsible for are checked out and which
  branch each is to be merged into, `list_project_branches` for a project's branches with the session responsible for
  each, and `list_project_changes` and `read_project_diff` for what has changed in a project of its console — to be
  used, rather than `git status` or `git diff`, for another session's worktree or branch and for the project's folder,
  which another session may be working in, while in its own worktree it runs `git` as usual (see "Reading a project's
  changes" in `docs/product/what-every-session-reads.md`). While Octoboard's memory is on at the launch, it next carries
  a section on its project's memory, with that memory's index as it stands at the launch (see "The index a session
  starts with" in `docs/product/memory.md`). Where its project's effective git workflow settings have
  sessions develop on a branch or in a worktree of their own at the launch, it also carries the development rule (see
  "The development rule" in `docs/product/git-workflow-settings.md`), where **Ask questions as choices** is on at the
  launch, the guidance on asking the user after it (see "The guidance on asking the user" below), and where the language
  the daemon holds is not English, the guidance on the user's language after that (see "The guidance on the user's
  language" below). Every project session's text then says that everything else is the ordinary work of the project,
  under its own instructions, conventions and configuration. Every role description, for console and project sessions
  alike, ends with a separated section on the scope of host instructions: they govern the assistant's behavior without
  establishing the user's task context. Requests are interpreted through the current project, where applicable, and the
  conversation; a concept mentioned in the host instructions alone does not make a request about the host unless the
  user explicitly refers to it. Instruction priority resolves conflicts without determining what the user's request
  refers to. The text is written from who the session reports to when it launches, and says that what Octoboard tells it
  of a later change of owner replaces what it says (see "When a binding changes while a session runs" in
  `docs/product/changing-session-owners.md`); a change made while the session was not running is told at its next
  launch, ahead of that launch's own instruction (see "A change of owner told at the next launch" in
  `docs/product/changing-session-owners.md`). The role and the session's binding as it stands decide which tools the
  session is offered, so an independent project session still has `report`, and calling it is refused while it is
  independent (see `docs/product/reporting-to-owners.md` and "Project coordinators' tools" in
  `docs/product/hub-orchestration.md`).

The arguments and files through which each agent is given these are in "How each agent is given the injection" in
`docs/product/per-agent-launch-specifics.md`.

The injected status hooks are built to be invisible. They never steer the agent, print nothing (with the one exception below),
never fail the turn, and carry a short timeout (3 seconds; a command hook gets 15 on Windows, where an agent's first
hook of a launch starts cold and slowly, except Codex's `Interrupt`, which keeps 3) so a daemon that is unreachable costs a turn a fraction of a second rather than stalling it.
The hooks, like the session's other helpers, call the process holder that runs the session rather than the daemon,
with a key of the launch's own, and the holder answers a report at once even while no daemon is connected (see
"Session helpers and the holder" in `docs/product/process-holders.md`). A
Claude Code that can post hooks itself (2.1.63 or later) is given that address instead of a command, so its
hooks start no process; an older one, or one whose version Octoboard cannot tell from where it is installed, gets the
command. So does one whose settings restrict the addresses hooks may post to (`allowedHttpHookUrls`, in the user's, the
project's or the managed settings, read where "The default model" in `docs/product/model-and-reasoning-effort.md` says)
to a list that leaves out that loopback address, since Claude Code would skip such hooks silently; a
restriction delivered by other means than those is not seen. The one exception to printing nothing is the address form
with an address that cannot be reached or does not answer in time: Claude Code shows its own non-blocking notice about
the failed hook, where the command form stays silent.

A project session launched while **Keep sessions to their own project** is on is also given the cross-project guard's
hook, which a console session never is (see "The edit check" in
`docs/product/keeping-sessions-to-their-own-project.md`). Unlike the status hooks it is synchronous, so the agent waits
for it before each file-edit call, and has a timeout of its own, **10 seconds** on every host. It is always a command,
never an address, whatever form the status hooks take, since it reads a list the launch writes into the session's
scratch directory and calls neither the holder nor the daemon. It prints nothing unless it refuses the call, and a
failure of its own, its timeout included, lets the call through.

**The launch description can remain in the conversation after the role changes.** Claude Code and Grok Build record that text on the first turn and replay it on every resume. The coordinator or worker role and reporting relationship can
still change: Octoboard's notices supersede the earlier description, and tools follow the current relationship (see
"When a binding changes while a session runs" in `docs/product/changing-session-owners.md`). For the same reason, a
session opened to resume a conversation started outside Octoboard (see
`docs/product/resuming-a-conversation-by-its-session-id.md`) may not take up its role description with Claude Code or
Grok Build, which keep what that conversation recorded; the status hooks and Octoboard's MCP server are injected as on
any resume.

A resume reassembles the full injection, because the agents resolve hooks and MCP servers from the launch
arguments every time and a resume that omitted them would leave a session nobody can observe and nothing to report
with.

The model and reasoning effort resolved for the session are passed on the launch as well (see "The model and reasoning
effort" below), as is an explicit permission preset or mode chosen for the agent (see
"Approval modes" in `docs/product/per-agent-settings.md`), and, while one of the agent's compatibility switches is on
for the session (see "Which value a launch uses" in `docs/product/compatibility-switches-in-launch-settings.md`),
what has the session load another agent's configuration: Claude Code's for Codex and Grok Build, Codex's for Claude
Code and Grok Build — MCP servers, skills and a section added to the role description (see "Claude compatibility" and
"Codex compatibility" in `docs/product/per-agent-settings.md`, and `docs/product/compatibility-between-agents.md`).
These are the user's choices carried to the agent, separate from the injected capabilities.

### The console session's instruction file

Octoboard writes the shared instruction text into `octoboard-instructions.md` in the console's working directory,
then exposes it under each agent's instruction filename. On a host that makes file links, `CLAUDE.md` and `AGENTS.md`
are relative symbolic links to that file. A regular file or a link to another target at either name is replaced.
Where the host makes no file links (Windows without Developer Mode, not elevated), `CLAUDE.md` is instead a regular
file holding the single line `@octoboard-instructions.md`, which Claude Code expands into the text, and `AGENTS.md`
holds a copy of the full text. Each write refreshes that copy. A host that gains or loses file links gets the form it
can use, replacing the files or links left there.

Claude Code reads `CLAUDE.md`, Codex reads `AGENTS.md`, and Grok Build reads both: with links it loads the shared text
once, and without them it reads the full copy and takes the import line as written. A console's working directory is
not a git repository, and Grok reads the instruction files there once the folder is trusted, which the entry each
Grok console launch writes into its session's own copy of the trust store sees to (see "Console sessions' trust
confirmations" in `docs/product/folder-trust.md`). Console sessions of every
agent, running together or not, read the same instruction text, and the file names Octoboard's tools without any
agent's prefix. Each console session's role description points it to the file by the name its own agent reads.

The file holds the console session's role (see the console session in `docs/product/hub-orchestration.md`) in two
sections: one on what it does and does not do — decompose, dispatch, follow up and summarize work on the console's
projects, and make no change to a project itself, reading one being how it checks what its sessions report, with
`list_project_changes` and `read_project_diff` rather than `git status` or `git diff` for what a session has changed —
and a
separate one on work that touches no project, which it carries out itself. The file also holds how to use the
orchestration and settings tools, and, when **Ask questions as choices** is on as the file is written, the guidance on
asking the user (see "The guidance on asking the user" below) after the rest of the built-in part, with one sentence
more: a decision among a few options goes through that question tool rather than a page pushed with `show_page`, which
is for a choice that needs a page — a rich comparison, many items, a form. When the language the daemon holds is not
English as the file is written, the guidance on the user's language (see "The guidance on the user's language" below)
follows. When Octoboard's memory is on as the file is written, a section on the console's memory, holding its index as it
stands then, follows as the built-in part's last section, and the file's list of tools names the memory tools; while it
is off the file says nothing of memory (see "The index a session starts with" in `docs/product/memory.md`). When the
console has console session instructions of the user's (see
"Console session instructions" in `docs/product/consoles.md`), they are the file's last section, held between a
`<user_instructions>` and a `</user_instructions>` tag under a heading of the file's own. That heading's paragraph says
the tags hold the user's own text, that any heading inside them belongs to that text and not to the file's sections, and
that the console session follows the user's text wherever it disagrees with the built-in part, except that it cannot
change the section on what the console session does and does not do — the console session still never changes a project
itself. The section on work that touches no project is not covered by that exception, so the user's text can override
it. The text is kept as written, except that a closing `</user_instructions` of its own, in any letter case, has its `<`
written as `&lt;`, so it cannot end the user's part early. A console without any has no such section.

The file is written:

- **when the console is created** — a console whose file cannot be written is not created;
- **after an edit to the console is saved** — a failure to write it is logged and leaves the saved edit standing, and
  the next console session launch writes the file;
- **before every console session launch**, a resume and an account switch included — a failure to write it fails the
  launch.

Edits made to the file by hand therefore do not survive the next write. None of these writes recreates a missing
console working directory: a console session launch into one is refused instead.

An edit to the console's console session instructions reaches every agent's console sessions begun after it, and so does
a change to **Ask questions as choices** or to the language the daemon holds, neither of which writes a file of its own:
each is read at the next write. A Claude Code or Codex console session resumed after it reads it too (on Codex 0.142.5,
only after a compaction), while a Grok Build console session never does within its life, resumed or not: only a new one
does. Codex reads at most 32 KiB of a project's instruction files by default (its own `project_doc_max_bytes` setting),
so a very long text can be cut off for Codex console sessions, its end lost without notice; Octoboard sets no limit on
the text itself. The built-in part, with every optional section at its fullest and the memory index at its limit, is
kept within 28 KiB, so at least 4 KiB of the user's text always fits under that default.

### The guidance on asking the user

A section headed "Asking the user", in a project session's role description and in the console session's instruction
file while **Ask questions as choices** is on (see that section in `docs/product/agent-behavior-settings.md`), telling the session:

- when the user has to decide something, to ask through its agent's tool for putting a question to the user with
  options to pick from, so the user picks an option directly, rather than listing the options in its reply. A Claude
  Code session is told the tool's name, `AskUserQuestion`, and a Grok Build session `ask_user_question`; a Codex
  session is told of the tool without a name; the console's instruction file, one text for every agent, names both
  tools with the agent each belongs to;
- only where it has no such tool, or cannot use it, to put the questions on their own at the end of its reply,
  numbered, with lettered options;
- to say what is to be decided and give options with clear boundaries, marking as recommended the one it is confident
  in and saying why, and where it is not confident, saying what keeps it undecided and what would settle it;
- that a question that only lacks information and has no options to offer is not forced into the tool with invented
  options, but goes into the numbered list at the end of its reply, for the user to answer directly.

A project session is given the section only when the switch is on at that launch. As with the development rule, a change
to the switch is sure to reach only project sessions begun after it: Claude Code and Grok Build keep the role text a
conversation was first given, so a session begun earlier may keep the section, or its absence, even when resumed. When
a change reaches a console session is in "The console session's instruction file" above.

### The guidance on the user's language

A section headed "The user's language", in a project session's role description and in the console session's instruction
file, while the daemon holds a language that is not English, in any region (see "The language the daemon holds" in
`docs/product/language.md`). Nothing is added while the daemon holds no language, or for English: sessions then follow
the language the user writes in. The section names the language in English followed by its tag, such as "Simplified
Chinese (zh-CN)" or "Portuguese (pt)", says the user set Octoboard's interface to it, and tells the
session:

- to talk to the user in it: its replies, its questions, the reports and pages meant for the user, and the titles it
  gives sessions (see "Naming the sessions a session starts" in `docs/product/hub-orchestration.md`);
- unless the user writes to it in another language or asks for one, or the project's or the user's own instructions
  say which language to use with the user;
- that what Octoboard or any other session writes to it (headed, or the brief it was started with) and a tool's output
  are not the user writing, so whatever their language they do not change the language it uses with the user;
- that everything it writes into a project — files, code, comments, commit messages, pull requests — follows the
  project's own conventions instead.

In a project session's role description it comes after the guidance on asking the user, where that is given, and
before the sentence on the project's ordinary work. A project session is given the section only when a language
other than English is held at that launch; as with the guidance on asking the user, a change to it is
sure to reach only project sessions begun after it, since Claude Code and Grok Build keep the role text a conversation
was first given, so a session begun earlier may keep the section, its absence or the language it named, even when
resumed. When a change reaches a console session is in "The console session's instruction file" above.

### Claude Code's status line

A Claude Code session is also launched with a status line command of Octoboard's, given as a launch setting like the
hooks, so no configuration file is written. Claude Code passes a status line its session's figures, the model's
context window among them, which its own records never hold; the command reports that window to Octoboard (see
"Claude Code's context window" in `docs/product/session-context.md`). It is given only to a Claude Code known to be
2.0.0 or later; one older, or one whose version Octoboard cannot tell from where it is installed, gets none. A status
line set by managed (policy) settings outranks Octoboard's, which then never runs.

A status line from the launch outranks the user's and the project's own, so the command runs theirs in its place, and
what the user sees is what their own status line would show:

- which one is theirs is resolved on every run, so an edit made while the session runs is followed: the user settings
  in the Claude Code config directory in effect (the session's own, else a `CLAUDE_CONFIG_DIR` the user's shell
  exports, else `~/.claude`), then the project's `.claude/settings.json`, then its `.claude/settings.local.json`,
  each later one overriding the earlier ones key by key; a file that cannot be read or parsed counts for nothing;
- only a `command` status line is run, through `/bin/sh`, with the same input, environment and working directory
  Claude Code gave Octoboard's, and its output and exit status are passed on unchanged; one that cannot be started or
  is ended by a signal counts as exiting with status 1; with none configured, nothing is printed;
- its `padding`, `refreshInterval` and `hideVimModeIndicator` are carried into Octoboard's as they stand at launch,
  each only with a value of the type Claude Code takes for it (a `refreshInterval` of at least 1 second); a change
  to them takes effect at the next launch;
- reporting the window prints nothing, and a daemon that is slow or unreachable delays the status line by no more
  than a fraction of a second.

What cannot be carried over is having no status line at all. With any status line configured, Claude Code hides its
"? for shortcuts" footer hint and, in fullscreen, keeps a blank row for the line, so a user with no status line of
their own sees those two differences in sessions Octoboard runs. Running `claude` outside Octoboard is unaffected.

## The model and reasoning effort

The session's model and reasoning effort are resolved under the inheritance and override rules in
`docs/product/model-and-reasoning-effort.md`. Each resolved value is passed as that agent's own launch argument, on
every launch, a resume and an account switch included, and nothing else about the launch changes. A value left to
the agent's own default produces no launch argument for that value, except a Claude Code model, which gets one when
Octoboard chooses it (see "The default model" in `docs/product/model-and-reasoning-effort.md`).

Octoboard checks neither value against anything at launch. Which models and which effort levels exist is the agent's
own business — the dialogs that set them offer what the agent itself answers, and the agent is what answers for a
value it does not know anyway, in the session's own output, like any other complaint it has about its arguments. Each CLI applies the launch values in its own way.

- **Claude Code** silently lowers an effort the chosen model does not support down to `high`, rather than refusing it.
  It refuses a model the account cannot use, with its own message saying so.
- **Codex** refuses an effort the chosen model does not advertise, with an error of its own naming that model.
- **Grok Build** takes both as flags and saves them in its per-session configuration copy. Neither value as passed at
  launch reaches the user's own Grok configuration. With a linked source configuration, a model the user selects in the
  session reaches the file the link leads to when the session exits, if it differs from both the model passed at launch
  and the copy taken at launch; reasoning effort is never carried back (see "The per-session Grok home" in
  `docs/product/per-agent-launch-specifics.md` for the rules).
- **Antigravity CLI** groups model IDs that differ only by effort. With an effort but no model, the CLI applies it
  to its default model and can warn that it is using a supported level instead.
- **GitHub Copilot CLI** accepts the models and efforts from its account catalogue, including Auto where available.
- **OpenCode** offers its model IDs without an effort picker.
- **Qwen Code** lists models from the account’s selected authentication method.
- **Pi** uses a provider/model ID and the selected model’s available thinking levels.

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

**On a Windows daemon host there is no login shell to capture.** The environment is instead built as Windows builds a
new sign-in's: the daemon's own environment, with three sets of variables, as the system stores them, laid over it in
the order Windows applies them — the machine's environment variables, then the user's own, then the ones Windows sets
for the user at sign-in (the user name, the profile, application-data and home directories, the domain and the logon
server among them). A later set wins over an earlier one for the same name, so the sign-in values stand even where the
user has set a variable of the same name, and the user name a program sees is the signed-in user's, not the `SYSTEM`
the machine's variables hold. `PATH` is the machine's followed by the user's. A value stored as expandable has each
variable it names between percent signs replaced by that variable's value as it stands when the value's own set is
applied, so `%USERNAME%` in one of the user's own variables gives the machine's value, as in a real sign-in. Building
it starts nothing and cannot time out or fail, so the refusals and the clean-up below do not arise there; a variable
the user changes in the system's settings is picked up under the same reuse rules. An agent's binary is looked up on
that `PATH` with the extensions `PATHEXT` lists.

A new session is listed before any of that has happened: it is recorded and shown as *starting* immediately, and the
launch — the environment capture, the injected files and the agent process — carries on behind it, so a slow capture
holds up nothing the user sees. What a *starting* session allows, and what happens to it when the launch is refused for
any reason named below, is in "Starting the session" in `docs/product/opening-sessions.md` and in
`docs/product/session-statuses.md`.

**Capturing it is bounded at ten seconds, and a capture that does not complete refuses the launch.** Octoboard
refuses rather than starting an agent with a half-built environment, because a session missing the user's `PATH` or
their keys fails later in ways that do not point back here. Three outcomes are refused, and the message names which
one it was:

- the capture ran out of time — a shell startup file is blocked on something, or a background process it started is
  still holding the shell's output open;
- the shell exited with an error, in which case its own diagnostic output is included;
- the shell exited cleanly without having produced a complete environment dump.

The same capture is what gives a git clone the user's `git` and git credentials, so a clone is refused the same
way (see "Associating a project" in `docs/product/projects.md`).

**A capture leaves nothing it started running.** Whatever its outcome — success, any of the refusals above, a timeout
included — once a capture ends, everything still in its shell's process group is first sent a hangup, so it can clean
up (drop a lock, finish a write), and whatever is still there after at most half a second is killed. That reaches the
helpers a user's interactive rc file starts and disowns (`&!`, `disown`), such as a prompt theme's status daemon or an
update check. A helper that detaches itself into a new session (`setsid`, as ssh-agent, gpg-agent and tmux do) is out
of reach and is not stopped. What a capture hands on is therefore only an environment, never a process: a shell
terminal's own shell runs the rc file itself and starts its own helpers, but an agent, which is run directly with the
captured environment and no shell, may inherit variables that point at an rc helper the capture started and has since
stopped.

**What an outer session's launch set is put back first.** A daemon started from inside an agent session — a
development daemon run from an agent's terminal — inherits that session's environment, and so does the shell it
captures. Every launch therefore records, in a variable of its own, `OCTOBOARD_SESSION_EXPORTS`, each variable it set
over the shell environment, and each variable its agent's CLI is known to set for the commands it runs under names
people set themselves (pagers, colour and progress switches, `GIT_EDITOR`, the locale; each agent's list, with the
exact value its CLI sets, is its own): for each, a short digest of the value, never the value itself, and the value
the shell environment held before, if any. In an inherited environment each recorded variable that still holds the
recorded value gets back the value the shell environment held before that launch, or is removed if it held none. A
variable holding anything else was set since, by the user inside the session or by their shell profile, and is kept,
as is one that already held the same value before the session. So a session the inner daemon starts on the default
account runs on the user's own configuration directory, not the outer session's account, and inherits none of the
outer session's switches, files or git hook variables. The record stays within 24 KiB as written: a previous value too
long for it is not recorded, and that variable is then removed rather than left holding the outer session's value,
which the daemon logs at the launch. `PATH` is left as inherited: Claude Code does not change it, and Codex and Grok
Build run commands through the user's shell with its profile read again, so the inherited `PATH` is the profile's doing
as much as the CLI's, and the capture rebuilds it.

Then two groups of variables are removed from that snapshot:

- the snapshot shell's own terminal variables, which would otherwise make the agent downgrade its renderer, colours
  and mouse handling; the session's terminal is declared as `xterm-256color` instead;
- the variables that mark a process as *running inside an agent session*: Octoboard's own session variables, the
  launch's record above among them, and those each agent's CLI sets only to mark its session, by name, each agent's
  list its own. Without this, an Octoboard started from inside an agent session would hand that session's identity to
  every agent it launches, which silently changes their behaviour. Authentication is unaffected — credentials are not
  kept in these variables.

Besides the terminal type, the variables set over the snapshot are:

- `OCTOBOARD_PROCESS_TAG`, a fresh identity for every launch, including resumes and account switches, inherited by its
  subprocesses so Octoboard can follow and clean up the launch's background work (see "Which processes belong to a
  launch" in `docs/product/background-processes.md`). An inherited tag from the environment snapshot is removed;
- `OCTOBOARD_SESSION_EXPORTS`, the launch's record of what it changed (described above), which no snapshot passes on;
- `HOME`, when the user's shell leaves it unset, empty or relative: the daemon's own home directory (`HOME` when
  absolute, else the user's entry in the system user database, else `/`), so that every directory an agent or
  Octoboard takes from it is one place rather than one relative to the session's working directory;
- for a session that holds a config directory of its agent, the agent's own variable: `CLAUDE_CONFIG_DIR` for Claude
  Code, `CODEX_HOME` for Codex, each set to that directory and replacing any value the user's shell exports. A session
  that holds none gets the snapshot's own value, if there is one, unchanged; Octoboard sets `CODEX_HOME` for nothing
  else;
- for every Grok Build session, `GROK_HOME`, set to the session's own Grok home (see "The per-session Grok home" in
  `docs/product/per-agent-launch-specifics.md`). A Grok Build config directory is not passed as `GROK_HOME` itself;
- for a Grok Build session on a host that makes no file links, `GROK_AUTH_PATH`, set to the source home's `auth.json`
  unless the environment snapshot already holds it (see "On a host without file links" in
  `docs/product/per-agent-launch-specifics.md`);
- for every Grok Build session, `GROK_THEME`, which picks Grok's own colour theme to match the session terminal (see
  "Grok Build's theme" in `docs/product/per-agent-launch-specifics.md`);
- for a Grok Build session while Grok Build's Claude compatibility is on, the five `GROK_CLAUDE_*_ENABLED` variables, set
  to `true`, except `GROK_CLAUDE_MCPS_ENABLED`, set to `false` where Octoboard gives Claude Code's MCP servers itself,
  together with the `OCTOBOARD_CLAUDE_MCP_<n>_<m>` variables that carry those servers' literal header and `env` values
  (see "Claude compatibility for Grok Build" in `docs/product/compatibility-between-agents.md`);
- for a Codex session while Codex's Claude compatibility is on, the variables that carry the passed Claude Code MCP
  servers' header and `env` values (see "Claude compatibility for Codex" in
  `docs/product/compatibility-between-agents.md`);
- for a Claude Code or Grok Build session while the agent's Codex compatibility is on, the `OCTOBOARD_CODEX_MCP_<n>_<m>`
  variables that carry the passed Codex MCP servers' literal header and `env` values (see "Codex's MCP servers" in
  `docs/product/compatibility-between-agents.md`);
- for a Claude Code session whose account's directory shares the default configuration, `CLAUDE_CODE_REMOTE_MEMORY_DIR`
  (when exactly, an account whose last change of sharing is incomplete included, is in "Claude Code's auto memory" in
  `docs/product/sharing-the-default-configuration.md`);
- while Octoboard's memory is on at the launch, for a Claude Code or Grok Build session, the variable that turns the
  agent's own memory off; a Codex session is given a launch argument for it instead (see "The agents' own memory" in
  `docs/product/memory.md`);
- for a project session, a `core.hooksPath` entry through `GIT_CONFIG_PARAMETERS`, after the entries the user's shell
  exports there, together with `OCTOBOARD_SESSION_ID`, `OCTOBOARD_DAEMON_PORT` and `OCTOBOARD_DAEMON_CREDENTIAL_PATH` —
  the port of the process holder that runs the session and the path of the owner-only file holding the key it issued for
  the launch (see "Session helpers and the holder" in `docs/product/process-holders.md`), or the daemon's own port and
  key where the terminals run in the daemon itself — which point every git in the agent's process tree at Octoboard's
  hook dispatchers; those still run the user's own git hooks, report the branches that git creates (see "The session's
  git hooks" and "What attribution needs" in `docs/product/branch-responsible-session.md`), and run the revert gate (see
  `docs/product/revert-gate.md`). The key itself is never in the environment, which an agent may write into files of its
  own, and no name among them holds `KEY`, `SECRET` or `TOKEN`, which an agent's filter of secrets strips. A console
  session gets none of them.

The agent binary is resolved from that same snapshot's `PATH`, which is that login + interactive shell's `PATH`, not the
one a terminal happens to have. So when the shell lists an old install of an agent before a newer one, the old one is
launched, and an agent too old for the hooks Octoboard injects — a Codex older than the async hooks Octoboard injects,
say — cannot report the session's status.

Which directory a session holds, and the refusal of a launch whose directory no longer exists, are described in "Agent
config directories" in `docs/product/consoles.md`.

**The daemon's own environment is read the same way, once, as it starts**, when it was inherited from a session —
something in it marks one: what the outer launch set is put back and the session's marks are removed, so everything
the daemon starts or opens on its machine — the system terminal, the file manager, a file or link opened through the
system, on every operating system — gets the user's values. A daemon not started inside a session keeps its
environment exactly as it is. The helper programs Octoboard runs inside a session — its hooks, its git reports, the
status line, the session's MCP server — keep that session's environment.

**What a session may do under macOS's privacy rules is Octoboard's, not the agent's.** macOS attributes a privacy
request made by a session's process — taking a screenshot, driving another application's interface — to Octoboard, so
the session acts under Octoboard's own grants and a grant the user gave some other application does not count. The two
permissions this concerns are granted ahead of time in Settings; see `docs/product/macos-privacy-permissions.md`.

**Whether an agent is installed at all is worked out once, not per launch.** Right after the daemon starts it takes one
login-shell snapshot of its own — the same kind described above — resolves each agent's binary against that snapshot's
`PATH`, and reads what each agent's default account resolves to from it (see "Agent config directories" in
`docs/product/consoles.md`). A binary that resolves is all "available" means: whether an account's directory holds a
login is never checked, since a user may sign in through an API key from their shell, a credential helper or an
organization's gateway, none of which shows in the directory. This does not hold up the daemon's start: the application
is served immediately, and the result follows once it lands, typically within a couple of seconds and budgeted the same
ten seconds a launch's own snapshot is. That snapshot is also the one the first launch can reuse. Until it lands every
agent reads as *not yet determined*, during which nothing is refused for being unavailable — a launch whose agent turns
out to have no binary is refused by the ordinary path above, which already covers it. A snapshot that does not complete
leaves availability not yet determined for the rest of that run rather than marking every agent unavailable. With every
agent determined unavailable, no session can be opened at all; see "When no agent can open a session" in
`docs/product/opening-sessions.md`.

## Agent session data

Conversation history and session records belong to each agent and stay wherever that agent keeps them. Their
retention and cleanup follow the agent's own rules, except for the automatic and opt-in moves to the system Trash
described in "Moving the agent's own records to the Trash" in `docs/product/deleting-sessions.md`. Before resuming a
Codex session whose rollout Codex itself has archived, Octoboard runs `codex unarchive` for it (see "Archiving,
interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`). Octoboard copies a session's
records only when that session is switched — by the user, or by its owner — to another account of the same agent, and
only that session's (see "Carrying the conversation" in `docs/product/switching-session-accounts.md`). Copying leaves
the records in the account they came from as well, so
switching back needs no second copy; and what Octoboard copied is the agent's own from then on, retained and cleaned up
by the agent's rules like any other.

Codex compresses a session's rollout once it is about a week old, replacing `rollout-<timestamp>-<thread id>.jsonl` with
`rollout-<timestamp>-<thread id>.jsonl.zst` in the same directory. Octoboard takes the compressed rollout for the
session's record wherever it looks for one: the session's context size and task title are still read from it, the
check for a session's records before a deletion and the check for whether its conversation can still be resumed find
it, it is moved to the Trash with the session, and an account switch copies it.

Reverting a turn makes Codex continue the thread in a **history segment**, a later rollout named
`rollout-<timestamp>-<thread id>_<segment id>.jsonl` (compressed like any other rollout once old), which records how
much of the file before it the thread keeps. A thread reverted more than once has several, each continuing an earlier
file. Octoboard treats a thread's first rollout and all its segments, plain or compressed, as one record. Its **head**
is the file no other continues — of several, the one started last — which holds the conversation as it stands; it is
not necessarily the file written to last:

- the session's context size is the latest request's in the head; a head that has had no turn since the revert shows
  the figure where it picks up the conversation from the file it continues (see "Session context size" in
  `docs/product/session-context.md`);
- the fallback task title, the user's first prompt (see "Where the task title comes from" in
  `docs/product/session-titles.md`), is taken from the thread's first rollout when it is there, since a segment holds
  only what came after its cut;
- the check for a session's records before a deletion and the check for whether its conversation can still be resumed
  find any file of the thread (so a thread is never taken for gone while part of it is there), and every one of them
  is moved to the Trash with the session;
- an account switch copies them all and points the target's own record of the thread at the head (see "Carrying the
  conversation" in `docs/product/switching-session-accounts.md`);
- a handoff package reads the conversation as it stands across them (see "The handoff package" in
  `docs/product/handing-off-sessions.md`).

A subagent's rollout is named by its own thread id, which never contains its parent's, and is no part of the parent's
record.
