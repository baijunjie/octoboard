# Agent CLI reference

Per-agent facts about the three agent CLIs Octoboard launches: the hook events, what their payloads carry, what the
payloads can and cannot be correlated on, what a failing hook costs, how a project's own configuration layers around an
injected one, the mechanism and conditions for injecting Octoboard into each, and what it takes to move a session's
conversation into another config directory.

Established against **Claude Code 2.1.274**, **Codex 0.160.0** and **Grok Build 1.0.46**; what the Claude Code section
says about a declined prompt was measured on **2.1.286** as well, and held identically on both. What "Moving a
conversation to another config directory" says was measured on **Claude Code 2.1.289**, **Codex 0.160.0** and **Grok
Build 1.0.46**. What "Folder-trust confirmations" says about Codex and Grok Build was measured on **Codex 0.161.0** and
**Grok Build 1.0.50**. What "A long MCP tool call is moved to the background" and "A pasted prompt reaches
`UserPromptSubmit` wrapped" say was observed on **Claude Code 2.1.295**. All three rewrite their hook surface, their
payload fields, their configuration layering and their on-disk layout on upgrade, so check a detail here against the
installed version before relying on it.

How Octoboard's status mapping handles these traps is encoded in `apps/daemon/src/hooks.rs`; the product behavior
built on them is in the product docs.

## Claude Code

33 hook events exist, payload as JSON on stdin.

**Every payload** carries `session_id`, `transcript_path`, `cwd` and `hook_event_name`. Turn-scoped events add
`prompt_id`, which is **stable for the whole turn, including a re-run of the `Stop` hook** — it is the per-turn key to
hold state against.

**Events whose payload a mapping has to know:**

- `Stop` — `last_assistant_message` holds the turn's final assistant text, so the transcript never has to be parsed for
  it. `background_tasks` lists work still running, and a `Stop` with a non-empty list means paused, not finished. An
  entry's id is the same value as the `backgroundTaskId` in the matching `PostToolUse`'s `tool_response`, so a
  background task can be joined to the call that started it without reading the transcript.
- `StopFailure` — mutually exclusive with `Stop`: a turn that ends in an API error fires only `StopFailure`, so a
  mapping keyed on `Stop` alone leaves that session looking busy forever. It does not mean the session died; in the
  interactive TUI it stays alive at the prompt. It carries `error`, an optional `error_details` (the raw HTTP status
  plus the verbatim response body, not prose) and `last_assistant_message`. On this event `last_assistant_message` is
  the **user-facing error text, not model output**, so anything that reads that field as the turn's result has to treat
  `StopFailure` differently from `Stop`. `error` is not a function of the HTTP status: a plain 400 gives `unknown`, a
  400 whose body says the prompt is too long gives `invalid_request`, and a 529 gives `server_error` rather than
  `overloaded`.
- `PermissionRequest` — carries **no `tool_use_id`**, even though the binary's own embedded documentation lists one.
  The call the prompt belongs to has to be taken from the immediately preceding `PreToolUse`, which does carry it.
- `PostToolBatch` — when the call it covers was rejected by Claude Code's own pre-execution guard, the rejection is
  inside `tool_calls[0].tool_response`; that is the only place it is reported. Such a call fires only `PostToolBatch`,
  skipping `PreToolUse`, `PostToolUse` and `PostToolUseFailure`, so a mapping that pairs pre with post misses it.
- Nothing announces that the user answered a prompt or a question. An **approved** call's resolution is observable as
  the `PostToolUse` or `PostToolUseFailure` for it, so per-call state has to be keyed on
  `(session_id, prompt_id, tool_use_id)`. A **declined** one is not observable at all — see "A declined prompt" below.
- `Notification` carries its state in `notification_type`. Two were seen firing: `permission_prompt`, a fixed 6.0 s
  after the `PermissionRequest` it belongs to, and `idle_prompt`, exactly 60.0 s after a `Stop`. The idle timer is
  **armed by `Stop`**, so a turn that never fires one never produces an `idle_prompt` either, and `idle_prompt` is no
  use as a general backstop.

**A pending permission prompt** is `PermissionRequest`; a question put through the agent's own ask-the-user tool is also
reported. A question asked as plain prose is not: the turn simply ends, and no payload field tells it from a finished
one.

**A declined prompt emits no hook event whatsoever** — not for a permission prompt answered **No**, and not for the
agent's own `AskUserQuestion` cancelled with Esc. Measured on both 2.1.274 and 2.1.286, each run carrying a positive
control: no `Stop`, `StopFailure`, `PostToolUse`, `PostToolUseFailure`, `PostToolBatch`, `Notification` of any type, or
`SessionEnd`, through 211.7 s / 139.5 s (permission) and 93 s / 37 s (question) of waiting. Answering **No** from the
menu and pressing **Esc** are indistinguishable; the TUI prints `Interrupted · What should Claude do instead?` and sits
at an empty prompt. The only thing that ever breaks the silence is the user's next `UserPromptSubmit`.

A decline is recorded as a **user interrupt** rather than as its own kind of event, so its only machine-readable trace is
the pair of transcript records described under "A user interrupt emits no hook event" below — which is what makes the
transcript the only way to observe one.

Two traps around this, both measured:

- `PermissionDenied` **exists** in the executable's strings (alongside `emitPermissionDenied`,
  `executePermissionDeniedHooks`, and user-facing text about the hook permitting a retry) and **never fires** — not on a
  declined prompt, and not when a `PreToolUse` hook itself returns a `deny` decision.
- `--settings` **does not validate hook names at all**: registering `PermissionDenied`, or a wholly invented name,
  starts without a word of complaint, while the same file's *permission rules* are checked loudly. So a hook name being
  accepted is no evidence the event exists, let alone that it fires.

**`AskUserQuestion` has no plain decline option.** Its list offers the answers, then `Type something.` and
`Chat about this`; Esc cancels. Esc writes the same two records as a declined permission prompt, field for field.
`Chat about this` does not: its `tool_result` opens with the same "The user doesn't want to proceed with this tool use"
sentence but continues into the user's feedback, there is **no** `[Request interrupted by user for tool use]` record, and
the turn carries on — `PostToolBatch` and `Stop` both fire. So the two exits are told apart by the second record, not by
the first.

**A user interrupt emits no hook event**, in the thinking phase or mid-tool. Only the interrupted turn is silent; the
next turn reports normally. The session's next `UserPromptSubmit` is the reliable sign that the previous turn is over
and should close the dangling `PreToolUse`; until then the session reads as working, which is stale rather than wrong.
The interrupted tool's own child process is already gone. The only machine-readable trace is in the transcript at
`transcript_path`: a `tool_result` with `is_error: true` beginning "The user doesn't want to proceed with this tool
use", followed by a user block `[Request interrupted by user for tool use]`. Both are entries whose `message.content` is
an **array** of blocks, and the pair also carries `"toolDenialKind": "user-rejected"`. A declined prompt and a declined
`AskUserQuestion` write this same pair, which is why one mechanism recovers all three cases.

**A failing hook**: a non-zero exit renders an error block in the TUI carrying the hook's stderr, while hook stdout is
never shown. `exit 2` *blocks*, event-specifically, and feeds stderr to the model. A hook entry with no `timeout`
field blocks the turn for its full duration; the implicit default is over 30 s.

**Project configuration layering**: hooks injected through `--settings` merge with the project's
`.claude/settings.json` hooks rather than replacing them, and injected `permissions` merge with the project's.
Precedence among permission rules is by rule *kind* rather than by source — `deny` > `ask` > `allow` — so an injected
`allow` can never re-open something the project denies.

**`--mcp-config` takes a list of values, not one.** Every non-flag argument after it is read as a
further config path, resolved against the cwd: `claude --mcp-config '<json>' mcp list` fails with
`MCP config file not found: <cwd>/mcp`. Anything positional — the task prompt in particular — has
to come before it rather than after.

**A long MCP tool call is moved to the background.** Claude Code moves an MCP tool call still running after 120 seconds
to the background (it reports that the MCP tool is still running after 120s and was moved to the background as a task).
The turn ends and the session goes idle while the call is pending, and the call's result arrives later as a task
notification. Observed on 2.1.295, where the pieces a hook sees are:

- The `Stop` that ends the turn lists the call in `background_tasks` as an MCP entry: `id` (the task id), `type`
  `"MCP task"`, `status` `"running"`, `description` (`<server>/<tool>`), `server` and `tool` (the unqualified tool
  name).
- The notification is delivered as a turn of its own, with its own `UserPromptSubmit`, whose `prompt` opens with
  `<task-notification>` and, on the next line, `<task-id>…</task-id>` carrying that same id; the call's result follows
  further down. Nothing orders this turn against a message written into the session meanwhile.

**A pasted prompt reaches `UserPromptSubmit` wrapped.** A long paste — every message Octoboard writes is one — is handed
over in `prompt` as `<pasted_content id="…">` on its own line, after a leading blank line, then the pasted text, then a
closing `</pasted_content id="…">`. Observed on 2.1.295.

**An appended system prompt is recorded once per conversation** and replayed verbatim on every resume, because
`--system-prompt-snapshot` defaults to `on`; different text passed on a later launch is silently ignored. It must still
be passed on every launch, because after a compaction the snapshot is re-rendered from whatever *that* launch passed.
Passing `--system-prompt-snapshot off` inverts this, at the price of the role vanishing from any launch that omits it
and of prompt-cache replay.

## Codex

12 hook events, payload as JSON on stdin. The binary embeds the **full JSON Schema of every hook payload**, with
`additionalProperties: false` and an explicit required list; for a given build that is the authoritative field
reference.

**Every payload** carries `session_id`, `transcript_path`, `cwd`, `hook_event_name`, `model` and `permission_mode`.
Turn-scoped events add `turn_id`, the per-turn key. The compaction events are the exception: they carry no
`permission_mode`.

**Events whose payload a mapping has to know:**

- `SessionStart` — `source` is one of `startup`, `resume`, `clear`, `compact`, `fork`. Its `session_id` is the only
  place Codex ever publishes its own id, and the thread id is the same value. `~/.codex/session_index.jsonl` is not an
  alternative source: it records only *named* threads, and is written late.
- `Stop` — `turn_id`, `last_assistant_message`, `stop_hook_active`.
- `Interrupt` — fires on Esc or Ctrl+C during an in-flight turn (Ctrl+C confirmed by hand). It carries the base
  payload plus the cancelled turn's own `turn_id` and is mutually exclusive with `Stop`. Esc pressed while the session
  is idle fires nothing at all, and `Interrupt` never fires in `codex exec`, so this branch cannot be exercised
  headlessly. A *declined* approval aborts the turn and fires `Interrupt` too, so cancel and decline are told apart by
  whether a `PermissionRequest` went unresolved just before it.
- `PermissionRequest` — observed firing *before* the dialog reached the user.
- `SessionEnd` — `reason` is schema-pinned to the constant `"other"` and therefore carries no information. `async` is
  ignored for this event: Codex forces it synchronous, so its 3 s clamp is wall-clock cost on every teardown.
- `PreCompact` / `PostCompact` — fire on both a manual and an automatic compaction. A manual one gets a fresh
  `turn_id`; an automatic one fires **after** `Stop` and **reuses the finished turn's `turn_id`**, so the same
  `turn_id` can appear on `Stop` and then again on a compaction event, which turn-keyed state must not read as the turn
  reopening. `PostCompact` carries no summary and no token counts.
- **There is no question or notification event.** The two features that would produce a structured question to the
  user, `default_mode_request_user_input` and `request_permissions_tool`, are off in this build.

**The user's own configuration can remove the prompt entirely.** With `approvals_reviewer = "auto_review"` Codex
resolves an approval request itself: the hook fires, no modal is shown and the tool proceeds. A raised hand would ask
the user to answer something they never see.

**A Codex older than the *async* hook support Octoboard injects** (0.142.5 was seen, resolved through the login-shell
`PATH`; inferred from the warning, since the session ended on an API error before status could be observed) prints
"skipping async hook ... async hooks are not supported yet", so hook-driven status cannot work with it.

**A failing hook** renders a visible history cell — `Hook failed` with the exit code, or `hook timed out after Ns`;
the hook's own stderr text is not shown — and a *synchronous* hook that hangs costs the turn its full timeout. With
`async = true` a hook that exits non-zero and writes stderr produces no cell and no warning whatsoever.

**Project configuration layering**: project hooks in `<cwd>/.codex/hooks.json` merge with injected ones;
`<cwd>/AGENTS.md` is concatenated *after* `~/.codex/AGENTS.md` inside a single instructions block, separated by a
`--- project-doc ---` line, so the user's global file survives; skills are discovered in both `<cwd>/.agents/skills/`
and `<cwd>/.codex/skills/`. `<cwd>/.codex/config.toml` is **not read at all**, which is why Codex has no project-level
permission or approval configuration for an injected one to collide with.

**An update prompt can come first**: when a newer version is out, Codex can open with an "Update now / Skip / Skip
until next version" prompt (seen as an "Update available!" banner in the probe on 0.161.0). Octoboard passes
`-c check_for_update_on_startup=false` on every launch, the setting Codex's own config defines for whether it checks
for updates at startup and shows that prompt, so nothing sits in front of the folder-trust confirmation; it applies to
that launch only. Measured on 0.160.0, with an update made available by raising `latest_version` in Codex's cached
`version.json`: a plain launch opened with the update prompt, and a launch with the override went straight to the
folder-trust confirmation and showed no update prompt after it either.

**Two visible side effects of passing any `-c` override**: it forces embedded mode, so the TUI carries a permanent
`⚠ 1 warning` badge explaining that command-line overrides require it, and the session is invisible to `codex agents`.

## Grok Build

16 hook events, payload as JSON on stdin, with **every field present in both camelCase and snake_case**. The hook
process additionally receives `GROK_HOOK_EVENT`, `GROK_HOOK_NAME` — whose `global/`, `project/` or `managed:` prefix is
usable provenance — `GROK_SESSION_ID` and `GROK_WORKSPACE_ROOT`.

**The per-turn `Stop` carries `promptId`, `lastAssistantMessage`, `backgroundTasks` and `sessionCrons`**, and the
teardown fire carries none of them, so the two are distinguishable by payload shape as well as by `reason`.

**Events whose payload a mapping has to know:**

- `Stop` — fires **twice** per session: once per turn with `reason: "end_turn"` and again at teardown with
  `reason: "shutdown"`, so filter on the reason or every session ends with a phantom report. `SIGTERM` is what produces
  the graceful teardown, so it is how a Grok session should be stopped.
- `SessionStart` — `source` on a resume is `"load"`, not `"resume"`.
- `PreToolUse` — fires *before* the permission check, so a call that is then denied leaves an open `PreToolUse` with
  neither `PostToolUse` nor `PostToolUseFailure` after it.
- `PostToolUse` / `PostToolUseFailure` — mutually exclusive. For an MCP call routed through the dispatcher, `toolInput`
  is the *wrapper* (`{"tool_name": …, "tool_input": {…}}`) with the real arguments nested one level inside it, while
  the top-level `toolName` is already the qualified `<server>__<tool>` name, which is what to match on.
- `PermissionDenied` — carries only `toolName`, `toolUseId`, `toolInput` and `toolInputTruncated`: no reason, no rule
  and no source, so on its own it cannot say whether a rule or the user stopped the call.
- `StopCancelled` — its `reason` / `reasonDetails` is the only place a user declining a prompt can be told apart from a
  policy denial.
- `Notification` — `permission_prompt` is a pending permission prompt (its ordering against the dialog was not
  established); `idle_prompt` is the turn-end backstop, below. A question through Grok's own ask-the-user tool is
  reported.
- `StopFailure` — observed live on Grok Build 1.0.46, with the provider answering HTTP 400: `hookEventName`
  `stop_failure` (`hook_event_name` `StopFailure`), `error` `"invalid_request"`, `errorDetails` of the form `API error
  (status 400 Bad Request): invalid_request_error: ...`, `lastAssistantMessage` of the form `Turn failed: API error
  (status 400 Bad Request): ...`, and `permissionMode`. An HTTP 500 does not produce it: Grok retried for more than
  seven minutes, past its `max_retries` of 8, and never ended the turn. It was provoked by setting the key
  `endpoints.models_base_url` (Grok Build 1.0.46) in a throw-away `GROK_HOME`, with the user's `auth.json` linked in,
  to a local stand-in answering HTTP 400.

**Some Grok turns produce no stop event at all** (bash mode, builtin slash commands, cancel-and-send, rewinds). The
backstop is `Notification` with `idle_prompt`: a turn cancelled before its first token emitted no stop event of any
kind, and `idle_prompt` was the only turn-end signal. It fires about 60 s after the turn ends, carries **no turn id**
(so it can only be attributed by session and clock) and needs at least one turn to have ended; a session that only ran a
slash command never emits it. **Bash mode (`!`)** fires no tool or turn hook events and bypasses permissions (a deny
rule does not stop it); in one run an `idle_prompt` backstop followed about 60 s after the command.

A single observation run, so inconclusive: after a `Stop` the backstop arrived 60 s later when the session stayed idle,
but for an ending followed by a new prompt within the same second the backstop never arrived in the capture. The guard
against a stale backstop closing a still-running quiet turn was **not proven**: the row stayed `working` through a 180 s
silent tool call with no spurious flip, but the backstop that would have tested it never came.

**A failing hook** costs one scrollback line carrying the first line of its stderr, after which the turn proceeds —
and those lines appear **only in the TUI**, never in headless `-p`, so hook behavior has to be checked in the TUI. On
`PreToolUse` a `deny` in the hook's stdout JSON is honoured **regardless of its exit code**, so a hook meant only to
observe must emit no JSON at all.

**Project configuration layering**: hooks are additive across the global, project and managed layers, and
byte-identical handlers are deduplicated across them.

## Across the three agents

**Injected MCP tool names** differ in shape, which matters wherever a role description has to name a tool: a server
registered under the key `k` exposes its tool `t` as `mcp__k__t` on Claude Code and Codex, and as `k__t` on Grok.

**All three connect their MCP servers when the process starts**, before any turn — a `command`-type server's child is
already running by the time the TUI is drawn. So whether an injected server was accepted is observable without a prompt
ever being submitted.

**Subagent events**, none of which Octoboard registers, carry join traps that make them unusable as if they were the
session's own:

- Claude Code — `SubagentStop` also fires for internal helper agents, which are the ones with an empty `agent_type`.
- Codex — `transcript_path` is the *subagent's* file on `SubagentStart` but the *parent's* on `SubagentStop`, where the
  subagent's own is in `agent_transcript_path`; `session_id` is the parent's while `turn_id` is the subagent's own, so
  the pair can be joined to the parent only by `agent_id`; and the subagent thread emits no `SessionStart` or `Stop` of
  its own.
- Grok — `SubagentStart` fires in the parent and `SubagentStop` in the child, where `sessionId` equals `subagentId`;
  the resolved `subagentType` is not the one the caller asked for, so a matcher written against a requested name never
  matches; and a parent's `Stop` is no promise that its subagents have finished.

## Moving a conversation to another config directory

All three agents resume a conversation under a second config directory when **the session's own record alone** is
copied into it. No index, registry or per-project entry has to travel with it, although all three keep one. Each result
carried a control in the same run.

| Agent | What to copy | Where it goes |
|---|---|---|
| Claude Code | `<session id>.jsonl` | `<config dir>/projects/<cwd slug>/` |
| Codex | `rollout-<timestamp>-<thread id>.jsonl` | anywhere under `<home>/sessions/` |
| Grok Build | the `<session id>/` directory, whole | `<home>/sessions/<url-encoded cwd>/` |

**The record's path relative to the config directory is what matters**, and the source directory already holds it, so a
copy goes from `<source>/<relative path>` to `<target>/<same relative path>` with the intermediate directories created.
Neither Claude Code's slug rule nor Grok's encoding of a working directory has to be implemented, and neither was
measured. For Grok Build the config directory is the **account's** directory, the source home a per-session home is
built from, never the per-session home itself, which is discarded with the process.

- **Claude Code** — the record copied alone into a project-slug directory that had never held it resumed with the
  conversation intact. The global config file's `projects` map was empty in the target and nothing was needed there, so
  `.claude.json` is not part of a resume's lookup. Session resolution comes **before** the login check: an
  unauthenticated directory asked to resume a session it does not hold answers `No conversation found with session ID:
  <id>`, and the same directory with the record copied in answers `Not logged in · Please run /login`.
- **Codex** — a record copied into a fresh home at a date path unrelated to its own (`sessions/2099/01/01/`) resumed
  with the conversation intact: the date directories are how Codex writes, not how it looks up, and
  `session_index.jsonl` is not consulted. Control: a thread id with no record in the home answers `thread/resume:
  thread/resume failed: no rollout found for thread id <id>`. Resolution precedes the login check here too.
- **Grok Build** — the session directory copied under a different working directory's encoded name resumed with the
  conversation intact; `sessions/session_search.sqlite` is not consulted. **An unauthenticated home does not refuse**:
  measured with Grok Build 1.0.46, `grok --resume <id>` against a home with no login stays up on a browser device-code
  sign-in (`Approve in your browser to finish signing in`, then `Waiting for approval`) instead of exiting. So, as with
  Claude Code and Codex, a switch to an account that is not signed in comes up and counts as a success, and the
  relaunched session can start the agent's own sign-in flow. Whether the session is looked up before or after that
  sign-in was not measured, so a relocation cannot be verified without a login. One difference from the others: **a
  session missing locally is fetched from a remote registry** (`Session "<id>" not found locally, restoring
  conversation from remote...`, then a 404 in the control), so a switch between two homes of the same account might not
  need a copy at all; across two different accounts the registry is the other account's and will not serve the
  session.

**The session comes up under the target directory's whole setup.** A config directory holds the agent's global
configuration, not only its login: the same Codex thread resumed in a fresh home came up with that home's own defaults
(`reasoning effort: none` where the original had `medium`) because `config.toml` lives in the home. Claude Code's
`settings.json` and Grok's `config.toml` are in the same position.

**A second config directory is a second login.** Claude Code keeps its credential in the macOS Keychain under a service
name derived from the config directory, so two directories hold two independent logins: a fresh `CLAUDE_CONFIG_DIR`
reports `Not logged in` while the default directory is logged in, and copying the logged-in directory's own account
record into it changes nothing. Read out of the 2.1.289 executable and not measured: the service name appears to be
`Claude Code-credentials`, with `-<first 8 hex of sha256(config directory)>` appended when `CLAUDE_CONFIG_DIR` is set.
Codex and Grok Build keep the login in a file inside the directory (`auth.json` in both), which is inferred from the
layout, not measured. A fresh Grok home is signed in without a terminal by `GROK_HOME=<dir> grok login` (a browser
device code; plain `grok` needs a TTY and fails with `Device not configured` without one), and one turn of
`grok -p "<prompt>"` then creates its `sessions/`, which is what makes it an initialized Grok home (measured with Grok
Build 1.0.46).

Claude Code was measured to create a config directory that does not exist yet. Codex 0.161.0 does not: it exits with
`CODEX_HOME points to "<dir>", but that path does not exist`, so the daemon creates a pinned Codex directory itself
before launching.

**The trap a verification has to rule out** is an agent that resumes *successfully* into an empty conversation. None of
the three did that here, each refusing distinctly, but a future version could introduce it and it would look like a
successful switch.

## Injecting Octoboard into each agent

How each agent is given Octoboard's status hooks, its MCP server and a role description, with the condition each one
brings. Everything is per launch (arguments, an environment variable, or files Octoboard writes itself) and never an
edit of the project's or the user's files, and was exercised against the versions above. What a user sees of it is in
`docs/product/launching-agents.md`.

| Capability | Claude Code | Codex | Grok Build |
|---|---|---|---|
| Launch with an initial task | `claude "<task>"` | `codex "<task>"` | `grok "<task>"` |
| Pre-allocate a session id | `--session-id <uuid>` | **Not possible.** Take it from the `SessionStart` payload; the interactive thread is created lazily on the first prompt, so a session opened without a task has no id until the user types | `--session-id <uuid>` (new sessions only) |
| Inject hooks | `--settings <json-or-path>`, merged with the project settings | `-c 'hooks.<Event>=[{hooks=[{type="command",command=…,timeout=3,async=true}]}]'`, one per event, plus the hook-trust step | A per-session `GROK_HOME` whose `hooks/` directory is Octoboard's and whose other entries symlink to the source home |
| Inject MCP | `--mcp-config <json>` | `-c 'mcp_servers.octoboard.command=…'`, `.args=[…]` and `.default_tools_approval_mode="auto"` | An `[mcp_servers.octoboard]` block appended to the `config.toml` copy inside that `GROK_HOME` |
| Inject a role description | `--append-system-prompt` | `-c 'developer_instructions="…"'`, which adds a developer message and leaves the rest of the prompt byte-identical; not `-c instructions=`, which replaces the system prompt | `--rules "…"`, appended to the system prompt and persisted into the session record |
| Resume | `claude --resume <id>` | `codex resume <id>` | `grok --resume <id>` |
| Identity seen by the MCP child | `CLAUDE_CODE_SESSION_ID` is set | argv or an explicit `env` table; no `CODEX_*` variable reaches the child | `GROK_SESSION_ID` is set; `{{session_id}}` templating does *not* work |

- **Claude Code** injects cleanly through flags. Never pass `--setting-sources` (it silently drops the project's own
  permission rules and hooks) or `--strict-mcp-config` (it silently drops the project's and the user's MCP servers). The
  injected MCP server's key must not collide with one the project defines, or the project's is silently never spawned.
- **Grok Build** has no flag for hooks or MCP, and its `GROK_CONFIG` / `GROK_CONFIG_PATH` overlay accepts only
  allowlisted keys, so both silently drop. The working mechanism is a per-session `GROK_HOME` where every entry links
  back to the source home, except a copied `config.toml` carrying the MCP block, a copied `trusted_folders.toml` with
  nothing added to it, and an Octoboard `hooks/` directory. `auth.json` and `sessions/` stay links, so login is shared
  and sessions stay resumable from the user's own `grok`.
- **Codex** gates hooks behind a persisted trust hash: without it an interactive session raises a blocking review modal
  and a headless one **hangs indefinitely**. `--dangerously-bypass-hook-trust` clears that at the cost of two warning
  lines per launch. The hash is taken over the handler definition, which includes the hook command, and that command is
  the session's own script path, so hashes captured once could never match a later session. Removing the warning would
  mean a session-independent hook command. Project trust is not passed: a per-launch `projects` override is saved
  nowhere, so Codex's own confirmation is pressed instead (see "Folder-trust confirmations" below).
- **Workspace trust gates the project's own configuration on Claude Code and Grok**, and fails silently. On Grok an
  untrusted folder makes the project's `AGENTS.md`, hooks and MCP servers not load; trust lives inside `GROK_HOME` (an
  undocumented `--trust` flag also exists), and Grok also needs a recognized **git** root, since project hooks did not
  load in a trusted non-git directory. Octoboard makes no trust decision of its own in either agent's store and passes
  none for a launch; it presses the agent's own confirmation (see "Folder-trust confirmations" below). On Claude Code an
  untrusted workspace makes the project's `allow` rules ignored (with a line on stderr) while `deny` still applies, so
  the session is only ever more restrictive. Its trust lives in the global config file (`~/.claude.json`, or
  `.claude.json` inside the config directory in effect), which Octoboard reads and never writes. It is read rather than
  matching the stderr line because on a PTY stderr is the rendered UI, and matching rendered text would need a VT
  emulator in the daemon; it warns only on an explicit negative, so a renamed key does not warn on every launch.
- **Each agent reads a different instruction filename, matched by exact spelling.** Claude Code reads `CLAUDE.md` and
  `CLAUDE.local.md` and does **not** read `AGENTS.md`, and cannot be made to; where it must be shown a project's
  `AGENTS.md`, `--append-system-prompt-file` lands it in the system prompt. Codex reads `AGENTS.md`. Grok reads
  `AGENTS.md`, `Agents.md`, `CLAUDE.md`, `Claude.md` and `CLAUDE.local.md`, and on a case-sensitive volume loads every
  matching spelling present. No agent accepts an all-lowercase name. Grok locates a project by walking up for `.git`,
  and without one reads no project instructions and no project hooks, which is why a Grok console session's role cannot be an
  instruction file in a console's working directory and travels in `--rules` instead.
- **Resume re-injects everything.** Hooks and the MCP server are resolved from the launch arguments every time and are
  lost on a resume that omits them, silently. The role description differs: Grok persists `--rules` into the session
  record, while Claude Code records its appended system prompt on the first request and replays it verbatim, so a
  changed role text is ignored on resume. A session's role is immutable for its lifetime.
- **Hook scripts must fail silently and fast on all three.** Every agent surfaces a failing hook, and a hook with no
  timeout blocks the turn for its full duration. Octoboard's scripts exit 0 unconditionally, write nothing to stderr,
  set a short explicit timeout (Codex clamps some events to 3 s), are asynchronous where the agent allows it, and give
  the daemon call a hard deadline. Grok's HTTP hooks cannot reach the daemon at all, since its SSRF protection rejects
  plain HTTP and private addresses, so hooks are `command` type.
- Injection never modifies project files or the user's global configuration; where an agent can only be configured
  through a file, a "use this config directory" variable or flag is preferred.

### Folder-trust confirmations

Each agent asks once per folder it has not been told to trust, on a confirmation of its own, before any hook runs.
Octoboard recognizes it in the terminal output and presses it; what pressing it records is the agent's own business.

- **Claude Code** shows its workspace-trust screen the first time it runs in a directory, with the cursor on "No,
  exit" and "Yes, I trust this folder" below it; a Down and then an Enter accept. It records the trust in its global
  config file (above), and runs its first hook only after the screen is answered.
- **Codex** shows its folder-trust confirmation only when the working directory is inside a git repository; outside
  one it never asks. The screen reads "Trust this folder? Codex can read, edit, and run files here, …", then "› 1.
  Trust and continue", "2. Quit" and "enter continue · esc quit". The cursor (`›`, U+203A) starts on option 1, and
  Enter alone accepts; any carriage return reaching the screen accepts. Codex records `[projects."<repository root>"]
  trust_level = "trusted"` in `$CODEX_HOME/config.toml`: from a subdirectory it records the repository root, and from
  a linked worktree the main repository's root, so the record can cover more than the folder asked about. A trusted
  parent directory in that file does not cover a repository below it. No hook fires before or right after the
  confirmation, so only a bound on time and output can end a watch for it. Codex redraws only the cells that changed,
  so text read off the screen can lose characters wherever the frame before held the same one; the confirmation's own
  phrases came through intact in its first drawing, a path above it did not. The review Codex raises for new hooks
  without `--dangerously-bypass-hook-trust` reads "Hooks need review … 2. Trust all and continue" and is a different
  modal. On 0.161.0 a key the confirmation ignores (a letter, Right, Tab, `?`) draws nothing but a bare
  synchronized-update frame (`ESC[?2026h ESC[39m ESC[49m ESC[0m ESC[?2026l`); accepting it clears the screen and
  redraws it whole, starting with the ">_ OpenAI Codex (v…)" banner, which is how an answer given in the terminal is
  told apart.
- **Grok Build** shows its confirmation only when the folder holds content its trust gates, such as an `AGENTS.md`,
  whether or not it is a git repository, so a console directory holding an instruction file is asked about too. The
  screen reads "Do you trust the contents of this directory?", "Grok Build may run or modify contents in this
  directory, posing security risks.", "Yes, proceed y" and "No, quit n", with no cursor; `y` or Enter accepts, and a
  decline exits. Grok writes `[folders."<canonical cwd>"]` with `trusted = true` and `decided_at` into
  `$GROK_HOME/trusted_folders.toml`, saving by replacing the file, so against Octoboard's per-session `GROK_HOME` the
  entry lands only in the session's copy and a symlink there is replaced rather than followed; no setting moves the
  store. Octoboard therefore carries that one entry, unchanged, into the user's own store, after its own press and after
  the person answers in the terminal alike, holding the `trusted_folders.toml.lock` Grok keeps beside it. Grok locks
  that same file before it replaces the store (measured on 1.0.50: with an exclusive `flock` held on it elsewhere, an
  accepted confirmation wrote nothing until it was released), and Octoboard's lock is exclusive, so the two writes
  exclude each other. `SessionStart` fires right after the press. While it waits, Grok animates its logo at about
  2 KB/s, so the first drawing soon leaves any window of recent output and the terminal is never quiet.

### Writing into a running session

The sequence for all three agents is `ESC[200~`, the text with LF separators, `ESC[201~`, then `CR`; multi-line text
arrives as one message with no delay between paste and Enter. Every agent queues a message written mid-turn and consumes
it when the turn ends. (Codex additionally has `codex queue --thread <session id>`.) Four rules, each failing silently
or destructively if ignored:

- A message starting with `/` runs as a slash command even inside a bracketed paste; prefix a single space.
- A macOS PTY master accepts only about 1022 bytes before `EAGAIN` when the child is not draining, so the write is a
  non-blocking partial-write-and-retry loop in slices, never a blocking `write_all` on the daemon's event loop.
- Nothing may be written while a modal dialog is up. The paste is discarded but the trailing `CR` confirms whatever is
  highlighted: at Claude Code's trust dialog that was seen to exit the session for a short message (an 8.8 KB paste
  left it untouched, unexplained), at Grok's approval modal it would select "always-approve", and at Codex's or Grok's
  folder-trust confirmation it accepts, trusting the folder. Only Codex's approval modal was seen to take a paste
  without effect. Never send bare keys either, since Grok and Codex treat digits as confirm hotkeys. The one
  deliberate exception is the daemon's press of an agent's own folder-trust confirmation: Down and Enter for Claude
  Code, Enter for Codex, `y` for Grok, and nothing else.
- The gate comes from hook-reported state, never from the terminal, because none of the agents signal modal state
  through terminal modes. If the hook state is missing or stale, do not write. The folder-trust confirmations are the
  exception: no hook runs before one is answered, so it can only be read from the output, and nothing is written into
  a session from its launch until its first hook has arrived; for Codex, which runs none until its first prompt, also
  until the confirmation has been pressed, the person's answer in the terminal is seen to replace it, or the watch for
  it has ended with none sighted.
