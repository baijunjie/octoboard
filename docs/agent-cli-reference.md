# Agent CLI reference

Per-agent facts about the three agent CLIs Octoboard launches: the hook events, what their payloads carry, what the
payloads can and cannot be correlated on, what a failing hook costs, how a project's own configuration layers around an
injected one, the mechanism and conditions for injecting Octoboard into each, what it takes to move a session's
conversation into another config directory, how each signs in a config directory without a terminal and what it answers
before that, what happens when two accounts share a session directory or the rest of a configuration directory, and how
to ask each one how much of its plan quota is left and how to start a usage window that has not begun counting.

Established against **Claude Code 2.1.274**, **Codex 0.160.0** and **Grok Build 1.0.46**; what the Claude Code section
says about a declined prompt was measured on **2.1.286** as well, and held identically on both. What "Moving a
conversation to another config directory" says was measured on **Claude Code 2.1.289**, **Codex 0.160.0** and **Grok
Build 1.0.46**. What "Folder-trust confirmations" says about Codex and Grok Build was measured on **Codex 0.161.0** and
**Grok Build 1.0.50**. What "A long MCP tool call is moved to the background" and "A pasted prompt reaches
`UserPromptSubmit` wrapped" say was observed on **Claude Code 2.1.295**. Each agent's "Model and reasoning effort" note
was measured on **Claude Code 2.1.296**, **Codex 0.160.0** and **Grok Build 1.0.50**; the Codex facts were not
re-checked on **0.161.0**, which the folder-trust facts above were measured on. What the Grok Build section's "What the
model itself is handed, and what it calls" says was re-confirmed on **Grok Build 1.0.52** with a real account, as was
the timeout paragraph's `tool_timeout_sec = 660`; "The id a turn records is not one of the ids `grok models` prints" was
first measured on that build. What "Plan quota" says was measured on **Claude Code 2.1.302**, **Codex 0.162.1** and
**Grok Build 1.0.60**. What the Claude Code section's "Permission mode" says was measured on **Claude Code 2.1.303**.
What "Starting a quota window" says was checked on 2026-10-22 against **Claude Code 2.1.308**, **Codex 0.162.1** and
**Grok Build 1.0.67**.
What "Sharing a session directory between accounts" says, and the "Codex, history segments" and "Under a different
account" notes under "Moving a conversation to another config directory", were measured on 2026-10-19 on **Claude Code
2.1.302**, **Codex 0.162.1** and **Grok Build 1.0.62**. What "Codex resume from aged rollouts" says was measured on
**Codex 0.162.1**. What "Sharing a configuration directory between accounts" says was measured on 2026-10-20 on **Claude
Code 2.1.304**, **Codex 0.162.1** and **Grok Build 1.0.62**. The Grok Build chain-following note and the per-session
`config.toml` copy note in that section, including plugin switches, and the Grok Build section's notes on what startup
saves of the model and effort and on theme saves with `GROK_THEME` set, were measured on 2026-10-21 on **Grok Build
1.0.62**. What "Signing in a config directory without a terminal" says was measured on 2026-10-19 on **Claude Code
2.1.303**, **Codex 0.162.1** and **Grok Build 1.0.62**. What "Writing into a running session" says about invisible
characters was read from the executable of, and measured on, **Claude Code 2.1.304** on 2026-10-20. All three rewrite
their hook surface, their payload fields, their configuration layering and their on-disk layout on upgrade, so check a
detail here against the installed version before relying on it.

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
  A backgrounded shell command (`run_in_background`, measured on 2.1.303) is listed as `id`, `type` `"shell"`,
  `status` `"running"`, `description` and `command`. The list is the complete current one on every `Stop`, so it
  can be replaced wholesale rather than patched (measured on 2.1.303: with two backgrounded sleeps finishing at
  different times, each `Stop` listed exactly the ones still running, and the last listed none). A finished task
  is reported to the session as a `UserPromptSubmit` whose prompt is a `<task-notification>` carrying its
  `<task-id>`, followed by a `Stop` of its own. A subagent running in the background is listed the same way (see
  "Subagent events"). `/clear` does not end a running background shell: the next `Stop`
  after the `SessionStart` with source `clear` still listed it (measured through `-p --input-format stream-json`;
  the interactive TUI is unmeasured).
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
  use as a general backstop. It is **not suppressed by a paused `Stop`** except for background *agent* tasks (2.1.303):
  with a shell, Monitor or MCP task still running, `idle_prompt` fires 60 s after the `Stop` that listed it, so the
  notification cannot be read as "nothing is running".

**A configuration that names no permission mode does not ask.** On 2.1.296 a config directory whose `settings.json`
carries no `permissions` block runs in *auto mode* (status line `⏵⏵ auto mode on (shift+tab to cycle)`), and a `Write`
creating a new file in the session's own working directory went through with no prompt and no `PermissionRequest`. The
mode that prompts has to be asked for: `permissions.defaultMode: "default"` in that directory's `settings.json` when
the account follows its configuration and a launch passes no `--permission-mode` (an explicit choice in Settings is
passed as that flag; see "Permission mode" below). The running session then calls that mode `⏸ manual mode on`, which
matches neither the settings value nor the word "normal" — read the mode off the session's own transcript instead,
where it is recorded unambiguously as `{"type":"permission-mode","permissionMode":"default"}` and
`{"type":"mode","mode":"normal"}`.

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
and should close the dangling `PreToolUse`. The interrupted tool's own child process is already gone. The only
machine-readable trace is in the transcript at `transcript_path`. An interrupt in the thinking or writing phase writes
a single top-level `"type":"user"` entry whose `message.content` is `[{"type":"text","text":"[Request interrupted by
user]"}]`, carrying `interruptedMessageId` (seen on 2.1.296, 2.1.302 and 2.1.303). Mid-tool, it writes a `tool_result`
with `is_error: true` beginning "The user doesn't want to proceed with this tool use", followed by a user block
`[Request interrupted by user for tool use]`. Both are entries whose `message.content` is
an **array** of blocks, and the first of them — the one holding the `tool_result` — also carries
`"toolDenialKind": "user-rejected"`, at entry level rather than inside the block; the
`[Request interrupted by user for tool use]` entry carries none. A declined prompt and a declined `AskUserQuestion`
write this same pair, which is why one mechanism recovers all three cases.

**The markers are also replayed.** `api-request-blob` entries written into the main transcript while a session works
carry the conversation so far in their `message.content`, earlier interrupt markers included (once with a trailing
`\n`, once without), and `queue-operation` entries can carry the marker text too; only a top-level `"type":"user"`
entry is the user's own. **A new conversation's transcript does not exist yet** when its first `UserPromptSubmit` hook
runs (measured on 2.1.303 in print mode), so a watcher armed there has to wait for the file to appear.

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

**`SessionStart` and `Setup` run no HTTP handler** (2.1.307): an `http` handler registered for either is skipped, with
only a debug-log line ("HTTP hooks are not supported for SessionStart") and nothing on screen, and those two are the
only events the check names. So a session that runs every hook over HTTP sends nothing at all on a launch or a
`--resume` with no prompt until the user types. A `command` handler with `async: true` for `SessionStart` runs to
completion and shows nothing in the TUI; measured, it fired with `source` `startup` 0.3–0.9 s after a fresh launch and
with `resume` 0.3–1.9 s after a `--resume <id>` with no prompt. Like every hook it is skipped, not deferred, while the
workspace-trust screen is up: in an untrusted folder it fired about 0.5 s after the screen was accepted and not at all
when it was declined. Octoboard therefore registers `SessionStart` as an async command even where every other event
is HTTP.

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
The level a reply ran at is always on record: each main-transcript assistant line carries it as a top-level `effort`,
also when nothing sets one — every such line of 155 transcripts written by 2.1.286 to 2.1.304, on a `settings.json`
with no `effortLevel`.

**Discovering models** (Claude Code 2.1.296): there is **no `models` subcommand**, but there is a real alias layer.
`opus`, `sonnet` and `haiku` resolve to the newest member of each family through the catalogue Claude Code fetches from
its server (opus-5-5, sonnet-5-5 and haiku-5-5 when measured), so nothing needs listing and no version has to be
written down. `best` means "the top model if the account has it, else Opus" and **degrades by construction, so it
cannot be refused**. Three traps: `fable` on its own is gated by the account and **can be refused**; `latest` is a
reserved word and **not an alias**; `mythos` has no alias. The list Octoboard offers is therefore the four strings
`best`, `opus`, `sonnet`, `haiku`. `--effort` takes `low`, `medium`, `high`, `xhigh` and `max`, and a level the resolved
model lacks is silently lowered, so no level is ever refused.

**Permission mode** (Claude Code 2.1.303): `--permission-mode <mode>` is a global flag, so it works alongside `--resume`
(including `bypassPermissions` with the `--settings` key, which showed no warning on a resume). The listed values are
`acceptEdits`, `auto`, `bypassPermissions`, `manual`, `dontAsk` and `plan`; `default` is still accepted and equals
`manual`, and is what the hooks and the transcript report. `manual` exists only from 2.1.200, so Octoboard passes
`default` for "ask me".

- **`bypassPermissions` blocks on each config directory's first use.** After onboarding or login and the trust screen
  it shows a "Bypass Permissions mode" warning with the cursor on "No, exit" (Enter exits with status 1, Esc with 0); no
  hook fires while it is up. Accepting writes only `skipDangerousModePermissionPrompt: true` to
  `$CLAUDE_CONFIG_DIR/settings.json`. The same key in `.claude.json` does not suppress it.
  `--settings '{"skipDangerousModePermissionPrompt":true}'` suppresses it and writes nothing, which is what Octoboard
  passes (merged into its hook `--settings` JSON); background and `claude agents` sessions do not honour a flag-only
  consent, which is irrelevant for a PTY launch.
- **Each mode as launched** (2.1.303, fresh config dir logged in through the Keychain, a conversation with one turn,
  then stopped and resumed): no flag started in `⏵⏵ auto mode on`, `--permission-mode auto` in `⏵⏵ auto mode on`,
  `--permission-mode default` in `⏸ manual mode on`, and `--permission-mode bypassPermissions` plus the `--settings`
  key in `⏵⏵ bypass permissions on`, on the fresh launch and on `--resume` alike. No warning screen appeared and no
  `settings.json` was written to the config dir in any of them. A conversation with no turn yet is relaunched with a
  new `--session-id` instead of `--resume`.
- **`auto` has no blocking consent** (since 2.1.152), only a non-blocking notice, recorded as
  `hasSeenAutoModeEntryWarning` in `.claude.json`. It is available on Max, and may fall back with a notice where it is
  not (plan, model, `disableAutoMode`); those fallbacks and a non-Max plan were not measured.
- **In `auto`, an `ask` rule raises a normal prompt** with `PreToolUse`, then `PermissionRequest` (`permission_mode` is
  `auto`), then `Notification` `permission_prompt`. A tool call the classifier blocks shows no prompt: `PreToolUse` and
  then `Stop`, with no `PermissionRequest` and no `PostToolUse`.

**Dialogs at a resume and in an idle session** (2.1.307, read from the executable and reproduced with fake
subscription credentials against a local stub). None runs a hook, and `idle_prompt` is not sent while one is up.

- **"Resume this conversation?" at launch.** After `--resume` or `--continue` with **no prompt on the command line**,
  Claude Code can ask before the first prompt, at the same moment `SessionStart` with `resume` fires: "This
  conversation has been inactive for 3d 0h 1m and is 42k tokens long. Resuming it will use about 42% of your 5-hour
  usage limit.", rows "1. Resume" and "2. Start a new conversation", footer "Enter to confirm · Esc to resume". It is
  shown only to a subscription account whose rate-limit tier is known, with the prompt cache cold (or the model
  changed), when the conversation's cost reaches a server-set share of the 5-hour limit (5% in the cached flags seen),
  and not once `resumeReturnDismissed` is set in the config directory's `.claude.json` (what "Don't ask me again"
  writes; no setting or variable reaches it). A bracketed paste is discarded and a CR picks the focused row; the
  focus starts on Resume, or under a server flag on "Start a new conversation", which then runs `/clear` (a
  `SessionEnd` with `reason` `clear`, then `SessionStart` with `clear` and a new session id). A typed digit picks its
  row; Esc keeps the conversation. **A launch with a prompt never shows it**: `--resume <id> "<prompt>"` ran the turn
  straight away, as did `-p`.
- **The same title in an idle session.** Under a server flag (off on the accounts seen) it also comes up on its own once
  the prompt cache has expired — at the cache TTL after the last request, an hour for a subscriber — with unnumbered
  rows and the footer "Enter to confirm · Esc to dismiss". Here a paste goes into the input box as a draft (the dialog
  adds "Your unsent message will still be there after you answer.") and the CR is swallowed, so nothing is lost and no
  turn starts.
- **"Resume from summary".** Under another server flag the no-prompt resume can show "This session is 3d old and 150k
  tokens." with "1. Resume from summary (recommended)", "2. Resume full session as-is", "3. Don't ask me again" and
  "Enter to confirm · Esc to cancel" (from 70 minutes and 100k tokens, adjustable with
  `CLAUDE_CODE_RESUME_THRESHOLD_MINUTES` and `CLAUDE_CODE_RESUME_TOKEN_THRESHOLD`). A blind Enter runs `/compact`.

Octoboard passes an instruction for a relaunched conversation as the launch prompt, which keeps both launch dialogs
away, and holds its writes into any session whose screen shows one of them (`docs/product/message-delivery.md`).

**A notice that waits for Enter** (2.1.308): in `auto` mode with `ANTHROPIC_BASE_URL` pointing elsewhere, the first
classifier call of a session raised "We're changing auto mode to no longer charge for classifier requests …" mid-turn,
ending "Enter to continue · Esc to cancel" (the rewind dialog's footer too), with no hook. A message written then was
lost: its Enter dismissed the notice and the text went nowhere. Octoboard holds its writes while that footer is on
screen.

**Views of background agents** (2.1.307, reproduced against a local stub). A turn that starts an `Agent` with
`run_in_background` adds a task list under the prompt box — a filled `⏺ main` row and a row per agent — and the hint
"↓ to manage". With the footer focused ("↑/↓ to select", and "Enter to view · x to stop" on an agent's row) a paste is
dropped and a CR opens the agent's view. **In that view** the conversation area shows the subagent's transcript, the
`❯` prompt box stays (its placeholder reads "Message @<agent type>…"), and the main row turns hollow, `◯ main`. Text
sent there goes to the subagent, with no `UserPromptSubmit`; sent to a finished agent it resumes it. The view stays up
after the agent finishes. No single key leaves it safely: Esc in the input stops a running agent, Ctrl+C kills it, and
← at an empty input opens the agents view; ↓ to the main row and Enter returns to the conversation with the footer
still focused. **The agents view** (`claude agents`, ← at an empty prompt, hinted as "← for agents" for the first ten
seconds) moves the conversation into a background session forked under a new session id, starts a transient
`claude daemon`, and shows "New sessions: …", "0 awaiting input · 1 working · 0 completed" and "describe a task for a
new session": a paste and CR there start another session with that text. `CLAUDE_CODE_DISABLE_AGENT_VIEW=1` (setting
`disableAgentView`) removes the agents view and its hint and leaves the subagent view. Octoboard holds its writes while
either view, or the focused task list, is on screen.

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
and `<cwd>/.codex/skills/`. `<cwd>/.codex/config.toml` is layered over `CODEX_HOME/config.toml` once the project is
trusted and ignored before that (see "The effort a turn runs at when nothing sets one" below), and a `-c` override
outranks both: on 0.162.1 a trusted project's `[tui] terminal_title = []` stopped every terminal title, and
`-c tui.terminal_title=[…]` brought them back.

**An update prompt can come first**: when a newer version is out, Codex can open with an "Update now / Skip / Skip
until next version" prompt (seen as an "Update available!" banner in the probe on 0.161.0). Octoboard passes
`-c check_for_update_on_startup=false` on every launch, the setting Codex's own config defines for whether it checks
for updates at startup and shows that prompt, so nothing sits in front of the folder-trust confirmation; it applies to
that launch only. Measured on 0.160.0, with an update made available by raising `latest_version` in Codex's cached
`version.json`: a plain launch opened with the update prompt, and a launch with the override went straight to the
folder-trust confirmation and showed no update prompt after it either.

**Screens before the session starts, and the overrides that keep them away** (0.162.1, measured with a scratch
`CODEX_HOME` against a local stub). Each is drawn before Codex sets its first terminal title and runs no hook, and a
paste and CR answer it:

- On a resume from a directory other than the one the rollout last recorded, a working-directory chooser ("Working
  directory · resume", "1. Use session directory (…)", "2. Use current directory", "3. Always use session directory",
  "4. Always use current directory", "enter continue · esc use session · ⌃c quit"); a CR picks the session directory.
  `-c tui.resume_cwd="current"` skips it.
- When the effective model has an `upgrade` entry in the catalogue (`models_cache.json`), "Meet <model> …" with "1. Try
  new model" and "2. Use existing model"; a CR accepted the new model and wrote `model`, `model_reasoning_effort` and
  `[notice.model_migrations]` into the user's `config.toml`. `-c 'notice.model_migrations={"<from>"="<to>"}'` for the
  model marks it seen and skips it.
- With no login, the sign-in screen ("Welcome to Codex", "1. Sign in with ChatGPT", "2. Sign in with Device Code",
  "3. Provide your own API key", "Press enter to continue"); a CR starts the ChatGPT sign-in. Nothing skips it.

Mid-session, close to a rate limit, Codex draws "Approaching rate limits" / "Switch to <model> for lower credit
usage?" with "1. Switch to …", "2. Keep current model" and "3. Keep current model (never show again)" after a turn, with
no title change and no hook; a paste and CR switched the model and the message was never sent.
`-c notice.hide_rate_limit_model_nudge=true` keeps it away. Octoboard passes these three overrides, the migration table
built from the session's catalogue cache, and holds its writes while the sign-in screen or any of these is on screen.

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

**The effort a turn runs at when nothing sets one** (Codex 0.162.1, read off existing records on 2026-10-20, no turn
spent): a rollout's `turn_context` payload carries the effort the configuration resolved to, and **leaves the `effort`
key out** when no layer sets `model_reasoning_effort` — 335 such records on the account measured, none carrying `null`.
Codex then runs the model at its catalogue entry's `default_reasoning_level`. The catalogue is cached per account as
`models_cache.json` in `CODEX_HOME`, in the `{"models": [...]}` shape `codex debug models` prints, next to
`fetched_at`, `client_version`, `etag` and `identity`. The default is the account's, not the model's alone: the same
day, `gpt-6.1-sol` defaulted to `medium` on one account and to `low` on another. That this is the level actually run is
corroborated rather than read off a request: in 35 sessions whose turn contexts left the effort out, each subagent Codex
spawned was recorded in a `SubAgentActivity` item with `reasoning_effort` `medium`, that account's default, and not
with the entry's `multi_agent_reasoning_effort` (`xhigh`). Which configuration layers a turn context reflects was
measured with `codex doctor --json`, whose `config.load` check names the effective `model` but not the effort: a
`.codex/config.toml` in the working directory is layered over `CODEX_HOME/config.toml` once the project is trusted and
ignored before that, and a top-level `profile` key in `config.toml` makes the configuration fail to load (`invalid
data`) — `--profile <name>` on this version layers `$CODEX_HOME/<name>.config.toml` instead.

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
before the positional task. In a fullscreen TUI (1.0.62), startup alone persists these as `[models] default` and
`default_reasoning_effort`; a headless `-p` run did not. An explicit `/model other-local high` changed both keys in the
TUI. A launch's own model chosen again cannot be told from its startup save. The per-session `GROK_HOME` keeps
`config.toml` as a real copy; what Octoboard carries from it, and why the model passed at launch and the effort are
left out, is under "Grok itself rewrites the per-session `config.toml` copy" in "Sharing a configuration directory
between accounts".

**Theme** (Grok Build 1.0.62, from `grok --help`, the strings in the executable and Grok's own theming guide): there is
**no `--theme` flag**. The environment variable `GROK_THEME` (or `LC_GROK_THEME`) forces a theme and outranks
`config.toml`'s `[ui] theme`. The values include `groknight` (the default, dark: its backgrounds are near-black
greys), `grokday` (light greys with dark text), `tokyonight`, `rosepine-moon`, `oscura-midnight`, `terminal` (the
terminal's own colours) and `auto`. On a local macOS desktop `auto` follows the system appearance
(`AppleInterfaceStyle`), not the terminal's background, and picks the `auto_light_theme` / `auto_dark_theme` themes
(`grokday` / `groknight` by default). A dark theme on a light terminal leaves the cells Grok paints with the terminal's
default colour showing through as light holes between dark bands.

With `GROK_THEME=groknight`, `grokday` or `auto`, fullscreen startup left `[ui] theme` unchanged, including leaving
the key absent when it was absent initially (1.0.62, scratch homes, fake API key and a local model stub). A model
change also left it unchanged. Explicit `/theme tokyonight` and `/theme grokday` saved those names even with
`GROK_THEME=grokday`; selecting the environment's own theme is still an explicit save. So a saved `[ui] theme` is
always the user's choice, and Octoboard carries it without filtering out the injected name (see "Grok itself rewrites
the per-session `config.toml` copy").

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

The effort is on record as well: the session directory's `summary.json` carried `reasoning_effort` in each of the 114
summaries on the account measured (installed build 1.0.62 on 2026-10-20; a summary names no version), including
sessions that ran a level other than the `config.toml` `[models] default_reasoning_effort`. A catalogue entry's
`reasoning_effort` and the `default: true` level of its `reasoning_efforts` name the model's default; Grok's own
configuration reference says `default_reasoning_effort` applies to "the default model", and whether it also applies to
a model chosen with `--model` was not measured.

**Hooks that never come, and screens after an error** (1.0.67, scratch `GROK_HOME`, fake key, local stub). On a launch
and on `--resume <id>` with no prompt, `SessionStart` (with `source` `load` on a resume) arrives 0.5–1.0 s after the
start, and right after the folder-trust confirmation is accepted. With `allow_managed_hooks_only = true` in Grok's
requirements no hook of Octoboard's runs at all, so nothing marks the session started; a write before `SessionStart`
was buffered and submitted. After a usage-limit error (the stub answering HTTP 402) Grok fires `Notification` with
type `agent_error` and `StopFailure`, then draws "You hit your weekly limit." with "1 (○) Upgrade tier", "2 (○) Buy
more credits", "3 (○) Try Again" and "↑/↓ navigate · y copy Enter :submit / Shift+x :dismiss": a paste and CR closed it
with no `UserPromptSubmit` and opened the browser on a purchase page. The modal is on screen before those hooks reach
a hook command. "3" (or the arrows and Enter) retries, firing `UserPromptSubmit` with the original prompt; Shift+x
dismisses it with no hook and leaves the input free. An HTTP 429 shows "Rate limited | Retrying (attempt 1)…" and
"Queued · Enter to send now", and a paste there was queued, not lost; Grok retries once, honouring `retry-after`, and
when that retry is refused too it fires `agent_error` and `StopFailure` (`error` `rate_limit`) with an inline "Retry
failed" line, no modal, and submits a queued prompt by itself. An HTTP 400 fires the same two hooks (`error`
`invalid_request`, as for the 402) with an inline line and no modal; a single HTTP 500 is retried with no hook.
Octoboard lets a Grok session's writes through 30 s after its launch when no hook has come and no trust screen has been
sighted (one that has is held until the rendered screen no longer shows it), holds them for two seconds after a
`StopFailure` or an `agent_error`, and holds them while the usage-limit modal is on screen.

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

**What the user said, in an agent's own record**, beyond the user lines one would expect (observed on Claude Code
2.1.295–2.1.302, Codex 0.162.1 and Grok Build 1.0.62):

- Claude Code — a message typed while the model is working is recorded only as an `attachment` line with
  `attachment.type` `queued_command` and `attachment.origin.kind` `human`, the text in `attachment.prompt` (a string or
  a list of text blocks); no `type: "user"` line carries it. Conversely, `type: "user"` lines whose `origin.kind` is
  present and not `human` are not the user's: `task-notification` (a background task's or subagent's completion,
  text opening with `<task-notification>`) and `peer` (a message from another session).
- Compaction — Claude Code and Codex keep the earlier messages in the same record. Grok Build rewrites
  `chat_history.jsonl`: the first `<user_query>` stays, most of the earlier conversation moves to
  `compaction/segment_NNN.md` beside it (indexed by `compaction/INDEX.md`), and the summary is a `user` line with
  `synthetic_reason` `compaction_meta` opening "This session is being continued from a previous conversation"; another
  `compaction_meta` line carries the user info and rules.

**Subagent events** carry join traps that make them unusable as if they were the session's own. Octoboard drops them
from the session's status, and registers Claude Code's `SubagentStart` and `SubagentStop` only for the background-task
list:

- Claude Code — `SubagentStop` also fires for internal helper agents, which are the ones with an empty `agent_type`.
  Measured on 2.1.303:
  - The parent session's hooks receive every subagent event at any depth, all under the main `session_id`, each carrying
    `agent_id`. The internal helper that summarises a subagent's progress fires `PreToolUse` and `PostToolBatch` with an
    `agent_id` but **no `agent_type` key at all**, so `agent_type` alone does not tell them from the session's own
    events; a non-empty `agent_id` does.
  - A `SubagentStop`'s `background_tasks` is the whole session's registry, as a `Stop`'s is: every running shell,
    Monitor (type `"shell"`) and subagent (type `"subagent"`, with `agent_type`) at any depth, with no owner field (an
    MCP task listed in a `SubagentStop` was not measured). It always lists the stopping subagent itself as running. A
    subagent paused to wait on its own background work is omitted from the list (its task is reported `completed`).
  - `SubagentStart` fires again with the same `agent_id` on every resume, and carries no description. A subagent's
    `<task-id>` in a `<task-notification>` is its `agent_id`, which is its `background_tasks[].id`.
  - While the main session is idle, each wake of a subagent runs `SubagentStart` → its tool events → `SubagentStop`, and
    only when the subagent's notification is routed to the main session a `<task-notification>` `UserPromptSubmit` and a
    main `Stop` follow. In the TUI, a subagent that stops while one of its own children is still running sends the main
    session no notification until later. Nothing structured tells a notification of a subagent that paused from one that
    finished; the `finished` flag in the subagent's `agent-<id>.meta.json` toggles with each turn, so it does not
    either.
  - A subagent handing over to the next one can fire `SubagentStop(B)` listing only B and `SubagentStart(A)` in the same
    instant, so the list is momentarily empty.
- Codex — `transcript_path` is the *subagent's* file on `SubagentStart` but the *parent's* on `SubagentStop`, where the
  subagent's own is in `agent_transcript_path`; `session_id` is the parent's while `turn_id` is the subagent's own, so
  the pair can be joined to the parent only by `agent_id`; and the subagent thread emits no `SessionStart` or `Stop` of
  its own.
- Grok — `SubagentStart` fires in the parent and `SubagentStop` in the child, where `sessionId` equals `subagentId`;
  the resolved `subagentType` is not the one the caller asked for, so a matcher written against a requested name never
  matches; and a parent's `Stop` is no promise that its subagents have finished.

## Moving a conversation to another config directory

All three agents resume a conversation under a second config directory when **the session's own record** is copied
into it — for Codex, every rollout file of the thread, since a thread that was ever reverted is a chain of them (see
"Codex, history segments" below). No index, registry or per-project entry has to travel with it, although all three
keep one. Each result carried a control in the same run.

| Agent | What to copy | Where it goes |
|---|---|---|
| Claude Code | `<session id>.jsonl` | `<config dir>/projects/<cwd slug>/` |
| Codex | `rollout-<timestamp>-<thread id>.jsonl` and every later segment `rollout-<timestamp>-<thread id>_<segment id>.jsonl`, each as `….jsonl.zst` once Codex has compressed it | anywhere under `<home>/sessions/` |
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
- **Codex, a rollout it has compressed** — with the feature `local_thread_store_compression` on (off by default in
  0.162.1; see "Codex's compression is per-home maintenance" below), Codex replaces an old rollout with
  `rollout-…-<thread id>.jsonl.zst` in the same directory (zstd). On the measured machine 286 rollouts were
  compressed, none of them with a file date in the last week; "about a week" comes from those dates, not from a
  measured threshold. Its database does **not** follow: `threads.rollout_path` in `state_5.sqlite` still names the
  `.jsonl` path that no longer exists (no row of 1,212 ends in `.zst`; read from a copy of the real database). A
  compressed rollout copied alone, byte for byte, into a fresh home with no database resumed with the conversation
  intact (`codex exec resume <thread id>`, 0.162.1, rollout written by 0.160.0; the session id echoed was the
  original's). Codex **decompresses it on resume**: afterwards the target home held a plain `.jsonl` at the same
  place, the `.zst` was gone, and the new database's `rollout_path` named that `.jsonl` inside the target home. So the
  database path is no obstacle to a copy, and the target's rollout is a plain file again until it ages out once more.
- **Codex, history segments** (0.162.1) — `rollout-<start>-<thread id>_<segment id>.jsonl` (or `.zst`) is **not a
  subagent's rollout but a later segment of the same thread**. An earlier reading of this section took it for a
  subagent's; every such file on the measured machine (26, written by 0.159.2–0.162.1) has a `session_meta` carrying the
  thread's own id, and its lines carry only that thread id. The `session_meta` has `history_mode: "paginated"` and
  `history_base: {thread_id, end_ordinal_exclusive, end_byte_offset}`, where `thread_id` names the file the segment
  continues from — the thread's first rollout by the thread id, a later segment by its segment id — and the segment's
  ordinals pick up at that cut. So the files form a chain, its last segment (the chain's leaf) is the conversation's
  head, and the database's `rollout_path` names the head (read from a copy of the real database). Reverting a turn
  starts a segment: through `codex app-server`, a thread with two turns, `thread/revert` before the second, then a third
  turn, left the first rollout (ordinals 0–25) plus a segment (ordinals 15–28, based on the first at ordinal 15). A
  segment can also start a fresh history instead (`history_base` null, ordinals from 0; seen on disk, trigger not
  measured). Resumed with `codex exec resume <thread id>` in fresh homes, asking for the codewords given so far:
  - every file of the chain present, no database row → the head, and the right conversation (first and third turns);
    the same with the chain compressed, where only the head was decompressed and the first rollout read in place;
  - the first rollout alone → that file, **the reverted turn back and the third turn gone, with no error or warning**;
  - the head segment alone → refused: `thread/resume failed: invalid paginated history lineage for <id>: missing source
    rollout`.

  So a resume that matches the newest file containing the id picks the head, which is correct. What makes a file the
  newest was not settled. In the fork described under "Which rollout a home resumes, over a shared directory" below,
  a home with no row picked the segment with the later timestamp in its name. That segment was also the chain's leaf,
  while the other segment had the later mtime. So mtime does not decide it; name order or lineage might.
  **Octoboard's switch carries the whole chain**: the head as the record and every other file of the thread as a
  companion, through the same staged copy. Measured with 0.162.1 on a thread made through `codex app-server` (two
  turns, `thread/revert` of the second, a third turn, `thread/revert` of the third, a fourth: a first rollout and two
  segments), the chain copied by that code into a fresh home, plain and compressed, resumed with the surviving first
  and fourth turns and neither reverted one; the new database row named the head (a compressed head came back as a
  plain `.jsonl`, the other two files stayed compressed). Octoboard reads the head for a thread's context size: the
  last `token_count` record with a non-null `info` gives `last_token_usage.total_tokens` and `model_context_window`.
  The cumulative total in a segment continues from the segment before it (in a real thread the first rollout ended on
  964,390 and its segment began at 1,036,910), which is why the last request's figure is read and not the total. A
  head segment has no `token_count` until its next turn after a revert, so until then the last one in the file it
  continues, before that segment's `history_base.end_byte_offset`, stands in, or failing that the nearest earlier file's. A segment holds only the lines after the
  last cut, so the opening prompt comes from the first rollout, and the conversation as it stands is the head preceded
  by each file it continues up to its `history_base.end_byte_offset` (decompressed bytes; in a real thread the offset
  fell on the line boundary before the next segment's first line). Subagents are separate
  threads: a spawned agent's rollout is `rollout-<child start>-<child thread id>.jsonl`, with
  `source.subagent.thread_spawn.parent_thread_id` in its `session_meta`, so its name never contains the parent's id. A
  home with no row resuming the parent with two such rollouts beside it opened the parent with the conversation intact
  (and took rows for both children). An earlier reading of this section took the `_<segment id>` files for subagents'
  and had the switch leave them behind, which resumed a reverted thread at a stale branch.
  **A target whose own row names an older head resumes that head, with the whole chain copied.** Measured with 0.162.1:
  a home holding the first rollout and the first segment was resumed once, so its row named that segment; the whole
  chain was then copied in over it (the third file added, the other two replaced) and the thread resumed again. It
  came up on the older branch (the first and third turns; the third had been reverted since, and the fourth was
  absent), with no error, and the row still named the older segment. So a target that held the thread before it was
  reverted elsewhere has to have that row dealt with, not only the files.
  **The row decides, and either removing it or repointing it fixes the resume** (0.162.1, same setup, with a
  `thread_attachments` row and a `thread_dynamic_tools` row seeded for the thread, the database edited with `sqlite3`
  while Codex was not running):
  - Deleting the thread's `threads` row cascades into those two tables when foreign keys are on (both are declared
    `ON DELETE CASCADE`; the `sqlite3` shell has them off by default, and Codex's own connections turn them on). With foreign keys off the children stayed, Codex's own insert of the fresh row succeeded with no foreign-key
    complaint, and the children were attached to it again. The resume answered "ALPHA, DELTA". The fresh row is built
    from the rollouts, so the old row's title, pin, section and project were gone; its title and preview now came
    from the head.
  - Updating only `rollout_path` to the head's path (the whole chain in place) also answered "ALPHA, DELTA", and
    kept the row's other columns and its children untouched. The row's `title`, `preview` and `first_user_message`
    stayed those of the older branch and were not refreshed by the resume's turn. With the chain compressed, a path
    naming the `.jsonl` that no longer exists and one naming the `.jsonl.zst` both resumed the head.
  - Clearing only the home's `thread_history_1.sqlite` rows (the `threads` row left standing) still resumed the older
    branch, and deleting them along with the `threads` row changed nothing: its tables (`thread_turns`, `thread_items`,
    `thread_history_projection_state`) are keyed by the id of a rollout *file*, one set for the first segment and one
    for the head here, not by the thread's, and Codex rebuilt them.
  - Other tables keyed by thread id: `thread_spawn_edges`, `thread_goals` (in `goals_1.sqlite`) and the queue tables
    were empty in the measured home and were not exercised; `thread_sections` is not per thread.
  - **Octoboard's switch updates `rollout_path` only**, in every `state_<n>.sqlite` of the target's database
    directory that has a `threads` table, to the head's path in the target without `.zst`, the form Codex keeps for a
    compressed rollout (no row of 1,212 in a real database ended in `.zst`). It changes nothing else, so nothing
    cascades and the user's title, pin and links stay. The cost is that the older branch's title and preview stay on
    the row. A home with no row is left alone, since it resumes the head by name.
  - **Codex can keep its databases outside the home** (0.162.1): `sqlite_home` in `config.toml` and the
    `CODEX_SQLITE_HOME` environment variable both moved the whole set of `*.sqlite` files, and with both set the
    configuration won. A relative `sqlite_home` (`"db"`, `"../up"`) was taken against the home, and a leading `~/`
    was expanded with the process's `HOME`. A relative `CODEX_SQLITE_HOME` was taken against the directory Codex
    was started in, not the home. The binary also names an enterprise requirement for
    `sqlite_home` that overrides the environment variable; that was not looked for. The switch reads the target's
    `config.toml` and the environment the relaunch will see (the home's `~` is that environment's `HOME`), and fails
    with `relocation_failed`, rather than update a database it has guessed, when the configuration cannot be read or
    parsed, `CODEX_SQLITE_HOME` is relative (the starting directory is not known there), or the directory cannot be
    listed; a directory that does not exist means no database.
  - **A managed daemon holding the thread was not measured.** In a scratch home every 0.162.1 TUI start launched
    `<home>/packages/app-server-daemon/…/codex app-server --managed-daemon` (parent pid 1), which outlived the TUI and
    copied about 323 MB into the home's `packages/` (measured 2026-10-20). Whether a daemon in the target holding the
    thread loaded would ignore a changed row is unverified.
- **Claude Code, beside the transcript** — `projects/<slug>/<session id>/` holds `subagents/`, `tool-results/` and
  `custom-title.json`, and `file-history/<session id>/` holds the rewind checkpoints (`<hash>@v<n>`). Both are layout
  as found on disk; a resume with the transcript alone was measured above, and what a resume without these loses
  (rewind, subagent transcripts) was not.
- **Grok Build** — the session directory copied under a different working directory's encoded name resumed with the
  conversation intact; `sessions/session_search.sqlite` is not consulted. **An unauthenticated home does not refuse**:
  measured with Grok Build 1.0.46, `grok --resume <id>` against a home with no login stays up on a browser device-code
  sign-in (`Approve in your browser to finish signing in`, then `Waiting for approval`) instead of exiting. So, as with
  Claude Code and Codex, a switch to an account that is not signed in comes up and counts as a success, and the
  relaunched session can start the agent's own sign-in flow. Whether the session is looked up before or after that
  sign-in was not measured, so a relocation cannot be verified without a login. For a session it does not hold
  locally, Grok prints `Session "<id>" not found locally, restoring conversation from remote...`. Its remote registry
  has never been seen to restore one: every attempt so far answered 404 (see "Grok Build over a linked `sessions/`"
  below). Whether the registry would serve a session to a different account is unmeasured.
- **Under a different account** — a conversation copied into a config directory signed in to another account:
  - **Codex resumes it, encrypted reasoning included** (0.162.1). The two homes were signed in by copies of the
    `auth.json` of two genuinely different ChatGPT accounts: a personal Plus account and a Business workspace, with
    different account and organisation ids. The thread was made under A on `gpt-6.1-sol` at medium effort, with a turn
    whose rollout holds a `reasoning` item carrying `encrypted_content`. Copied into B's home and resumed on the same
    model, it ran a turn and recalled the earlier conversation. The response's per-input-item usage in the trace log
    (`RUST_LOG=trace`) listed that reasoning item's id, so it was sent and accepted. Control in the same run: the same
    rollout with eight characters of the `encrypted_content` changed was refused with `invalid_encrypted_content`
    ("The encrypted content for item rs_… could not be verified. Reason: Encrypted content could not be decrypted or
    parsed."). So the server verifies an earlier turn's reasoning, and it accepted the other account's. The reverse
    direction (made under B, resumed under A) was the same. A resume on a smaller model (`gpt-6-luna`) worked too, with
    a warning that the session was recorded with another model.
  - **Claude Code does not resend an earlier turn's thinking, so its signature is no obstacle; the resume under a
    second account itself was not measured** (2.1.302). Two scratch config directories were each signed in by the
    user, to a personal Max account (A) and to a Team account (B). Under A, `claude -p --model haiku`
    (`claude-haiku-5-5`) wrote a transcript whose assistant turn holds a `thinking` block with a 1,580-character
    `signature`. Two copies were resumed under A with `--resume`, one as written and one with eight characters of the
    signature changed. Both resumed and recalled the conversation; the tampered signature was not refused. The
    transcript's own request log explains why: Claude Code 2.1.302 writes `api-request-blob` and `api-request` lines
    into the transcript, and in the resumed request the earlier assistant turn was sent as its text alone, without the
    thinking block. The request asked for `thinking: {type: "adaptive", display: "omitted"}`, with context management
    `clear_thinking_20251015`, `keep: "all"`. The resume under B could not be run because every request B made was
    answered "You've hit your weekly limit"; the signature finding makes a refusal on signatures unlikely.
  - **Grok Build:** not measured; the machine has one Grok Build account.

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
Codex and Grok Build keep the login in a file inside the directory (`auth.json` in both). For Codex that is measured: a
copy of an account's `auth.json` in an otherwise fresh home works as that account (0.162.1). For Grok Build it is
inferred from the layout. A fresh Grok home is signed in without a terminal by `GROK_HOME=<dir> grok login` (a browser
device code; plain `grok` needs a TTY and fails with `Device not configured` without one), and one turn of
`grok -p "<prompt>"` then creates its `sessions/`, which is what makes it an initialized Grok home (measured with Grok
Build 1.0.46).

Claude Code was measured to create a config directory that does not exist yet. Codex 0.161.0 does not: it exits with
`CODEX_HOME points to "<dir>", but that path does not exist`, so the daemon creates a pinned Codex directory itself
before launching. Which commands create it, and what each one answers before a login, is under "Signing in a config
directory without a terminal" below.

**The trap a verification has to rule out** is an agent that resumes *successfully* into an empty conversation. None of
the three did that here, each refusing distinctly, but a future version could introduce it and it would look like a
successful switch.

## Signing in a config directory without a terminal

Measured on 2026-10-19 with **Claude Code 2.1.303**, **Codex 0.162.1** and **Grok Build 1.0.62**, each against scratch
config directories that either did not exist or had just been created empty. Each command ran the way the daemon runs
an account command: the agent's directory variable set, the working directory the home directory, stdin `/dev/null` or
a pipe, stdout and stderr piped, no terminal. No login was meant to be completed. Each was started, its output read,
and the process killed; one Grok Build login completed on its own (see below).

**All three sign in without a terminal**, and none needs anything written to its stdin:

| | Claude Code | Codex | Grok Build |
|---|---|---|---|
| Command | `claude auth login` | `codex app-server` with `account/login/start`, or `codex login [--device-auth]` | `grok login` |
| Flow | OAuth with a loopback callback, plus a manual URL whose code is pasted back | OAuth with a loopback callback, or a device code | device code |
| Where the URL is | handed to `$BROWSER`; a second, manual URL on stdout | the JSON-RPC reply; the CLI prints it | stderr |
| Opens a browser itself | through `$BROWSER`, else `open` on `PATH`, so it can be replaced | `app-server`: no. `codex login`: yes, nothing found to stop it. `--device-auth`: no | yes, nothing found to stop it |
| A directory that does not exist | created | refused, exit 1 | created |
| Done | exit 0, `Login successful.` (from the executable) | `account/login/completed` with `success: true` (from the schema) | exit 0, `✓ Signed in as <email>` on stderr |

- **Claude Code** (`claude auth login`; `--console` and `--sso` were not tried) prints on stdout `Opening browser to sign
  in…`, then `If the browser didn't open, visit: <url>`, then `Paste code here if prompted > ` with no newline. It
  listens on `127.0.0.1` at an ephemeral port (53228 in one run) and hands the browser a URL of its own, `https://claude.com/cai/oauth/authorize?code=true&client_id=…&response_type=code&redirect_uri=http%3A%2F%2Flocalhost%3A<port>%2Fcallback&scope=…&code_challenge=…&code_challenge_method=S256&state=…`.
  The browser is run as `$BROWSER <url>` when `BROWSER` is set, and otherwise as `open <url>` looked up on `PATH`. Both
  were replaced by scripts that only recorded their argument, and no LaunchServices launch came from the process. So
  the URL Octoboard should open is the one handed to `BROWSER`, captured by a `BROWSER` that reports it back, and the
  loopback callback then finishes the login with nothing pasted. The URL on stdout is a different one: its
  `redirect_uri` is `https://platform.claude.com/oauth/code/callback`, the page that shows the user a code to paste.
  The paste does work without a terminal: `notarealcode#notarealstate` written to a piped stdin was taken and sent,
  and the command answered `Login failed: Request failed with status code 400` on stderr and exited 1 within a second.
  An empty stdin (`/dev/null`, or a pipe held open) does not end the wait. SIGTERM ends it with exit 143. It created a
  config directory that did not exist (`.claude.json` and `backups/`) before any sign-in. On success the credential
  goes to the Keychain item named under "A second config directory is a second login" above, and the account record
  to `.claude.json`. Both, and the `Login successful.` line, come from the executable and the earlier
  measurements, not from a completed login here. `claude auth status` reporting `loggedIn: true` afterwards is the
  check to make.
- **Codex** has three ways in, and the app server is the one made for a front end. `codex app-server` (the same command
  and flags as the quota probe) takes `account/login/start` after `initialize`:
  - `{"type":"chatgpt"}` replies at once with `{"type":"chatgpt","loginId":"<uuid>","authUrl":"https://auth.openai.com/oauth/authorize?…&redirect_uri=http%3A%2F%2F127.0.0.1%3A1455%2Fauth%2Fcallback&…"}`
    and listens on `127.0.0.1:1455`. It opens no browser: no LaunchServices launch came from the process, and nothing
    was run through `BROWSER` or `open`. With 1455 already held by another process, the reply took about four seconds
    and named port 1457, where it listened instead. Whether the authorization server accepts that port was not
    tried, so two logins at once are a risk.
  - `{"type":"chatgptDeviceCode"}` replies with `{"type":"chatgptDeviceCode","loginId":…,"verificationUrl":"https://auth.openai.com/codex/device","userCode":"<4 characters>-<5 characters>"}`,
    with no listener and no browser.
  - `account/login/cancel` with the `loginId` answers `{"status":"canceled"}`, followed by the notification
    `account/login/completed` with `{"loginId":…,"success":false,"error":"Login was not completed"}` (`"Login server
    error: Login was not completed"` for the browser flow). The schema
    (`codex app-server generate-json-schema`, `v2/AccountLoginCompletedNotification.json`) has the same notification
    carry `success: true` on a completed login, and `account/updated` follows a change of account. That is the signal
    to wait for, then `account/read` to confirm; a completed login was not observed.

  The CLI does the same from a shell. `codex login` prints on stderr `Starting local login server on
  http://localhost:1455.`, `If your browser did not open, navigate to this URL to authenticate:`, the URL (redirect
  `http://127.0.0.1:1455/auth/callback`) and ``On a remote or headless machine? Use `codex login --device-auth`
  instead.`` It opened the browser itself through LaunchServices, with `BROWSER` set and a recording `open` first on
  `PATH`, so it would put up a page Octoboard did not ask for. `codex login --device-auth` prints on **stdout** the
  link `https://auth.openai.com/codex/device`, `Enter this one-time code (expires in 15 minutes)` and the code, with ANSI
  colour escapes even into a pipe, and opens nothing and listens on nothing. Both died on SIGTERM. Neither creates a
  missing `CODEX_HOME`: both exit 1 with `Error loading configuration: CODEX_HOME points to "<dir>", but that path does
  not exist`. On success Codex writes `auth.json` into the home. That rests on the earlier measurement of a copied
  `auth.json` under "A second config directory is a second login" above. Whether the `cli_auth_credentials_store`
  setting could send it elsewhere was not examined.
- **Grok Build** (`grok login`, with no flag) printed on **stderr** `To sign in, open this URL in your browser:`,
  `https://accounts.x.ai/oauth2/device?user_code=<4 characters>-<4 characters>`, `Confirm this code in your browser:`,
  the code, and `Waiting for authorization...`, about a second after it started. Its debug log says the device flow was
  chosen by a login configuration fetched from the server (`resolved interactive transport transport="device"
  source=remote`), so the default could change without an upgrade; `--device-auth` asks for it explicitly, and
  `--oauth` was not tried. It listens on nothing. **It opens the browser itself through LaunchServices** (the system
  log showed `LAUNCH: Asking CSUI to launch 1 items` from the process in both runs it was watched in), with `BROWSER`
  set and a recording `open` first on `PATH`; `GROK_TEST_OPEN_URL_FILE`, a name in the executable, did not stop it either. So Octoboard
  cannot be the only one to open the page. It created a `GROK_HOME` that did not exist (`agent_id`, `docs/`, `logs/`).
  In the first run, nothing on the machine knowingly approved the code. The login still completed 5.4 seconds after the
  URL appeared: the browser it opened was already signed in to x.ai, and whether a person clicked through is not
  known. It wrote `auth.json` into the scratch home, printed `✓ Signed in as <email>` on stderr and exited 0. So
  completion is exit 0, with `auth.json` present in `GROK_HOME`. Killed while waiting, it died on SIGTERM.

  A plain `grok` run (no `login` subcommand) against a home with no login does the same: it starts the grok.com
  sign-in inline (`auth: inline auth flow` in `$GROK_HOME/logs/unified.jsonl`), and on a machine whose browser was
  signed in it wrote a real `auth.json` into a scratch home within seconds (Grok Build 1.0.62, 2026-10-20). Another
  run, with a scratch `HOME` too and `auth.json` holding `{}`, still authenticated from a session token found outside
  both directories (`auth: cached_token handler set api_key (SessionToken)`); where that token is kept was not found.
  With `GROK_CODE_XAI_API_KEY` set, Grok selected `xai.api_key`, logged `has_cached_token:false` and completed no
  sign-in.

**How long each waits**, left running with nobody to authorize:

- **Claude Code** was still waiting, its loopback port still listening, when it was killed after 610 seconds. So it
  has no timeout of ten minutes or less, and the daemon has to set its own.
- **Codex's app server** gave up the browser flow at 600 seconds with `account/login/completed`
  `{"success":false,"error":"Login timed out"}`. The process stayed up, and an `account/login/cancel` afterwards
  answered `{"status":"notFound"}`. The device-code flow says its code expires in 15 minutes; neither it nor
  `codex login` was left running to the end.
- **Grok Build** was not left waiting, as every run opened a browser page. How long it waits is unknown.

**Before a login, on the quota path.** The same probes as in "Plan quota" below, against a directory that does not
exist and one created empty:

- **Claude Code does not tell the two apart, and recreates a missing directory.** Both the probe and `claude auth status`
  create the directory (`.claude.json`, `backups/`, and for the probe `plugins/` and `sessions/`) and then answer as
  for an empty one. The probe's `get_usage` succeeds, with
  `{"subscription_type":null,"rate_limits_available":false,"rate_limits":null,"behaviors":null,"session":{…}}`. That
  is presumably what an account without a subscription answers as well (not measured), so it does not say "not signed
  in" alone. The `initialize` reply in the same exchange does: its `account` is `{"tokenSource":"none","apiProvider":"firstParty"}`.
  Read from the executable, not measured against a signed-in directory: a subscription login replaces `tokenSource`
  with `subscriptionType` (with `email` and `organization`), and an API key adds `apiKeySource`. `claude auth status` (JSON by
  default) prints on stdout `{"loggedIn":false,"authMethod":"none","apiProvider":"firstParty",…,"configDirectory":"<dir>"}`
  and exits **1**; `--text` prints `Not logged in. Run claude auth login to authenticate.`, exit 1. The probe and
  `claude auth status` each answered in under half a second. The probe also writes `Ignoring <n> permissions.allow entries from
  .claude/settings.json: this workspace has not been trusted…` to stderr, because the home directory it runs in holds
  `~/.claude/settings.json` as a project file and the fresh directory has no trust record for it. That is noise, not
  an error.
- **Codex** keeps the two distinct. With the directory missing, `codex app-server` writes nothing on stdout and exits
  1 within a tenth of a second, its stderr ending in `Error:` and `CODEX_HOME points to "<dir>", but that path does
  not exist`, after a `WARNING: proceeding, even though we could not create PATH aliases: …` line naming the same
  path. It does not create the directory. Empty, it starts, and `account/rateLimits/read` answers `{"error":{"code":-32600,"message":"codex account authentication required to read
  rate limits"},"id":2}`. The code alone is JSON-RPC's generic invalid request, so it is the message that marks the
  case. `account/read` (params `{}`) is the structured answer:
  `{"result":{"account":null,"requiresOpenaiAuth":true,"workspaceRouting":null}}`, where a signed-in home has
  `account` set to `{"type":"chatgpt","email","planType"}` or `{"type":"apiKey"}` (per the schema). `codex login status` prints
  `Not logged in` on stderr and exits 1 for the empty home, and `Error loading configuration: CODEX_HOME points to …`
  with exit 1 for the missing one. The empty home is not left empty: the app server writes its databases, `skills/`
  and `installation_id` into it.
- **Grok Build does not tell the two apart, and creates a missing directory.** `grok agent --no-leader stdio` creates it
  (`config.toml`, `README.md`, `docs/`, `logs/`, `sessions/` and lock files; on 1.0.62 also `installed-plugins/`, and on
  a home that already has a `config.toml` it appends `[marketplace] default_skills_installs_purged = true`, writing
  through a symlink) and then answers `_x.ai/billing` for either with
  `{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"Authentication required","data":"Authentication required to
  fetch billing data"}}`. The `initialize` reply in a home with no login has `agentCapabilities.auth` `{}`,
  `authMethods` `[{"id":"grok.com","name":"Grok","description":"Sign in with Grok"}]` and `_meta.defaultAuthMethodId`
  `null`. What those fields hold once signed in was not measured, so they are not yet a signal. A home whose stored
  sign-in has expired (grok 1.0.62) answers `_x.ai/billing` with a generic error and the cause only in `data`:
  `{"jsonrpc":"2.0","id":2,"error":{"code":-32603,"message":"Internal error","data":"Billing service error: Invalid or
  expired credentials (auth_kind=bearer, x_xai_token_auth=xai-grok-cli, upstream=PermissionDenied, reason=no auth
  context)"}}`.

So "the directory is missing" can only be told reliably by looking for the directory before the probe runs. Codex is the
only one that refuses a missing directory; Claude Code and Grok Build quietly recreate it, empty, and then report it as
not signed in. "Not signed in" has a stable marker for each: Claude Code's `account.tokenSource` of `"none"` in the
`initialize` reply, or `claude auth status` exiting 1 with `loggedIn: false`; Codex's `account/read` with `account:
null`, or the `authentication required` message; Grok Build's `-32000` `Authentication required` error, or for expired
credentials the `-32603` error whose `data` says `Invalid or expired credentials`. Each of these markers comes from one
build, so a reply that matches none of them should still be shown as a plain failure.

## Sharing a session directory between accounts

Octoboard does not link an account's session directory into another account's on its own: it does so only as part of
an account sharing the whole of the default account's configuration, conversation databases included (see
`docs/product/sharing-the-default-configuration.md`). This section records the measurements of a session directory
shared without the rest; linking the rest of a configuration directory, and what changes once the databases are linked
too, is under "Sharing a configuration directory between accounts" below. Codex and Claude Code lose or split conversations in a shared directory, in ways they give no
warning of, and Grok Build has no guard against two writers:

- **Codex.** One home's rollout compression unlinks a rollout that another home holds open, and the turns that home
  writes afterwards are lost. A home whose database row names an older history segment resumes a stale branch and
  writes to it. Two homes running one thread both append to the same rollout, with duplicate ordinals.
- **Claude Code.** The account with the shortest `cleanupPeriodDays` deletes every account's old transcripts through
  the link. Two config directories resuming one conversation at once fork the transcript, and a later resume follows
  one branch only.
- **Grok Build.** Nothing stops two homes, or two processes in one home, from writing the same session.

Octoboard's own per-session Grok Build homes are a different arrangement: each links `sessions/` to the home of the same
account. Even so, a terminal `grok` under the same account can still resume a session that an Octoboard session is
running, and nothing stops it. Resuming a conversation under another account is a separate question, covered under
"Under a different account" in "Moving a conversation to another config directory" above.

Measured on 2026-10-19 with **Codex 0.162.1**, **Claude Code 2.1.302** and **Grok Build 1.0.62**, the installs the
login shell resolves, in scratch config directories holding copies. For Codex: two homes, A and B, signed in to the
two different accounts described under "Under a different account", with B's `sessions/` a symbolic link to A's. Every
conversation was checked by asking it what it had been told, together with the rollout path the home recorded in its
database.

**Codex's compression is per-home maintenance, blind to another home's writers.**

- **What triggers it.** It runs only with the feature `local_thread_store_compression` on. That feature is listed as
  under development and off by default in 0.162.1, and in the 0.163.0-alpha.10.2 bundled with the ChatGPT desktop
  app; how the default home on the measured machine came to have compressed rollouts was not traced. With the feature
  on, every Codex start runs a "rollout compression worker" over the home's `sessions/`. That includes `codex exec`,
  even one that fails at once, such as resuming an unknown id. It compresses rollouts whose mtime is older than a
  threshold: a file dated 2026-10-01 was compressed, files from the same day were not; the threshold itself was not
  measured. It runs at most once per cooldown, recorded in `<home>/.tmp/rollout-compression.lock` (a pid and a start
  time); deleting that file re-arms it. That lock lives in the home, so two homes sharing `sessions/` each run their
  own worker over it.
- **The guard is per home.** Within one home, a rollout with a turn in progress was skipped (`skipped=1`) even with its
  mtime forced old; the worker respects the home's own writer lock, `<home>/thread-writer-locks/<thread id>.lock`.
  **Across homes it is not guarded, and a turn is lost**: B had a turn in progress (a shell `sleep 45`) on a rollout in
  the shared directory, its mtime was forced old, and A started. A compressed the rollout (`compressed=1`). B finished
  the turn and printed its answer, but the shared record ended at B's prompt: the tool call, its output and the answer
  were written into the unlinked file. The same happened with B's TUI idle on the thread (resumed, no turn) while A
  compressed: B's next turn showed on B's screen and is nowhere in the record. B's process keeps writing to the file it
  opened; only a later resume finds the path again (decompressing it) and records from there, so the turns in between
  are gone.
- **When that can happen for real.** Only a rollout older than the threshold is compressed, and a resume, even one
  with no turn, rewrites the head rollout with a fresh mtime. So a loss needs a process in one home to keep a thread
  open past the threshold while another home with the feature on starts Codex. Keeping a thread open is not only a
  visible session's doing: the 0.162.1 TUI runs its threads in a per-home daemon
  (`<home>/packages/app-server-daemon/…/codex app-server --listen unix:// --managed-daemon`). It was still up 17
  minutes after the TUI exited, holding the thread's writer lock and the open rollout. While it held them, another
  resume of the thread from the same home was refused with `thread <id> already has an active writer`. Whether an
  Octoboard session's Codex goes through the same daemon was not checked.
- **Each home's database after the other's maintenance.** Compression changes no database. After compressing its own
  rollout, A's row still named the `.jsonl`, and A resumed the compressed rollout through that row all the same. B,
  with no row, resumed the rollout A had compressed, decompressed it in the shared directory, and recorded the
  link-resolved path. A then resumed it and saw B's turn. Each home also keeps its own projection of a rollout in
  `thread_history_1.sqlite` (`thread_history_projection_state`: `next_rollout_byte_offset`, `next_rollout_ordinal`),
  which falls behind whatever the other home appends. Resumes still worked. A home's revert failed once, with
  `turn <id> does not have a persisted start boundary`, after the file had changed under its projection; that was
  provoked by restoring an older copy of the file, which sharing does not do by itself.

**Which rollout a home resumes, over a shared directory.** A home with no row resumes the right conversation through the
link, as described under "Codex, history segments" above. Measured: a home whose row named the head as a `.jsonl` while
the chain was compressed resumed the head, with the right conversation. **Sharing adds one arrangement: a home whose row
names an older head resumes the older branch.** A made a thread with one revert, so A's row named segment S1. B, with no
row, resumed it through the link, took two turns, reverted the second and took a third, which started segment S2 on top
of S1. A then resumed the thread and followed its row to S1: B's reverted turn came back, B's third turn was missing,
and A appended its own turn to S1. Afterwards B (row on S2) and a fresh home with no row both opened S2, without A's
turn. So one home's revert in a shared directory splits the conversation into branches, and which branch a home sees
depends on its own row.

**Two homes can run the same Codex thread at once, and both write to one file.** A had a turn in progress (`sleep 30`)
when B resumed the same thread from the other home and completed a turn. Neither was refused, since writer locks are
per home. Both appended to the same rollout: B's turn lies inside A's, and ordinals 23–34 occur twice, each writer
numbering on its own. Later resumes from A, from B and from a fresh home all went through and saw both turns. Within
one home the second writer is refused (the `already has an active writer` above).

**One account's `cleanupPeriodDays` prunes every account's conversations in a linked `projects/`** (Claude Code
2.1.302). The setup was a signed-in scratch config directory with `cleanupPeriodDays: 1` and its `projects/` linked to a
shared directory. The shared directory held three copies of real transcripts: two with mtimes a month back, one of them
with a sidecar directory (`<id>/subagents/…`), and one fresh. A `claude -p` turn lasting about 15 seconds deleted both
old transcripts and the old sidecar directory through the link. It kept the fresh copy and the transcript the turn
itself wrote, and wrote `<config dir>/.last-cleanup`. Nothing marks which account wrote a transcript, so the shared
directory is pruned by whichever sharing account has the shortest period (the default is 30 days).

When the sweep runs:

- **During a `-p` turn.** It ran about five seconds into the turn, also on a run that had nothing old enough to delete.
- **Not soon after a TUI starts.** An idle TUI did not run it within 75 seconds. Read out of the executable, not
  measured: housekeeping is put off by ten minutes while the user has been active within the last minute.
- **At most once per `.last-cleanup` interval.** Once the file had been written, a later 14-minute TUI run did not
  sweep again.
- **Not without a login.** In unauthenticated directories it never ran, even after 13 idle minutes at the prompt.

Read out of the executable: the transcript part lists `<config dir>/projects/` with a plain directory read, and
transcripts from a desktop host (Claude Desktop, Cowork) are exempt.

**Two Claude Code directories resuming one conversation at once both write to the transcript, and it forks** (2.1.302).
Both signed-in scratch directories had `projects/` linked to one shared directory. A ran `claude -p --resume <id>` with
a turn that made a tool call. While that turn was in progress, B ran `claude -p --resume <id>` on the same id. Neither
was refused. B saw the transcript ending in A's `tool_result` and closed it with a synthetic assistant entry
(`No response requested.`). B then appended its own prompt and the request's error entry; the request was refused by B's
plan limit, but its lines were written all the same. A carried on and wrote its next assistant entries with `parentUuid`
set to its own `tool_result`. So two branches now hang from the same entry. A later `--resume` from A followed A's
branch: asked for every earlier prompt, it listed A's prompts and not B's. B's prompt is still in the file but out of
the conversation. Which branch a resume follows when both complete a turn was not measured. The per-directory process
registry (`sessions/<pid>.json`) is not under `projects/`, so neither directory can see the other's process.

**Grok Build over a linked `sessions/`, one account in two homes** (Grok Build 1.0.62). Two scratch `GROK_HOME`s, each
signed in by a copy of the same `auth.json`, B's `sessions/` linked to A's. **The account's Grok Build balance was
exhausted**: every turn failed with `402 Payment Required: Grok Build usage balance exhausted`. So these results cover
how a session is found and written, not a completed model turn. A failed turn still records its prompt in
`updates.jsonl` and `chat_history.jsonl`.

- **B resumes A's session from the shared directory.** `grok --resume <id> -p …` from B found the session through the
  link, with no remote lookup, and appended B's prompt to the same session directory. A's TUI, resuming afterwards,
  showed both A's and B's prompts.
- **The remote session registry restored nothing.** A third home with its own empty `sessions/` asked for a session it
  did not hold. Grok printed `Session "<id>" not found locally, restoring conversation from remote...` and then
  `Failed to restore session from remote: fetching session record: session get failed: 404 Not Found`. That happened
  for every id tried:
  - the scratch session;
  - one made with `relay.enabled = true` ("Enable session relay sync" in the config reference);
  - two real sessions of the account from the user's own home, one of them with 74 messages.

  No positive control was found, so this does not show whether a completed session is ever uploaded, or under what
  setting. The registry plays no part over a shared directory, since the local copy is found first.
- **Nothing stops two processes running one session.** A's TUI held the session open, registered in A's
  `active_sessions.json`; that registry is per home. A `grok --resume <id> -p …` from B was not refused, and neither was
  one from A itself. Each went on to its turn and appended its prompt to the shared `updates.jsonl`. How two completed
  turns interleave there was not measured, because of the exhausted balance.
- **Not measured:** whether a session made under one Grok account resumes, or is restored from the registry, under a
  different account. The machine has one Grok Build account.

## Sharing a configuration directory between accounts

Measured on 2026-10-20 on **Claude Code 2.1.304**, **Codex 0.162.1** and **Grok Build 1.0.62**, the installs the login
shell resolves, in scratch config directories (a throwaway `HOME`, the agent's config-directory variable set
explicitly, a local stub as the model endpoint with a fake key, no login). The setup was a default directory D and
account directories whose entries are symbolic links into D. "Executable" below marks what was read out of the
binary and not observed. A link to a file is called a file link, a link to a directory a directory link.

**What an account keeps in its directory divides three ways**, and only the first two are candidates for linking.

- **Claude Code**
  - *Configuration:* `settings.json`, `CLAUDE.md`, `keybindings.json`; `agents/`, `commands/`, `skills/`,
    `output-styles/`, `rules/`, `plugins/`. From the executable only, loading not measured: `workflows/`, `routines/`,
    `themes/`, `hooks/`, `agent-memory/` (user-scope subagent memory), `loop.md`. `settings.local.json` in the config
    directory is not read (measured). `skills/synced/<org>_<account>/` holds skills synced from claude.ai per
    organisation and account; how it behaves across signed-in accounts was not measured.
  - *Conversation history:* `projects/` (which also holds `<slug>/memory/`, the auto memory), `file-history/`,
    `paste-cache/` (referenced from `history.jsonl`), `image-cache/`, `session-env/`, `plans/` (named by a random slug,
    not the session id) and the file `history.jsonl`. `todos/` and `tasks/` are swept by cleanup, but nothing in 2.1.304
    wrote them.
  - *Per account or per process:* the login and identity files (`.credentials.json`, `.claude.json`, `backups/`,
    `.claude.json.backup`, `hfi-auth.json`, `.session_ingress_token`, `mcp-needs-auth-cache.json`); `sessions/`, a
    per-process registry (`<pid>.json` and a `.key`), so linking it would show one account the other's processes,
    sockets and keys; organisation policy (`remote-settings.json`, `policy-limits.json` with its signature and consent
    files); `shell-snapshots/`; caches, logs and telemetry (`cache/`, `stats-cache.json`, `statsig/`, `telemetry/`,
    `debug/`, `mcp-discovery-cache/`, `state/` and similar); the daemon, job and remote-control state; install-level
    files (`local/`, `chrome/`, updater files, `.deep-link-register-failed`); `.last-cleanup`. The executable has its
    own list of what a host configuration snapshot leaves out as per-host state or credentials, and it matches this
    split. `bridge-spawn` is always at `~/.claude/bridge-spawn` whatever `CLAUDE_CONFIG_DIR` says (executable).
  - *Absolute paths inside shareable entries:* `plugins/known_marketplaces.json` `installLocation` and
    `plugins/installed_plugins.json` `installPath` hold the installing directory's own path; the `debug/latest`
    symlink; the request blobs in transcripts hold the writing account's auto-memory path.
- **Codex**
  - *Configuration:* `config.toml`; `AGENTS.md` and `AGENTS.override.md` (Codex never writes the home copy); `rules/`
    (Codex appends on "always allow"); `skills/` (Codex also writes `skills/.system/` on start, with a versioned
    marker, so the directory is not user-only and `.system` comes from whichever version ran last); `prompts/` (0.162.1
    does not load custom prompts at all); `plugins/`, `themes/`, `hooks.json`, `agents/` (named in the binary or empty
    in the real home; not exercised).
  - *Conversation history:* `sessions/`; `archived_sessions/` (`codex archive` moves the rollout into the archiving
    home's own, so another home cannot find it unless this directory is shared too); `session_index.jsonl` (thread
    names); `history.jsonl` (prompt recall, not needed to resume); `state_5.sqlite`, with its `-wal` and `-shm` files
    beside the link's target.
  - *Per account or per process:* `auth.json` and `.credentials.json` (the MCP OAuth file store); `models_cache.json`
    (per-account catalogue and default effort); `cache/` (account-dependent apps and connectors); `installation_id`,
    `version.json`; `thread_history_1.sqlite` (a projection Codex rebuilds), `goals_1`, `queue_1`, `memories_1` /
    `memories_v2_1`, and `logs_2.sqlite` with its WAL files; the daemon's `packages/`, `app-server-daemon/` and
    `app-server-control/` (a symlink to a socket in `/tmp/codex-daemon-<uid>/<hash>`, so linking it would let one
    account's TUI reach another's daemon); locks and runtime state (`thread-writer-locks/`, `.tmp/`, `tmp/`,
    `.sqlite-maintenance.lock`, `.sandbox_migration`, `shell_snapshots/`, `project-metadata-locks/`,
    `rollout-migrations/`, `log/`); entries the ChatGPT desktop app keeps in the same directory
    (`.codex-global-state.json`, `sqlite/codex-dev.db`, `computer-use/`, `browser/`, `worktrees/`, `pets/` and
    others; the CLI created none).
  - *Absolute paths inside shareable entries:* the real `config.toml` holds the default home's path in `notify` and in
    `[mcp_servers.node_repl.env]` (`CODEX_HOME = "/…/.codex"`), and any absolute `sqlite_home` or `log_dir` would
    apply to every account that shares the file; rollouts record the running home's `skills/.system` path per turn;
    database rows record absolute rollout paths. A shared `config.toml` also makes the per-project trust
    (`[projects."…"] trust_level`) common to every account.
- **Grok Build**
  - *Configuration:* `config.toml` (model, MCP servers, plugin switches, marketplace sources); `AGENTS.md`,
    `pager.toml`, `lsp.json`, `settings.json` (marketplace settings in Claude Code's format), `sandbox.toml`,
    `statusline.sh`, `disabled-hooks`; `rules/`, `skills/`, `commands/`, `agents/`, `personas/`, `workflows/`,
    `plugins/`, `hooks/`, `installed-plugins/`. No Grok writer was found for `lsp.json`, `settings.json`,
    `sandbox.toml` or `statusline.sh`, and loading of `personas/`, `workflows/` and those four was not checked.
  - *Conversation history:* `sessions/`. It also holds a per-folder `prompt_history.jsonl`, the search index
    `session_search.sqlite` and, per the guide and not measured, per-project `permission.toml` "always allow" grants,
    so linking it shares those too. `memory-v2/` was never created in a logged-out scratch home, even with
    `[memory] enabled = true`.
  - *Per account or per process:* credentials (`auth.json` with its lock, `mcp_credentials.json`);
    `trusted_folders.toml` with its lock (see below); identity, state and caches (`agent_id`, `models_cache.json`,
    `settings_cache.json`, which is signed per account, `campaigns_state.json`, `experiments_state.json`,
    `tip_cursor.json`, `slash-mru.json`, `last-copy.txt`, `dashboard/`, `grove/`, `worktrees.db`, `worktrees/`,
    `worktree_pool/`, `long-running-background-tasks/`, `marketplace-cache/`); runtime state (`active_sessions.json`
    with its lock, which stays per home, `.config-init.lock`, `managed_config.lock`, `daemon/`, `leader.log`); logs
    (`logs/`, `memtrace/`, `crash/`); install and update files (`bin/`, `downloads/`, `docs/`, `bundled/`, `bundles/`,
    `vendor/`, `version.json` and similar); managed configuration (`managed_config.toml`, `requirements.toml`).
  - *Absolute paths inside shareable entries:* `summary.json` `grok_home` (the creating home, informational; resume
    from another home works), `chat_history.jsonl` and `prompt_context.json` (paths of the skills and instruction
    files loaded under whichever home ran), `installed-plugins/registry.json` `path`, `hooks-paths` entries.

**Whether a linked file survives the agent saving it** (a file link replaced by a plain file stops following the
target).

| File | Writer | Result |
|---|---|---|
| Claude Code `settings.json` | `/model`, a `/config` toggle, a `/permissions` rule saved to User settings, `claude plugin marketplace add` and `plugin install`, a startup migration | link kept, target updated: Claude Code follows the link one step, writes a temp file and renames it onto the target |
| Claude Code `CLAUDE.md`, `keybindings.json` | the model's Edit and Write tools | refused (`Refusing to write …: it is a symbolic link. Write to the link's target path instead: <target>`), link intact; whether the model retries on the target was not measured |
| Claude Code `CLAUDE.md` | `/memory` | opens `$VISUAL` / `$EDITOR` on the link path; vim kept the link |
| Claude Code `history.jsonl` | normal appends | appended through the link, link kept; a lock is created beside the target (executable: proper-lockfile on the real path) |
| Codex `config.toml` | first TUI start, folder-trust accept, `/model`, `codex mcp add` / `remove` | link kept, the target replaced (new inode, mode 644 to 600) |
| Codex `history.jsonl`, `session_index.jsonl` | appends, `[history] max_bytes` trimming, `/rename` | written in place, same inode |
| Codex `AGENTS.md` | none | Codex never writes it |
| Grok `config.toml` | `grok mcp add -s user`, a `/settings` toggle, `/model` | link kept, the target replaced (new inode) |
| Grok `AGENTS.md`, `pager.toml` | the model's edit tools, `/settings` | link kept, the target replaced |
| Grok `disabled-hooks` | `/hooks` disable and enable | link kept; disable writes in place, enable replaces the target |
| Grok `trusted_folders.toml` | accepting the trust prompt | **link replaced by a plain file** |
| Grok `hooks-paths` | `/hooks-add` | write refused: `<home>/hooks-paths is a symlink`; its entries must lie under the home |

- **A two-level chain breaks the first link** (Claude Code 2.1.304). With the account's `settings.json` linked to
  `default/settings.json`, itself a link to `dotfiles/settings.json`, a save from the account replaced
  `default/settings.json`'s link with a plain file, and the dotfiles copy stopped updating. A save from the default
  account (one level) wrote through correctly. Not measured for Codex.
- **Grok Build follows the whole chain** (1.0.62, measured 2026-10-21, scratch homes, a fake API key, the model
  endpoint on `127.0.0.1`). Default directory D held the files; account A linked `AGENTS.md`, `pager.toml` and
  `disabled-hooks` at D's resolved paths; a per-session farm linked those names either at D's files (one link) or at
  A's entries (two links). In both layouts the model's `write` of `AGENTS.md` and of `pager.toml`, and its `search_replace`
  of `AGENTS.md`, replaced D's file (new inode) and left A's link and the farm's link in place — the farm's still
  pointing at A when the layout was two links. `/hooks` disable wrote D's `disabled-hooks` in place (same inode);
  `/hooks` enable replaced D's file (new inode, emptied). A's link and the farm's link stayed, including when the
  farm pointed at A. The one-link results in the table above still hold. A two-link chain was not measured for
  `config.toml`, `trusted_folders.toml` or `/settings` as a `pager.toml` writer. `/theme` writes `[ui] theme` into
  `config.toml`, not into `pager.toml`.
- **Grok itself rewrites the per-session `config.toml` copy** (1.0.62). `grok mcp add --scope user` and the TUI command
  `/theme` replaced that copy (new inode: the server block, or `[ui] theme`, added). Grok's write alone left D's file
  and A's link unchanged. Octoboard keeps the copy to contain its injected MCP block, then, after the process exits and
  before removing the session home, merges these changes onto the source link's resolved target: `ui.theme`,
  `plugins.enabled`, `plugins.disabled`, and `models.default` unless it equals the `--model` the launch passed (startup
  saves that one; see "Model and reasoning effort" in the Grok Build section), with no exception when no model was
  passed. Plugin install/enable/disable writes the two string arrays, each carried whole; explicit empty arrays are
  carried too. Other UI keys, marketplace startup state, reasoning effort, custom-model definitions, credentials and MCP
  servers are not carried. Only values changed from the pre-launch copy are eligible. A plain source file has no
  writeback; a link removed, broken or redirected to another target before exit is skipped. The account's link stays in
  place. Different fields changed by two sessions in the same daemon are merged; for the same field the last publishing
  exit wins, while an unchanged session writes nothing. An external edit saved before this merge is retained outside
  those changes. External writers do not take the daemon's lock, so an edit concurrent with its read and replacement can
  race. A failed writeback is logged and does not stop removal of the session home. A `grok` run directly against the
  linked account file still updates D itself (the table's `grok mcp add -s user` row).
- **Grok's `disabled-hooks` is keyed by hook source**, e.g. `global/octoboard:session_start[0].hooks[0]`. Switching
  Octoboard's hook off in one session writes the entry into the source home's file, silencing it in every later session
  from that home; with the file linked, that is every sharing account.
- **Grok's sandbox mode refuses a linked tree** (1.0.62, measured in a per-session home built over a home with
  `bundled/`). `grok --sandbox workspace` refuses to start when `hooks/`, `hooks-paths` or `bundled/` is a symlink (the
  guide adds `bundles/` and a symlinked `GROK_HOME`). Octoboard's per-session homes copy `bundled/`, `bundles/`,
  `hooks-paths` and `sandbox.toml`, and a launch whose profile Octoboard can read as not `off` is refused before
  the home is built. Sandbox mode is off by default.

**Items load through links** (measured with a control for each).

- **Claude Code:** `CLAUDE.md`, a skill, an agent and a command reached the request (the command body expanded); an
  output style reached the system prompt; `settings.json` `env` reached a Bash call; a `rules/` file reached the
  request; a `keybindings.json` binding took effect.
- **Codex:** every account home ran on D's provider, model and trust from a linked `config.toml`; `AGENTS.md` appeared
  in `codex debug prompt-input` and in the stub's request; a skill was listed in both homes, with its roots resolved to
  D; a `rules/` file loaded (`loaded 1 .rules files`), and a broken rule in D made `exec` fail with `Error loading
  rules: …`. A rule's effect on a real tool call was not verified. `prompts/` could not be confirmed (it does not load
  in a plain home either).
- **Grok:** in an account home with each item linked, `grok inspect --json` found `AGENTS.md`, `rules/`, a skill, a
  command, an agent, hooks and a `plugins/` plugin; a real turn's request carried the markers of all of those but
  hooks, which fired. Through Octoboard's per-session home (a farm over the account home, whose entries are links into
  D) the same items loaded and the session landed in D's `sessions/`; `hooks-paths` entries loaded.
- **Grok's `hooks/` does not reach an Octoboard session.** The per-session home leaves `hooks/` out and writes its own,
  so only Octoboard's hook ran there. A `grok` run directly against the account home does load the linked hooks.
- **Grok's marketplace plugins stay off in other accounts.** `grok plugin install` copies the plugin into
  `installed-plugins/` but turns it on (`[plugins].enabled`, `[[marketplace.sources]]`) in `config.toml`. A plugin
  installed in D showed in another account's list, with `inspect` saying `enabled: true`, but its skill did not load
  until the plugin was added to that account's own `config.toml`; a plugin installed from the account landed in D's
  directory and was on only there. With `config.toml` linked the switch is shared too (not re-measured). Plugins placed
  directly in `plugins/` are found under any home that links it.
- **Plugin registries record the installing directory.** Grok's `installed-plugins/registry.json` holds a path into the
  installing home (`homes/A/installed-plugins/…`), which points into an empty directory once that account stops
  sharing. Claude Code's `installed_plugins.json` and `known_marketplaces.json` do the same. With A's `plugins/` link
  removed, the default account still loaded the plugin installed through A and `plugin list` showed it enabled, with
  no `.orphaned_at` after two starts (executable: an install into a symlinked cache folder is refused under a flagged
  storage backend, not observed).

**Concurrent conversation-history writes from two directories.**

- **Claude Code** (2.1.304). Two TUIs at once, accounts A and B with `projects/` and the rest linked, same working
  directory, six turns each including Read, Edit and Bash: each transcript was complete (own session id only, no
  dangling parents), `file-history/<id>/` and `session-env/<id>/` stayed separate, and `history.jsonl` took 12 new
  interleaved lines, all valid JSON. A conversation started under A resumed under B, which sent A's whole
  conversation and appended to the same file; B's `/resume` listed A's sessions. Resuming one id from two accounts at
  once still forks the transcript (see "Sharing a session directory between accounts").
- **One account's cleanup reaches every linked directory** (Claude Code 2.1.304, `cleanupPeriodDays: 1`). It deleted
  old entries through the links in the transcript and its project directory, `file-history/<id>/`, `session-env/<id>/`,
  `image-cache/<id>/`, `tasks/<id>/`, the plan file, the paste-cache file, the shell-snapshots file and the debug file.
  An old `todos/` file and an old `history.jsonl` entry were kept.
- **Codex** (0.162.1). Two homes sharing `config.toml`, `sessions/` and `history.jsonl`, each running a TUI (two
  turns) and two `exec` workers (four turns) at once: every process exited 0, each of the six rollouts held only its
  own turns, `history.jsonl` interleaved and parsed, and every thread resumed with all its turns from the other home
  and from a fresh one. Two homes on the same thread were not re-measured here (see "Sharing a session directory
  between accounts").
- **Grok** (1.0.62). Eight `-p` sessions at once (four per account, same working directory, within 0.3 s, replies
  delayed 4 s): all exited 0, all eight session folders were complete, the shared `prompt_history.jsonl` held all eight
  prompts with no broken line, and `session_search.sqlite` passed `integrity_check` with eight rows. Two TUIs at once
  both finished a turn, each home's `active_sessions.json` listing only its own. Cross-account resume carried the
  earlier turns in both directions, also from one per-session home to another. Two processes on one session were not
  measured here.

**Codex's state database through a link** (0.162.1).

- **The write-ahead log sits beside the target.** With `A/state_5.sqlite` linked to `D/state_5.sqlite`, no `-wal` or
  `-shm` appeared in A, and `lsof` showed D's database and both companions open.
- **Concurrency.** Three rounds: six `exec` workers over D, A and B (two each) with four turns; fifteen workers with six
  turns; nine workers while TUIs, each with its daemon, ran turns in A and D. All exited 0, every thread had its row,
  every rollout had the expected `task_complete` count, `PRAGMA integrity_check` returned ok on copies after exit and
  on a `.backup` taken while the daemons ran, and no locked or busy error appeared in stderr, the daemon logs or any
  `logs_2.sqlite`.
- **A dangling link** made Codex create the database at the target and keep the link. **A corrupt target** made Codex
  run with no runtime databases, without a warning, leaving the link and file untouched.
- **Mixed versions.** A fake future migration row (99) was tolerated. The real default database was already at
  migration 61 (from the desktop app's newer Codex) while the CLI knows 59, so mixed versions on one database already
  happen. A future `state_6.sqlite` could not be measured: a new version would presumably create a plain `state_6` in
  each home and the link would quietly stop being used, so launch-time repair matches `state_*.sqlite` and reports one
  the catalog does not name.
- **Effect of sharing it.** Every account sees every account's rows (the resume picker lists them). Backfill runs once
  per database, so an account's private rollouts made before it linked are not imported.
- **Carried rows.** With an account's thread rows carried into the shared database and its rollouts moved into the
  default home, both homes' pickers listed both homes' threads, and `exec resume <id>` from either home carried the
  earlier turns.
- **Rows written through a link.** A thread started in a linked home is recorded by that home's spelling
  (`A/sessions/…`). Once A's `sessions/` stopped being a link, D answered `no rollout found for thread id <id>` for it
  until the row was rewritten to D's own path; after the rewrite D resumed it with its history.

**Codex resume across homes with only `sessions/` shared** (0.162.1; each home keeps its own database).

- A thread made by `exec` in A resumed by `exec resume <id>` in B with A's turns in the request, and A later saw B's
  turn. A TUI thread made in A was listed by B's `codex resume` picker (B adds the row on start by scanning the shared
  directory, naming the link path `B/sessions/…`, and the resolved path after a resume), and both `exec resume` and TUI
  resume carried both turns. The picker reads the database and never lists `exec` threads.
- **Archiving needs `archived_sessions/` shared**: after `codex archive` in A, B answered `no rollout found for thread
  id …`.
- **With `archived_sessions/` shared but the databases separate**, B's row predated the archive and still named the old
  path: resume gave `no rollout found` and `codex unarchive` from B failed (`failed to unarchive session`), whereas a
  fresh home gave the proper "is archived" refusal; after A unarchived, B resumed again. With the database shared too
  (A and B), archive and unarchive were consistent across homes.
- **Names need `session_index.jsonl` shared**: `/rename` writes the home's own index, and in B without it
  `exec resume <name>` silently started a new session (Codex does this for any unknown name); with the index linked
  the name resolved.
- The revert case (a row naming an older segment) was not re-measured; a shared database removes it, since there is one
  row.

**Claude Code's auto-allow for auto memory and plan files is lost through a link** (2.1.304; `-p` and TUI, with a
real-directory control). A write into `projects/<slug>/memory/` is refused under `-p`, and the TUI asks "Do you want to
create …? … (outside working directory) via a symlink"; the resolved shared path behaves the same. A plan file in
`plans/` is refused under `-p` (`Cannot write to …plans/… while in plan mode`) and prompts in the TUI. In a real
directory both go through. Mechanism (executable, confirmed by the results below): the write check runs for every
spelling of the path, the requested `<account>/projects/…` and the link-resolved `<shared>/projects/…`, and each must
be allowed. Auto memory is allowed when the path starts with `<configDir>/projects/<key>/memory/` (the account's
spelling) and a plan file when its parent equals `<configDir>/plans`; the resolved spelling never matches, and only
fixed system pairs such as `/private/tmp` and `/tmp` count as equivalent.

- **Auto memory, what restored it:**
  - `CLAUDE_CODE_REMOTE_MEMORY_DIR=<shared directory's real path>` works under `-p` and in the TUI (process
    environment); set in the shared `settings.json` `env` block it works for sharing and default accounts (`-p` only
    measured). Memory lands in `<shared>/projects/<key>/memory/` with the same key the default account uses natively
    (the git root), so nothing moves and a resume under another account sees it. It is an internal, undocumented
    variable and is passed on to child processes. From the executable, not measured: it also moves user-scope agent
    memory to `<shared>/agent-memory` and local-scope agent memory to `<shared>/projects/<key>/agent-memory-local`, and
    a restarted background job drops it.
  - `permissions.allow` rules for both spellings (`Edit(/<shared>/projects/*/memory/**)` and
    `Edit(/<account>/projects/*/memory/**)`) work, but the account spelling differs per account, so it needs a
    per-launch `--settings`; one spelling alone is refused; the wildcard `Edit(//**/projects/*/memory/**)` worked under
    `-p` and is broad.
  - Not restoring it, or only badly: `autoMemoryDirectory` set to the link path (refused, prompts); set to the
    resolved `<shared>/projects/<slug>/memory` it works, but it is one directory for all projects (another working
    directory used it too), merging every project's memory; `additionalDirectories: [<shared>/projects]` does nothing.
- **Plan files, what restored it:** `plansDirectory: ".claude/plans"` (relative to the project) works under `-p` and in
  the TUI, also from a shared `settings.json`, for account and default alike; plans then live in the project's working
  tree (could be committed), visible to every account there, move the default account's plans too, and fall out of
  the configuration directory's cleanup sweep (inferred). An absolute `plansDirectory` outside the project is ignored
  and refused (it must resolve inside the project root, else it falls back to `<config>/plans`).
- **Plan files, what did not:** allow rules for both spellings, `additionalDirectories`, and
  `CLAUDE_CODE_REMOTE_MEMORY_DIR`. Linking per-project subdirectories still puts a link in the path (same rejection by
  reasoning; not run).
- **Not measured:** leaving `plans/` unshared, which keeps the auto-allow but means an in-progress plan file does not
  follow a conversation resumed under another account.

**Residue the probes leave outside the scratch directory.**

- **Grok** writes lock-slot files to `/tmp/grok-file-lock-<uid>/` whatever `TMPDIR` says.
- **Claude Code** creates per-working-directory temp directories in `/tmp/claude-<uid>/` despite `TMPDIR`. Its first
  TUI run from a throwaway `HOME` also registered `<HOME>/Applications/Claude Code URL Handler.app` as a system-wide
  `claude-cli://` handler with LaunchServices (`lsregister -u` removes it); setting `disableDeepLinkRegistration` to
  `"disable"` in the settings stops later runs from doing it.
- **Codex** makes every TUI start in a scratch home launch `<home>/packages/app-server-daemon/…/codex app-server
  --managed-daemon` (parent pid 1), which outlives the TUI and copies about 323 MB into that home's `packages/`. Its
  sockets in `/tmp/codex-daemon-<uid>/` are removed on exit.

**Not verified.** Claude Code: the model's retry of a refused Edit or Write on the link's target; synced-skill buckets
across accounts; whether the history lock is shared across accounts; the retention prune of `history.jsonl` (it never
ran, even in a control); loading of `workflows/`, `themes/`, `hooks/`, `agent-memory/` and `loop.md` through a link;
the install refusal for a symlinked `plugins/`; Claude Code 2.1.274. Codex: a link on a future `state_6.sqlite`;
`plugins/`, `agents/`, `themes/`, `hooks.json` through a link; whether rollouts reference `generated_images/`; a rule's
effect on a real tool call; whether a daemon holding a thread notices another home changing the shared row; `prompts/`
on other versions. Grok: two processes on one session; loading of `personas/`, `workflows/`, `lsp.json`,
`settings.json`, `sandbox.toml`, `statusline.sh`.

## Codex resume from aged rollouts

Measured on **Codex 0.162.1** with `codex exec resume <id> "<prompt>"` in a throwaway `CODEX_HOME`, on a thread whose
first turn set a secret word; success means the model recalled it. Compression was done with `zstd`, giving the same
`rollout-…-<id>.jsonl.zst` name Codex itself produces.

| Rollout location | Resumes? |
|---|---|
| `sessions/YYYY/MM/DD/…jsonl` (control) | yes |
| `sessions/YYYY/MM/DD/…jsonl.zst` | yes |
| `archived_sessions/…jsonl` | no — ``session <id> is archived. Run `codex unarchive <id>` to unarchive it first.`` |
| `archived_sessions/…jsonl.zst` | no — the same refusal |

- **A compressed rollout resumes in place, and Codex decompresses it**: after the resume the `.zst` was gone and a plain
  `.jsonl` stood at the same path. So a `.zst` under `sessions/` counts as a present record.
- **An archived thread is refused by where it sits, plain or compressed**, with the text above (`code -32600`). It is
  not a missing record: `codex unarchive <id>` moves the file to `sessions/YYYY/MM/DD/` keeping its extension (a `.zst`
  stays `.zst`), and the resume that follows works from it. So a rollout in `archived_sessions/` is present but needs
  an unarchive first.
- **Codex's database** (`state_5.sqlite`, table `threads`, columns `rollout_path`, `archived`, `archived_at`) is
  consulted before the directories. With a row present, resume reads the file at `rollout_path`; when that file is
  missing the answer is `no rollout found for thread id <id>` **even if the record sits elsewhere under `sessions/` or
  `archived_sessions/`** (measured with the plain control). With no database at all Codex rebuilds it by scanning, and
  finds the record wherever it sits (`.jsonl` or `.jsonl.zst`, `sessions/` or `archived_sessions/`).
- **The recorded path is not kept in step with compression.** On the measured machine's own database every thread's
  `rollout_path` ends in `.jsonl` (980 under `sessions/`, 233 archived with `archived = 1` under `archived_sessions/`)
  while the files on disk are largely `.jsonl.zst`; a row with a `.jsonl` path and a `.zst` file at that path still
  resumed (tested with the file in `sessions/`). Do not treat `rollout_path` as the file's real name.
- **A row with `archived = 1` but no `archived_at` did not refuse**: the resume went through and set `archived` to 0.
  The refusal needs `archived_at` as well (the copy with `archived_at` set was refused), so a hand-edited row can
  mislead.
- **A resume that succeeds from a database row is not proof of the directories**: with the original home's database
  copied along, the row's absolute path pointed back into the *source* home and every location "resumed" from there.
  Clear or rewrite the row (or use no database) before judging a relocation.

## Plan quota

Each CLI can say how much of an account's plan quota is used, over a channel meant for its own front ends. All three
are marked experimental by their makers, so a reply that does not parse is a reason to show the quota as unavailable,
never to fail anything else. Measured against the default account of each on Claude Code 2.1.302, Codex 0.162.1 and
Grok Build 1.0.60, with the working directory set to the home directory and the agent's configuration-directory
variable (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GROK_HOME`) set to a pinned account's directory, or left alone for the
default account. The CLI handles its own login, Keychain access and token refresh, so nothing here reads a credential.

Codex and Grok Build want **stdin kept open until the reply is on stdout**: they exit silently when it closes first.
Claude Code answers with stdin open as well, so all three are asked the same way. Each answers in about one to three
seconds. A request that gets no reply within fifteen seconds is given up on and the CLI's whole session killed.

**Claude Code** (do not pass `--bare`: it skips the Keychain, so the login is not found):

```
claude -p --input-format stream-json --output-format stream-json --verbose --no-session-persistence \
  --strict-mcp-config --mcp-config '{"mcpServers":{}}'
```

The last two flags keep the user's MCP servers from starting; the reply is the same with and without them. Settings are
deliberately left to load (see "What a probe touches" below).

Write one JSON object per line:

```
{"type":"control_request","request_id":"r1","request":{"subtype":"initialize"}}
{"type":"control_request","request_id":"r2","request":{"subtype":"get_usage","skip_behaviors":true}}
```

The `initialize` reply (a large `control_response` listing commands) is passed over; the reply to `r2` is
`{"type":"control_response","response":{"subtype":"success","request_id":"r2","response":{...}}}`, and a refused
request has `"subtype":"error"` and an `"error"` string. In `response.response`:

- `subscription_type` (`"max"`) and `rate_limits_available`;
- `rate_limits.limits[]`, the list to read: entries `{kind, percent, severity, resets_at, scope, is_active}` where
  `kind` is `session` (the 5-hour window), `weekly_all` (the weekly window) or `weekly_scoped` (a weekly window of one
  model, named by `scope.model.display_name`, e.g. `"Fable"`). `percent` is used, not remaining; `resets_at` is
  ISO 8601 with a fractional second and an offset;
- `rate_limits.five_hour` and `seven_day` (`{utilization, resets_at}`) and `model_scoped[]`
  (`{display_name, utilization, resets_at}`) carry the same windows in an older layout, with `seven_day_opus` and
  `seven_day_sonnet` as `null` on the measured account;
- a number of other `rate_limits` keys, each `null` or a bucket with no label (one measured account had a dollar-capped
  one at 100%), plus `extra_usage`, `spend` and `seven_day_breakdown`. None of them is a quota window to show.

An account with no subscription is expected to report `rate_limits_available: false` or a `null` `rate_limits`; that
was not measured, as no such account was at hand. A directory with no login answers with exactly that shape (Claude
Code 2.1.303), so the reply alone cannot tell the two apart. What each agent answers when its directory is missing or
not signed in is under "Signing in a config directory without a terminal" above.

**Codex**:

```
codex app-server -c features.plugins=false -c features.remote_plugin=false -c features.apps=false -c features.hooks=false
```

The `-c` overrides keep the probe from reaching out on start (see "What a probe touches" below); the reply is the same
with and without them. They are configuration overrides rather than `--disable <feature>` because `--disable` refuses a
feature name the installed Codex does not know, where an unknown configuration key is ignored (measured with
`-c features.not_a_real_feature=false`).

Write JSON-RPC lines (the server's own lines carry no `jsonrpc` member):

```
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"octoboard","version":"0.1.0"}}}
{"jsonrpc":"2.0","method":"initialized"}
{"jsonrpc":"2.0","id":2,"method":"account/rateLimits/read"}
```

Notifications (`remoteControl/status/changed`, `account/updated`) arrive between the replies and are passed over. The
reply to id 2 is `{"id":2,"result":{...}}`, or `{"id":2,"error":{"code","message"}}`. `result.rateLimits` is
`{limitId:"codex", primary, secondary, credits, planType, ...}` where `primary` and `secondary` are
`{usedPercent, windowDurationMins, resetsAt}` — `300` and `10080` minutes on the measured account, `resetsAt` in
Unix **seconds** — and `planType` is `"plus"`. `result.rateLimitsByLimitId` repeats the same under each limit id. The
schema is available from `codex app-server generate-json-schema` (`v2/GetAccountRateLimitsResponse.json`).

**Grok Build**:

```
grok agent --no-leader stdio
```

Write ACP (JSON-RPC) lines. The method name's **leading underscore is required**:

```
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}
{"jsonrpc":"2.0","id":2,"method":"_x.ai/billing","params":{}}
```

The server sends notifications of its own in between (`_x.ai/mcp/servers_updated`). The reply to id 2 is
`{"jsonrpc":"2.0","id":2,"result":{"config":{...},"subscription_tier":"SuperGrok"}}`, with `config.creditUsagePercent`
(a float, `100.0` on the measured account), `config.currentPeriod` `{type:"USAGE_PERIOD_TYPE_WEEKLY", start, end}`
(ISO 8601), and spend fields (`onDemandCap`, `onDemandUsed`, `prepaidBalance`) that are not a quota window. There is
one credit pool, not a session and a weekly window.

**What a probe touches.** The poller runs each probe every five minutes per account, so what a start does matters.
Measured on the same builds by listing the files each probe changed in the agent's home and the processes alive in its
process group when the reply arrived.

- **Claude Code** syncs plugins on start (it rewrites `plugins/synced/…` and a `.in_use` marker per installed plugin
  version) and registers the process in `sessions/<pid>.json` with a `.key` file. `--setting-sources local` stopped the
  plugin writes, but it also drops the user's `settings.json`, which can carry `apiKeyHelper`, `env` (API key or auth
  token, base URL, Bedrock or Vertex, proxy, extra CA certificates) and `forceLoginMethod`, so a probe could fail or read
  a different identity than the account's sessions use; the probe therefore leaves settings loading as it is. An
  `enabledPlugins` override through `--settings` did not stop the sync. `--strict-mcp-config` with an empty
  `--mcp-config` does keep the user's MCP servers from starting. So the plugin sync and the `sessions/` registration
  stay; a later Claude Code start clears a registration whose process is gone (the probe is killed, so it leaves its
  own until then). No session record is written, with `--no-session-persistence`.
- **Codex**, bare: the app server starts a plugin catalogue sync that runs `git ls-remote` over SSH against a plugin
  marketplace and writes `plugins/cache/…` and `.tmp/git-…`. With `features.plugins`, `features.remote_plugin`,
  `features.apps` and `features.hooks` set to false neither happens and the reply is unchanged. Still written:
  `models_cache.json`, `logs_*.sqlite`, `memories_*.sqlite-shm` and a `tmp/arg0/codex-arg0…` directory that Codex clears
  on a later start. No MCP server process was running at the reply.
- **Grok Build** has no flag to skip anything: `grok agent stdio` accepts only `--debug`, `--debug-file` and
  `--leader-socket`. It announces the user's configured MCP servers (`_x.ai/mcp/servers_updated`) but none ran as a
  child process at the reply. It writes `models_cache.json`, `sessions/session_search.sqlite` and `logs/unified.jsonl`
  in `GROK_HOME`, so a probe of an account does write into that account's directory.

What remains unavoidable: each probe starts one agent process and writes the files named above, in the agent's home
and, for Claude Code, its `sessions/` registration; a Claude Code probe syncs the user's plugins; and a Grok Build probe
starts with the account's whole configuration. Whether a user's hook fires was not observed for any agent: no session or
turn starts in a probe, and the hook events documented above are session events.

## Starting a quota window

A window only starts counting at the account's first request after it resets; until then the CLI reports it with no
reset time. A **kick** is one one-word request on that account that starts it. Each agent is run the way its quota probe
is (login-shell environment, the configuration-directory variable set to the account's directory, the home directory as
working directory, the CLI's whole session killed when it ends, after sixty seconds or when the daemon shuts down), with
stdin closed and the reply not read: that it exited with status 0 is all that is wanted, and more than 64 KiB on stdout
fails the kick (a runaway CLI is cut off, its output not kept). Each takes the cheapest model its catalogue offers and
its lowest reasoning effort, and is made as small as the CLI allows: a one-line system prompt in place of its own, no
tools, one turn, a capped reply, and as little of the user's setup as can be left out without changing which sign-in is
used. No agent is run with an option that changes the credentials it reads (Claude Code's `--bare` skips the Keychain).

```
claude -p hi --model haiku --effort low --no-session-persistence --system-prompt "Reply with one word." --tools "" \
  --disable-slash-commands --strict-mcp-config --mcp-config '{"mcpServers":{}}' \
  --settings '{"disableAllHooks":true,"outputStyle":"default"}'
    # with CLAUDE_CODE_MAX_OUTPUT_TOKENS=128 CLAUDE_CODE_DISABLE_CLAUDE_MDS=1 in the environment
codex exec --ephemeral --skip-git-repo-check --ignore-user-config --ignore-rules -m gpt-6-luna \
  -c model_reasoning_effort="low" -c instructions="Reply with one word." \
  -c include_permissions_instructions=false -c include_apps_instructions=false -c include_environment_context=false \
  -c include_collaboration_mode_instructions=false -c web_search="disabled" \
  -c features.plugins=false -c features.remote_plugin=false -c features.apps=false -c features.hooks=false \
  -c features.shell_tool=false -c features.unified_exec=false -c features.sleep_tool=false \
  -c features.view_image=false -c features.image_generation=false -c features.goals=false \
  -c features.multi_agent=false -c features.browser_use=false -c features.browser_use_external=false \
  -c features.in_app_browser=false -c features.computer_use=false -c features.tool_suggest=false \
  -c features.skill_search=false hi
grok -p hi -m grok-4.7 --effort low --system-prompt-override "Reply with one word." --no-subagents \
  --disable-web-search --no-plan --max-turns 1 --disallowed-tools run_terminal_cmd,read_file,search_replace,list_dir,grep,\
kill_task,todo_write,get_task_output,wait_tasks,task,scheduler_create,scheduler_delete,scheduler_list,monitor,\
search_tool,use_tool,workflow,enter_plan_mode,exit_plan_mode,ask_user_question,send_feedback,image_gen,image_edit,\
image_to_video,reference_to_video,write,web_search,web_fetch,Agent
    # with GROK_CONFIG='{"models":{"max_completion_tokens":128}}', GROK_CLAUDE_SKILLS_ENABLED=0,
    # GROK_CLAUDE_MCPS_ENABLED=0, GROK_CLAUDE_AGENTS_ENABLED=0, GROK_CLAUDE_RULES_ENABLED=0,
    # GROK_CLAUDE_HOOKS_ENABLED=0, GROK_TURN_SUMMARY=0 and GROK_SESSION_RECAP=0 in the environment
```

How each was chosen, from the installed CLIs' `--help`, their on-disk documentation and requests captured by a local
stand-in endpoint on a scratch configuration directory with a fake key (so nothing reached the vendor); sizes are the
request body in characters (2026-10-22):

- **Claude Code** takes `haiku`, the alias for the newest Haiku (see "Discovering models"), so no version is written
  down; `--effort low` is always accepted. `--no-session-persistence` keeps the conversation off disk,
  `--strict-mcp-config` with an empty `--mcp-config` keeps the user's MCP servers from starting as the probe does, and
  `--tools ""` sends no tool definitions. `--system-prompt` replaces the default prompt (9.3 KB even with no tools) with
  20 characters. `--settings` merges over the user's settings: `disableAllHooks` stopped both a user's own
  `SessionStart` and `UserPromptSubmit` hooks and a plugin's (loaded with `--plugin-dir`), none of the four firing where
  all four did without it, and `outputStyle: "default"` drops the user's output style from the request (-1.4 KB in the
  measured one). `--disable-slash-commands` leaves skills out. `CLAUDE_CODE_MAX_OUTPUT_TOKENS` sets the request's
  `max_tokens` (128000 otherwise) and `CLAUDE_CODE_DISABLE_CLAUDE_MDS` drops the user's `CLAUDE.md` from it. With all of
  it the request body went from 28 KB to 1.9 KB. The prompt comes straight after `-p`, since `--tools` and
  `--mcp-config` take lists and would swallow a prompt placed after them. What a kick still loads: the user's settings
  other than hooks and output style (an `apiKeyHelper`, `env`, proxy and `forceLoginMethod` must stay, as they decide
  the sign-in), and the plugin sync a probe does too. There is no flag to cap thinking: `alwaysThinkingEnabled` in
  `--settings` left the request's `thinking` unchanged.
- **Codex** has no alias, so the model is a catalogue slug: of `codex debug models`, `gpt-6-luna` is the newest entry
  described as "Fast and affordable", and its levels include `low`. The effort is only a configuration override. `exec`
  is its non-interactive mode, `--ephemeral` writes no session files and `--skip-git-repo-check` lets it run from the
  home directory. `-c instructions=` replaces the model's base prompt (18 KB) with the one line; the three `include_*`
  overrides and `include_collaboration_mode_instructions` leave the permissions, apps, environment and collaboration
  messages out, and the four feature overrides also set for the probe's app server stop the plugin sync, apps and
  hooks. On Codex 0.162.1 the request went from 53 KB to 26 KB with the feature overrides below. What still goes in is
  an 18 KB tool section (`exec`, `wait`, `request_user_input` and the `collaboration` tools: no `-c` override removed
  them), 2.7 KB of skills instructions, the multi-agent messages (3 KB) and the user's `$CODEX_HOME/AGENTS.md`
  (`project_doc_max_bytes=0` did not drop it).
  **Feature overrides, tried on Codex 0.163.0** (the build at hand; against a stand-in endpoint with a fake API
  key, every feature `codex features list` shows as on and not removed set to `false` on top of the ones above, one at a
  time): none changed the request body (25,700 characters each time). That covers `code_mode_host`, `tool_suggest`,
  `skill_search`, `computer_use`, `browser_use` (with `browser_use_external` and `in_app_browser`), `shell_snapshot`
  (which only affects how a shell starts) and every other default-on feature. What a fake key cannot show is what a
  ChatGPT sign-in adds (connector and plugin suggestions, the browser and computer tools), so the kick sets
  `tool_suggest`, `skill_search`, `computer_use`, `browser_use`, `browser_use_external` and `in_app_browser` off all the
  same. In the same measurement `shell_tool`, `goals` and `view_image` each took 0.7 to 3 KB out of the body, which is
  33,005 characters without any feature override, while `multi_agent=false` and `unified_exec=false` left it as it was
  (the `collaboration` and `exec` tools stayed). The kick sets those two off, with `sleep_tool` and `image_generation`,
  as a precaution all the same.
  **`--ignore-user-config`**: `-c mcp_servers={}` and `-c notify=[]` do not empty the user's `[mcp_servers.*]` and
  `notify` (a `-c` override merges into those tables, and both still started in a scratch home), so only leaving
  `config.toml` out keeps them and the user's plugins from starting. The sign-in is read from `CODEX_HOME` all the same:
  on an account out of credits the request got as far as the server's "workspace is out of credits" refusal. What it
  costs is the rest of that file: an account whose `config.toml` selects another credential store
  (`cli_auth_credentials_store`) or provider is not served, and its kick fails and is logged. An unknown model slug is
  not refused at launch, only at the first turn, so a catalogue that drops this slug makes the kick fail with that
  error.
- **Grok Build** lists four models and describes `grok-4.7-build-fast` as "Fast variant. 2x the price", so the kick
  takes the plain `grok-4.7`; whether `grok-4.5` or `grok-4.6` costs less was not found out. `-p` is its single-turn
  mode and `--effort` is an alias of `--reasoning-effort` (`low` is in every model's levels). `--system-prompt-override`
  replaces the system prompt (5.2 KB). `--tools ""` takes no tool out; the tools are removed by name with
  `--disallowed-tools` (the ids above, which cover every built-in tool, subagents, web search and plan mode of the
  1.0.67 build), taking the request's tool section from 39 KB to none, and with the rest the whole request from 55 KB to
  6 KB. `--no-subagents`, `--disable-web-search`, `--no-plan` and `--max-turns 1` keep the turn to the one reply. The
  `GROK_CLAUDE_*_ENABLED` variables stop it scanning Claude Code's skills, MCP servers, agents, rules and hooks (the
  skills list shrank from 7.6 KB to 2.4 KB; what remains comes from another skill source), and `GROK_CONFIG`, an overlay
  that accepts `models` among its few allowed tables, caps the reply with `max_completion_tokens` (the request's
  `max_tokens`). The request also sends two session-title requests of about 1 KB each to another model, which no option
  found stopped. What a kick still loads: Grok's own `config.toml` and a session record left in the account's directory
  (no flag keeps it off disk or leaves its MCP servers out; a configured MCP server was not started before the turn in
  the measured run, as for the probe). `grok -p` runs the agent in its own process: with `[cli] use_leader = true` set
  in a scratch `GROK_HOME` it created no `leader.sock` and no leader process, so it does not join the shared leader at
  `~/.grok/leader.sock`, and a non-default account's kick cannot spend the default account's window through it. Run from
  the home directory, which holds project sources of Claude Code's (`.claude/`) and was never trusted in the scratch
  directory, it was not stopped by the folder-trust gate; it started and sent its request.

What was checked is the requests above, captured and sized as stated, and that each CLI parses these flags. On the
default accounts at hand each kick was refused for want of quota, so that a kick starts a window was not observed on any
of them: Claude Code's weekly window was used up (`You've hit your weekly limit`, exit status 1, the session window
still without a reset time afterwards), Codex's only window (a weekly one) was used up (`Your workspace is out of
credits`, exit status 1, no 5-hour window listed, its reset time set) and Grok Build's sign-in had expired. So that the
three models are served on every plan, that `low` is accepted by each of them on the server's side (Codex's request was
accepted far enough to be refused for credits), that a capped reply still ends with status 0, and that an unstarted
Codex or Grok Build window reports no reset time were not measured.

## Injecting Octoboard into each agent

How each agent is given Octoboard's status hooks, its MCP server and a role description, with the condition each one
brings. Everything is per launch (arguments, an environment variable, or files Octoboard writes itself) and never an
edit of the project's or the user's files, and was exercised against the versions above. What a user sees of it is in
`docs/product/launching-agents.md`.

| Capability | Claude Code | Codex | Grok Build |
|---|---|---|---|
| Launch with an initial task | `claude "<task>"` | `codex "<task>"` | `grok "<task>"` |
| Pre-allocate a session id | `--session-id <uuid>` | **Not possible.** Take it from the `SessionStart` payload; the interactive thread is created lazily on the first prompt, so a session opened without a task has no id until the user types | `--session-id <uuid>` (new sessions only) |
| Inject hooks | `--settings <json-or-path>`, merged with the project settings | `-c 'hooks.<Event>=[{hooks=[{type="command",command=…,timeout=3,async=true}]}]'`, one per event, plus the hook-trust step | A per-session `GROK_HOME` whose `hooks/` directory is Octoboard's and whose other entries mostly symlink to the source home |
| Inject MCP | `--mcp-config <json>` | `-c 'mcp_servers.octoboard.command=…'`, `.args=[…]`, `.default_tools_approval_mode="approve"` and `.tool_timeout_sec=660` | An `[mcp_servers.octoboard]` block appended to the `config.toml` copy inside that `GROK_HOME`, with `tool_timeout_sec = 660` |
| Inject a role description | `--append-system-prompt` | `-c 'developer_instructions="…"'`, which adds a developer message and leaves the rest of the prompt byte-identical; not `-c instructions=`, which replaces the system prompt | `--rules "…"`, appended to the system prompt and persisted into the session record |
| Resume | `claude --resume <id>` | `codex resume <id>` | `grok --resume <id>` |
| Choose the model and reasoning effort | `--model <id>`, `--effort <level>` | `-c model="<id>"`, `-c model_reasoning_effort="<level>"` | `--model <id>`, `--reasoning-effort <level>` |
| Identity seen by the MCP child | `CLAUDE_CODE_SESSION_ID` is set | argv or an explicit `env` table; no `CODEX_*` variable reaches the child | `GROK_SESSION_ID` is set; `{{session_id}}` templating does *not* work |

- **Claude Code** injects cleanly through flags. Never pass `--setting-sources` (it silently drops the project's own
  permission rules and hooks) or `--strict-mcp-config` (it silently drops the project's and the user's MCP servers). The
  injected MCP server's key must not collide with one the project defines, or the project's is silently never spawned.
- **Grok Build** has no flag for hooks or MCP, and its `GROK_CONFIG` / `GROK_CONFIG_PATH` overlay accepts only
  allowlisted keys, so both silently drop. The working mechanism is a per-session `GROK_HOME` where every linked entry
  points at the source entry's fully resolved target, except a copied `config.toml` carrying the MCP block, a copied
  `trusted_folders.toml` with nothing added to it, an Octoboard `hooks/` directory, and real copies of `bundled/`,
  `bundles/`, `hooks-paths` and `sandbox.toml`. `auth.json` and `sessions/` stay links, so login is shared and
  sessions stay resumable from the user's own `grok`. The mechanism cannot run under Grok's sandbox (Grok Build
  1.0.62, macOS):
  - With any profile but `off`, Grok refuses to start when `bundled/` or `bundles/` is not a real directory, or
    `hooks-paths` or `sandbox.toml` not a real file: `hook write-deny ensure failed: Grok directory has wrong type
    (expected real directory)`, then `Refusing to start with its protections missing`. Missing ones it creates itself,
    together with empty `managed_config.toml` and `requirements.toml`.
  - Past that check, the sandbox lets Grok write only to its home, the working directory and temp directories, judged
    on the resolved path, so a `sessions/` link into another directory fails session creation
    (`session.create.failed … FS_PERMISSION_DENIED`); a real `sessions/` in the home works, and without the sandbox
    the link works.
  - The profile a conversation started under is saved as `sandbox_profile` in its `summary.json` (`"off"` when
    unsandboxed) and restored on a resume.

  Octoboard therefore refuses a launch whose profile it can read as non-`off` (`grok_sandbox_unsupported`).
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
- **Codex** shows its folder-trust confirmation only when the working directory is inside a git repository; outside one
  it never asks and reads `AGENTS.md` all the same (measured on 0.161.0, and again on 0.162.1, where an interactive
  launch in a fresh non-git directory drew no confirmation and its first request carried that directory's `AGENTS.md`,
  while one in a fresh `git init` directory drew the confirmation below). The screen reads "Trust this folder? Codex can
  read, edit, and run files here, …", then "› 1. Trust and continue", "2. Quit" and "enter continue · esc quit". The
  cursor (`›`, U+203A) starts on option 1, and Enter alone accepts; any carriage return reaching the screen accepts.
  Codex records `[projects."<repository root>"] trust_level = "trusted"` in `$CODEX_HOME/config.toml`: from a
  subdirectory it records the repository root, and from a linked worktree the main repository's root, so the record can
  cover more than the folder asked about. A trusted parent directory in that file does not cover a repository below it.
  No hook fires before or right after the confirmation, and none at startup at all until the first prompt is submitted.
  Codex sets no terminal title before the confirmation or the sign-in screen that precedes it on an account's first
  launch; its first title (OSC 0/2) comes once its session starts (0.160.0–0.163.0), so that title marks it past both.
  With `[tui] terminal_title = []` it never sets one, which is why Octoboard passes `-c tui.terminal_title=[…]`. The ">_
  OpenAI Codex (v…)" banner is drawn before the sign-in screen and the confirmation alike, so it marks neither. Codex
  redraws only the cells that changed, so text read off the screen can lose characters wherever the frame before held
  the same one; the confirmation's own phrases came through intact in its first drawing, a path above it did not. The
  review Codex raises for new hooks without `--dangerously-bypass-hook-trust` reads "Hooks need review … 2. Trust all
  and continue" and is a different modal. On 0.161.0 a key the confirmation ignores (a letter, Right, Tab, `?`) draws
  nothing but a bare synchronized-update frame (`ESC[?2026h ESC[39m ESC[49m ESC[0m ESC[?2026l`); accepting it clears the
  screen and redraws it whole, starting with the ">_ OpenAI Codex (v…)" banner, which is how an answer given in the
  terminal is told apart.
- **Grok Build** shows its confirmation only when the folder holds content its trust gates, such as an `AGENTS.md`,
  whether or not it is a git repository, so a console directory holding an instruction file is asked about too. The
  screen reads "Do you trust the contents of this directory?", "Grok Build may run or modify contents in this directory,
  posing security risks.", "Yes, proceed y" and "No, quit n", with no cursor; `y` or Enter accepts, and a decline exits.
  Grok writes `[folders."<canonical cwd>"]` with `trusted = true` and `decided_at` (an integer of Unix seconds) into
  `$GROK_HOME/trusted_folders.toml`; on 1.0.62, if the existing store does not parse (a string `decided_at`, say), an
  accepted confirmation fails with "Couldn't save folder trust: the trust store could not be read…" and Grok exits. It
  saves by replacing the file, so against Octoboard's per-session `GROK_HOME` the entry lands only in the session's copy
  and a symlink there is replaced rather than followed; no setting moves the store. Octoboard therefore carries that one
  entry, unchanged, into the user's own store, after its own press and after the person answers in the terminal alike,
  holding the `trusted_folders.toml.lock` Grok keeps beside it. Grok locks that same file before it replaces the store
  (measured on 1.0.50: with an exclusive `flock` held on it elsewhere, an accepted confirmation wrote nothing until it
  was released), and Octoboard's lock is exclusive, so the two writes exclude each other. `SessionStart` fires right
  after the press. While it waits, Grok animates its logo at about 2 KB/s, so the first drawing soon leaves any window
  of recent output and the terminal is never quiet.

### Writing into a running session

The sequence for all three agents is `ESC[200~`, the text with LF separators, `ESC[201~`, then `CR`; multi-line text
arrives as one message with no delay between paste and Enter. Every agent queues a message written mid-turn and consumes
it when the turn ends. (Codex additionally has `codex queue --thread <session id>`.) Five rules, each failing silently
or destructively if ignored:

- A message starting with `/` runs as a slash command even inside a bracketed paste; prefix a single space. The same
  holds for a prompt given on the command line (Claude Code 2.1.308, a fresh launch against a stub endpoint under a
  scratch `HOME`): `claude "/help me please"` runs the command and makes no model request, while `claude " /help me
  please"` reaches the model as text, so the launch prompt is guarded by the same leading space. Not run with
  `--resume`.
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
  through terminal modes. If the hook state is missing or stale, do not write. Two kinds of screen are the exception,
  since no hook announces them, and are read off the terminal instead. The folder-trust confirmations are read from the
  output: no hook runs before one is answered, and nothing is written into a session from its launch until its first
  hook has arrived; for Codex, which runs none until its first prompt, also until the confirmation has been pressed, the
  person's answer in the terminal is seen to replace it, or the watch for it has ended with none sighted; for Grok
  Build, which may run no hook at all, at most until 30 s have passed with no confirmation sighted, or, for one sighted,
  until the rendered screen no longer shows it. The dialogs an agent raises that only a person can answer (Claude Code's
  launch and resume dialogs and its views of background agents, Codex's and Grok Build's own screens) are read from the
  rendered screen, which is looked at before each write: nothing is written while one is up.
- Claude Code does not send a prompt that holds an invisible character. When Enter submits one, it removes those
  characters, leaves the cleaned text in the input box with "Removed N invisible characters · review and press Enter to
  send", and starts no turn (its code returns before the submission that fires `UserPromptSubmit`, read from the
  executable rather than observed through a hook); the `CR` that follows the paste is spent on that hold, so the write
  succeeds and nothing is sent. A message written mid-turn is held the same way rather than queued, and is still sitting
  in the input box when the turn ends. A later bare `CR` sends the cleaned text. What it removes is: the control
  characters other than tab and LF; U+00AD, U+034F, U+061C, U+115F, U+1160, U+17B4, U+17B5, U+180B–U+180F,
  U+200B–U+200F, U+2028–U+202E, U+2060–U+206F, U+3164, U+FE00–U+FE0F, U+FEFF, U+FFA0, U+FFF0–U+FFFB, U+1107F,
  U+13430–U+1343F, U+16FE4, U+1BCA0–U+1BCA3, U+1D173–U+1D17A and U+E0000–U+E0FFF. A line break other than LF (CR, VT,
  FF, NEL, U+2028, U+2029) becomes LF but still counts as a removal; only a CR right before an LF is dropped without
  counting. It keeps some of them by context: a zero-width joiner between two `Extended_Pictographic` characters
  (looking back past up to two U+FE0F or skin-tone modifiers), U+FE0E or U+FE0F right after an emoji or between a digit,
  `#` or `*` and U+20E3, the England, Scotland and Wales tag flags, and script-dependent cases — a joiner or non-joiner
  among letters of Arabic, Indic and some other scripts, a zero-width space in Thai, Khmer, Lao or Myanmar text, a
  direction mark in a line holding right-to-left letters, U+034F after a combining mark, and a few more. Private-use and
  unassigned code points, and format characters outside the list such as U+0600, pass untouched. The UI's bidirectional
  isolates (U+2068, U+2069), which an agent copies into what it writes, are the usual trigger. So Octoboard removes the
  same characters from every message before writing it and keeps only the joiner and presentation-selector cases above:
  the script-dependent ones need Unicode script data to judge, and the tag flags are too rare to be worth a rule, so
  those characters are removed too (a tag flag arrives as a plain black flag). Removing one too many costs a letterform,
  a direction mark or a flag; keeping one too many costs the message. It is encoded in
  `apps/daemon/src/term/invisible.rs`. Because a held prompt fires no `UserPromptSubmit`, that hook is also how
  Octoboard notices a write to an idle Claude Code session that was not sent, whatever held it; a write mid-turn cannot
  be checked that way, since the agent sends nothing until its turn ends (the behaviour is in "A message written but not
  sent" in `docs/product/message-delivery.md`). Codex and Grok Build were not measured: neither executable carries any
  wording about such a hold, but neither was run against one.
