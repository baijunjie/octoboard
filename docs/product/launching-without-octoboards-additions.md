# Launching without Octoboard's additions

How a session starts and runs when Octoboard cannot give its launch everything it adds, what refuses a launch at
all, and how the user is told. What a launch with Octoboard's additions carries is in "What is injected on every
launch" in `docs/product/launching-agents.md`; what each agent's launch with less or plain launch passes is in
`docs/product/per-agent-launch-specifics.md`.

A session always starts as its agent's CLI would in a plain terminal, whatever Octoboard can or cannot add to it. Each
launch is made at one of three levels, tried from the top and only ever downwards:

- **with Octoboard's additions**, the ordinary launch, everything in "What is injected on every launch" in
  `docs/product/launching-agents.md`;
- **with less**, after the agent refused that launch as it started, where its provider knows exactly what to leave out
  (see "When the agent exits as it starts" below); which agents have such a launch and what it leaves out is in
  `docs/product/per-agent-launch-specifics.md`;
- **plain**: the agent's CLI as a person would start it for this session (see "The plain launch" below).

What Octoboard tells the user of a session that runs with less than its additions is a standing notice in the
session's terminal banner, kept there while it holds, for every window, whenever it connects (see
`docs/product/standing-session-notices.md`). The terminal itself works as usual.

## What refuses a launch

Only what a plain terminal would also fail on, or what the agent's CLI cannot do for this session whatever Octoboard
adds, refuses a launch, the session's other refusals (a turned-off account, a daemon shutting down and the like, see
"When the agent or the account cannot open a session" in `docs/product/opening-sessions.md`) aside:

- the session's working directory is not a directory the daemon can reach (`directory_unreachable`);
- the agent's CLI is not found on the launch environment's `PATH` (`binary_not_found`; see "The launch environment"
  in `docs/product/launching-agents.md` for one not found on a fallback environment);
- a pinned config directory that is gone, on a resume, or one the agent's variable cannot name (see "Agent config
  directories" in `docs/product/consoles.md`);
- what the agent's provider refuses for every launch of the session: an account with a directory of its own for
  Antigravity CLI and OpenCode, whose CLIs have one sign-in whatever directory they are given
  (`provider_unsupported`), a Codex account directory that does not exist and cannot be created, and on OpenCode 2
  OpenCode's own answer that it has no conversation to resume (see "OpenCode" in
  `docs/product/per-agent-launch-specifics.md`).

Anything else that keeps Octoboard from preparing its additions — a file of the user's it cannot carry into the
launch, a directory or file of its own it cannot write for the session's helpers, a configuration of the agent's still
waiting on a migration the CLI would run itself, a native program or launcher that would carry the additions but
cannot be found — starts the session plain instead, with the standing notice `launch_enhancements_unavailable`, which
nests the failure as its reason (its `reason`, `reason_code` and the failure's own parameters, the file or setting it
names among them), so the banner says why, and the failure's remedy where it has one.

## The plain launch

A plain launch starts the agent's CLI with only what a person would give it for this session: the conversation to
resume, the model and the task, each in the CLI's own spelling, and with the session's account through the agent's own
variable (see "The launch environment" in `docs/product/launching-agents.md`). On a Windows daemon host where the program the launch starts is a script
launcher (`.cmd`, `.bat` or `.ps1`), whose command line cannot carry free text, a new session's task is given as its
first message instead. What Octoboard sets in the environment stays, since a variable cannot make a CLI refuse to start:
the git hook dispatchers, the process tag and the session's variables.

Everything else of the launch is left out, and the session runs without what depends on it:

- **no Octoboard hooks**: the session's status follows what the program itself reports through its terminal, or,
  where it reports nothing, its terminal title (see "A session whose hooks are absent and whose program reports
  nothing" in `docs/product/turns-hooks-do-not-report.md`); the messages Octoboard holds for its start go in once its
  input box is drawn with no trust confirmation up, whether or not the agent sets a terminal title (see "Messages held
  until a session can take them" in `docs/product/message-delivery.md`); and a cancel writes nothing again that the agent may
  have dropped, since nothing tells a message the turn took from one the cancel dropped (see "Messages held until a
  session can take them" there);
- **no Octoboard MCP server**, so none of Octoboard's tools: a session that has an owner cannot `report` to it, and its
  owner is told nothing of the plain launch; only the user is, through the banner;
- **no role description**, no approval mode or permission preset chosen in Octoboard (the agent's own configuration
  decides what the session may do, as the notice that says why the launch is plain also says), no reasoning effort, no compatibility switches, no
  Claude Code status line and none of Octoboard's per-agent arrangements (a per-session home, a private server, an
  extension), and the cross-project guard's edit check is not in force (its git check still is, through the git
  hook dispatchers, which stay);
- **no conversation id for a new conversation**: the CLI was given none, so none is recorded, even for an agent whose
  id Octoboard otherwise chooses in advance, and the banner carries the standing notice `conversation_id_unknown`: the
  conversation cannot be resumed by its id, and the session's next start opens a new conversation. It is withdrawn once
  an id is recorded for the session.

## When the agent exits as it starts

A release of an agent's CLI may refuse an option Octoboard passes and exit at once. A launch's process **exited as it
started** when all of these hold:

- it ended by itself within **10 seconds** of its start: not stopped, archived, handed over or ended for a restart, and
  still the session's process, with the session neither archived nor deleted;
- it showed no sign of an interactive program before it ended: no hook or helper call of the launch reached Octoboard
  (a hook, a call to the session's MCP server, the status line, an extension's channel), it made no status report
  through its terminal, and it completed no terminal title. Turning on the alternate screen or bracketed paste is no
  such sign, since a CLI can do both and exit refusing its options;
- no key was typed into it from a window; the terminal's own answers to the program's queries do not count.

Such a launch is not recorded as an interruption: the session goes back to *starting*, its owner is not told, what is
queued for it stays queued, and it is started again once, one level down — with less where the agent's provider gives
such a launch for what the CLI wrote, otherwise plain. A launch with less that exits as it starts too is started plain.
A plain launch that exits as it starts ends the session as any exit does, interrupted, with the CLI's own message on
screen, as in a plain terminal; so at most three processes are started for one launch. When another operation holds
the session for longer than a moment as the process exits, it is not started again.

The level a session came up at is told once its process has shown a sign of an interactive program, or has outlived the
10 seconds: a launch with less raises the provider's own standing notice of what it left out, and a plain one the
standing notice `launch_retried_plain`, whose `detail` is what the CLI wrote as it exited (its first 2 KiB and its last
4 KiB where it wrote more). Nothing is raised for a launch that exits as it starts in turn. A level that stayed up is
remembered for that agent, program and version for **10 minutes**: a launch of the same agent program and version in
that time starts directly at that level, and still tells the user. A daemon that starts again remembers none.

## When the hooks never arrive

A launch with Octoboard's hooks expects its first hook either as the agent starts or with its first turn, as its
provider measured it (see `docs/product/per-agent-launch-specifics.md`). When one is due and none arrives within **30
seconds**, the launch is from then on read as one without hooks, as a plain launch is: its status follows what the
program reports (see "A session whose hooks are absent and whose program reports nothing" in
`docs/product/turns-hooks-do-not-report.md`), what Octoboard held for its
start goes in at its input box (see `docs/product/message-delivery.md`), and the banner carries the standing notice
`hooks_not_received`, with `conversation_id_unknown` besides where no conversation id is recorded for the session. The
30 seconds do not run while the agent's folder-trust confirmation is up, since no hook runs while it waits for a
person; they start again once it has gone, and a confirmation left unanswered raises nothing. A hook that arrives later withdraws
`hooks_not_received`, and the session is read as one with hooks again from then on. A launch whose plan left the
agent's hooks out expects none: what Octoboard holds for its start goes in at its input box from the start, as a plain
launch's does, and where it also has no conversation id of its own (it resumes none, and its agent
has none chosen in advance) it carries `conversation_id_unknown` from its start.
