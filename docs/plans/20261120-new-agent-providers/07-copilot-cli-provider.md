# GitHub Copilot CLI

> Goal: make a GitHub Copilot CLI session a full member of the board: launched and resumed with Octoboard's hooks, MCP
> server and role text injected, its status read from its hooks, its accounts listed with their sign-in state and
> signed in from Settings, its permission mode chosen in Settings.
> Completion criteria: in a development build with a signed-in Copilot plan, a console session and a project session
> start, show working, waiting for the user at a permission prompt and at a question, done and paused, call an
> Octoboard tool without a permission prompt, report to a parent, take an instruction, are interrupted by a parent,
> and resume after being archived, with the user's own Copilot configuration unchanged; Log in signs an account in
> through the device code shown in the login dialog; the permission setting takes effect at the next launch; the
> provider's tests, `pnpm test:rust`, `pnpm clippy:rust`, `pnpm check:windows`, the UI tests and the provider-neutral
> check pass; only Copilot CLI's own files changed.

The overview is `README.md` in this directory and Copilot CLI's facts are in `cli-fact-sheets.md`, "GitHub Copilot
CLI". This milestone needs `04-register-preview-providers.md` merged and runs beside the other providers'. It touches
only Copilot CLI's descriptor, its adapter directory, its test data, its presentation module and fixture, and its
section of `docs/agent-cli-reference.md`.

Copilot CLI's code cannot be read, but its own help topics document its hooks, files and variables thoroughly, and
every injection has a flag or a variable for one run.

## Technical design

### Launch

- [ ] A new session passes `--session-id=` with the id Octoboard allocated; a resume passes `--resume=` with the
  stored id, which fails for a session that has gone where `--session-id=` would start a new one.
- [ ] The first task is `-i`. The model is `--model` and the effort `--reasoning-effort`.
- [ ] A resume sets `COPILOT_DISABLE_RESUME_AUTO_CD`, so the session runs in the directory Octoboard launches it in.
- [ ] A pinned account's directory is exported as `COPILOT_HOME`.
- [ ] The chosen permission mode becomes the flag of that mode; with none chosen no flag is passed and Copilot's own
  settings decide.

### Injection

- [ ] The hooks are a plugin directory Octoboard writes in the session's scratch directory, loaded with
  `--plugin-dir`. Its hook entries run Octoboard's hook forwarder without a shell, with a short timeout.
- [ ] The Octoboard MCP server is `--additional-mcp-config`, as a file in the scratch directory, and an allow rule
  for that server's tools is passed so they raise no prompt.
- [ ] The role text is an instruction file in a scratch directory that `COPILOT_CUSTOM_INSTRUCTIONS_DIRS` names,
  added to whatever the user's shell already sets there. If the measurement shows the text is not read from there on
  every launch and resume, the alternative is the session-start hook's added context.

### Hooks

- [ ] Copilot's native events are registered, not its Claude-compatible names, because only the native set includes
  the permission request, the notification and the error. Each entry passes its event's name to the forwarder
  (`--event`) if the measurement shows the native payload does not name it.
- [ ] Copilot's `HookInterpreter` reads them: `sessionStart` announces the session's start; `userPromptSubmitted`
  starts a turn, starts the conversation and carries the prompt; `preToolUse`, `postToolUse` and
  `postToolUseFailure` open and close a tool call, `ask_user` waiting on the user from its start;
  `permissionRequest` and the `permission_prompt` and `elicitation_dialog` notifications are waiting for the user;
  `agentStop` ends the turn; `errorOccurred` that is not recoverable ends it abnormally; `subagentStart` and
  `subagentStop`, and any event carrying an agent id, belong to a subagent and only change the background work;
  the completion notifications of background shells and agents change the background work too.
- [ ] The agent's session id is the payload's `sessionId`.

### Screens

- [ ] Copilot's folder-trust prompt is its trust screen, pressed under the shared permission, choosing the option
  that remembers the folder so later launches do not ask.
- [ ] The interrupt key is as measured; its help documents Ctrl-C as stopping a turn at once and Esc twice as
  interrupting it.
- [ ] Copilot's own draft stash (Ctrl-S) is not used in this milestone.
- [ ] The title, which shows what the agent is working on, gives the task title where the measurement shows a stable
  form.

### Accounts and models

- [ ] `AccountOps` for Copilot answers the sign-in reading and the account's name from the home's record of
  signed-in accounts, as measured with the user's account, and a login flow that runs `copilot login --device-code`
  with no terminal and reports the page and the code to confirm, as Grok Build's does.
- [ ] A token in `COPILOT_GITHUB_TOKEN`, `GH_TOKEN` or `GITHUB_TOKEN` in the user's shell overrides every stored
  login. Whether such an environment makes every account read as signed in is measured and written down, not worked
  around.
- [ ] No quota in this milestone. The model catalogue is left empty; listing models through ACP is later work.

### Descriptor

- [ ] The values of `04-register-preview-providers.md` confirmed or corrected, with the measured prefix and separator
  of an MCP tool's name, the session marker variables (`COPILOT_CLI`, `COPILOT_AGENT_SESSION_ID`), and how its CLI is
  installed and updated (npm package `@github/copilot`; `copilot update`).
- [ ] Whether the provider starts the native executable in place of the npm loader
  (`AgentProvider::native_binary`), decided by the measurement of how a stop reaches the agent through the loader.
- [ ] One setting, `approval_mode`, a choice whose values are the permission modes the measurement confirms can be
  set for an interactive session by a flag, the one that approves everything asking for confirmation; exported, not
  shown to agents.

## Implementation plan

Measure first. Without a login, in a scratch home and working directory:

- [ ] That a plugin directory given by `--plugin-dir` loads its hooks without a trust decision or an install step,
  and what an untrusted folder changes.
- [ ] The trust prompt's text and keys, and where the decision is recorded.
- [ ] What `copilot login --device-code` prints with no terminal, up to the code; nothing is authorized.
- [ ] What `--session-id=` and `--resume=` do for an id that exists and one that does not.

With the user signed in, since Copilot does nothing without a sign-in or a model provider of the user's own:

- [ ] Every native hook event's payload: whether it names its event, the session id, the turn and tool call ids, the
  final message; what fires on Esc twice, on Ctrl-C, on a declined permission and around `ask_user`.
- [ ] The name an Octoboard tool reaches the model under, that the allow rule covers it, and what the model is told
  when a call outlasts the timeout.
- [ ] Whether the role text in the instructions directory reaches the model, once, on a new session and on a resume;
  whether a console directory holding `AGENTS.md`, `CLAUDE.md` and `GEMINI.md` as links to one file gives Copilot the
  text once.
- [ ] How the stored login is kept across several Copilot homes, and what the home records of the signed-in account.
- [ ] The title in each state; the interrupt key; a bracketed paste followed by Enter; what a stop through the npm
  loader leaves behind.
- [ ] Which permission modes a flag can set for an interactive session.

Then build:

- [ ] The launch plan and the plugin directory, with tests of the arguments and of the files written.
- [ ] The hook interpreter, with the measured payloads as test data.
- [ ] The screen reader's measured answers.
- [ ] The sign-in reading and the login flow, with fixtures of the CLI's output.
- [ ] The setting and its copy; the descriptor's remaining facts.
- [ ] Copilot CLI's section of `docs/agent-cli-reference.md`, with the version each fact was measured on.

## Later, not in this milestone

The plan's budget and its Session and Weekly limits in the quota panel, the model list through ACP, its records
(history, context size, removal, a missing conversation), carrying a conversation between accounts, sharing the default
configuration, its draft stash, and anything on Windows beyond the cross-check.

## Notes for the developer

**Reusable capabilities**

- Octoboard's hook forwarder, with the optional event name from `01-provider-neutral-repairs.md`.
- The login machinery: a provider's flow runs the CLI in the background, reports the page and a device code, and is
  cancelled by dropping it.
- The sign-in reading from `03-accounts-without-plan-quota-or-browser-sign-in.md`.
- Starting a native program in place of a launcher on `PATH`, which the provider interface already offers.
- The hold on tool calls that wait on the user; the trust mechanism; provider settings.

**Development notes**

- A `preToolUse` hook that exits non-zero denies the call. Octoboard's forwarder exits 0 whatever happens; keep it
  the only thing a hook entry runs.
- The login flow completes in the user's own browser. Ask before running one that could, and never authorize in a
  probe.
- Copilot syncs session history to the user's GitHub account when signed in. Octoboard changes no setting of the
  user's for it; what a probe session leaves there is told to the user.
- A permission setting is not shown to agents; its heading copy lands with it.
- If something needs a change outside Copilot CLI's files, stop and report it.

**Reference docs**

- `docs/conventions/provider-neutral-core.md`
- `docs/conventions/error-codes.md`, "Provider codes"
- `docs/conventions/i18n-in-code.md`
- `docs/product/session-statuses.md`, "What the statuses are derived from"
- `docs/product/logging-accounts-in.md`
- `docs/product/per-agent-settings.md`, "Provider settings" and "Approval modes"
- `docs/product/folder-trust.md`, "How a confirmation is pressed"
- `docs/agent-cli-reference.md`, "Signing in a config directory without a terminal" and "Injecting Octoboard into
  each agent"
