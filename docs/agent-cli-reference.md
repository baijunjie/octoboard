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
`UserPromptSubmit` wrapped" say was observed on **Claude Code 2.1.295**. Each agent's "Model and reasoning effort" note
was measured on **Claude Code 2.1.296**, **Codex 0.160.0** and **Grok Build 1.0.50**; the Codex facts were not
re-checked on **0.161.0**, which the folder-trust facts above were measured on. What the Grok Build section's "What the
model itself is handed, and what it calls" says was re-confirmed on **Grok Build 1.0.52** with a real account, as was
the timeout paragraph's `tool_timeout_sec = 660`; "The id a turn records is not one of the ids `grok models` prints"
was first measured on that build.
All three rewrite their hook surface, their payload fields, their configuration layering and their on-disk layout on
upgrade, so check a detail here against the installed version before relying on it.

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

**A configuration that names no permission mode does not ask.** On 2.1.296 a config directory whose `settings.json`
carries no `permissions` block runs in *auto mode* (status line `⏵⏵ auto mode on (shift+tab to cycle)`), and a `Write`
creating a new file in the session's own working directory went through with no prompt and no `PermissionRequest`. The
mode that prompts has to be asked for: `permissions.defaultMode: "default"` in that directory's `settings.json`, since
a launch passes no `--permission-mode`. The running session then calls that mode `⏸ manual mode on`, which matches
neither the settings value nor the word "normal" — read the mode off the session's own transcript instead, where it is
recorded unambiguously as `{"type":"permission-mode","permissionMode":"default"}` and `{"type":"mode","mode":"normal"}`.

**A pending permission prompt** is `PermissionRequest`; a question put through the agent's own ask-the-user tool is also
reported. A question asked as plain prose is not: the turn simply ends, and no payload field tells it from a finished
one.

**A declined prompt emits no hook event whatsoever** — not for a permission prompt answered **No**, and not for the
agent's own `AskUserQuestion` cancelled with Esc. Measured on both 2.1.274 and 2.1.286, each run carrying a positive
control: no `Stop`, `StopFailure`, `PostToolUse`, `PostToolUseFailure`, `PostToolBatch`, `Notification` of any type, or
`SessionEnd`, through 211.7 s / 139.5 s (permission) and 93 s / 37 s (question) of waiting. On those versions answering
**No** from the menu and pressing **Esc** are indistinguishable; the TUI prints `Interrupted · What should Claude do
instead?` and sits at an empty prompt. The only thing that ever breaks the silence is the user's next
`UserPromptSubmit`.

**The prompt and what follows it have moved since.** On 2.1.296 a `Write` permission prompt offers `1. Yes`,
`2. Yes, and switch to accept edits (auto-approve file edits and common file commands) for this session (shift+tab)`
and `3. No`, with a footer `Esc to cancel · Tab to amend` — there is no "No, and tell Claude what to do differently"
option and no feedback field, and the highlighted default is `1. Yes`, so Enter approves. A decline prints no
`Interrupted` line: under the tool call the TUI writes `⎿ User rejected write to <file>` and the rejected body, then
the turn's own summary line (`✻ Brewed for 2s · done <time>`), then sits at an empty prompt. Only `Esc` was pressed on
2.1.296; `3. No` was not tried there, so the two being indistinguishable stands for 2.1.274 / 2.1.286 alone.

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
an **array** of blocks, and the first of them — the one holding the `tool_result` — also carries
`"toolDenialKind": "user-rejected"`, at entry level rather than inside the block; the
`[Request interrupted by user for tool use]` entry carries none. A declined prompt and a declined `AskUserQuestion`
write this same pair, which is why one mechanism recovers all three cases.

**Both marker strings survive 2.1.296**, re-measured byte for byte on a declined `Write` prompt: the `tool_result` has
`is_error: true` with `content` a **plain string** beginning "The user doesn't want to proceed with this tool use.",
and the entry after it is a `text` block whose text equals `[Request interrupted by user for tool use]` exactly. The
rejection text continues past that opening sentence ("… The tool use was rejected (eg. if it was a file edit, the
new_string was NOT written to the file). STOP what you are doing and wait for the user to tell you how to proceed.", and
then a note about the user's next message), which is why matching the prefix rather than the whole sentence is what
keeps working across versions.

**A failing hook**: a non-zero exit renders an error block in the TUI carrying the hook's stderr, while hook stdout is
never shown. `exit 2` *blocks*, event-specifically, and feeds stderr to the model. A hook entry with no `timeout`
field blocks the turn for its full duration; the implicit default is over 30 s.

**Hook handler types.** A handler is a `command` (run through a shell) or, from **2.1.63**, an `http` one that POSTs
the event's JSON to a URL from inside Claude Code's own process, so an event costs no process. An HTTP hook is always
synchronous: it has no `async` field, and the turn waits for the answer within the handler's `timeout`. A `2xx` with
an empty body is success with **no decision** (the same as exit 0 with no output), a JSON object body is read as a
command's output would be, and a `2xx` with any other body, a non-`2xx` status, a connection failure and a timeout are
each a **non-blocking error** that shows a notice in the transcript and lets the turn proceed. From **2.1.139** a
`command` handler with an `args` list is run in *exec form*: the command is resolved as an executable and spawned
directly with `args` as its argument vector, with no shell.

**`allowedHttpHookUrls` can switch every HTTP hook off.** When that setting is defined in any settings file (user,
project, local or managed; the arrays merge), Claude Code runs an HTTP hook only if its URL matches one of the patterns
(`*` is a wildcard) and skips the rest without running them; an empty array skips every HTTP hook. So a user or managed
setting that allowlists other URLs without the loopback hook URL leaves an injected HTTP hook doing nothing, and with
it every status change. Octoboard reads the user, project and managed files at launch and keeps the command handler
when the allowlist leaves its URL out; a policy delivered through an MDM profile or the claude.ai console is not
readable from a file and stays a known gap.

**`Notification` takes a matcher on its `notification_type`** (from **2.0.37**): a hook registered with
`"matcher": "idle_prompt"` is delivered only for that type, which keeps the other notifications from reaching a
handler that reads one.

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

**Model and reasoning effort**: `--model <id>` and `--effort <level>`, both global flags, so they work alongside
`--resume`. They sit at the end of the flag list, after `--append-system-prompt`, so nothing non-flag follows a
list-valued flag such as `--mcp-config`. On a model that does not support `max` or `xhigh`, Claude Code **silently
lowers the level to `high`** rather than refusing it.

**Discovering models** (Claude Code 2.1.296): there is **no `models` subcommand**, but there is a real alias layer.
`opus`, `sonnet` and `haiku` resolve to the newest member of each family through the catalogue Claude Code fetches from
its server (opus-5-5, sonnet-5-5 and haiku-5-5 when measured), so nothing needs listing and no version has to be
written down. `best` means "the top model if the account has it, else Opus" and **degrades by construction, so it
cannot be refused**. Three traps: `fable` on its own is gated by the account and **can be refused**; `latest` is a
reserved word and **not an alias**; `mythos` has no alias. The list Octoboard offers is therefore the four strings
`best`, `opus`, `sonnet`, `haiku`. `--effort` takes `low`, `medium`, `high`, `xhigh` and `max`, and a level the resolved
model lacks is silently lowered, so no level is ever refused.

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
  alternative source: it records only *named* threads, and is written late. **With `source: "startup"` it does not
  fire until the first prompt is submitted**, and then arrives together with that prompt's `UserPromptSubmit`, in
  either order — measured 40 s after the process started, both within the same second, and the order varied run to
  run. A mapping that gives it a status therefore races the turn the prompt opens. When the other four sources arrive
  is unmeasured.
- `Stop` — `turn_id`, `last_assistant_message`, `stop_hook_active`.
- `Interrupt` — fires on Esc or Ctrl+C during an in-flight turn (Ctrl+C confirmed by hand). It carries the base
  payload plus the cancelled turn's own `turn_id` and is mutually exclusive with `Stop`. Esc pressed while the session
  is idle fires nothing at all, and `Interrupt` never fires in `codex exec`, so this branch cannot be exercised
  headlessly. A *declined* approval aborts the turn and fires `Interrupt` too, so cancel and decline are told apart by
  whether a `PermissionRequest` went unresolved just before it.
- `PermissionRequest` — fires *before* the dialog reaches the user, and **in the same instant as that call's
  `PreToolUse`**: both hook processes were spawned on the same microsecond timestamp, measured twice (an MCP tool and a
  shell command), so with `async = true` which of the two posts first is a coin flip — both orders were seen across
  runs, the two posts arriving 0.3 ms to 1.9 ms apart. Nothing in the payloads orders them either: neither carries a
  sequence number or a timestamp of its own, and both carry the same `turn_id`, `tool_name` and `tool_input` (only
  `PreToolUse` adds `tool_use_id`). **Granting the approval fires nothing of its own**: the next event is that call's
  `PostToolUse`, measured 0.1 s after the answer, so a session's hand stays up for as long as the approved call runs. A
  decline fires `Interrupt` instead.
- `PostToolUse` — a call that **fails** still fires it. There is no Codex counterpart to Claude Code's
  `PostToolUseFailure`, and none is needed: a command exiting 3 and a write the sandbox refused each fired it, with the
  output as `tool_response`. Measured under `codex exec`, where no modal is raised, so a failing call that was *granted
  at a modal* is unmeasured; nothing suggests the path differs.
- `SessionEnd` — `reason` is schema-pinned to the constant `"other"` and therefore carries no information. `async` is
  ignored for this event: Codex forces it synchronous, so its 3 s clamp is wall-clock cost on every teardown.
- `PreCompact` / `PostCompact` — fire on both a manual and an automatic compaction. A manual one gets a fresh
  `turn_id`; an automatic one fires **after** `Stop` and **reuses the finished turn's `turn_id`**, so the same
  `turn_id` can appear on `Stop` and then again on a compaction event, which turn-keyed state must not read as the turn
  reopening. `PostCompact` carries no summary and no token counts.
- **There is no question or notification event.** The two features that would produce a structured question to the
  user, `default_mode_request_user_input` and `request_permissions_tool`, are off in this build.
- **There is no failure event either**, no counterpart to Claude Code's `StopFailure`. A turn that ends in an error
  fires `UserPromptSubmit` and **nothing else** — no `Stop`, no `Interrupt`, nothing — while Codex returns to its
  prompt about 0.7 s later and the terminal goes completely silent. Measured with an account at its usage limit,
  three sessions, every turn, with a wrapper logging every hook invocation. Esc pressed on such a session fires
  nothing either, so there is no event anywhere that ends the turn.

**A turn's own outcome is in Codex's session record**, the rollout file whose path every hook payload carries as
`transcript_path` (its naming and location are in "Moving a conversation to another config directory" below).
Each turn is an `event_msg` line whose `payload` is `task_started`, then one whose `payload` is `task_complete`,
both carrying the same `turn_id` as that turn's hook payloads. A turn that **failed** has `last_agent_message: null`
and an `error` object (`message`, and `codex_error_info` — `"usage_limit_exceeded"` for the measured cause); a turn
that **ended normally** has `last_agent_message` set and no `error` key at all (its keys are exactly `completed_at`,
`duration_ms`, `last_agent_message`, `started_at`, `time_to_first_token_ms`, `turn_id`, `type`). An interrupted turn
was seen with a null message, no `error` and no `time_to_first_token_ms`, so a null message alone does not mean a
failure. The file is created about 0.25 s **after** the first prompt of a session — not at launch — and Codex
flushes it per record: a failed turn's `task_complete` was readable 1.26 s after the prompt, with the process still
running. The record can land before that turn's hook callback does: the margin measured on a session's first turn was
0.249 s.

**The user's own configuration can remove the prompt entirely.** With `approvals_reviewer = "auto_review"` Codex
resolves an approval request itself: the hook fires, no modal is shown and the tool proceeds. A raised hand would ask
the user to answer something they never see.

**An MCP server's tools are pre-approved per server, and `approve` is the value that asks nothing.**
`mcp_servers.<key>.default_tools_approval_mode` takes `auto`, `prompt`, `writes` or `approve` — the four variants of the
binary's own `AppToolApproval` enum, which `codex app-server generate-json-schema` reports too, and the same four the
per-tool `mcp_servers.<key>.tools.<tool>.approval_mode` takes. Only `auto` and `approve` were exercised; what `prompt`
and `writes` do is unmeasured. `approve` was measured on 0.160.0 and is unmeasured on anything older; an unknown value
in a `-c` override is likely a startup failure rather than a silent fallback. With `auto` a tool carrying no MCP
annotations is put to the user: an interactive session opens `Allow the <key> MCP server to run tool "<tool>"?` with
Allow / Allow for this session / Always allow / Cancel, and nothing reaches the server until it is answered — while an
approval policy of `never`, which `codex exec` runs under, does not ask at all and fails the call with `MCP tool call
requires approval, but approval policy is never` without ever sending it. With `approve` the call is sent, no dialog
appears and the `PermissionRequest` hook does not fire; measured headlessly under `never` and in the TUI under
`on-request`, against a stub server whose tool declares no annotations. The value reaches that one server only: a shell
command needing escalated permissions in the same turn still raised its own approval. Annotations are no substitute —
they could at most speak for the tools that change nothing, while the per-server value settles every tool whatever it
does.

**A long MCP tool call is abandoned after 300 seconds, silently.** `tool_timeout_sec`, a per-server key under
`mcp_servers.<key>`, sets the limit; the default is 300. When it passes Codex gives up on the call client-side and
tells its own model `timed out awaiting tools/call after 300s` — and the server is sent **nothing at all**: no
`notifications/cancelled`, no `$/cancelRequest`, not one further frame until the process exits, and no stdin EOF.
Measured on 0.160.0 against a recording stdio server that answered `initialize` and `tools/list` and never answered
`tools/call`, and confirmed in the app. Unlike Claude Code's move to the background above, nothing marks the call as
still running, so a server holding the call sees no signal to withdraw it and an answer sent after the limit reaches a
caller that is no longer listening. The key takes seconds as a number (an integer or a fraction); 0 is accepted at
startup, and what it does to a call is unmeasured, so it is not a known way to remove the limit. A negative value or a
non-number such as a quoted string fails at startup with an error naming the key (checked on 0.160.0), which is the
evidence that this version reads that key path at all; a misspelled key name was not tested.
Octoboard passes `mcp_servers.octoboard.tool_timeout_sec` at 660, Octoboard's own 600 s forwarding limit plus a minute,
so that its limit is the one that ends a call; the key is per server, so no other server's limit moves. Measured
with a control on 0.160.0: with the override a call answered after 330 s completed and its result reached the model,
which quoted it; without it the identical call failed at 300 s to the second and the server was sent nothing. A call
outlasting 660 s was not tried, so that end of the range is assumed.

**The model is not held silent while an MCP call is outstanding.** Codex lets it take a turn roughly every 64 s for as
long as the call sits in progress, each one saying only that the tool is still running. Those turns do **not** reset
the time limit — in the control above it fired on schedule with four of them behind it. What this costs a call
Octoboard deliberately keeps waiting is in "Waiting for the answer" of `docs/product/session-requests.md`.

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

**Model and reasoning effort**: the model has a flag, `-m <model>`; the effort has **none** and is reachable only as the
config override `-c model_reasoning_effort=<level>`. Octoboard passes both as `-c` overrides (`-c model="<id>"`) so the
two are set the same way, ahead of the positional task, which Codex takes last. Codex **rejects an effort the chosen
model does not advertise**, with an error specific to that model, rather than lowering it.

**Discovering models** (Codex 0.160.0): Codex has **no alias, family or "latest" indirection**, so a model is only ever
its catalogue slug. `codex debug models` prints the catalogue as JSON, `{"models": [...]}`, after refreshing it over
the network (about 600 KB, because every entry carries its full instructions); `--bundled` skips the refresh and prints
the built-in fallback. The fields that matter per entry: `slug` (what `-c model=` takes), `display_name`, `visibility`
(`list` or `hide`; hidden entries are internal or retired and not for a picker), `priority` (ascending is display
order), and `supported_reasoning_levels`, a list of `{effort, description}` whose `effort` values are exactly what
`model_reasoning_effort` accepts for that model (a level set can include ones Claude Code has no word for, such as
`ultra`), with `default_reasoning_level`. **The server's answer arrives sorted by `priority`; the bundled fallback does
not** (it was observed in an unrelated order), so sort it yourself. The command reads `CODEX_HOME`, so a catalogue is
that account's. **An unknown slug is not rejected at launch**: the session boots and fails only at its first turn, which
is why a list that is wrong cannot be left to the launch to catch.

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

**A long MCP tool call is cancelled, and the server is told.** `tool_timeout_sec`, a per-server key under
`mcp_servers.<key>`, sets the limit; Grok's own user guide (`~/.grok/docs/user-guide/07-mcp-servers.md`) documents the
default as 6000 seconds, and a measurement bounds it from below: a call left unanswered was still outstanding at 310 s,
so the `default: 60` that another string inside the executable suggests is not what this build does. The exact default
was not measured. When the limit does pass, Grok sends the server a `notifications/cancelled` carrying `{"requestId":
<id>, "reason": "request timeout"}` — unlike Codex, which sends nothing at all — and leaves the connection open: no
stdin EOF followed the cancellation, nor a normal exit of the same session. Its own model is told `Tool
\`<server>__<tool>\` failed via \`use_tool\`: MCP tool '<tool>' timed out after <n> seconds`, and the turn carries on.
Measured on 1.0.50 against a recording stdio server that never answered `tools/call`, with the limit at 20 s (the
cancellation arrived at 20.65 s) and a tool that did answer in the same run as the control; the key is honoured in the
block Octoboard appends to the `config.toml` copy, which is where the 20 s came from. Octoboard sets it to 660 there,
its own 600 s forwarding limit plus a minute, so that its limit is the one that ends a call: a cancellation withdraws a
waiting session request (`request_session`), which would end the user's 8 minutes to answer early (see "How a request ends" in `docs/product/session-requests.md`). On 1.0.52 with a
real account, the appended `[mcp_servers.octoboard]` block was read off a live farm carrying
`tool_timeout_sec = 660`.

**The model is held silent while an MCP call is outstanding**, the opposite of Codex. Through a
`request_console_session` call (the tool is now `request_session`, which waits the same way) that Octoboard kept
waiting the full 8 minutes, Grok sent its endpoint nothing between the turn's own request and the one carrying the
tool's error: no turn of its own, no token spent (measured on 1.0.50 against a stub inference endpoint).

**What the model itself is handed, and what it calls** (1.0.50 against a stand-in endpoint, re-confirmed on 1.0.52
against a real account): an MCP tool is **not** among the tools offered to the model. They are reached through Grok's
own `use_tool`, whose `tool_name` is the qualified `<server key>__<tool>` and whose `tool_input` holds the arguments —
the wrapper the `PreToolUse` payload shows — and a `search_tool` call first is not required, though 1.0.52 injects a
system reminder of its own telling the model to call `search_tool` for a tool's input schema before `use_tool`. In four
real Grok Build 1.0.52 sessions driven through `request_console_session` (now `request_session`) end to end, every one
called `search_tool` at least once, and what a real model called was the `use_tool` wrapper with the qualified
`octoboard__<tool>` name. Every chat request sets `stream: true`, so anything standing in for the endpoint has to answer as a stream. A turn is also
followed by requests that offer no tools at all and belong to the TUI rather than the agent: the dashboard summary line,
the session title, and a prediction of the user's next line. Inference for one model can be routed on its own through a
`[model.<name>]` entry carrying its own `base_url` (and `api_key`), without the `endpoints.models_base_url` that moves
every model's.

**Model and reasoning effort**: `--model <id>` and `--reasoning-effort <level>`, placed after the `--rules` pair and
before the positional task. Grok persists some session settings back into `config.toml`; the per-session `GROK_HOME`
farm keeps `config.toml` as a copy, so a flag cannot leak into the user's own configuration.

**Discovering models** (Grok Build 1.0.50): Grok has **no alias or "latest" indirection** either. `grok models` prints
to stdout the login line, a blank line, `Default model: <id>`, a blank line, `Available models:` and then one bullet
line per model, `  * <id> (default)` for the default and `  - <id>` for the others, in the order Grok shows them. It
carries no display name and no effort levels. It also refreshes `models_cache.json` in the config directory (`GROK_HOME`),
a JSON object with `fetched_at`, `grok_version`, `etag` and `models`, a map **keyed by model id**; each value's `info`
carries `name` (the display name), `hidden`, `supports_reasoning_effort`, `reasoning_effort` (the default) and
`reasoning_efforts`, a list of `{id, value, label, description, default}` whose `value` is what `--reasoning-effort`
takes (the level set differs per model: the oldest of the four measured lacked `xhigh`). Octoboard takes the id list
from the text, which is the one Grok itself shows, and the names, efforts and `hidden` flag from the file; the file
alone would do for the data but only exists after Grok has run in that home, and the text is what refreshes it. The
cache is per account (its `identity` differs). **An unknown id is refused loudly at launch** with `Couldn't set model
'<id>'.`, the opposite of Codex's silent deferred failure.

**The id a turn records is not one of the ids `grok models` prints** (1.0.52). With nothing set, a session runs the
`config.toml` default — `grok-4.7` on the account measured — while its own session record and usage file name the
served model `grok-4.7-build`. So the id to match a turn against the list Octoboard offers is the configured one, not
the one the records carry.

## Across the three agents

**Injected MCP tool names** differ in shape, which matters wherever a role description has to name a tool: a server
registered under the key `k` exposes its tool `t` as `mcp__k__t` on Claude Code and Codex, and as `k__t` on Grok.

**All three connect their MCP servers when the process starts**, before any turn — a `command`-type server's child is
already running by the time the TUI is drawn. So whether an injected server was accepted is observable without a prompt
ever being submitted.

**Terminal titles and the titles in an agent's own records.** Each agent sends its title with OSC 0/2 and decorates it
(raw strings observed with Codex 0.163, Claude Code 2.1.301 and Grok Build 1.0.52, `proj` being the working directory's
name):

- Codex — items joined by ` | `, the project last: `proj` at startup, `⠧ ⠧ | proj` and `⠙ | proj` while working, `[ ! ]
  Action Required | proj` at an approval, `Explain pseudo terminal | proj` once the thread is named, and an empty title
  at exit. The project item is the working directory's name (whether it is the git repository root's name inside a
  worktree is unverified). A thread is named in `<CODEX_HOME>/session_index.jsonl`, append-only lines
  `{"id","thread_name","updated_at"}` with the thread id as `id`, the last line of an id winning, written by `/rename`
  too. Codex's own naming fails while an MCP server is supplied through `-c` (openai/codex#53087), so an Octoboard
  session stays unnamed there. What the user typed is in the rollout as an `event_msg` of type `user_message`
  (`payload.message`) up to 0.142.5, and as one of type `item_completed` whose `item` is a `UserMessage`
  (`item.content[].text`) in 0.163.0; its user-role `response_item` messages include the injected environment context
  and `AGENTS.md` instructions.
- Claude Code — a status glyph, a space and the title: `✳ Claude Code` (a placeholder until the conversation has a
  title), `◐ Claude Code`, `◐ Pseudo terminal explanation`, `✳ Pseudo terminal explanation`, and an empty title at exit.
  The transcript carries `{"type":"custom-title","customTitle"}` (the user's rename) and `{"type":"ai-title","aiTitle"}`
  lines; the last of each kind wins and the rename outranks the generated one.
- Grok Build — items joined by ` - `, its own name last: `grok` at startup and exit, `⠋ - Waiting for response… - grok`
  and `⠋ - Waiting for response… - Explain in one sentence what a pseudo te… - grok` while working, `Explain in one
  sentence what a pseudo te… - grok` when idle. The session directory's `summary.json` holds `generated_title`.

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
That suffix is why **setting the variable costs the login whatever directory it names** — pointed at the real
`~/.claude`, under the real `HOME`, `claude auth status` still answers `loggedIn: false` / `authMethod: "none"` while
echoing back the right `configDirectory` (measured on 2.1.296 across a real and a throwaway `HOME`, with a fresh
directory and with the real one named explicitly). So a pinned directory needs its own `claude auth login` and cannot
borrow the default directory's; whether one signed in that way then stays signed in was not tried. Two preconditions
for reading any of this back, both measured the same day: the environment has to keep `USER` — without it the binary
answers `loggedIn: false` even under the real `HOME` with the variable unset — and a throwaway `HOME` needs the
`Library/Keychains` symlink, without which the unset-variable case answers `loggedIn: false` too. Codex differs on
every count: `CODEX_HOME` pointed at the user's real `~/.codex` keeps Codex logged in under a throwaway `HOME`, and it
needs neither `USER` nor the symlink.
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
| Inject MCP | `--mcp-config <json>` | `-c 'mcp_servers.octoboard.command=…'`, `.args=[…]`, `.default_tools_approval_mode="approve"` and `.tool_timeout_sec=660` | An `[mcp_servers.octoboard]` block appended to the `config.toml` copy inside that `GROK_HOME`, with `tool_timeout_sec = 660` |
| Inject a role description | `--append-system-prompt` | `-c 'developer_instructions="…"'`, which adds a developer message and leaves the rest of the prompt byte-identical; not `-c instructions=`, which replaces the system prompt | `--rules "…"`, appended to the system prompt and persisted into the session record |
| Resume | `claude --resume <id>` | `codex resume <id>` | `grok --resume <id>` |
| Choose the model and reasoning effort | `--model <id>`, `--effort <level>` | `-c model="<id>"`, `-c model_reasoning_effort="<level>"` | `--model <id>`, `--reasoning-effort <level>` |
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
  undocumented `--trust` flag also exists), and Grok's project hooks, unlike its instruction files, also need a
  recognized **git** root, since they did not load in a trusted non-git directory. Octoboard makes no trust decision of
  its own in either agent's store and passes none for a launch; it presses the agent's own confirmation (see
  "Folder-trust confirmations" below). On Claude Code an untrusted workspace makes the project's `allow` rules ignored
  (with a line on stderr) while `deny` still applies, so the session is only ever more restrictive. Its trust lives in
  the global config file (`~/.claude.json`, or `.claude.json` inside the config directory in effect), which Octoboard
  reads and never writes. It is read rather than matching the stderr line because on a PTY stderr is the rendered UI,
  and matching rendered text would need a VT emulator in the daemon; it warns only on an explicit negative, so a renamed
  key does not warn on every launch.
- **Each agent reads a different instruction filename, matched by exact spelling.** Claude Code reads `CLAUDE.md` and
  `CLAUDE.local.md` and does **not** read `AGENTS.md`, and cannot be made to; where it must be shown a project's
  `AGENTS.md`, `--append-system-prompt-file` lands it in the system prompt. Codex reads `AGENTS.md`. Grok reads
  `AGENTS.md`, `Agents.md`, `CLAUDE.md`, `Claude.md` and `CLAUDE.local.md`, and on a case-sensitive volume loads every
  matching spelling present. No agent accepts an all-lowercase name. Folder trust, not a git repository, is what
  gates Grok's instruction files: in a trusted folder that is not a repository Grok reads `AGENTS.md` and `CLAUDE.md`
  from the working directory alone (Grok's own guide says outside a repository only the working directory is read),
  and an untrusted one loads neither (measured on Grok Build 1.0.52). Its project *hooks* are the part that still
  needs a git root. Grok loads one file reached by two names once when the second is a symlink to the first, and a
  pair of separate identical files twice, so a console's working directory holds one `AGENTS.md` and a relative
  symlink `CLAUDE.md -> AGENTS.md`, and every agent's console session reads the same text. Claude Code follows the
  symlink, and replacing the target by a temporary file and an atomic rename leaves the link working, with the next
  launch seeing the new content (Claude Code 2.1.300). The file's text therefore names the tools without any agent's
  prefix, since each agent shows them under its own.
- **Each agent hands the instruction file to the model whole.** Custom tags and Markdown headings arrive intact, with
  nothing escaped, reformatted or truncated; only the framing around the file differs. Claude Code puts it in the first
  user message, inside a system reminder, after "Contents of …/CLAUDE.md (project instructions…)". Codex puts it in a
  user message under "# AGENTS.md instructions for <dir>", inside `<INSTRUCTIONS>`. Grok puts it in the first user
  message as `<always_applied_workspace_rule name=".../Agents.md">`, a spelling of its own for the name on a
  case-insensitive volume. Measured on Claude Code 2.1.300, Codex 0.162.1 and Grok Build 1.0.52 by capturing the
  requests against local stubs.
- **Resume re-injects everything.** Hooks and the MCP server are resolved from the launch arguments every time and are
  lost on a resume that omits them, silently. The role description differs: Grok persists `--rules` into the session
  record, while Claude Code records its appended system prompt on the first request and replays it verbatim, so a
  changed role text is ignored on resume. A session's role is immutable for its lifetime. Grok also freezes its
  project instruction files per session: on a resume and on `--fork-session` it replays the stored original system
  prompt and first-message instructions, so neither a new `--rules` nor an edited `AGENTS.md` reaches it, and only a
  fresh session sees a change (Grok Build 1.0.52).
- **An instruction file edited between launches reaches a resumed session differently on each agent.** Probed by
  capturing the exact model requests against local stub servers (Claude Code 2.1.300, Codex 0.162.1, Grok Build
  1.0.52).
  - **Claude Code** keeps the old copy in the replayed history. When a file changed between launches, the resumed
    session's new turn carries the whole new file with a note that it replaces the earlier copy ("Instruction files
    were re-read when this session started; these differ from their earlier copies — each replaces its earlier
    copy"); an unchanged file adds nothing, and each further change appends another full copy. A change made while
    the process runs is announced on the next turn as "changed on disk", with the changed lines. A compaction drops
    every earlier copy and re-injects one fresh block.
  - **Codex** also keeps the old copy in the replayed history, on `exec resume` and on an interactive resume alike.
    When `AGENTS.md` changed between launches, the resumed session's new turn carries the whole new file, opening with
    "These AGENTS.md instructions replace all previously provided AGENTS.md instructions."; an unchanged file adds
    nothing, and each further change appends another full copy on the next resume. Unlike Claude Code, it reads the
    file once at process start, a resume launch included, and holds it in memory: a change made while the process runs
    is not sent on later turns, and a compaction drops every earlier copy and re-injects that launch-time copy, not
    what is on disk. Such a change reaches the session on its next resume.
    Codex 0.142.5 behaved otherwise: a resume replayed only the stored copy, and a changed file reached the session
    only through a compaction.
  - **Grok Build** freezes the files per session, on a resume and on a fork (above).

  So an edited console instruction reaches every agent's new sessions and a resumed Claude Code or Codex session as
  well (Codex 0.142.5 excepted), and a Grok session never within its life.
- **Codex truncates the project instruction files it reads at `project_doc_max_bytes`, 32 KiB by default**, so a long
  user text in a console's `AGENTS.md` can be cut off for Codex alone: Claude Code and Grok have no such cap that
  Octoboard knows of. Verified only partly on Codex 0.162.1: the binary's own default-config snapshot reads
  `project_doc_max_bytes = 32768`, and the key is a documented `config.toml` setting. Whether the cap counts the
  built-in guidance, a global `~/.codex/AGENTS.md` and the console's text together, and what exactly is cut, was not
  measured, because that needs a model request. Octoboard sets no limit on the user's text.
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
  one it never asks and reads `AGENTS.md` all the same (measured on 0.161.0, and again on 0.162.1, where an interactive
  launch in a fresh non-git directory drew no confirmation and its first request carried that directory's `AGENTS.md`,
  while one in a fresh `git init` directory drew the confirmation below). The screen reads "Trust this folder? Codex can
  read, edit, and run files here, …", then "› 1. Trust and continue", "2. Quit" and "enter continue · esc quit". The
  cursor (`›`, U+203A) starts on option 1, and Enter alone accepts; any carriage return reaching the screen accepts.
  Codex records `[projects."<repository root>"] trust_level = "trusted"` in `$CODEX_HOME/config.toml`: from a
  subdirectory it records the repository root, and from a linked worktree the main repository's root, so the record can
  cover more than the folder asked about. A trusted parent directory in that file does not cover a repository below it.
  No hook fires before or right after the confirmation, so only a bound on time and output can end a watch for it. Codex
  redraws only the cells that changed, so text read off the screen can lose characters wherever the frame before held
  the same one; the confirmation's own phrases came through intact in its first drawing, a path above it did not. The
  review Codex raises for new hooks without `--dangerously-bypass-hook-trust` reads "Hooks need review … 2. Trust all
  and continue" and is a different modal. On 0.161.0 a key the confirmation ignores (a letter, Right, Tab, `?`) draws
  nothing but a bare synchronized-update frame (`ESC[?2026h ESC[39m ESC[49m ESC[0m ESC[?2026l`); accepting it clears the
  screen and redraws it whole, starting with the ">_ OpenAI Codex (v…)" banner, which is how an answer given in the
  terminal is told apart.
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
