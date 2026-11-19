# Qwen Code

> Goal: make a Qwen Code session a full member of the board: launched and resumed with Octoboard's hooks, MCP server
> and role text injected, its status read from its hooks, its accounts listed with their sign-in state, its permission
> mode chosen in Settings.
> Completion criteria: in a development build with a signed-in Qwen Code, a console session and a project session
> start, show working, waiting for the user at a permission prompt and at a question, done and paused, call an
> Octoboard tool without a permission prompt, report to a parent, take an instruction, are interrupted by a parent,
> and resume after being archived, with the user's own Qwen Code configuration unchanged; the permission setting takes
> effect at the next launch; a signed-out account reads Not logged in with the way to sign in; the provider's tests,
> `pnpm test:rust`, `pnpm clippy:rust`, `pnpm check:windows`, the UI tests and the provider-neutral check pass; only
> Qwen Code's own files changed.

The overview is `README.md` in this directory and Qwen Code's facts are in `cli-fact-sheets.md`, "Qwen Code". This
milestone needs `04-register-preview-providers.md` merged and runs beside the other providers'. It touches only Qwen
Code's descriptor, its adapter directory, its test data, its presentation module and fixture, and its section of
`docs/agent-cli-reference.md`.

Qwen Code has no bundled docs and was read from minified code, so more of this milestone is measurement than for the
others; but its flags and hook names are close to Claude Code's.

## Technical design

### Launch

- [ ] A new session passes `--session-id` with the id Octoboard allocated; a resume passes `--resume`.
- [ ] The first task is `-i`, which runs the prompt and stays interactive; a positional prompt is one-shot and is
  never used.
- [ ] The model is `--model`. Qwen Code has no effort flag.
- [ ] The role text is `--append-system-prompt`; the Octoboard MCP server is `--mcp-config`, as a file in the
  session's scratch directory, with the server trusted so its tools raise no prompt and a timeout above Octoboard's
  own forwarding limit.
- [ ] A pinned account's directory is exported as `QWEN_HOME`.
- [ ] The chosen permission mode is `--approval-mode`; with none chosen the flag is left out and Qwen Code's own
  configuration decides.

### Hooks

- [ ] The hooks are injected through a settings file in the session's scratch directory named by
  `QWEN_CODE_SYSTEM_SETTINGS_PATH`, carrying over the content of the system settings file it replaces, if the
  measurement shows hook lists are added across layers as in Gemini CLI. If they are not, the alternative to measure is
  the structured event output Qwen Code writes beside its TUI (`--json-file`), read by the adapter and applied as
  events.
- [ ] Each hook is Octoboard's hook command with a short timeout, asynchronous where Qwen Code allows it.
- [ ] Qwen Code's `HookInterpreter` reads its events: `SessionStart` announces the session's start;
  `UserPromptSubmit` starts a turn, starts the conversation and carries the prompt; `PreToolUse` and `PostToolUse`
  (and its failure form) open and close a tool call, the ask-the-user tool waiting on the user from its start;
  `PermissionRequest`, or the notification Qwen Code sends for a permission prompt, is waiting for the user; `Stop`
  ends the turn with its final message where the payload carries one; `StopFailure` ends it abnormally;
  `SubagentStart` and `SubagentStop` change the background work and are never the session's own events.
- [ ] What its hooks do not report (a cancelled turn, a declined prompt) is closed from its terminal title, whose
  prefix shows a running turn and a waiting confirmation (`ScreenReader::title_activity`, `title_needs_user`,
  `title_cancel`), where the measurement shows the title is reliable.

### Screens

- [ ] Qwen Code's folder-trust dialog is its trust screen, pressed under the shared permission.
- [ ] The key that interrupts a turn, as measured.
- [ ] The title without its status prefix and its `Qwen - <folder>` default gives the task title.

### Accounts and models

- [ ] `AccountOps` for Qwen Code answers the sign-in reading from the account's settings and credential record, as
  measured with the user's account. No login flow and no quota in this milestone; `signInHint` says how to sign in
  inside Qwen Code.
- [ ] The model catalogue is left empty unless the measurement finds the models in a file Qwen Code keeps; listing
  them through ACP is later work.

### Descriptor

- [ ] The values of `04-register-preview-providers.md` confirmed or corrected, plus the session marker variables Qwen
  Code sets, and how its CLI is installed and updated (npm package `@qwen-code/qwen-code`; `qwen update`).
- [ ] One setting, `approval_mode`, a choice of `plan`, `default`, `auto_edit`, `auto` and `yolo`, the last asking
  for confirmation; exported, not shown to agents. Its copy, in every language, is in the presentation module.

## Implementation plan

Measure first. Without a login, in a scratch home and working directory, with a local stand-in for the model where a
turn is needed:

- [ ] How hook lists and MCP servers merge between the system settings file, the user's and the project's; whether
  hooks need an approval before they run; whether they run in an untrusted folder; what a failing hook costs.
- [ ] Every hook event's payload: the field that names the event, the session id, the turn id, the tool call id, the
  final message; what fires on Esc, on a declined permission and around the ask-the-user tool.
- [ ] What the structured event output holds, in case the hooks cannot be injected.
- [ ] Whether the id given to `--session-id` must be a UUID; what `--resume` does for an id that has gone.
- [ ] Whether Qwen Code relaunches itself as a child process, and whether stopping the session reaches the child
  (`QWEN_CODE_NO_RELAUNCH`).
- [ ] The trust dialog's text and keys; the interrupt key; the title in each state; a bracketed paste followed by
  Enter.
- [ ] Whether a role given on a resume replaces, repeats or is ignored.
- [ ] Whether a console directory holding `QWEN.md` and `AGENTS.md` as links to one file gives Qwen Code the text
  once.

With the user signed in:

- [ ] Which sign-in the user's Qwen account is (subscription, Coding Plan or a key), where its record is kept, and
  what the signed-in and signed-out states look like on disk.
- [ ] A whole turn through a real model, with a permission prompt and a question.

Then build:

- [ ] The launch plan, with tests of its arguments and of the files written to the scratch directory.
- [ ] The hook interpreter, with the measured payloads as test data.
- [ ] The screen reader's measured answers.
- [ ] The sign-in reading; the setting and its copy; the descriptor's remaining facts.
- [ ] Qwen Code's section of `docs/agent-cli-reference.md`, with the version each fact was measured on.

## Later, not in this milestone

The model list through ACP, a login flow if one can be driven, the quota of a subscription, its records (history,
context size, removal, a missing conversation), carrying a conversation between accounts, sharing the default
configuration, and anything on Windows beyond the cross-check.

## Notes for the developer

**Reusable capabilities**

- Octoboard's hook command and per-session hook script, and their timeouts.
- The command every adapter registers as the session's MCP server, and the limit an agent's own tool timeout is set
  above.
- The hold on a session's tool calls that wait on the user (the tool call changes an interpreter reports), which keeps
  a hand raised while another call of the same turn ends.
- The title observer and the record of what a title says about a turn.
- Provider settings: declared in the descriptor and read from the launch's settings; the store, the wire, the export
  and the Settings page follow the declaration.

**Development notes**

- Hooks must fail fast and silently: a short explicit timeout on every one, nothing on stdout or stderr.
- Qwen Code's hook names look like Claude Code's. Its interpreter is still its own: read no payload field that was
  not measured on Qwen Code, and share no code with another agent's adapter.
- A permission setting is not shown to agents, and a choice that lets sessions run every action asks for
  confirmation.
- The setting's heading copy lands with the setting: Settings silently leaves out a setting without one.
- If something needs a change outside Qwen Code's files, stop and report it.

**Reference docs**

- `docs/conventions/provider-neutral-core.md`
- `docs/conventions/error-codes.md`, "Provider codes"
- `docs/conventions/i18n-in-code.md`
- `docs/product/session-statuses.md`, "What the statuses are derived from"
- `docs/product/turns-hooks-do-not-report.md`
- `docs/product/per-agent-settings.md`, "Provider settings" and "Approval modes"
- `docs/product/folder-trust.md`, "How a confirmation is pressed"
- `docs/agent-cli-reference.md`, "Claude Code" and "Injecting Octoboard into each agent"
