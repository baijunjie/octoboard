# Gemini CLI

> Goal: make a Gemini CLI session a full member of the board: launched and resumed with Octoboard's hooks, MCP server
> and role text injected, its status read from its hooks and its title, its accounts listed with their sign-in state
> and email, its models offered, its approval mode chosen in Settings.
> Completion criteria: in a development build with a Google sign-in, a console session and a project session start,
> show working, waiting for the user at a permission prompt and at a question, done and paused, call an Octoboard tool
> without a permission prompt, report to a parent, take an instruction, are interrupted by a parent, and resume after
> being archived, with the user's own Gemini CLI configuration unchanged; a turn cancelled in the terminal returns the
> session to its prompt; an account whose directory is not named `.gemini` is refused with the reason; the model
> picker lists the account's models; the approval setting takes effect at the next launch; the provider's tests,
> `pnpm test:rust`, `pnpm clippy:rust`, `pnpm check:windows`, the UI tests and the provider-neutral check pass; only
> Gemini CLI's own files changed.

The overview is `README.md` in this directory and Gemini CLI's facts are in `cli-fact-sheets.md`, "Gemini CLI". This
milestone needs `04-register-preview-providers.md` merged and runs beside the other providers'. It touches only Gemini
CLI's descriptor, its adapter directory, its test data, its presentation module and fixture, and its section of
`docs/agent-cli-reference.md`.

Gemini CLI's code and docs are readable, so less is unknown than for the others; its difficulties are three facts
already known: no flag appends a role, hooks do not run in a folder it does not trust, and its directory variable
names the parent.

## Technical design

### Launch

- [ ] A new session passes `--session-id` with the id Octoboard allocated; a resume passes `--resume` with the stored
  id. The first task is the positional prompt. The model is `--model`; Gemini CLI has no effort flag.
- [ ] A pinned account's directory, which is named `.gemini`, is exported as its parent in `GEMINI_CLI_HOME` through
  the layout's conversion. The provider refuses an account directory of another name
  (`AgentProvider::refuse_unlaunchable_account`) with a code of its own, and its `dirHint` says the name is fixed.
- [ ] The chosen approval mode is `--approval-mode`; with none chosen the flag is left out.

### Injection

- [ ] Hooks and the MCP server are a settings file in the session's scratch directory named by
  `GEMINI_CLI_SYSTEM_SETTINGS_PATH`. It carries over the content of the system settings file that the variable
  replaces (the one at the platform's default path, or the one the user's shell already names), so an administrator's
  settings still apply.
- [ ] Its `mcpServers.octoboard` entry is trusted, so the tools raise no prompt, and its timeout is above Octoboard's
  own forwarding limit.
- [ ] The role text is given by whichever of these the measurement shows reaches the model once on a new session and
  on a resume, tried in this order: a context file in a scratch directory the injected settings include; the
  `SessionStart` hook's added context. `GEMINI_SYSTEM_MD` is never used, since it replaces Gemini CLI's own prompt.
  If only the second works, it needs a hook that prints, which is a gap to report (`README.md`, "Open").

### Hooks and title

- [ ] Each registered hook is Octoboard's hook command with a short explicit timeout, since Gemini CLI waits for
  every hook and its default is a minute. The per-chunk model events are not registered.
- [ ] Gemini CLI's `HookInterpreter` reads its events: `SessionStart` announces the session's start; `BeforeAgent`
  starts a turn, starts the conversation and carries the prompt; `BeforeTool` and `AfterTool` open and close a tool
  call, `ask_user` waiting on the user from its start; the `ToolPermission` notification is waiting for the user;
  `AfterAgent` ends the turn with `prompt_response` as its final message.
- [ ] The title is the second signal (`ScreenReader::title_activity`, `title_needs_user`, `title_cancel`): ✦ and ⏲
  are a running turn, ◇ is the prompt, ✋ is a wait on the user. It closes the turn of a cancel no hook reports, and
  it is the only signal in a folder the user declined to trust, where no hook runs.
- [ ] The title names only the folder, so it gives no task title.

### Screens

- [ ] Gemini CLI's folder-trust dialog is its trust screen, pressed under the shared permission with "Trust folder".
  Octoboard passes neither `--skip-trust` nor `GEMINI_CLI_TRUST_WORKSPACE`: the trust decision stays the user's and is
  recorded by Gemini CLI itself.
- [ ] The key that interrupts a turn, as measured.

### Accounts and models

- [ ] `AccountOps` for Gemini CLI answers the sign-in reading from the account's auth selection and credential
  record, and the email from its record of the active Google account. Log in is offered only if the measurement
  shows the Google sign-in can be driven without a terminal; otherwise `signInHint` says to sign in inside Gemini CLI.
- [ ] The model catalogue lists what `gemini models` prints for the account.
- [ ] No quota in this milestone.

### Descriptor

- [ ] The values of `04-register-preview-providers.md` confirmed or corrected, plus the session marker variable
  (`GEMINI_CLI`), and how its CLI is installed and updated (npm package `@google/gemini-cli`, Homebrew formula
  `gemini-cli`).
- [ ] One provider code, an error, for an account directory that is not named `.gemini`.
- [ ] One setting, `approval_mode`, a choice of `default`, `auto_edit`, `plan` and `yolo`, the last asking for
  confirmation; exported, not shown to agents.

## Implementation plan

Measure first. Without a login, in a scratch home and working directory, with a local stand-in for the model where a
turn is needed:

- [ ] That hooks and an MCP server in the file `GEMINI_CLI_SYSTEM_SETTINGS_PATH` names load beside the user's and a
  project's, in a trusted folder and in an untrusted one; what a failing or slow hook costs.
- [ ] The trust dialog: whether it appears by default, its text and keys, where it records, and what the title and
  the hooks do before it is answered.
- [ ] Every registered event's payload; what fires on Esc, on a declined permission and around `ask_user`; the order
  of the permission notification against the dialog.
- [ ] The two role mechanisms, on a new session and on a resume: how many times the text reaches the model.
- [ ] What `--session-id` and `--resume` do for an id that exists, one that has gone, and one of another project;
  whether an explicit id must be a UUID.
- [ ] The title in each state, including with the user's `ui.hideWindowTitle` or `ui.dynamicWindowTitle` off; the
  interrupt key; a bracketed paste followed by Enter.
- [ ] The name an Octoboard tool reaches the model under, and what the model is told when a call outlasts the
  timeout.
- [ ] Whether a console directory holding `GEMINI.md` beside the other providers' instruction files gives Gemini CLI
  the text once.
- [ ] What it leaves running when it exits with a command still running, and whether it finishes on the host's
  request to end.

With the user signed in with their Google account:

- [ ] Where the login is kept (the credential file or the system keychain), what the signed-out and signed-in states
  look like on disk, and whether ACP's `authenticate` runs the Google sign-in with no terminal.
- [ ] What `gemini models` prints signed in and signed out.
- [ ] A whole turn through a real model, with a permission prompt and a question.

Then build:

- [ ] The launch plan and the settings file, with tests of the arguments, of the file written and of the content
  carried over from an existing system settings file.
- [ ] The refusal of a wrongly named account directory, with its code and copy.
- [ ] The hook interpreter, with the measured payloads as test data.
- [ ] The screen reader's measured answers, titles included.
- [ ] The sign-in reading, the email and the model catalogue, with fixtures.
- [ ] The setting and its copy; the descriptor's remaining facts.
- [ ] Gemini CLI's section of `docs/agent-cli-reference.md`, with the version each fact was measured on.

## Later, not in this milestone

The per-model daily quota and the window kind it needs, a login flow if ACP can drive one, its records (history,
context size, removal, and that Gemini CLI itself deletes sessions after 30 days), carrying a conversation between
accounts, sharing the default configuration, and anything on Windows beyond the cross-check.

## Notes for the developer

**Reusable capabilities**

- The layout's parent-directory conversion from `02-descriptor-facts-for-new-agent-shapes.md`, and the leading
  glyphs added there.
- Octoboard's hook command and its timeouts; the command registered as the session's MCP server.
- The title observer and the watch that closes a turn whose title went back to the prompt with no hook.
- The trust mechanism; the hold on tool calls that wait on the user; provider settings and provider codes.
- The bounded reads of files an adapter reads, for the system settings file carried over.

**Development notes**

- A hook's stdout must be empty or JSON; Octoboard's forwarder prints nothing.
- The settings file is Octoboard's own, written per session; the user's and the system's files are read, never
  written.
- Gemini CLI writes into its directory even for `--help`. Every probe uses a scratch home.
- A provider code is declared in the descriptor and worded in the presentation module, in every language.
- If something needs a change outside Gemini CLI's files, stop and report it.

**Reference docs**

- `docs/conventions/provider-neutral-core.md`
- `docs/conventions/error-codes.md`, "Provider codes"
- `docs/conventions/daemon-processes-and-io.md`
- `docs/conventions/i18n-in-code.md`
- `docs/product/session-statuses.md`, "What the statuses are derived from"
- `docs/product/turns-hooks-do-not-report.md`
- `docs/product/folder-trust.md`
- `docs/product/per-agent-settings.md`, "Provider settings" and "Approval modes"
- `docs/product/session-titles.md`, "Leading activity glyphs"
- `docs/agent-cli-reference.md`, "Injecting Octoboard into each agent"
