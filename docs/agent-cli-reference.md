# Agent CLI reference

Per-agent facts about the three agent CLIs Octoboard launches: the hook events, what their payloads carry, what the
payloads can and cannot be correlated on, what a failing hook costs, and how a project's own configuration layers
around an injected one.

Established against **Claude Code 2.1.274**, **Codex 0.160.0** and **Grok Build 1.0.46**. All three rewrite their hook
surface, their payload fields and their configuration layering on upgrade, so check a detail here against the installed
version before relying on it.

What is deliberately not here: the mechanism each adapter uses to inject hooks, MCP and a role description, and the
conditions it must satisfy, which are in `docs/mvp.md` section 6; the traps Octoboard's own status mapping already
encodes, which are in `daemon/src/hooks.rs`; and the conditions those traps place on reporting and the raised hand,
which are in `docs/mvp.md` sections 5.3 and 5.5.

## Claude Code

33 hook events exist, payload as JSON on stdin.

**Every payload** carries `session_id`, `transcript_path`, `cwd` and `hook_event_name`. Turn-scoped events add
`prompt_id`, which is **stable for the whole turn, including a re-run of the `Stop` hook** — it is the per-turn key to
hold state against.

**Events whose payload a mapping has to know:**

- `Stop` — `last_assistant_message` holds the turn's final assistant text, so the transcript never has to be parsed for
  it. `background_tasks` lists work still running. An entry's id is the same value as the `backgroundTaskId` in the
  matching `PostToolUse`'s `tool_response`, so a background task can be joined to the call that started it without
  reading the transcript.
- `StopFailure` — carries `error`, an optional `error_details` (the raw HTTP status plus the verbatim response body,
  not prose) and `last_assistant_message`. On this event `last_assistant_message` is the **user-facing error text, not
  model output**, so anything that reads that field as the turn's result has to treat `StopFailure` differently from
  `Stop`. `error` is not a function of the HTTP status: a plain 400 gives `unknown`, a 400 whose body says the prompt is
  too long gives `invalid_request`, and a 529 gives `server_error` rather than `overloaded`.
- `PermissionRequest` — carries **no `tool_use_id`**, even though the binary's own embedded documentation lists one.
  The call the prompt belongs to has to be taken from the immediately preceding `PreToolUse`, which does carry it.
- `PostToolBatch` — when the call it covers was rejected by Claude Code's own pre-execution guard, the rejection is
  inside `tool_calls[0].tool_response`; that is the only place it is reported.
- Nothing announces that the user answered a prompt or a question. What is observable is the resolution of the pending
  call, as the `PostToolUse` or `PostToolUseFailure` for it, so per-call state has to be keyed on
  `(session_id, prompt_id, tool_use_id)`.

**A user interrupt leaves its only machine-readable trace in the transcript** at `transcript_path`: a `tool_result`
with `is_error: true` beginning "The user doesn't want to proceed with this tool use", followed by a user block
`[Request interrupted by user for tool use]`.

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
- `Interrupt` — the base payload plus the cancelled turn's own `turn_id`. Esc pressed while the session is idle fires
  nothing at all, and `Interrupt` never fires in `codex exec`, so this branch cannot be exercised headlessly.
- `SessionEnd` — `reason` is schema-pinned to the constant `"other"` and therefore carries no information. `async` is
  ignored for this event: Codex forces it synchronous, so its 3 s clamp is wall-clock cost on every teardown.
- `PreCompact` / `PostCompact` — fire on both a manual and an automatic compaction. A manual one gets a fresh
  `turn_id`; an automatic one fires **after** `Stop` and **reuses the finished turn's `turn_id`**, so the same
  `turn_id` can appear on `Stop` and then again on a compaction event, which turn-keyed state must not read as the turn
  reopening. `PostCompact` carries no summary and no token counts.
- **There is no question or notification event.** The two features that would produce a structured question to the
  user, `default_mode_request_user_input` and `request_permissions_tool`, are off in this build.

**A failing hook** renders a visible history cell — `Hook failed` with the exit code, or `hook timed out after Ns`;
the hook's own stderr text is not shown — and a *synchronous* hook that hangs costs the turn its full timeout. With
`async = true` a hook that exits non-zero and writes stderr produces no cell and no warning whatsoever.

**Project configuration layering**: project hooks in `<cwd>/.codex/hooks.json` merge with injected ones;
`<cwd>/AGENTS.md` is concatenated *after* `~/.codex/AGENTS.md` inside a single instructions block, separated by a
`--- project-doc ---` line, so the user's global file survives; skills are discovered in both `<cwd>/.agents/skills/`
and `<cwd>/.codex/skills/`. `<cwd>/.codex/config.toml` is **not read at all**, which is why Codex has no project-level
permission or approval configuration for an injected one to collide with.

**Two visible side effects of passing any `-c` override**: it forces embedded mode, so the TUI carries a permanent
`⚠ 1 warning` badge explaining that command-line overrides require it, and the session is invisible to `codex agents`.

## Grok Build

16 hook events, payload as JSON on stdin, with **every field present in both camelCase and snake_case**. The hook
process additionally receives `GROK_HOOK_EVENT`, `GROK_HOOK_NAME` — whose `global/`, `project/` or `managed:` prefix is
usable provenance — `GROK_SESSION_ID` and `GROK_WORKSPACE_ROOT`.

**The per-turn `Stop` carries `promptId`, `lastAssistantMessage`, `backgroundTasks` and `sessionCrons`**, and the
teardown fire carries none of them, so the two are distinguishable by payload shape as well as by `reason`.

**Events whose payload a mapping has to know:**

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
- `StopFailure` — the payload shape is taken from Grok's shipped documentation and has never been observed from a live
  session, so the fields are the one part of this reference that rests on the vendor's own description.

**A failing hook** costs one scrollback line carrying the first line of its stderr, after which the turn proceeds —
and those lines appear **only in the TUI**, never in headless `-p`, so hook behaviour has to be checked in the TUI. On
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
