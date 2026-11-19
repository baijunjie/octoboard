# Pi

> Goal: make a Pi session a full member of the board: launched and resumed with Octoboard's MCP server and role text
> injected, its status taken from Pi's own program status reports, its accounts listed with their sign-in state, its
> models offered.
> Completion criteria: in a development build with a signed-in Pi, a console session and a project session of Pi
> start, show working, waiting for the user, done and paused as Pi reports them, call an Octoboard tool, report to a
> parent, take an instruction written into them, are interrupted by a parent, and resume after being archived, with the
> user's own Pi configuration unchanged; the model picker lists the account's models; a signed-out account reads Not
> logged in with the way to sign in; the provider's tests, `pnpm test:rust`, `pnpm clippy:rust`, `pnpm check:windows`,
> the UI tests and the provider-neutral check pass; only Pi's own files changed.

The overview is `README.md` in this directory and Pi's facts are in `cli-fact-sheets.md`, "Pi". This milestone needs
`04-register-preview-providers.md` merged and runs beside the other providers'. It touches only Pi's descriptor, its
adapter directory, its test data, its presentation module and fixture, and its section of
`docs/agent-cli-reference.md`.

Pi is the easiest of the five: its flags cover the launch, and it reports its own status in the protocol Octoboard's
terminals already read.

## Technical design

### Status

- [ ] Pi keeps no status integration of its own in this milestone (`AgentProvider::has_status_integration` stays
  false), so its sessions take their status from the OSC 7501 reports Pi makes by itself: `working` as working,
  `blocked` as waiting for the user, `done` and `idle` as at its prompt, `error` as at its prompt and paused
  (`docs/product/program-status-reports.md`, "From the root record to the session's status").
- [ ] The launch does not force reporting on. If the measurement shows Pi does not report in an Octoboard terminal
  without it, the launch sets `PI_PROGRAM_STATUS=1`.

### Launch

- [ ] A new session passes `--session-id` with the id Octoboard allocated; a resume passes `--session` with the
  stored id, never `--session-id`, which would quietly create a new session where the old one has gone.
- [ ] The model is `--model`, in Pi's `provider/id` spelling; the reasoning effort is `--thinking`.
- [ ] The role text is `--append-system-prompt`, given as a file in the session's scratch directory.
- [ ] A pinned account's directory is exported as `PI_CODING_AGENT_DIR`.
- [ ] Octoboard's extension is loaded with `--extension`, on every launch and resume.

### The extension

- [ ] The extension is a JavaScript file kept as an asset of Pi's adapter directory and written into the session's
  scratch directory at launch, with a data file beside it holding what differs per session.
- [ ] It registers the Octoboard MCP server for the session with direct exposure, so the model is offered the tools
  under `mcp__octoboard__<tool>`, and with a timeout above Octoboard's own forwarding limit
  (`MCP_TOOL_TIMEOUT_SECS`).
- [ ] It forwards Pi's `session_start` through the session's hook script, and Pi's hook interpreter answers
  `announces_session_start` for it, so that messages held for the session are released when Pi is up rather than at
  the fallback. It forwards nothing else in this milestone.
- [ ] It never throws into Pi and never prints: a failure to reach the daemon is silent, like a hook's.

### Screens

- [ ] The project-trust prompt Pi shows for a folder with protected `.pi` resources is Pi's trust screen
  (`ScreenReader::trust_screen`), pressed under the same permission as the other agents'. Octoboard does not pass
  `--approve`.
- [ ] The key that interrupts a turn (`ScreenReader::interrupt`), as measured.
- [ ] Pi's title, where it carries a session name, gives the task title (`AgentProvider::task_title`).

### Accounts and models

- [ ] `AccountOps` for Pi answers the sign-in reading only: signed in when the account holds a usable credential for
  some model provider, as measured from `auth.json` or from what Pi itself lists as usable. No login flow, no quota.
- [ ] The presentation module's `signInHint` names `pi auth login`.
- [ ] The model catalogue lists what `pi --list-models` prints for the account, with the thinking levels Pi takes.

### Descriptor

- [ ] The values of `04-register-preview-providers.md` confirmed or corrected, plus the session marker variables Pi
  sets for the processes it starts, and how its CLI is installed and updated (npm package
  `@earendil-works/pi-coding-agent`; `pi update`).
- [ ] No setting: Pi has no permission modes to choose between.

## Implementation plan

Measure first. Without a login, in a scratch home and working directory:

- [ ] Whether Pi asks the support query and reports in a terminal that answers it, and what it reports at start, on a
  prompt, at an extension dialog, on Esc and on an error. A local stand-in can answer the model request.
- [ ] What `--session-id` and `--session` do for an id that exists, one that does not, and one from another directory.
- [ ] That an extension given by `--extension` loads without a trust decision, that a server it registers reaches the
  model under the expected name, and what the model is told when a call outlasts the timeout.
- [ ] The trust prompt's text and keys; the interrupt key; the title; what the input box does with a bracketed paste
  followed by Enter.
- [ ] Whether a role given by `--append-system-prompt` on a resume replaces, repeats or is ignored.
- [ ] Whether a console directory holding `AGENTS.md`, `CLAUDE.md`, `GEMINI.md` and `QWEN.md` as links to one file
  gives Pi the text once.
- [ ] What Pi leaves running when it exits with a command still running, and whether it finishes on the host's
  request to end.

With the user signed in:

- [ ] What `pi --list-models` and `pi auth check` print signed in and signed out, and which of them the sign-in
  reading is built on.
- [ ] A whole turn through a real model: the reports in order, a tool call to Octoboard, a report to a parent.

Then build:

- [ ] The launch plan and the extension, with tests of the arguments for a new session, a resume, a pinned account, a
  model and an effort, and of the files written to the scratch directory.
- [ ] The hook interpreter's one answer and the screen reader's measured answers, with captures as test data.
- [ ] The sign-in reading and the model catalogue, with fixtures of the CLI's output.
- [ ] The descriptor's remaining facts and the presentation module's copy in every language.
- [ ] Pi's section of `docs/agent-cli-reference.md`, with the version each fact was measured on.

## Later, not in this milestone

Pi's own events as its status integration (the turn's last message, the prompt it was given), its records (history,
context size, removal, a missing conversation), carrying a conversation between accounts, delivery of messages through
the extension, and anything on Windows beyond the cross-check.

## Notes for the developer

**Reusable capabilities**

- The program status path: every session terminal answers the support query and turns the root record into the
  status of an agent with no status integration. Nothing in it is Pi's to write.
- The command every adapter registers as the session's MCP server, and the limit an agent's own tool timeout is set
  above.
- The per-session hook script and the session's scratch directory, removed with the session's process.
- The trust mechanism, which presses a provider's own confirmation from the screen description the provider gives.
- Running an agent's CLI as one of its accounts for a short query, for the model list and the sign-in reading.

**Development notes**

- Resume re-injects everything: the extension and the role file go with every launch.
- A default is the answer of a provider with none of the thing; override only what was measured.
- The extension is the provider's own code in another language. It lives in Pi's adapter directory, names nothing of
  another agent, and is covered by whatever lint and format rules apply to JavaScript files there.
- If something needs a change outside Pi's files, stop and report it: it is a gap in the generic layer.

**Reference docs**

- `docs/conventions/provider-neutral-core.md`
- `docs/conventions/daemon-processes-and-io.md`
- `docs/conventions/error-codes.md`, "Provider codes"
- `docs/conventions/i18n-in-code.md`
- `docs/product/program-status-reports.md`
- `docs/product/launching-agents.md`, "What is injected on every launch"
- `docs/product/folder-trust.md`, "How a confirmation is pressed"
- `docs/product/message-delivery.md`, "Messages held until a session can take them"
- `docs/agent-cli-reference.md`, "Injecting Octoboard into each agent"
