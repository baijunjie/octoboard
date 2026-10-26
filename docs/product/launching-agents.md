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
  the Trash" in `docs/product/removing-projects.md`).
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
- **The user's own agent configuration is never written to, with five narrow exceptions.** Octoboard does not edit
  `~/.claude.json`, `~/.codex/`, `~/.grok/`, an account's config directory, or anything else the agent reads as the
  user's global setup, and it never makes a trust decision of its own in any of them. The first exception is switching a
  session to another account: it copies that one session's conversation — its record and what the agent keeps of the
  conversation beside it — into the target account's directory, adding it there or replacing an earlier copy, by way of
  a transient `.octoboard-switch` directory at the root of the target directory, so a failure never leaves a
  half-written item among the agent's own (what is copied for each agent, and how a failed copy is undone, are in
  "Carrying the conversation" in `docs/product/switching-session-accounts.md`). It touches no settings file of the
  agent's, nothing of other sessions in either directory, and it leaves the originals where they were. The second
  belongs to the same switch, for a Codex session: Codex remembers each thread in a database in its home (or wherever
  the home's `sqlite_home` setting or `CODEX_SQLITE_HOME` puts it) and resumes the rollout that database names, so a
  target account that held the thread earlier would resume an older branch of it however whole the copied conversation
  is. The switch therefore updates the `rollout_path` that database keeps of that thread to the carried head where it
  leads to another file, and changes nothing else in it; if it cannot, the switch fails before the session is relaunched
  (see "Carrying the conversation" in `docs/product/switching-session-accounts.md`). The third is Grok Build's trust
  store: once Grok's own trust confirmation has been accepted, the one entry Grok wrote into its session's copy of the
  store is carried, unchanged, into the user's own (see "Carrying Grok Build's trust record" in
  `docs/product/folder-trust.md`). The fourth is an account set to share the agent's default configuration: its
  directory's shareable entries are replaced by links into the default account's directory, its configuration set aside
  as backups and its conversation history moved into the default directory, the links repaired before each of its
  sessions launches, and all of it undone when sharing is turned off, together with the paths the default directory's
  Codex thread database and plugin registries record through the account (see
  `docs/product/sharing-the-default-configuration.md`). The fifth is settings saved in a Grok Build session whose source
  `config.toml` is a link: after the process exits, selected changes are carried into the file that link leads to,
  leaving the link in place (see "Per-agent specifics a user will notice" below). Creating a Codex account's directory
  that does not exist yet, empty, before its first launch is not an exception: Codex refuses to start without it, and
  nothing of the user's is in it.

Where an agent stops to ask whether to trust a folder, Octoboard decides nothing on its behalf: it presses the
agent's own confirmation, with the keystrokes a person would type — for a project session only after the user has
given Octoboard permission, for a console session in the console's own working directory without asking — and the
agent records the trust in its own configuration, as it does when a person answers. See
`docs/product/folder-trust.md`.

Claude Code's folder trust and Codex's approval reviewer affect what Octoboard can promise for a session (see "Per-agent
specifics a user will notice" below and "The raised hand" in `docs/product/sessions.md`). Their configuration files are
read at launch and never rewritten. Claude Code's trust is given by answering its trust confirmation. Codex's reviewer
and Claude Code's approval mode can be overridden for every account of the agent in Settings; see "Approval modes" in
`docs/product/agent-accounts.md`.

Everything Octoboard adds is injected **per launch** and disappears with the process. In consequence, a project's own
configuration keeps working exactly as it does outside Octoboard: its instruction file, its skills, its hooks and its
permission rules all take effect, because the session runs with the project's directory as its working directory and
nothing suppresses the agent's normal discovery of them. A hook the project itself defines and Octoboard's own hook
both run, in the same session.

## What is injected on every launch

Three things, and for Claude Code a status line besides (see "Claude Code's status line" below), and nothing else,
besides the variables a project session's agent is given for its git (see "The launch environment" below):

- **Status hooks** — the events the session statuses in `docs/product/sessions.md` are derived from.
- **Octoboard's own MCP server** — the tools described in `docs/product/hub-orchestration.md`. It is
  a child process of the agent, started fresh per session, and each session's tools are reachable
  only by that session. The agent's own and the project's own MCP servers keep working alongside it. Claude Code and
  Codex are told to run its tools without asking (see "Per-agent specifics a user will notice" below).
- **A role description** — whether this session is a console session or a project session, and the reporting conventions
  that go with that. A console session's is short: it is the console session, it decomposes requests on its console's
  projects, dispatches them, follows up and summarizes, does itself any work that touches no project, and does not
  change a project itself, and the rest is in its instruction file, which may end with the user's own instructions,
  which take precedence over the rest of the file wherever they disagree but never let it change a project itself (see
  "The console session's instruction file" below). A project session bound to a console session is told that console
  session is waiting on its result and how to `report` to it; one bound to a project session is told which project
  session, by its id, dispatched it and that it reports there; either is told that its owner can hand it to another
  session, which Octoboard then tells it in a message. One nobody owns — opened by hand without a console session to
  report to, or started on another session's request — is told that no session dispatched it, that nobody but the user
  owns it, and not to call `report`; that it may start sessions in its own project and drive them, which report back to
  it; and that if it binds itself to an owner, that call's result tells it what changes. A lead session, bound to a
  console session only after it was opened, is given that same unbound project session's text on every launch, as at its
  first (see "Lead sessions" in `docs/product/hub-orchestration.md`). Every project session's text also sets out how it
  relates to other sessions: `new_session` for a session of its own where it has it, together with when to start one
  and how to choose the session a task goes to (see "Choosing the session a task goes to" in
  `docs/product/hub-orchestration.md`), `handoff_session`, `switch_session_account`, `transfer_session`,
  `release_session` and `bind_to_session` where it has them, `request_session` for a session that is nobody's,
  `request_resume_session` for resuming a conversation the user asked for, `get_account_quotas` for what each account's
  quota has left, the reads every session has whatever its kind — `get_session`, `read_session_history`,
  `list_consoles`, `list_projects` and `list_archived` — with the rule that a session not its own is to be left alone
  whatever it can read of it, `list_agent_models` to check before naming a model or effort, `list_sessions` and
  `share_info` for information, and `get_session_branches` for where the branches a session is responsible for are
  checked out. Where its project's effective git workflow settings
  have sessions develop on a branch or in a worktree of their own at the launch, it also carries the development rule
  (see "The development rule" in `docs/product/git-workflow-settings.md`), where **Ask questions as choices** is on at the launch, the guidance on
  asking the user after it (see "The guidance on asking the user" below), and where the language the daemon
  holds is not English, the guidance on the user's language after that (see "The guidance on the user's language" below). Every project session's text ends by saying that everything else is the
  ordinary work of the project, under its own instructions, conventions and configuration. The text is written from who the session
  reports to when it launches, and says that what Octoboard tells it of a later change of owner replaces what it says
  (see "When a binding changes while a session runs" in
  `docs/product/changing-session-owners.md`); a change made while the session was not running is told at its next
  launch, ahead of that launch's own instruction (see "A change of owner told at the next launch" in
  `docs/product/changing-session-owners.md`). The role and the session's binding as it stands decide which tools the
  session is offered, so an unbound project session still has `report`, and calling it is refused while it is unbound
  (see `docs/product/reporting-to-owners.md` and "The unbound project session's tools" in
  `docs/product/hub-orchestration.md`).

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
"Resuming a conversation by its session ID" in `docs/product/sessions.md`) may not take up its role
description with Claude Code or Grok Build, which keep what that conversation recorded; the status hooks and
Octoboard's MCP server are injected as on any resume.

A resume reassembles the full injection, because all three agents resolve hooks and MCP servers from the launch
arguments every time and a resume that omitted them would leave a session nobody can observe and nothing to report
with.

The model and reasoning effort resolved for the session are passed on the launch as well (see "The model and reasoning
effort" below), as is an explicit Codex approval reviewer or Claude Code approval mode chosen for the agent (see "Approval
modes" in `docs/product/agent-accounts.md`). These are the user's choices carried to the agent, separate from the injected
capabilities.

### The console session's instruction file

Octoboard writes one instruction file into the console's working directory, `AGENTS.md`, with `CLAUDE.md` beside it as
a relative symbolic link to it; a regular `CLAUDE.md` found there is replaced by the link. Claude Code reads
`CLAUDE.md`, Codex reads `AGENTS.md`, and Grok Build reads both and loads the linked pair once — a console's working
directory is not a git repository, and Grok reads the instruction files there once the folder is trusted, which
Octoboard's press of a console session's trust confirmation sees to (see `docs/product/folder-trust.md`). So console
sessions of every agent, running together or not, read the same text, and the file names Octoboard's tools without any
agent's prefix. Each console session's role description points it to the file by the name its own agent reads.

The file holds the console session's role (see the console session in `docs/product/hub-orchestration.md`) in two
sections: one on what it does and does not do — decompose, dispatch, follow up and summarize work on the console's
projects, and make no change to a project itself, reading one being how it checks what its sessions report — and a
separate one on work that touches no project, which it carries out itself. The file also holds how to use the
orchestration and settings tools, and, when **Ask questions as choices** is on as the file is written, the guidance on
asking the user (see "The guidance on asking the user" below) after the rest of the built-in part, with one sentence
more: a decision among a few options goes through that question tool rather than a page pushed with `show_page`, which
is for a choice that needs a page — a rich comparison, many items, a form. When the language the daemon
holds is not English as the file is written, the guidance on the user's language (see "The guidance on the user's language" below) follows, as the built-in part's last section. When the console has console session instructions of the user's (see "Console
session instructions" in `docs/product/consoles.md`), they are the file's last section, held between a
`<user_instructions>` and a `</user_instructions>` tag under a heading of the file's own. That heading's paragraph says
the tags hold the user's own text, that any heading inside them belongs to that text and not to the file's sections,
and that the console session follows the user's text wherever it disagrees with the built-in part, except that it
cannot change the section on what the console session does and does not do — the console session still never changes a
project itself. The section on work that touches no project is not covered by that exception, so the user's text can
override it. The text is kept as written, except that a closing `</user_instructions` of its own, in any letter case,
has its `<` written as `&lt;`, so it cannot end the user's part early. A console without any has no such section.

The file is written:

- **when the console is created** — a console whose file cannot be written is not created;
- **after an edit to the console is saved** — a failure to write it is logged and leaves the saved edit standing, and
  the next console session launch writes the file;
- **before every console session launch**, a resume and an account switch included — a failure to write it fails the
  launch.

Edits made to the file by hand therefore do not survive the next write. None of these writes recreates a missing
console working directory: a console session launch into one is refused instead.

An edit to the console's console session instructions reaches every agent's console sessions begun after it, and so
does a change to **Ask questions as choices** or to the language the
daemon holds, neither of which writes a file of its own: each is read at the next write. A Claude
Code or Codex console session resumed after it reads it too (on Codex 0.142.5, only after a compaction), while a Grok Build
console session never does within its life, resumed or not: only a new one does. Codex reads at most 32 KiB of a
project's instruction files by default (its own `project_doc_max_bytes` setting), so a very long text can be cut off
for Codex console sessions; Octoboard sets no limit on the text itself.

### The guidance on asking the user

A section headed "Asking the user", in a project session's role description and in the console session's instruction
file while **Ask questions as choices** is on (see that section in `docs/product/settings.md`), telling the session:

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

A section headed "The user's language", in a project session's role description and in the console session's
instruction file, while the daemon holds a language that is not English, in any region (see "The language
the daemon holds" in `docs/product/language.md`). Nothing is added while the daemon holds no language, or for English: sessions then follow the language the user writes in. The section names the language in English followed by its tag, such as "Simplified
Chinese (zh-Hans)" or "Brazilian Portuguese (pt-BR)", says the user set Octoboard's interface to it, and tells the
session:

- to talk to the user in it: its replies, its questions, the reports and pages meant for the user, and the titles it
  gives sessions (see "Naming the sessions a session starts" in `docs/product/hub-orchestration.md`);
- unless the user writes to it in another language or asks for one, or the project's or the user's own instructions
  say which language to use with the user;
- that everything it writes into a project — files, code, comments, commit messages, pull requests — follows the
  project's own conventions instead.

In a project session's role description it comes after the guidance on asking the user, where that is given, and
before the closing sentence on the project's ordinary work. A project session is given the section only when a language
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
the agent's own default produces no launch argument for that value.

Octoboard checks neither value against anything at launch. Which models and which effort levels exist is the agent's
own business — the dialogs that set them offer what the agent itself answers, and the agent is what answers for a
value it does not know anyway, in the session's own output, like any other complaint it has about its arguments. What
the three do differs, and is worth knowing before a value is set:

- **Claude Code** silently lowers an effort the chosen model does not support down to `high`, rather than refusing it.
- **Codex** refuses an effort the chosen model does not advertise, with an error of its own naming that model.
- **Grok Build** takes both as flags and saves them in its per-session configuration copy. Neither value as passed
  at launch reaches the user's own Grok configuration. With a linked source configuration, a model the user selects in
  the session reaches the file the link leads to when the session exits, if it differs from both the model passed at
  launch and the copy taken at launch; reasoning effort is never carried back (see "Per-agent specifics a user will
  notice" below for the rules).

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
  notice" below). A Grok Build config directory is not passed as `GROK_HOME` itself;
- for every Grok Build session, `GROK_THEME`, which picks Grok's own colour theme to match the session terminal (see
  "Per-agent specifics a user will notice" below);
- for a Claude Code session whose account's directory shares the default configuration, `CLAUDE_CODE_REMOTE_MEMORY_DIR`
  (when exactly, an account whose last change of sharing is incomplete included, is in "Claude Code's auto memory" in
  `docs/product/sharing-the-default-configuration.md`).
- for a project session, a `core.hooksPath` entry through `GIT_CONFIG_COUNT`, together with `OCTOBOARD_SESSION_ID` and
  `OCTOBOARD_DAEMON_PORT`, which point every git in the agent's process tree at Octoboard's hook dispatchers; those
  still run the user's own git hooks, report the branches that git creates (see "The session's git hooks" and
  "What attribution needs" in `docs/product/branch-responsible-session.md`), and run the revert gate (see
  `docs/product/revert-gate.md`). A console session gets none of them.

The agent binary is resolved from that same snapshot's `PATH`, which is that login + interactive shell's `PATH`, not the
one a terminal happens to have. So when the shell lists an old install of an agent before a newer one, the old one is
launched. Seen with Codex: a Finder-style launch resolved an older Homebrew build ahead of the current one in
`~/.local/bin`. That Codex was older than the async hook support Octoboard injects (inferred from its "skipping async
hook" warning; the session ended on an API error before status could be observed), and such a Codex cannot supply hooks
(see "Codex" in `docs/agent-cli-reference.md`). The cause is the user's `PATH` order, not Octoboard.

Which directory a session holds, and the refusal of a launch whose directory no longer exists, are described in "Agent
config directories" in `docs/product/consoles.md`.

**What a session may do under macOS's privacy rules is Octoboard's, not the agent's.** macOS attributes a privacy
request made by a session's process — taking a screenshot, driving another application's interface — to Octoboard, so
the session acts under Octoboard's own grants and a grant the user gave some other application does not count. The two
permissions this concerns are granted ahead of time in Settings; see "Permissions" in `docs/product/settings.md`.

**Whether an agent is installed at all is worked out once, not per launch.** Right after the daemon starts it takes
one login-shell snapshot of its own — the same kind described above — resolves each agent's binary against that
snapshot's `PATH`, and reads what each agent's default account resolves to from it (see "Agent config directories" in
`docs/product/consoles.md`). A binary that resolves is all "available" means: whether an account's
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
Its status line is Octoboard's, which runs the user's own in its place; the footer differences that brings for a user
without one are in "Claude Code's status line" above.

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

An explicit approval mode chosen for Claude Code in Settings is passed on every launch of any of its accounts, a resume
included, as the permission mode Claude Code starts in; following each account's configuration passes none, so the
account's `settings.json` decides. Under **Approve everything**, every launch also tells Claude Code that its own warning for that
mode has been accepted, so the blocking screen it would otherwise raise on a config directory's first use of the mode
never appears; this holds for that launch alone, and nothing is written into the account's configuration files. The
choices are in "Approval modes" in `docs/product/agent-accounts.md`.

Octoboard's own tools are pre-approved for the session on every launch, a resume included, under every approval mode,
following the account's configuration included, so a session's calls to them, a console session's orchestration
included, raise no Claude Code permission prompt. The launch carries one `allow` permission rule naming Octoboard's MCP
server, `mcp__octoboard`; like the acceptance of the warning above, it holds for that launch alone and nothing is
written into any settings file. The rule covers that one server's tools alone: every other MCP server, whatever its
name, Bash, file edits and every other tool keep the approval mode in effect. It adds to the user's and the project's
own `allow` rules rather than replacing them, and a `deny` or `ask` rule of the user's or the project's that names one
of Octoboard's tools still wins over it — a `deny` takes that tool away from the session altogether. Under
**Automatic**, Octoboard's tools are therefore not put to Claude Code's auto-mode classifier either. What a tool itself
refuses, and the requests Octoboard puts to the user before acting on a session's behalf (see
`docs/product/session-requests.md`), are unaffected.

**Codex.** Codex cannot be given a session id in advance, and its conversation is created lazily on the first prompt
submission — so a session opened without a task has no agent-side id until the user types something, unless it was
opened to resume a conversation, whose id it has from the start. The launch
conditions that are visible:

- project trust is not passed: in a git repository Codex has not been told to trust, its own folder-trust
  confirmation appears, and Octoboard presses it under the user's permission (see `docs/product/folder-trust.md`);
- Codex's check for a newer version at startup is turned off for that launch (`check_for_update_on_startup=false`),
  so that its update prompt does not come up ahead of the trust confirmation; the user's own setting is left as it is;
- the items of Codex's terminal title are set for that launch to its activity, the thread's name and the project's name
  (`tui.terminal_title=["activity","thread-name","project-name"]`), overriding the user's and a trusted project's own
  `tui.terminal_title` inside Octoboard's terminal. Codex's first title is what tells Octoboard its session has
  started, and its activity title is what raises the hand for an action Codex waits on (see "Messages held until a
  session can take them" in `docs/product/message-delivery.md` and "The raised hand" in `docs/product/sessions.md`),
  so a configuration that draws no title would otherwise hold the session's messages for good;
- a resume starts in the session's own working directory without asking (`tui.resume_cwd="current"`), instead of
  Codex offering to switch to the directory the conversation last ran in;
- Codex's prompt to try a newer model is suppressed for every model that the models cache (`models_cache.json`) in
  the Codex home that launch uses lists an upgrade for (`notice.model_migrations`), and nothing is passed when that cache is
  missing, unreadable or lists none; its nudge to switch to another model as a rate limit nears is hidden
  (`notice.hide_rate_limit_model_nudge=true`). Each of these screens, like the working-directory choice above, would
  take a message written into the session as its answer; one that comes up all the same holds the session's messages
  until the user has answered it (see "Messages held until a session can take them" in
  `docs/product/message-delivery.md`). Like the title items and the working directory, these hold for that launch
  alone; nothing is written into the user's configuration;
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

An explicit Codex approval reviewer chosen for Codex in Settings takes precedence on every launch of any of its
accounts, a resume included. When the setting follows each account's configuration, whether Codex resolves approval requests by itself is read
from `config.toml` in the Codex home that launch uses: the session's Codex config directory, else a `CODEX_HOME` the
user's shell exports, else `~/.codex`. A trusted project's own `.codex/config.toml`, which Codex itself layers over
that file, is not consulted for this. The choices and their lifetime rules are in "Approval modes" in
`docs/product/agent-accounts.md`.

**Grok Build.** A Grok session runs against a per-session Grok home that Octoboard builds afresh on every launch from a
**source home**: the session's Grok Build config directory when it holds one, else a `GROK_HOME` the user's shell
exports, else `~/.grok`. The per-session home links back to the source home's entries, so the login and the
session records are the source home's own, and a session Octoboard started stays resumable from the user's own `grok`
command run against that same home — for a console's Grok Build config directory, `GROK_HOME=<that directory> grok`.
Each link points at the source entry's fully resolved target. When the source entry is itself a link, that target is
the final file or directory, not the link, so the session reaches it through one link. For an account that shares the
default configuration, a save from the session of a linked shared file — `AGENTS.md`, `pager.toml` or
`disabled-hooks` — updates the default directory's resolved file and leaves the account's own link in place. A
dangling source link has nothing to resolve to, and the session's link points at that source entry.

Some of its entries are **copies** taken at launch rather than links. Two are files whose copying has consequences
worth knowing:

- the Grok configuration file: an edit to the source file while a session is running is not picked up by that
  session. With a plain source `config.toml`, settings the session saves are lost when the session ends. With a
  source `config.toml` that is a link to an existing file — an account sharing the default configuration, or any home
  whose `config.toml` the user has linked elsewhere, such as into a dotfiles repository — selected settings are
  carried into the file the link leads to after the process exits and before the per-session home is removed,
  leaving the link in place: `ui.theme`, `plugins.enabled`, `plugins.disabled`, and `models.default`. Only values
  that differ from the copy taken at launch are carried; each plugin list is carried whole, an empty one included,
  but removing a key does not remove it from the linked file. A model equal to the model passed at launch is
  excluded, including when the user explicitly selects it again. Reasoning effort, Octoboard's MCP entry and every
  other setting stay in the copy. The next launch reads the source file afresh.

  The link must still lead to the same file it led to at launch: a link removed, broken or redirected to another file
  receives nothing. Sessions exiting under the same daemon merge only what they changed into the current file: changes
  to different settings survive, and for the same setting — a plugin list counting as one — the last exit wins. A
  session that changed nothing does not restore an earlier value. A simultaneous save from an external Grok process or
  editor can still race with this. A failure to write is logged, and the per-session home is still removed, so the
  settings it would have carried are lost; so are they if the Octoboard daemon itself stops while the session is
  running.
- the trust store, `trusted_folders.toml`: copied as it is, with nothing added, so a folder the user has not trusted
  Grok with shows Grok's own trust confirmation. The one entry Grok writes into the copy when that confirmation is
  accepted is the exception to its being lost: it is carried into the source home's store (see "Carrying Grok
  Build's trust record" in `docs/product/folder-trust.md`).

Grok's built-in bundles and its sandbox files — `bundled/`, `bundles/`, `hooks-paths` and `sandbox.toml` — are copies
taken on every launch as well, whichever of them the source home holds. A bundle update Grok downloads during a
session lands in Octoboard's copy and is not kept; the next launch copies whatever the user's own `grok` has fetched
since.

**A Grok Build launch that would run under Grok's own sandbox is refused** before the per-session home is built,
with the code `grok_sandbox_unsupported`, whose message names the sandbox profile and what sets it. The sandbox lets
Grok write only to its own home, the working directory and temporary directories, judged by where links really lead,
and the per-session home's login and session records are links back into the source home, so a sandboxed session
could not even create its conversation. The alert's remedy is to turn the sandbox off where it is set, or to have
whoever manages that setting do so; a conversation begun under the sandbox keeps it, so it is to be continued in Grok
itself. A profile of `off` is no sandbox. Which profile applies is read as Grok layers it:

- `[sandbox] profile` in the system-wide `/etc/grok/requirements.toml` is a pin that takes precedence over
  everything: any other profile there refuses, and `off` there allows the launch whatever else is set;
- on a resume, the profile Grok saved with the conversation (its `summary.json`'s `sandbox_profile`), or a
  `GROK_SANDBOX` in the launch environment (see "The launch environment" above), refuses when either is not `off`,
  since Grok restores a saved profile and refuses a resume that asks for a different one; a resume whose
  conversation has no saved profile is judged like a new session;
- otherwise the first of these that sets a profile decides: `GROK_SANDBOX` in the launch environment, then
  `[sandbox] profile` in the source home's `config.toml`, then the fleet default in the system-wide
  `/etc/grok/managed_config.toml`.

Not read: macOS managed preferences for Grok, so a sandbox set only there is not refused up front and the session
fails inside Grok instead; and the source home's own `requirements.toml` and `managed_config.toml`, which are left
out of the per-session home, so Grok never loads them for an Octoboard session.

Octoboard's own MCP server is declared in that copy of the configuration file, and its entry sets **Grok Build's own
time limit on a call to Octoboard's tools** above Octoboard's own limit on a forwarded call (the 10 minutes in
"Waiting for the answer" of `docs/product/session-requests.md`), so that a call which waits for the user, such as a
session request, is ended by Octoboard's limit and not by Grok's — Grok cancels a call it gives up on, and a
cancellation withdraws such a request. The limit is set on Octoboard's own entry alone; no other MCP server of the
user's is touched, and the injected MCP entry is never carried into the user's own configuration.

**Grok Build's theme follows the session terminal's appearance.** Grok's own default theme is a dark one, which would
paint dark bands across a terminal in the light appearance, so every Grok Build launch sets `GROK_THEME`:

| The launch | `GROK_THEME` |
|---|---|
| asked for from a window in the light appearance | `grokday` |
| asked for from a window in the dark appearance | `groknight` |
| with no window behind it | `auto`, which has Grok follow the operating system's appearance |

A window asks for a launch when it opens a session (with the new-session dialog or with defaults), resumes one, switches
one to another account or hands one off, and it sends the appearance its Light, Dark or System choice has resolved to
at that moment (see "Light, dark and follow the system" in `docs/product/appearance.md`). No window is behind a launch an
agent asks for through Octoboard's tools (opening or reopening a session, handing one off, switching one to another
account), a session opened, reopened or resumed by an approved session request, or an archived owner relaunched on the way while a session
is resumed.

The appearance is taken at each launch and not kept with the session: a running Grok session keeps the theme it was
launched with when the window's appearance changes later, and its next launch picks the theme afresh. `GROK_THEME` takes
precedence over the theme set in the Grok configuration file at startup, and does not save a theme into that file. An
explicit `/theme` saves the chosen theme in the session copy; when the source `config.toml` is linked, it is carried
into the linked file on exit under the rules above, even when it is the theme the appearance picked. The next launch
applies the terminal's appearance again. Claude Code and Codex are told nothing of the appearance.

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
