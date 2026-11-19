# OpenCode

> Goal: make an OpenCode session a full member of the board: launched and resumed with Octoboard's plugin, MCP server
> and role text injected, its status read from the events its plugin forwards, its one account listed with its sign-in
> state, its models offered.
> Completion criteria: in a development build with a provider key signed in, a console session and a project session
> start, show working, waiting for the user at a permission prompt and at a question, done and paused, call an
> Octoboard tool without a permission prompt, report to a parent, take an instruction, are interrupted by a parent,
> and resume after being archived, with the user's own OpenCode configuration unchanged; Settings shows OpenCode's
> default account alone, with no Add account; the model picker lists the account's models; the provider's tests,
> `pnpm test:rust`, `pnpm clippy:rust`, `pnpm check:windows`, the UI tests and the provider-neutral check pass; only
> OpenCode's own files changed.

The overview is `README.md` in this directory and OpenCode's facts are in `cli-fact-sheets.md`, "OpenCode". This
milestone needs `04-register-preview-providers.md` merged and runs beside the other providers'. It touches only
OpenCode's descriptor, its adapter directory, its test data, its presentation module and fixture, and its section of
`docs/agent-cli-reference.md`.

OpenCode is the furthest from the first three agents: no hooks, a database instead of record files, no variable for
its data directory. By the user's decision it has the default account only, and Octoboard takes no key for it
(`README.md`, "Key design decisions"). It targets the generation installed today; the vendor already documents the
next one.

## Technical design

### Launch

- [ ] A new session passes no id: OpenCode takes none. The agent's session id is learned from the first event the
  plugin forwards. A resume passes `--session` with the stored id.
- [ ] The first task is `--prompt`. The model is `--model`, in OpenCode's `provider/model` spelling. OpenCode's TUI
  has no effort flag.
- [ ] The chosen permission mode `auto` is `--auto`; with none chosen nothing is passed.
- [ ] No account directory is exported: OpenCode has the default account only (`Capabilities::owned_accounts` off).

### Injection

- [ ] Octoboard's configuration for the session is a directory in the scratch directory named by
  `OPENCODE_CONFIG_DIR`: its config file holds the Octoboard MCP server, the role text as an instruction file, and a
  permission rule that allows that server's tools; its plugin directory holds Octoboard's plugin.
- [ ] Where the user's shell already sets `OPENCODE_CONFIG_DIR`, the user's directory must keep applying. The
  alternative to measure for that case is the inline config (`OPENCODE_CONFIG_CONTENT`), merged over what the user's
  shell already carries there, with the plugin named in its plugin list.
- [ ] The MCP entry's timeout is above Octoboard's own forwarding limit.

### The plugin

- [ ] The plugin is a JavaScript file kept as an asset of OpenCode's adapter directory, written into the scratch
  configuration directory at launch with a data file holding what differs per session.
- [ ] It forwards a fixed, small set of OpenCode's bus events through the session's hook script, each as a payload
  Octoboard defines: the event's name under the key the generic layer reads, the OpenCode session id, and only the
  fields the interpreter uses. It forwards no streaming part updates.
- [ ] It marks an event of a child session (one with a parent) as a subagent's, so it never moves the session's own
  status or replaces its session id.
- [ ] It never throws into OpenCode and never prints: a failure to reach the daemon is silent.

### Events

- [ ] OpenCode's `HookInterpreter` reads the plugin's payloads: the first event of the root session gives the agent
  session id and announces the session's start; a status of busy starts a turn and, on the first one, the
  conversation; a permission asked or a question asked is waiting for the user, and its reply ends that wait; idle
  ends the turn, with the last assistant message the plugin read for it; a session error ends it abnormally.
- [ ] The user's prompt, where the plugin can read it from the message that starts a turn, is the submitted prompt.

### Screens

- [ ] The key that interrupts a turn, as measured.
- [ ] The title `OC | <title>` gives the task title; `OpenCode` alone gives none.
- [ ] OpenCode is not known to have a folder-trust confirmation; the default, a screen never sighted, stands unless
  the measurement finds one.

### Account and models

- [ ] `AccountOps` for OpenCode answers the sign-in reading only: signed in when the default account holds a
  credential for some model provider, as measured from its credential record or from `opencode auth list`. No login
  flow, no quota. `signInHint` names `opencode auth login`.
- [ ] The model catalogue lists what `opencode models` prints.

### Descriptor

- [ ] The values of `04-register-preview-providers.md` confirmed or corrected, including what a tool's name is built
  from, the ask-the-user tool if OpenCode's is on by default, the session marker variables it sets (`OPENCODE`,
  `OPENCODE_PID`, `AGENT`), and how its CLI is installed and updated (npm package `opencode-ai`, its own installer
  and `opencode upgrade`).
- [ ] One setting, `approval_mode`, a choice of `ask` and `auto`, the second asking for confirmation; exported, not
  shown to agents.

## Implementation plan

Measure first. Without a login, in a scratch home (and scratch XDG directories) and working directory, with a local
stand-in for the model where a turn is needed:

- [ ] That a plugin in the directory `OPENCODE_CONFIG_DIR` names loads beside the user's global and project
  configuration, and how that directory, `OPENCODE_CONFIG` and `OPENCODE_CONFIG_CONTENT` rank and merge; whether the
  plugin list takes a file path.
- [ ] Every bus event the interpreter needs, with its fields: the session's creation, status, idle and error, the
  permission and question pairs, the message that carries the user's prompt and the one that carries the final
  answer; which of them a child session fires; what fires on an interrupt.
- [ ] What happens to the reported session when the user starts another session inside the same TUI, and what the
  provider does about it.
- [ ] The name an Octoboard tool reaches the model under, that the permission rule covers it, the timeout key and
  what the model is told when a call outlasts it.
- [ ] Whether the role file named in the instructions reaches the model once on a new session and on a resume;
  whether a console directory holding `AGENTS.md` and `CLAUDE.md` as links to one file gives OpenCode the text once.
- [ ] What `--session` does for an id that has gone; the interrupt key; the title; a bracketed paste followed by
  Enter; whether OpenCode has any confirmation at a first run in a folder.
- [ ] What it leaves running when it exits with a command still running, and whether it finishes on the host's
  request to end.

With the user signed in with a provider key:

- [ ] What `opencode auth list` and `opencode models` print signed in and signed out.
- [ ] A whole turn through a real model, with a permission prompt and a question.

Then build:

- [ ] The launch plan, the scratch configuration directory and the plugin, with tests of the arguments and of the
  files written.
- [ ] The hook interpreter, with the measured events as test data.
- [ ] The screen reader's measured answers.
- [ ] The sign-in reading and the model catalogue, with fixtures of the CLI's output.
- [ ] The setting and its copy; the descriptor's remaining facts.
- [ ] OpenCode's section of `docs/agent-cli-reference.md`, with the version each fact was measured on and a note of
  which facts the vendor's next generation changes.

## Later, not in this milestone

More than one account, its records (which need a record store that is not file-shaped: history, context size, removal,
a missing conversation), delivery of messages through OpenCode's own server with the TUI attached to it, and anything
on Windows beyond the cross-check.

## Notes for the developer

**Reusable capabilities**

- Octoboard's per-session hook script: the plugin runs it with a payload on its standard input, and holds no key.
- The hook application path, which reads the agent's session id from a payload for an agent that cannot be given one,
  as it does for Codex.
- The subagent rule of hook application: an event marked as a subagent's changes only the background work.
- The command registered as the session's MCP server; provider settings.
- `PrivateServer` and `MessageTransport`, for the later delivery through OpenCode's server.

**Development notes**

- The plugin runs inside OpenCode's process on every event. Keep it to forwarding: no network call of its own, no
  work on the event path that can block a turn.
- The payload is Octoboard's own format; version it in one place, and keep the interpreter tolerant of a field it
  does not find.
- Anything set in the launch environment is inherited by every program the agent starts. `OPENCODE_CONFIG_DIR` is
  OpenCode's own variable; do not set an XDG variable.
- If something needs a change outside OpenCode's files, stop and report it.

**Reference docs**

- `docs/conventions/provider-neutral-core.md`
- `docs/conventions/error-codes.md`, "Provider codes"
- `docs/conventions/i18n-in-code.md`
- `docs/product/session-statuses.md`, "What the statuses are derived from"
- `docs/product/per-agent-settings.md`, "Provider settings" and "Approval modes"
- `docs/product/message-delivery.md`, "Messages held until a session can take them"
- `docs/agent-cli-reference.md`, "Codex" and "Injecting Octoboard into each agent"
