# Agent CLI reference

Per-agent facts about the eight agent CLIs Octoboard launches: the hook events, what their payloads carry, what the
payloads can and cannot be correlated on, what a failing hook costs, how a project's own configuration layers around an
injected one, the mechanism and conditions for injecting Octoboard into each, what it takes to move a session's
conversation into another config directory, how each signs in a config directory without a terminal and what it answers
before that, what happens when two accounts share a session directory or the rest of a configuration directory, and how
to ask each one how much of its plan quota is left, how to start a usage window that has not begun counting, and what
differs when the daemon runs on Windows.

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
**Grok Build 1.0.67**. What "Sharing a session directory between accounts" says, and the "Codex, history segments" and
"Under a different account" notes under "Moving a conversation to another config directory", were measured on 2026-10-19
on **Claude Code 2.1.302**, **Codex 0.162.1** and **Grok Build 1.0.62**. What "Codex resume from aged rollouts" says was
measured on **Codex 0.162.1**. What "Sharing a configuration directory between accounts" says was measured on 2026-10-20
on **Claude Code 2.1.304**, **Codex 0.162.1** and **Grok Build 1.0.62**. The Grok Build chain-following note and the
per-session `config.toml` copy note in that section, including plugin switches, and the Grok Build section's notes on
what startup saves of the model and effort and on theme saves with `GROK_THEME` set, were measured on 2026-10-21 on
**Grok Build 1.0.62**. What "Signing in a config directory without a terminal" says was measured on 2026-10-19 on
**Claude Code 2.1.303**, **Codex 0.162.1** and **Grok Build 1.0.62**. What "Writing into a running session" says about
invisible characters was read from the executable of, and measured on, **Claude Code 2.1.304** on 2026-10-20; what it
says about input written before bracketed paste is turned on, and about pasted pieces that end in an image extension, on
**Claude Code 2.1.310** on 2026-10-25; and what it says about the terminal probe's bracketed-paste toggle at start on
**Claude Code 2.1.310** on 2026-10-26. What the Claude Code section's "How the global config file is saved, and what a
kill leaves behind" says was read from the executable of, and measured on, **Claude Code 2.1.316** on 2026-11-02. What
the Grok Build section's "Leader mode leaves the TUI's stop to a `SIGKILL` of the leader" note says was measured on
**Grok Build 1.0.70** on 2026-11-02. What "The agents on Windows" says was measured in November 2026 on Windows 11 23H2
on **Claude Code 2.1.318** and **2.1.319**, **Codex 0.166.0** (the stop, the hook commands and the app server also on
**0.163.0**) and **Grok Build 1.0.77**. What the Claude Code section says about the calls of one message overlapping was
measured on **Claude Code 2.1.319** on 2026-11-10, and what the Grok Build section says about the calls of one response
running side by side on **Grok Build 1.0.77** on the same day. What the Claude Code section says about its Program
Status Protocol reports and its title without activity glyphs was measured on **Claude Code 2.1.322** on 2026-11-13.
What the three hook sections say about a `PreToolUse` deny blocking a file write was measured on 2026-11-12 and
2026-11-13 on macOS on **Claude Code 2.1.322** (`claude -p`), **Codex 0.167.1** (`codex exec`) and **Grok Build 1.0.77**
(`grok -p`), against scratch directories, by a probe whose `PreToolUse` hook printed a `deny` decision with a reason.
What the Grok Build section's "When Grok writes its conversation" says was measured on **Grok Build 1.0.89**.
What the three sections say about a prompt that starts with `-`, or that names a subcommand, was measured on 2026-11-22 on **Claude Code 2.1.330**,
**Codex 0.167.1** and **Grok Build 1.0.92**, and what the Pi section says about a subcommand's name on **Pi 1.5.2**.
CLI upgrades can change the hook surface, payload fields, configuration layering and on-disk layout. Check a detail here
against the installed version before relying on it. The Antigravity CLI, GitHub Copilot CLI, OpenCode, Qwen Code and Pi
measurements name their versions in each agent’s section.

The permission-resume note below distinguishes the native implementations in **Claude Code 2.1.274** and **2.1.319**.
Grok's permission-mode startup and resume behavior was measured on **Grok Build 1.0.77** with isolated configuration,
fake credentials and no model turn.

How Octoboard's status mapping handles these traps is encoded in each provider's hook interpreter,
`apps/daemon/src/adapter/<provider>/hooks.rs`; the product behavior built on them is in the product docs.

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
  "Subagent events"). Work that finishes during a turn is left out of that turn's `Stop` even when the model has not
  been told yet, so an empty list does not mean nothing more will follow at once (measured from transcripts on
  2.1.316: two subagents resumed with `SendMessage` handed back during one turn, and its `Stop` listed nothing). Their
  `<task-notification>`s wait in Claude Code's queue — the transcript's `turn_duration` entry counts them as
  `pendingBackgroundAgentCount` — and start a new turn about 20 ms after that `Stop`. A notification that arrives while
  a turn is running is usually delivered inside it as a `queued_command` attachment, with no `UserPromptSubmit`, so a
  task list kept from the hooks alone can hold work that has already finished and been reported; the `Stop`'s list is
  the one to trust. A subagent resumed with `SendMessage` hands back twice: an `<agent-message>` prompt carrying its
  final report, which can start a turn of its own, and then its `<task-notification>`; the hook schema in the 2.1.317
  executable lists peer messages among the prompts that fire `UserPromptSubmit` (with `source` `system`; not measured),
  so a mapping that reads that event as the input box being sent has to leave both out. On 2.1.322 a subagent launched
  in the background hands back as one resumed with `SendMessage` does when it finishes, and the hand-back starts a turn
  of its own: its prompt, which carries no `<task-id>`, opens `<agent-message from="<agent id>">` with a body beginning
  `[Subagent hand-back]` (sometimes after `Another Claude session sent a message:`), and is followed by the agent's
  `SubagentStop` and the turn's `Stop`; a `<task-notification>` for the same agent can still come after it. The marker
  is what tells a completion from a message another Claude session sent, which comes in the same `<agent-message>` block
  without it. `/clear` does not end a running background shell: the next `Stop` after the `SessionStart` with source
  `clear` still listed it (measured through `-p --input-format stream-json`; the interactive TUI is unmeasured). A local
  interactive `--resume <id>` does not start a background subagent the previous process left running again: it fires
  only `SessionStart` with source `resume` and writes a `<task-notification>` with `<status>stopped</status>` to the
  transcript alone, so no `UserPromptSubmit` fires (measured on 2.1.322 and 2.1.319). An agent the model revives with
  `SendMessage` fires `SubagentStart` inside that turn and is listed in the turn's `Stop`.
- `StopFailure` — mutually exclusive with `Stop`: a turn that ends in an API error fires only `StopFailure`, so a
  mapping keyed on `Stop` alone leaves that session looking busy forever. It does not mean the session died; in the
  interactive TUI it stays alive at the prompt. It carries `error`, an optional `error_details` (the raw HTTP status
  plus the verbatim response body, not prose) and `last_assistant_message`. On this event `last_assistant_message` is
  the **user-facing error text, not model output**, so anything that reads that field as the turn's result has to treat
  `StopFailure` differently from `Stop`. `error` is not a function of the HTTP status: a plain 400 gives `unknown`, a
  400 whose body says the prompt is too long gives `invalid_request`, and a 529 gives `server_error` rather than
  `overloaded`.
- `PermissionRequest` — carries **no `tool_use_id`**, even though the binary's own embedded documentation lists one.
  The call the prompt belongs to has to be taken from a `PreToolUse`, which does carry it; not simply the immediately
  preceding one, since the calls of one message overlap (see "The calls of one message overlap" below), but the open
  call with the same `tool_name` and `tool_input`.
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

**The calls of one message overlap, and one can end while another waits on the user** (2.1.319, measured on
2026-11-10 in a PTY against a local stand-in for the Messages API, with HTTP hooks; the capture is
`apps/daemon/testdata/claude_hooks_question_beside_late_call.jsonl`). Claude Code starts a call as soon as its block
has streamed: with a background `Agent` call and then an `AskUserQuestion` in one message, `PreToolUse` `Agent` fired
on the first block, and in `default` mode the `Agent`'s `PostToolUse` followed within 80 ms, before the question's
block arrived. In `auto` mode the `Agent` call waits for the safety check, which is asked only once the whole message
has streamed, while the question is shown at once. The order was then `PreToolUse` `Agent`, `PreToolUse`
`AskUserQuestion`, `PermissionRequest` `AskUserQuestion` (a question raises one too, also with no `tool_use_id`),
and the `Agent`'s `PostToolUse` when the check answered, seconds later, with the question still waiting. The title
stayed `✳` from the question's dialog to the answer, through that `PostToolUse`, the background agent's start, and
90 s of its own tool calls and progress summaries. Answering fired the question's `PostToolUse` and then a
`PostToolBatch` listing both calls. So a post event says only that its own call ended. Which call a hand was raised
for has to be kept by `tool_use_id`, and a `PermissionRequest`'s call taken from the open calls by its `tool_name` and
`tool_input`, which it does carry. A real transcript of a session in `auto` mode on 2.1.319 shows the same order: the
`Agent` call's `tool_result` was written 1 s after the `AskUserQuestion` block, while the question waited.

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

A decline is recorded as a **user interrupt** rather than as its own kind of event, so its only machine-readable trace
is the pair of transcript records described under "A user interrupt emits no hook event" below — which is what makes the
transcript the only way to observe one.

Two traps around this, both measured:

- `PermissionDenied` **exists** in the executable's strings (alongside `emitPermissionDenied`,
  `executePermissionDeniedHooks`, and user-facing text about the hook permitting a retry) and **never fires** — not on a
  declined prompt, and not when a `PreToolUse` hook itself returns a `deny` decision.
- `--settings` takes a path to a settings file as well as inline JSON; hooks registered in such a file fire as from
  inline JSON (measured on Claude Code 2.1.322).
- `--settings` **does not validate hook names at all**: registering `PermissionDenied`, or a wholly invented name,
  starts without a word of complaint, while the same file's *permission rules* are checked loudly. So a hook name being
  accepted is no evidence the event exists, let alone that it fires.

**`AskUserQuestion` has no plain decline option.** Its list offers the answers, then `Type something.` and `Chat about
this`; Esc cancels. Esc writes the same two records as a declined permission prompt, field for field. `Chat about this`
does not: its `tool_result` opens with the same "The user doesn't want to proceed with this tool use" sentence but
continues into the user's feedback, there is **no** `[Request interrupted by user for tool use]` record, and the turn
carries on — `PostToolBatch` and `Stop` both fire. So the two exits are told apart by the second record, not by the
first.

**A user interrupt emits no hook event**, in the thinking phase or mid-tool. Only the interrupted turn is silent; the
next turn reports normally. The session's next `UserPromptSubmit` is the reliable sign that the previous turn is over
and should close the dangling `PreToolUse`. The interrupted tool's own child process is already gone. Once the model has
shown output, the only machine-readable trace is in the transcript at `transcript_path`. An interrupt in the writing
phase writes a single top-level `"type":"user"` entry whose `message.content` is `[{"type":"text","text":"[Request
interrupted by user]"}]`, carrying `interruptedMessageId` (seen on 2.1.296, 2.1.302, 2.1.303 and 2.1.310). Mid-tool, it
writes a `tool_result` with `is_error: true` beginning "The user doesn't want to proceed with this tool use", followed
by a user block `[Request interrupted by user for tool use]`. Both are entries whose `message.content` is an **array**
of blocks, and the first of them — the one holding the `tool_result` — also carries `"toolDenialKind": "user-rejected"`,
at entry level rather than inside the block; the `[Request interrupted by user for tool use]` entry carries no
`toolDenialKind`, and from 2.1.310 carries `interruptedMessageId` as well. A declined prompt and a declined
`AskUserQuestion` write this same pair, which is why one mechanism recovers all three cases. Every user entry
of a turn, both interrupt entries included, carries the turn's `promptId` (in the fixtures from 2.1.274 on), the
`prompt_id` the turn's hooks carry. Esc pressed while a message the user queued waits cancels the turn and runs the
message as the next one, with no hook at all for it (2.1.322, hook payloads captured in a PTY): its `UserPromptSubmit`
fired when it was queued, about 15 ms after its Enter and mid-turn, carrying the *running* turn's `prompt_id`. The
cancelled turn's `[Request interrupted by user]` carries the cancelled turn's `promptId`; a `queue-operation` entry of
`dequeue` is written just before it in the file, though stamped 4–11 ms after it; and the dequeued message follows
within about 25 ms as a user entry with a new `promptId` whose `parentUuid` is the marker. That turn's `Stop` fires at
its end. A message taken into the running turn at a tool boundary instead writes `queue-operation` `remove` and a
`queued_command` attachment, with no new `promptId`.

**An interrupt before the model has shown anything writes nothing** on 2.1.310 (measured in a PTY against a local stub
API, with the request held unanswered and with only `message_start` sent, and once against the real API with Opus still
in its thinking phase): no interrupt entry, no hook, and the prompt entry written at submit stays the last user entry.
Claude Code puts the prompt's text back in the input box, and submitting it again writes the prompt a second time, as a
sibling of the first (the same `parentUuid`); a bracketed paste and Enter written then submit the restored text with the
pasted text joined straight onto it, no separator between them. On 2.1.296–2.1.303 an interrupt in the thinking phase
still wrote `[Request interrupted by user]`. The one trace left is the terminal title's status glyph (see "Terminal
titles and the titles in an agent's own records" under "## Across the agents"): `◐`/`◑` alternate about once a
second while the turn runs, through tool calls and while the request waits, and the glyph turns to `✳` 70 ms after the
Esc. The same `✳` follows every `Stop` (14 ms after the hook was posted) and every `PermissionRequest` whose dialog is
shown (20–45 ms after), and stays through the dialog; after the dialog is answered the glyph turns again with no hook
until the tool's `PostToolUse`. The `◐` of a new turn came 6 ms *before* its `UserPromptSubmit` was posted. All on
2.1.310.

**From 2.1.322 the glyphs give way to Program Status Protocol reports** (measured in a bare PTY on macOS). Claude Code
sends the OSC 7501 support query at startup; once the terminal answers it, it reports its root state through OSC 7501
(`state=<state>:app=claude-code`) and drops the activity glyph from its title, which reads just `Claude Code` or the
conversation's title (`Essay about rivers`). It reports `state=idle` right after the answer; `state=working` about 55 ms
after a prompt is submitted (its `UserPromptSubmit` hook comes about 40 ms after the Enter either way); `state=idle`
about 80 ms after an Esc cancels a turn, before output or after it; `state=done` when a turn ends normally; and
`state=clear` three times at exit, followed by an empty title. A report is made only when the state changes, never
repeated. Without the answer the title keeps its glyphs: `✳ Claude Code` at the prompt, `◐`/`◑` alternating about once
a second while a turn runs, and `✳` again at its end. Whether it reports `blocked` at a permission prompt or a question
was not measured. Read from the 2.1.323 executable's bundled code, not measured: the root state is `working` while a turn
runs and also while the program is idle with a message queued, so a turn carried on from a queued message after an Esc
reports no `idle` in between; and `blocked` while it waits on a permission prompt, a sandbox request, a worker request or
a goal proposal (`kind=permission`), on input it needs (`kind=question`), or on an open dialog (no kind).

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

A pattern is matched as a URL, not as plain text (measured on 2.1.314, with the hook URL carrying the daemon's
`?key=`): the scheme and host compare without regard to case, a `*` port matches any port, a pattern with no path
(`http://127.0.0.1:*`) matches any path and query, and otherwise the pattern's path and query are matched against the
URL's path and query case-sensitively, `*` included. So `http://127.0.0.1:*/hook/*` and
`http://127.0.0.1:*/hook/*?key=*` let the hook through with its query, while `http://127.0.0.1:*/hook/<session>` (no
wildcard for the query), `http://localhost:*` and `http://127.0.0.1:*/HOOK/*` block it, each logging "HTTP hook
blocked: … does not match any pattern in allowedHttpHookUrls" to the debug log only.

**`Notification` takes a matcher on its `notification_type`** (from **2.0.37**): a hook registered with
`"matcher": "idle_prompt"` is delivered only for that type, which keeps the other notifications from reaching a
handler that reads one.

**Project configuration layering**: hooks injected through `--settings` merge with the project's
`.claude/settings.json` hooks rather than replacing them, and injected `permissions` merge with the project's.
Precedence among permission rules is by rule *kind* rather than by source — `deny` > `ask` > `allow` — so an injected
`allow` can never re-open something the project denies.

**A `PreToolUse` deny blocks a write, and so does a `deny` permission rule, on different ground** (2.1.322, `claude -p`
in `bypassPermissions`, a probe hook printing `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":
"deny","permissionDecisionReason":"<text>"}}`). A deny with a reason, from a `command` hook and from an `http` hook
alike, blocks `Edit`, `Write` and `Bash`, and the model sees `PreToolUse:<Tool> hook error: <reason>` in full. A rule
`Edit(//abs/path/**)` passed in `--settings` blocks `Edit`, `Write` and a `Bash` write whose path is literal in the
command (`echo > /abs`, `sed -i /abs`, `cd /abs && echo > rel`) in `bypassPermissions`, `acceptEdits` and `auto`, but
not `python3 -c "open('/abs','w')"`, a path built from a variable, or a script run with `sh`. Its message is the generic
"File is in a directory that is denied by your permission settings", and when a rule and a hook deny the same call the
model sees only that generic message, not the hook's reason. An `http` hook whose URL is unreachable lets the call
through (fail open), silently. After a deny the model did not try to write the file another way.

**What Claude Code reads from other agents' layouts** (2.1.319, measured with `claude -p --model haiku
--no-session-persistence` in scratch git repositories):

- A project's `AGENTS.md` is read when the project has no `CLAUDE.md` (a fallback) and not when a `CLAUDE.md` exists;
  `~/.codex/AGENTS.md` is never read. The executable carries an `instructionFiles` setting (`claude-md`,
  `claude-md-or-agents-md` (the default), `claude-md-and-agents-md`, `managed-only`; short forms `claude`,
  `agents-fallback`, `both`, `none`), but setting it through `--settings` or `.claude/settings.local.json` changed
  nothing, so it is probably gated.
- `.agents/skills` and `.codex/skills` are not read.
- `--plugin-dir <dir>` loads a directory that has no plugin manifest: its `skills/*/SKILL.md` appear as
  `<dirname>:<skill>`, and symlinked skill directories are followed.
- In a `--mcp-config` JSON string, `${VAR}` and `${VAR:-default}` are expanded in a stdio server's `args` and `env`
  values and in an HTTP server's `headers`. A `--mcp-config` server replaces a project `.mcp.json` server of the same
  name, and the project one is never started.
- It ships `/import codex|gemini|cursor`, which writes the other agent's configuration into Claude Code's own files.

**An MCP server's tools are pre-approved by a rule naming the server alone** (Claude Code 2.1.310, real PTY, stub
inference and stub stdio servers whose unannotated tools logged every `tools/call`). `"permissions":{"allow":
["mcp__<key>"]}` in the `--settings` JSON runs every tool of the server registered under `<key>` with no prompt, in
`default` mode, in `auto` mode (where it also keeps those calls away from the auto-mode classifier, which otherwise
judges them) and on `--resume`; without the rule the same call prompts in `default` mode. It is an exact match on the
key, not a prefix: servers keyed `<key>2` and `<key>_x` still prompted, as did Bash. Allow rules in the user's and the
project's settings files still applied alongside it, and their `deny` or `ask` for one of the server's tools still won —
a `deny` removes the tool from the model's view entirely (ToolSearch no longer finds it). `mcp__<key>__*` is accepted
and behaves the same, and is the form Claude Code's own settings warning suggests; neither form raises a warning. MCP
tools are deferred behind ToolSearch in this build, so a scripted stub has to call `ToolSearch` (`select:<tool>`)
before the tool itself; ToolSearch never prompts. A fresh config directory with no `defaultMode` started in `auto` mode
on 2.1.310 too.

**`--mcp-config` takes a list of values, not one.** Every non-flag argument after it is read as a
further config path, resolved against the cwd: `claude --mcp-config '<json>' mcp list` fails with
`MCP config file not found: <cwd>/mcp`. Anything positional — the task prompt in particular — has
to come before it, or after a bare `--`, which closes the list.

**A prompt that starts with `-` is taken only after a bare `--`** (2.1.330, measured on macOS on 2026-11-22). As a
bare argument, in any position, it is refused before anything starts, with exit code 1: `claude '--help me with this'
--session-id <id>` prints `error: unknown option '--help me with this'`, `'- a list'` gives `unknown option '- a
list'`, and `'-p x'` gives `unknown option '- x'`. After `--` the next argument is the prompt whatever it starts with,
and the `--` also ends `--mcp-config`'s list: `claude -p --session-id <id> --mcp-config '<json>' --append-system-prompt
<text> --permission-mode default --model <id> --effort medium -- '--help me with this'` recorded the user message
`--help me with this` in its transcript, and `- a list` and `-p x` were recorded the same way (print mode, a scratch
`HOME`, a fake API key and `ANTHROPIC_BASE_URL` on a closed local port, so no request left the machine). Without the
`--`, `--mcp-config '<json>' 'plain task'` fails with `MCP config file not found: <cwd>/plain task`, and Claude Code's
own message says "put a prompt before --mcp-config or after --". Everything after the `--` is positional, a flag
included, so the `--` and the prompt come last: in `claude -- <prompt> --session-id <id>` the flag is not read.

The interactive launch takes it the same way (2.1.330, on a pseudoterminal under a scratch `HOME` with onboarding, the
API key and the folder's trust pre-answered in its `.claude.json`, the same fake key and closed port): `claude
--settings <file> --mcp-config '<json>' --session-id <id> -- '--help me with this'` drew `❯ --help me with this`, ran
the `UserPromptSubmit` hook with `prompt` `--help me with this` and went on to retry the refused connection. A resume
does too, with no "Resume this conversation?" dialog: `claude … --resume <id> -- '- carry on'` submitted `- carry on`
at once.

**A command-line prompt that holds invisible characters is cleaned and sent, not held** (2.1.330, same pseudoterminal
and scratch `HOME`). Unlike a prompt pasted into the input box, which Claude Code cleans and leaves there unsent
("Writing into a running session"), `claude … --session-id <id> -- $'## Goal\n\n⁨x⁩ do it'` started the turn
at once, with the notice "Removed 2 invisible characters from the launch prompt before sending it"; the
`UserPromptSubmit` hook's `prompt` was the cleaned text, `## Goal\n\nx do it`. A zero-width space (U+200B) and a
right-to-left override (U+202E) were removed the same way. So the prompt the hook reports differs from the argument
by those characters. The set is the one the input box's check has ("Writing into a running session"): of 62
characters sampled in one prompt, the ends and middle of every range Octoboard removes and twenty blank-looking
neighbours outside them (U+00A0, U+2000, U+202F, U+3000, U+2800, U+FFFC among them), each was removed or kept as
Octoboard's own strip does, U+2028 and U+2029 becoming newlines. The joiner of `🧑‍💻` and the selectors of `⚠️` and
`1️⃣` were kept; a joiner between Devanagari letters was kept too, which Octoboard removes, and a right-to-left mark
between two Hebrew letters was removed. A subcommand's name wrapped in them is still a prompt: `-- $'⁨mcp⁩'`
started a turn whose hook `prompt` was `mcp`, and so did `-- $'\u200bdoctor'`, a single zero-width space ahead of the
word, with the notice "Removed 1 invisible character from the launch prompt before sending it" and the hook `prompt`
`doctor`: the characters are removed where the prompt is sent, after the arguments are read. Octoboard passes a task
as it was written, these characters included, since some are part of a script's spelling (the zero-width non-joiner of
Persian, a joiner that picks a Devanagari conjunct's form) and Claude Code keeps those; it matches the prompt the hook
reports against the task with the invisible characters ignored on both sides.

**An argument that names a subcommand runs it, after `--` too, unless `-p` is given** (2.1.330, same scratch `HOME`).
`claude --session-id <id> --settings <file> -- mcp` printed `Usage: claude mcp [options] [command]` and exited 1, with
and without the `--`; `claude --session-id <id> --settings <file> --mcp-config '<json>' -- doctor` ran `claude doctor`;
on the pseudoterminal `claude … --session-id <id> -- mcp` printed the same usage and exited, with no
`UserPromptSubmit`. Ahead of the flags the subcommand gets them: `claude mcp --session-id <id>` and `claude mcp
--resume <id>` fail with `error: unknown option`. So a prompt that is exactly `update` or `install` would run that
subcommand (not run here: both change the installation). Only the whole argument is matched: `-- 'mcp list'` and `--
'mcp '` (a trailing space) are prompts, and so is `-- ' mcp'`, with a leading space. That space is kept, not trimmed:
the transcript's user message and the `UserPromptSubmit` hook's `prompt` are both ` mcp`, in print mode and on the
pseudoterminal (`❯ mcp` on screen), and on a resume (`--resume <id> -- ' doctor'` gave the hook ` doctor`). With `-p`
ahead of it nothing is dispatched: `claude -p --session-id <id> -- mcp`, and the same without the `--`, recorded the
prompt `mcp`. The subcommands 2.1.330 lists are `agents`, `attach`, `auth`, `auto-mode`, `doctor`, `gateway`, `import`,
`install`, `logs`, `mcp`, `plugin` (`plugins`), `purge`, `respawn`, `rm`, `setup-token`, `stop` (`kill`),
`ultrareview` and `update` (`upgrade`). Octoboard passes a prompt that is one word of ASCII letters, digits, `-` and
`_` with a leading space, whichever subcommands the installed version has.

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

### Model and reasoning effort

The model is passed as `--model <id>` and the effort as `--effort <level>`, both global flags, so they work alongside
`--resume`. They sit at the end of the flag list, after `--append-system-prompt`, so nothing non-flag follows a
list-valued flag such as `--mcp-config`. On a model that does not support `max` or `xhigh`, Claude Code **silently
lowers the level to `high`** rather than refusing it. The level a reply ran at is always on record: each main-transcript
assistant line carries it as a top-level `effort`, also when nothing sets one — every such line of 155 transcripts
written by 2.1.286 to 2.1.304, on a `settings.json` with no `effortLevel`.

### Discovering models

Measured on Claude Code 2.1.296, the account catalogue read off disk on 2.1.317: there is **no `models` subcommand**,
but there is a real alias layer. `fable`, `opus`, `sonnet` and `haiku` resolve to the newest member of each family
through the catalogue Claude Code fetches from its server (fable-5-1, opus-5-5, sonnet-5-5 and haiku-5-5 when measured),
so no version has to be written down. `best` means "the top model if the account has it, else Opus", but it falls back
to Opus **without checking that the account may use Opus**, so it is not safe to rely on: a Pro account measured here
has both `fable` and `opus` disabled. A model the account cannot use is **refused** at launch with
`Model 'X' is not currently available for your account.` followed by the catalogue's notice. `latest` is a reserved word
and **not an alias**; `mythos` has no alias. The list Octoboard offers is therefore the four strings `fable`, `opus`,
`sonnet`, `haiku`, best first, and an unset setting launches with the first of them the account may use, unless Claude
Code's own configuration names a model (below). `--effort` takes `low`, `medium`, `high`, `xhigh` and `max`, and a level
the resolved model lacks is silently lowered, so no level is ever refused.

Claude Code caches the account's catalogue on disk, which says what each alias resolves to and whether the account may
use it, without running anything. It is `<config dir>/cache/model-catalog/<organizationUuid>-<hash>-cc.json`, where
`<config dir>` is `CLAUDE_CONFIG_DIR`, else `~/.claude` (the default account's catalogue is under `~/.claude/` although
its global config is `~/.claude.json`), `<organizationUuid>` and the account id come from the global config's
`oauthAccount.organizationUuid` and `oauthAccount.accountUuid`, both lower-cased, and `<hash>` is the first 12 hex
digits of the SHA-256 of the lower-cased account id. Several organizations' catalogues can share one directory, so the
organization is part of the name. Beside them lie `*-ccd.json` and `*-headless-failed.json` files, which say nothing
about the models and are not read. The file is `{version: 2, fetchedAt, staleAt, catalog}`, and `catalog.config` holds
`models`, a list of `{id, name, short_name, section, disabled?, notice?, runtime}`, and `provider_alias_targets`, a map
from each alias to `{default: <id>}` (and, as measured on gateway, Bedrock and Vertex accounts, `per_provider` targets,
which Octoboard does not use: only `provider_alias_targets[alias].default`, the first-party OAuth target, is read). The
rules: an alias resolves to the `models` row whose `id` is `provider_alias_targets[alias].default`; that row's `name` is
the version to show (`Opus 5.5`); the account may use it unless `disabled` is `true` or `section` is `deprecated` (a
disabled row's `notice.text` is Claude Code's reason, such as "Fable is included with Max and Ultra, or available with
usage credits on Pro."; an enabled one may carry a notice too, a usage warning, which is not a refusal); and
`runtime.effort_levels` lists the levels. Max and Team accounts measured here have all four enabled, Pro ones have
`fable` and `opus` disabled. A file past its `staleAt` is still the best answer there is. The global config's
`modelAccessCache` (`[{apiName, entitled}]`, empty on every account measured) is honoured as a further refusal where an
entry says `entitled: false`; that `apiName` is the catalogue row's `id` is an assumption, unverified because no
measured account has an entry. `additionalModelOptionsCache` is not reliable and is ignored.

### Claude Code's own model configuration

From Claude Code's documented settings, not measured here: with no `--model`, Claude Code takes the model from
`ANTHROPIC_MODEL` in its environment, else the `model` of the merged settings files, whose precedence from highest is
the managed settings, the project's `.claude/settings.local.json`, the project's `.claude/settings.json` and the user's
`<config dir>/settings.json`. A settings file's `env` block is applied to Claude Code's environment, so an
`ANTHROPIC_MODEL` in it outranks any `model`; Octoboard reads it from the same files, higher file first. The managed
settings are `managed-settings.json` and the `*.json` files of `managed-settings.d/` (merged in the order of their
names, after the base file) in `/Library/Application Support/ClaudeCode` on macOS and `/etc/claude-code` on Linux;
policy delivered through an MDM profile or the claude.ai console is in no file and cannot be seen. Octoboard counts any
of these as the model being set upstream: it then passes no `--model`, so that Claude Code applies it (a `--model` would
override all of them). Only when none names a model does the launch pass the first usable alias. The account-level
sources (the user's settings, `ANTHROPIC_MODEL` in the login-shell snapshot or a settings `env` block, the managed
settings) are also what the listing's `default_model` reports, as the raw string, with
`default_model_source: configured`; the project's are known only at launch, so a session in a project that sets a model
can run another one than a picker showed. Known gap: a managed `availableModels` allowlist is not read, so a launch with
a best-available alias it excludes is refused by Claude Code.

### Permission mode

Measured on Claude Code 2.1.303: `--permission-mode <mode>` is a global flag, so it works alongside `--resume`
(including `bypassPermissions` with the `--settings` key, which showed no warning on a resume). The listed values are
`acceptEdits`, `auto`, `bypassPermissions`, `manual`, `dontAsk` and `plan`; `default` is still accepted and equals
`manual`, and is what the hooks and the transcript report. `manual` exists only from 2.1.200, so Octoboard passes
`default` for "ask me".

- **`bypassPermissions` blocks on each config directory's first use.** After onboarding or login and the trust screen
  it shows a "Bypass Permissions mode" warning with the cursor on "No, exit" (Enter exits with status 1, Esc with 0); no
  hook fires while it is up. Accepting writes only `skipDangerousModePermissionPrompt: true` to
  `$CLAUDE_CONFIG_DIR/settings.json`. The same key in `.claude.json` does not suppress it.
  `--settings '{"skipDangerousModePermissionPrompt":true}'` suppresses it and writes nothing, which is what Octoboard
  passes (merged into the settings file it names with `--settings`); background and `claude agents` sessions do not
  honour a flag-only consent, which is irrelevant for a PTY launch.
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

### Permission mode on resume without an explicit flag

Native implementation inspected in 2.1.274 and 2.1.319. An interactive direct resume can restore a stored mode instead
of simply reading the account's current default. It does not restore stored `bypassPermissions`; restoring `auto`
depends on current availability, and a stored Manual mode can yield to a configured default. The newer implementation
separately restores an unexited plan on an interactive resume when no explicit mode or fork was requested; the older
implementation lacks that path. An explicit `--permission-mode` prevents the saved mode from replacing the launch's
selection. These are source observations, not additional runtime probes. The official [permission mode on
resume](https://code.claude.com/docs/en/sessions#permission-mode-on-resume) reference documents the conditions for
direct terminal resume, the picker, `/resume` and non-interactive resume.

### Dialogs at a resume and in an idle session

Measured on 2.1.307, read from the executable and reproduced with fake subscription credentials against a local stub.
None runs a hook, and `idle_prompt` is not sent while one is up.

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

What Octoboard does about the dialogs above and the screens below, the notice that waits for Enter, the auto-mode nudge,
the computer-use access request and the views of background agents, is in "Messages held until a session can take them"
in `docs/product/message-delivery.md`.

### A notice that waits for Enter

Measured on 2.1.308: in `auto` mode with `ANTHROPIC_BASE_URL` pointing elsewhere, the first classifier call of a session
raised "We're changing auto mode to no longer charge for classifier requests …" mid-turn, ending "Enter to continue ·
Esc to cancel" (the rewind dialog's footer too), with no hook. A message written then was lost: its Enter dismissed the
notice and the text went nowhere.

### "Make auto mode your default permission mode?"

Measured on 2.1.310, read from the executable and reproduced with a fake key against a local stub. Offered once per
launch while the config directory's `.claude.json` has no `hasSeenAutoDefaultNudge`, its user settings set
`permissions.defaultMode` to anything but `auto` (a fresh directory, with none set, never shows it), no project, local,
flag (`--settings`) or policy settings set a default mode, auto mode is available for the model, and the server flag
`tengu_maple_pier` is on. Whatever `--permission-mode` the launch passes does not matter. It comes up under the
transcript in place of the prompt box about 50–170 ms after the box draws, or about 20 ms after the first turn's `Stop`
when the launch carried a prompt; while a draft is in the input it waits, behind a row reading "Claude has a question
for you — it shows once you send or clear what you're typing.". Body text, then unnumbered rows "Yes, set auto mode as
my default permission mode" (focused) and "No, keep <mode>" ("manual mode", "accept edits"), and **no footer**. No hook
fires before, during or after it. A bracketed paste is dropped and a CR picks the focused row; an Enter within 150 ms of
it mounting is ignored. Yes switches the session to auto, writes `permissions.defaultMode: "auto"` into the user
settings and sets `hasSeenAutoDefaultNudge`; No and Esc only set `hasSeenAutoDefaultNudge`.

### The computer-use access request

Read from the 2.1.325 executable and a user's screenshot, not reproduced. The built-in computer-use MCP server's
`request_access` raises "Computer Use wants to control these apps" (the same title for one app) from inside the running
tool call: `PreToolUse` has already moved the session to working, no `PermissionRequest` comes, and the title glyph goes
back to `✳` while it waits. Below the reason and the apps it shows "N other apps will be hidden while Claude works." and
two options, "Deny, and tell Claude what to do differently (esc)" (focused by default) and "Allow for this session (N
app[s])", with printable keys off; the screenshot's footer reads "Enter to confirm · Esc to cancel". A paste is dropped
and an Enter picks the focused option. When Accessibility or Screen Recording is not granted it shows "Computer Use
needs macOS permissions" in its place, in the same frame, with each permission's state, "Grant the missing permissions
in System Settings, then select 'Try again'." and rows "Open System Settings → Accessibility", "Open System Settings →
Screen Recording" and "Try again" (read from the executable only; its footer is taken to be the same). Claude Code may
take a moment after the answer to show its turn running again (not measured).

From 2.1.328 (read from the 2.1.328 and 2.1.329 executables, and a user's 2.1.329 screenshot) the old title is gone:
the request reads "Claude wants to use <app>" when it asks for one app (for example "Claude wants to use Simulator",
with the app row "● Simulator (full access)"), "Claude wants to use these apps" for several, and "Claude wants these
permissions" when it asks for no app. Full access adds the line "With full access, Claude can change or delete files on
your computer without asking first."; the options and the footer are unchanged. The permissions screen is titled
"Computer use needs macOS permissions" on 2.1.329, with a lowercase "use". The tool call is open throughout (its
`PreToolUse` came, its `PostToolUse` comes only after the answer), which is what keeps the title's cancel watch off it
whatever the title says.

### Views of background agents

Measured on 2.1.307, reproduced against a local stub. A turn that starts an `Agent` with `run_in_background` adds a task
list under the prompt box — a filled `⏺ main` row and a row per agent — and the hint "↓ to manage". With the footer
focused (a hint row of its own: "↑/↓ to select", or "Enter to view · x to stop" while an agent's row is selected) a
paste is dropped and a CR opens the agent's view. **In that view** the conversation area shows the subagent's
transcript, the `❯` prompt box stays (its placeholder reads "Message @<agent type>…"), and the main row turns hollow,
`◯ main`; on 2.1.309, selecting that row without leaving the view marks it `❯ ◯ main` and shows no hint (seen in a
user's screenshot, not reproduced). Text sent there goes to the subagent, with no `UserPromptSubmit`; sent to a finished
agent it resumes it. The view stays up after the agent finishes. No single key leaves it safely: Esc in the input stops
a running agent, Ctrl+C kills it, and ← at an empty input opens the agents view; ↓ to the main row and Enter returns to
the conversation with the footer still focused. **The agents view** (`claude agents`, ← at an empty prompt, hinted as "←
for agents" for the first ten seconds) moves the conversation into a background session forked under a new session id,
starts a transient `claude daemon`, and shows "New sessions: …", "0 awaiting input · 1 working · 0 completed" and
"describe a task for a new session": a paste and CR there start another session with that text.
`CLAUDE_CODE_DISABLE_AGENT_VIEW=1` (setting `disableAgentView`) removes the agents view and its hint and leaves the
subagent view.

### Background sessions and resuming

Measured on 2.1.329, in a PTY with a real account. A background session (`claude --bg`) runs a conversation under a
process of its own and outlives the terminal that started it; `claude --bg --resume <id>` continues an existing
conversation that way **under the same id**, whereas ← into the agents view forks it under a new one (above), leaving
the original free. While a background session holds a conversation, a bare `claude --resume <id>` attaches to it
("Opening the background session (<short id>)…"), but a resume with further flags (`--append-system-prompt`, as every
Octoboard launch passes) prints "That session is running in the background (<short id>). Run `claude attach <short
id>` to open it, or `claude stop <short id>` first to resume it here. Add --fork-session to branch off a copy instead."
and exits 1.

- `claude agents --json` lists the account's sessions (it follows `CLAUDE_CONFIG_DIR`; another directory lists none),
  as a JSON array answered in about 0.15 s. A background entry has `kind: "background"`, a short `id` (the first eight
  characters of the conversation id), `sessionId`, `state`, `status`, and a `pid` while it runs, including idle after
  its turn (`state: "done"`, `status: "idle"`); a stopped one stays listed without `pid`. An interactive entry has
  `kind: "interactive"`, a `pid` and **no `id`**. `--all` adds completed background sessions. With
  `CLAUDE_CODE_DISABLE_AGENT_VIEW=1` it exits 1 at once ("'claude agents --json' is disabled by …"). Captures are in
  `apps/daemon/testdata/claude-agents-captures.md`.
- `claude stop <short id>` stops a background session, keeping its conversation, and exits 0 (also for one already
  stopped); it does not accept the full conversation id, and an unknown id exits 1 ("No session matching …"). After
  the stop, the flagged resume succeeds.

### Keys that interrupt a turn

Measured on 2.1.310, in a PTY with a real account. One Esc stops a turn, about 160 ms after it while the model writes;
what that leaves in the transcript and the title is under "A user interrupt emits no hook event" above, and an Esc
before the model has shown anything puts the prompt back in the input box. Esc during a tool, with a message typed into
the queue meanwhile, interrupts the turn and then takes the queued message as a new turn, whose `UserPromptSubmit` had
fired when it was queued; a message queued while the model was still writing was also seen taken into the running turn
before the Esc. Text typed into the input box mid-turn stays there after the Esc. An Esc written in the same write as a
bracketed paste still worked. Ctrl-C mid-turn interrupts as Esc does, but a single Ctrl-C at the prompt arms "Press
Ctrl-C again to exit". Ctrl-U deletes one line of the input box a press, and at the start of a line the line break
before it, so a three-line prompt took six presses to empty; the box then shows "Ctrl+Y to paste deleted text". A fresh
session's empty box shows a placeholder such as `❯ Try "refactor <filepath>"`, which reads like text.

### Permission prompts, questions and the slash popup on screen

Measured on 2.1.328 (120x40 PTY, stub API; screens in `apps/daemon/testdata/claude-interaction-captures.md`). Hooks
say what a call is (`PreToolUse` with `tool_use_id`, then `PermissionRequest` without it); the options exist only on the
screen. A prompt is a solid rule, a title, the call between dashed rules, `Do you want …?`, numbered options and, for
Bash, Write and Edit, a footer `Esc to cancel`, with `· Tab to amend` only while the highlight is on the first or last
row and no field is open. Options that switch the mode (`Yes, and switch to auto mode`, `… accept edits … (shift+tab)`)
sit among the others. A digit picks at once; Tab opens a text field on the first or last row, whose empty placeholder is
faint (a screen with faint cells blanked reads `Yes,` / `No,`); the `don't ask again for: <rule>` row is a field too.
WebFetch has no footer and no amend. A plain No and Esc leave no hook. `AskUserQuestion` replaces the box with chips
(`☐`, `☒`, `✔ Submit`), the question, options, a `Type something` row (single-select `3. Type something.`, focused and
empty `3.`; multi-select `4. [ ] Type something`, which typing checks), a rule and `Chat about this`; a multi-select has
an unnumbered `Next`/`Submit` row and a review page (`1. Submit answers`). The slash popup is drawn above the box's top
rule with its highlighted command marked `  ❯ /name`; pasted `/probe` + Enter ran `/probecmd`. Not measured: a digit
pressed in a focused text field or the rule row, Down past the last row, an MCP prompt.

### How the global config file is saved, and what a kill leaves behind

Read from the 2.1.316 executable; the write count measured. `.claude.json` is saved under a lock on `<file>.lock`,
retried with a backoff of about 0.2 to 4 seconds a try; when the lock cannot be had, the save goes ahead unlocked. The
save writes `<file>.tmp.<pid>.<12 hex digits>` (created exclusively), syncs it to disk, closes it and renames it over
`<file>`. Every error it can catch removes the temporary file, a failed rename included, which falls back to writing
`<file>` in place. So a temporary file is left behind only by a process that dies between creating it and the rename: an
empty one when it died before the write, a complete copy of the config when it died after it. Nothing removes such
leftovers later, so they accumulate beside `.claude.json` in each configuration directory. The file is saved often,
startup included: one print-mode run in a fresh configuration directory saved it nine times within about 160 ms, the
last about 15 ms after its deferred startup work began, which is the moment a short headless run answers. A process that
saves on exit is likewise exposed to a kill. On `SIGTERM` or `SIGINT` Claude Code runs its own shutdown, which races its
cleanup against two seconds and then exits; a failsafe timer forces the exit after at least five seconds (more when a
`SessionEnd` hook has a longer timeout). Several processes sharing one configuration directory do not by themselves
leave temporary files: each names its own, and the rename within one directory is atomic.

### A rewind in the transcript

Surveyed on 162 local main transcripts of 2.1.286 to 2.1.328. A transcript is a tree joined by `parentUuid` (falling
back to `logicalParentUuid` where `parentUuid` is null, as on a `compact_boundary`), not a list: the results of parallel
tool calls hang on side branches of it, so walking back from the file's last line does not give the conversation. A
rewind, and a prompt submitted again after an interrupt that wrote nothing, show as a new human prompt whose `parentUuid`
ancestry does not contain the previous human prompt; the abandoned lines stay in the file. Of 1,288 prompts after a
transcript's first, 1,256 descended from the prompt before them and 32 did not, every one of the 32 an abandoned prompt.
The live conversation feed's Claude Code reader drops the entries after the new prompt's parent line on such a fork.

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
- **There is no notification event, and a question has no `PermissionRequest`.** Plan mode's `request_user_input` fires
  `PreToolUse` (carrying the questions) and, once answered, `PostToolUse`; see "Codex 0.167.1: prompts on screen". The
  features `default_mode_request_user_input` and `request_permissions_tool` are off in this build.
- **There is no failure event either**, no counterpart to Claude Code's `StopFailure`. A turn that ends in an error
  fires `UserPromptSubmit` and **nothing else** — no `Stop`, no `Interrupt`, nothing — while Codex returns to its
  prompt about 0.7 s later and the terminal goes completely silent. Measured with an account at its usage limit,
  three sessions, every turn, with a wrapper logging every hook invocation. Esc pressed on such a session fires
  nothing either, so there is no event anywhere that ends the turn.

**A turn's own outcome is in Codex's session record**, the rollout file whose path every hook payload carries as
`transcript_path` (its naming and location are in "Moving a conversation to another config directory" below). Each turn
is an `event_msg` line whose `payload` is `task_started`, then one whose `payload` is `task_complete`, both carrying the
same `turn_id` as that turn's hook payloads. A turn that **failed** has `last_agent_message: null` and an `error` object
(`message`, and `codex_error_info` — `"usage_limit_exceeded"` for the measured cause); a turn that **ended normally**
has `last_agent_message` set and no `error` key at all (its keys are exactly `completed_at`, `duration_ms`,
`last_agent_message`, `started_at`, `time_to_first_token_ms`, `turn_id`, `type`). A manual `/compact` turn ends with a
null message, no `error` and no `time_to_first_token_ms`, so a null message alone does not mean a failure. An
interrupted turn — Esc, Ctrl+C or a declined approval — has no `task_complete`: it ends with a `turn_aborted` record
(`reason: "interrupted"`, measured on 0.163.0). The file is created about 0.25 s **after** the first prompt of a session
— not at launch — and Codex flushes it per record: a failed turn's `task_complete` was readable 1.26 s after the prompt,
with the process still running. The record can land before that turn's hook callback does: the margin measured on a
session's first turn was 0.249 s.

**The user's own configuration can remove the prompt entirely.** With `approvals_reviewer = "auto_review"` Codex
resolves an approval request itself: the hook fires, no modal is shown and the tool proceeds. A raised hand would ask
the user to answer something they never see.

**Permission presets (0.166.0).** The native UI selects built-in permission profiles together with an approval
policy and reviewer:

| Preset | `default_permissions` | `approval_policy` | `approvals_reviewer` |
|---|---|---|---|
| Ask for approval | `:workspace` | `on-request` | `user` |
| Approve for me | `:workspace` | `on-request` | `auto_review` |
| Full access | `:danger-full-access` | `never` | `user` |

Each value can be supplied with `-c`. A session-level `default_permissions` selects the built-in profile ahead of
lower-layer legacy sandbox settings, without inheriting their extra writable roots or network access. Combining it
with `--sandbox` or `--approve-for-me` would select legacy sandbox configuration instead. These rules were checked
against the matching release's `utils/approval-presets/src/lib.rs` and `core/src/config/mod.rs`, and exercised through
offline `debug prompt-input` initialization with the adapter's hook, MCP, instruction and terminal overrides. All
three presets and no override were checked against permissive legacy settings and a named custom profile: eight
cases, without a model turn. Workspace presets produced restricted network access and excluded the legacy extra
writable root; full access produced unrestricted permissions; no override used the account configuration.

**Plain resume preserves saved permissions when no explicit preset is supplied (0.166.0).** Direct launch overrides
for these three keys replace the saved choices; omitting them lets Codex restore the destination thread's permissions.
Clearing a launch preset therefore does not reset a resumed thread to its account's current configuration, and even
`--profile` alone does not force a reset. This is established by `tui/src/resume_permissions.rs`,
`tui/src/app_server_session.rs` and `tui/src/resume_permissions_tests.rs` in the matching release; the offline
initialization probe does not exercise a live thread resume.

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

**A synchronous `PreToolUse` deny blocks a write** (0.167.1, `codex exec` in `workspace-write` with
`--dangerously-bypass-hook-trust`, a probe command hook printing a `deny` decision with a reason). It blocks
`apply_patch` and shell commands. The payload of an `apply_patch` call has `tool_name` `"apply_patch"` and
`tool_input.command` holding the patch text, whose paths are relative to the payload's `cwd`; a shell call has
`tool_name` `"Bash"`. The model sees `Command blocked by PreToolUse hook: <reason>. Command: <command>` and did not
retry the write another way. A hook command that does not exist lets the call through.

**Project configuration layering**: project hooks in `<cwd>/.codex/hooks.json` merge with injected ones;
`<cwd>/AGENTS.md` is concatenated *after* `~/.codex/AGENTS.md` inside a single instructions block, separated by a `---
project-doc ---` line, so the user's global file survives; skills are discovered as set out below.
`<cwd>/.codex/config.toml` is layered over `CODEX_HOME/config.toml` once the project is trusted
and ignored before that (measured with `codex doctor --json`, 0.162.1), and a `-c` override outranks both: on 0.162.1 a
trusted project's `[tui] terminal_title = []` stopped every terminal title, and `-c tui.terminal_title=[…]` brought them
back.

**Codex's skill discovery** (0.166.0, `codex debug prompt-input`): a skill is found at any depth under a root, is not
deduplicated by name, and takes its name from its front matter. The roots are `<cwd>/.codex/skills`,
`<cwd>/.agents/skills`, the repository root's `.codex/skills` and `.agents/skills`, `$CODEX_HOME/skills` (including
`.system`) and `~/.agents/skills`; a `.agents/skills` above the git root is not read. `[[skills.config]]` with
`enabled = false` removes only the skill it names. Project skills load whatever the project's trust.

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

Mid-session, close to a rate limit, Codex draws "Approaching rate limits" / "Switch to <model> for lower credit usage?"
with "1. Switch to …", "2. Keep current model" and "3. Keep current model (never show again)" after a turn, with no
title change and no hook; a paste and CR switched the model and the message was never sent. `-c
notice.hide_rate_limit_model_nudge=true` keeps it away. What Octoboard passes against these screens is in "Codex" in
`docs/product/per-agent-launch-specifics.md`, and how it holds its writes while one is up in "Messages held until a
session can take them" in `docs/product/message-delivery.md`.

**Two visible side effects of passing any `-c` override**: it forces embedded mode, so the TUI carries a permanent
`⚠ 1 warning` badge explaining that command-line overrides require it, and the session is invisible to `codex agents`.

**A prompt that starts with `-` is taken only after a bare `--`, which comes after every option** (0.167.1, measured on
macOS on 2026-11-22 with a scratch `CODEX_HOME`). As a bare argument it is refused at argument parsing, with exit code
2: `codex '--help me with this'` and `codex '--definitely-not-an-option'` print `error: unexpected argument '…' found`
(the second with the tip "to pass '--definitely-not-an-option' as a value, use '-- --definitely-not-an-option'"),
`codex '- a list'` prints `unexpected argument '- ' found`, and `codex '-p x'` reads `-p` as `--profile` and refuses
its value ` x`. After `--` the prompt passes the parser, on a plain launch (`codex -c x=1 -- '--help me with this'`)
and on a remote terminal (`codex --remote ws://127.0.0.1:<port> -- '--help me with this'`): without a terminal both get
as far as `Error: stdin is not a terminal`, where the same remote launch without the `--` is refused as an unexpected
argument. On a pseudoterminal, `codex --no-daemon -c … -- '--help me with this'` with a dummy API key and a model
provider on a closed local port wrote a rollout whose user message is `--help me with this`. An option after the prompt
is not read as one: `codex -- '--help me with this' -c x=1` fails with `unrecognized subcommand '-c'`.

**After the `--`, the name of a subcommand is a prompt** (0.167.1, same scratch `CODEX_HOME`). As a bare argument it
runs the subcommand: `codex -c x=1 completion` printed the completion script and `codex -c x=1 features --help` the
help of `features`, so a first task that was exactly `logout` or `update` would have run it. After `--` it does not:
`codex -c x=1 -- completion`, `codex -c x=1 -- features` and `codex --remote ws://127.0.0.1:<port> -- logout` all get
as far as `Error: stdin is not a terminal` without a terminal, and on a pseudoterminal `codex --no-daemon -c … --
completion` wrote a rollout whose user message is `completion`.

### Model and reasoning effort

The model has a flag, `-m <model>`; the effort has **none** and is reachable only as the config override
`-c model_reasoning_effort=<level>`. Octoboard passes both as `-c` overrides (`-c model="<id>"`) so the two are set the
same way, ahead of the positional task, which Codex takes last, after a bare `--`. Codex **rejects an effort the chosen model does not
advertise**, with an error specific to that model, rather than lowering it.

### Discovering models

Measured on Codex 0.160.0: Codex has **no alias, family or "latest" indirection**, so a model is only ever its catalogue
slug. `codex debug models` prints the catalogue as JSON, `{"models": [...]}`, after refreshing it over the network
(about 600 KB, because every entry carries its full instructions); `--bundled` skips the refresh and prints the built-in
fallback. The fields that matter per entry: `slug` (what `-c model=` takes), `display_name`, `visibility` (`list` or
`hide`; hidden entries are internal or retired and not for a picker), `priority` (ascending is display order), and
`supported_reasoning_levels`, a list of `{effort, description}` whose `effort` values are exactly what
`model_reasoning_effort` accepts for that model (a level set can include ones Claude Code has no word for, such as
`ultra`), with `default_reasoning_level`. **The server's answer arrives sorted by `priority`; the bundled fallback does
not** (it was observed in an unrelated order), so sort it yourself. The command reads `CODEX_HOME`, so a catalogue is
that account's. **An unknown slug is not rejected at launch**: the session boots and fails only at its first turn, which
is why a list that is wrong cannot be left to the launch to catch.

### The effort a turn runs at when nothing sets one

Read off existing records of Codex 0.162.1 on 2026-10-20, no turn spent: a rollout's `turn_context` payload carries the
effort the configuration resolved to, and **leaves the `effort` key out** when no layer sets `model_reasoning_effort` —
335 such records on the account measured, none carrying `null`. Codex then runs the model at its catalogue entry's
`default_reasoning_level`. The catalogue is cached per account as `models_cache.json` in `CODEX_HOME`, in the
`{"models": [...]}` shape `codex debug models` prints, next to `fetched_at`, `client_version`, `etag` and `identity`.
The default is the account's, not the model's alone: the same day, `gpt-6.1-sol` defaulted to `medium` on one account
and to `low` on another. That this is the level actually run is corroborated rather than read off a request: in 35
sessions whose turn contexts left the effort out, each subagent Codex spawned was recorded in a `SubAgentActivity` item
with `reasoning_effort` `medium`, that account's default, and not with the entry's `multi_agent_reasoning_effort`
(`xhigh`). Which configuration layers a turn context reflects was measured with `codex doctor --json`, whose
`config.load` check names the effective `model` but not the effort (the project layer is under "Project configuration
layering" above): a top-level `profile` key in `config.toml` makes the configuration fail to load (`invalid data`) —
`--profile <name>` on this version layers `$CODEX_HOME/<name>.config.toml` instead.

### Codex 0.167.1: prompts on screen

Measured with Octoboard's arrangement (private app server, `--remote` terminal, asking mode), 120x40, against a stub
model; captures in `apps/daemon/testdata/codex-interaction-captures.md`. The daemon reads these
(`adapter/codex/interaction.rs`).

- **Command and patch approvals** replace the composer. A command's reads `Would you like to run the following
  command?`, `Environment:`, `Reason:` and `$ <command>`; a patch's `Would you like to make the following edits?`,
  `Description:` and `Destination:` (a long path is cut mid-word across rows). Three rows follow, the highlight (`›`)
  on the first: `1. Yes, proceed (y)`, `2. Yes, and don't ask again for … (p)` (a patch: `… for these files (a)`) and
  `3. No, and tell Codex what to do differently (esc)`; the footer is `Press enter to confirm or esc to cancel`. The
  digit and the letter take their option at once, no Enter; `p` also writes a prefix rule; Esc, `3` and `n` cancel and
  interrupt the turn. There is no field for text. The hooks are `PreToolUse` and `PermissionRequest` (tool `Bash` or
  `apply_patch`, the patch as `tool_input.command`).
- **MCP tool approvals** are a form, `Field 1/1`, `Allow the <server> MCP server to run tool "<tool>"?`, the
  arguments, and `1. Allow`, `2. Allow for this session`, `3. Always allow`, `4. Cancel` with descriptions after two
  spaces; footer `enter to submit | esc to cancel`. A digit selects and submits; `4` and Esc cancel the call, not the
  turn. The hook's tool is `mcp__<server>__<tool>`. An MCP server's own **elicitation** is the same form with another
  title (`Field 1/1 (1 required unanswered)`) and fires no hook at all.
- **`request_user_input`** exists in Plan mode (and in Default mode behind an unstable flag; without it Default mode
  rejects the call at once: `PreToolUse`, no `PostToolUse`, then `Stop`, and the payloads do not tell the modes apart).
  `PreToolUse` carries the questions (`id`, `header`, `question`, `options[].{label, description}`, no multi-select) and
  no `PermissionRequest` follows; `PostToolUse` carries the answers. One page per question, `Question k/n (m
  unanswered)`, the options numbered with the terminal's own `None of the above` last, footer `tab to add notes | enter
  to submit answer | esc to interrupt` (`←/→ to navigate questions` with several). A digit answers and goes on, or
  submits on a single question; Tab opens a notes field under the highlighted option (bracketed paste fills it, Enter
  answers); a note alone is the `None of the above` option plus the note. Esc interrupts the turn. Submitting with a
  question unanswered asks `Submit with unanswered questions?` first.
- **The slash popup** lists the matching commands above the composer, each `/name  description`; the highlighted one is
  led by `›`, and the descriptions of the others are drawn faint. An Enter runs the highlighted entry, so a typed
  `/sta` runs `/status`.
- **Pasted images** show as `[Image #n]` in the composer, one per path pasted on its own.

### Keys that interrupt a turn

Measured on 0.163.0, in a PTY with a real account. One Esc stops a turn: the `Interrupt` hook fires 20–130 ms later, the
screen shows "■ Conversation interrupted", the composer is left empty (the prompt is not put back) and the rollout
records `turn_aborted`. A message submitted mid-turn is steered into the running turn: it fires a `UserPromptSubmit`
with the turn's own `turn_id`, and an Esc after it ends the turn with that message unanswered. Ctrl-C mid-turn
interrupts the same way, but Ctrl-C at the prompt exits Codex ("To continue this session, run: codex resume …"). At the
prompt one Esc shows "esc again to edit previous message" and a second opens the transcript rewind ("Browsing transcript
… ↵ rewind · esc back"). An Esc written in the same write as a bracketed paste breaks the paste: its text lands in the
composer behind a literal `[200~`. With Octoboard's title items (`activity`, `thread-name`, `project-name`) the activity
item is a braille spinner while a turn runs, and the title is back to the project or thread name once the Esc has
landed.

### Keys that clear the composer

Measured on 0.167.1 on 2026-11-15, macOS: the ordinary TUI in a PTY of 100 by 40, no private app server, a scratch
`CODEX_HOME` with a fake key and a local stand-in for its inference API that never answers, so a submitted prompt keeps
a turn running; launched with `--dangerously-bypass-hook-trust`, `check_for_update_on_startup=false`, Octoboard's
`tui.terminal_title` and `notice.hide_rate_limit_model_nudge=true`. Ctrl-U deletes from the cursor back to the start of
its line, and at the start of a line the line break before it, so a three-line paste took five presses to empty, each
press changing the composer on the screen; a line wrapped over three rows went in one press, as did a collapsed paste
(`[Pasted Content 2400 chars]`, the placeholder a 2,400-character bracketed paste is shown as) together with the text
before it on its line. With the cursor moved back into a line, a press left the text after the cursor, and a second
press, at the start of the first line, changed nothing. On an empty composer it does nothing: no hint, no exit. Mid-turn
it clears the same way and the turn runs on ("Working" kept counting, and no "Conversation interrupted" was shown), and
Codex stayed up throughout. No undo hint is shown. End (`ESC[F`) moves the cursor to the end of its own line only, idle
and mid-turn, after which a press clears that whole line; with the cursor on an earlier line of a multi-line draft the
lines below are left, and Ctrl-End (`ESC[1;5F`) did not move the cursor at all, so no key was found that puts the cursor
at the end of the whole draft. On Windows the same held (0.166.0, native ARM64, measured 2026-11-16 on Windows 11 ARM64
with the same scratch home and stand-in, through `node-pty`'s pseudoconsole on the inbox console host, its default,
rather than the daemon's own with the bundled one): each press deleted back to the start of the line, three lines
written as one burst of keys (line feeds between them) took five presses, a 2,400-character burst of keys showed as
`[Pasted Content 2400 chars]` and went in one press, a bracketed paste of the same 2,400 characters showed and cleared
the same way, a press mid-turn cleared the composer with the turn running on, and Codex stayed up.

### An MCP server's environment and secret headers can come from Codex's own environment

Besides `command`, `args`, `env`, `url` and `http_headers`, a server under `mcp_servers.<key>` accepts `cwd`, `env_vars`
(names of variables forwarded from Codex's own environment to a stdio server), `env_http_headers` (a header name mapped
to the variable holding its value) and `bearer_token_env_var`; all four were accepted by 0.163.0 in a scratch
`CODEX_HOME`, listed by `codex mcp list`. `env_vars` cannot rename a variable: the server receives it under the name
Codex holds it by. What a server does with them at run time, and whether commands Codex runs for its model see the same
variables, was not measured.

### Codex copies its whole environment into files

Measured on 0.166.0 and 0.167, macOS and Linux. Every exported variable of the process lands in the shell snapshots
under `$CODEX_HOME/shell_snapshots/` (mode 0644 under the usual umask) and in `logs_2.sqlite`, so nothing secret can be
handed to Codex in a variable. The commands its model runs get that environment through `shell_environment_policy`,
whose default excludes (names matching `*KEY*`, `*SECRET*`, `*TOKEN*`) are off unless `ignore_default_excludes = false`
is set, in the user's or the project's `.codex/config.toml`; with them on, a variable so named is dropped on its own,
`GIT_CONFIG_KEY_<n>` included while its `GIT_CONFIG_COUNT` stays, which fails every git ("missing config key").

### A job a command starts in the background ends with the command

Measured on 0.167, macOS. A plain `nohup … &` from a command the model runs is gone once that command finishes; a job
meant to outlive it has to be started in a session of its own (`setsid`).

Every agent also ends, as it exits, the process group of each command still running (see "Exiting ends the process
groups of running commands" under "Across the agents").

### What a rollout records of a turn's items

Surveyed on 824 local rollouts of 0.163.0 to 0.167.1. A rollout holds only `item_completed` events of a turn's items
(`UserMessage`, `AgentMessage`, `CommandExecution`, `FileChange`, `McpToolCall`, `CollabAgentToolCall`, `Extension`,
...): no `item_started` event and no text deltas, so nothing in it shows an item in progress. Some calls are recorded
when the model makes them instead, as `response_item` `function_call` or `custom_tool_call` lines followed by their
`function_call_output` or `custom_tool_call_output`: the code-mode `exec` cell, `wait` on such a cell, and the subagent
calls `spawn_agent`, `send_message`, `wait_agent`, `followup_task`. Some of those share their id with an item of another
type written between the call and its output: `wait_agent` with a `CollabAgentToolCall`, `sleep` with an `Extension`
of `kind` `clock.sleep`, and `request_user_input_async` with an `AgentMessage` that repeats its questions.

## Grok Build

**Permission modes** (1.0.77). The [official permission modes](https://docs.x.ai/build/features/permissions) are Ask,
Auto and Always-approve. Their user-configuration values under `[ui].permission_mode` are `ask`, `auto` and
`always-approve`; the corresponding explicit CLI values for `--permission-mode` are `default`, `auto` and
`bypassPermissions`. Plan mode and sandboxing are independent settings. Deny rules and hooks still apply in
Always-approve mode.

These three CLI selections were checked on fresh launches and direct resumes with the complete Octoboard hook/MCP
injection, per-session configuration copy and shared session directory, using fake credentials, a local model catalog
and no model turn. The actual mode was read from `SessionStart.permissionMode`, not inferred from `summary.json`.
Explicit Ask replaced a previous Always-approve mode on the same session; Auto and Always-approve each replaced Ask on
resume. The account's configuration bytes were unchanged. A resume with no explicit mode and an account configuration
selecting Always-approve entered `bypassPermissions` after a prior Ask launch, so omitting the flag must not be described
as preserving all saved permission choices. It leaves the result to Grok's configuration and resume rules.

Auto remains feature-gated. With `[auto_mode].enabled=false`, passing `--permission-mode auto` produced `default` in
the startup hook and connected MCP normally. These probes establish startup selection and fallback only; they did not
exercise the classifier's decisions on tool calls.

16 hook events, payload as JSON on stdin, with **every field present in both camelCase and snake_case**. The hook
process additionally receives `GROK_HOOK_EVENT`, `GROK_HOOK_NAME` — whose `global/`, `project/` or `managed:` prefix is
usable provenance — `GROK_SESSION_ID` and `GROK_WORKSPACE_ROOT`.

**The per-turn `Stop` carries `promptId`, `lastAssistantMessage`, `backgroundTasks` and `sessionCrons`**, and the
teardown fire carries none of them, so the two are distinguishable by payload shape as well as by `reason`.

**`backgroundTasks` lists the work the turn left running, and each finished piece wakes the session** (1.0.77, measured
on 2026-11-13 with a headless `grok -p` run against the live model; the capture is
`apps/daemon/testdata/grok_hooks_background_work_turns.jsonl`). An entry has camelCase keys only: `id`, `type` (`shell`,
`monitor` or `subagent`), `status` (`"running"`), and `command` for a shell command or `description` and `agentType`
for a subagent; the list is `[]` when nothing runs. With a background `sleep 20` and a background subagent started in
one turn, that turn's `Stop` listed both; the shell command's completion then started a turn of its own with a
`UserPromptSubmit` whose prompt was `<system-reminder>` + `Background task "<id>" completed (exit code: 0).` and more
lines, and that turn's `Stop` listed only the subagent; the subagent's completion started another, whose prompt was
`<system-reminder>` + `While you were idle, 1 background subagent completed:` with a `=== Task <id> ===` block, and
that turn's `Stop` listed nothing. The subagent's own `SubagentStart`, `UserPromptSubmit` and `SubagentStop` carry
`subagentType`; its `SubagentStop` came between the two wakes. The Grok user guide ("Hooks", "Stop Decision Control")
documents the same shape. Not measured: whether a monitor's events, or Grok with `features.auto_wake` off (the guide
says an idle-drain notification turn runs instead), wake the session the same way.

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

**The calls of one response run side by side, and one can end while `ask_user_question` waits on the user** (1.0.77,
measured on 2026-11-10 in a PTY against a local stand-in for its inference API, in Ask, Auto and Always-approve modes;
the captures are `apps/daemon/testdata/grok_hooks_*.jsonl`). Grok first takes the calls one at a time, in order: each
fires `PreToolUse` and gets its permission decision (a rule, Auto's classifier, or a prompt) before the next call's
`PreToolUse` fires. Then it runs them all at once. With a `run_terminal_command` running `sleep 6` and an
`ask_user_question` in one response, in either order, the shell command's `PostToolUse` came about 6 s after the
question's `Notification` `elicitation_dialog`, with the question still on screen. A background `spawn_subagent`'s
`PostToolUse` was emitted 1–3 ms after that notification (by the payloads' `timestamp`; the two hook processes start
together, so they arrive in either order), and a foreground one's when its subagent finished. With the question first,
the other call's `PreToolUse` follows the question's, and in Auto mode the question appears only once the classifier has
decided on that other call. A permission prompt, by contrast, holds the whole response: no call of it starts, not even
one that needs no permission, until the prompt is answered (checked against the process list), and a question in the
same response appears only after it. Work an earlier response left running does not end through a post event either: a
command put in the background (`block_until_ms: 0`) fires its `PostToolUse` at once and reports its completion as a
`Notification` `task_complete`, and a background subagent's own events carry `subagentType` and the subagent's own
`sessionId`. So a post event says only that its own call ended, and which call a hand was raised for has to be kept by
`toolUseId` (also `tool_use_id`), which `PreToolUse`, both post events and `PermissionDenied` carry; `Notification`, for
`permission_prompt` and `elicitation_dialog` alike, names no call. The terminal title keeps its spinner while a question
waits, and is prefixed with `⚠ Action Required - ` while a permission prompt does.

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

**A `PreToolUse` deny blocks a write, over HTTP too** (1.0.77, `grok -p` in `acceptEdits`, a hook file in
`GROK_HOME/hooks/`, a probe printing a `deny` decision with a reason). A `command` hook's deny blocks `search_replace`
(Grok's file edit) and `run_terminal_command`, and so does an `http` hook to `http://127.0.0.1:<port>`, which the probe
reached. The model sees `Hook denied: <reason>`, the reason cut to its first 256 characters (the prefix not counted;
characters, not bytes: a reason holding CJK text kept 256 characters, 276 bytes; measured on 1.0.77 in real sessions
on macOS and Windows) and followed by `… [+N chars]`, and did not retry the write another way. On Windows a hook's
`command` that is a bare path to a `.cmd` script runs through `cmd.exe` (`grok.exe` → `cmd.exe` → the script's
program). An unreachable hook URL lets the call through, but with `"failClosed": true` it denies with
`Hook denied: fail-closed hook '<name>' failed: HTTP request failed for <url>: error sending request`.

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
waiting session request (`request_session`), which would end the user's 8 minutes to answer early (see "How a request
ends" in `docs/product/session-requests.md`). On 1.0.52 with a real account, the appended `[mcp_servers.octoboard]`
block was read off a live farm carrying `tool_timeout_sec = 660`.

**The model is held silent while an MCP call is outstanding**, the opposite of Codex. Through a
`request_session` call (measured under its earlier name `request_console_session`) that Octoboard kept waiting the
full 8 minutes, Grok sent its endpoint nothing between the turn's own request and the one carrying the
tool's error: no turn of its own, no token spent (measured on 1.0.50 against a stub inference endpoint).

**What the model itself is handed, and what it calls** (1.0.50 against a stand-in endpoint, re-confirmed on 1.0.52
against a real account): an MCP tool is **not** among the tools offered to the model. They are reached through Grok's
own `use_tool`, whose `tool_name` is the qualified `<server key>__<tool>` and whose `tool_input` holds the arguments —
the wrapper the `PreToolUse` payload shows — and a `search_tool` call first is not required, though 1.0.52 injects a
system reminder of its own telling the model to call `search_tool` for a tool's input schema before `use_tool`. In four
real Grok Build 1.0.52 sessions driven end to end through `request_session` (then named `request_console_session`),
every one called `search_tool` at least once, and what a real model called was the `use_tool` wrapper with the qualified
`octoboard__<tool>` name. Every chat request sets `stream: true`, so anything standing in for the endpoint has to answer
as a stream. A turn is also followed by requests that offer no tools at all and belong to the TUI rather than the agent:
the dashboard summary line, the session title, and a prediction of the user's next line. Inference for one model can be
routed on its own through a `[model.<name>]` entry carrying its own `base_url` (and `api_key`), without the
`endpoints.models_base_url` that moves every model's.

### Model and reasoning effort

The model is passed as `--model <id>` and the effort as `--reasoning-effort <level>`, placed after the `--rules` pair
and before the positional task, which comes last, after a bare `--`.

**A prompt that starts with `-` is taken only after a bare `--`** (1.0.92, measured on macOS on 2026-11-22 without a
terminal, with a scratch `GROK_HOME`, a fake `GROK_CODE_XAI_API_KEY` and a `[model.probe]` entry on a closed local
port). As a bare argument it is refused at argument parsing, with exit code 2: `grok --no-leader --model probe '--help
me with this'` and `'--definitely-not-an-option'` print `error: unexpected argument '…' found`, and `'- a list'` prints
`unexpected argument '- ' found` with the tip "to pass '- ' as a value, use '-- - '"; `'-p x'` is not refused but read
as the option `-p` (`--single`), a headless single turn. After `--` the prompt is taken as written: `grok --no-leader
--model probe -- '--definitely-not-an-option'` recorded `--definitely-not-an-option` as the prompt in the scratch
home's `prompt_history.jsonl` and as the user message of the session's `chat_history.jsonl`. `[PROMPT]...` takes every
argument after the `--`, joined with spaces, so the `--` and the prompt come last: `grok --no-leader --
'--definitely-not-an-option' --model probe` recorded the prompt `--definitely-not-an-option --model probe` and ran on
the default model instead of the one named. After the `--`, the name of a subcommand is a prompt as well: `grok
--no-leader --model probe version` printed the version and exited, where `grok --no-leader --model probe -- version`
and `-- logout` recorded the prompts `version` and `logout` and ran nothing else. Not measured: the same launches on a
terminal, in the fullscreen TUI.

In a fullscreen TUI (1.0.62), startup alone persists these as `[models] default` and
`default_reasoning_effort`; a headless `-p` run did not. An explicit `/model other-local high` changed both keys in the
TUI. A launch's own model chosen again cannot be told from its startup save. The per-session `GROK_HOME` keeps
`config.toml` as a real copy; what Octoboard carries from it, and why the model passed at launch and the effort are left
out, is under "Grok itself rewrites the per-session `config.toml` copy" in "Sharing a configuration directory between
accounts".

The effort is on record as well: the session directory's `summary.json` carried `reasoning_effort` in each of the 114
summaries on the account measured (installed build 1.0.62 on 2026-10-20; a summary names no version), including
sessions that ran a level other than the `config.toml` `[models] default_reasoning_effort`. A catalogue entry's
`reasoning_effort` and the `default: true` level of its `reasoning_efforts` name the model's default; Grok's own
configuration reference says `default_reasoning_effort` applies to "the default model", and whether it also applies to
a model chosen with `--model` was not measured.

### Theme

Measured on Grok Build 1.0.62, from `grok --help`, the strings in the executable and Grok's own theming guide: there is
**no `--theme` flag**. The environment variable `GROK_THEME` (or `LC_GROK_THEME`) forces a theme and outranks
`config.toml`'s `[ui] theme`. The values include `groknight` (the default, dark: its backgrounds are near-black greys),
`grokday` (light greys with dark text), `tokyonight`, `rosepine-moon`, `oscura-midnight`, `terminal` (the terminal's own
colours) and `auto`. On a local macOS desktop `auto` follows the system appearance (`AppleInterfaceStyle`), not the
terminal's background, and picks the `auto_light_theme` / `auto_dark_theme` themes (`grokday` / `groknight` by default).
A dark theme on a light terminal leaves the cells Grok paints with the terminal's default colour showing through as
light holes between dark bands.

With `GROK_THEME=groknight`, `grokday` or `auto`, fullscreen startup left `[ui] theme` unchanged, including leaving
the key absent when it was absent initially (1.0.62, scratch homes, fake API key and a local model stub). A model
change also left it unchanged. Explicit `/theme tokyonight` and `/theme grokday` saved those names even with
`GROK_THEME=grokday`; selecting the environment's own theme is still an explicit save. So a saved `[ui] theme` is
always the user's choice, and Octoboard carries it without filtering out the injected name (see "Grok itself rewrites
the per-session `config.toml` copy").

### Discovering models

Measured on Grok Build 1.0.50: Grok has **no alias or "latest" indirection** either. `grok models` prints to stdout the
login line, a blank line, `Default model: <id>`, a blank line, `Available models:` and then one bullet line per model,
`  * <id> (default)` for the default and `  - <id>` for the others, in the order Grok shows them. It carries no display
name and no effort levels. It also refreshes `models_cache.json` in the config directory (`GROK_HOME`), a JSON object
with `fetched_at`, `grok_version`, `etag` and `models`, a map **keyed by model id**; each value's `info` carries `name`
(the display name), `hidden`, `supports_reasoning_effort`, `reasoning_effort` (the default) and `reasoning_efforts`, a
list of `{id, value, label, description, default}` whose `value` is what `--reasoning-effort` takes (the level set
differs per model: the oldest of the four measured lacked `xhigh`). Octoboard takes the id list from the text, which is
the one Grok itself shows, and the names, efforts and `hidden` flag from the file; the file alone would do for the data
but only exists after Grok has run in that home, and the text is what refreshes it. The cache is per account (its
`identity` differs). **An unknown id is refused loudly at launch** with `Couldn't set model '<id>'.`, the opposite of
Codex's silent deferred failure.

### Grok Build's permission and question cards (1.0.89)

Measured with `--no-leader` (captures in `apps/daemon/testdata/grok-interaction-captures.md`). A card replaces the
composer as a block of rows behind a `┃` margin with a footer under it. Permission: `›  <what it asks>`, the command or
arguments behind `│`, then rows numbered **from 2** (`2 (●) Yes, proceed`, `3 (○) No, reject (type to add feedback)`,
`4 (○) Never allow: <command>`; edits `2 Yes, allow edits in <dir> for this session`, `3 Yes`, `4 No…`; MCP `2 Always
allow: …`, `3 Yes`, `4 No…`, `5 Never allow…`) and an unnumbered last row that switches the session to bypass mode and
rewrites `config.toml` (never pressed). Digits choose at once; Down stops at the ends, Tab wraps; typing or pasting on the
No row starts a message (`3 (●) ❯ text`, footer `Enter:send │ Esc:back`). `ask_user_question`: `N (○) label  description`
rows (`[ ]`/`[x]` for multi-select), a `z` row for a written answer, several questions one at a time (`[k/n]`, no review
page); a digit picks and advances, Enter submits ticked rows, Shift+X dismisses. Esc parks the keyboard in the scrollback
(footer `Tab:permission` / `Tab:question`, card still drawn); Tab returns. The slash completion is a row
`❯ /name <args> description` between two rules above the composer; with that row up, pasted `/sess` plus Enter ran
`/session-info`, while pasted `/probe` plus Enter only completed into `/probecmd ` (a command that takes arguments) and
sent nothing; why they differ is not measured. Pasted image paths each draw `[Image #N]` in the composer. Grok drops an
image that is too small when the message is sent, with a notice on the screen (32x32 and 40x40 PNGs went through, a
PNG under 512 pixels was dropped in one test; the threshold is unclear). A card with a pick or a tick already on it
(`Esc:unselect` in its footer) is a card half answered and is not published.

### The id a turn records is not one of the ids `grok models` prints

Measured on 1.0.52. With nothing set, a session runs the `config.toml` default — `grok-4.7` on the account measured —
while its own session record and usage file name the served model `grok-4.7-build`. So the id to match a turn against
the list Octoboard offers is the configured one, not the one the records carry.

### Hooks that never come, and screens after an error

Measured on 1.0.67, scratch `GROK_HOME`, fake key, local stub. On a launch and on `--resume <id>` with no prompt,
`SessionStart` (with `source` `load` on a resume) arrives 0.5–1.0 s after the start, and right after the folder-trust
confirmation is accepted. With `allow_managed_hooks_only = true` in Grok's requirements no hook of Octoboard's runs at
all, so nothing marks the session started; a write before `SessionStart` was buffered and submitted. After a usage-limit
error (the stub answering HTTP 402) Grok fires `Notification` with type `agent_error` and `StopFailure`, then draws "You
hit your weekly limit." with "1 (○) Upgrade tier", "2 (○) Buy more credits", "3 (○) Try Again" and "↑/↓ navigate · y
copy Enter :submit / Shift+x :dismiss": a paste and CR closed it with no `UserPromptSubmit` and opened the browser on a
purchase page. The modal is on screen before those hooks reach a hook command. "3" (or the arrows and Enter) retries,
firing `UserPromptSubmit` with the original prompt; Shift+x dismisses it with no hook and leaves the input free. An HTTP
429 shows "Rate limited | Retrying (attempt 1)…" and "Queued · Enter to send now", and a paste there was queued, not
lost; Grok retries once, honouring `retry-after`, and when that retry is refused too it fires `agent_error` and
`StopFailure` (`error` `rate_limit`) with an inline "Retry failed" line, no modal, and submits a queued prompt by
itself. An HTTP 400 fires the same two hooks (`error` `invalid_request`, as for the 402) with an inline line and no
modal; a single HTTP 500 is retried with no hook. How Octoboard holds its writes around these hooks and screens is in
"Messages held until a session can take them" in `docs/product/message-delivery.md`.

### The usage-limit screen again

Measured on 1.0.77, scratch `GROK_HOME` and `HOME`, fake key, `[endpoints] models_base_url` and `xai_api_base_url` both
pointed at a local stub; with only the first set, the key probe went to xAI, failed, and Grok began its grok.com device
sign-in. A 402 fires `UserPromptSubmit`, `Notification` `agent_error` and `StopFailure` (`error` `invalid_request`) with
no automatic retry, and `updates.jsonl` records the turn as `turn_completed` with `stop_reason` `error` and "status 402"
in its result. The modal reads as in 1.0.67, each option followed by a description ("Resubmit the last prompt once you
have usage again" for Try Again), but its own last row is "↑/↓ navigate · y copy" with "Enter:submit" at its right, and
the dismiss key moved to the bottom bar: "Tab:next answer │ Esc:scrollback │ Shift+x:dismiss". Option 1 is highlighted
first. In the one sample taken, the `agent_error` hook reached its command before the modal was on screen, unlike
1.0.67. Try Again, by "3" or by the arrows and Enter, shows "Trying again…" and fires `UserPromptSubmit` with the
original prompt and a new `promptId`; a retry refused again fires `agent_error` and `StopFailure` and redraws the modal,
and one that succeeds ends with `Stop` (`end_turn`). Each retry writes its own `user_message_chunk` and
`turn_completed`. Shift+x dismisses it with no hook. Esc moves the focus to the scrollback: the modal stays drawn, the
bottom bar becomes "Tab/Space:question │ Ctrl+e:collapse thinking │ Ctrl+x:shortcuts", and Shift+x does nothing until
Tab brings the focus back. `idle_prompt` came 60 s after the `StopFailure` with the modal still up. The 400 and 429
cases behaved as in 1.0.67. Choosing 1 or 2, or Enter on the first highlight, was not tried on this build, since on
1.0.67 it opened a purchase page.

### Keys that interrupt a turn

Measured on 1.0.70, in a PTY against a local stand-in for its inference API. Esc does not stop a turn; Ctrl-C does.
While the model streams, Ctrl-C fires `StopCancelled` (`reason` `user_interrupt`) 50–190 ms later and the screen shows
"Turn cancelled by user". Before the first token arrives it fires no hook at all — no `StopCancelled` and no `Stop`; the
`idle_prompt` notification came about a minute later — and puts the prompt back in the composer, and the title drops its
braille spinner 16 ms after the key; the spinner stays through tool calls. With text in the composer, Ctrl-C clears the
text and the turn runs on; a second Ctrl-C then cancels it. Ctrl-C at the prompt asks to be pressed again to quit.
During a blocking shell command with a message queued, Ctrl-C fired `StopCancelled` and the queued message was submitted
4 ms later. Ctrl-U deletes one line of the composer a press, at the start of a line the line break before it, and a line
wrapped over two rows in one press; the composer then shows "Input cleared · ctrl+z to undo". Not measured: Ctrl-C with
the subagents' cancel-turn panel up, and at a permission card.

### Leader mode leaves the TUI's stop to a `SIGKILL` of the leader, so every launch passes `--no-leader`

Measured on 1.0.70, in a PTY as a session of its own, scratch `GROK_HOME`, fake key, local stand-in for its inference
API. Grok's user guide documents leader mode as `--leader` or `[cli] use_leader = true` (off by default), and
`--no-leader` as forcing a local agent; `grok --help` lists neither for the TUI, only `--leader-socket`, but the TUI
accepts `--no-leader` on 1.0.41, 1.0.67 and 1.0.70 (`--no-leader --version` exits 0 where an unknown flag exits 2), and
the parser never takes it as another option's value, so its place on the line does not matter. With `use_leader = true`
the TUI starts `grok agent leader --no-exit-on-disconnect …` in a process group of its own but in the TUI's session,
listening on `$GROK_HOME/leader.sock`, and is only its client; Grok's hooks then run from the leader. A socket path too
long for a Unix socket fails to bind, and the TUI then never draws. Sent `SIGTERM` mid-turn, the TUI exited at once with
status 0 and fired no hook, while the leader logged "kept session resident across client disconnect (live work)", stayed
in the exited TUI's session without a terminal, ran the turn on, fired its `end_turn` `Stop` with the turn's `promptId`
114 s later, when the held request was answered, and kept running until signalled itself, when it fired the `shutdown`
pair. Octoboard's stop does not let it get that far: the leader is neither in the TUI's process group nor reached by the
`SIGTERM`, but the sweep of an exited agent's session `SIGKILL`s it at once. Sweeping the session that way as soon as
the TUI exited fired no hook at all, mid-turn or in the 135 s after, skipped the "shutdown: flushing session" Grok logs
on a signal it handles, and left `leader.sock` behind. The same launch line with `--no-leader` and `use_leader = true`
started no leader and no socket, and `SIGTERM` mid-turn fired `Stop` and `SessionEnd` with `reason` `shutdown` from the
TUI itself, which exited with status 0, and nothing in the 135 s after; so did a `--resume`. With a leader already
running on the same socket, a `--no-leader` TUI did not connect to it (nothing new in its `leader.log`). Neither
`grok models`, `grok -p` nor `grok login --device-auth` started a leader with `use_leader = true`, and `models` and `-p`
did not connect to a running one; the quota reading runs `grok agent --no-leader stdio`. Not established: whether a
`requirements.toml` pin of `cli.use_leader`, which Grok's configuration reference allows, outranks `--no-leader`; a
`[cli] use_leader = true` in the scratch home's `requirements.toml` did not turn leader mode on at all, so it could not
be tested.

### Grok scans Claude Code's configuration by default

The `[compat.claude]` table of Grok's `config.toml` has five boolean cells, `skills`, `rules` (which covers
`CLAUDE.md`), `agents`, `mcps` and `hooks`, each defaulting to true, and each overridden by an environment variable
`GROK_CLAUDE_<CELL>_ENABLED` (`true` or `false`); Grok's user guide shipped under `~/.grok/docs/` and the 1.0.70
executable both name them, and like cells exist for Cursor (`GROK_CURSOR_*`) and Codex (`GROK_CODEX_*`). That the
variable outranks the file is what the guide states.

Measured on **Grok Build 1.0.77** with `grok inspect --json` and `grok -p` under a scratch `HOME` and `GROK_HOME`
(logged in through `GROK_AUTH_PATH`):

- The Claude compatibility really starts the MCP servers of `$HOME/.claude.json`, both the user `mcpServers` and
  `projects.<cwd>.mcpServers`; `GROK_CLAUDE_MCPS_ENABLED=false` turns them off. Grok ignores `CLAUDE_CONFIG_DIR` and
  always reads `$HOME/.claude.json`. A project `.mcp.json` is a separate source (listed as `mcpJson`): the Claude cells
  do not gate it, and Claude Code's `enabledMcpjsonServers` does not apply to it.
- In `config.toml`'s `[mcp_servers]`, `${VAR}` and `${VAR:-default}` are expanded in `args`, `env` values and
  `headers`, and a stdio server inherits Grok's environment. When Grok rewrites `config.toml` (appending
  `[marketplace]`, say) it keeps the `${...}` references verbatim. An expanded value can still appear in the session's
  `events.jsonl`, in the `target` of an `mcp_server_failed` event.
- `[skills] paths` loads `<path>/<name>/SKILL.md`; a `commands/` directory under such a path is not loaded as slash
  commands.
- Grok reads `~/.agents/skills` (and, per its guide, the project's `.agents/skills`). It does not read
  `~/.codex/AGENTS.md`, `~/.codex/skills`, `.codex/skills` or the servers in Codex's `config.toml`. The `[compat.codex]`
  cells `skills`, `rules`, `agents`, `mcps` and `hooks` are documented as reserved and are inert; only `sessions` exists.

### When Grok writes its conversation

Measured on Grok Build 1.0.89, in a scratch `GROK_HOME`, `HOME` and `TMPDIR` with the agent markers stripped, a fake
`GROK_CODE_XAI_API_KEY` and a `[model.probe]` entry whose `base_url` was a local stand-in streaming `chat_completions`
(no account, no network). A headless `-p` run made one tool call (`sleep 6; echo done`) between two streamed texts,
while a poller sampled the sizes and line counts of `chat_history.jsonl` and `updates.jsonl` every 50 ms, and a second
run streamed one 9-second text with no tool call. Neither file is written as a model call streams, and neither waits
for the turn's end:

- A model call's assistant line goes into `chat_history.jsonl` when its stream ends, before its tools run, with its
  text and all its `tool_calls` in the one line (the line appeared 21 ms after the stand-in sent the call). A tool's
  `tool_result` line is appended when the tool ends (the command's line came in the poll that saw it finish, 6 s
  later), and the next model call's line when its stream ends.
- `updates.jsonl` is written at the same moments, a few tens of milliseconds after the record: an `agent_message_chunk`
  carrying the call's whole text, then a `tool_call` for each call, whose `kind` and `status` (`Other`, `Pending`) are
  at `params._meta.updateParams`, not in the update. Nothing in it shows text while a call streams: in the 9-second
  stream the file did not change until the stream ended. In 180 recorded sessions (created 2026-09-29 to 2026-11-18;
  a session does not record the version) each `promptId` and `streamStartMs` pair has exactly one chunk.
  The `tool_call_update` that names the call's `kind` and `rawInput` and the one with `status` `completed` or `failed`
  reached the file only when the tool ended, in the one run measured, though their `agentTimestampMs` is earlier;
  `turn_completed` closes the turn. A user prompt is a `user_message_chunk` there, and its `timestamp` is an integer in
  seconds, not a string.
- So the record alone shows a tool call running (its assistant line, no result line yet), and `updates.jsonl`, written
  after the record's line, showed nothing ahead of it in the runs measured; what it adds is a call's `failed` status:
  a `tool_result` line is `{type, tool_call_id, content}` with no error mark, and the content of a failing `grep`,
  `list_dir` or `read_file` opens with `Error: <path>`, which is text rather than a field. A command's content opens
  with `exit: <code>`. Octoboard's live conversation feed reads the record and takes the failure from the updates; it
  also keeps what the log shows ahead of the record as pending, for an order not seen here (the TUI, another version).
- A compaction rewrites `chat_history.jsonl` but leaves `updates.jsonl` as it was: after `auto_compact_completed` the
  log still holds the chunks and calls of the model calls the record no longer has. A turn cut off mid-turn has no
  `turn_completed`; the next prompt's `user_message_chunk` follows it.
- A server-side web search is a `backend_tool_call` line in the record, `{type, kind: {tool_type: "web_search", action:
  {type, query, sources}, id, status}}`, with no `tool_result`; in the log it is a `tool_call` with `status`
  `in_progress`, `_meta.backend: true`, the title "Web search:" (no query) and `rawInput` `{variant: "WebSearch",
  backend: true}`, then a `tool_call_update` `completed` whose `rawOutput` carries the query. Its id is
  `ws_<uuid>_call-<uuid>-<n>`. The record also holds `reasoning` lines with encrypted content.

The probe left nothing outside its scratch directory and the scratch `GROK_HOME`: no login was touched, the stand-in
listened on 127.0.0.1 only and was stopped afterwards.

## Antigravity CLI

Measured on Antigravity CLI 1.5.14 (the native
binary `agy`, installed by Google's script at `~/.local/bin/agy`) on macOS. Unless a line says otherwise, a fact was
measured in a scratch home and a scratch working directory with a local stand-in answering its inference requests
(`"modelProvider": "gemini"` given through `--settings`, a fake `GEMINI_API_KEY` and `GOOGLE_GEMINI_BASE_URL`); "docs"
marks a fact read from antigravity.google/docs and not measured, "binary" one read from the strings of the executable.
One turn ran through a real model with the user's own sign-in, a personal Google account on the free plan.

**Windows 11 ARM64 (23H2, build 22631.6199).** The vendor's PowerShell installer uses the ARM64
release manifest and installs under `%LOCALAPPDATA%\agy\bin`. The manifest's SHA-512 matched the
executable extracted into the probe's own equivalent layout. `agy.exe --version` returned **1.5.16**.
The provider recognises `agy/bin/agy.exe` as a native install for `agy update`. In the desktop
logon, Google OAuth created the `gemini:antigravity` Credential Manager entry. The native
presence reader reported signed in from the same desktop logon without a dialog. The daemon's
isolated `/usage` probe reported weekly windows for Gemini Models and Claude and GPT models.
The daemon's login status timed out after its authorization-code question closed, even though
the signed-in quota probe succeeded. The CLI wrote `OAuth: authenticated successfully as …`
to a dated file under the private login home's `antigravity-cli/log`. The login flow now checks that marker
after it submits the code on every host. The daemon refused a scratch home with an earlier-layout hook and no
migration marker. The hook remained in place and there was
still no marker. One native CLI model turn returned the verification word in its session record.
On interactive startup with a fresh scratch profile, both the injected and unmodified CLI
opened a colour scheme wizard. Its theme preview includes sample tool, error and warning lines.
After the theme choice both launches opened the CLI's own Terms of Service & Data Use screen.
Octoboard reported `waiting_user` and held messages while that screen was up. The Windows
interactive session lifecycle was left unverified by the user's decision at that screen.
The Windows sandbox probe also reported
a missing module on this 23H2 guest. [Google lists Windows 11 24H2 with the August 2026 update as
the minimum for that sandbox](https://antigravity.google/docs/sandbox/).

It is not a later Gemini CLI: a different program (its tools, prompts and state names are those of the Windsurf
line), which shares only the `~/.gemini` directory, the `GEMINI.md` and `AGENTS.md` instruction files and the
`GEMINI_API_KEY` variable with it. The user's first launch showed no prompt to migrate Gemini CLI's configuration
(Gemini CLI's directory existed, with an API-key sign-in selected and no Google credential on file), and a first
launch imports none of it: in a scratch home holding Gemini CLI's `settings.json` (an MCP server, a hook, a theme), a
`GEMINI.md`, an extension under `extensions/` with a server and a command, `trustedFolders.json` and an
`antigravity/mcp_config.json`, the CLI wrote what it writes in an empty home and nothing else (its log has
`Detecting Gemini extensions in …/.gemini/extensions`, `Found 1 extensions: [myext]`, and no more of it; the
executable holds a "Migrating %d extensions to plugin format" that this launch did not reach).

**Linux daemon** (1.5.16, Debian 13 arm64, glibc 2.41). A local Gemini stand-in authenticated with a
disposable API key through the launch's `modelProvider` setting. A console and a trusted project session ran
turns and called the injected Octoboard MCP server. The project received a parent instruction and the console
received its report. Both interrupted a turn and both relaunched after archive. A Linux process trace captured Antigravity CLI
starting the injected hook through `sh -c`. The signed-out daemon quota
probe ran `agy -p /usage --output-format json` in its own home and reported `quota_command_failed` with no
window. The CLI reported authentication required. A later quota refresh held that reading and the source
`~/.gemini` file contents were unchanged. Real Google OAuth probes reached the authorization page on Linux.
The first probe stopped at its own deadline. The second waited without a deadline and was stopped
before the clipboard handoff reached the CLI. The real Linux sign-in item and hosted quota went unmeasured.

### Install and update

- `curl -fsSL https://antigravity.google/cli/install.sh | bash` downloads one sha512-checked binary to
  `~/.local/bin/agy` and runs `agy install`, which appended a PATH line to `~/.zshrc` and `~/.profile` here. Not an
  npm package. It updates itself in the background; `AGY_CLI_DISABLE_AUTO_UPDATE=true` turns that off (docs), and
  every probe set it. `agy --version` prints the bare version.
- `agy update` (it takes no flag) updates the binary in place and prints `Update successful! Please restart agy.`,
  or `You are already on the latest version.`; the background updater records its last check in
  `antigravity-cli/updater/update_status.json` (`{"success":true,"message":"Already on the latest version."}`). No
  command prints the latest version without updating. The installer reads it from
  `https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/<platform>.json`
  (`{"version", "url", "sha512"}`), which named 1.5.15 on 2026-11-23 while this machine's binary, its own updates off,
  was 1.5.14. A plain GET on 2026-11-23 answered for `darwin_arm64`, `darwin_amd64`, `linux_amd64`, `linux_arm64`,
  `linux_amd64_musl`, `linux_arm64_musl`, `windows_amd64` and `windows_arm64` (all 1.5.15), and 404 for the `x64`,
  `win32` and hyphenated spellings; `install.sh` names the platform `<os>_<arch>` with `arch` `amd64` or `arm64`, adding
  `_musl` where it finds musl's loader. So Octoboard tells an install at `~/.local/bin/agy` as the CLI's own, updates it
  with `agy update`, and reads its latest version from the manifest of the daemon's platform, the glibc one on Linux
  (`CliPackaging::native_manifest`).
- **The installed version cannot be read without running the binary.** The install is the one 229 MB executable,
  in no versioned directory and with no file beside it naming the version; the executable binds no Info.plist and its
  Go build information is 144 bytes with no version in them. `1.5.14` occurs in it once, at byte 183,621,963, with no
  marker around it, and finding it took 48 s. So `installed_version` gives none and the version is asked with
  `agy --version`.

### Directories, and there is no variable for them

- Everything is under the home directory: `~/.gemini/antigravity-cli/` (`settings.json`, `brain/<conversation id>/`,
  `conversations/`, `history.jsonl`, `log/` with `cli.log` linking the latest, `cache/`, `presence/<conversation
  id>.lock`, `updater/`) and `~/.gemini/config/` (`config.json`, `mcp_config.json`, `hooks.json`, `projects/`), which
  the Antigravity app and IDE share (docs).
- No environment variable moves either directory. None is documented; of the names in the binary,
  `ANTIGRAVITY_CONFIG_REL_DIR` and `ANTIGRAVITY_EXECUTABLE_DATA_DIR` (a sidecar's data directory) changed nothing
  when set. `HOME` does: with another `HOME` the CLI creates and uses `.gemini` there.
- **An undocumented flag does: `--gemini_dir=<absolute path>`** makes that directory stand for `~/.gemini` for the
  launch. `agy --help` does not list it; `agy acp --help` lists it for the ACP mode (`--gemini-dir`, `--gemini_dir`,
  "Path where Gemini files are stored (default .gemini)", beside `--app-data-dir` and `--cli_dir_name`). Only the
  underscore spelling joined with `=` was taken by the interactive CLI: `--gemini-dir <path>` printed the usage and
  exited. The login is not in that directory: `agy --gemini_dir=<a fresh directory> models` with the real `HOME`
  listed the signed-in account's models.
- First launch in a new home runs a wizard (colour scheme, then "Terms of Service & Data Use" with `[Done]`) before
  anything else. `cache/onboarding.json` with `consumerOnboardingComplete`, `enterpriseOnboardingComplete` and
  `onboardingComplete` true skips it (how the probes started).

### Sign-in

**Linux credential presence** (Debian 13 arm64, GNOME Keyring 48.0, 2026-11-24). With a disposable item carrying
`service=gemini` and `username=antigravity`, Secret Service's D-Bus `SearchItems` returned its object path in
the unlocked list before locking the collection and in the locked list after locking it. The absent lookup
returned two empty lists. Search raised no prompt in either state and requested no secret. With no session bus,
the query failed. A signed-out Antigravity CLI 1.5.16 `agy models` run exited 1 and did not show those attributes
in the observed Secret Service searches. The Linux 1.5.16 binary passes that service and username to
go-keyring for its login lookup. Octoboard's Linux Rust reader returned Present for that fixture both unlocked
and locked, Absent for a fresh empty collection, and CannotTell for a disconnected session bus. Monitoring
recorded no prompt or secret read. The fixture verified the reader independently of the real Linux sign-in.

- A personal Google account signs in from the TUI through the browser ("Google OAuth"); an organisation through "Use
  a Google Cloud project"; an API key through `"modelProvider": "gemini"` in settings plus `GEMINI_API_KEY` (docs for
  the last two; the key route is what the stand-in used, and the banner then reads `Gemini API key` where a signed-in
  session shows the address).
- **Google OAuth in the TUI** (1.5.14 on macOS, 2026-11-24). Selecting Google OAuth in an isolated home
  reported `blocked/auth` and opened a browser authorization request with PKCE. Its redirect was an HTTPS page
  on `antigravity.google`. The browser showed an authorization code after the same account was approved, and
  the CLI asked to paste it back into the terminal. There was no loopback redirect in this route. The clipboard
  was sent directly to the PTY, followed by Enter. The CLI stayed blocked and no files appeared in the isolated
  home, so the probe did not establish that the clipboard handoff reached the prompt or that login completed.
  Both probe homes were deleted, and no authorization code or credential was captured. Octoboard drives this
  route in a private configuration directory with the user's system sign-in store available. It presents the
  authorization page and asks for the code in a masked field. The code goes directly to the CLI's terminal as
  a bracketed paste followed by Enter. In the macOS development-daemon smoke run on 1.5.14, the private
  login reused completed onboarding flags and trusted its disposable workspace. After the owner submitted a real
  code, the daemon detected the CLI's success marker in the private login log, reported `succeeded`, and read
  the free plan's two weekly quota windows.
- The login is one item of the system keychain, service `gemini`, account `antigravity` (its attributes were read,
  never its secret), shared by the CLI, the app and the IDE (docs). No file under `~/.gemini` holds the signed-in
  address; only the log does (`consumerOAuth: authenticated successfully as …`). So there is one account for a user of
  the machine, and no record on disk says whether it is signed in. On macOS Octoboard reads the sign-in state from
  whether that item exists, never from its content ("Limits settled on 1.5.14" below). An account row with a
  directory of its own is refused wherever it would be used (a launch and the model listing answer
  `provider_unsupported` for `owned_accounts`; its sign-in state cannot be told).
- With the login keychain manually locked on macOS 26.2, the same presence-only lookup used by Octoboard
  returned status 0 in 0.020 seconds. There was no wait or observed prompt. This confirms item presence
  while locked and gives no information about whether the saved credential can be used. Both output streams
  were discarded.
- A launch with no login reports `state=blocked:kind=auth` with the message "Sign in required" over OSC 7501 (below).
  A signed-in launch sends the same report for a few hundredths of a second before `idle`.
- `agy models` prints `Fetching available models...` and then one `<id><TAB><label>` line per model. On the free
  plan: `gemini-3.8-flash-high|medium|low`, `claude-sonnet-4-6`, `claude-opus-4-6-thinking`, `gpt-oss-120b-medium`.
  `--model` and `--effort low|medium|high|xhigh|max` choose per session.

### Launch, resume and the session id

- Flags: `--conversation <id>` resumes, `-c` continues the latest of the directory, `-i <prompt>` starts with a first
  prompt and stays interactive, `-p` runs headless, `--mode accept-edits|auto|plan`,
  `--dangerously-skip-permissions`, `--add-dir <dir>` (repeatable), `--settings <json or file>` ("for this session
  only, never saved: a JSON object of settings.json keys"), `--log-file`. A file's path is taken, after a space or
  joined with `=`: a file naming a trusted folder and `modelProvider` gave a launch with no trust screen and the key's
  banner, and the file was left as written. A file with an unknown key ends the command as the JSON does (`Error:
  --settings: unknown setting "hooks"`), a missing one with `Error: reading --settings file: open <path>: no such file
  or directory`, both with status 1.
- No flag gives a new conversation an id. The id (a UUID) is first seen in the hooks of the first turn; on exit the
  CLI prints `Resume with -c (or command below):` and `agy --conversation=<id>`.
- A resumed conversation redraws its earlier turns, fires no `SessionStart`, and keeps its id in every later hook.
- What a conversation keeps (1.5.14, headless against a stand-in, 2026-11-22): the directory
  `antigravity-cli/brain/<id>/` (the transcripts `.system_generated/logs/transcript.jsonl` and
  `transcript_full.jsonl`, each a list by `step_index`, with their chunk files; the user's text is wrapped in
  `<USER_REQUEST>` with metadata blocks), the database `antigravity-cli/conversations/<id>.db`, and
  `antigravity-cli/presence/<id>.lock`. Shared, naming every conversation: `antigravity-cli/conversation_summaries.db`
  and `cache/last_conversations.json`. A headless run wrote no `history.jsonl`.
- **The database decides resumability** (1.5.14, macOS, stand-in, 2026-11-24). Individually removing the brain
  directory, the presence lock, or both shared indexes left resume on the original id with the old prompt in the
  next model request. Removing only `conversations/<id>.db` yielded `warning: conversation "<id>" not found` and a
  different id. The interactive launch with a missing id also printed that warning, opened a fresh composer and
  recorded its next prompt as a new conversation. Octoboard checks the database's presence before launching a resume.
- A running interactive conversation has `<id>.db-wal` and `<id>.db-shm` next to its database. After a TUI turn,
  `antigravity-cli/history.jsonl` contained prompt recall with `display`, `timestamp` and `workspace`, with no
  conversation id. Removal takes the database and its companions, the brain directory, the presence lock and the
  conversation's `annotations/<id>.pbtxt` title when present. The shared database, last-conversations index and
  workspace prompt recall stay in place.
- Octoboard reads the full transcript for history and the live feed. `input_tokens` on the latest
  `PLANNER_RESPONSE` is the request's input size: a stand-in request with 321 prompt tokens and 7 output tokens
  recorded `input_tokens: 321`. The transcript has no window size. History and the live feed date messages from
  `created_at`. The feed also shows tool calls. A single call's following `GENERIC` result can be matched to it,
  while results have no call id for pairing multiple calls.
- **Rewind** (1.5.14, stand-in): choosing the third prompt in `/rewind` removed its reply from the display and put
  the prompt back in the composer without changing the seven transcript lines. Replacing the prompt and sending
  it appended two lines with step indexes 5 and 6, reusing the indexes of the abandoned prompt and reply. Archive
  history and the live feed both replace the suffix from the reused index. Before a new turn materializes, the
  transcript itself gives no indication of the rewind.
- `Ctrl+D` on an empty prompt, twice, exits with status 0; `SIGTERM` at the prompt exits with status 0 too.

### Injection: a directory of the launch's own

- With `--gemini_dir=<dir>` the CLI reads `<dir>/config/hooks.json`, `<dir>/config/mcp_config.json` and
  `<dir>/GEMINI.md` as the global ones and keeps its own data in `<dir>/antigravity-cli`. Measured with a directory
  holding those three files written for the launch, `antigravity-cli` as a link to the home's own and the other
  entries of `config/` as links: the hooks fired, the MCP server started and was called, the text of `GEMINI.md`
  reached the model once (as a rule), and the session's only workspace folder was its working directory
  (`workspacePaths`, the model's "Active Workspaces", and the directory a command ran in). Through the link the trust
  the user gave went to the home's `settings.json` and the conversation to the home's `brain/`; a resume through a
  second such directory (the first deleted) found the conversation and fired its hooks. The CLI rewrote none of the
  links in that run. A hook's `transcriptPath` names the conversation through the launch's directory.
- What this costs: the flag is undocumented; the three files replace the user's global ones for the launch, so
  whatever the user has there has to be carried into them, and a server or hook the user adds from inside the session
  lands in the launch's copy (`agy --gemini_dir=<dir> mcp add …` wrote the launch's `mcp_config.json` and left the
  home's alone), so it is gone when the session ends.
- What the CLI writes by itself: nothing in those three files. A session that was trusted, ran a turn, changed its
  model with `/model` and its effort with `/effort` left the three copies byte for byte and replaced none of the
  links (those of the directory and those of `config/`): the trust and the model went through the `antigravity-cli`
  link into the home's `settings.json`. A user's own hook and MCP server carried into the copies ran beside
  Octoboard's.
- Telling a CLI that takes the flag from one that does not: an undefined flag ends any `agy` command at once with
  status 2 and `flags provided but not defined: -<name>` before the usage, while `agy --gemini_dir=<dir> mcp list`
  reads `<dir>/config/mcp_config.json`, prints its servers and exits with status 0, with no sign-in and no network.
  Status 2 is what every error of usage gives, so only the line tells the flag's.
  `--version` answers whatever flags come with it, so it tells nothing.
- How strictly the CLI reads `mcp_config.json` (`mcp list`): an empty file, one of white space only, `{}` and
  `{"mcpServers": null}` are no servers; a byte order mark, a `//` or `/* */` comment, a trailing comma, an array and
  anything else that is not strict JSON end it with status 1 and `Error: failed to parse <path>`.
- A session reads more than `mcp list` does (print mode in a scratch home, each case told by whether the file's
  server was started and its hooks fired, 2026-11-23). `mcp_config.json`: a byte order mark, `//` and `/* */`
  comments above or inside the object, and a comma before a closing brace, alone or together, were all read (the
  server started). A `#` comment, a file that is an array and `mcpServers` that is a list were not: the session ran
  with no server of the file and said so only in its log (`Failed to load MCP config: … failed to parse JSON config`).
  `hooks.json`: a byte order mark was read; a comment of either kind, a trailing comma or a `#` comment made the
  session load no hook of the file (`failed to parse hooks.json`, `loaded 0 named hooks`) and run on. Octoboard reads
  each file with exactly those options and refuses a launch on a file the session would not read
  (`provider.antigravity.user_config_unusable`), since the session would otherwise run without the user's servers or
  hooks and say nothing.
- A settings key given with `--settings` stands in place of the user's own of that name, and inside `permissions`
  each list does: with `permissions.allow` given, a command the user's own `allow` list covered asked for
  permission again, while a command of their `deny` list was still refused (`Permission denied for command(…).
  Matches user-configured d…`). `--dangerously-skip-permissions` keeps the `deny` list too: the denied command was
  refused, everything else ran with no prompt.
- What the CLI makes in a launch's directory built from a home that has no `config/` (or an empty one):
  `config/.migrated`, `config/config.json` (`{"plugins": {"swarm": {"enabled": true}}}`) and `config/projects/`
  (with `default-cli-project.json`). Given links to the home's own in their place, it made `.migrated` and the
  project file through the links, in the home; `config.json` it wrote by replacing the file, so the link became a
  file of the launch's and the home got none. A launch with no `--gemini_dir` also leaves an empty
  `config/mcp_config.json` in such a home.
- **`config/.migrated` marks a migration out of an earlier layout, and it must not run through a launch's
  directory.** With no marker in the configuration directory, the CLI moves `rules/`, `workflows/`, `skills/`,
  `plugins/`, `hooks.json` and `mcp_config.json` from its data directory (`antigravity-cli/`) into `config/`, leaves a
  link at each old place, and writes the empty marker (`migrate.go`: `Migration marker … does not exist. Attempting
  migration from AppDataDir …`, `Migrating … to …`, `Created symlink from … to …`); `config.json` and `projects/`
  there are not moved. In a home's own run that is all in the home. Through a launch's directory (its
  `antigravity-cli` a link to the home's, its `config/` its own) the same run moved the user's four directories and
  two files **out of the home into the launch's `config/`**, the two files over the ones written for the launch (so
  the launch's hooks and MCP server were the user's old ones, with none of the launch's own), and left the home's
  `antigravity-cli/` entries as links into the launch's directory, which lead nowhere once it is deleted. It did so
  whether `config/.migrated` was a link to the home's missing marker (the marker then lands in the home, so the
  user's own run never migrates) or absent (the marker lands in the launch's directory). With an empty
  `config/.migrated` file of the launch's own, nothing was moved and no marker was written to the home, the launch's
  hooks fired, and neither the old `hooks.json` nor the old server was loaded; the home's own next run, with no
  `--gemini_dir`, then migrated everything in place and loaded both.
- Which commands run that migration, in a home with the earlier layout and no marker: the interactive CLI and `agy
  models` do (after `agy models` the home had its marker, `skills` was a link and `config/hooks.json` was there);
  `agy --version`, `agy --help` and `agy mcp list` do not (`mcp list` printed `No MCP servers configured.`, reading
  `config/` alone). So Octoboard's probe of the directory flag (`mcp list`) never migrates, and a listing of the
  models run against the user's own home migrates it in place, as the user's own run would.
- **Where the destination is already there, the migration overwrites a file and leaves a directory.** In a home
  with no marker holding each entry in both places, a run of the interactive CLI and a run of `agy models` did the
  same: `config/hooks.json` and `config/mcp_config.json` were **replaced by the ones of `antigravity-cli/`** (the
  newer files' content was gone, with no copy kept), a link left at each old place; `skills/` and `workflows/`, whose
  destinations existed (one holding a file, one empty), were not moved, not merged and not linked (`Migration: failed
  to rename … file exists`), and stayed directories of `antigravity-cli/`; `rules/` and `plugins/`, which had no
  destination, were moved. The marker was written all the same (`Migration complete`), and a second run did nothing.
  So a run of the CLI always ends the state Octoboard refuses, but for a user who has both an earlier layout's
  `mcp_config.json` or `hooks.json` and a newer one in `config/` (which the Antigravity app also writes), that run,
  Octoboard's own listing of the models included, loses the newer file.
- What Octoboard does with it: a launch whose home has no marker the CLI would find (a link that leads nowhere is
  none) and still holds one of the six entries in `antigravity-cli/`, as anything but the link the migration leaves
  (one that leads to the entry of that name in `config/`), is refused
  before the launch's directory is made and before the flag is probed, with a remedy to run `agy` once
  (`provider.antigravity.earlier_layout_not_migrated`): behind the launch's own marker the session would run without
  the user's earlier rules, hooks and servers and say nothing, and a hook among them may be one that denies. The
  migration's own links refuse nothing; a link of the user's to anywhere else is an entry not yet moved.
- Where that home also has `hooks.json` or `mcp_config.json` in both places, the one in `config/` a file that holds
  something, the refusal is another, checked first (`provider.antigravity.earlier_layout_file_in_both_places`, naming
  both files): the run the first refusal asks for would replace the file in `config/` with the earlier one, so the
  user is told to keep the one they want and move or remove the other before making it. An empty file in `config/`,
  which a run of the CLI leaves in a home that had none, is not that case. Directories in both places are not: the
  CLI leaves both as they are.
- The listing of the models asks the same two questions before it runs `agy models`, and where a launch would be
  refused it runs nothing and gives the refusal as the reason it has no list. So nothing Octoboard runs by itself
  starts the migration in the user's home: of its runs of `agy` against that home, the version probe (`agy
  --version`) and the probe of the directory flag (`agy --gemini_dir=<launch directory> mcp list`) do not migrate,
  `agy models` is run only in a home that has its marker or nothing to move, and a session runs behind the marker of
  its launch directory. In a home with no marker and nothing of the six to move, `agy models` still writes the marker
  and an empty `config/mcp_config.json`, as any first run does. A listing that could not migrate at all would be `agy
  --gemini_dir=<a directory with its own marker> models`, which is recorded above as listing the signed-in account's
  models: since 2026-11-23 that is what Octoboard runs, in a directory of its own that it removes after, so its
  listing writes nothing into the home ("Limits settled on 1.5.14" below).
- What is left in `antigravity-cli/` after a run that could not move a directory: with the marker there and `skills/`
  a directory in both places, a session listed the skills of both (`/skills`: `Global skills · From
  ~/.gemini/antigravity-cli/skills` beside `From ~/.gemini/config`, and the screen names
  `~/.gemini/antigravity-cli/skills/{skill_name}/SKILL.md` as where to create a global skill) and named both to the
  model. So a `skills/` left there is still loaded. `rules/` and `plugins/` left there are not (2026-11-23, the
  marker present, an entry of each kind in both places): a rule file with the front matter `trigger: always_on`
  reached the model's request from `config/rules/` and not from `antigravity-cli/rules/`; a plugin installed with
  `agy plugin install <directory>` lands in `config/plugins/<name>/` and is listed and loaded (its skill named in
  the request), while a copy of it under `antigravity-cli/plugins/` is neither listed by `agy plugin list` nor
  loaded. A Markdown file in `workflows/` gave no slash command from either place (`Unknown command`), so nothing
  can be said of a `workflows/` left behind. Octoboard tells the user at a launch whose home has `rules/` or
  `plugins/` left there with something in them (`provider.antigravity.earlier_layout_leftovers_not_read`, naming
  both directories and, as `left`, which of the two is left: `rules`, `plugins` or `both`), and starts the session.
  It is a notice of its own beside the one of a settings file the CLI does not read
  (`provider.antigravity.settings_not_read`): a launch with both tells both, the settings one first, and the window
  keeps each until the user dismisses it.
- Other migrations run at every start and keep their state in the data directory (`manager.go`: `Migration
  [MIGRATION_ID_PLUGIN_ENABLEMENT] …`, which seeds `config.json`). After a first run through a launch's directory the
  home's own run found them `already … COMPLETED` and made no `config/config.json` in the home.
- `--settings` takes settings keys only, for the launch, and saves nothing: `permissions` (`{"allow":
  ["mcp(octoboard/*)"]}` let the server's tools run with no prompt), `trustedWorkspaces` (a folder named there got no
  trust screen), `modelProvider`, `toolPermission`, `agentMode`. An unknown key ends the launch: `Error: --settings:
  unknown setting "hooks"`. Hooks and MCP servers are not settings.
- How the CLI writes and reads its own `antigravity-cli/settings.json`: it writes a file beside it
  (`settings.json.<uuid>.tmp`) and renames that over it, at mode 0600 (a trusted folder and an `/effort` change each
  gave the file a new inode, and a hard link to the old one kept the old text), so a reader finds one whole version
  or the other. It reads strict JSON, with a byte order mark taken off (and then writes the file back without it): a
  `//` or `/* */` comment, a trailing comma or anything else that does not parse makes the session run on its
  defaults (no trusted folder, no `modelProvider`), leaves the file as it was, and says so only in its log (`failed
  to unmarshal cli settings (file may be corrupt, using defaults)`). A value of the wrong kind does the same for the
  whole file: `permissions` that is a list, `permissions.allow` that is a text and `trustedWorkspaces` that is a text
  each gave `failed to load cli settings, using defaults: invalid settings: <key>: invalid value …`. A key it does not
  know is passed over and the rest is read. Octoboard's launch does what the CLI does with such a file: it starts the
  session on the defaults, repeating none of the file's allow rules in the launch's settings, and tells the user the
  file is not read (`provider.antigravity.settings_not_read`). Only a settings file that cannot be read at all (a
  directory in its place, no permission), for which the CLI was not measured, still refuses the launch.
- **The CLI rejects the whole settings file for any key it knows whose value it does not take, and it knows 39**
  (2026-11-23; every JSON field name in the executable asked through `--settings`): `agentMode`,
  `allowNonWorkspaceAccess`, `altScreenMode`, `artifactReviewPolicy`, `clearScrollbackOnResize`, `colorScheme`,
  `contextWindow`, `copyOnSelect`, `customModelsConfig`, `disableSlashCommands`, `editor`, `editorMode`,
  `enableTerminalSandbox`, `gcp`, `hideActiveItems`, `historySize`, `model`, `modelProvider`, `notifications`,
  `orchestrator`, `permissions`, `pickerGrouping`, `queuedMessages`, `runningLightSpeed`, `sandboxProxy`,
  `seasonalThemesOff`, `seasonalThemesOptIn`, `showFeedbackSurvey`, `showThinking`, `showTips`, `statusLine`, `title`,
  `toolPermission`, `trustedWorkspaces`, `useG1Credits`, `useTerminalCursor`, `verbosity`, `vimInsertFirst`, `voice`.
  Several take only a few words (`toolPermission`, `agentMode`, `verbosity`: `unrecognized value "…"`), and inside
  `permissions` each of `allow`, `deny` and `ask` has to be a list of texts (`deny` that is a text, `ask` that is an
  object and `allow` holding a number each rejected the file; an unknown key inside `permissions` did not). An
  element that is `null` is taken: `["Bash(ls:*)",null]` and `[null]` in `allow`, `[null]` in `deny` and
  `["x",null]` in `ask` were read by the file's own load (no log line) and taken by `--settings` (status 0), while
  `[true]` was rejected by both, as `[1]` is (2026-11-23, a scratch home and directory, no sign-in, no turn). With
  such a file the user's `deny` rules are not in force either. The rule is the CLI's own, so Octoboard asks the CLI:
  `agy --gemini_dir=<the launch's directory> --settings <a copy of the file's keys> mcp list` validates what
  `--settings` names the same way, ends in about 0.1 s with status 1 and `Error: --settings: invalid value for
  setting "<key>": …` on its standard error (status 0 where it takes them), needs no sign-in and no network, and
  wrote nothing into the directory or the home. Measured against the file's own load (the log of `agy
  --gemini_dir=<dir> models` with the same text as `antigravity-cli/settings.json`) for each of the 39 keys with
  eight kinds of value (`null`, a text, a number, `true`, an empty list, a list of numbers, an empty object, an object
  of a number) and nine cases inside `permissions` and beside it, 321 in all: the two agreed on every one but two
  kinds, which the question leaves out. A key that is `null` is unset in the file, while `--settings` refuses `null`
  for nine keys (`altScreenMode`, `artifactReviewPolicy`, `colorScheme`, `editorMode`, `pickerGrouping`,
  `queuedMessages`, `runningLightSpeed`, `toolPermission`, `verbosity`): `null` keys are not asked about. A key the
  CLI does not know is passed over in the file, while `--settings` refuses it first, before any value, one key a run
  (`Error: --settings: unknown setting "<key>"`, the key quoted as a JSON text is): it is taken out and the question
  asked again. A file the CLI rejects so is taken as not read, with the same notice. The launch asks only once the
  CLI has not refused `--gemini_dir`, and each answer is kept for the binary, its version and the text asked, so a
  launch whose settings were asked about starts no process for them; a run that said nothing (timed out, could not
  start) is kept for five minutes only, then asked again, since it counts as the file read. Not covered: a CLI that cannot be asked (not
  found, a run that times out or ends some other way) leaves what Octoboard tells by itself, the kinds named above
  and each of `allow`, `deny` and `ask` a list of texts; values nested deeper than the kinds tried (inside `statusLine`, `gcp`, `voice` and the like)
  were compared only through the eight kinds; and the comparison is of 1.5.14.
- A rule kept from a permission prompt ("… (Persist to settings.json)") is added to the `permissions.allow` list
  **on disk**, not to the list the session runs with: with `allow` given through `--settings` as the user's own rule
  and `mcp(octoboard/*)`, choosing it for `echo` wrote `command(echo)` after the user's rule in the home's
  `settings.json` (rewritten indented, the `deny` and `ask` lists kept), wrote no `mcp(octoboard/*)` there and left
  the `--settings` file alone. The row above it ("always allow in this conversation") writes no rule.
- `--add-dir <dir>` loads the added directory's `.agents/hooks.json`, `.agents/mcp_config.json` and `AGENTS.md` with
  no trust question, but makes it a workspace folder, **and the folder whose path sorts first is the session's
  default directory**: commands run there, every hook's `cwd` names it and the model's list starts with it. The
  folders of a new conversation are sorted by path as given, `..` resolved and symbolic links not (`aa-inject` before
  the project put the commands in the injected directory; `zz-inject` did not; a link named `zz-link` to a directory
  sorting first kept the project first; `<project>/../../aa-sib` sorted as `aa-sib`). `--new-project` changes none of
  it. A resumed conversation keeps the folders it was created with, in that order, and appends a new `--add-dir`
  after them: a deleted directory stays listed. So an added directory is not a way in.
- The other places are the user's: `~/.gemini/config/hooks.json` and `mcp_config.json` (global, shared with the app),
  the project's own `.agents/`, and installed plugins (`agy plugin install <directory>`, which installs for every
  session; no flag or settings key loads one for a launch).
- A first prompt joined to its flag is taken whole, whatever it starts with: `--prompt-interactive=--model looks like
  a flag` and `-i=-x task with a dash` each reached the model as written. `-i -- <text>` is refused (the usage is
  printed).

### Hooks

- `hooks.json` maps a name to events: `SessionStart`, `PreInvocation`, `PostInvocation`, `Stop` take a list of
  handlers (`{"type": "command", "command": …, "timeout": <seconds>}`); `PreToolUse` and `PostToolUse` take
  `[{"matcher": "*", "hooks": [handler…]}]`. The payload is JSON on stdin, camelCase, and names its event as
  `hookEventName`. Common fields: `conversationId`, `cwd`, `workspacePaths`, `transcriptPath`
  (`…/brain/<id>/.system_generated/logs/transcript_full.jsonl`; the docs page says `transcript.jsonl`), `artifactDirectoryPath`, `modelName`, `lastUserInput`.
- A hook process inherits the CLI's environment, an API key in it included.
- Nothing fires at launch. A turn is `SessionStart` (the first turn of a new conversation only), then
  `PreInvocation` / `PostInvocation` once per model call (`invocationNum`, `initialNumSteps`), then `Stop` with
  `terminationReason` (`NO_TOOL_CALL` for a finished turn, `ERROR` for a failed one), `error` (the text, or empty),
  `fullyIdle`, `continuationActive` and `executionNum`.
- A tool call is `PreToolUse` and `PostToolUse` with `toolCall` `{id, name, args}` and `stepIdx`; `PostToolUse` adds
  `error`. `PreToolUse` fires before the permission prompt is drawn; `PostToolUse` only once the call ran.
- `PreToolUse` answers `{"decision": "allow"|"deny"|…, "reason": …}` on stdout. `deny` stops the call and the model
  is told `Encountered error in tool execution: tool call denied by pre-tool hook: <reason>`; the screen still lists
  the call as done ("Edited 1 file"). `allow` does not skip a permission prompt. A handler that fails (a syntax error
  here) blocks the call as well, and the log has its stderr.
- The tools that write a file are `write_file` and `edit_file`, each naming it as `Path`, through the stand-in and
  through a real model alike (one real turn that created a file, edited it and created two more called exactly
  these). A path that is not absolute is refused by the tool itself (`must be an absolute path`).
- A command declined at its permission prompt fires its `PreToolUse` and nothing after: no `PostToolUse`, no `Stop`.
- A tool event runs only an entry with a `matcher`: a plain handler under `PreToolUse` or `PostToolUse` never ran,
  for any tool, while `{"matcher": "*", "hooks": […]}` ran for every one. Every entry of an event's list whose matcher
  takes the call runs: a `*` entry and a `write_file|edit_file` entry in one `PreToolUse` list both ran for a
  `write_file`, and the second's `deny` stopped it (the file was not written) whatever the first gave. A handler that
  writes nothing and exits 0 neither allows nor denies: the call still drew its permission prompt (measured
  2026-11-24 against the stand-in, through a `hooks.json` shaped as a launch writes it).
- **What a launch registers** (`apps/daemon/src/adapter/antigravity/home.rs`): Octoboard's hook as a plain handler
  for `SessionStart` and `Stop`, and as a `*` entry for `PreToolUse` and `PostToolUse`, each event's command told its
  name; the cross-project guard, where the launch has one, as an entry of its own after it in `PreToolUse`'s list,
  for `write_file|edit_file`. Octoboard's hook writes nothing on stdout. Those are the events the provider reads (the
  conversation's id and the turn's reply, the background work, the pending interactions), which a test holds the
  registration to.
- Tool names differ from the docs page: `read_file`, `write_file` (`Path`, `Content`, `Overwrite`, `Append`),
  `edit_file` (`Path`, `Target`, `Replacement`), `run_command` (`CommandLine`, `toolSummary`, `WaitMsBeforeAsync`),
  `ask_question` (`questions[]` of `question`, `options`, `is_multi_select`), `call_mcp_tool`, and the subagent,
  schedule and search tools.
- A turn cancelled with Esc fires no `Stop`: the last event is its `PreInvocation`.

### MCP

- `mcp_config.json`: `{"mcpServers": {"<name>": {"command", "args", "env", "timeoutSeconds"}}}` (default 180 s a
  call, docs). The CLI starts the server at launch (`server/discover`, which a server may refuse, then `initialize`
  with protocol `2025-11-25` and `tools/list`).
- Tools are not declared to the model one by one: it gets `call_mcp_tool` `{ServerName, ToolName, Arguments}` and
  reads a schema file. **`call_mcp_tool`, `list_resources` and `read_resource` are offered only once a server has
  answered `tools/list`, and the CLI does not wait for that** (2026-11-23, the tools of each request read at the
  stand-in). A prompt given at launch (`--prompt-interactive=<task>`, in a trusted folder) went out 0.1 s after the
  first `idle`, its request held none of the three, and they were there from the second request on. A first message
  pasted half a second after the first `idle` had them in its first request. With a server that takes 4 s to start, a
  message sent 0.5 s after `idle` had none, and one sent after the server was up had them. So Octoboard never gives
  the task with `--prompt-interactive`: it is the session's first message, and neither it nor anything else is
  written into the session until Octoboard's own server has answered the CLI's `tools/list`, or 15 s have passed
  since the launch (`AgentProvider::awaits_tool_listing`). The CLI gives no sign of a server being ready, so the
  server itself tells the daemon. Measured on 2026-11-23 with a recording stdio server, no prompt sent and no turn
  run, 27 launches, the machine's load average 11 from other work: the CLI starts the server as its input box
  comes up (the server's process began between 0.02 s before and 0.07 s after the box was drawn, and 0.2 to 0.3 s
  before it in the two slow launches below), and `initialize` and `tools/list` followed within 0.01 s. From the
  launch, `tools/list` came after 0.17 to 0.24 s with the machine idle (8 launches; 1.0 s for the
  first of them and 3.1 s for a single launch before the series, each the first after a pause), 0.21 to 0.47 s with
  8 busy loops and 0.24 to 0.42 s with 16 (8 and 6 launches, more loops than cores). A server that slept 4 s before
  reading was asked `tools/list` 4.2 to 4.4 s after the launch, 4 s after the box (2 launches). In a folder that is
  not trusted the server is started and asked while the trust screen is up (0.22 s, 2 launches). So a message
  written the moment the box is up can precede the listing by a few hundredths of a second with a fast server, and
  by as long as a slow one takes; the bound is several times the slowest listing seen.
- A call asks first (`MCP`, `<server>/<tool>`, "Allow calling this tool?", six numbered rows from "Yes, allow tool
  call" to "No, and always deny … (Persist to settings.json)") unless a `permissions.allow` rule `mcp(<server>/*)`
  covers it. `tools/call` carries `_meta` with `antigravity.google/conversation_id` and the artifacts directory. The
  result reaches the model as `Created At: … Completed At: … <text>`.

### Account MCP inventory and compatibility

Measured on 1.5.14 (macOS, 2026-11-24, isolated homes and a local stand-in).

- `mcp list` printed a name, transport, enabled state and command or URL for the global
  `config/mcp_config.json`. The remote example was listed with either `url` or `serverUrl`. `mcp disable` wrote
  `disabled: true` to the named entry. The command listed no plugin server.
- A native plugin with `mcp_config.json` started the probe's stdio command from `config/plugins/<name>/`. The
  same configuration at `.mcp.json` did not start it. `plugin list --json` gave the plugin name, enabled state
  and absolute path, with no change to the populated home or to a scratch home seeded with an earlier-layout
  plugin. Octoboard uses that listing and the session's JSON reader for the global and enabled plugin MCP files.
  It lists configured active servers with runtime and remote sign-in unknown. A remote entry publishes only its
  scheme and authority, omitting userinfo, path and query.
- Project `AGENTS.md` and `GEMINI.md` both reached the same model request. The adjacent `CLAUDE.md`,
  `.claude/rules/`, `.cursor/rules/` and GitHub Copilot instructions did not. A native global MCP server
  started, while Gemini CLI's `settings.json` server and the Claude Code global and project MCP servers did not.
  Gemini CLI extensions are detected as in the initial probe above, and the probe's extension remained
  uninstalled and did not start its server. `--settings` rejected `contextFileName`, `loadGeminiExtensions`
  and `loadClaudeConfig`; the launch help names no such compatibility switch. The CLI exposes an explicit
  `plugin import` command for Gemini and Claude plugins, which is a configuration operation. Octoboard offers
  no additional per-launch compatibility switch. Its normal launch keeps the CLI's native instruction loading.

### Status over OSC 7501

- With `AGY_CLI_OSC7501=on` the CLI writes `ESC ] 7501 ; state=<state>[:kind=<kind>]:app=antigravity[:msg=<base64>]`
  on every change (without the variable it first asks the terminal whether it supports the reports; `off` disables
  them; `AGY_CLI_OSC7501_MESSAGES=brief` drops the details; binary and changelog). It sets no terminal title unless a
  `title` command is configured ("Capabilities measured on 1.5.14" below).
- Measured sequence: `idle` once the prompt is ready (after the trust screen is answered, or at launch in a trusted
  folder); `working` when a message is submitted; `done` with the reply's text when the turn finishes; `error` with
  the error text when it fails; `idle` when a turn is cancelled with Esc, with no `done`.
- **The `msg` of a `done` report is the reply cut to its first 199 characters and `…`** (202 bytes decoded, for a
  reply of 5,010 characters), by the CLI itself, well inside the 2,048 bytes a report may carry. So a parent that is
  told a turn's last message from the report gets its opening only. The whole reply is in no hook payload (`Stop`
  carries the termination reason and the error, not the text). Octoboard reads it from the conversation's
  transcript, which `Stop` names as `transcriptPath` through the launch's directory, and hands it to the turn end the
  report closes ("Limits settled on 1.5.14" below, "The transcript" to "The path `Stop` gives").
- A wait on the user is `blocked` with a kind: `permission` (`Trust this folder?`, `Approve Bash: <command>`,
  `Approve Edit: Write: <path>`, `Approve McpTool: <server>/<tool>`), `question` (the question's text; also `Finish
  setup` during the first-launch wizard) and `auth` (`Sign in required`). Answering returns to `working`.

### Screens and keys

- Trust, in a folder not in `trustedWorkspaces`: `Accessing workspace:`, the path, `Do you trust the contents of this
  project?`, `Antigravity CLI requires permission to read, edit, and execute files here.`, rows `> Yes, I trust this
  folder` and `No, exit`, footer `↑/↓ Navigate · enter Confirm`. Enter accepts: the CLI goes on in place, appends the
  folder to `trustedWorkspaces` in its settings and draws the prompt (no restart, no hook). "No, exit" ends the
  process with status 0 and saves nothing. Nothing of the prompt is drawn before the answer.
- **A first prompt given with `-i` runs before the trust screen is answered**: in an untrusted folder its whole turn
  (`SessionStart` to `Stop`, two model requests) completed in the first second, behind the trust screen, and showed on
  the screen after the answer. It ran once.
- The prompt: a banner (`Antigravity CLI <version>`, the signed-in address or `Gemini API key`, the model, the
  directory), a `>` composer between two rules and a footer `? for shortcuts` with the model at the right; `esc to
  cancel` replaces the footer during a turn and a prompt.
- Permission prompts are numbered lists answered with Enter (first row "Yes"): "Run this command?" (four rows,
  six once the highlight moves), "Allow creation of this file?" (two rows), the MCP one above. A digit answers its
  row at once ("Capabilities measured on 1.5.14" below). A letter typed at one is not an answer; a paste followed by
  Enter answered the highlighted row. `ask_question` draws `Question 1/1: <text>`, the options numbered,
  `Write-in...` last, footer `↑/↓ Navigate · enter Select · esc Skip`.
- In its default mode a file write inside the workspace and a command both asked first. Commands run in a terminal
  sandbox by default (docs). `--mode accept-edits` let the file write through with no prompt and still asked about the
  command; `--dangerously-skip-permissions` asked about neither. `--mode plan` and `--mode auto` behaved as the
  default against the stand-in (what they change is the model's own conduct, and `auto` needs an account and the
  sandbox); each was measured with the user's account ("Capabilities measured on 1.5.14" below).
- The input box is the rows between the last two full-width rules, the first starting with `>` and a row added with
  Ctrl-J indented under it; a shorter rule parts two turns of the conversation above. It stays drawn during a turn,
  under the turn's `Generating...` row. Ctrl-U empties the row the cursor is on and no more.
- After a failed turn, and at other times, the CLI draws a survey (`How's the CLI experience so far?` and `[1] Good
  [2] Fine  [3] Bad  [0] Skip`) in place of the input box. With `"showFeedbackSurvey": false` in the launch's
  settings the box stays.
- Esc during a turn cancels it: `⎿  Interrupted · What should Antigravity CLI do instead?`. A failed turn draws `⚠
  agent executor error: …` with an `Error ID:` line, and may be followed by a survey line (`[1] Good  [2] Fine  [3]
  Bad  [0] Skip`).
- A bracketed paste followed by Enter submits a message.

### The real turn

Two turns in all, both on the free plan. The second, through a launch directory as described above, asked a real
model to create a file, edit it and create two more: `write_file` and `edit_file` as recorded under "Hooks", 17 s,
`gemini-3.8-flash-low`. The first: one message ("Reply with the single word: pong") through the added-directory injection with the user's sign-in, the
real `HOME`, a scratch working directory and trust given by `--settings`: the same four hooks and the same reports
(`idle`, `working`, `done` with `pong`), model `gemini-3.8-flash-high`, 5 s. It drew on the free plan's quota. It left
one conversation in the user's `~/.gemini/antigravity-cli` (its `brain/` directory, a `presence/` lock, a history
line and a log); the user's settings were unchanged.

### Through a development daemon

Sessions run by a development daemon of the provider as first built (its data under a throwaway home, its sessions'
shell environment pointing at a scratch agent home and the stand-in, then at the real home for five real turns on the
free plan, `gemini-3.8-flash-low`). Two things these runs show have changed since: the task is no longer given with
`--prompt-interactive` but always as the first message, written once Octoboard's server has answered the CLI's
listing of its tools; and a turn's whole reply, not the report's cut `msg`, reaches the parent ("Limits settled on
1.5.14" below).

- A console session in a folder the CLI did not trust, with a first task: `starting`, `idle` (the task waiting as its
  first message), `waiting_user` while the trust screen was up, `idle` once Octoboard had pressed it, then `working`
  and `idle`, the task having run once and the conversation's id learnt from its first hook.
- A project session in a trusted folder, with a first task: launched with `--prompt-interactive=<task>` (since
  changed: the first message), `working` then `idle`. In a folder neither the CLI nor Octoboard trusts, the trust screen waits for the user
  (`waiting_user`), and the parent is told so.
- A command's permission prompt and a question each read `waiting_user` and went back to `working` on the answer; a
  failed turn left the session `idle` and paused; the input box stayed after it (the launch turns the survey off).
- A tool of Octoboard's (`list_consoles`, `report`, `instruct_session`, `interrupt_session`) ran with no prompt. With
  a real model the tool was called in the first turn of a cold session and answered: that session's first message went in a few
  seconds after the launch, by when the CLI offered `call_mcp_tool` (a first message now waits for the listing, "MCP"
  above).
- A report reached the parent as a message, and a turn that ended with no report reached it with the turn's reply as
  its text (then the `done` report's cut `msg`; now the whole reply).
- An instruction written while the session was idle started a turn. Written during a turn, the CLI held it above the
  input box (`▸ <text>… (alt+m to send immediately)`, footer `Press up to edit queued messages`) and sent it when the
  turn ended. **A cancel puts the messages still held back into the input box, unsent**: one row a message, in the
  order they were written, joined as rows added with Ctrl-J are, and nothing of the interrupted prompt beside them
  (two held messages and one, 2026-11-23, `antigravity_screen_cancel_gave_two_back.bin` and `…_gave_one_back.bin`).
  Nothing on the screen tells such a box from a draft of the user's with the same rows, but Octoboard knows the
  messages it wrote while the turn ran: a box that begins with the latest of them, whole and in the order written,
  holds those messages given back, and they are written again after its next message, only what follows them being
  told as a draft (`antigravity_screen_two_messages_held.bin` for the held state, in which the box reads empty). No
  one key clears a box of
  several rows (Esc does nothing at the prompt, Ctrl-C only arms the exit), but Ctrl-U pressed again does: a press
  empties the row the cursor is on, the next takes the empty row away, so two rows were gone after three presses.
  The provider declares both (`cancel_restores_held_messages`, `clears_input_box_with_ctrl_u`), so Octoboard clears
  the box before its next message instead of waiting behind it.
- An interrupt by the parent (Esc) stopped the turn: `idle`, `Interrupted · What should Antigravity CLI do instead?`.
- With the setting that keeps a session to its own project on, `write_file` and `edit_file` into another registered
  project were refused by the guard, the model reading Octoboard's reason; a write in the session's own project went
  on to the CLI's own prompt.
- A session resumed after being archived, and after the daemon was stopped and started, with `--conversation <id>`.
- **Every launch reports `blocked` / `auth` for a moment before `idle`** (0.1 s to 1.7 s measured), signed in or not,
  and with or without `AGY_CLI_OSC7501=on` (without it the CLI asks the terminal first, which a session's terminal
  answers). Octoboard first read it as `waiting_user` for that moment, and a parent was told its child was waiting
  for the user, which cost the parent a turn. It now counts such a report only once it has stood for 3 seconds, and
  with that: a signed-in child launched, archived and resumed under a signed-in parent went `starting` to `idle` each
  time and never read `waiting_user`, and the parent stayed `idle` with an empty input box, told nothing (no turn was
  spent). A launch with no login (a scratch home) read `idle` first and `waiting_user` 3.3 s later, at its `Select
  login method:` screen, for the parent and for the child alike, and one message was queued for the parent.
- A signed-in account's empty input box shows a hint for its mode, `Auto mode: edits & safe commands auto-approved
  (shift+tab to cycle)`, in grey (SGR 90, as the rules are), not faint. The executable holds the same hint for
  `Accept-edits mode`, `Plan mode` and `Best-of-N mode`. The banner names the plan after the address (`(Antigravity
  Starter Quota)` on the free plan), and the first launch after an update of the default mode shows a notice above
  the box (`Auto is now the default agent mode`, `esc to dismiss`).

- A trust screen behind the launch's sign-in moment, with the real sign-in (three real turns): a parent in a console
  directory the CLI had not seen went `starting`, `idle`, `waiting_user` for 0.4 s while its trust screen was up, and
  `idle` once Octoboard had pressed it. A child with a first task in a folder that neither the CLI nor Octoboard
  trusts went `starting`, `idle`, and `waiting_user` 0.2 s later, and the parent was told once that it waits for the
  user. When the screen was answered in the terminal the child went `idle`, then `working` as its first task went in,
  once, and `idle`; nothing was reported for the answered screen, and the parent received one report, with the turn's
  reply as its text.

Also measured apart from the daemon: a trusted folder does not cover the folders inside it (a child of a trusted
parent got the trust screen); a launch with no login draws `Welcome to the Antigravity CLI. You are currently not
signed in.` with `Select login method:` (`1. Google OAuth`, `2. Use a Google Cloud project`) and stays at `blocked` /
`auth`, and `agy models` then ends with status 1 and `Error: Please sign in to view available models. Launch the CLI
without arguments to sign in.`

### Limits settled on 1.5.14 (2026-11-23)

Measured with the stand-in in scratch homes unless a line says otherwise; no real turn was spent. 1.5.14 was still
the version `agy --version` printed.

- **The transcript.** `brain/<id>/.system_generated/logs/transcript_full.jsonl` holds an entry a line: `step_index`,
  `source`, `type`, `status`, `created_at`, and `content` or `tool_calls`. The user's message is `source`
  `USER_EXPLICIT`, `type` `USER_INPUT`, its text wrapped in `<USER_REQUEST>` with metadata blocks after it. Each
  answer of the model is `source` `MODEL`, `type` `PLANNER_RESPONSE`, with `model` and token counts, and `content`
  (the text it wrote, whole: 5,010 characters for the long reply) or `tool_calls` (`[{name, args}]`) for an answer
  that only calls tools. A tool's result is `type` `GENERIC` with the result as `content`. `transcript.jsonl` beside
  it has the same entries with long fields cut (`truncated_fields`, 4,119 characters of that reply). Each is
  mirrored under `chunks/<name>/00000000.jsonl`; a second chunk was never seen. When `Stop` fires the turn's last
  answer is already in the file (read by the hook itself); a cancelled turn and a failed one leave nothing after the
  user's message, and `Stop` of a failed turn (`terminationReason` `ERROR`) carries the error and no reply. A
  real model's conversations (seven on this machine, their shape read and none of their text) hold two more kinds
  of entry, both `source` `SYSTEM`: `type` `SYSTEM_MESSAGE` with `content`, seen after a `USER_INPUT`, between two
  answers and after the last answer, and `type` `ERROR_MESSAGE` with `error` and no `content`, seen between a
  `USER_INPUT` and the `PLANNER_RESPONSE` that answered it all the same. Neither is an answer or ends a turn
  (`testdata/antigravity_transcript_system_lines.jsonl`, the same shapes with made-up text).
- **Which turn a `Stop` is of.** `Stop` names no step, but its `lastUserInput` is the turn's message, and the
  transcript's `USER_INPUT` holds exactly that text between `<USER_REQUEST>\n` and `\n</USER_REQUEST>` at the start
  of its `content` (a message of two rows included). The turn cannot be taken to be the last in the file: a message
  written during a turn is held and sent by the CLI itself when the turn ends, and its `USER_INPUT` was appended
  0.17 s after the turn's `Stop` hook ended and its answer 0.32 s after (one held message; 0.50 s and 0.59 s with
  two, which went out as one message of two rows, `lastUserInput` `held message one\nheld message two`;
  `testdata/antigravity_transcript_held_messages.jsonl`). `fullyIdle` was `true` and `continuationActive` `false`
  in every `Stop`, the one with a message held behind it too, so neither says a turn follows. So the reply of a turn
  is the `content` of the last `PLANNER_RESPONSE` between the last `USER_INPUT` that is the `Stop`'s message and the
  `USER_INPUT` after it. Where the held message has the very text of the turn's own, the two cannot be told apart
  and the later one is taken. A `Stop` whose message no entry of what was read holds gives no reply, and the
  report's cut `msg` stands.
- **The path `Stop` gives** is spelled as the launch gave its directory: `--gemini_dir=/tmp/…/launch/../launch`
  (`/tmp` a link) gave `transcriptPath` `/tmp/…/launch/antigravity-cli/brain/<id>/…`, the `..` folded and no link
  resolved; resolved, it is `<home>/.gemini/antigravity-cli/brain/<id>/…`. Octoboard takes the path only where it is
  written as the launch directory of the session the payload was posted for spells that conversation's transcript
  (`<the session's scratch directory>/antigravity/antigravity-cli/brain/<id>/.system_generated/logs/transcript_full.jsonl`,
  the scratch directory being the one the daemon made for that session's process), compared as the host reads one
  path before anything is resolved, then resolves it and requires it to be that transcript right under what the
  launch's own link to the data directory resolves to, so that link is the one followed: a file of the same shape
  anywhere else, another session's launch directory included, is not opened, nor one a link below the data
  directory leads to, and a `Stop` that reaches a session with no process gives no reply. It reads the last 4 MiB of the file at most (`testdata/antigravity_transcript_full.jsonl`), and the
  report's cut `msg` stands where nothing is read.
- **A cancelled turn** reports `working` and then `idle` with no `done` and fires no `Stop` (above), which the
  program status path reads as a cancel: the session is at its prompt, paused, and no report goes to its owner.
- **What a session changes in the launch's own files is lost with the session.** The launch's directory holds its own
  `config/hooks.json`, `config/mcp_config.json` and `GEMINI.md`, and `config/config.json` becomes a file of the
  launch's once the CLI writes it (above). So a server added with `agy mcp add` from inside a session (measured), a
  plugin switched on or off (measured for a user who had no `config.json`), and whatever is written to the launch's
  copy of `hooks.json` or of the global rule file `GEMINI.md` are gone when the session ends, and nothing is carried
  back into `~/.gemini`. Not so a rule file in `config/rules/`: every other entry of the home's `config/` is a link
  in the launch's, so a file written into a `config/rules/` the home already has lands in the home and stays (by the
  layout; what a session does for a home with no `config/rules/` was not measured). The window therefore names
  hooks, MCP servers, plugin choices and `GEMINI.md`, and not "rules" (`provider.antigravity.sessionNote`, shown under
  the agent's name in its settings). To keep such a change, make it in `agy` run directly, outside Octoboard: every
  later session reads it from the home.
- **A link to a directory that is missing is not followed to make it.** With the launch's `antigravity-cli` a link
  to a `~/.gemini/antigravity-cli` that does not exist (the home with no `.gemini`, and with an empty one), the CLI
  ended with status 1 (`creating log directory: mkdir <launch>/antigravity-cli: file exists`) and made nothing in
  the home. With `antigravity-cli` there and `config/projects` such a link, it failed the same way at its first
  conversation (`create projects dir …: file exists`). With no `projects` entry at all it made
  `config/projects/default-cli-project.json` in the launch's directory and ran. So a launch can only reach a home's
  directories that exist: where they do not, Octoboard makes those two (`antigravity-cli` and `config/projects`)
  before it links them. The user ruled this allowed on 2026-11-23, and no exception to the rule that Octoboard
  leaves the user's own system as it was: "Those directories are things `~/.gemini` has anyway; they are not created specially for our application. I consider this allowed. You are over-controlling; it is not an exception, it is expected behaviour."
- **The listing through a directory of its own.** `agy --gemini_dir=<a fresh directory holding an empty
  config/.migrated> models`, with the user's own `HOME` and sign-in, printed the same six models as `agy models`
  (`Fetching available models...` goes to its standard error) with status 0. Nothing under `~/.gemini` changed
  (`find -newer` a file made before the run); in its own directory it left what a first start leaves
  (`antigravity-cli/` with its database, log and cache, `config/config.json`, `mcp_config.json`, `projects/`).
  Octoboard lists the models this way: the directory is made under its own run directory, private to the user,
  given its marker, and removed after the run however it ended. The earlier-layout refusals are still asked first,
  so a home a launch would be refused for lists nothing and says why.
- **Trust given by the launch's settings.** With `trustedWorkspaces` naming the working directory in the file
  given with `--settings`, a launch in a home that had never trusted the folder showed no trust screen, ran a turn,
  and left the home with no `settings.json` at all: nothing was recorded.
- **Whether the keychain item exists can be asked without a prompt** (macOS 26.2, the user's own keychain).
  `/usr/bin/security find-generic-password -s gemini -a antigravity`, with no `-w` and no `-g` so that it asks for
  the item and never its secret (its output was discarded), ended with status 0 in 0.02 s for the item that is
  there; the same for an account name that has no item ended with status 44 (`The specified item could not be found
  in the keychain`) in 0.04 s. Neither waited on anything, so no authorisation prompt was raised. Octoboard asks
  exactly this for the default account on macOS, through its generic question of an item's presence
  (`crate::accounts::credential_presence`, the provider naming only the item), bounded to 5 s, its output dropped
  unread: status 0 reads signed
  in (with no address: nothing on disk gives one), 44 signed out, anything else "cannot tell". Where the shell
  environment holds a `GEMINI_API_KEY`, a missing item reads "cannot tell" instead: the key route needs no item and
  that route does not establish a Google sign-in. On Windows Octoboard asks for the generic target
  `gemini:antigravity` in the daemon's logon session. On Linux it uses Secret Service's `SearchItems`,
  as described under "Sign-in". A locked match counts as present. A store that cannot be reached or
  does not answer in 5 s leaves presence unknown.
- **Instruction files that are links to one file give the model the text once.** In a working directory with
  `AGENTS.md` and, as links to it, `GEMINI.md`, `CLAUDE.md`, `QWEN.md` and `.github/copilot-instructions.md`, the
  text was in the model's request once, named as `GEMINI.md`.
- **`--gemini_dir`** is still taken by 1.5.14, the newest version this machine has. The CLI updates itself in the
  background, so an update that drops the flag stops every Antigravity CLI session with
  `provider.antigravity.launch_directory_flag_not_taken` until the provider is changed.

### Capabilities measured on 1.5.14 (2026-11-23)

Against the stand-in in scratch homes unless a line says otherwise. Two turns ran through a real model with the
user's sign-in, each with `--gemini_dir=<a scratch directory>` under the user's own `HOME`, `gemini-3.8-flash-low`,
a scratch working directory trusted through `--settings`, and the first-launch wizard skipped by
`cache/onboarding.json`; neither touched `~/.gemini`.

- **The task title.** The terminal title is set only to what a `title` command prints: `"title": {"type":
  "command", "command": …}` is a key of the settings (`windowTitle` and `terminalTitle` are refused as unknown),
  run through the shell (quotes and `$HOME` in it are the shell's) at each change of the agent's state, with a JSON
  payload on its standard input: `cwd`, `session_id` and `conversation_id` (both empty until the first turn),
  `transcript_path`, `model` (`id`, `display_name`, `effort`), `workspace`, `version`, `context_window`,
  `product`, `agent_state` (`authenticating`, `idle`, `working`), `sandbox`, `terminal_width`. The release notes say
  a `conversation_title` field was added in 1.1.27; it was absent from every payload, before and after the title was
  generated. The CLI writes the printed text as an OSC 2 title when it changes, and an empty title as it exits. The
  title it generates in the first turn (a request of its own to a lighter model, "You are a conversation title
  generator") is kept in `antigravity-cli/annotations/<conversation id>.pbtxt` as one line `title:"<text>"`, `"` and
  `\` escaped and CJK kept as UTF-8 (`title:"苹果 \"quoted\" title"`; the real turns' were `title:"Create Hello File
  And Ls"` and `title:"Create And Remove File"`), and in `conversation_summaries.db` (`conversation_summaries.title`).
  A command reading the annotation set the title 0.3 s into the first turn. So each launch gives a title command of its
  own in its `--settings`: Octoboard's title helper (`octoboardd agent-helper --provider antigravity --helper title --
  <annotations>`), which prints the annotation's title and `task_title` unescapes. It needs no script or tool of the
  host's; its command line is a POSIX shell's, as the hook commands are, so how it reaches the CLI on Windows, not
  measured, is the hooks' too. The launch's `title` stands in place of a title command of the user's own for the session.
- **Effort.** `--effort` is the level a model id carries: with the base id (`--model gemini-3.8-flash --effort
  medium`) the request went to `gemini-3.8-flash` with `thinkingConfig.thinkingLevel` `MEDIUM`, the same as
  `--model gemini-3.8-flash-medium`; the base alone runs its default level (`MEDIUM` for Flash). A level the base has
  no id for is refused (`error: invalid model selection (--model "" --effort "medium"): gemini-3.1-pro has no "medium"
  effort (available: low, high)`), so is another level beside a leveled id (`--model gemini-3.8-flash-high conflicts
  with --effort=low`), each with status 1 before any turn. With the user's account `--effort` for
  `claude-sonnet-4-6` and `claude-opus-4-6` was refused (`--effort is not supported for model …`), and
  `--model gpt-oss-120b --effort medium` and `--model gemini-3.8-flash --effort high` were taken (`-p "/usage"`, no
  turn). So the catalogue lists the ids of one base as one model under the base id with its levels as efforts, and a
  model whose id and label name no level keeps its id and takes none. With no `--model`, `--effort` applies to the
  CLI's default model (2026-11-24, scratch home against the stand-in, `-p`): `--effort high` sent `gemini-3.1-pro` a
  `thinkingBudget` of -1 where `--effort low` and no flag sent 1024, and `--effort medium` was refused as above. The
  interactive CLI does not refuse it (2026-11-24, the same home, a PTY at 120x40): `--effort medium` alone drew a
  warning under the start banner, `gemini-3.1-pro has no "medium" effort (available: low, high). Using "Gemini 3.1 Pro
  (Low)" instead.`, then the empty input box with `Gemini 3.1 Pro · low` in the footer, and the session went on. A
  launch with an effort and no model passes `--effort` alone; which model is the default is the CLI's, so Octoboard
  cannot refuse a level beforehand, and the CLI's warning is what says that another level is used.
- **`--mode plan`** (one real turn, asked to create a file and run `ls`): its first model call was a `write_file`,
  which reached the permission prompt ("Allow creation of this file?") as in the default mode; nothing was answered
  and no file was written. The executable's prompt says "Editing tools are refused while plan mode is on"; they were
  not. It does not behave as its name says on 1.5.14, so it is not offered.
- **`--mode auto`** (one real turn, asked to create a file, run `ls` and run `rm -rf ./hello.txt`): all three ran with
  no prompt, `PreToolUse` and `PostToolUse` around each, then `Stop` `NO_TOOL_CALL`; the box's hint is `Auto mode:
  edits & safe commands auto-approved (shift+tab to cycle)`. Offered as `auto`. `/config` in print mode lists
  `agentMode` `auto` as the default, and a signed-in launch with no `--mode` shows that hint.
- **Images.** An image file's path pasted on its own is taken as an attachment: the row above the box reads
  `▸ 📎 1 media attached (file, 168 B, image/png)  (ctrl+o to expand)`, then `▸ 📎 2 media attached  (ctrl+o to
  expand)`, and the next message carries each as `inlineData` (`image/png`) with `The user has uploaded 2 image(s):`
  in its metadata. An Enter right behind such a paste is lost; a path with other text in the paste stays text. Ctrl-U
  does not take an attachment away, Esc on an empty box does. A sent message shows `📎 2 image(s) (336 B)`.
- **Slash commands.** `/` opens a completion under the box (`> /add-dir  Add a directory to the workspace`, five
  rows and `↓ 39 more`, then `↑/↓ Navigate · enter Select · tab Complete`). A pasted prefix highlights the first match
  (`/cha`: `> /changelog`) and an Enter completes the box to it without running it; an exact name runs on Enter
  (`/changelog` opened the changelog); an unknown one shows `No matches`, and an Enter puts `Unknown command:
  /nosuchcmd` in the box. With arguments the completion offers the argument's values (`/model gemini-3.1-pro-high`:
  `> Gemini 3.1 Pro (High)  gemini-3.1-pro-high`) and an Enter ran it (`Model set to Gemini 3.1 Pro (High)`). A
  message pasted with a space before its slash was sent as text, the space dropped.
- **Background commands.** A `run_command` that outlasts its `WaitMsBeforeAsync` goes on as a task: its result to the
  model is `Tool is running as a background task with task id: <conversation id>/task-2`, with the description and a
  log under `brain/<id>/.system_generated/tasks/`; the turn ends with `Stop` `fullyIdle: false`, and no `done` report
  comes (the status stays `working`). The panel `● [22:50:05] sleep 6; echo finished-bg running` sits between the
  box and a rule of its own above the footer, which reads `1 task(s) · /tasks`. When the task ends, its
  `PostToolUse` fires (same call id), the CLI hands the model a `SYSTEM_MESSAGE` (`sender=<task id>`, `Task id "…"
  finished with result: …`, a `SYSTEM` entry of the transcript) and runs a turn of its own on it, firing
  `PreInvocation` / `PostInvocation` and a `Stop` with `fullyIdle: true`, and only then reports `done`. The model is
  also offered `manage_task`, `invoke_subagent` (it "runs in the background … and sends you a message when it
  completes"), `send_message`, `schedule`, `define_subagent`, `manage_subagents`, `run_workflow` and `swarm`.
- **Subagents.** `define_subagent` then `invoke_subagent` (`TypeName`, `Role`, `Prompt`) in one answer: the call
  returns at once (`Created the following subagent:` and its `conversationId`; an unknown type name fails with
  `subagent "general" not found or not allowed to be invoked`), the screen shows `Spawned 1 subagent` and a panel row
  `● Agent(sleeper: Probe Sleeper) · Gemini 3.1 Pro  Working · 1s` with `1 subagent(s)` in the footer, and the
  session's turn ends with `Stop` `fullyIdle: false` and no `done`. The subagent's conversation runs through the same
  hooks: `SessionStart`, its model calls' pairs and its own `Stop` (`fullyIdle: true`), each under its own
  `conversationId`, with `lastUserInput` empty (its task reaches it as a `SYSTEM_MESSAGE` from the session's
  conversation), where every event of the session's own conversation names the message its turn answers; a message
  of attached images alone is not sent (an Enter on a box holding only an attachment sent nothing). When it ends, the
  session's conversation gets a `SYSTEM_MESSAGE` (`Subagent <id> has gone idle. This was the subagent's most recent
  response: …`), runs a turn of its own on it, and its `Stop` says `fullyIdle: true`; then comes `done`. The
  conversation's `.system_generated/subagents/<id>.json` holds its type name, role and state. The subagent's row sits
  in the panel under the box as a command's does (`● Agent(sleeper: Probe Sleeper) · Gemini 3.1 Pro  Working · 1s`),
  and a permission prompt drawn while it runs hides the panel with the box.
- **Which conversation a turn is of.** Every turn of the session's own conversation the CLI was seen to start by
  itself names the user's last message as `lastUserInput`, as the turns the user starts do (measured 2026-11-24
  against the stand-in): the turn on a background command's result and the one on a subagent's hand-back (above),
  the turn on a `schedule` timer that fired (`DurationSeconds` 3: `PreToolUse`, `Stop` `fullyIdle: false`, the
  timer's `PostToolUse` 3 s later, then a turn and `Stop` `fullyIdle: true`), and `/compact` (a `Stop` naming
  `/compact`, `terminationReason` `TERMINAL_CUSTOM_HOOK`). A subagent's turns name it empty: its first, and the one a
  `send_message` from the session starts once it has gone idle (a `Stop` alone, `fullyIdle: true`, under the
  subagent's conversation id). After a resume (`--conversation <id>`) the CLI started no turn of its own: a subagent
  and a timer left running when the previous process ended (`SIGTERM`; the timer's `PostToolUse` fired as it ended)
  were not taken up again, and the first event was the `Stop` of the user's next message.
- **How Octoboard reads the background work** (`apps/daemon/src/adapter/antigravity/background.rs`): a command whose
  `PreToolUse` came and whose `PostToolUse` had not at a `Stop` with `fullyIdle: false` is listed by its call id, its
  command line as the description, and its `PostToolUse` takes it off; a subagent's events (told by their empty
  `lastUserInput`) list it under its conversation id from its first event, described by the role it was invoked with
  where it was the only one waiting, and its own `Stop` with `fullyIdle: true` takes it off; the session's `Stop` with
  `fullyIdle: true` empties the list. A subagent's events are read for nothing else, so its conversation id is never
  taken for the session's. A command whose prompt was declined fires its `PreToolUse` alone, and is dropped once a
  turn on another message begins. A turn is told by its message alone: no event Octoboard registers names the turn
  (`Stop`'s `executionNum` was 0 for every turn; `stepIdx` counts the conversation's steps), so a command declined
  before a turn on the same text is listed with a command that turn leaves running, until the `Stop` with
  `fullyIdle: true` that follows the running one's end. A `schedule` timer or cron job is listed and taken off as a
  command is, under the kind `schedule` (the tool's name; the CLI names no kind for it), described as the CLI draws
  the call: `4s: check the build`, `* * * * *: cron fired`.
- **Timers and cron jobs** (measured 2026-11-24 against the stand-in, with the hooks registered as the launch registers
  them; `apps/daemon/testdata/antigravity_hooks_schedule_timer.jsonl` and `…_schedule_cron.jsonl`). `schedule`
  takes `Prompt` and either `DurationSeconds` (a one-shot timer) or `CronExpression` (with `MaxIterations`,
  `IsDaemon`, `ExactTiming`), and also `TimerCondition`; the CLI's description says it "runs as a background task and
  returns immediately" and that `manage_task` cancels it. A timer (`DurationSeconds` 4, `Prompt` `check the build`)
  fired `PreToolUse`, then the turn's `Stop` with `fullyIdle: false`; the screen showed `Scheduling 1 timer (4s: check
  the build)`, the panel row `● [02:07:53] 4s timer running` and `1 task(s)` in the footer. When it fired, its
  `PostToolUse` (the same call id) came, then the CLI's turn on the prompt (a `SYSTEM_MESSAGE`) and a `Stop` with
  `fullyIdle: true`, and the panel was gone. A cron job (`* * * * *`, `MaxIterations` 2) drew `Scheduling 1 cron (* *
  * * *: cron fired)` and the row `cron (every minute) running`; its first run was a turn of its own ending in a `Stop`
  with `fullyIdle: false` and no `PostToolUse`, and its second and last run fired its `PostToolUse` before its turn,
  whose `Stop` said `fullyIdle: true`. A timer cancelled through `manage_task` (`Action` `kill`, `TaskId` as its
  call's result names it, `<conversation id>/task-2`; `apps/daemon/testdata/antigravity_hooks_schedule_cancelled.jsonl`)
  fired its own `PostToolUse` (its call id) right after the `manage_task` call's, then that turn's `Stop`; the screen
  drew `Killed 1 task (task-2)` and the panel dropped its row (`15m timer running`), keeping a cron job's
  (`cron (daily at 03:00) running`) scheduled beside it.
- **Interactions.** Measured against the stand-in in the default mode. On a permission prompt a digit answers its row at
  once (`2` on the command prompt allowed it for the conversation, `4` declined it, `1` allowed an MCP call), Down moves
  the highlight and shows the rows not drawn at first, and Tab ("Amend") on a "No" row did nothing visible and a paste
  after it was not taken. The command and MCP prompts offer six rows: allow, allow such calls in this conversation,
  allow them "(Persist to settings.json)", deny, deny them in this conversation, deny them "(Persist to settings.json)";
  the file prompts two. A rule kept "(Persist to settings.json)" goes into the user's own settings file. On
  `ask_question` a digit on a question of one choice picks its option and goes on to the next question, or sends the
  answers after the last; on a question of several choices (`[ ]` / `[x]` before each option) a digit ticks or unticks
  its option and moves the highlight there, space toggles the highlighted one, and Enter takes the ticked ones and goes
  on; the digit of `Write-in...` opens `Your answer:` under the rows, a paste goes into it and Enter takes it and goes
  on (on the first question of two as on the last); Esc skips the question on the screen, the model being told `User
  Skipped`, and on the last sends the answers. The model got `A1: Beta`, `A2: Red, Blue`, `A3: custom answer`. Octoboard
  publishes both kinds (`apps/daemon/src/adapter/antigravity/interaction.rs`): a permission prompt with every row it
  draws, none taking text, answered by the row's digit, each row that keeps a rule (in this conversation or "(Persist to
  settings.json)") as remembering the answer under the CLI's own label, so the user chooses where it is kept as in the
  terminal; the form only as the CLI drew it (its first question, nothing ticked, the highlight on the first option), a
  written answer offered on questions of one choice only (one on a question of several choices was not measured),
  skipped with Esc question by question.
- **Command groups at exit.** A command's shell (`zsh -c …`) is a child of `agy` and leads a process group of its
  own. A hang-up of the terminal and an exit by Ctrl-D twice each ended the command, but a subshell of it that ignored
  SIGHUP, SIGTERM and SIGINT outlived both, still running in that group. So `ends_command_groups_on_exit` stays off.
- **Native memory.** None of its own: the model is offered fourteen tools (`read_file`, `run_command`,
  `manage_task`, `send_message`, `schedule`, `invoke_subagent`, `define_subagent`, `manage_subagents`, `write_file`,
  `edit_file`, `search_web`, `run_workflow`, `swarm`, `ask_question`), none for a memory, the system prompt names no
  memory, `/config` lists 38 settings and none for one, and the real conversations left no memory or knowledge
  entry in their directory. The executable holds memory and knowledge fields (`disable_memory`,
  `knowledge_base_items`, …) of the protocol it shares with the Antigravity app; nothing in the CLI was seen to use
  them. `GEMINI.md` is the user's own rule file, not a memory. Declared `NativeMemory::Absent`.

### Plan quota

Measured on 1.5.14 on 2026-11-22 with the user's own sign-in, a personal Google account on the free plan (the banner's
"Antigravity Starter Quota"), every run with `--gemini_dir=<a scratch directory>` under the user's own `HOME` and
`AGY_CLI_DISABLE_AUTO_UPDATE=true`. No prompt was sent, so no turn ran.

- **Print mode answers `/usage` without a turn.** `agy -p "/usage"` prints one tab-separated record per bucket
  (`<group>`, `<bucket name>`, `<remaining>%`, `<reset time, UTC, RFC 3339>`) and exits with status 0 after five to
  nine seconds, with its stdin left open or closed alike. With `--output-format json` it prints one JSON line:
  `{"conversation_id": "", "status": "SUCCESS", "response": <the text>, "num_turns": 0, "usage": {… all 0},
  "command": {"name": "usage", "data": {"description": …, "groups": [{"name", "description", "buckets": [{"id",
  "name", "description"?, "window", "remaining_fraction", "reset_time"}]}]}}}`. On the free plan there were two groups
  of one bucket each: "Gemini Models" ("Models within this group: Gemini 3.8 Flash") with `id: "gemini-weekly"`, `name:
  "Weekly Limit Remaining"`, `window: "weekly"`, `remaining_fraction: 0.9565528035163879`, `reset_time:
  "2026-11-28T16:35:55Z"`; and "Claude and GPT models" (Claude Sonnet 4.6, Claude Opus 4.6, GPT-OSS 120B) with `id:
  "3p-weekly"`, `remaining_fraction: 1` and a `reset_time` a week from the reading that moved by a few seconds between
  readings: a window nothing has been spent in has not begun to count. `remaining_fraction` is what is left, not what
  is used. `/quota` is the same command; `-p "/help"` lists the commands print mode answers (`/agents`, `/changelog`,
  `/config`, `/credits`, `/effort`, `/help`, `/hooks`, `/model`, `/permissions`, `/plugin`, `/skills`, `/usage`).
- `-p "/credits"` failed on this account: status 1, `"status": "ERROR"`, `"error": "/credits failed: retrieving credits:
  no credits info found"`. By the CLI's own release notes (in the executable), an API-key connection has no account
  quota and these commands exit with status 2 there; that was not run.
- **The `/usage` panel in the TUI** ("Models & Quota") shows the account's address, then per group its models, "Weekly
  Limit Remaining", a bar with the percentage to two decimals, and "Refreshes in 151h 0m", or "Quota available" for
  the window nothing was spent in.
- **What a reading writes, and where.** Into the directory `--gemini_dir` names it wrote what any first start writes
  (`antigravity-cli/builtin/`, `cache/`, `log/cli-<time>.log` with `cli.log` linking the latest, `implicit/<uuid>.pb`,
  `installation_id`, `jetski_state.pbtxt`, `conversation_summaries.db`; `config/config.json`, an empty
  `config/mcp_config.json`, `config/.migrated`, `config/projects/`), about 1 MB, plus a log and an `implicit/` file
  per run. A fresh, empty directory needs nothing prepared: print mode shows no first-launch wizard. The user's
  `~/.gemini` (340 files and directories hashed before and after) was unchanged by every run of this section, and so
  were the keychain item's modification date and `~/Library/Application Support/antigravity/credential-user-scope`
  across a reading. Run without `--gemini_dir` the same command would write those files into the user's `~/.gemini`
  (not run there).
- **Octoboard's reading** (1.5.14, 2026-11-24): two fresh signed-in probes on the free account returned the
  two weekly groups in 5.5 and 5.8 seconds, with zero model turns. The JSON names both groups and gives no plan
  name. The provider uses `weekly_model` with the group label, giving “Gemini Models weekly” and “Claude and GPT
  models weekly” in the English window labels. The probe preserves the shell's home for the keychain, disables
  automatic update and supplies a private directory via `--gemini_dir`. It clears that directory when the run
  ends, including the log and `implicit/` residue. It sends no input and offers no command to start a quota
  window. Development-window rendering still needs a smoke check.
- **Signed out** (1.5.14, 2026-11-24): with an isolated home and no Google sign-in, the command wrote authentication
  and waiting messages to stderr and no JSON reply within 60 seconds. A timeout does not establish the account's
  sign-in state. After a failed quota read, the poll uses Antigravity's independent credential presence check.
  A signed-out result reports that the account is not logged in. A signed-in or unknown result keeps the quota failure.
- **Sources that gave nothing more**: the ACP mode (`agy acp`): `session/new` names the plan
  (`result._meta.antigravity.tier`: `{"name": "Antigravity Starter Quota", "id": "free-tier", "upgradeUri": …}`) and no
  quota, its `usage_update` is the context window (`used`, `size`), and `usage` and `_usage` are refused with -32601;
  the log, which holds no quota figure; and the files under the directory. The status line can show quota too (a
  custom `statusLine` command is given a JSON payload with quota buckets, by the release notes), but only inside a
  running TUI; not measured.

### Not measured

Successful completion of Google OAuth and hosted quota on Linux. On Windows, the user waived the interactive
session lifecycle beyond the first-run Terms of Service & Data Use screen.
Whether `transcript_full.jsonl` is ever cut into several chunk files; an organisation account through **Use a Google
Cloud project** or an account with a real Gemini API key; quota and AI credits on Google AI Pro or Ultra (the free
plan showed two weekly windows and `/credits` reported "no credits info found"); the status line's payload; a
subagent resumed by `send_message`, and the role of each of several subagents invoked at once; a written answer to a
question of several choices; the ACP mode beyond `initialize` and `session/new`; how the title command runs on
Windows.

### ACP mode

Measured on 1.5.14 on 2026-11-22 with `agy acp --gemini_dir=<a scratch directory>`, signed in on the free plan
through the keychain item. The probe wrote `initialize` (protocol version 1, no client capability) and then `session/new` (a
scratch working directory, no MCP server) as JSON-RPC lines on stdin, read the answers, sent no prompt, and ended the
input.

- `initialize` was answered after 5.7 s: `agentInfo` (`Antigravity ACP Server`, the CLI's version), four
  `authMethods` (`oauth-personal`, `oauth-business`, `gemini-api-key`, `agent-platform`) and capabilities for loading,
  resuming, closing and listing sessions.
- `session/new` was answered 4 s later with `sessionId`, `models`, `modes`, `configOptions` and `_meta`:
  - `models.availableModels` is the six ids `agy models` prints, with the same labels, and `currentModelId`
    (`gemini-3.8-flash-high`).
  - `configOptions` splits model and effort, which `agy models` does not: `model` lists four base models
    (`gemini-3.8-flash`, `claude-sonnet-4-6`, `claude-opus-4-6-thinking`, `gpt-oss-120b-medium`), and `thought_level`
    (`low`, `medium`, `high`) is the effort of the current one. Whether the levels differ per model was not measured:
    that takes setting the session's model.
  - `modes`: `default`, `accept-edits`, `auto`, `plan`, each with a description.
  - `_meta.antigravity.tier` names the account's plan: `{"name": "Antigravity Starter Quota", "id": "free-tier",
    "upgradeUri": …}`.
- Two notifications followed: `available_commands_update` (eleven slash commands with descriptions) and
  `usage_update` (`used: 0`, `size: 1048576`, the session's context).
- The session left a conversation database in the directory (`antigravity-cli/conversations/<session id>.db`), beside
  what any first run creates there. The process exited by itself 0.4 s after its input ended.
- Nothing was spent: no prompt was sent, and the session's usage read 0.

No provider code uses it yet.

## GitHub Copilot CLI

Unless stated otherwise, measurements used 1.0.114 (macOS,
the npm package `@github/copilot`) in a scratch home and working directory, with no sign-in: Copilot was pointed at a
local stand-in for the model (`COPILOT_OFFLINE=true`, `COPILOT_PROVIDER_BASE_URL`, `COPILOT_MODEL`; `copilot help
providers-env`), which answered with scripted text and tool calls. The native executable holds no readable code, but
on first use it unpacks its JavaScript into the cache directory (`pkg/<platform>-<arch>/<version>/`, `cli-main.js` among
it), which can be read; a fact taken from there says so. What is still open is listed under "Not measured" at the end.

**Windows 11 ARM64 (23H2, build 22631.6199).** npm's global install in a scratch prefix wrote
`copilot.cmd`. It ran on ARM64 Node 26.13.0 and reported **1.0.114**. The first invocation in a fresh
profile spent 26.5 s extracting its package. The install's manifest lets Octoboard read its
version without waiting for that extraction. In an interactive scheduled task, Octoboard's
inbox-ConPTY login flow reported success after the browser device authorization. There was no
plaintext-storage question. The bundled-host attempt ended before displaying a device page.
The desktop Credential Manager listing showed two Copilot-labelled GitHub entries. The native
reader in that desktop logon reported signed in for the existing account and signed out for
a scratch home naming an absent login. No credential dialog appeared. The daemon's
signed-in SDK probe reported a credit-period window, and one native CLI turn returned the
verification word. The sandboxed project test detected the folder-trust prompt and pressed it.
Afterwards the Windows composer drew `╻▄▄`, `┃`, and `╹▀▀` for its border, with one space
between `┃` and a typed draft. It drew no `❯` input marker. The provider now recognises this
complete frame. The rebuilt daemon sent a prompt after project trust. The stand-in used
the session read, report and instruction tools, and the expected sessions received the
report and instruction. The daemon interrupted an active local request and archived the
project. The resumed project sent again without a new folder trust prompt.

**Linux daemon** (1.0.114, Debian 13 arm64, glibc 2.41). A disposable account used the login flow below and
a local model stand-in. The console and trusted project session ran a turn, reached the injected Octoboard
MCP server, exchanged an instruction and a report, interrupted a turn and relaunched after archive. The
daemon's SDK quota probe returned `quota_not_signed_in` and no window with the simulated login. A hosted
device login using the container's Secret Service then let a private CLI SDK probe verify the saved login
and read the Free plan's `chat` AI credits with a remaining percentage and reset epoch. The probe used
`auth.getStatus`, `session.create` and `session.quota.refresh`. Its first 60-second attempt received no
authentication reply. The successful retry precreated its log directory and allowed up to 120 seconds
for that reply. The source `COPILOT_HOME` file contents were unchanged in both attempts. A follow-up trace
captured Copilot directly executing the SessionStart, UserPromptSubmitted and AgentStop scripts, each with a
`/bin/sh` shebang.

**Launch and resume.**

- `--session-id=<uuid>` starts a new session under that id. The id has to be a valid UUID (version and variant
  included): any other value is refused with "The value is not a valid UUID, so a new session cannot be created" and
  exit status 1. Given the id of an existing session it resumes that session instead.
- `--resume=<id>` resumes; for an id that matches nothing it prints "Error: No session, task, or name matched '<id>'."
  and exits with status 1, where `--session-id=` would have started a new session.
- A resume changes to the directory recorded for the session unless `COPILOT_DISABLE_RESUME_AUTO_CD` is `1` or `true`;
  with it set, the resumed session ran in the directory it was started in.
- `-i <prompt>` starts interactive mode on a prompt; `--model <id>` and `--reasoning-effort <level>` are plain flags.
  A prompt that starts with `-` is refused as an option when it is an argument of its own (`error: unexpected argument
  '--resume is how this task starts' found`); joined to the long flag, `--interactive=<prompt>`, it is taken whole.
- The `copilot` npm installs is a Node loader (`npm-loader.js`) that starts the platform package's native executable
  (`node_modules/@github/copilot-<platform>-<arch>/copilot`) as its child with inherited stdio. It forwards `SIGINT`,
  `SIGTERM`, `SIGHUP` and `SIGQUIT` to the child, and once the child has ended by a forwarded signal it ends itself by
  the same signal. A `SIGTERM` to the loader ended the session cleanly: the `sessionEnd` hook ran with `reason:
  "user_exit"`, the session's `events.jsonl` got its `session.shutdown`, no process was left behind, and the loader
  wrote the escape sequences that leave the alternate screen and turn mouse reporting and bracketed paste off.

**The record.** `$COPILOT_HOME/session-state/<session id>/events.jsonl` is append-only JSON lines, each `{type, data,
…}`. Among them: `user.message`, `assistant.turn_start` and `assistant.turn_end` (with a `turnId` that counts the model requests of
one prompt from `"0"`), `assistant.message` (`content`, `toolRequests`), `hook.start` and `hook.end` for every hook,
`permission.requested` and `permission.completed` (`result.kind`: `approved`, `cancelled`,
`denied-interactively-by-user`), `abort` when the user cancels a turn (`reason` `user_abort` for Esc twice,
`user_initiated` for Ctrl-C and for Esc at a permission prompt), and `session.error` (`message`, `statusCode`) when a turn ends in an error. A subagent's lines carry its `agentId`
beside `type`; the session's own carry none.

What a session keeps (1.0.114): the directory `session-state/<id>/` (`events.jsonl`, `workspace.yaml`,
`checkpoints/`, `rewind-file-snapshots/`, `files/`, `research/`), the lock
`session-state/.session-operation-locks/<id>.lock`, and the index database `session-store.db` beside the records
directory. The session directory is the record Octoboard locates and follows through `events.jsonl`. Removing the
conversation removes its directory and its operation lock. Copilot's index retains the row until Copilot updates it.
A resume of an id with no directory is refused even if the index still has its row.

- **A carried directory is sufficient.** In two Copilot homes of the same macOS keychain login, the destination
  opened the original prompt and response after receiving just the session directory. Copilot registered the
  carried id in its own index after a rename in the resumed session. Octoboard carries the session directory.
- **History and the live feed.** `user.message.data.content` and `assistant.message.data.content` hold the prompt and
  reply. A subagent's events have `agentId`; completion notices have their own event type, and a
  `<system_notification>` prompt is synthetic. The live feed also reads `tool.execution_start` and
  `tool.execution_complete`, paired by `toolCallId`, with completion status and output. In the measured runs
  Copilot flushed complete events in batches. The feed updates as those batches arrive.
- **Rewind rewrites the file.** After two stand-in turns, selecting the second turn and then **Conversation only**
  removed its events from `events.jsonl` and appended a `session.info` with `infoType: "rewind"`. The file now had
  only the first turn. The readers rebuild from the rewritten file. The `id` and `parentId` fields alone are
  insufficient for ancestry-based filtering, since a `session.usage_record` and its `assistant.message` can both
  name the same parent.
- **Context.** The latest main `session.usage_record` with `interactionType: "conversation-agent"` gives
  `usage.inputTokens` and `usage.maxPromptTokens`. With an input of 320 tokens and 200 cache hits, the record had
  `inputTokens: 320` and `cacheReadTokens: 200`. The real turn recorded 18,293 input tokens with a 922,000-token
  limit. The terminal's subsequent `/context` showed `19k/922k tokens (2%)`, its rounded estimate after the
  reply. Octoboard shows the latest recorded request, with the share rounded to a whole percent.
- **Title.** After the first stand-in prompt `workspace.yaml` had `name: First milestone probe` and
  `user_named: false`. The hosted model later supplied a generated name for its turn. `/rename Milestone: reader's
  title` wrote a single-quoted YAML name with a doubled apostrophe and changed `user_named` to `true`. Both
  automatic and explicit titles are shown in Copilot's terminal, and Octoboard reads both. The captures are
  `apps/daemon/testdata/copilot_rewound_events_1.0.114.jsonl` and
  `apps/daemon/testdata/copilot_workspace_title_1.0.114.yaml`.

**Hooks.** A hooks file is `{"version": 1, "hooks": {"<event>": [entry, …]}}`. An entry of `type: "command"` runs
`bash`, `powershell`, or a shell-free `exec` with `args`; `timeoutSec` defaults to 30; `matcher` is a regular
expression over the tool's name (for `preToolUse`, `postToolUse`, `permissionRequest`) that has to match it whole.

- **A native payload does not name its event.** Every payload carries `sessionId`, `timestamp` (epoch milliseconds)
  and `cwd`, and nothing that says which event it is, except `notification`, which carries `hook_event_name:
  "Notification"`, and `permissionRequest`, which carries `hookName`. So each entry has to be told its event.
- **`sessionStart` runs with the session's first prompt, not at start-up**: a session started with no prompt ran no
  hook at all for as long as it sat at its prompt, and ran `sessionStart` (`source: "new"`, `initialPrompt`) just
  before `userPromptSubmitted` once a prompt was sent. A resume is the same, with `source: "resume"`. No hook runs
  while the folder-trust prompt is up, with or without `-i`.
- `userPromptSubmitted` carries `prompt`, verbatim for a bracketed paste of two lines followed by Enter.
- `preToolUse`, `postToolUse` and `postToolUseFailure` carry `toolCallId`, `toolName` and `toolArgs`; the post events
  add `toolResult` (`resultType`, `textResultForLlm`) or `error` (a string). There is no turn id in any payload.
- **The file-writing tools differ by model family, and so does the shape of `toolArgs`.** Asked through a stand-in
  under each offered model's id, Copilot gives every family but GPT (`claude-*`, `gemini-*`, `grok-*`, `kimi-*`,
  `mai-*`, and a model it does not know) the tools `create` and `edit`, whose `toolArgs` is an object naming the file as
  `path` (`{"path", "file_text"}`, `{"path", "old_str", "new_str"}`). The GPT models (`gpt-5.x`, `gpt-6.x`, the codex
  ones) get neither: they get `apply_patch`, a free-form tool, whose `toolArgs` is **the patch itself as a string**
  (`*** Begin Patch`, then `*** Add File: <path>`, `*** Update File: <path>`, `*** Delete File: <path>` or `*** Move
  to: <path>` headers, `*** End Patch`). Which of the two sets a hosted model gets is not told by its name alone: on a Copilot Free account Copilot Fusion
  chose MAI Code 2 Flash, which called `apply_patch`, where the stand-in under the id `mai-code-1.1-flash` was given
  `create` and `edit`. So all three are taken as file-writing tools whatever the model. Octoboard's cross-project
  guard, registered for `(create|edit|apply_patch)`, denied each of the three for a file in another project (the
  hosted `apply_patch` among them: "Denied by preToolUse hook: Blocked: this file is in project …", no file made) and
  let a `create` inside the project through. For any of the three the permission prompt's `permissionRequest` names the tool
  `edit` and carries `toolInput` `{"file_path", "diff"}`, and the notification reads "Edit file: <path>". A `matcher`
  is, by GitHub's hooks reference, anchored at both ends as a whole (`^(?:…)$`); Octoboard groups its alternatives
  so that it holds either way.
- A permission prompt answered "3. No, and tell Copilot what to do differently" runs no post event for the call: the
  record gets `permission.completed` with `result.kind: "denied-interactively-by-user"`, and the turn carries on to its
  `agentStop`.
- One model message that calls a tool needing a prompt and one that does not (`bash` with a `touch`, and `view`) ran
  both `preToolUse`, then `view`'s `postToolUse`, and only then the prompt's `permissionRequest` and notification: the
  other call had ended before the prompt went up.
- **A call can end while another call's permission prompt is up** (2026-11-23). One model message that calls `bash` (a
  `touch`) and an MCP tool that takes 6 s ran both `preToolUse`, the prompt's `permissionRequest` and notification, the
  MCP call's `postToolUse` 6 s later with the prompt still on screen, and the `bash` call's `postToolUse` only once the
  prompt was answered. The notification names no call, but the record does: `permission.requested` carries
  `permissionRequest.toolCallId` and a `requestId`, which the `permission.completed` repeats, and is made before the
  notification hook is run. Octoboard holds that call as the one waiting, so the other call's end leaves the hand up.
- **The record is written to its file in batches, later than its lines' own times** (2026-11-23, a watcher reading
  the file every 2 ms beside hooks that listed the file when they started). `events.jsonl` does not exist until the
  session's first prompt; from then on a burst of events reaches the file about 0.1 s after its last event, each new
  event putting the write off again: the line of a `permission.requested` was in the file 0.16 s after its own time, the
  first line of a burst up to 0.4 s after, and `permission.completed` and `session.shutdown` within 5 ms. So a hook
  that reads the record when it starts finds it behind: the notification hook of a prompt did not find the prompt's
  `permission.requested`, and an `agentStop` hook found neither the turn's last message nor the result of the turn's
  last call (with the stand-in, whose answers come at once, the whole first turn: the file was not there). Octoboard
  therefore reads the record for the prompt behind a raised hand half a second after the hand went up, so a call that
  ends within that time of another call's prompt still lowers the hand. What `agentStop` reads is corrected the same
  way: the record's own `hook.start` line of that `agentStop` is written with the turn's last lines, so Octoboard's
  read at a turn's end looks again every 0.1 s, for half a second at most, until that line is in, and only then takes
  the turn's final message and the work its last call started. A record still behind after half a second gives what it
  holds, and the rest is read at the next `agentStop`.
- `permission.completed` is in the file before the call's `postToolUse` hook starts (one run: written 22 ms before the
  hook, which found it there).
- **An MCP elicitation raises no notification and leaves no line of its own** (2026-11-23, a stub MCP server whose
  tool sends `elicitation/create`; Copilot declares `elicitation: {form, url}` to the server). The form was drawn and
  answered, the server got `action: "accept"` with the content, and the only hooks were the call's `preToolUse`,
  `permissionRequest` and, 11 s later, `postToolUse`: no `notification` of type `elicitation_dialog` (that type came
  only with `ask_user`) and nothing in the record between the call's start and its end. The form is drawn on the
  screen as `ask_user`'s is: `<server> needs information.` above the message and the fields, and a footer ending
  `ctrl+d decline · esc cancel` whichever field has the focus; a pasted message is dropped there and the Enter answers
  the focused field (2026-11-23, the stand-in, captures in `apps/daemon/testdata/copilot-captures.md`, "The form").
  Octoboard reads that screen as a dialog that puts something to the user (`copilot_form`), so the session reads as
  waiting for the user and holds its writes while the form is up. Copilot also reports `state=blocked:kind=question`
  over OSC 7501 while it is up.
- **Copilot does not wait for a `notification` hook.** With the hook's delivery held back 4 s, the prompt was drawn and
  answered, `postToolUse` and `agentStop` ran, and the `permission_prompt` notification arrived 3.8 s after the turn's
  end. The record then holds the request completed and a `hook.start` of `agentStop` after it; Octoboard reads that
  when a notification raises the hand and lowers it again, to working or, the turn over, to idle. In the record a
  call's `tool.execution_start` comes before its `preToolUse` hook, its `tool.execution_complete` after its
  `postToolUse` hook has ended.
- **`permissionRequest` runs for a call that is then allowed without a prompt too** (an MCP tool under an `--allow-tool`
  rule, a shell command under `--allow-all`), so it does not say that a prompt is up. Its payload is `hookName`,
  `toolName` (an MCP tool as `<server>/<tool>` here, `<server>-<tool>` everywhere else), `toolInput` and
  `permissionSuggestions`.
- `notification` carries `notification_type`, `title` and `message`: `permission_prompt` ("Permission needed") when a
  permission prompt is drawn, `elicitation_dialog` ("Information requested") when `ask_user` asks.
- `agentStop` carries `transcriptPath` (the session's `events.jsonl`), `stopReason: "end_turn"` and
  `stop_hook_active`; it carries no final message. The turn's last `assistant.message` is in the record by then.
- A subagent's own events (`userPromptSubmitted`, `preToolUse`, `permissionRequest`, `errorOccurred`, `agentStop`) and
  `subagentStart` and `subagentStop` carry `agentId`, `agentType` and `parentAgentId`; the session's own events carry
  none of them. `subagentStop` adds `response`. A subagent's `postToolUse` carries them too. One event about a
  subagent's tool call carries none: the `permission_prompt` notification of its permission prompt, which so reads as
  the session's own. The answer to that prompt is in the record as a `permission.completed` line with the subagent's
  `agentId`, and no event of the session's own follows it until the subagent is done. A subagent has no `ask_user`
  tool: its model's call of it ends with `postToolUseFailure` ("Tool 'ask_user' does not exist."), with no form. A
  subagent whose model request fails ends the session's `task` call with `postToolUseFailure`.
- `errorOccurred` carries `error` (`message`, `name`, `stack`), `errorContext` and `recoverable`. **It runs once for
  every failed model request, retried or final, always with `recoverable: true`**: a request answered with status 500
  ran it six times over 16 s while Copilot retried, and a request answered with status 400 once. Neither turn ran
  `agentStop`. So `errorOccurred` does not say that a turn has ended; the record's `session.error` does.
- `sessionEnd` carries `reason` (`user_exit` after a `SIGTERM`).
- **A turn the user cancels runs no hook.** Esc twice and Ctrl-C while the model wrote both left `userPromptSubmitted`
  as the turn's last hook; the record got `abort`. Esc at a permission prompt is the same: `notification` is the last
  hook, and the record gets `permission.completed` with `result.kind: "cancelled"` and then `abort`.
- `ask_user` runs `preToolUse`, `notification` (`elicitation_dialog`) and, once answered, `postToolUse`; Esc at its
  form also runs `postToolUse` ("User cancelled the request.") and the turn carries on.
- A `preToolUse` command that prints `{"permissionDecision": "deny", "permissionDecisionReason": "…"}` and exits 0
  denies the call: the model is told "Denied by preToolUse hook: <reason>", and no post event runs for that call.
- Hooks get `COPILOT_CLI=1` and `COPILOT_HOOK_DEPTH`. A shell command the agent runs gets `COPILOT_CLI=1`,
  `COPILOT_AGENT_SESSION_ID` (the session's id), `COPILOT_LOADER_PID`, `COPILOT_CLI_PATH`,
  `COPILOT_CLI_BINARY_VERSION` and `COPILOT_CLI_RESOLVED_DIST_DIR`, beside what Copilot itself was started with.
- A message typed and sent while a turn runs is shown under the timeline as pending ("pending · ↑ recall") and is
  not sent to the model while the model is writing; Ctrl-C then drops every pending one with the turn: no
  `userPromptSubmitted` runs for them and the record gets no `user.message`. One sent while a shell command runs
  moves that command to the background at once (its call ends with "moved to background by the user"), and the
  message is in the next model request, after the call's result: it joins the running turn. No `userPromptSubmitted`
  runs for it either, though the record gets its `user.message`. The command's completion then arrives as a prompt
  wrapped in `<system_notification>` (`Shell command "<description>" (shellId: N) has completed successfully.`)
  with a `shell_completed` notification.
- A model answer with no text ends the turn with `agentStop` and then a `session.error` ("No response was returned…")
  in the record.

**OSC 7501.** Copilot reports its own state through the Program Status Protocol without being asked, as
`state=<state>:app=copilot`: `idle` at start-up (before the folder-trust prompt is answered), `working` when a turn
starts, `blocked` with `kind=permission` at a permission prompt and `kind=question` at an `ask_user` form or an MCP
server's form, `done` when a
turn ends normally, `idle` when the user cancels it, and `error` when it ends in an error (after the last retry).

**Terminal title** (OSC 0). `GitHub Copilot` until the first prompt, then `<the session's name> - GitHub Copilot`,
where the name was the first prompt's text with its line breaks dropped. It did not change while a turn ran, at a
permission prompt, or for a later prompt, and a resume shows the same title from the start. It carries no activity
glyph.

**Injection.**

- `--plugin-dir <directory>` loads a plugin for the run with no install step and no trust decision: a directory
  holding `plugin.json` (`{"name": …}`) and `hooks.json` had its hooks run in a folder that was never trusted.
- `--additional-mcp-config @<file>` takes `{"mcpServers": {"<name>": {"type": "local", "command": …, "args": […]}}}`.
  A server's tool reaches the model as `<server>-<tool>` (`octoboard-report`). `--allow-tool=<server>` lets every tool
  of that server run without a prompt. **A server entry's `timeout`, in milliseconds, is the limit on one call**: with
  `5000`, a call that took 10 s ended after 5 s, the model was told "MCP server '<name>': McpError: MCP error -32001:
  Request timed out", and `postToolUseFailure` ran with that text as its `error`.
- `COPILOT_CUSTOM_INSTRUCTIONS_DIRS` is a comma-separated list of directories. Of the files tried in such a directory,
  **only `instructions/<name>.instructions.md` was put into the system prompt**, whole and with no `applyTo` front
  matter; an `AGENTS.md` there was only listed for the model to read as a "nested AGENTS.md", and
  `copilot-instructions.md` and `.github/copilot-instructions.md` there were not read. The system prompt is built
  again on every launch: a resume carried the file's current text, not the text of the first launch.
- A working directory holding `AGENTS.md`, `CLAUDE.md`, `GEMINI.md` and `QWEN.md` as links to one file put that file's
  text into the system prompt once, in one `<custom_instruction>` block.

**Folder trust for one run** (2026-11-23, each in a new home). Enter on the first option, `1. Yes`, trusts the folder for
the session and writes nothing: `config.json` got no `trustedFolders`. No flag does the same for an interactive
session started as Octoboard starts one: `--trust-folder` is refused beside `--session-id` ("the argument
'--session-id <id>' cannot be used with '--trust-folder'", and by its help it is for one prompt-mode session);
`--add-dir=<the folder>` and `--allow-all-paths` still drew the confirmation; `COPILOT_ALLOW_ALL=true`, spelled
exactly so, drew none and recorded nothing, but it also lets every tool run without asking.

**Folder trust.** In a folder that is not trusted Copilot draws "Confirm folder trust", the folder's path, "Do you
trust the files in this folder?" and three options with `❯` in front of the one the cursor is on, which starts on the
first: `1. Yes` (for this session), `2. Yes, and remember this folder for future sessions` and `3. No (Esc)`. Down
moves the cursor to the second, and Enter there adds the folder to `trustedFolders` in `$COPILOT_HOME/config.json`
(JSON with two `//` comment lines above it) and prints "Folder <path> has been added to trusted folders." A subfolder
of a trusted folder is trusted. A first prompt given with `-i` runs only once the prompt is answered.

**The composer.** Copilot's input box is the rows between two horizontal rules of `─` across the window, the first
starting with `❯`. Both rules are drawn faint (SGR 2), the mark and the draft plain, so a reading of the screen that
blanks faint cells sees two empty rows where the rules are. Copilot fills the window: the footer (`← open sidebar · …`
at the prompt, `◉ Working · …` during a turn) is always the last row, the bottom rule the row above it, and the
composer's rows end right above that. The footer is cut short and never wrapped, at 120, 80, 60 and 40 columns
alike. With the sidebar open (Left at an empty prompt) the whole view is drawn to the right of the sidebar's border,
`│❯ <draft>`, so the mark is no longer in the first column; the rules start at the border too, the border is drawn in
a colour (SGR 90) and not faint, and the sidebar's own column holds the sessions' names. Octoboard reads the composer
there as what is right of each row's first border (captured at 120 and 80 columns, 2026-11-23). The sidebar is not
kept across launches: a session started after one that ended with it open came up with it closed. A draft's wrapped
rows and further lines are indented two columns. A draft
with an empty line in it shows that line as an empty row of the box; an empty composer shows `❯` alone, with no
placeholder. The composer is drawn only once the folder-trust prompt has been answered, and at once in a folder already trusted: in three captures of an untrusted folder, with and without
`-i`, no `❯` but the trust prompt's cursor was written before the answer. A prompt already sent is shown in the
timeline as ` ❯ <text>` too, indented one column, below text and not below a rule. A permission prompt and an
`ask_user` form take the composer's place while they are up: the form sits between two rules as the composer does,
its cursor row ` ❯ <choice>` indented and below the form's own lines; a menu's cursor row is inside a box (`│ ❯ 1.
Yes`). The model picker (`/model`) has a search row ` ❯  Search models…` right below a rule, indented too. So the
composer's `❯` is the only one in the window's first column. A draft longer than the window wraps onto rows indented
two columns, as a second line of the draft is. The captures are `apps/daemon/testdata/copilot-captures.md`.

**Permissions.** A session starts in "Manual Approval" (the footer says so). `--allow-all` starts it in "Allow All",
where a shell command ran with no prompt. `--assisted-approval` is refused in an interactive session
("--assisted-approval is only supported in non-interactive prompt mode or direct workflow mode.", exit status 1), so
`--allow-all` and its parts are the only permission mode a flag sets for one. A permission prompt reads "Do you want to
run this command?" with `1. Yes`, `2. Yes, and don't ask again for …` and `3. No, and tell Copilot what to do
differently (Esc to stop)`.

**Keys that interrupt a turn.** Ctrl-C while a turn runs stops it at once, and leaves a draft in the input box as it
was; Esc twice within half a second does the same. Neither puts the interrupted prompt back in the input box. A single
Esc only shows what a second would do. Ctrl-U empties the input box's line. At an empty idle prompt Ctrl-C only warns,
and a second quits.

**Octoboard's sign-in.** Copilot's login flow declares a wait of sixteen minutes, a minute more than a device code
lasts, so a code nobody enters ends with Copilot's own "Device code expired before authorization completed" as the
reason.

**Signing in.** With no terminal (stdin `/dev/null`), `copilot login --device-code` prints on stdout, after up to
ten seconds, `To authenticate, visit https://github.com/login/device and enter code XXXX-XXXX` and then `Waiting for
authorization...`, and waits. Once the code is authorized in a browser it prints `Signed in successfully as <login>.`
and exits with status 0 (measured with the user's account, 1 min 49 s after it started). A code nobody enters expires
after 15 minutes: the CLI then prints `Login failed: Error: Device code expired before authorization completed` on
stderr and exits with status 1. A session with no sign-in and no model provider says "Please use /login to sign in to
use Copilot" above its prompt.

- **The token is kept in the system credential store, under the host and the login, not under the Copilot home.** On
  macOS it is a generic-password item of the login keychain whose service is `copilot-cli` and whose account is
  `https://github.com:<login>:github`. The home's `config.json` gets
  only `loggedInUsers` (`[{host, login, kind: "githubDotCom"}]`) and `lastLoggedInUser` (`{host, login}`): the login
  and no email. A home that was never signed in has neither key.
- **A home that was never signed in is signed in all the same when the credential store can be reached.** With the
  user's own `HOME` and `COPILOT_HOME` naming a new, empty directory, Copilot came up signed in (its GitHub MCP server
  connected, a model offered), and wrote no login to that directory's `config.json`. The machine's GitHub CLI was
  signed in as the same login, and Copilot accepts that CLI's tokens: it was the GitHub CLI's token such a home used
  (`auth.getStatus` of the SDK server, under "Plan quota" below, says `gh-cli` there and `user` in a home that names
  the login); either way a home's own record does not decide whether Copilot is signed in there. Homes signed in as one
  login share one token.
- **Asking whether the item exists raises no system prompt** (macOS 26.2, 2026-11-23). `security
  find-generic-password -s copilot-cli -a https://github.com:<login>:github`, which asks for the item's attributes and
  never its secret (no `-w`, no `-g`), answered in 0.1 to 0.3 s with exit status 0 for the login that is signed in and
  44 ("The specified item could not be found in the keychain") for a login that is not, with no authorisation dialog,
  run from a process that is not Copilot. A locked keychain was not measured.
- **The item signs in only a home that names its login.** With the GitHub CLI kept off `PATH` (its token is in the
  keychain too, so an empty `GH_CONFIG_DIR` does not hide it), `auth.getStatus` of the SDK server answered `Not
  authenticated` in a new, empty home, `authType: "user"` in a home whose `config.json` names the signed-in login, and
  `Not authenticated` in a home naming a login that has no item. So a home that names a login is signed in exactly
  when that login's item exists (or the file holds its token in plain text), and a home that names none is signed in
  only through the GitHub CLI or an environment token.
- **Octoboard's sign-in reading follows that item's presence**: a home that names a login reads signed in
  while the credential store holds the item of service `copilot-cli` and account `<host>:<login>:github`, the host as
  `config.json` names it, and signed out while it holds none; a home whose `config.json` holds the login's token
  (`authTokens`) reads signed in without the store being asked. An empty `loggedInUsers` array with no
  `lastLoggedInUser` reads signed out, the state left by `/logout`. A fresh home with neither key gives no local
  reading. The lookup supplies the sign-in reading after an inconclusive quota failure. On Windows the CLI's
  ARM64 1.0.114 binary forms the generic credential target as `<account>.<service>`. Octoboard checks for
  `<host>:<login>:github.copilot-cli` in the daemon's logon session. On Linux it searches by service and username.
  Each native lookup has 5 s to answer and reports presence alone.
- **Linux credential presence** (1.0.114, Debian 13 arm64, GNOME Keyring 48.0, 2026-11-24). The CLI used
  Secret Service with an unlocked disposable collection during an OAuth stand-in login. Its `SearchItems`
  calls matched `service=copilot-cli` and `username=https://github.com:<login>:github`, and also looked up the
  older username without the `:github` suffix. A hosted device flow also completed using this Linux Secret
  Service. An attribute-only lookup found the item for the saved login, and the account's `config.json` held
  no `authTokens`. The method returns separate arrays of unlocked and locked item
  paths, with two empty arrays for absence, and did not prompt during the presence probe. `gdbus call` exits 0
  for both a present item and an absent one. Octoboard reads the structured D-Bus reply and counts
  a locked or unlocked match as present. The Linux Rust reader returned Present for the fixture
  with `username=https://github.com:octoboard-presence:github` in both collection states, Absent for a fresh
  empty collection, and CannotTell for a disconnected bus. Monitoring recorded no prompt or secret read.
- **Linux without a credential store** (1.0.114, Debian 13 arm64, local OAuth stand-in). The actual CLI with
  stdin and stdout on a PTY issued the whole plaintext question in one 73-byte `write`. It needed no `TERM`
  and emitted no terminal query in the capture. A yes exited 0 and stored the disposable credential in
  `config.json`; a no exited 1 and stored none. With stdin from `/dev/null` and stdout redirected, it displayed
  the device instructions, omitted the question, reported that the credential was not saved and exited 1.
  With the Linux daemon, its control socket exposed `keep_token_in_plain_text`, accepted the dialog answers,
  and returned success on yes and `provider.copilot.token_not_kept` on no (222 ms after the answer).
  Chromium on Debian rendered the account login alert dialog against the native daemon and local OAuth server.
  The dialog opened with focus on itself. Its decline button showed the failure alert and saved no credential.
  Its accept button finished the login and wrote the disposable credential.
  Copilot invoked `xdg-open` outside the CLI's foreground process group, as measured with a disposable browser
  helper. The helper received no SIGHUP on CLI exit or PTY closure.
- **Signing out (1.0.114, macOS).** In two scratch Copilot homes naming the same macOS keychain login,
  `/logout` in one wrote `loggedInUsers: []` and removed `lastLoggedInUser` from that home's `config.json`.
  The other home's file kept its login keys. The shared keychain item was absent afterwards, so the saved Copilot
  login was gone in both. A GitHub CLI login or environment token can still authenticate a CLI query where the
  home names no login. The quota probe reads that CLI authentication; the local sign-in reading follows the
  saved Copilot login.
- **`config.json` is read as JSON with comments.** Copilot took a `//` line inside the object, a `//` comment after a
  value, a `/* */` comment, a comma before the closing brace and a byte order mark, each read as signed in and the
  file then rewritten in its own layout; a file that is not JSON even so read as not signed in and was left as it was.
- **Without a credential store Copilot asks, but only on a terminal** (read from `cli-main.js`): the login's result is
  then `needs-plaintext-consent`, and the question is put with `readline` only when both stdin and stdout are a
  terminal. Otherwise no question is asked, the answer counts as no, and the CLI writes `Login succeeded, but the
  token was not saved. Install a system keychain or rerun login and accept plaintext storage.` to stderr and exits
  with status 1. The answer is read as one line, `y` or `yes` in any case meaning yes. So a login that is to put the
  question to the user has to run under a pseudo-terminal, and Octoboard's does: it runs `copilot login
  --device-code` on a terminal of its own, tells the prompt by `Store token in plaintext config file?`, puts the
  question to the user and writes their `y` or `n` back; on a no the login fails with Octoboard's own reason for it
  (`provider.copilot.token_not_kept`). On a terminal the login writes the same two plain lines as on pipes (no
  interface is drawn), and the question's whole prompt is written in one piece by `readline`. After the two lines the
  device-code login calls `presentDeviceCode`, which copies the code to the clipboard and opens the browser, whether
  or not it has a terminal (the call is not behind a terminal check; a failure prints `Failed to copy code or open
  browser: …` to stderr). Not run on a host without a credential store: on macOS the "Keychain Not Found" dialog
  comes first, as below; whether the login queries its terminal, needs `TERM`, or leaves the browser it opened running
  after it exits on a terminal is not measured either.
- **Without a credential store, on a terminal.** With `HOME` naming a directory that has no keychain,
  macOS showed "Keychain Not Found" for the item, and once that was dismissed the CLI asked `System keychain
  unavailable. Store token in plaintext config file? (y/N)`; on `y` the token went into `config.json` as `authTokens`,
  keyed by `<host>:<login>` and `<host>:<login>:github`, beside the same two login keys. With no terminal the
  question is not asked at all (above), yet two logins in such a home did not end with "the token was not saved":
  they sat at "Waiting for authorization..." after their codes were authorized and ended only when the codes
  expired. The likely cause is the "Keychain Not Found" dialog, which macOS puts up for the item and which nothing
  dismisses when there is no terminal session to answer it, so the CLI never gets as far as its result; this cause was
  not measured. Octoboard's declared wait ends such a login at the code's expiry. `/logout` in such a home fails
  ("Failed to remove token: Keychain error").
- **An environment token.** With `GH_TOKEN` set to a token GitHub rejects, a home with no login of its own said
  "Authentication token found but could not be validated. GitHub returned: Bad credentials" and was not signed in,
  where without the variable it was: the variable is used before the credential store. A home that names a login in
  its `config.json` stayed signed in with the same variable set. The same rejected value in `COPILOT_GITHUB_TOKEN`
  left the home with no login signed in (one run). `config.json` is not changed by any of them.

**MCP inventory (1.0.114).** `copilot mcp list --json` reports a map at `mcpServers`, with each entry's
`type`, `enabled`, `source` and optionally `sourcePlugin` and `url`. A signed-in Free home listed the hosted
read-only `github-mcp-server` and the built-in `computer-use`; the offline stand-in listed `computer-use` and a
local marketplace plugin. A configured HTTP server disabled in `settings.json` remained in the native list
with `enabled: false`. The native command lists configuration and supplies no runtime state or OAuth status.
Octoboard runs it in a disposable Copilot home because a listing rewrote a hand-formatted `config.json`. It
uses the account's `mcp-config.json` through `--additional-mcp-config @<file>`, passes the enabled and disabled
server settings, and gives enabled plugin directories to `--plugin-dir`. The reading skips disabled servers.
Remote URLs are reduced to scheme and authority because configurations can carry credentials. The projected
measurement is `apps/daemon/testdata/copilot_mcp_list_1.0.114.json`.

**Default configuration sharing (1.0.114).** A separate Copilot home read linked `settings.json`,
`mcp-config.json` and `instructions/`. `copilot config footer.showQuota on` and `copilot mcp remove` wrote
through their respective links. A Git-installed plugin went under `installed-plugins/_direct/<slug>` and
registered an entry in `config.json` at `installedPlugins[].cache_path`, an absolute path into that home. A
local directory marketplace used the plugin in place with `settings.json` entries `extraKnownMarketplaces` and
`enabledPlugins`. In a scratch run of `copilot plugin marketplace add <directory>`, Copilot wrote the absolute
source path through the link into the default `settings.json`. It used the account's spelling even when the
marketplace itself was reached through a link to the default directory. When the source is private to the account,
that registration breaks if the account directory is deleted. Octoboard shares `settings.json`,
`mcp-config.json`, `instructions/` and `session-state/`. The sharing registry rule repoints
`extraKnownMarketplaces.*.source.path` only when the same directory has an independent spelling,
such as the target of a shared directory link. A private source that cannot be repointed is reported as a
sharing or repair failure. A scratch resume through linked `session-state/` sent the original prompt
and linked instructions to the local model stand-in. The plugin cache and `config.json` (account
sign-in and cache paths) stay private with the session index database.

**Claude configuration compatibility (1.0.114).** In a trusted scratch project the control loaded
`CLAUDE.md`, `.claude/rules/`, `AGENTS.md` and the injected role directory into the model request. It also
listed the project's Claude skill, agent and command in `/env`. In a second run `--no-custom-instructions`
removed all four instruction sources including the injected role while the Claude skill, agent and command
remained. The CLI's flags and configuration reference expose no per-launch switch for the Claude layer.
Octoboard offers no Claude compatibility setting for Copilot.

**Plan quota.** Measured on 1.0.114 on 2026-11-22 with the user's account on the free plan, in scratch Copilot homes
under the user's own `HOME` (so the sign-in was the real one), with no prompt sent and so no model turn.

- **What `/usage` shows on the free plan**: "Local CLI activity · this device" (a 180-day grid of prompts), "Active
  account (credentials & quota) <login> (GitHub)", "Current session · all accounts used" (`Changes`, `AI Credits`), and
  under "Active account/provider usage" one row, `Plan` with a bar, `0% used • resets in 8 days` and `1 / 200 AIC`.
  There was no `Session` and no `Weekly` row: the usage limits `copilot help limits` describes were not shown for this
  account, with 1 of 200 credits used.
- **The source that answers: the CLI's SDK server.** `copilot --headless --no-auto-update --stdio` is what GitHub's
  Copilot SDK starts the CLI with (read from the SDK's `client.ts`; `--headless` and `--stdio` are not in `copilot
  --help`). It speaks JSON-RPC 2.0 with **each message under a `Content-Length: <n>` header and a blank line, both
  ways**; the same requests written one per line got no answer at all. `ping` answers `protocolVersion: 4`,
  `status.get` the version, and `auth.getStatus` `{isAuthenticated, authType, host, statusMessage, login}`.
  - `account.getQuota` (no session) answers `{"quotaSnapshots": {"chat": {…}, "completions": {…},
    "premium_interactions": {…}}}`, each `{isUnlimitedEntitlement, entitlementRequests, usedRequests,
    usageAllowedWithExhaustedQuota, overage, overageAllowedWithExhaustedQuota, remainingPercentage, resetDate, hasQuota,
    tokenBasedBilling}`. On the free plan `chat` was `entitlementRequests: 200`, `usedRequests: 1`,
    `remainingPercentage: 99.6`, `hasQuota: true`, `tokenBasedBilling: true` (the `Plan` row of `/usage`), `completions`
    2000 with 2 used, and `premium_interactions` 0 with `hasQuota: false`. **Its `resetDate` is not the reset**: it was
    the moment of the reading, a different value in every process.
  - `session.create` (`{"sessionId": <a UUID of the client's choosing, or none>, "workingDirectory": …}`) answers the
    session's id, and `session.quota.refresh` (`{"sessionId"}`) then answers `{"snapshots": {"chat": {… the fields above
    without `resetDate`, plus `resetDateEpochMs`}}, "planTier": "free", "isFreeUser": true, "isTbbUser": true,
    "providerQuotas": [{"quotaId": "chat", "unit": "ai_credits", "capacityState": "available", "budgetMetadata":
    {"entitlement": 200, "consumed": 1, "remainingPercentage": 99.6, "resetAtEpochMs": …, "unlimited": false, …},
    "entitledQuantity": 200, "availableQuantity": 199}], "upgradeUrl": …}`. `resetDateEpochMs` was `1796083200000`,
    2026-12-01T00:00:00Z, which is the "resets in 8 days" of `/usage`. `session.quota.get` answers the same shape from
    what the session already holds: before a refresh it had no reset time. `session.delete` (`{"sessionId"}`) answers
    `{"success": true}` and removes the session's directory, leaving a zero-byte
    `session-state/.session-operation-locks/<id>.lock`. `session.usage.getMetrics` is the session's own usage, all
    zeros with no prompt sent. Nothing in either reply is a Session or Weekly limit.
  - **The server answers requests in no fixed order.** With `session.create`, `session.quota.refresh` and
    `session.delete` all written before the first answer, the refresh was refused ("Session not found for sessionId"),
    the delete too, and the create then failed. Each has to be written after the answer before it.
- **Which login a reading is of.** In a home whose `config.json` names no login, `auth.getStatus` answered `authType:
  "gh-cli"`, `statusMessage: "<login> (via gh)"`: the token of the machine's GitHub CLI. In a home whose
  `loggedInUsers` and `lastLoggedInUser` name the login, it answered `authType: "user"`: the keychain item of that
  login. In a home naming a login that has no keychain item it answered `gh-cli` with the GitHub CLI's login, and
  `account.getQuota` answered that account's quota with nothing saying so: the `login` of `auth.getStatus` is the only
  thing that tells whose quota was read.
- **What a reading writes.** With the home's `config.json` as Copilot itself last left it, an `account.getQuota`
  reading added `logs/process-<time>-<pid>.log`, `logs/.copilot-log.lock` and a zero-byte `settings.json.lock` and
  changed no file (`settings.json`, `mcp-config.json` and `config.json` identical byte for byte); with `--log-dir
  <directory>` the log went there and a second reading changed nothing in the home at all. The session route adds
  `installed-plugins/` and `session-state/` with the lock named above. **But a start rewrites `config.json` whenever
  it is not as this version writes it**: an empty home got a `config.json` with `firstLaunchAt`; one holding only
  `trustedFolders` was rewritten with `firstLaunchAt` added (a fixed date, 2026-03-11) and its array reflowed; one
  holding every key but written by hand in another layout (no comment lines, its array on one line) was rewritten in
  Copilot's own. A home that does not exist is created. Outside the home, a reading rewrites `copilot-user-cache.json` and `managed-settings*` under the
  cache directory (`~/Library/Caches/copilot` on macOS), which `COPILOT_CACHE_HOME` moves; a cache directory of its own
  got a 255 MB copy of the CLI package under `pkg/` on first use.
- **A reading that touches none of the user's files.** With `COPILOT_HOME` naming a directory outside the user's
  Copilot home whose `config.json` holds `firstLaunchAt`, `loggedInUsers` and `lastLoggedInUser` for the account's
  login (the login is found by host and login in the credential store, not by home), `COPILOT_CACHE_HOME` a directory
  of its own and `--log-dir` a third, `auth.getStatus` answered `authType: "user"` with that login and
  `account.getQuota` its quota, and neither the user's cache directory nor anything else of the user's changed
  (measured by hashing every file before and after; the user had no `~/.copilot` and none was created).
- **Octoboard's Free quota reading (1.0.114).** It prepares a private Copilot home with the account's login
  metadata, gives the server a working directory and logs there, and keeps the CLI's normal cache. The account's
  configured plaintext token, where present, reaches the server through its environment as it does for the model
  listing. It sends `auth.getStatus`, then `session.create`, then `session.quota.refresh`. If the status is
  signed out or names someone other than the account's configured login, the run stops before creating the session.
  A home without a named login uses the authentication its launch would use. The private home, including the
  temporary session and its lock, is removed after the server exits. The free plan's monthly credit budget and its
  actual reset time come from the `chat` budget in `providerQuotas`. It measured about 4.2 KB in 2.3 seconds with
  a named keychain login, with no model turn. Without the keychain login, the GitHub CLI and environment credentials,
  `auth.getStatus` reported `isAuthenticated: false`. A direct refresh in that run returned JSON-RPC error
  `-32603`; Octoboard stops at the status. The quota reply capture is
  `apps/daemon/testdata/copilot_quota_free_1.0.114.json`. Copilot calls this window **Plan**. Octoboard
  uses that name for the `credit_period` according to Copilot's descriptor.
- **Sources that gave nothing**: the ACP mode (`copilot --acp`): `initialize` and `session/new` carry no quota, the
  methods `account/getQuota`, `account.getQuota`, `github.com/copilot/usage` and `_github.com/copilot/usage` are refused
  with -32601, and `usage` is only one of the `availableCommands`, which a client can send no other way than as a
  `session/prompt` (not sent: nothing says a prompt of that text stays away from the model); the home's files after
  these runs (`config.json`, the log, `session-state/`), none of which holds a quota; `--usage-output-file`, which by
  `copilot help billing` writes one session's own usage after a prompt, not the plan's (not run: it needs a prompt);
  and the help topics `billing` and `limits`, which name no command but `/usage`.

**Against the hosted model** (one session of each kind, the user's plan): the hook payloads were the stand-in's, field
for field, with the model's own `toolCallId`s; the allow rule let `octoboard-report` run with no prompt; `sleep 6` ran
with no prompt. The model renames the session within seconds of the first prompt, so the title goes from the prompt's
text to a summary ("Respond with OK - GitHub Copilot") and then stays. A shell command started detached (`mode:
"async"`, `detach: true`) ends its call at once; when it finishes, the session gets a turn of its own whose
`userPromptSubmitted` prompt is `<system_notification>`, `Detached shell "<description>" (shellId: N) has
completed.`, `</system_notification>`, with a `notification` of type `shell_detached_completed` beside it.

**Background work** (2026-11-23, against the stand-in; the captures are `apps/daemon/testdata/copilot-captures.md`,
"Background work").

- A shell command started with `mode: "async"` ends its call at once with `<command started in background with
  shellId: N>`, or `<command started in detached background with shellId: N>` with `detach: true`; a running command
  the user's message moves to the background ends with `<command with shellId: N moved to background by the user. …>`.
  The record's `tool.execution_complete` carries the id as `result.structuredContent.shellId`; a command that ran to
  its end carries one too, with `<shellId: N completed with exit code …>`. The ids count from 0 in every session.
  The tag closes `result.content`, after the command's own output (`\n<shellId: 0 completed with exit code 0>` for a
  command that printed nothing), so only the closing tag says where a command went: its output can hold the same
  words.
- A `read_bash` call for a command that has ended answers `<shellId: N completed with exit code …>`, or `<detached
  command with shellId: N completed with exit code …>` for a detached one, with the same `structuredContent.shellId`.
  **When the model reads the end first, no notification follows**: a command started in the background, and one
  started detached, each read to its end by a `read_bash` call of the turn that started it (`delay: 7` over a `sleep
  3`), got no `system.notification`, no notice prompt and no `notification` hook in the 27 s the session stayed open.
  That result is then the only thing that says the command ended, and Octoboard takes it off the list there.
- The turn ends with the command running. Its completion is a `system.notification` line in the record (`kind`:
  `{type, shellId, description}`, `exitCode` too for one not detached) and a turn of the session's own: a
  `userPromptSubmitted` whose prompt is `<system_notification>`, `Shell command "<description>" (shellId: N) has
  completed successfully.` or `Detached shell "<description>" (shellId: N) has completed.`, `</system_notification>`,
  a `notification` of type `shell_completed` or `shell_detached_completed`, and an `agentStop`. Two commands gave two
  such turns, each when its own command ended.
- A detached command the model stops with `stop_bash` (`<command with id: N stopped>`) gets no notification.
- A background agent (`task` with `mode: "background"`) ends its call at once ("Agent started in background with
  agent_id: …"), with `subagentStart` run before the session's `agentStop` and `subagent.started` (`executionMode:
  "background"`, `agentDescription`) in the record. When it is done its `subagentStop` runs, the record gets
  `subagent.completed`, and the session gets a turn whose prompt is `Agent "<name>" (<type>) has finished processing
  and is now idle.` with a `notification` of type `agent_idle`: the notice names the agent's display name and no id.
  A notification of type `agent_completed` was not seen.
- **An idle background agent written to again works with no start** (`write_agent` with its `agent_id`): no
  `subagentStart` and no `subagent.started`, and when it is done no `subagent.completed`. What does run and is
  written: the agent's own `userPromptSubmitted` (carrying its `agentId`) and a `user.message` of its own in the
  record, then its `agentStop`, a `subagentStop`, and the same `agent_idle` notification and notice as the first time,
  whose record line names its `agentId`. Octoboard lists it again from that prompt until that stop.
- **A session resumed after its process was killed** (`kill -9` of the CLI's own process, then `--session-id` with the
  same id): the record has no `session.shutdown` for the killed process and gets a `session.resume` line (`resumeTime`,
  `eventCount`, `sessionWasActive: false`) when the session is resumed, with `sessionStart` run with `source:
  "resume"`. Both of the killed process's commands, the detached one and the one only in the background, were still
  running, orphaned, and the resumed process knows neither (shell ids start again at 0). Octoboard counts nothing from
  before a `session.resume`, as before a `session.shutdown`. Killing only the `copilot` launcher script let the CLI
  shut down in order (`sessionEnd` with `user_exit`, `session.shutdown`), which ended the background command and left
  the detached one running.
- No hook says what is running. Octoboard lists it from the record at each `agentStop` of the session's (commands
  whose call's closing tag said they went to the background, background agents started and not completed, each agent
  under Copilot's name for its kind, `agentName`), takes a command off at its completion notice, at a `read_bash` or
  `write_bash` result that says it completed or at a `stop_bash`, and an agent off at its `subagentStop`, and counts
  nothing from before a `session.shutdown`. It is listed from the end of the turn that started it, as every agent's
  background work is, not from the call.
- **The record is read on from where the last `agentStop` got to,** an offset kept with the session's process: each
  read gives the work the new lines started and the work they ended, so a detached server started early in a session
  stays listed however much the record has grown since. Only the first read of a process (a launch, a resume, a
  daemon that adopted the session after a restart) reaches back a fixed 4 MiB from the record's end: work started
  before that and not listed already is missed by it. A turn that both starts and ends work is two changes of one
  event; where the record was read from its first byte the list it leaves replaces what is listed, and where the first
  read started inside a longer record the start is listed at the turn's end and the end is taken off with the
  session's next event. The record read is the one under the account's directory the launch runs on, placed by the
  session id, never the `transcriptPath` a payload names.
- **A `permission_prompt` notification that lands after its turn's end raises no hand.** Copilot does not wait for
  the notification hook; when the record shows every prompt of the turn answered and the session's `agentStop` run,
  and still shows it half a second later (a prompt put up since, a background agent's say, has its request in the
  file by then), the notification is not applied at all. Where the record is behind, the hand goes up and is lowered
  half a second later as before, beneath a session request's hand where one is held.

**Model and reasoning effort** (2026-11-23, 1.0.114, the latest on npm that day, the user's free-plan account through
the GitHub CLI's token, a scratch `COPILOT_HOME` and `COPILOT_CACHE_HOME`, no prompt sent).

- The ACP mode still lists no models on 1.0.114 ("ACP mode" below); no later version was published to measure.
- **The SDK server lists them** (`copilot --headless --no-auto-update --stdio`, the framing under "Plan quota"):
  `models.list` (no session; `selectionId`, `gitHubToken` and `streamerMode` optional) answers `{models, auto,
  imageModels}`, each model `{id, name, capabilities, vendor, policy, billing, supportedReasoningEfforts,
  defaultReasoningEffort}` by the SDK's own types (`copilot-sdk/generated/rpc.d.ts` in the unpacked package), the
  efforts present only for a model that takes them. `session.model.list` (`{sessionId}`, after `session.create`)
  answers `{list, providers, modelPriceCategories, quotaSnapshots, auto, resolvedAuthLogin}`, `list` being the models
  the picker offers. **On the free plan both lists were empty**: `models: []` and `list: []`, with `auto` holding the
  Auto tiers (`balance` "Standard" enabled and the default; `hydrafusion` "High" and `hydrafusion-max` "Max"
  disabled, `user_not_eligible`; `fast` enabled but not shown in the picker) and `modelPriceCategories` naming sixteen
  models Auto may choose, none of them selectable. The footer says `Copilot Fusion`, the name Auto is shown under. So
  this account's model list is Auto alone, which takes no reasoning effort; `--model auto` (or `fusion`) is what
  `--help` names for it, and `--auto-tier <id>` picks a tier. Answered in 2 to 3.5 s from the start; the session
  route leaves `session-state/` in the home, as under "Plan quota". A paid plan's list, with its models' efforts, is
  not measured.
- **Octoboard's catalogue** (2026-11-23) asks `models.list` of the SDK server, run with `COPILOT_HOME` naming a
  private directory of Octoboard's own under its run directory whose `config.json` holds `firstLaunchAt` and the
  account's sign-in keys (`loggedInUsers`, `lastLoggedInUser`) and nothing else, `--log-dir` inside it, and the
  directory removed after the run; the account's own directory is never written, and no credential is written
  anywhere. Where the account keeps its token in its own `config.json` (`authTokens`, the user's plain-text choice),
  the token is handed to the run through `COPILOT_GITHUB_TOKEN`, only where the environment gives none of
  `COPILOT_GITHUB_TOKEN`, `GH_TOKEN` and `GITHUB_TOKEN` (2026-11-24, 1.0.114, a scratch `COPILOT_HOME` naming no
  login under the real `HOME`: with the GitHub CLI's token in `COPILOT_GITHUB_TOKEN`, `auth.getStatus` answered
  `authType: "env"`, `statusMessage` `<login> (via COPILOT_GITHUB_TOKEN)`, and `models.list` the same list as the
  `gh-cli` fallback without it, Auto alone on the free plan; a malformed `GH_TOKEN` gave `Not authenticated` and
  `models.list` refused, so a token variable is taken before any stored login and the fallback; a value in no token's
  shape in `COPILOT_GITHUB_TOKEN` was passed over for the fallback). The list is asked for only once
  `auth.getStatus` names the login the account's `config.json` names (compared without case); otherwise the listing
  is unavailable, since a home naming a login whose credential is gone signs in as the GitHub CLI's login without
  saying so (under "Plan quota"). A home that names no login lists whatever the run signs in as, as a session there
  would. Measured that way with the
  user's account: answered in 3.0 s, exit 1.4 s after the input ended, and the run left `config.json`, a zero-byte
  `settings.json.lock` and the log in that directory. None of them holds the token handed through
  `COPILOT_GITHUB_TOKEN` (2026-11-24, 1.0.114, the GitHub CLI's token handed that way to a private directory shaped as
  the listing's: `auth.getStatus` answered `authType: "env"`, `models.list` answered, and the directory's four files,
  `config.json`, `settings.json.lock`, `logs/.copilot-log.lock` and the process log, searched before removal, held
  neither the token nor its first 20 characters). The cache directory is the CLI's own and not moved (a cache of
  its own would copy the 255 MB package on every listing): the run rewrote `copilot-user-cache.json` and
  `managed-settings/` under `~/Library/Caches/copilot`, as every start does. Auto is offered first as `auto`
  (`--model auto`, shown with its default tier's name, `Auto (Standard)`) while that tier is enabled, then every listed
  model with its `supportedReasoningEfforts` (none where it lists none) and its `policy.state` as whether the account
  may use it. The free-plan answer is `apps/daemon/testdata/copilot_models_list_free_1.0.114.json`.

**The draft stash** (2026-11-23, at an idle composer, no sign-in needed; captures in
`apps/daemon/testdata/copilot-captures.md`, "The draft stash"). Ctrl-S with a draft in the composer empties it and
adds `stashed ·` to the footer, right before the model's name (`… tab next tab   stashed · Copilot Fusion`), kept at
80 columns where the footer's other items are cut short; Ctrl-S at the empty composer puts the draft back and the item
goes. `copilot help config` names no setting that rebinds the key; the composer's `editorMode` `vim` was not measured.
Octoboard uses the stash from 1.0.114 on, reading the footer's `stashed` item under the composer.

**Image messages** (2026-11-23, at an idle composer; against the stand-in). A bracketed paste of an image file's path
alone is attached: the composer shows `[image: <file name>]` in its place, a second image's placeholder right after
the first with nothing between (`[image: red-one.png][image: blue-two.png]`), and text pasted after them follows on the
same row; Ctrl-U empties all of it. Enter after the text sent the message, the Enter not lost: the model's request
carried the prompt with the placeholders as text, then for each image a text part `Image file at path <path>` and an
`image_url` part holding the file as a `data:image/png;base64,…` URL. Octoboard writes an image message so (each path
pasted alone, then the text) and counts the placeholders. Captures: `copilot_images_120_*`.

**Slash commands** (2026-11-23, against the stand-in). `/` opens a completion list right above the composer's top
rule, each row `/<name>` with its description after a column of spaces, behind a margin of `┃` or of two spaces, the
highlighted one led by `❯`; the composer itself shows the highlighted entry's name in place of what was typed (`/`
shows `/add-dir`, `/he` typed or pasted shows `/help`), and Enter runs that entry (`/he` and Enter opened `/help`'s
view). With an argument being written (`/model `) the list shows the command's forms, `❯ /model` highlighted, and the
composer an argument hint. A prompt already sent and the working directory's row above the rule are indented one
column. Octoboard reads the highlighted entry's name as the command an Enter would run. **Ctrl-U does not clear a
draft of several lines**: on `first` / `second` it emptied the second line, and further presses left `first` and the
line break as they were, so Copilot's reader does not declare it the box's clearing key, and a lone command (a
`/name` with no argument, which a completion may change) is refused by the generic layer; a command with an argument
is sent. Ctrl-C at the idle prompt emptied the same two-line draft at once. The ACP mode lists 34 commands with
descriptions ("ACP mode"). Captures: `copilot_commands_120_*`.

**Interactions** (2026-11-23, against the stand-in, at 120 columns; captures `copilot_permission_120_*` and
`copilot_question_120_*`). The permission prompt's box holds the call's description, the command in a box of its own,
`Do you want to run this command?`, `1. Yes`, `2. Yes, and don't ask again for` the command `in this directory (<path>)`
and `3. No, and tell Copilot what to do differently (Esc to stop)`, the cursor `❯` on the first, and the hint `↑/↓ to
navigate · enter to select · esc to cancel`. Down moves the cursor and stops at the last row (no wrap). On the third
row, text typed or pasted takes the label's place (`3. use echo (Esc to stop)`) and Enter refuses the call with it
as the user's feedback ("The user rejected this tool call. User feedback: use echo"), the turn going on; Enter there
with no text does nothing; Esc refuses the call and stops the turn ("Operation cancelled by user"). The `ask_user`
form (`message` and a `requestedSchema` of fields) shows `Copilot needs information.`, the message, a row of the field
names with the current one in brackets when there are several, and the current field: a string with `enum` is
`<name>:` and its values, ` ❯ ` on one, and `Other (type your answer)`, on which a text field `❯ Type your answer...`
opens below the form and takes what is typed; a string without `enum` is the name, its description and a text field
` ❯ ` between rules. Down stops at the last value; Enter accepts the field and moves to the next, the last sending the
form (`choice: zeta`, `note: hello`); Ctrl-D declines; Esc cancels and ends the turn. Octoboard publishes the
permission prompt for a shell command whose text the inner box shows (allow, allow and remember, refuse with optional
feedback, refusing with none being Esc), and the form when every field is a string, answering by Down, typing and
Enter; a cursor above which the answer lies is stuck, since no key that moves up is among the inputs a press writes.
A form's footer ends `esc cancel`, or `esc clear` once a text field holds text; while it does, Ctrl-D does nothing and
Esc empties the field, after which Ctrl-D declines (2026-11-24), so Octoboard declines with Esc first there. Its
message and a text field's text wrap onto further rows, the fields' row coming after the message and a blank row.

The other prompts (2026-11-24, the same way; captures `copilot_permission_{create,edit,view,mcp,long,huge}_120_*`,
`copilot_feedback_wrap_120_*`, `copilot_question_wrap_120_*`, listed in `apps/daemon/testdata/copilot-captures.md`):

- **A file created or edited** (`create`, `edit`): the box's title `Create file` (or the edit's), the file's name and
  directory, the diff, `Do you want to create <path>?` (`edit` for an edit), and the same three rows as a command's,
  the second reading `Yes, and don't ask again for file operations when in this directory (<dir>)`. Esc cancels the
  call and the turn.
- **A read outside the allowed paths** (`view` of `/etc/hosts`): `Allow path access`, the path in a box of its own,
  `Read access covers this directory and its descendants:`, the directory in a second box, `Do you want to allow
  this?`, and four rows: `1. Yes`, `2. Yes, and remember this path for this session`, `3. Yes, allow Copilot to read
  files in the listed directory for this session` and `4. No (Esc)`. Enter on the fourth, like Esc, refuses the call
  and ends the turn ("Operation cancelled by user"); there is no feedback row.
- **An MCP tool** (`<server>-<tool>`): the tool's name as the box's title, the arguments as indented JSON in a box of
  their own (in the call's key order), `Do you want to use this tool?`, and three rows, the second reading `Yes, and
  don't ask again for tool "<tool>" from "<server>" in this directory (<dir>)`.
- **A long command** wraps in its box at spaces; one too tall for the screen (4,200 characters) is drawn with only the
  start of its box, no bottom side, and the hint becomes `↑/↓ to navigate · pgup/pgdn to scroll · enter to select ·
  esc to cancel`.
- **Feedback that wraps**: the third row reads `3. <text>`, the text wrapping onto rows indented under it, and
  `(Esc to stop)` on a row of its own; Enter sends the whole text.

Octoboard publishes each of these, matched to its call by what it shows (a command or a read's path in its box,
whitespace left out and a text over 300 characters by its start; a created or edited file by the question's path; an
MCP tool by its name and its arguments' box), the rows that keep a rule (`don't ask again`, `remember this path`,
`allow Copilot to read files in the listed directory`) as allow and remember, and `No (Esc)` as refuse with no text.
Each key of an answer is written only while the prompt on the screen is still the answered call's, so a prompt queued
behind it is never answered with its keys; feedback is pasted only onto the bare label, and sent once the row shows
the answer's text. Not measured: `apply_patch`'s prompt and a URL's (not published); fields of other types; a form
whose `Other` takes a paste.

**Command groups at exit** (2026-11-23, against the stand-in, `--allow-all`, started through the npm loader as
Octoboard starts it). A shell call ran as `/bin/bash --norc --noprofile -c <command>` leading a process group of its
own, a child of the native program. `SIGTERM` to the loader during `(sleep 303 &) ; sleep 304 & sleep 301` ended
Copilot within a second, and with it every process of the group, the orphan `sleep 303` (parent 1, same group)
included. So Copilot ends the process groups of the commands it still runs as it exits, as the released agents do.

**Memory** (2026-11-23). Copilot keeps a memory of its own, Copilot Memory: `/memory` (or `/memory show`) at a new
session of the free-plan account said `Status: enabled`, "Copilot stores and recalls facts across sessions", and that
repository memories, kept on GitHub for everyone using Copilot Memory in the repository, are available only for
GitHub-hosted repositories; a user's own are managed at `github.com/settings/copilot/memory`. It is on by default and
turned off by the `memory` setting (`copilot help config`), which `/memory off` writes into the account's own
settings. **No launch can turn it off for one session**: `--enable-memory` only turns it on in prompt mode (`-p`, where
it starts off), `--help` and `copilot help environment` name no flag or variable that turns it off for an interactive
session, and the interactive session applies the setting as read from the account's settings, defaulting to on (read
from `cli-main.js`). Writing the setting is changing the user's own configuration, so Octoboard declares the memory
one that cannot be switched off. Whether `--excluded-tools` of the memory tool stops it from storing was not measured.

**Installed version** (2026-11-23). The npm install is `lib/node_modules/@github/copilot/` (`package.json` with
`name` `@github/copilot`, its `npm-loader.js` the `copilot` the `bin` directory links to) and the platform package
under its own `node_modules/@github/copilot-<platform>-<arch>/` (`package.json` and the native `copilot`), both at
1.0.114; `optionalDependencies` pin every platform package to the package's own version. The loader starts the
platform package's program. A copy of the unpacked package under a higher version in the cache directory
(`pkg/darwin-arm64/1.0.200/`) did not change what ran (`status.get` answered 1.0.114). Octoboard reads the version
from the platform package beside the launcher, else from `@github/copilot`, never by running the CLI.

**Not measured**: which login a home signed in to two GitHub accounts uses, whether configuration directories
with different logins stay isolated, and carrying a conversation between those logins. These need a second GitHub
account with Copilot. Also unmeasured: what
`/logout` leaves in `config.json` and whether it signs out the other homes of the same login (it removes the token
from the credential store, so it needs the user's consent);
whether asking for the token item prompts while the macOS keychain is locked; a subagent's own background commands;
how late the record is written while a hosted model streams its answer (the write-out times above are the
stand-in's, whose answers come at once); the
quota of a paid plan and when the Session and Weekly usage limits apply. On 1.0.114 with 1 of 200 free-plan
credits used, `/usage`, `account.getQuota` and `session.quota.refresh` showed the monthly Plan budget alone.
The quota requests for an account with no Copilot plan also remain unmeasured.

### ACP mode

Measured on 1.0.114 on 2026-11-22 with `copilot --acp`, the user's own `HOME` (the token is in the keychain) and
`COPILOT_HOME` naming a scratch directory, signed in on the free plan. The probe wrote `initialize` (protocol version 1, no client capability) and then `session/new` (a
scratch working directory, no MCP server) as JSON-RPC lines on stdin, read the answers, sent no prompt, and ended the
input.

- **It does not list the models.** `session/new` was answered with `sessionId`, `modes` and `configOptions`, and no
  `models`; the options are `mode` (agent, plan, autopilot) and `allow_all` (`on`, `off`), with no option of the
  `model` category. No `config_option_update` followed in the two seconds after. `models/list` is "Method not found".
  `session/set_model` and `session/set_config_option` with `configId: "model"` both answered success for a model id
  that does not exist, so neither tells which ids are valid. `copilot help acp` names no way either. So ACP is not a
  means to Copilot's model list on this version.
- `initialize` was answered after 5 to 14 s: `agentInfo` (`Copilot`, the CLI's version), one `authMethods` entry
  (`copilot-login`, whose `_meta` carries the path of the native executable and `["login"]`) and capabilities for
  loading, closing and listing sessions and for logging out. `session/new` took 1 to 4 s more. Both requests written
  at once, before any answer, were answered in order.
- `available_commands_update` came with the session, up to three times: 34 slash commands with descriptions and
  input hints.
- **Signed out** (`GH_TOKEN` set to a token GitHub rejects, a new `COPILOT_HOME`), `initialize` answered the same
  and `session/new` was refused: `{"code": -32000, "message": "Authentication required"}`.
- **Every `session/new` leaves a directory** `$COPILOT_HOME/session-state/<session id>/` (`workspace.yaml` with the
  id, the working directory and the client's name, `checkpoints/index.md`, a lock file) and a lock under
  `session-state/.session-operation-locks/`. They stay after the process ends. `session/list` in a later run did not
  list them, and they hold no `events.jsonl`. The first run in a home also writes `config.json` (`firstLaunchAt`).
- The process exited by itself about 2 s after its input ended, having closed the session it opened.
- Nothing was spent: no prompt was sent and the session's directory holds no model request.

No provider code uses it.

## OpenCode

The baseline below was measured on 1.19.6 on macOS,
in a scratch home and working directory, with a local stand-in answering the model's requests, and then with the
default account signed in (an Anthropic API key) through a development daemon with a data directory of its own,
`anthropic/claude-haiku-4-5` answering: a console session and a project session it opened, a permission prompt, a
question, an Octoboard tool call, a report to the parent, an instruction, an interrupt, a stop of the daemon and a
resume, and a resume after an archive. Octoboard launches any version (measured on 1.19.6, 2.3.2, 2.4.0 and 2.4.1; none is refused for being unmeasured). The differences measured on 2.x
are in "OpenCode 2" below.

**Windows 11 ARM64 (23H2, build 22631.6199).** npm 11.20.0 installed `opencode-ai` **1.19.6**
into a scratch prefix. Its `opencode.cmd` uses `%~dp0` and runs
`node_modules\opencode-ai\bin\opencode.exe %*`. The launcher reported 1.19.6 on that host.
Octoboard resolves the shim to this executable when the package is beside it, which gives
version and install detection the same executable as the terminal launch. The first desktop
bootstrap timed out while OpenCode was downloading ripgrep. The provider keeps the same
workspace request open for up to four minutes on Windows, with the ordinary request timeout
unchanged. A run with a fresh cache delivered the first console and project messages and the
first after `/new`. A rendered terminal now recognises the composer even for providers that
have no folder confirmation. The resumed Windows project delivered its prompt to the stand-in.

**Linux daemon** (1.19.6, Debian 13 arm64, glibc 2.41). A fresh native private server advertised its
listen port before initializing the workspace for `POST /session`. With the launch's injected configuration
and fresh XDG directories, that first request returned HTTP 200 after 34.06 seconds. With fresh
directories and no injected configuration it returned in 1.60 seconds, and with the injected configuration
and a warm cache it returned in 0.32 seconds. This exceeded the daemon's 10-second steady-state request
budget and caused a fallback to a standalone terminal. The first session creation or lookup has a separate
45-second budget. Warm starts reached the Octoboard MCP server and the pinned Anthropic model completed
turns in a console and project session. The pair archived and relaunched. In a follow-up with fresh runtime
state and staged npm dependencies, a loopback forwarder delayed each native server's first session creation by
13 seconds. The patched daemon attached in 16.8 seconds for the console and 14.5 seconds for the project.
On the attached pair, the local model received the project's report in the console and the console's
instruction in the project. The interrupt command cancelled the active project model stream and returned
the session to idle. A process trace showed the OpenCode plugin directly executing the launch's hook
script with a `/bin/sh` shebang. With an unstaged npm dependency directory the follow-up crossed the 45-second deadline
during workspace initialization and fell back to a standalone terminal.

### Launch and resume

- `opencode` starts the TUI; `--prompt <text>` gives the first prompt, `--model <provider/model>` the model,
  `--auto` approves every permission its rules do not deny, for that process. There is no reasoning-effort flag.
- The first prompt has to be in the flag's own argument, `--prompt=<text>`: given as the next argument, a prompt that
  starts with a dash (`- fix the build`, a markdown list) is taken for an option, and the TUI starts with no prompt
  and no session. Octoboard keeps the first task in its outbox until the TUI is ready.
- Under `--auto` a rule that says `ask` still fires `permission.asked`, answered at once by OpenCode itself
  (`permission.replied` 6 to 24 ms later, measured again for `bash`, `external_directory` and two calls at once; a
  rule that says `deny` fires neither). The flag is not kept by the session: resumed without it, the same
  call opens a prompt that waits. Octoboard's plugin therefore tells of a prompt only once it has stayed open 300 ms,
  and forwards its answer whether or not it told of the prompt.
- No flag takes an id for a new session. A session is created only when its first prompt is submitted, not when
  the TUI starts, and its id (`ses_…`) is first seen in the `session.created` event.
- `--session <id>` continues a session. An id that has gone prints `Error: Session not found: <id>` and exits with
  status 1 at once.
- It sets `AGENT=1`, `OPENCODE=1` and `OPENCODE_PID=<its pid>` on its own process as it starts, so every program it
  runs inherits them.

### Configuration, and how Octoboard injects

Configurations are merged in this order, a later one over an earlier: the global file (`~/.config/opencode`), the
file `OPENCODE_CONFIG` names, the project's `opencode.json`, the configuration directories (the project's
`.opencode`, `~/.opencode`, then the one `OPENCODE_CONFIG_DIR` names), the inline JSON of `OPENCODE_CONFIG_CONTENT`,
an organization's and a managed configuration, and last `OPENCODE_PERMISSION`. Objects merge key by key; the
`instructions` and `plugin` lists are added to; any other list is replaced.

- **`OPENCODE_CONFIG_CONTENT` is what Octoboard sets.** It wins over the user's global and project configuration and
  leaves the rest of theirs in force. Where the user's shell already sets it, Octoboard merges its own over theirs.
  OpenCode reads the variable with the reader of its configuration files: comments and trailing commas are taken
  (a value with a line comment, a block comment and a comma before the closing brace loaded), and Octoboard reads the
  user's value the same way, with the shared reader of a user's JSON. Measured with `opencode debug config`: a comma
  that follows no value is refused (`[,]`, `{,}`, `["a",,]`: `ValueExpected`), a byte order mark is refused
  (`InvalidSymbol`), and a line comment ends at a carriage return as at a line feed. A value OpenCode refuses makes it
  exit with status 1 and `Config file at OPENCODE_CONFIG_CONTENT is not valid JSON(C)`, whatever the command.
- **`{env:NAME}` and `{file:path}` are put in place in the text before it is parsed**, in the inline value as in a
  file: inside a string or outside one (`{"username":{env:T}}` with `T` holding a quoted string loaded, and a whole
  value of `{env:WHOLE}`), once (a variable whose value is `{env:B}` stays that text), nothing for a variable that is
  not set, the name taken as written (`{env: T }` is not `T`), a token with no name (`{env:}`, `{file:}`) left as
  text, and inside a block comment too. A file's content is
  trimmed and escaped as a string's (so `{file:…}` outside a string is JSON only for a file holding a number or
  `true`/`false`), a relative path is taken from the working directory, a missing file is refused, and a `{file:…}` on
  a `//` line is skipped. The variables go in first and the files after, in the text with the variables in place:
  `{file:{env:P}}` reads the file `P` names, a variable whose value is `{file:…}` has the file put in, and a file's
  content is not read for tokens again (a file holding `{env:X}` gave that text). The path is taken as written up to
  the closing brace (`{file: x}` names a file whose name starts with a space), `~/` at its start stands for the home
  directory (`{file:~/vals/flag.txt}` loaded from a scratch `HOME`) and no other use of `~` does
  (`{file:~name/…}` was looked for below the working directory), and the trim takes a byte order mark. A directory
  is refused as a missing file is. Octoboard keeps a user's value that reads as written as it is, its tokens left to
  OpenCode; one that is JSON only with them in place is read with the variables of the launch's environment in
  place, and with the files named outside a string in place, read as OpenCode reads them, and merged. A
  `{file:…}` inside a string is left for OpenCode in that case too, so the content of a file named there stays out
  of the launch's environment. OpenCode substitutes again in what it is handed, so in that second case a variable
  whose value itself holds `{env:…}` has it put in, which OpenCode alone leaves as text. A file named outside a
  string is read within 64 KiB, as a regular file; where it cannot be read the user's value is not merged and
  Octoboard's own configuration goes alone.
- **`OPENCODE_CONFIG_DIR` is not used.** A plugin in its `plugin` directory does load, but OpenCode installs
  `@opencode-ai/plugin` (62 MB of `node_modules`, with a `package.json`, a lock file and a `.gitignore`) into every
  configuration directory on every launch, and the variable holds one directory, so a user's own would be lost.
- With any of the three variables set, OpenCode does not write its `$schema` stub into a missing global config file.
- The `plugin` list takes an absolute path of a JavaScript file as well as a `file://` URL, and nothing is installed
  for it. The plugin is loaded once, into the TUI's own process, before the input box is ready, and initialised once
  more as the process shuts down.
- `instructions` is a list of files; each reaches the model once per request, in the system prompt, on a new session
  and on a resume alike. A working directory whose `AGENTS.md` and `CLAUDE.md` are links to one file gives the model
  that text once.
- An MCP server is `mcp.<name>` with `type: "local"`, `command` (a list) and `timeout`, in milliseconds, per server,
  five seconds by default. A call that outlasts it ends as the tool error `MCP error -32001: Request timed out`, and
  the turn goes on.
- A server's tool reaches the model as `<server>_<tool>` (`octoboard_get_session`), and that name is the permission
  OpenCode asks for. Tools of an MCP server are not asked about by default; under a user configuration of
  `permission: {"*": "ask"}` they are, unless the inline configuration holds `permission: {"octoboard_*": "allow"}`,
  which wins.
- Of the rules that match a tool, the one written last in `permission` holds: `{"octoboard_*": "allow", "*": "ask"}`
  asks for an Octoboard tool, `{"*": "ask", "octoboard_*": "allow"}` does not. `permission` may also be one action
  for every tool (`"ask"`), which merges as the rule for `*`. A key keeps the place it first had when one
  configuration is merged over another (`opencode debug config` prints the merged order): a global file of
  `{"octoboard_*": "deny", "*": "ask"}` under an inline `{"octoboard_*": "allow"}` merged to
  `{"octoboard_*": "allow", "*": "ask"}`, and the Octoboard tool was asked for, the user's `*` holding. A pattern
  matches by wildcard, so `octoboard_**` matches what `octoboard_*` does under a key of its own: the same file under
  an inline `{"octoboard_**": "allow"}` merged to `{"octoboard_*": "deny", "*": "ask", "octoboard_**": "allow"}`,
  and the tool ran unasked (the call reached a stand-in MCP server). A rule written inline is still the user's to
  overrule: a file of theirs that writes that very key ahead of a `*` keeps it there.
- **A plugin's `config` hook is handed the merged configuration, and what it changes there holds** (1.19.6). It is
  called once the files, the inline value and `OPENCODE_PERMISSION` are merged (a hook saw the rules of a global
  file, of the inline value and of `OPENCODE_PERMISSION` in one `permission`, nested rules as written), and before
  the agents' rules are built: a hook that replaced `config.permission` with the same rules under a `*` written
  first gave, over a global file of `{"bash": "allow", "edit": "deny"}`, the rules `* ask`, `bash allow`,
  `edit deny` in that order, in `opencode debug agent build` and in the running TUI alike (its server's
  `app.agents()`, asked from a plugin). The hooks run in the order of the merged `plugin` list, after those of the
  plugin directories: a plugin of a configuration file's list and one in the project's `.opencode/plugin` both ran
  ahead of the inline value's and were handed the rules as merged, and a plugin listed after Octoboard's in the
  inline value ran after it and was handed the rules Octoboard had written. Octoboard's plugin is the last entry of
  the list the launch hands over, so its hook runs after every plugin of the user's; a rule one of theirs adds in
  its own hook is there for it to order.
- **Octoboard writes the launch's permission rules there, not inline**: its plugin's `config` hook rewrites
  `config.permission` in memory, and nothing is written to a file. A rule written inline cannot be placed: a key
  keeps its first place in the merge and a new one goes last, so an inline `"*": "ask"` lands after the rules of a
  file that writes no `*` ahead of them (`{"bash": "allow", "edit": "deny"}` merged to
  `{"bash": "allow", "edit": "deny", "*": "ask"}`, and the deny became a prompt), and a file's own `octoboard_**`
  ahead of a `*` stays ahead of it. Two rules are written, measured with `opencode debug agent build` and the
  plugin as a launch writes it:
  - **Under the `ask` approval mode, `"*": "ask"` as a default beneath the user's own rules.** Where the merged
    rules write no `*` it goes first, so what the user allowed is not asked for and what they denied stays denied:
    a global `{"bash": "allow", "edit": "deny"}` gave `* ask`, `octoboard_** allow`, `bash allow`, `edit deny`.
    Where they write a `*` of their own, `ask` takes its place where it stands, so the rules written after it hold
    and those before it stay overruled, as the user's own default overruled them
    (`{"edit": "deny", "octoboard_report": "ask", "*": "allow", "bash": "deny"}` gave `edit deny`,
    `octoboard_report ask`, `* ask`, `octoboard_** allow`, `bash deny`). A `permission` that is one action
    (`"allow"`) is that `*`. OpenCode takes a `*` that holds rules by pattern (`"*": {"rm *": "deny"}`, followed as
    `* rm * deny`); the default goes among those the same way (`* * ask`, `* rm * deny`). `OPENCODE_PERMISSION` is
    merged in before the hook sees the rules, a `*` of its own included. **An agent's own `permission` is left as
    the user wrote it, its `*` included**: it is followed after all of these, so a rule the user gave one agent
    holds over the default, an allow or a deny alike, which is the user's rule staying in force. With the default
    an edit inside the working directory is asked for (permission `edit`), which is not by default. Under `--auto`
    such a prompt is answered by OpenCode itself. With no approval mode chosen, and under `auto`, the hook writes
    no default: OpenCode's own configuration decides.
  - **The rule that allows Octoboard's tools, `octoboard_**`, right after the rules' `*`, or first where they write
    none, under every approval mode and with none.** There the default does not ask for Octoboard's tools, and a
    rule of the user's that names one of them and is written after their default still wins, a deny or an ask
    alike, as with the other agents (`docs/product/per-agent-launch-specifics.md`). Measured: no `*`
    (`{"bash": "allow", "edit": "deny"}`): `octoboard_** allow`, `bash allow`, `edit deny`; the user's `*` first
    with a rule naming a tool after it (`{"*": "ask", "edit": "deny", "octoboard_archive_session": "deny"}`):
    `* ask`, `octoboard_** allow`, `edit deny`, `octoboard_archive_session deny`; the user's `*` in the middle (the
    example under `ask` above); the user's `*` last with a rule naming a tool before it
    (`{"edit": "deny", "octoboard_archive_session": "deny", "*": "ask"}`): `edit deny`,
    `octoboard_archive_session deny`, `* ask`, `octoboard_** allow`, where the user's own `*` had already overruled
    their deny. A rule of the user's under the very key `octoboard_**` stands as written where it follows their `*`
    (`{"*": "ask", "edit": "deny", "octoboard_**": "deny"}` stayed as it was, the tools denied) or where they
    write no `*` (`{"octoboard_**": "ask", "edit": "deny"}` under `ask`: `* ask`, `octoboard_** ask`, `edit deny`),
    and gives way where their `*` overruled it (`{"octoboard_**": "deny", "*": "ask", "edit": "deny"}`: `* ask`,
    `octoboard_** allow`, `edit deny`). An agent's own `permission` is followed after the configuration's, so where
    it writes a `*` of its own the hook writes the rule after that one too
    (`agent.build.permission` of `{"*": "ask", "webfetch": "deny", "octoboard_instruct_session": "ask"}` gave
    `* ask`, `octoboard_** allow`, `webfetch deny`, `octoboard_instruct_session ask`); an agent's rules with no `*`
    are left alone (`{"webfetch": "deny"}` stayed one rule, and a top-level `octoboard_archive_session deny` ahead
    of it held). Where an agent's own `*` follows a top-level rule that names a tool, that `*` has overruled the
    rule, by the user's own writing, and Octoboard's rule after it allows the tool.
  - **The legacy keys are folded in before the hook.** A top-level `tools: {"todowrite": false}` was in the
    `permission` the hook was handed (`todowrite deny`, after the default), `mode.build.permission` was
    `agent.build.permission` there, and `agent.plan.tools: {"bash": false}` was that agent's `bash deny`: a deny
    written with them stays after the default like any other.
  - **The default follows OpenCode's built-in rules, the agents' own included, so under `ask` a built-in deny
    becomes a prompt, as it does where the user writes `"*": "ask"` themselves.** An agent's rules are OpenCode's
    defaults, then the built-in rules of that agent, then the merged `permission`, then the agent's own from the
    configuration (`opencode debug agent plan`, `general`, `explore`). So with the default the plan agent's
    `edit deny` and `task general deny`, the `plan_enter`/`plan_exit`/`question` denies of the agents that do not
    have them, the general agent's `todowrite deny` and the explore agent's `* deny` are all asked for; a user's
    own file of `{"permission": {"*": "ask"}}` with no plugin gave the plan agent the same order. The hook is
    handed no built-in rule and the configuration has no place ahead of them, so it cannot keep them; it is left
    as OpenCode's own meaning of an `ask` default. In every mode `octoboard_** allow` follows the explore agent's
    `* deny` too, so a built-in subagent is not denied Octoboard's tools.
  The hook swallows a failure of its own, assigns nothing unless every rule was worked out, and puts back what it
  assigned where a later assignment fails, so a failure leaves the configuration as OpenCode merged it: `ask` then
  asks for nothing more than the user's own rules do, and Octoboard's tools are asked for where those rules ask.
- **`opencode debug agent build` prints the agent's rules in the order they are followed**, as JSON on standard
  output: `permission` is a list of `{"permission", "pattern", "action"}`, OpenCode's built-in rules first (the last
  of them `plan_enter`), then the merged `permission` in its order, a nested rule as one entry per pattern, then the
  agent's own, then one built-in `external_directory` rule. It took 1.3 s in a home with no plugin to install. It is
  not a read: like the TUI's own start it writes `"$schema"` into every configuration file it loads that lacks one
  (the global and the project's `opencode.json` both, unconditionally in the loader, whatever variable is set),
  writes a `.gitignore` into the global configuration directory, creates the database, the log and
  `~/.cache/opencode/models.json` where there is none, and with a `plugin` entry installs `@opencode-ai/plugin` into
  the global configuration directory. Measured side by side in fresh scratch homes, the TUI's start wrote the same
  set, so the command writes nothing a launch from the same directory would not; run ahead of a launch that is then
  not made, it has written all of it for nothing.
- A shell the agent starts gets `{...process.env, ...env}` where `env` is what a plugin's `shell.env` hook filled
  in: a variable cannot be removed there, but an empty `OPENCODE_CONFIG_CONTENT` (or `OPENCODE_CONFIG_DIR`) reads as
  unset. Octoboard's plugin hands the user's own value back this way, so an OpenCode started from the session's
  shell does not load the session's plugin, tools or role; it hands back the user's
  `OPENCODE_DISABLE_TERMINAL_TITLE` the same way, which the launch overrules for OpenCode's own process.
- The command of an MCP server, with its arguments, did not reach OpenCode's log (`<data directory>/log`) or its
  screen: not when the command does not exist, not when it exits at once, not when it runs, at the default log level
  and with `--log-level DEBUG`.

### Events a plugin sees

A plugin's `event` hook receives every bus event as `{type, properties}`. Those Octoboard's plugin reads:

| Event | Properties | When |
|---|---|---|
| `session.created` | `sessionID`, `info` (`id`, `parentID` for a child, `title`) | At the first prompt of a session; for a subagent's child session; for a session started with `/new` |
| `session.updated` | `sessionID`, `info` | Often; carries the generated title |
| `message.updated` | `info` (`id`, `sessionID`, `role`, `finish`, `error`) | A message is created or changes |
| `message.part.updated` | `part` (`type`, `text`, `messageID`, `sessionID`, `id`) | A user message's text, as typed; an assistant message's text, whole |
| `session.status` | `sessionID`, `status.type` (`busy`, `idle`, `retry`) | `busy` at the start of a turn and again with every step of it |
| `session.idle` | `sessionID` | The turn is over; fires twice after an error |
| `session.error` | `sessionID`, `error` (`name`, `data.message`) | `MessageAbortedError` for an interrupt, before the idle |
| `permission.asked` | `id`, `sessionID`, `permission`, `patterns`, `metadata`, `always`, `tool` | A permission prompt opens |
| `permission.replied` | `sessionID`, `requestID`, `reply` | It is answered |
| `question.asked` | `id`, `sessionID`, `questions`, `tool` | The `question` tool asks |
| `question.replied`, `question.rejected` | `sessionID`, `requestID` | It is answered or dismissed |

- A prompt submitted while a turn runs joins it: one `busy` span and one `session.idle` for both.
- A prompt written the moment the answer's message is finished (four runs, the write 1 to 2 ms after the event)
  did not join the turn ending: the idle came first, then the new user message and `busy`, a turn of its own, which
  the plugin reports as one. The user message of the turn that ended is reported once more after the idle.
- **Nothing tells a plugin which conversation the terminal shows.** `/new` fires no event; the new session's
  `session.created` comes with its first prompt. Picking a session from the list (`/sessions`) fires none either;
  the session picked is seen again only when a prompt is submitted in it. A turn left running in a conversation the
  user has moved away from goes on reporting under its own id, and a permission prompt it raises is not drawn until
  the user is back in it (the terminal showed the other conversation's empty box). Octoboard's plugin therefore
  takes the conversation a prompt was last submitted in for the one shown, a prompt that joins a turn left running
  included, and forwards only that conversation's events; the id Octoboard records moves with a turn's start and
  nothing else. The conversation left stays in OpenCode's session list and is resumed by its id.
- A subagent's permission prompt is drawn in the root session's terminal and reported under the child's
  `sessionID` (`permission.asked`, `permission.replied`); Enter allows it and the child's and the root's turns go on.
  Esc on it opens a second step (`Reject permission`, "Tell OpenCode what to do differently", `enter confirm  esc
  cancel`) where Esc on the root session's own prompt rejects at once. A subagent that calls the `question` tool
  raises no question: the call fails in the child and its turn ends.
- Two tool calls of one step ask at once (two `permission.asked` 2 ms apart; a `question.asked` and a
  `permission.asked` 22 ms apart). OpenCode draws one prompt at a time, a permission prompt ahead of a question
  asked before it, and the next once the first is answered. Esc on the first of two permission prompts rejects
  both, then the idle. Esc on a question dismisses it (`question.rejected`), then the idle.
- The `task` tool runs a subagent in a child session, whose events carry the child's `sessionID`; only
  `session.created` and `session.updated` name its `parentID`.
- The `question` tool is on by default in the TUI (the client `cli`).
- A model request that fails is retried (`session.status` of type `retry`); no `session.error` came within twelve
  seconds of a stand-in answering 500. One answered 401 is not retried: one `session.error` carrying the answer's
  message in `error.data.message`, then the idle.
- A permission the user rejects ends the turn: `permission.replied`, then `session.idle`, with no request to the
  model, no answer and no `session.error`.
- By default a tool reaching outside the working directory asks (`permission` is `external_directory`), and editing
  and running commands inside it do not.
- `SIGTERM` while a permission prompt is open ends the turn as an interrupt does (`MessageAbortedError`), then
  `session.status` idle and `session.idle`; no `permission.replied` comes for the prompt.
- Esc while a permission prompt is open rejects it: `permission.replied` with `reply: "reject"`, then
  `session.status` idle and `session.idle`, and no `session.error`. A second Esc a tenth of a second later finds
  nothing to interrupt. So in both cases no answer to a prompt comes after the turn's end; the plugin still drops
  one that would.

Octoboard's plugin forwards, through the session's hook script, payloads of its own: `hook_event_name`, `format`,
`session_id`, and by event `turn_id`, `prompt`, `last_assistant_message`, `error`, `request_id`, `tool` and
`subagent`. The names are `session.created`, `turn.started` (the first `busy` of an idle session),
`session.idle` (only for a turn that is open), `turn.cancelled` and `session.error` (an interrupt and a failure,
told apart by the error's name), and the permission and question events. One hook script runs at a time, in the
order the events came.

The plugin also writes a program status report to the terminal (`OSC 7501 ; state=… ST`, one write to standard
output): `working` at a turn's start and when a prompt is answered, `blocked` with `kind=permission` or
`kind=question` while one is open, `idle` when the turn ends, is cancelled or fails. Octoboard presses an interrupt
key only where the terminal shows the turn running as well as the events, and the title says nothing of it; without
the reports an interrupt was refused with "its terminal does not show a turn running". Octoboard's terminal takes the
reports out of the output. They did not disturb the TUI's drawing in these runs. The plugin reports `blocked`
only for a prompt it has told of, with `kind=permission` while any permission prompt is open, which is the one
drawn. With a terminal of 250 by 90 whose reader stopped reading for 6 and for 12 seconds while a turn ended in a
400-line answer, every report came whole and in order once reading went on: OpenCode's own turn end waited on the
terminal, and the idle report was written with it.

### Terminal

- The title is `OpenCode` outside a session and `OC | <title>` in one, and is cleared at exit. `OpenCode` is first
  written when the input box takes input: a paste sent before it is lost, one sent right after is submitted.
  `OPENCODE_DISABLE_TERMINAL_TITLE` turns the title off; an empty value reads as unset.
- The title is the same while a turn runs as after it.
- A bracketed paste followed by Enter submits the pasted text. One submitted while a turn runs is shown above the
  box marked `QUEUED` and is taken into that turn at its next step. A paste enters neither the command list nor
  shell mode, whatever it starts with: ` /models`, ` /nosuch thing` and ` /usr/bin is a path` (as Octoboard writes a
  leading slash, after a space), `/nosuch thing` with nothing before it, and `!echo hi`, `! echo hi` and ` !echo hi`
  each ran as a prompt to the model, its text as typed, and the plugin forwarded its `turn.started` (one launch with
  the plugin and the stand-in model). Typed, a `/` as the first key opens the command list above the box.
- The input box is a block of rows down a `┃` bar, closed by a rule that starts with `╹` and runs on in `▀`: an empty
  row, the rows of text (indented two cells), an empty row, and the row naming the agent and the model. Outside a
  session the block is indented to the middle of the screen and an empty box shows `Ask anything… "<an example>"`,
  grey and not faint; in a session an empty box shows nothing. At 200 columns (not at 120) a sidebar is drawn to the right
  on the same rows, so text of the sidebar follows the box's on a row; the rule ends where the box does.
- A permission prompt or a question is drawn in place of the box, down the same bar, with no rule under it.
- Ctrl-U deletes back to the start of the line, and at the start of a line the line break before it, at the idle
  prompt and while a turn runs alike. Ctrl-J starts a new line.
- The list opened by a `/` or an `@` typed in the box is drawn right above the box, down the same bar, its rows
  indented one cell where the box's are indented two; the box under it is drawn whole, and Ctrl-U closes the list
  with the text. Octoboard reads the box under it as what was typed.
- A dialog is drawn over the conversation, the box whole under it, with its title and `esc` on its first row:
  `Commands` (ctrl+p), `Sessions`, `Select model`, `Select agent`, `Themes`, `Help` (`esc/enter`), `Status`,
  `Connect a provider`, `Export Options`, `Timeline`. While one is up it takes the keys: a pasted message went into
  the search field of the command, session and model lists (`No results found`), the Enter picked nothing, and no
  prompt was submitted. Octoboard holds its writes while one of the ten is on the screen. Other dialogs were not
  looked for. The dialog covers the middle of the screen only: over a conversation that fills the screen (120 and
  200 columns) the conversation's text stood on the title's row to the left of every dialog and, at 120 columns,
  to its right too, so the title's row is never a row of its own. What stays on a dialog's screen whatever is typed
  into it: under the session list `pin/unpin ctrl+f  delete ctrl+d  rename ctrl+r`, under the model list
  `Connect provider ctrl+a  Favorite ctrl+f`, in the help `Press ctrl+p to see all available actions and commands`,
  in the status `N MCP Servers` (`No MCP Servers` with none, `1 MCP Servers` with one), in the export options
  `Filename:` and `Press return to confirm, tab for options`. At 40 columns the two lists' key hints are split into
  columns that wrap (`pin/` over `unpin`, `Connect` over `provider`) and the two sentences wrap; Octoboard asks,
  beside the title, only for what stays on one row there: `Press ctrl+p` in the help, `MCP Servers` in the status,
  `Filename:` and `Press return to confirm` in the export options. The command, agent, theme, provider and timeline
  lists keep nothing but the title row: the `Search` placeholder goes with the first key and the rows become what
  the search matches (`No results found`). Octoboard recognises every one of the ten by its title row as drawn: the
  title, a run of blanks, `esc` (`esc/enter` for the help). The run was the same at 100, 120 and 200 columns (31
  cells under `Connect a provider`, the shortest, to 69 under `Sessions` and `Timeline`) and shrinks with the
  terminal below that: 31 to 59 at 80 columns, 29 to 45 at 60, 9 (`Connect a provider`) to 25 (`Status`) at 40.
  Octoboard asks for nine blanks at least, so a sentence that spells a title with `esc` after it
  (`Timeline escape hatch`, `Status escalated`) is no dialog. Below 40 columns OpenCode still draws the dialogs with
  fewer blanks (5 to 15 at 30 columns, 1 to 9 at 24), a longer title running into its `esc`
  (`Connect a providersc` at 30) and, at 20, wrapped onto rows of its own; at 16 it fails as it starts (Bun reports
  a segmentation fault). So 40 columns is the narrowest terminal at which Octoboard recognises all ten. In a
  narrower one Octoboard does not hold its writes for a dialog it cannot see: a message written while one is up
  lands in it (measured at 30 columns: the text in the command list's search field, the list still open, no prompt
  submitted and no event), and since every prompt submitted at the idle prompt is reported (`turn.started`, 0.62 s
  after the first Enter of a launch and 0.10 s after the next two, one launch with the stand-in model), a write no
  report follows is not taken as sent. At 30 columns the input box is drawn whole and empty under seven of the ten
  dialogs; the session list covers the whole box, and the model and provider lists cover its upper rows. So
  OpenCode declares that a box reading empty off a whole frame after an unconfirmed write means the message went
  elsewhere: its sender and the user are told `message_not_delivered` (it may be lost; send it again), and nothing
  is pressed into the dialog again. Under a dialog that covers the box, the box is unknown and the message is left
  unsent as any other (`message_not_sent`). A message written while a turn runs is not waited on (OpenCode queues it until the turn ends), so one
  that lands in an unrecognised dialog then is lost without a word.
  Each of the ten is opened by the person (ctrl+p, or its command), and
  none is raised by OpenCode to ask something: a session showing one holds its writes and is not shown or reported
  as waiting for the user.
- No confirmation is shown at a first run in a folder, one holding an `AGENTS.md` included, and none is recorded:
  a first run and a turn there wrote the database, `<state>/opencode` (`model.json`, `prompt-history.jsonl`,
  `locks`) and an npm log under `~/.npm/_logs`, and nothing in a configuration directory or in the folder. So a
  console session's directory needs no trust from Octoboard.
- **Esc interrupts a turn only when pressed twice**: the first press shows `esc again to interrupt`, the second ends
  the turn. Two presses a tenth of a second apart are taken as two keys. At the idle prompt one Esc, or two, does
  nothing and leaves a draft in the input box as it was.
- An interrupt does not put the interrupted prompt back in the input box. A prompt submitted while the turn ran is
  kept in the conversation but does not run once the turn is cancelled.
- Octoboard uses the plugin's terminal report (OSC 7501) where it reaches the terminal. With the TUI attached to
  the private server, the server-side plugin reports the hook status and the attached terminal draws a spinner and
  `esc interrupt` below the composer. The interrupt uses that footer alongside the hook status. On Windows an
  active stand-in request closed early after two Esc keys and the terminal returned to the idle prompt. An Esc
  pressed on a stale reading lands on the idle prompt,
  where it does nothing and leaves a draft as it was, and an open permission prompt or question reads as waiting on
  the user from the hook status, which keeps the key away. Both were measured again on 1.19.6 on 2026-11-23 (the
  version had not moved): one Esc and then two more at the idle prompt changed nothing on the screen, kept the draft
  and forwarded nothing; Esc on an open permission prompt rejected it and Esc on a question dismissed it, each ending
  the turn with an idle and no interrupt. Both are to be measured again on a new OpenCode version.
- On `SIGTERM` it ends its turn as an interrupt does, ends the command it is running (a `sleep` it had started was
  gone) and exits with status 0.

### Accounts and models

- The data directory (`~/.local/share/opencode`, by `XDG_DATA_HOME`) holds the database `opencode.db` and, once a
  credential is stored, `auth.json`, an object with an entry per model provider. No variable of OpenCode's own moves
  it, so OpenCode has the default account only. A launch given an account's directory is refused
  (`provider_unsupported`, operation `owned_accounts`): planned, it would run as the default account under the
  other's name. On 1.19.6, `db path` still named the database under `XDG_DATA_HOME` with
  `OPENCODE_DATA_HOME`, `OPENCODE_DATA_DIR` and `OPENCODE_TEST_HOME` all pointed elsewhere. The executable has
  no OpenCode-specific data-directory override. Carrying a conversation and sharing the default configuration are
  unavailable because there is no second account. OpenCode has no plan quota of its own.
- `opencode auth list` prints the stored credentials and ends them with `N credentials`; when the environment
  supplies a provider's key it adds an `Environment` block ending in `N environment variable(s)`. Signed out it prints
  `0 credentials`. The output carries colour escapes when piped.
- OpenCode works with no credential and no key in the environment: on the vendor's free models, on a provider whose
  key is written in a configuration file, on a local or compatible provider. Octoboard therefore never reads its
  account as signed out, which would refuse every new session: signed in where a credential or an environment key is
  found, and otherwise not told.
- `opencode models` prints one `provider/model` id per line. Signed out it still lists the vendor's free models
  (`opencode/…-free`); a provider's key in the environment adds that provider's models.
- With a provider's API key stored through `opencode auth login`, `auth.json` holds
  `{"<provider>": {"type": …, "key": …}}`, `opencode auth list` prints the provider with `api` and `1 credentials` in
  the shape of the capture taken with a made-up key, and `opencode models` lists the vendor's free models and then
  that provider's, as with the key in the environment. The free list is the vendor's and changes between runs:
  twelve in the captures, eleven a day later, when Octoboard read that account as signed in from the file and listed
  the 31 models.
- **OpenCode has no plan of its own, so there is no plan quota to read.** It signs in to model providers, each of
  which counts usage against its own key or subscription. `opencode stats` (1.19.6, "show token usage and cost
  statistics") reports what the local database recorded of past sessions, not a limit, a remaining share or a reset;
  Octoboard reads no quota for OpenCode, and its descriptor declares no quota window.
- It is installed by its own installer (under `~/.opencode/bin`), from npm (`opencode-ai`) or with Homebrew, and
  `opencode upgrade` updates it.

### MCP inventory and Claude compatibility

Measured on 1.19.6 on macOS with scratch directories. `opencode mcp list` lists the merged account configuration.
A disabled local entry was printed as `○ disabled_example disabled` and an executable that exited before connecting
as `✗ failed_example failed`, with a diagnostic and command beneath it. The provider takes the names and statuses.
The command and diagnostic can include credentials, so they stay out of the inventory. The account's lookup runs from
its home, so it does not include a project's separate configuration.

A local model stand-in received markers from both the scratch home's `.claude/CLAUDE.md` and the project's
`CLAUDE.md`, and `opencode debug skill` found the scratch home's Claude skill. `OPENCODE_DISABLE_CLAUDE_CODE=true`
suppressed both instructions and skills. `OPENCODE_DISABLE_CLAUDE_CODE_PROMPT=true` suppressed the instructions only,
and `OPENCODE_DISABLE_CLAUDE_CODE_SKILLS=true` suppressed the skill only. Octoboard sets these flags per launch with
its Claude compatibility setting and restores the shell's values for commands the agent runs.

**Log in in Octoboard is not offered.** In 1.19.6, `opencode auth login --help` exposes the provider and method
selectors `--provider` and `--method`. Its API-key path asks for a credential in the CLI. The available account's
measured login is by API key. The OAuth flow and credential state are shelved until the user signs in with OAuth.

### Conversations in the database

Measured on 1.19.6 on macOS on 2026-11-23, in a scratch home, scratch XDG directories and scratch working directories
(one outside a git repository, one a fresh repository), with a local stand-in answering the model's requests: five
sessions made with `opencode run` (a plain turn, a `bash` tool call, a `task` subagent, a request the stand-in answered
401, a turn in the repository) and a TUI on one of them. No real model turn was spent. Times are wall-clock, each of
one run.

- **A conversation is rows of `opencode.db`** (SQLite in WAL mode, with `opencode.db-wal` and `-shm` beside it): a
  `session` row (`id`, `parent_id`, `project_id`, `directory`, `title`, `tokens_input` and the other running totals,
  `time_created`, `time_updated`, `time_archived`), its `message` rows (`id`, `session_id`, `time_created`,
  `time_updated`, and `data`, the message's JSON) and their `part` rows (`message_id`, `session_id`, `data`). The ids are
  `ses_…`, `msg_…` and `prt_…`, 26 letters and digits after the prefix; none is a UUID. There is no file per
  conversation. A session in a git repository also has a snapshot directory, `snapshot/<project id>/<hash>`, named for
  the project, not the session.
- **`opencode export <id>`** prints one JSON object on standard output, `{info, messages}`, and
  `Exporting session: <id>` on standard error. `info` is the session (`id`, `title`, `directory`, `model`, `tokens`,
  `time`, `parentID` for a child). Each of `messages` is `{info, parts}`: a user message's `info` has `role: "user"`
  and `time.created`; an assistant message's has `role: "assistant"`, `parentID` (the user message it answers),
  `modelID`, `providerID`, `tokens` (`total`, `input`, `output`, `reasoning`, `cache.read`, `cache.write`), `finish`
  (`stop`, `tool-calls`) and `time.completed` once it is over, or `error` (`name`, `data.message`) and no parts for a
  request that failed. A turn with a tool call is two assistant messages. The parts are `text` (`text`, `time`),
  `tool` (`tool`, `callID`, `state` with `status`, `input`, `output`, `metadata`, `title`, `time`), `step-start` and
  `step-finish` (`tokens`). A subagent's call is a `tool` part of tool `task` whose `state.metadata.sessionId` names
  the child session, which is exported by its own id and is not in the parent's export. It took 1.8 to 2.6 s for a
  session of one or two turns, and 1.0 s with the files warm. An id the database does not hold prints
  `Error: Session not found: <id>` on standard error, nothing on standard output, and exits with status 1.
  `--sanitize` redacts the transcript.
- **`opencode session list --format json`** prints an array of `{id, title, updated, created, projectId, directory}`,
  newest first; `-n` limits it. It lists only the sessions of the project of the directory it is run in (run outside
  the repository it did not list the repository's session, and the reverse), and never a subagent's child session. In a
  data directory with no database it prints nothing at all, exits with status 0, and creates the database. 6.4 s on the
  first run, 1 to 3 s after.
- **`opencode db "<SQL>" --format json`** runs a query against the database and prints the rows as a JSON array
  (`tsv` by default); `opencode db path` prints the database's path. It answers from any directory and for every
  session, child sessions included: `select id, parent_id from session` listed all five. 0.7 to 0.8 s. It takes any
  statement, so a caller that only reads sends only `select`.
- **All three run while a TUI holds the database, and while its turn runs.** With a TUI open on a session and a turn
  in progress (the stand-in holding its answer), `export`, `session list` and `db` each answered in about a second and
  showed the turn so far: the user message and an assistant message with no parts and no `time.completed`.
- **They are not read-only on the files.** `session list` and `export` change the modification time of `opencode.db`
  (and of the `-wal` and `-shm` files) with no row changed; `db "select 1"` changes that of the `-wal` and `-shm`
  files alone. Each appends to `log/opencode.log` in the data directory.
- **`opencode session delete <id>`** removes a session: it prints `Session <id> deleted` and exits with status 0, in
  1.1 s, from any directory, while a TUI was open on another session. The session's row, its child sessions' rows and
  every `message`, `part` and `event` row of them were gone, and no file in the data or state directory named the id
  afterwards. It leaves the `project` row and the project's snapshot directory. An id the database does not hold
  prints `Error: Session not found: <id>` and exits with status 1. There is no Trash and no undo.
- **The context figure OpenCode shows is the last assistant message's `tokens`**, not the session's totals: after a
  turn of two requests of 1,234 input and 56 output tokens each, the TUI's sidebar read `Context 1,290 tokens 1% used`
  and its footer `1.3K (1%)` (the model's `limit.context` was 100,000), while `session.tokens_input` and the export's
  `info.tokens.input` read 2,468. The window is the model's `limit.context` from the provider's configuration; no row
  holds it.
- So each of what Octoboard reads of a conversation has a means in OpenCode's own commands, with no database library:
  the history and the feed from `export <id>` or from `db` over `message` and `part` (ordered by `time_created`, and
  by `time_updated` for what changed); whether an account holds a conversation from `db "select id from session"`
  (`session list` cannot say: it leaves out other projects and child sessions); the context figure from the last
  assistant message's `tokens`; removal through `session delete <id>`. Not measured: a session of thousands of
  messages (how long `export` takes and how much it prints), a compacted or reverted session's rows, a session deleted
  while a TUI is open on it, and `time_archived`.

Octoboard uses bounded `db` SELECTs against an existing database for history, live feed, context tokens and
resumability. A single session/message/part query distinguishes an empty session from one that is missing. An
assistant message still streaming is withheld along with the rows after it. A completed message becomes one stable
feed line, and its hook stirs the collector while the turn continues. The database does not give a model's context
window, so the context reading reports tokens. Explicit cleanup runs `session delete`; an unused-session deletion
or automatic archive cleanup keeps the database conversation. OpenCode still owns its log and database journals.
In a 1.19.6 probe with three sessions busy against the stand-in, three CLI reads took 1.30 s serially and 0.58 s in
parallel. Octoboard admits two OpenCode record commands at a time with a five-second command limit. The common
collector schedules row queries independently and applies their completed results during its regular passes.

### Capabilities the released agents have

Measured on 1.19.6 on macOS on 2026-11-23, in a scratch home, scratch XDG directories and a scratch working directory
(a fresh git repository), with the plugin as committed and a local stand-in answering the model's requests
(`@ai-sdk/openai-compatible`, the model `stub/m`; for `apply_patch`, the same stand-in under the model id
`gpt-5.1-codex`). No real model turn was spent.

- **The cross-project guard: built, through the plugin.** OpenCode has no hook, but an error a plugin throws from
  `tool.execute.before` (`(input: {tool, sessionID, callID}, output: {args})`) refuses the call: the tool does not
  run, no permission prompt opens (the hook runs before the tool asks, so a `write` outside the working directory
  raised no `external_directory` prompt), and the model is handed the error's message, verbatim, as the tool's
  result (`Blocked: PROBE REASON for write` came back to the stand-in as the tool message); the turn goes on. The
  tools that write a file are `write` (`filePath`, `content`) and `edit` (`filePath`, `oldString`, `newString`,
  `replaceAll`), and for a GPT model `apply_patch` (`patchText`, a patch whose `*** Add File:`, `*** Update File:`,
  `*** Delete File:` and `*** Move to:` headers name the files) in their place. These three are all: 1.19.6's own
  rule set files the tools `edit`, `write` and `apply_patch` under the `edit` permission and no others, no
  `multiedit` or other writing tool is in its program (searched on 2026-11-24), and the tools it offered the
  stand-in were `bash`, `edit`, `glob`, `grep`, `question`, `read`, `skill`, `task`, `todowrite`, `webfetch` and
  `write`. Octoboard's plugin runs the launch's
  guard script for each call of those, with `{format, tool, args, cwd}` on its standard input, `cwd` being the
  directory OpenCode hands the plugin, and throws the reason of a `{"decision": "deny", "reason": …}` it prints;
  anything else, a failure, or a script that outlasts the guard's ten seconds lets the call through. Measured with the
  plugin and a stand-in guard script: a `write` into a directory the script refused was not written and the model
  was handed the refusal; one inside the working directory was written. `bash` is not asked about, as with the other
  agents: the git layer catches what it writes.
- **Native memory: none.** OpenCode 1.19.6 keeps no memory of its own across sessions: nothing in its configuration
  schema, its commands or its program names one, and the turns of these probes wrote nothing but the database, the
  log, the snapshot directory and `<state>/opencode` (`model.json`, `prompt-history.jsonl`, `locks`). What carries
  from one session to the next is the instruction files the user keeps (`AGENTS.md` and the like), which OpenCode
  writes only when the user runs its `/init` command. Declared `NativeMemory::Absent`; there is nothing to switch off.
- **Image messages: built.** An image file's path pasted on its own becomes an attachment: the box shows
  `[Image <n>]` for each, numbered from 1 and separated by a space (`[Image 1] [Image 2] look at these`), and the
  request carries the image (`image_url`) for a model that takes images; a model that does not gets an error text in
  its place. A path pasted with other text (`see /…/red.png`) stays text, and the Enter after it is not lost. The
  plugin reports the prompt as the box held it, placeholders included (`[Image 1] ` for an image with no text).
- **Slash commands: built.** A command pasted with its arguments (`/probecmd hello`) and submitted runs as the
  command (`command.executed`; the plugin reports the template's expansion as the prompt, `PROBECMD says hello`), so a
  paste does reach the command where the text names one. A lone `/name` opens the command list above the box, its
  first row highlighted (rows down the box's bar, one cell in: `/probecmd     probe`; `No matching items` for a name
  no command has); an Enter there takes the completion into the box (`/probecmd ` with a space after it) rather than
  running it, and a second Enter runs it. A lone `/name ` with the space already after it opens no list and runs on
  one Enter; a space typed after a pasted `/probecmd` closes the list, and the Enter after it runs the command, while a
  space and an Enter written together left `/probecmd ` in the box and ran nothing. So OpenCode declares that a lone
  command gets a space typed after it: Octoboard pastes `/name`, reads the list (another command offered is refused
  as `command_not_recognised`), types the space as a key of its own, waits for the list to close and presses Enter. A
  name no command has (`/nosuch`, then the space and Enter) goes to the model as the prompt `/nosuch `, as a command
  with arguments that names none does; so a lone command whose list reads `No matching items` alone is refused as
  `command_not_found` (`ScreenReader::list_offers_no_command`), its box cleared with Ctrl-U, and gets neither the space
  nor the Enter. The list is read as up while a row of the bar, one cell in, stands
  right above the box (a command, a mention for `@`, or `No matching items`), and as closed while the row above the box
  is anything else; with no box drawn (a permission prompt, a question) it is not read at all, and the Enter is not pressed.
  Measured again on 2026-11-24 against the stand-in (`testdata/opencode-captures.md`, "The command list and the
  space"): at the start screen and in a session, the space closed the list after `/probecmd` and after `/zzz`
  (`No matching items`) alike, the box still reading the command, and `/probecmd`, the space and one Enter ran it.
  `No matching items` is drawn in a grey of the palette (`ESC [ 38;5;8 m`), not faint, so it stays on the screen the
  input box is read from: the list reads as up, offering no command.
- **Command groups at exit: declared.** Every command OpenCode runs leads a process group of its own, the shell being
  a child of OpenCode's process. On `SIGTERM` it ended them all, within a second, a process the command had left
  behind with its parent gone (`(sleep 304 &)`, parent pid 1, in the command's group) included, and the same with two
  commands of one step running at once (two groups).
- **Answering its prompts: built, both kinds.** Its permission prompt opens with `Allow once` selected among
  `Allow once`, `Allow always` and `Reject`, the selection drawn by a background colour alone, the labels in the same
  text whichever is selected. `h` and Left select the previous option and `l` and Right the next, wrapping round;
  Enter picks; Esc rejects. `Allow always` opens a second step (`△ Always allow`, "This will allow the following
  patterns until OpenCode is restarted", `Confirm` selected, `Cancel`), confirmed by Enter (`reply: "always"`).
  `Reject` on a root session's prompt rejects at once, with no text (`reply: "reject"`); on a subagent's it opens
  `Reject permission` with a text field. Since a key that moves the selection changes no text on the screen, the
  answers are pressed from the prompt as it opens: Enter for `Allow once` (`reply: "once"`); `l` and Enter written
  together for `Allow always` (measured: one write of both opened the second step, and its Enter replied `always`);
  Esc for `Reject` (`reply: "reject"`). That is safe only for a prompt nobody has typed into, so OpenCode's reader
  declares its prompts unreadable after input: Octoboard publishes a prompt only while nothing has been written into
  the session since it first saw the prompt up, and never again once anything has; a key typed at the terminal
  leaves the prompt to the terminal. The declaration is the reader's, so it holds for its questions too, whose
  choices could be read after a key: a question typed into at the terminal is withdrawn for good as well. What the
  prompt shows under its title: `# Shell command` and `$ <command>` for `bash`, `→ Edit <file>` for `edit` (the file
  relative to the session's directory, `.` for the directory itself, else under `~`, else whole, as the TUI formats
  a path against its directory and home; the permission's pattern is relative to the repository's root, or to `/`
  outside a repository: `tmp/…/sub/dir/made.txt` for `→ Edit sub/dir/made.txt`), `← Access external directory
  <dir>` and its patterns for a path outside the working directory; a command that does not fit one row wraps by
  word onto the next rows (measured at 120 columns). The `Always allow` step lists the permission's `always` patterns
  (`- echo *`), or says "This will allow <permission> until OpenCode is restarted." where they are `*` alone, as for
  every `edit` call ("This will allow edit until OpenCode is restarted."). Its question shows the question, the
  options numbered with their descriptions under them, then
  `Type your own answer`, and the hint `↑↓ select  enter submit  esc dismiss`. With one question that takes one
  answer, `1` to `9` pick an option and send it at once (`answers: [["B"]]`); the digit after the last option opens
  the field for an answer of one's own (the placeholder `Type your own answer` under it), a paste fills it, and Enter
  sends it (`[["my own answer"]]`); Esc dismisses it (`question.rejected`). Several questions, or one that takes
  several answers (` (select all that apply)` after the question, `[ ]`/`[✓]` boxes), get a row of tabs (the headers,
  then `Confirm`): a digit picks an option of a question that takes one and moves to the next tab, an own answer's
  Enter does the same, a digit toggles an option of one that takes several (the own answer's field opens on its digit
  and its Enter checks it), `l` moves on, and a last `Review` page (`First: A`, `Second: D`) is sent by Enter
  (`[["A"], ["D"]]`, `[["mine"], ["D"]]`, `[["A", "mine"]]`). The question tool's `custom: false` changes nothing: an
  answer of one's own is still offered. A question, an option's label and its description longer than a row wrap by
  word onto the next rows, never truncated (measured at 120 columns, a question and a label of two rows, a description
  of two). The plugin forwards, with `permission.asked` and `question.asked`, the call's id (`call_id`, the event's
  `tool.callID`) and what is asked (`input`): a permission's `patterns`, its `always` and the strings of its
  `metadata` (`command` for `bash`, `filepath` for `edit`) without its `diff`, for an `edit` its file as the prompt
  draws it (`drawn_path`, formatted against the plugin's `directory` and the home directory as the TUI does), a
  question's `questions` (`question`, `header`, `multiple`, `options` with `label` and `description`). Octoboard's
  reader matches a permission prompt to the newest call whose whole detail it draws, over the rows it wraps to with
  whitespace taken out (the `$ <command>` of a `bash` call, an `edit` call's `drawn_path` exactly, the listed
  patterns of an `external_directory` call, every pattern of any other), never to a call with no patterns, and leaves
  a prompt that calls asking different things all fit unpublished; a question it matches to the newest call whose
  first question and options (labels with their descriptions) it draws whole, and publishes only on its first page,
  untouched. It presses a key only into a prompt it reads as the call's, the `Always allow` step by the call's
  `always`. The
  `blocked` the plugin reports to the terminal right as it queues the prompt's event moves no status here: OpenCode's
  status comes from the plugin's events, its reports stand only for the activity glyphs, and the watch of the screen
  starts from the `permission.asked` event, which records the call first.
- **Installed version: built for npm and Homebrew.** An npm install's program is `opencode-ai/bin/opencode.exe`, a
  copy of the platform package's `node_modules/opencode-<os>-<arch>…/bin/opencode`, and the `package.json` two levels
  up of either names the version (`opencode-ai` 1.19.6, `opencode-darwin-arm64` 1.19.6). Homebrew's formula
  `opencode` installs into `Cellar/opencode/<version>/bin/opencode`. The scoped OpenCode 2 npm packages
  also carry a manifest. The vendor's installer (`https://opencode.ai/install`, read on 2026-11-23) moves the downloaded program
  into `~/.opencode/bin/opencode` and writes nothing beside it, so that install's version is asked of the program.

### Not measured

- A credential other than an API key (an OAuth sign-in), and its `auth list` row: the user's `auth.json` holds one
  entry, of type `api` (2026-11-23). This needs an OAuth sign-in to a model provider through `opencode auth login`.
- A dialog other than the ten above, and a dialog in a terminal too small to show the box under it.
- A prompt answered in a conversation the user came back to through the session list without submitting a prompt
  there: nothing says the terminal shows it, so its answer is not forwarded and the id recorded stays the other
  conversation's until the next prompt.
- What OpenCode does with a `{env:…}` in a variable's value on its second reading in every case.
- A `{file:…}` whose file is not UTF-8, and one in a line comment that follows other text on its line.

### ACP mode

Measured on 1.19.6 on 2026-11-22 with `opencode acp --cwd <a scratch directory>`, a scratch `HOME` and scratch
`XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_STATE_HOME` and `XDG_CACHE_HOME`, with no credential. The probe wrote `initialize` (protocol version 1, no client capability) and then `session/new` (a
scratch working directory, no MCP server) as JSON-RPC lines on stdin, read the answers, sent no prompt, and ended the
input.

- `initialize` was answered after 17 s in the new directories (it installs its packages first): `agentInfo`
  (`OpenCode`, the CLI's version), one `authMethods` entry (`opencode-login`) and capabilities for loading, resuming,
  forking, closing and listing sessions.
- `session/new` was answered 3 s later with `sessionId` and `configOptions`, and no `models`: the option `model`
  lists the ids `opencode models` prints (the vendor's eleven free models that day) with the current one
  (`opencode/big-pickle`), and `mode` lists `build` and `plan`. There is no effort option. An
  `available_commands_update` with three commands followed.
- **The session is a row in the database**: `opencode.db`'s `session` table held one row after the run.
- The process exited by itself 0.2 s after its input ended. Nothing was spent: no prompt was sent.

It answers nothing the provider lacks but the model a new session starts on and the two modes, so no provider code
uses it.

### A server with the TUI attached to it

Measured on 1.19.6 on macOS on 2026-11-23, in a scratch `HOME` and scratch `XDG_DATA_HOME`, `XDG_CONFIG_HOME`,
`XDG_STATE_HOME` and `XDG_CACHE_HOME`, with no credential and a local stand-in answering the model's requests as a
provider of the inline configuration; no real turn was spent. The server was started with
`opencode serve --port <port>`, a session created with `POST /session`, and the TUI attached in a pseudoterminal with
`opencode attach <url> --session <id> --dir <the working directory>`. Octoboard starts this server privately on a free loopback port and attaches the TUI with its password in the
process environment.

- **The server.** `opencode serve` listens on `127.0.0.1` by default and prints
  `opencode server listening on http://127.0.0.1:<port>` on standard output, also with `--port 0`, which picks a free
  port. It was ready two seconds after it started in directories it had already initialised.
- **The password.** With `OPENCODE_SERVER_PASSWORD` set in the server's environment every route asks for HTTP basic
  authentication, user `opencode` (`OPENCODE_SERVER_USERNAME` names another): a request with no credential, or a
  wrong password, is answered `401` with `WWW-Authenticate: Basic`. `opencode attach` reads the same two variables,
  or takes `--password` and `--username`; it attached with the variable alone.
- **The route that sends a prompt.** `POST /session/<id>/prompt_async` with `{"parts": [{"type": "text", "text":
  …}]}` answers `204` at once and starts a turn in an idle session. A session id that does not exist is answered
  `404` with `{"name": "NotFoundError", …}`, a body that is not JSON `500`. A prompt with an empty `parts` list is
  accepted (`204`) and starts a turn all the same. `POST /session/<id>/message` is the same request answered when the
  turn is over, and `GET /session/status` lists the sessions that are busy (`{"<id>": {"type": "busy"}}`), an idle one
  not at all. The server also serves the next generation's routes under `/api/…` (`POST /api/session/<id>/prompt`
  with a `delivery` of `steer` or `queue`); they were not measured.
- **What the attached TUI shows.** A prompt sent through the route appears in the TUI as the user's own message, with
  the answer under it, exactly as one typed there. One sent while a turn runs is shown above the input box marked
  `QUEUED` and is taken into that turn at its next step, as a pasted one is: the stand-in was asked for it right
  after it had answered the running request, within the same busy span.
- **An interrupt through the server.** `POST /session/<id>/abort` answers `200` with `true` and ends the turn (the
  TUI shows it `interrupted`). A prompt that was waiting `QUEUED` is not run and is not handed back: it stays in the
  conversation, and the model first sees it with the next prompt, before that prompt.
- **The TUI ends first.** On `SIGTERM` the attached TUI exits with status 0 and the server runs on: the session took
  a prompt through the route afterwards and answered it.
- **The server ends first.** The server exits within a second of `SIGTERM`. The attached TUI does not exit and says
  nothing by itself; a prompt submitted in it then shows `Failed to send prompt` and
  `ECONNREFUSED: Unable to connect. Is the computer able to access the url?`, and the TUI stays up. So a launch that
  attaches the TUI to a server of its own has to end the terminal when the server has gone.
- **Octoboard's attachment.** On 1.19.6, a launch-scoped TUI plugin installed through `OPENCODE_TUI_CONFIG`
  reported its active session id. The id matched the server plugin's turn events after an HTTP prompt, and changed
  when `POST /tui/select-session` switched the attached terminal to another session. The transport reads that
  selection for text delivery. Until it knows a selection, and when the user has a custom `OPENCODE_TUI_CONFIG`,
  it uses terminal input. Commands and images use the terminal. A pre-existing user server password keeps the
  standalone launch. In Auto mode the server plugin replies once to each permission the agent asks for. In the
  1.19.6 stand-in run a `bash` tool configured to ask completed with that reply, and its shell confirmed the
  launch password was absent from the command environment.
- **Ownership after an interrupt.** On 1.19.6, a queued prompt could be removed with
  `DELETE /session/<id>/message/<id>` after an abort (`200`, then its id was absent from the message list).
  Octoboard leaves the accepted prompt with OpenCode, where it is visible in the queue and picked up with the next
  prompt. It does not submit another copy.
- Not measured: a command other than the agent's shell inheriting the server password, images through the HTTP
  route, a login flow on an OAuth account, or Windows.

### OpenCode 2

Measured on macOS ARM64 on 2026-11-24, with separate scratch homes and XDG directories, a loopback inference stand-in,
and a stand-in MCP server. The macOS ARM64 binaries came from the platform packages of pinned
[`@opencode/cli@2.3.2`](https://registry.npmjs.org/@opencode%2fcli/2.3.2),
[`@opencode/cli@2.4.0`](https://registry.npmjs.org/@opencode%2fcli/2.4.0) and
[`@opencode/cli@2.4.1`](https://registry.npmjs.org/@opencode%2fcli/2.4.1). Homebrew Core published 2.4.0 that day,
and the upstream [v2.3.2 tag and release](https://github.com/anomalyco/opencode/releases/tag/v2.3.2) publish
source and platform binaries. The old `opencode-ai` package still carries the 1.x line. The measured probes used no
account and spent no real model turns. The 2.4.1 npm manifest declares `bin/opencode.exe` as the launcher, and
its postinstall script links or copies the selected platform binary there. Octoboard recognises both npm package
layouts and uses the package of the detected install for its registry check and npm update.

- **Version and launch.** Installed npm and Homebrew versions are read from their manifest or Cellar path. An
  unpackaged binary is asked for `--version`, which 2.x prints as `opencode v2.x.y`. The major version picks the
  launch, server and record generation (1 or 2); no version is refused, and a format a newer version changed fails where it is read. All three 2.x builds take
  `opencode --server <url> --session <id>` to start the TUI on the private server. The server reads the launch's model
  from the inline configuration. Octoboard checks a resumed id at `GET /api/session/<id>` before starting the TUI,
  since `--session` by itself is also allowed to create a missing session in 2.x. When the host cannot run the
  companion, the TUI uses `--standalone` and receives text through the terminal.
- **Configuration and plugins.** `OPENCODE_CONFIG_CONTENT` still takes launch-scoped configuration. 2.x uses
  `plugins` (a path to a directory with `index.js` and `tui.js`), `permissions` (ordered
  `{action, resource, effect}` rules) and `mcp.servers.<name>`. The server plugin's `setup` registers
  `ctx.event.subscribe()`, `ctx.tool.hook("execute.before", …)`, `ctx.shell.hook("create.before", …)` and
  the permission and agent hooks. With the configured default `ask`, the model called the stand-in
  `octoboard_m35_marker` and received its result on all three builds without a permission prompt. The tool hook
  gets `input`. File tools take `path`, and `patch` takes `patchText`. A guarded 2.4.1 `write` returned the
  refusal to the model and left the named file absent. The shell hook returns the original inline configuration
  through the environment. The launch
  leaves the user's configuration files alone. The TUI also loads the plugin directory and exposes the selection
  through `ctx.ui.router.current()`; Octoboard reads that launch's route before sending text. The title still uses
  `OpenCode` / `OC | <title>`. Its switch is `terminal.title` in `OPENCODE_CLI_CONFIG_CONTENT`.
  On 2.4.1, `mcp list --standalone` reported zero servers before the configured MCP plugin loaded.
  `debug config --standalone` returned the configured server under `info.mcp.servers`. Octoboard extracts
  server names from this projection and reports runtime state as unknown.
- **Server and delivery.** `serve --hostname 127.0.0.1 --port 0` reports
  `server listening on http://127.0.0.1:<port>`. `OPENCODE_PASSWORD` authenticates both the server and TUI.
  An unauthenticated API call returned 401. `POST /api/session` with `location.directory` returns 200 with
  `data.id`. `POST /api/session/<id>/prompt` takes `text` and `delivery: "steer"` and returns 200 on
  admission. The TUI displayed the prompt and its answer. The 1.x route and its 204 receipt remain in the
  1.x transport. The adoption record carries the route version so a daemon restart preserves the transport.
- **Events and interactions.** The 2.x bus envelopes put the fields under `data`. The inbox emits
  `session.inbox.enqueued` and `session.inbox.delivered` with the same input id. A completed assistant step emits
  `session.step.ended`, and the execution ends with `session.execution.succeeded`. The v2 bridge forwards those
  as the existing prompt, record change and idle notifications. It translates native permission and question
  events (`permission.asked` / `permission.replied` and `form.created` / `form.replied`). On 2.4.1 the shell
  permission shows the command below the title and `Allow once`, `Always allow`, `Reject`. Enter accepts once.
  `l` and Enter submit the remembered allow directly. In Auto mode the same shell request completed without
  emitting a permission prompt. The single-question form still draws numbered choices
  that submit on a digit. The raw terminal bytes and the launch plugin's hook messages are in
  `apps/daemon/testdata/opencode_screen_*_2.4.1.bin` and `opencode_hook_payloads_*_2.4.1.jsonl`.
- **Records.** Each version uses `opencode.db` for the release channel. 2.3.2 stores v2 conversation rows in
  `session_v2` and `session_message`, with assistant content in the message's JSON. 2.4.1 projects assistant
  content from `session_part`. Both return the same transcript shape through `session export --standalone <id>`,
  with `info` and `messages` and message data keyed by `type`. Octoboard reads the projected export and folds
  completed messages for history, the feed and context. An export issued on 2.4.1 while the inference stand-in
  was paused returned the user message in 0.29 s. Resumability uses the paginated
  `api --standalone GET "/api/session?limit=1000"` listing, with no directory or parent filter. On 2.4.1,
  `session delete --standalone <id>` exited 0 and a concurrent server then returned 404 for that id. It is used
  only for the explicit record cleanup action. The bounded record commands use a private server for the query
  and preserve the user's own service. When a scratch 1.19.6 database was opened in 2.4.1, an export made
  during the native migration reported `Session not found`. Polling `GET /api/experimental/migration/v1` returned
  `running` twice and then `completed`. The next export returned the original id, prompt and answer.
  Octoboard waits for that status before relying on a v2 list, lookup, export or deletion while legacy records
  are being migrated.
- **Migration.** The upstream v2 server can import legacy `auth.json` into its credential table on its first
  start. The CLI can migrate legacy terminal settings into `cli.json`. Octoboard checks for pending migrations
  before launching 2.x or starting a 2.x background command. The credential check reads the completed migration
  marker, without opening a writable SQLite connection or reading a credential. If a migration is pending, the
  user starts their OpenCode 2 directly and closes it before retrying.

In the macOS development-daemon smoke run on 2.4.1, a stand-in turn reported 110 context tokens. Both entries were
readable from the archive. Resume kept the Octoboard and native conversation IDs and accepted another message.
Explicit cleanup removed a disposable conversation from the native database.

## Qwen Code

Everything below was measured on **Qwen Code
0.28.0** on macOS on 2026-11-20 and 2026-11-21, in a PTY of 120 by 40 with a scratch `HOME` and working directory, a
fake key and a local stand-in for an OpenAI-compatible inference API, unless it says it was read from the code. The
captures are in `apps/daemon/testdata/` (`qwen-captures.md`). Qwen Code then updated itself to **0.28.3** (see "Models,
install and update"), on which the hook payloads, the title and its glyphs, the input box, a permission prompt and a
question were checked again on 2026-11-22 and read the same. The sign-in record and what "Through a real model and a
development daemon" says were measured on 0.28.3 with a signed-in account of its own, signed in with an API key.
What is dated 2026-11-23 was measured on **0.28.3** run straight from its own `cli.js`, in a scratch `HOME` with
`general.enableAutoUpdate` off, against the same kind of stand-in.

**Windows 11 ARM64 (23H2, build 22631.6199).** npm's global install put `qwen.cmd` in the
scratch prefix. It ran on ARM64 Node 26.13.0 and reported **0.28.3**. The batch launcher
could report its version but rejected the multiline role argument. Interactive launches
resolve the same installed `cli-entry.js` and start it with Node. The x64 daemon opened
both session kinds through that entry point. In an isolated run on the inbox pseudoconsole,
the first messages sent through the daemon took about 36 seconds each to reach the local
model, with no additional keypresses. The local model selected and invoked Octoboard's
deferred tools. The console read its session, the child reported to the parent, and the
parent instructed the child. The daemon interrupted an active local request and the
terminal returned to idle. Archiving stopped the project session. The registered hooks
explicitly select PowerShell on Windows. See "Settings layers, and how Octoboard is injected"
below for the process-chain measurement.

### Launching and resuming

- `--session-id <id>` names a new session and must be a UUID: anything else is refused with `Invalid --session-id`
  and the usage text. `--resume <id>` continues one. A session that never had a turn keeps no record
  (`<id>.runtime.json` alone), and resuming it, like resuming an id that never existed, prints `No saved session found
  with ID <id>` and exits.
- `-i <prompt>` (`--prompt-interactive`) runs the prompt and stays interactive; a positional prompt is one-shot.
  **Given as two arguments, a prompt that starts with a dash is read as options**: `-i '- list item one'` and
  `-i '--help me with X'` each printed the usage text and exited (0.28.3). In one argument the prompt is taken
  whole: `--prompt-interactive=- list item two`, `--prompt-interactive=--help me with Y` and `-i=--help me with Z`
  each submitted exactly that text. So a launch passes `--prompt-interactive=<task>`. `--model <id>`; there is no
  effort flag.
- `--append-system-prompt <text>` reaches the system message of every model request. On a resume the text that launch
  passes replaces the earlier one: the system message is built afresh on every launch.
- `--approval-mode plan | default | auto-edit | auto | yolo`. With no flag and nothing configured a new session starts
  in `auto` in a folder it trusts, and in `default` while the folder is not trusted. In `auto` a classifier, itself a
  model request, passes each tool call or leaves it to a prompt ("Auto Mode couldn't classify this action …" where it
  cannot be reached). In `default` a command that only reads (`ls`) ran with no prompt, and `mkdir` and `sleep` raised
  one.
- Records: `<Qwen home>/projects/<working directory with separators replaced by dashes>/chats/<id>.jsonl`.
  A TUI start creates `<id>.runtime.json` immediately, including on a turnless start. Headless runs leave no runtime
  file. A turn creates the chat and `<id>.title.json`. A turnless id fails `--resume` with
  `No saved session found` (status 1), and a fresh `--session-id` with that same UUID starts and accepts a prompt.
  Octoboard resumes a turnless session as a fresh native conversation, keeping its Octoboard session id. An archived
  session whose chat has gone cannot be resumed, even if its title or runtime sidecar remains.
- **The current branch.** After two prompts, a conversation-only `/rewind` to before the second appended a
  `system/rewind` record whose `parentUuid` was the old leaf at the end of the first turn. The following
  `system/file_history_snapshot` linked to the rewind, which made that snapshot the leaf and left the abandoned
  messages in the file. Normal system lines, including telemetry and checkpoints, form part of the chain. Qwen
  Code's own leaf selector excludes artifact, source-snapshot, generated-UI and managed-session metadata. The
  history, token count and live feed follow the remaining `uuid` and `parentUuid` chain.
- **History and feed.** A prompt is a `user` record with `real_user` provenance (older records may leave it out).
  A reply is in an `assistant` record's `message.parts`; parts marked `thought` are private reasoning. Tool calls
  use `functionCall` parts and a `tool_result` record updates the matching `toolCallResult.callId`. A captured
  `write_file` carries the absolute path in `args.file_path` and reports its diff in `resultDisplay.fileDiff`.
- **Context size.** A stub returning 23 input tokens and 7 output tokens gave
  `usageMetadata.promptTokenCount: 23` and `totalTokenCount: 30`. The corresponding `Stop` reported
  `input_tokens: 23`, `context_limit: 200000` and `context_usage: 0.000115`. On the key account, the record and hook
  both gave 12272 input tokens, with the same window. Octoboard reads the prompt count on the current branch
  and takes the window from the running launch's `Stop` hook. This lets the count follow a rewind in the chat
  between hook reports.
- **Titles.** The title sidecar's `displayName` came from the opening prompt with `source: "prompt"`, from
  `/rename <name>` with `source: "manual"`, and from fast-model generation (including `/rename --auto`) with
  `source: "auto"`. Octoboard uses manual and automatic titles as names, with the opening prompt as the fallback.
- **Carrying and removal.** The chat, title and runtime sidecars belong to the session. A scratch account resumed
  after all three were copied to the same relative location in its Qwen home, and the next stub request contained
  the original prompt. One real key-account turn also produced a chat and title sidecar, which resumed with
  `/usage` after relocation to a second scratch home. The missing-id control refused it. `usage_record.jsonl`,
  `usage/token-usage-<month>.jsonl` and project `extract-cursor.json` are shared across sessions and stay behind.
  These checks used 0.28.3 on 2026-11-24. The record's own `version` field has been seen as both `0.28.0` and
  `0.28.3` on launches of the managed installation. The probe's explicit CLI version identifies the run.
- It runs in the process it was started in; no child was started on this machine. The code can hand over to another
  runtime (Bun) when one is installed, which `QWEN_CODE_NO_RELAUNCH` turns off; every launch sets it. After a folder
  is trusted in the trust dialog it starts over inside the same process.
- A console directory holding `QWEN.md` and `AGENTS.md` as links to one file gives the model that text once.
- The commands its shell tool starts get (an MCP server and a hook get fewer, and no `QWEN_CODE`; see "Settings
  layers, and how Octoboard is injected") `QWEN_CODE`, `QWEN_CODE_AGENT_ID`, `QWEN_CODE_CLI`,
  `QWEN_CODE_MANAGED_NPM_PIN`, `QWEN_CODE_MANAGED_NPM_ROOT`, `QWEN_CODE_MESSAGING_SOCKET`,
  `QWEN_CODE_MESSAGING_TOKEN`, `QWEN_CODE_MODEL`, `QWEN_CODE_MODEL_IDENTITY`,
  `QWEN_CODE_PRIVATE_RELAUNCH_ENV_PROVENANCE`, `QWEN_CODE_PROJECT_DIR`, `QWEN_CODE_PROMPT_ID`,
  `QWEN_CODE_SESSION_ID` and `QWEN_CODE_STARTUP_VERSION`.

### Settings layers, and how Octoboard is injected

**Linux system settings** (0.28.0, Debian 13 arm64, 2026-11-24). A headless run against a local model stand-in
opened `/etc/qwen-code/settings.json` and completed its turn, traced with `strace -f -e trace=openat`.

- The system settings file is the one `QWEN_CODE_SYSTEM_SETTINGS_PATH` names, else
  `/Library/Application Support/QwenCode/settings.json` on macOS, `C:\ProgramData\qwen-code\settings.json` on
  Windows and `/etc/qwen-code/settings.json` elsewhere. A launch on Windows build 22631.6199 without either
  system path override attempted to read both `settings.json` and `system-defaults.json` in
  `C:\ProgramData\qwen-code`, together with the scratch account's own settings. Octoboard picks among
  them, and decides whether its hook command gets the guard below, by the operating system of the daemon's host
  (`platform::HostOs`), which is the host its sessions run on.
- Every settings file is read as `JSON.parse(stripJsonComments(stripUtf8Bom(content)))` (read from the code): `//`
  and `/* */` comments and a byte order mark are allowed, and Qwen Code keeps the comments of a file it rewrites. A
  file that is not a JSON object after that, or cannot be read, is recorded as a settings error and reported, not
  passed over. Octoboard reads the files the same way, and **a system settings file that is there and cannot be used
  refuses the launch** (`provider.qwen.system_settings_unusable`): written without it, the launch's file would drop
  whatever an administrator set there (`permissions.deny`, the MCP servers allowed, the sandbox) and nothing would
  say so.
- The system defaults file, the lowest layer, is `system-defaults.json` **beside the system settings file**, unless
  `QWEN_CODE_SYSTEM_DEFAULTS_PATH` names another (read from the code). Naming a system settings file elsewhere moves
  where it is looked for, so a launch also sets that variable to the file Qwen Code would have read.
- Hook lists are added across layers: with `UserPromptSubmit` hooks in both the system file and the user's
  `settings.json`, both ran for every prompt, in no fixed order. A plain setting in the system file wins over the
  user's (`ui.hideWindowTitle`, `ui.showStatusInTitle` and `ui.enableFollowupSuggestions` were tried).
- Hooks of the system and the user layer need no approval before they run, and run in a folder that is not trusted,
  before and after its trust dialog is declined.
- A hook is `{"matcher": "", "hooks": [{"type": "command", "command": …, "timeout": …}]}` under its event's name, as
  in Claude Code. The command gets the payload on stdin. A `timeout` of 5 did not cut a hook that took longer than
  5 ms, so it is not in milliseconds: it is in seconds (`timeout * 1e3` in the code; a `sleep 30` hook with
  `timeout: 2` was cut after 2 s, 2026-11-23).
- **A failing hook is shown to the user** (2026-11-23). A `Stop` hook that exits 1 puts what it printed under the
  turn's reply: `⎿ Stop says:` and `Warning: <its stdout, else its stderr>`; a command that does not exist reads
  `Warning: bash: <path>: No such file or directory`, which is also how a hook is known to run in `bash`. A hook cut
  by its timeout shows nothing and holds the turn for the timeout, at each event it is registered for. So each of
  Octoboard's hook commands runs in a subshell with its output dropped and ends in `exit 0`: a daemon that is gone
  or an executable that moved shows the user nothing. On Windows build 22631.6199, 0.28.3 used
  `cmd.exe` by default and printed a failing Stop hook's stderr as a warning in headless mode.
  The `shell: "powershell"` entry selected Windows PowerShell. Its parent was the Qwen Node
  process. The parent's command line held the `--mcp-config` path for the outer Qwen only. Nested
  Qwen processes started by the shell tool and by an MCP server read the outer settings and had
  their own unmarked Node process above the hook shell. A probe using Octoboard's hook mode
  forwarded outer events and discarded the nested MCP process's SessionStart. The registered
  status hooks discard all output in PowerShell. The guard keeps its decision on stdout.
- **A hook can be asynchronous**: with `"async": true` on a command hook that sleeps 6 s, the turn was not held (it
  ended 1.4 s after the prompt, against 6.8 s for the same hook without it) and the hook still ran to its end.
  Octoboard registers none of its hooks so: each event moves the session's status, Qwen Code waiting for a hook is
  what keeps two events in the order they happened, and Octoboard's hook returns in milliseconds, so there is
  nothing to gain and an order to lose.
- `--mcp-config <path>` adds its servers to the user's own: both were started. A server with `"trust": true` has its
  tools run with no permission prompt in `default` mode. `timeout` is in milliseconds by the code's own default
  (`10 * 60 * 1000`), and it bounds one tool call: with `timeout: 5000` a call to a stub server's tool that took
  12 s was ended after 6 s with `Request timed out` (2026-11-23). The launch sets 660 000, a minute over the 600 s
  Octoboard's own forwarding allows a call.
- MCP tools are named `mcp__<server>__<tool>` and are not declared to the model: it finds one with `tool_search`
  (`select:<name>`) and calls it through `tool_call`, which is refused until the tool's schema has been reviewed that
  way. Hooks name the tool itself, not `tool_call`.

- **A Qwen Code started inside the session loads the launch's settings file too**: the variable that names it is in
  the environment of everything Qwen Code starts. With the agent running `qwen -p 'nested hello'` through its shell
  tool, the nested process fired the system file's `SessionStart`, `UserPromptSubmit` and `Stop` with its own session
  id, between the session's `PreToolUse` and `PostToolUse`; a `qwen -p` started by an MCP server's tool did the same
  (2026-11-23). Unguarded, those reach Octoboard as the session's own: a prompt, a turn's end with the nested reply,
  and another conversation's id. **No variable tells the nested process apart in every case**: the shell tool sets
  `QWEN_CODE=1` for what it runs, but an MCP server gets no `QWEN_CODE` (its environment is the process's own plus
  `QWEN_CODE_SESSION_ID`, `QWEN_CODE_PROJECT_DIR`, `QWEN_CODE_MODEL`, `QWEN_CODE_MODEL_IDENTITY` and the two
  `QWEN_CODE_MESSAGING_*`), and neither does a hook (those, `QWEN_CODE_AGENT_ID`, `QWEN_CODE_PROMPT_ID`,
  `QWEN_PROJECT_DIR` and the hook's own `env`), so a `qwen` either of them starts has none. The session id is no
  test: **`/clear` and `/new` each end the session and start one with a new id** (`SessionEnd`, `SessionStart`), which
  every later payload and `QWEN_CODE_SESSION_ID` carry. What does tell them apart is the process that runs the hook:
  a hook's parent (`$PPID`) is the Qwen Code that fired it, one process for every hook of a session, and only the
  session's own was started with the launch's arguments. So Octoboard's hook is given the launch's MCP
  configuration file as its mark (`octoboardd hook --fired-by <file>`): the hook program finds the process that
  fired it, the first above the shells that wrap its command, in the process table, and posts only when that
  process has the file on its command line; where that cannot be told, it posts. Nothing is started for it.
  Measured on macOS (2026-11-23, a script following the mode's rule in the hook's place, the mode itself not being
  built by the session that measured): Qwen Code runs a hook as `bash -c <command>`, a child of its own, and the
  subshell the command runs in is replaced by the program, so the chain is the program, `bash -c …` (which carries
  the hook's text, the key file's path in it), then the Qwen Code that fired it, whose command line holds
  `--mcp-config <file>` and nothing of the hook's. The session's own five events (`UserPromptSubmit`, `PreToolUse`,
  `PostToolUse`, the empty `UserPromptSubmit`, `Stop`) read "posts"; the two of a `qwen -p` its shell tool started
  (`UserPromptSubmit`, `Stop`) read "dropped". Earlier the same day the shell text this replaces was measured for a
  `qwen -p` started by an MCP server too, with the same parent. Registered so on macOS and Linux.
  On Debian 13 arm64 with Qwen Code 0.28.0, `strace` saw Qwen launch the hook through `bash -c` and the
  hook's firing Qwen carry the launch's MCP marker. Console and project sessions moved from working to idle
  with a local model stand-in. Each nested headless run, from the shell tool and from an MCP server, executed
  the inherited hook program twice and made no connection to the hook's loopback port. The outer
  conversation id stayed the same. Windows registers the plain hook command.
  - **A path outside ASCII** (2026-11-23, by hand with `/bin/sh` on macOS): `ps` writes a byte that is no
    character of its locale as an octal escape, so with `LC_ALL=C` a launch file under `…/会话 é/run/…` was never
    found in what `ps` printed. That was the shell text's trouble; the hook program reads the arguments from the
    process table as they are.
- **Hooks can be switched off wholesale**, and Octoboard does not switch them back on: `disableAllHooks: true` in
  any settings layer, safe mode (`--safe-mode`, `QWEN_CODE_SAFE_MODE`) and bare mode (`--bare`, `QWEN_CODE_SIMPLE`,
  which also loads no settings at all); `/hooks` then says `disabled (disableAllHooks)`, `(safe mode)` or
  `(bare mode)` (read from the code). Run on 2026-11-23: with `disableAllHooks: true` in the account's settings, and
  with `QWEN_CODE_SAFE_MODE=1` (the screen then shows `⚠ Safe Mode`), no hook of the system file ran through a whole
  turn, and the OSC 7501 reports and the title's glyphs were made as ever. A variable counts as on when it is `1`,
  `true`, `yes` or `on` (the code's `isTruthy`). With `QWEN_CODE_SIMPLE=1` no settings file is loaded, the account's
  included: an account whose key is in its settings came up at "Connect a Provider", reporting
  `state=blocked:kind=auth`. **Bare mode past that screen** (2026-11-23, the key and the endpoint given as
  `OPENAI_API_KEY`, `OPENAI_BASE_URL` and `OPENAI_MODEL`): the input box came up, signed in, and a turn ran. The
  launch's own settings are lost with every other, and of what they pin only one thing changes: the title kept its
  glyph (`◐︎` while the turn ran) and the reports were made, both being the defaults; no next prompt was suggested
  and no request made for one in the 10 s after the turn; the feedback question did come up (in the copy patched
  to always show it, see "Screens and keys"), which takes nothing of a message written as a paste. So bare mode
  costs the hooks and nothing else Octoboard reads.
  **A `disableAllHooks` that is no boolean** (2026-11-23, one turn each): the highest layer that holds the setting
  decides whatever the kind of its value, and the value counts as JavaScript takes it. `"no"` in the account's
  settings switched the hooks off; `0` in the system file over `true` in the account's left them on, and so did
  `null` there. Octoboard reads it the same way.
  The setting is not pinned, since that would turn back on hooks the user turned off.
  Instead a launch that finds one of the three tells the user: for one of the two variables in the session's
  environment with the notice `provider.qwen.hooks_off`, naming the variable; for `disableAllHooks` with
  `provider.qwen.hooks_off_by_setting`, naming the setting and the settings file whose value decides (the system
  file, the project's, the account's or the system defaults). Each says in its own sentence what to do, a notice
  drawing no remedy, and stays in the window until it is dismissed, under a short title of its own: it says that a
  protection is not in force and names a step. The project's file is read as a project's file is: one that is a
  link out of the project is not followed. Such a launch answers that it has no status integration
  (`AgentProvider::launch_has_status_integration`), so its whole status, turns opened and closed included, comes
  from the reports, and the notice says only that and how to turn the hooks back on. Such a launch also has
  nothing queued again at a cancel (`AgentProvider::launch_cancel_drops_held_messages`): its reports carry no
  prompt, so a held message the turn took cannot be told from one the cancel dropped. The reports give `done` in
  the gap between two model calls of one turn ("Status reports" below), so the provider declares that a `done` may
  be such a gap (`AgentProvider::reported_done_gap`, 1 s): a `done` that the next call's `working` follows within
  it is passed over, and only the turn's last `done` ends it, at most a second late. **A message queued with
  Ctrl+Q reads as part of the turn before it**: the turn it runs as reports its `working` in the same read as the
  first turn's `done` (2026-11-23, 0.28.3, twice against the stand-in: the turn a shell tool of 8 s, the message
  queued while the tool ran; the `Stop` of the first turn came 0.3 s to 0.4 s before the queued message's
  `UserPromptSubmit`, the two reports 0.00 s apart). So such a launch reports the two turns as one, ended once at
  the second's end: a known limit of holding a `done`, which cannot tell a new turn's `working` from the next model
  call's.

So a launch passes `--mcp-config` with a file in the session's scratch directory, `--append-system-prompt`, and
`QWEN_CODE_SYSTEM_SETTINGS_PATH` naming a settings file there that holds the content of the system file it stands in
for, Octoboard's hook added to each event's list, the three `ui` settings above, `ui.enableUserFeedback` off and
`ui.showScrollbar` off (see "Screens and keys"). It also sets `QWEN_CODE_SYSTEM_DEFAULTS_PATH` to the system defaults
file Qwen Code would have read. Each of its hook commands is the daemon's `hook` mode given `--fired-by` with the
launch's MCP configuration file, which keeps it quiet in a nested Qwen Code, inside a wrapper that shows the user
nothing.

**A launch file is never taken for the system layer.** A daemon started from a Qwen Code session's shell tool (a
development daemon run from a session) finds `QWEN_CODE_SYSTEM_SETTINGS_PATH` in its shell snapshot, naming that
session's launch file. Carried over, the file would bring that session's hook with it, and every event of a new
session would be posted to the old one too; and once that scratch directory is gone, the host's real system layer
would be dropped without a word. So each hook Octoboard registers carries `OCTOBOARD_SESSION_HOOK` in its own `env`
(a hook with an `env` entry runs as before, 0.28.3), a settings file holding such a hook is not the system file, nor
is a missing `run/<session>/qwen-settings.json`, and the platform's path is read instead; and whatever is carried
over has Octoboard's hooks taken out first. A relative `QWEN_CODE_SYSTEM_SETTINGS_PATH` is read against the daemon's
working directory, where Qwen Code reads it against the session's. The MCP configuration is an argument, not a
variable, and does not travel this way; `QWEN_HOME` does, as every agent's configuration variable does: a daemon
started inside a session of a pinned account takes that account's directory for its default account.

**To check again on an upgrade:** the guard rests on a hook's parent being the Qwen Code that fired it. A Qwen Code
that ran its hooks through a process of its own in between (a supervisor, a worker) would make every hook exit
unposted, and every session's status silent with no message. A test asks the installed Qwen Code
(`adapter::qwen::settings::tests::the_installed_qwen_code_runs_a_hook_as_a_child_of_the_process_the_launch_started`,
ignored unless asked for, since it needs `qwen` on `PATH`): a headless run (`qwen --mcp-config <file> -p hello`) in a
scratch home whose model endpoint nothing listens on fires `UserPromptSubmit` before the request fails, and the hook
writes down the command line of the process above its shell, which has to be that `qwen` with the file on it
(2026-11-23, 0.28.3). It passed on macOS with 0.28.3 on 2026-11-23. Run it with `--ignored`
after each upgrade. `QWEN_CODE` is no longer read by the guard, so a Qwen Code that set it for
its hooks changes nothing.

### Hook events

Every payload has `session_id`, `transcript_path`, `cwd`, `hook_event_name`, `timestamp` and `permission_mode`; an
event inside a turn has `prompt_id`, `<session id>########<n>`, the same for the whole turn.

- `SessionStart` (`source` `startup` or `resume`, `model`) runs as the process starts, **before the folder-trust
  dialog is answered**, and again after the dialog is accepted, preceded by a `SessionEnd`. So Octoboard does not
  register it, and takes the input box, not a hook, as the sign that the startup is behind the session.
- `UserPromptSubmit` has `prompt` and `submitted_prompt`, a paste as its text with no wrapper. It **fires again
  inside the turn with an empty `prompt`** and no `submitted_prompt`, each time the model is called back with a tool's
  result.
- `PreToolUse` (`tool_name`, `tool_input`, `tool_use_id`, `tool_call_id`) and `PostToolUse` (the same, with
  `tool_response` and `duration_ms`). The `PostToolUse` of an answered `ask_user_question` had no `prompt_id`.
  `PostToolUseFailure` was not seen: a `tool_call` the bridge refused fired no tool event at all.
- `PermissionRequest` (`tool_name`, `tool_input`, no call id) fires for a permission prompt and for
  `ask_user_question` alike, followed by a `Notification` of type `permission_prompt`. **Declining either with Esc
  fires nothing but an `idle_prompt` notification**: no `PermissionDenied`, no tool event and no `Stop`.
- `Notification` of type `idle_prompt` fires after every turn, and also right after a `PreToolUse` while the turn
  runs; in both places it came with and without a `prompt_id`. It says nothing.
- `Stop` has `last_assistant_message`, `stop_hook_active`, `background_tasks`, `crons`, `context_usage`,
  `context_limit` and `input_tokens`. `background_tasks` lists each task with `id`, `status` (`running`,
  `completed`), `agent_type`, `started_at` and `description`, finished ones included.
- `StopFailure` has `error` (`server_error`), `error_details` and a `last_assistant_message` that is the error as
  shown (`[API Error: 500 stub failure]`). It came about 90 s after the prompt, once Qwen Code's own retries had run
  out; the title showed the turn running throughout. The screen then shows the error and the ordinary input box.
- `SubagentStart` (`agent_id`, `agent_type`) and `SubagentStop` (those, `agent_transcript_path`,
  `last_assistant_message`, and the session's `background_tasks`, which still list the stopping agent as running). An
  agent runs in the background: its `PostToolUse` says so at once, the turn's `Stop` lists it, and its completion
  starts a turn of Qwen Code's own that **fires no `UserPromptSubmit`**, only a `Stop` under the next `prompt_id`.
- A turn cancelled with Esc or Ctrl-C fires no hook but the `idle_prompt` notification.
- `SessionEnd` (`reason` `prompt_input_exit`) at exit.

### Terminal title

`Qwen - <folder>` unless the user names it otherwise (the `CLI_TITLE` variable replaces it, read from the code),
padded with spaces to 80 columns, and no conversation was seen to change it; it starts with `◐` while a
turn runs and `✳` while a permission prompt or a question waits on the user, each followed by U+FE0E. Inside a turn it
goes back to the plain form between model calls, for 0.1 s to 0.6 s as measured. It is plain again within the second
of a cancel and of a declined prompt, which is how Octoboard closes both. `ui.hideWindowTitle` and
`ui.showStatusInTitle` switch the title and its glyph off, so the launch pins them.

### Program status reports

Qwen Code reports its state through OSC 7501 (`ESC ] 7501 ; state=<state>:app=qwen-code[:…] BEL`), with no support
query answered first: the PTY of these probes answers none. Each report is written right before the title that shows
the same thing, in 0.28.0 and 0.28.3 alike:

| Report | When | Title |
|---|---|---|
| `state=idle` | at its first screen; when a turn is **cancelled** (Esc and Ctrl-C alike); after the trust dialog is declined | plain |
| `state=working` | when a prompt is submitted, and again at each model call of the turn | `◐` |
| `state=done` | when a turn ends, **between the model calls of one turn** (for as long as the title is plain), and when a permission prompt or a question is declined | plain |
| `state=blocked:kind=permission:…:msg=<base64>` | at a permission prompt (`Permission needed: Shell`), and at the folder-trust dialog (`Approval needed`) | `✳` |
| `state=blocked:kind=question:…:msg=<base64>` | at `ask_user_question` (`Waiting for your answer`) | `✳` |
| `state=error:…:msg=<base64>` | when a turn fails (`Request failed`), once its retries have run out | plain |
| `state=clear` | at exit, and when it starts over after the trust dialog is accepted | |

No report names a turn, carries the final message or tells a subagent from the session. Octoboard's terminal takes the
reports out of what clients receive and, from the first one, reads the running turn and the return to the prompt from
them instead of from the title's glyphs (`crate::session::title_activity`): `working` is a turn running and every other
state is back at the prompt. A user's own `UserPromptSubmit` hook that takes two seconds does not widen the plain window
between two model calls: the hook runs while the report still says `working`, and the window stayed 0.1 s. Since Qwen
Code reports at its first screen, that is what happens in every session, and the provider's reading of `◐` is only what
stands behind it. The session's status is read from the hooks, and Qwen Code's provider declares what the reports
say beside them (`docs/product/program-status-reports.md`, "What reports can say beside an agent's hooks"): a
`blocked` report is its wait on the user, at a permission prompt, a question and the folder-trust dialog alike, and
the report after it the wait's end; `idle` straight after `working` is a cancelled turn, closed at once. The `✳` of
the title is read for the wait only where no report has been made. A declined prompt is reported `done`, which is
also the gap between two model calls, so it is still closed by the watch that waits for the terminal to have stayed
at the prompt for a second.

In the macOS development-daemon smoke runs on 0.28.3 and 0.28.4, the hook and question screen agreed while OSC 7501
reported the wait. The daemon published a structured question and the selected option reached the stand-in through
the control interface.

### Screens and keys

- **Input box**: the rows between two full-width rules, the first after `>` (`*` in YOLO mode). Empty, it shows
  `Type your message or @path/to/file` in grey, not faint, so the hint is still there on a screen with faint cells
  blanked and Qwen Code's reader gives a box that shows only the hint as empty itself: read as a draft, it held every
  message Octoboard wrote back for 30 s. A paste of 30 lines shows as `[Pasted Content 379 chars]`;
  a two-line paste shows its lines. A bracketed paste followed by Enter submits the text as pasted.
- **Follow-up suggestions** (`ui.enableFollowupSuggestions`, on by default) put a suggested next prompt in the empty
  box in the placeholder's grey, which reads as text typed there, and cost a model request a turn; the launch turns
  them off.
- **Folder trust** is off by default (`security.folderTrust.enabled`). Turned on, a folder not yet trusted gets "Do
  you trust this folder?" with `1. Trust folder (<name>)` under the cursor (`›`), `2. Trust parent folder (<name>)`
  and `3. Don't trust (esc)`. Enter records `TRUST_FOLDER` for the folder in `<Qwen home>/trustedFolders.json` and
  Qwen Code starts over in place; Esc records `DO_NOT_TRUST` and goes on. A `-i` prompt waits for the answer.
  The input box is never drawn before the dialog, nor between its acceptance and the fresh start: the dialog goes
  about 1.5 s after the Enter, "Initializing..." shows for 0.5 s to 1 s, and the box comes up after it; declined, the
  box comes up at once. Once, with three sessions starting at the same moment, an Enter written 9 s after the launch
  left the dialog up; alone, the same Enter was taken every time. That did not happen again on 2026-11-23: three
  sessions started at the same moment, each given its Enter 0.3 s after the dialog was drawn, as Octoboard's press
  does, all took it, in two rounds. With the machine busy the dialog stayed up 6 s to 7 s after the Enter before it
  went (the trust already being recorded), and "Initializing..." then lasted over 15 s; an Enter is queued, not
  lost, and a second one would be typed into whatever comes next. Octoboard's press waits 30 s for the dialog to
  go and presses once.
- **Trusting a folder for one launch, with nothing recorded** (2026-11-23, folder trust on in the account's
  settings, a folder not trusted): with `QWEN_CODE_TRUSTED_FOLDERS_PATH` naming a file that holds
  `{"<folder>": "TRUST_FOLDER"}`, no dialog came up and the account's `trustedFolders.json` was never created; and
  with `security.folderTrust.enabled: false` in the system settings file, the same. The first trusts one folder and
  replaces the user's trust file for the launch, the second turns the feature off for it.
- **Interrupting**: Esc stops a turn only while the input box is empty, and with the model having shown nothing puts
  the prompt back in the box; with text in the box Esc does nothing. Ctrl-C stops a turn and puts nothing back, and
  with text in the box clears the text instead. Ctrl-C at the prompt asks to be pressed again and then exits. Ctrl-U
  deletes back to the start of the line, and at its start the line break before it, idle and mid-turn alike.
- While a turn runs the box takes text: Enter "steers" the running turn with it and Ctrl+Q queues it. Either way the
  message is shown above the box, the status row reads `⏳ 1 queued`, and it is sent with the turn's next model
  call. **Cancelled before that, a message still held is never sent** (2026-11-23, a model call left hanging): Esc
  puts it back in the input box, as text followed by a space and a zero-width space (U+200B) where the cursor is,
  and Ctrl-C drops it. **A held message the turn takes is reported as a prompt sent, with its text**
  (2026-11-23, a tool call of 8 s, the message submitted while it ran; `qwen_hooks_steered.jsonl`,
  `qwen_hooks_queued.jsonl`): steered with Enter, the model call after the tool takes it and fires
  `UserPromptSubmit` with the message as `prompt`, under the running turn's `prompt_id`, in place of the empty one;
  queued with Ctrl+Q, the turn goes on without it (the empty `UserPromptSubmit`, then `Stop`) and the message then
  runs as a turn of its own, whose `UserPromptSubmit` carries it. So Qwen Code's screen reader declares that a
  cancel can drop held messages, those no prompt hook reported taken being the ones written again: a cancel the
  status reports declare (`idle` after `working`) has them queued again, held for after the next message. A launch
  with its hooks switched off has none written again, having no prompt reports. The caller of `interrupt_session`
  is told both, and to send again only what the session neither acted on nor shows queued (`get_session`'s
  `queued_messages`). The input box is read without the cursor's mark. Qwen Code's
  screen reader declares that a cancel gives held messages back, for the user's Esc: a box that then shows only the
  message Octoboard wrote last is cleared out of the way of the next message, not sent with it. After a Ctrl-C the
  box is empty and there is nothing to clear, so the interrupt, whose key is Ctrl-C, reads no box for text put back.
- A feedback question ("How is Qwen doing this session? (optional)", "1: Good 2: Bad 3: Fine 0: Dismiss") can appear
  above the box after a turn (`ui.enableUserFeedback`). **It takes a digit typed next as its answer**: a `1` typed
  into the empty box answered it and left the box empty, so a message starting with a digit would lose it. The launch
  turns the question off. The question is shown at random (one finished turn in four, read from the code), only
  after five prompts or more than ten tool calls in the session, with usage statistics on, and not again for 24
  hours, so it was forced: in a scratch copy of 0.28.3 with the random draw and the session minimum taken out of
  `chunks/startInteractiveUI-*.js`, the installed one untouched (2026-11-23). There the question came up after the
  first turn with `ui.enableUserFeedback` on and with it unset (bare mode), and **did not come up with it `false`**,
  the same copy and the same turn. **A paste is not taken by it**: `0 pasted text` written as a bracketed paste
  while the question was up went into the input box whole, digit included, and the question went away unanswered.
  Octoboard writes its messages as pastes, so a message loses nothing to the question even where the setting does
  not reach (bare mode); only a digit the user types is taken.

### MCP inventory and compatibility

- On 0.28.3, `qwen mcp list` reported defaults, account, workspace and system servers, and used the system entry
  for a duplicate name across layers. It tests connections, including running stdio servers, and `--json` exits
  1 as an unknown argument. In a scratch home the listing also migrated a legacy `hideWindowTitle` into `ui` on
  disk. Octoboard reads account-level `mcpServers` from system defaults, account settings and system settings,
  in that precedence order, with the highest definition winning by name. An account inventory has no workspace.
  It lists configured servers from these settings and reports runtime state as unknown. It leaves extension
  contributions to Qwen Code's own listing and publishes remote origins without user info, paths or queries.
- With distinct marker files in a scratch project, the default request included `QWEN.md` and `AGENTS.md`.
  A custom `context.fileName: "GUIDE.md"` selected that file. A per-launch system settings file with
  `["GUIDE.md", "CLAUDE.md"]` included both markers, while another test adding `GEMINI.md` found its marker too.
  The Claude Code instructions switch adds `CLAUDE.md` to the account's effective operator list for that launch,
  defaulting to `QWEN.md` and `AGENTS.md`. The system layer takes precedence over a workspace filename list, which
  Qwen applies only for a trusted workspace. No workspace configuration is moved into that layer.
- On 0.28.3 a project skill in `.agents/skills` appeared in the model request. Controls in `.claude/skills`
  and `.codex/skills` did not appear. The instruction switch controls filename discovery and uses Qwen Code's
  ordinary directory traversal. Qwen Code has a separate MCP import command for other agents' configurations.

### Default configuration sharing

With 0.28.3, a scratch account linked to a second scratch home's `projects`, `skills` and `QWEN.md` used the
linked user instructions and skill in its model request and made the new chat under the shared projects directory.
A follow-up on the npm CLI reporting 0.28.0 resumed a chat created under the default scratch home from the
sharing home. The local model stand-in received the original prompt, linked instructions and linked skill on
resume. Octoboard shares those three entries. The account retains `settings.json`, `.env` and
`subscription_credentials.json`. Settings can hold API keys in `env`, and extension directories can hold
extension-local `.env` files.
A local extension installation writes its source path into
`extensions/<extension>/.qwen-extension-install.json` (`source`), and its enablement into
`extension-store/state.json` (version 2, extension entry keyed by an installation identifier). Installing from a
source under the sharing account's directory left that path in the default home's linked install metadata.
**Extension sharing is not offered.** In a 0.28.0 scratch account with linked `extensions/`,
`qwen extensions settings set` created the default directory's `extensions/<name>/.env` through the link.
An extension can acquire this file after sharing begins, and the whole-entry sharing rule cannot exclude it.
Octoboard keeps `extensions/` and `extension-store/` account-local. The user accepted this outcome on
2026-11-25.

### Sign-in

`qwen auth` is removed; a method is chosen inside the TUI with `/auth` and kept as `security.auth.selectedType` in the
account's `settings.json`.

**A key, as `/auth` records it** (0.28.3, "Custom Provider" with a key): `security.auth.selectedType` is the
provider's method (`anthropic` for an Anthropic key) and there is **no `security.auth.apiKey`**. The key is in the
settings' `env` object under a name `/auth` makes up (`QWEN_CUSTOM_API_KEY_<the base URL in capitals>_<hash>`);
`modelProviders.<method>` lists each model as `{id, name, baseUrl, envKey}` with that name as `envKey`; and `model` is
`{name, baseUrl}`, the chosen model. The banner then reads `API Key | <model>`. No credentials file is written.

**How a key is looked for** (read from the code): the entry under `modelProviders` with the chosen model's id, and its
base URL where several have the id, names the variable (`envKey`), else the method's own does (`OPENAI_API_KEY` for
`openai` and `openai-responses`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, `GOOGLE_API_KEY` for `vertex-ai`). The variable
is read from the process environment, into which the settings' `env` and the `.env` files of the Qwen home, the home
directory and the project are loaded. `security.auth.apiKey` is looked at only when no entry names a variable.

**The `.env` files** (2026-11-23; the order read from the code, each place run): walking up from the working
directory, the first of `<directory>/.qwen/.env` and `<directory>/.env` that exists, in a folder that is trusted;
then `<Qwen home>/.env`, `~/.qwen/.env` where the Qwen home is another directory, and `~/.env`. A variable the
environment already has is kept unless it is empty, the files are read in that order, and the settings' `env` comes
last. A file is parsed by `dotenv` 16 (its line pattern is in the bundle): `NAME=value`, or `NAME: value` with white
space after the colon, after an optional `export`; the last line for a name wins within a file; a quoted value may
run over line ends; an unquoted one stops at the first `#`. Octoboard reads the three files that belong to the
account the same way, for the one variable the key is read from; it does not expand the escapes of a double-quoted
value, which changes no value from empty to set. With the
key taken out of the settings and put in the Qwen home's `.env`, in `~/.env`, in the project's `.env` or in the
project's `.qwen/.env`, the session came up signed in (`API Key | <model>`) each time and ran a turn; with the key
nowhere it came up at "Connect a Provider".

So Octoboard reads an account as signed in when that variable is set in the settings' `env`, in the shell
environment, or in one of the three `.env` files that belong to the account whatever the project (the account
directory's, `~/.qwen`'s, the home directory's), or, with no entry naming one, when `security.auth.apiKey` is set;
with a method chosen and none of these it says it cannot tell, since a project's `.env` file may hold the key and
an account's reading has no project. No method chosen is signed out, and so is a
`settings.json` that is not there; one that is there and cannot be read as settings cannot be told, and neither can
a subscription whose credentials file is there and cannot be read.

Qwen Code decides from the four layers merged (system defaults, the account's, the project's, system), so an
administrator can choose the method and hold the key in the system or the system defaults file. Octoboard reads the
account's file only and does not merge: where the account chose no method and one of those two files has a
`security` section, or cannot be read, it says it cannot tell rather than signed out.

Read from the code only: `qwen-oauth` is refused as discontinued, and `qwen-subscription` keeps its tokens in
`<Qwen home>/subscription_credentials.json` (`tokens.access_token`, with `baseUrl` and `clientId`).

**Without a terminal** (0.28.3, scratch accounts): `qwen auth login` exited 0 with the removal notice, and
`qwen -p "/auth"` said configuration is interactive. Environment variables allow an externally provisioned
API key to be used without a terminal. ACP advertised the `openai` method. Its `authenticate` request failed
(`-32603`) without a provisioned key, and succeeded with a fake key already in the environment, writing
`security.auth.selectedType` into account settings. The request supplies no key and Qwen Code's implementation
clears the cached credential file during authentication.

Qwen Code's own Web Shell also provides API-key onboarding. With `qwen serve --hostname 127.0.0.1 --port 0
--web --require-auth --no-open --no-token-qr` in an unsigned scratch account, it announced the allocated port and
served the Web Shell. The auth-provider catalogue returned 401 without the bearer authentication and 200 with it
(15 providers), and its installation endpoint validated the request and rejected an empty one without saving
credentials. The shell consumes the temporary listener credential through the URL fragment. Octoboard's login
opens that native page and waits for Qwen Code to save a usable sign-in for the selected account. The key is entered
in the Web Shell. In the 0.28.4 acceptance run, the owner opened the authenticated page and the provider
catalogue was readable. The user waived the owner's key submission and the resulting sign-in reading. Tests
cover reading a saved key.

### Models, install and update

No command lists models. The models an account can choose are the entries of `modelProviders` in its `settings.json`;
`<Qwen home>/model-registry.json` is a copy of a public table of model limits, not the account's models. The npm
package is `@qwen-code/qwen-code`; `qwen update` checks for an update and installs it.

**It updates itself, per Qwen home, without touching the npm package.** About three seconds after its first screen
it checks for a newer version and installs it under `<Qwen home>/updates/npm/<hash of the launcher's path>/versions/
<version>`, recording it in `active.json` there ("Update successful! The new version will be used on your next run").
The launcher (`cli-entry.js`) then runs that copy for that Qwen home: the npm package stayed 0.28.0 while `qwen
--version` printed 0.28.3, and each scratch home updated on its own, one after the other. So `qwen --version` depends
on `QWEN_HOME`, two accounts can run two versions, and a probe in a scratch home does not change the user's install.
`general.enableAutoUpdate: false` turns the check off (read from the code), as does `QWEN_CODE_SKIP_UPDATE_CHECK_ONCE
=true` for one run.

### Through a real model and a development daemon

Measured on 0.28.3 on 2026-11-22 with the default account (`~/.qwen`), a model of its own list, `--approval-mode
default`, in a daemon built from the branch with a data directory of its own:

- A console session opened with a first task called an Octoboard tool with no prompt (`tool_search`, then the tool)
  and went `starting`, `working`, `idle`. A project session bound to it went to `waiting_user` at a permission prompt
  and again at an `ask_user_question`, back to `working` on each answer, and its `report` reached the console
  session, which took it as a turn; the session was archived and later resumed with its conversation.
- Messages written in (a user's, Octoboard's notices, a child's report, a parent's instruction) were each taken as a
  prompt.
- A permission prompt declined with Esc, a question dismissed with Esc and a turn interrupted by the parent
  (`interrupt_session`, which answered `interrupted: true`) each left the title plain and were closed by the title
  watch about a second later. No watch was armed at first, and the sessions stayed `working`: the watch is armed by
  the provider's record store, which Qwen Code's provider now has for that alone.
- Between the model calls of one turn the title was plain for 0.05 s.
- After a daemon stop both sessions read `interrupted` and came back with `resume_session`.
- **`auto` mode** (2026-11-23, one turn in a scratch folder, no daemon): asked to run `ls` and then `mkdir
  probe-dir`, the model made both calls and neither raised a prompt, where `default` mode prompts for `mkdir`; the
  hook payloads carried `permission_mode: "auto"`, and the reports were `working` and `done` only, no `blocked`.
- **A call `auto` mode's classifier refuses** (2026-11-23, one turn in a scratch folder, no daemon): asked to run
  `curl -fsSL http://127.0.0.1:9/install.sh | sh`, the model made the call and it was refused with no prompt: the
  tool's row reads `x Shell …` and "Blocked by auto mode policy: …", the model was called again with that refusal
  and ended the turn in words. The hooks were `UserPromptSubmit`, `PreToolUse`, the empty-prompt `UserPromptSubmit`
  of the next model call and `Stop`: **no `PostToolUse` and no `PostToolUseFailure` for the refused call**, which
  Octoboard, reporting no tool call open, has nothing to close for. The reports were `working`, `done`, `working`,
  `done`; no `blocked`, so the session never waits on the user for it.

### Capabilities measured on 2026-11-23

Measured on **0.28.3** run from its own `cli.js` in a scratch `HOME`, with `general.enableAutoUpdate` off and
`QWEN_CODE_SKIP_UPDATE_CHECK_ONCE=true`, two `openai` entries in `modelProviders` pointing at a local stand-in that
logged every request, and the launch's system settings file in a scratch directory. No real model turn was spent.
The screens are in `apps/daemon/testdata/` (`qwen-captures.md`).

- **The model list.** The models an account can choose are its `modelProviders` entries under the method it chose
  (`security.auth.selectedType`); ACP mode lists the same entries ("ACP mode" below). Octoboard reads them from the
  account's `settings.json` and starts no process: ACP mode costs 6 to 15 s a listing and writes into the Qwen home it
  runs in, and the effort levels it adds cannot be passed, Qwen Code having no effort flag. A model is offered with
  no effort level. The model the settings name (`model.name`) is read from the system settings file, else the
  project's `.qwen/settings.json`, else the account's.
- **How the layers' models merge.** Qwen Code merges its settings from the system defaults, the user's
  `settings.json`, the project's `.qwen/settings.json` (where the folder is trusted) and the system settings file,
  later over earlier (0.28.3's `mergeSettings`); `modelProviders` has no merge rule of its own, so a later layer's
  list of a method replaces an earlier one's whole and the other methods are kept. Measured: a system file listing
  `openai` (`sys-model`) and `anthropic` (`sys-claude`) entries over a user file listing `openai` (`stub-model`) and
  `gemini` (`user-gemini`) ones, `/model` offered `sys-model`, `sys-claude` and `user-gemini`. **The picker lists
  every method's entries, but `--model` takes only the chosen method's**: with `openai` chosen, `--model sys-claude`
  ran the first `openai` entry. So Octoboard lists the chosen method's entries of the system defaults, the account
  and the system file merged so; the project's layer is not read, the catalogue not being given the session's
  folder, so models a project lists in its own `.qwen/settings.json` are missing from the list.
- **A signed-in account** (2026-11-23, read only, no model turn): its `settings.json`, read through the catalogue's
  rules, gave the entries listed under its chosen method, and Qwen Code's own `/model`, run in a scratch home holding a
  copy of that file (deleted after), offered the same entries (`[<method>] <id>`).
- **A model per session.** With `model.name` `stub-a` in the settings, `--model stub-b` made every request of the
  session name `stub-b`.
- **The cross-project guard.** A `PreToolUse` command hook that prints `{"hookSpecificOutput": {"hookEventName":
  "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": "<reason>"}}` refuses the call with no
  prompt, in `default` and in `yolo` mode alike: the call's row reads `x WriteFile …` with the reason under it, and the
  model is called again with the refusal. (`decision: "deny"` / `"block"` is read the same, by the code.) The matcher
  is a regular expression over the tool's name; the file-writing tools are `edit` and `write_file` (`file_path`) and
  `notebook_edit` (`notebook_path`). The guard is registered in the launch's system settings file for
  `^(edit|write_file|notebook_edit)$`, and, as the status hook is, made to decide only for the launch's own Qwen Code:
  the guard is given the same mark (`octoboardd guard-hook --fired-by <file>`) and decides only where the Qwen Code
  above the shell has it on its command line, or where that cannot be told. With the mark on Qwen Code's command line
  the write was refused; launched without it, the same write went through (measured with a shell wrapper reading
  `ps -ww -o args= -p $PPID`, which the mode's check replaced).
- **Its own memory.** Qwen Code keeps a memory of its own, on by default (`memory.enableManagedAutoMemory`): after a
  turn it makes a second model request, as a "managed memory extraction subagent", which writes
  `<Qwen home>/memories/MEMORY.md` (the user's scope) and `<Qwen home>/projects/<project>/memory/MEMORY.md` with
  `meta.json` and `extract-cursor.json` beside it, and every request carries an `# auto memory` section with the
  index. `memory.enableManagedAutoDream` (on by default) consolidates it. With both `false` in the launch's system
  settings file, which outranks the user's, the section was gone from the request, no extraction request was made and
  none of those files was written. So one launch can switch it off, and Octoboard's provider declares it switchable:
  a launch while Octoboard's memory is on writes both settings `false`.
- **Exiting ends its commands' process groups.** A command's shell (`bash -c`) leads a process group of its own, a
  child of Qwen Code. A background command (`(sleep 3041 &); sleep 3042`, `is_background`) was gone with its orphan
  (same group, parent 1) within a second of `SIGTERM` to Qwen Code, and a background `sleep` likewise at `/quit`; the
  groups are not the terminal's foreground group, so it is Qwen Code that ends them. A plain `sleep` in the foreground
  is refused by Qwen Code itself ("Blocked: standalone sleep …").
- **Images.** A path of an image file pasted on its own into the box is attached, not typed: the box stays empty and
  an `Attachments: [clipboard-<n>-0.png]` row is drawn above it, one `[…]` per image, with `↑ to manage attachments`
  under the box. The image is copied to `<Qwen home>/tmp/clipboard/` and sent with the next message as an `@` reference
  (a model the stand-in did not declare as taking images was given "[Unsupported image file …]" in its place).
  Neither Ctrl-U nor Esc takes an attachment off, and Enter with nothing written beside the attachments sends
  nothing: they go with the next message. The prompt `UserPromptSubmit` reports for such a message is `@<path>
  [@<path> …]`, a blank line, the text, then `--- Content from referenced files ---`, what was read of each file and
  `--- End of content ---`; the content block follows any prompt that names a file with `@`. So Qwen Code's screen
  reader gives the `Attachments:` row as the box's first row, counted as the images and left out of the text beside
  them, declares `message_images`, and takes a reported prompt to be a message written when it is the text without
  the content block, or without that and the line of references.
- **Slash commands.** A `/word` in the box draws a completion list under it, the highlighted row marked `>`
  (`> stats [model|…]  Show usage statistics dashboard.` for `/sta`), which Enter runs. **A space before the slash
  does not stop it**: ` /quit now` quit the session. A zero-width space (U+200B) before it was sent to the model as
  text (`\u200b/stats probe0`), so that is the guard a message's leading slash gets. Ctrl-U clears the box. The
  guard stays in the prompt `UserPromptSubmit` reports (`\u200b/stats probe guarded`), so a reported prompt is
  compared with the message written with invisible characters ignored on both sides.
- **Version and updates.** `qwen --version` prints the version the Qwen home it runs with uses: the launcher
  (`cli-entry.js`) runs `<Qwen home>/updates/npm/<first 16 hex digits of the SHA-256 of the launcher's real path>/
  versions/<version>/…/cli.js` where `active.json` there names a version whose `bootstrap` is this launcher, whose
  `baseVersion` is the npm package's version and whose `bootstrapCtimeMs` is the launcher's change time; anything else
  runs the npm package itself (read from 0.28.3's `cli-entry.js`). So an npm update of the package, which changes its
  version, makes every home run the new package until it updates itself again, and the version Octoboard asks of the
  CLI in the default account's environment is the one that account runs. Another account's home may run another
  version. So the version is read per account from these files, as the launcher reads them
  (`AgentProvider::account_version`): the active update's where `active.json` matches, else the npm package's
  `package.json`. The key was measured as computed (`3fe0cacc706db533` for this machine's launcher) and
  `bootstrapCtimeMs` is the change time as Node gives it, seconds × 1000 plus nanoseconds / 10⁶ in double precision
  (`1795100767270.9033` for a change time of `1795100767.270903249` s). A launch reads its account's version; the CLI
  status shows the default account's. The change time is read as Node reads it on every host: on Windows from the
  file's basic information (`ChangeTime`, as libuv turns it into `ctimeMs`), not from its last write time, and
  `bootstrapCtimeMs` is compared exactly, read from the number's own text. Where an `active.json` matches the launcher
  in all else and the change time cannot be read, the files tell nothing and the CLI is asked for `--version`.

### Answering its prompts

Measured on 0.28.3 as above, `--approval-mode default`. A permission prompt and an `ask_user_question` form are each
drawn in place of the input box, from the call's row, which starts `?` and ends with `←`, to the row
`Waiting for user confirmation...`.

- **Permission prompt.** `? WriteFile Writing to ok.txt ←` (a new file's lines under it), `? Edit a.txt ←` (the diff),
  `? NotebookEdit replace notebook cell c1 in nb.ipynb ←` (the notebook's diff), `? Shell mkdir probe-dir (Make a
  directory) ←` (a blank row, the command's rows, one a line, then `Allow execution of: 'mkdir'?`; a command of
  several lines wraps the call's row too), `? WebFetch Fetching content from <url> (format: auto) and processing with
  prompt: "…" ←` (then `URLs to fetch:` and `- <url>` on a row of its own), and `? nap (octoboard MCP Server) {…} ←`
  for an MCP tool of a server not trusted (`MCP Server: octoboard`, `Tool: nap`). The choices are `N. label` rows, the
  cursor's marked `›`: for a file write, an edit or a notebook cell `1. Yes, allow once`, `2. Yes, allow always`,
  `3. No, suggest changes (esc)`; for a command `1. Yes, allow once`, `2. Always allow run 'mkdir *' commands in this
  project`, `3. Always allow run 'mkdir *' commands for this user`, `4. No, suggest changes (esc)`; for a fetch the
  same with `Always allow fetch from 127.0.0.1 …`, for an MCP tool with `Always allow mcp__octoboard__nap …`. **A digit
  takes its choice at once**: `1` wrote the file, `3` declined the edit. "No, suggest changes", by its digit or by
  Enter on it, declines the call and ends the turn, with no text field and no model call.
- **"Yes, allow always" of a file change is per launch** (2026-11-23): `2` wrote the file and switched the session to
  accepting file changes (the footer read `auto-accept edits (shift + tab to cycle)`); the next `write_file` and
  `edit` ran with no prompt, a command still asked, the user's and the project's settings files were not written
  (no project `.qwen/` was made; the user's file gained only the `$version` every start writes), and a new launch in
  the same home and folder was back to `Ask permissions`. The "Always allow …" rows record a rule in the project's or
  the user's settings, as they say. Every choice that allows is offered from the window.
- **Which tools ask** (read from 0.28.3's code): besides those above, a web search, the creation of a task or a
  scheduled job, a skill, a subagent, image generation, a read outside the workspace and a few others can ask in
  `default` mode; their prompts were not drawn here (a web search's tool was not registered without a search
  provider). They are matched by a text of the call's input drawn as a whole row, or after `: `, as Claude Code's
  are, and only on a prompt not drawn for a tool measured above (its call's row naming `Shell`, `WriteFile`, `Edit`,
  `NotebookEdit`, `WebFetch` or `AskUserQuestion`, or an MCP tool's `(<server> MCP Server)`): such a prompt is that
  tool's call's alone, also where the call failed to match it.
- **A scrollbar beside the prompt.** While the history scrolls, Qwen Code draws a scrollbar in the last column of
  the rows above `Waiting for user confirmation...` (`█` on its thumb, `│` on its track; `ui.showScrollbar`, on by
  default, "auto-hiding"); it was there 1.2 s after a prompt came up and gone 5 s after the screen settled. A launch
  turns it off in its system settings (`ui.showScrollbar: false`), and the reader still takes such a cell off each
  row. Measured on 0.28.3 (2026-11-24, a scratch `HOME` against a stand-in, a PTY at 120x40, a 120-line answer and
  then a shell call's prompt): with `ui.showScrollbar: true` in the system settings file 38 rows ended in `█` or `│`
  1.2 s after the prompt came up; with `false` none did, then or 4 s later. The same file with an unknown `ui` key
  beside it was taken without a word (`-p` ran its turn, status 0); an earlier version, which may not know the key,
  was not measured.
- **Question form.** With more than one question a tab row (`▸ Colour   Size  Submit`; an answered tab gets `✓`), with
  one only the header; then the question, the options as `N. label` with the description on the row below, `[ ]` /
  `[✓]` before each where more than one may be chosen, the cursor's marked `❯`, and a last `Type something...` row.
  The footer is `↑/↓: Navigate | ←/→: Switch tabs | Enter: Select | Esc: Cancel` (`Space: Toggle | Enter: Confirm` for
  a multiple choice). A digit on a single choice chooses its option and goes on to the next question; on a multiple
  choice it only moves the cursor, Space ticks the row under it and Enter goes on. With the cursor on the last row it
  opens as a field (`> Type something...`) that takes typed or pasted text, which on a multiple choice ticks the row
  beside the others ticked; Enter there goes straight to the `Submit` tab, past any question after it. Down does not
  wrap. After the last of several questions comes `Submit`: `Your answers:` with each (`(not answered)` for one left
  out), `Ready to submit your answers?`, `1. Submit answers`, `2. Cancel`; a single question has none, its digit
  answers it. Esc dismisses the form, which fires nothing but the `idle_prompt` notification.
- So Qwen Code's interaction reader publishes a prompt only when it is drawn as Qwen Code drew it, matched to a call
  its hooks reported (a command by all its rows, a file by the name the call's row shows, a fetch by its address's
  row, an MCP tool by its server and tool rows, another tool by a text of its input), presses a choice only while the
  prompt on the screen is still that call's, and presses the answers with digits, Space, the pasted text, Enter and
  Esc; only the last question takes a written answer, beside the options on a multiple choice and in their place on
  a single one. A launch with its hooks switched off reports no call, so none of its prompts is published, and its
  cross-project guard does not run: its notice says both.

### Plan quota

An account that signs in with a key has no plan quota, and Qwen Code has nothing to read one from (0.28.3's code, and
0.28.0 run on 2026-11-22 in a scratch `QWEN_HOME` with a made-up key and a model endpoint nothing listens on, so no
model was reached):

- **`/usage` is another name of `/stats`**, the local statistics of the session and of the sessions stored in the
  home (`altNames: ["usage"]`; it runs in the TUI, in print mode and over ACP). `qwen -p "/usage"` printed `Session
  duration`, `Prompts`, `API requests`, `Tokens — prompt: …, output: …`, `Tool calls` and `Files` and exited with
  status 0. Those are counts of what this home's sessions did, not a quota: no limit, no remaining share, no reset.
- A used home also holds a `usage/` directory and a `usage_record.jsonl` (seen by name only; not read).
- The words "plan usage" appear only for a subscription login ("Run /usage to see your plan usage" in the
  subscription's welcome text; `/upgrade` and `/logout` are offered only when `security.auth.selectedType` is
  `qwen-subscription` and its session is signed in). What `/usage` shows a subscriber, and where it gets it, was not
  found in the code by searching and not run: it needs a subscription.

### Not measured

Qwen subscription sign-in and quota need a Qwen subscription or Coding Plan account. The
`subscription_credentials.json` record is known from code. On 0.28.3 with a key, `/usage` showed only the local
`/stats` counts. Also unmeasured: the owner's Web Shell key submission and resulting sign-in reading, and other
sign-in routes besides choosing a key through `/auth`. What Esc gives back where more than one message was held,
bare mode's tools and whether it loads the launch's MCP configuration, and the trust dialog's `blocked` report where Octoboard presses the dialog itself: it may raise the session's hand and tell
a bound owner the session waits for the moment before the press, as the title's glyph did before the reports were
read.

### ACP mode

Measured on 2026-11-22 with `qwen --acp`: on 0.28.3 with a signed-in account (`~/.qwen`, an API key), and on
0.28.0 in a scratch `HOME` and `QWEN_HOME`, once with a made-up key and two made-up `modelProviders` entries and once
empty. `QWEN_CODE_SKIP_UPDATE_CHECK_ONCE=true` and `QWEN_CODE_NO_RELAUNCH=true` were set. The probe wrote `initialize` (protocol version 1, no client capability) and then `session/new` (a
scratch working directory, no MCP server) as JSON-RPC lines on stdin, read the answers, sent no prompt, and ended the
input.

- `initialize` was answered after 6 s on the signed-in account and 10 to 14 s in a new home: `agentInfo` (`qwen-code`,
  the version that home runs), one `authMethods` entry and capabilities for loading, resuming and listing sessions.
- `session/new` was answered within a second with `sessionId`, `models`, `modes` and `configOptions`:
  - **`models.availableModels` is the account's `modelProviders`, entry for entry, and no more**: the signed-in account
    listed its own entries and the scratch home its two. Each is `{modelId, name, description, _meta}` where `modelId`
    is `<id>(<auth type>)` (`stub-a(openai)`), `name` the entry's `name`, and `_meta` carries `baseModelId` (the entry's
    `id`, what `--model` takes), `authType`, `baseUrl`, `envKey` (the variable's name, never its value), `contextLimit`
    and `isRuntime`. `currentModelId` is the settings' `model`.
  - **`configOptions` adds what the settings do not say**: `reasoning_effort` (category `thought_level`) with `none`,
    `default`, `low`, `medium`, `high`, `xhigh` and `max`, the same list whatever the model ("Providers map or clamp
    the requested tier for the active model"), beside `model` (the same ids), `mode` (the five approval modes) and
    `workflow_mode`.
  - `modes`: `plan`, `default`, `auto-edit`, `auto`, `yolo`, the current one `auto`.
- An `available_commands_update` followed: 54 slash commands with descriptions.
- **With no method chosen** (the empty home), `initialize` answered the same and `session/new` was refused:
  `{"code": -32000, "message": "Authentication required: Use /auth to sign in or configure a model first."}`.
- **It keeps no record of the session.** In the user's home the run changed three things: an empty directory
  `tmp/<a hash>` was created, the `sessions` directory's time changed with nothing left in it, and the marker
  `.openai-logs-cleanup-…` was rewritten. No `projects/…/chats` file was written. A new home also gets what any first
  run creates (`installation_id`, `model-registry.json`, `output-language.md`, the extension store).
- The process exited by itself 1 to 6 s after its input ended (status 1 after 6 s on the signed-in account, 0 in the
  scratch homes). Nothing was spent: no prompt was sent, and the made-up key and base URL were never used.

## Pi

Unless stated otherwise, measurements used Pi 1.5.2
(`@earendil-works/pi-coding-agent`, on Node 22) on macOS on 2026-11-20, in a scratch home and scratch working
directories, with a local stand-in answering the model requests as a provider of the scratch `models.json`; where a
fact is read from the package's bundled docs or code instead, it says so. What was then measured against a real
account and a real model is under "Through a real model and a development daemon" below.

**Windows 11 ARM64 (23H2, build 22631.6199).** npm's global install put `pi.cmd` in the scratch
prefix. It ran on ARM64 Node 26.13.0 and reported **1.5.2**. In an SSH preflight of the
development daemon, the private PTY relayed the xAI device page and code to the control
client. The same private-PTY flow reported success when run in the interactive desktop
after the user authorized the device in Chrome. One native CLI turn through xAI Grok 4.3
returned the verification word. In the development daemon, the channel helper started from a
scratch path with spaces. With the batch launcher, the helper could poll the daemon and survived
Ctrl-C, but nothing reached Node's standard-output pipe. A loopback fixture isolated this to
the batch launch: starting the helper as an executable delivered the message and returned
its outcome. The extension now starts it directly, detached. The console and project reached
the local model, including a send after raw Ctrl-C with the same helper and a send after archive
and resume. The session records show a successful Octoboard tool read, the child's report received
by the parent, and the parent's instruction received by the child. A single terminal Esc closed
an active stand-in request. Inbox ConPTY did not deliver Pi's OSC turn state, and the daemon
initially read Idle during the request. The provider sends Pi's working, user wait and settling
states through the extension's launch hook on that host. The rebuilt daemon showed Working,
cancelled the active request using its interrupt command and returned to Idle. A follow-up
acknowledged before Escape reached the local model once after the next prompt.

**Linux daemon** (1.5.2, Debian 13 arm64, glibc 2.41). A console and project session completed turns on a
file-based Anthropic account and called the injected Octoboard MCP server. The project received the console's
instruction, and the console's native record includes the project's report. Both sessions interrupted a turn,
archived and relaunched. Pi emitted native OSC 7501 status reports and kept records under `~/.pi/agent/sessions`.
On a follow-up turn with a local stand-in, `strace` recorded the Node extension directly executing the
launch's hook script with its `/bin/sh` shebang.

**Status: OSC 7501.** Pi asks the support query (`OSC 7501 ; ? ST`) as it starts and reports only once it is
answered; unanswered it reports nothing. `PI_PROGRAM_STATUS=1` would report without asking, which Octoboard does not
need, since every session terminal answers. Every report addresses the root record and carries `app=pi`:

| When | Report |
|---|---|
| Started, past the project-trust confirmation | `idle` |
| A prompt submitted (the positional one of the launch included) | `working` |
| The run finished, a tool call's included | `done` |
| The model request refused (HTTP 400) | `error`, `msg` the first line of the error |
| Esc during a run | `idle` |
| `/name` | the current state again, `msg` the session name |
| `/new` | `idle` |
| Quit (Ctrl-D) | `clear` |

`blocked` is reported while an extension's dialog waits (1.5.2, 2026-11-23, a scratch extension against a stand-in
model): `kind=permission` for `ctx.ui.confirm()`, `kind=question` for `ctx.ui.select()` and `ctx.ui.input()`, each
with `msg` the dialog's title. The answer, or Esc, is followed by the state before it: `idle` for a dialog a slash
command raised outside a run, `working` for one raised from a `tool_call` handler mid-run, which then went on to
`done`. Octoboard's program status path reads these as they are: *waiting for the user* and back. On Pi 1.5.2 on macOS, selecting OAuth after `/login xai` emitted
`blocked` with `kind=auth` while the device authorization waited. Pressing Escape returned to `idle` without
authorizing the device. Pi's own selectors (`/model`, `/resume`, `/settings`, `/tree`, `/fork`, `/login`'s menu, `/mcp`) report nothing. Nothing is
reported while the project-trust confirmation is up, nor for a `!` shell command the user runs.

A named session's reports carry the session name as `msg`: `/name` repeats the current state with it, and `working`
and `done` carry it from then on, while `error` carries the first line of the error and `idle` nothing (1.5.2,
2026-11-23, a session named `Fix - the build` against a stand-in model). No report carries what the model said. Pi
reports `done` or `error` as it handles its own `agent_settled` event, once a run is over and it will not run again by
itself (an error it retries, a message that was waiting); a run cancelled with Esc settles with `aborted` set and is
reported `idle`. Octoboard would take the `msg` of a `done` as what the turn said, so Pi's provider supplies the turn's
message itself: the extension keeps the text of the last assistant message of each `agent_end` (the error message
where a run ended on an error with no text), and forwards it at `agent_settled`, `null` where there is none and
nothing for a cancelled run, whose text it drops: a settling that follows no run (a manual `/compact`) would carry
it otherwise. The text is cut at 200,000 UTF-16 units, one short where that would split a surrogate pair, whose lone
half `JSON.stringify` writes as an escape the daemon's parser refuses the whole payload for. Measured, the forwarded event reached the hook script 30 to 250 ms after the `done`
report was written, inside the two seconds a turn end waits for it. A turn end that is given no message (a last
assistant message with no text, a manual `/compact`, which reports `working` and `done` with no run, or a hook that
fails) ends with none: Pi's provider answers that its report's `msg` is not the turn's message
(`HookInterpreter::report_msg_is_turn_message`), so the session name is never told as what a turn said. Not seen in a
development daemon.

**No hooks; an extension instead.** `--extension <file>` loads a JavaScript or TypeScript module for the run, before
the project-trust confirmation and without a trust decision of its own, and lists it under `[Extensions]` in the
start-up banner. An ES module with a default export loads from a `.js` file. Octoboard's extension
(`apps/daemon/src/adapter/pi/extension.js`) does the things listed below, beside the turn's message above and what
"Prompts, the cross-project guard, images and slash commands" describes. A factory
that throws while Pi loads the extension ends Pi (`Failed to load extension`), and the action methods (`getAllTools`,
`getActiveTools`) throw until the extension is bound, so the extension catches everything and reads nothing of Pi's
while loading. Pi runs the factory again for the session `/new` opens, after `session_shutdown` for the one before
(measured); the bundled `extensions.md` says the same of `/resume`, `/fork`, `/clone` and `/reload` (Pi "emits
`session_shutdown` for the old extension instance, reloads and rebinds extensions for the new session, then emits
`session_start`"), read from the doc and not measured. So what the extension keeps in memory lasts one session, and
nothing a `session_shutdown` handler ended has to be started again by the same instance:

- `pi.registerMcpServer("octoboard", { command, args, exposure: "direct", timeout })`. With `direct` exposure the
  tools are declared to the model beside the built-in ones as `mcp__octoboard__<tool>` (the default exposure,
  `codemode`, declares none of them); the first prompt waits for a `direct` server to connect. `timeout` is in seconds
  per request (Pi's default is 60): a call that outlasts it comes back to the model as the tool result
  `MCP request timed out after <ms>ms`, and the turn goes on. A server of the same name in the user's `mcp.json` takes
  precedence over a registered one (bundled `extensions.md`), measured on 1.5.2 on 2026-11-23 with stub servers that
  log their own start: with `{"mcpServers": {"octoboard": …}}` in the configuration directory's `mcp.json`, only the
  user's server was started, the registered one never, the model was declared the user's tools as
  `mcp__octoboard__<tool>`, and `/mcp` said `overridden: "octoboard" registered by <extension> is overridden by
  "octoboard" in <mcp.json>`. Names that differ only in `-` and `_` count as the same (bundled `mcp.md`), and a
  trusted project's `.pi/mcp.json` is read the same way. No flag or variable of 1.5.2 changes the precedence
  (`--no-mcp` turns every server off). So where the configuration directory's `mcp.json` or the working directory's
  `.pi/mcp.json` names such a server, the extension registers nothing with Pi's MCP support: it starts the session's
  server itself at `session_start`, speaks MCP to it over its stdin and stdout, and registers each tool with
  `pi.registerTool` under `mcp__octoboard__<tool>`. Measured on 1.5.2 on 2026-11-23 against stub servers and a
  stand-in model:
  - `registerTool` takes the server's JSON Schema as `parameters` unchanged, validates the arguments against it
    (`Validation failed for tool "…": - status: expected one of […]` comes back to the model as the result) and
    declares the tool to the model under exactly the name given. It works while loading and later.
  - A tool an extension registers and a tool of the same name that Pi's MCP support registers for the user's server
    do not clash: Pi keeps one definition a name, the first in extension order (bundled `runner.js`), and the
    launch's `--extension` came before the built-in MCP support, whichever of the two registered first. With the user's server offering `report` and
    `user_only`, the model was declared Octoboard's `mcp__octoboard__report` and the user's
    `mcp__octoboard__user_only`; a call of the first reached Octoboard's server and of the second the user's. So the
    user's own server keeps every tool but one that shares a tool name with Octoboard's.
  - The launch's first prompt is held until an async `session_start` handler returns, but a prompt after `/new` is
    not, so the extension waits for its registration in `before_agent_start`, up to ten seconds, as Pi's MCP support
    waits for its direct servers; the tools were declared in the first request of the launch and of a `/new` session.
  - A tool the server withdraws (`notifications/tools/list_changed`) is registered again as `hidden`, since Pi has no
    way to take a tool back, and one it adds is declared from the next request. Esc during a call aborts the signal
    `execute` is given, on which the extension sends `notifications/cancelled`, as it does for a request that
    outlasts the timeout, but never for `initialize`, which MCP lets no client cancel.
  - Against `octoboardd mcp` itself (0.1.0-beta.5, with no daemon to reach): the handshake at protocol `2025-06-18`,
    all 37 tools registered and declared, and a call's `isError` result came back to the model as its text.
  - Names are compared as Pi compares them, `-` and `_` alike and nothing else: a user server `octo-board` is a
    different server from `octoboard` and both ran. The extension does not check that the project is trusted or the
    entry valid: taking the registration over where Pi would have served the tools costs nothing.
  - Pi does not cut these tools' results as it cuts an MCP server's over 20 KB, and `/mcp` lists no Octoboard server
    in such a session.
- It forwards `session_start`, which fires once start-up is past the trust confirmation (`reason: "startup"`) and again
  when `/new`, `/resume` or `/fork` replaces the session in the same process. A session's file is written at its first
  message, so at `startup`, `new` and `fork` there is none and the extension names no id; at `resume` it names the
  session resumed. Measured on 2026-11-23: after `/fork` (which also puts the forked message's text into the input
  box) and a message, the fork was a file of its own id that no event had named, so a later resume would have opened
  the original. The extension therefore sends `session_start` once more, with the id and `reason: "saved"`, at the
  first `agent_end` after which the session's file exists and its id differs from the one last named: measured, one
  such event each after the first run of the launch's session, of a fork and of a `/new` session, and none after later
  runs.

**Launch and resume.**

- `--session-id <id>` opens the working directory's session of that id and creates it where there is none, silently,
  also when a session of that id exists under another directory. `--session <id>` opens the stored session; for an
  id nothing matches it prints `No session found matching '<id>'` and exits, and for a session of another directory it
  asks `Fork this session into current directory? [y/N]`. It matches partial ids too. So a new session passes
  `--session-id` and a resume `--session`.
- A session's file, `sessions/--<working directory, separators as dashes>--/<timestamp>_<id>.jsonl` under the
  configuration directory, is written at the first message, not at start.
- The file is a tree (1.5.2, headless against a stand-in, 2026-11-22). Its first line is a header (`type: "session"`,
  whose `id` names the conversation); every other line carries its own `id` and `parentId`. Going back to an earlier entry writes
  nothing by itself: the next message is appended with the earlier entry as its `parentId`, the abandoned lines stay,
  and with a summary a `branch_summary` line (`parentId` the entry gone back to, `fromId` the abandoned leaf) comes
  first. A resume carries on from the line written last, which the bundled `session-format.md` and
  `session-manager.js` call the leaf. A fork into another directory is a new file under that directory's folder with
  an id of its own, holding a copy of the branch and `parentSession` in its header: it needs nothing of the original.
- `--model` takes `provider/id`; `--thinking` takes `off`, `minimal`, `low`, `medium`, `high`, `xhigh` and `max`, lowers
  a level the model does not support to the highest it does (`xhigh` and `max` became `high`), and ignores a value it
  does not know.
- `--append-system-prompt` takes a file's path as readily as text: the file is listed under `[Context]` and its
  contents are appended once. Pi builds the system prompt afresh on every start, so the same flag on a resume leaves
  the text in the prompt once, neither doubled nor dropped.
- Positional messages start the first turn and leave the TUI interactive; `--` before them keeps a message that
  starts with `-` from being read as an option. A positional argument that starts with `@` is read as a file to
  include, after `--` too (1.5.2, 2026-11-23): `pi … -- '@nofile explain'` printed `Error: File not found: <cwd>/nofile
  explain` and exited. With a space before it (`' @notes.txt explain'`) it reached the model as that text, space
  included and no file read. Pasted into the input box of a running Pi, `@nofile explain` and `@notes` (a prefix of an
  existing file's name) were sent as typed, with no completion list opened by the paste. Octoboard queues a first task
  that starts with `@` as the session's first message instead of passing it as an argument.
- `--approve` (`-a`) trusts project-local files for the run: in a folder whose plain launch showed the trust
  confirmation, a launch with `--approve` showed none, started, and left no `trust.json` in the configuration
  directory; the next plain launch asked again (1.5.2, 2026-11-23). `--no-approve` ignores them for the run (help
  text).
- A subcommand (`install`, `remove`, `uninstall`, `update`, `list`, `config`, `auth`, `mcp`) is run only as the first
  argument; anywhere else its name is a message, with or without the `--` (1.5.2, measured on macOS on 2026-11-22
  without a terminal, with `PI_OFFLINE=1` and a scratch `PI_CODING_AGENT_DIR` holding no login). `pi list` printed
  `No packages installed.` and `pi mcp` its usage, where `pi --session-id <id> list`, `pi --session-id <id> -- list`,
  `pi -- list` and `pi --session-id <id> -- mcp` all went on to the prompt and stopped at `No API key found for the
  selected model.`, as `pi --session-id <id> -- 'list the files'` did. Octoboard passes its flags first, so a first
  task that is a subcommand's name needs no shaping.
- `PI_CODING_AGENT_DIR` names the configuration directory itself (default `~/.pi/agent`): `auth.json`,
  `models.json`, `settings.json`, `mcp.json`, `trust.json`, `sessions/`.

**Instruction files and compatibility.** Stand-in requests from Pi 1.5.2 included `CLAUDE.md` when it was the
directory's only context file and `AGENTS.md` when both were present. With project resources trusted, a skill from
`.agents/skills` reached the system prompt. The sentinels from `.claude/skills` and `.claude/commands` stayed out of
the request. `--no-context-files` suppressed both context filenames, leaving the `.agents` skill. `--no-approve`
suppressed the local skill and still loaded `AGENTS.md`. Pi 1.5.2 exposes no launch switch for just the Claude
fallback. Octoboard uses the native fallback and offers no compatibility switch for it.

**The project-trust confirmation** appears for a folder holding project resources Pi would load, before anything
else is drawn: measured with `.pi/settings.json` alone, and (2026-11-23) with no `.pi` at all in a folder holding
`AGENTS.md`, `.agents/skills/<name>/SKILL.md` and `.claude/settings.json`, which is what a console session's
directory holds. Of those three it is `.agents/skills` that raises it (1.5.2, 2026-11-23, one scratch folder a case):
a folder holding `.agents/skills/<name>/SKILL.md` alone was asked about, as was one holding an empty `.agents/skills`,
while a folder holding `AGENTS.md` alone, `.claude/settings.json` alone or `.claude/skills/<name>/SKILL.md` alone
started with no confirmation, as one holding none of them did. A `.pi/mcp.json` alone raises it too.

```text
 Trust project folder?
 <folder>
 This allows pi to load .pi settings and resources, install missing project packages, and execute project extensions.
 → Trust
   Trust parent folder (<parent>)
   Trust (this session only)
   Do not trust
   Do not trust (this session only)
 ↑↓ navigate  enter select  escape/ctrl+c cancel
```

The cursor (`→`) starts on `Trust`; Enter accepts and records the folder in `trust.json`, and Down moves the cursor,
redrawing only the two rows it leaves and enters. Bracketed paste is on from Pi's first byte, before this screen.

**The input box and its keys.**

- A bracketed paste followed by Enter submits.
- Esc stops a run, before the model has shown anything and after alike; the prompt is not put back in the input box,
  and the transcript shows `Operation aborted`.
- A message entered during a run waits as a steering message (`Steering: <text>`, one row a message, above the box,
  with a row `↳ Option+Up to edit all queued messages` under them) until the current response and its tool calls are
  done. Esc puts every message still waiting back into the input box, unsent, in order with an empty line between
  two, then an empty line and whatever was already typed there (measured again on 2026-11-23 with a draft typed
  during the run, `pi_box_drafted_cancelled.bin`); the interrupted prompt is not among them, and the
  next Enter sends all of it as one message. Nothing on the screen tells a message given back from typed text: what
  comes back is each message's own text, and it is by those texts, which it keeps for the turn, that Octoboard knows
  the messages at the start of the box and writes them again, in order, after its next message.
- The box is the rows between two full-width rules of `─`, above a footer of two rows (the working directory, then
  the context use and the model). Its rows start in the first column with no prompt mark, an empty box is one empty
  row with no placeholder, a line longer than the width wraps onto the next row, and a completion list (after `/`)
  is drawn under the lower rule. During a run the upper rule reads `── <spinner> Working ───…`. The rules are drawn
  in the colour of the thinking level (bundled `theme.js`), which at `off` is faint (SGR 2), as the footer and the
  steering rows are, so with faint cells blanked the box has nothing around it to be found by: Octoboard reads it off
  the screen as drawn.
- The box shows at most five rows, or three tenths of the terminal's height where that is more; beyond that it
  scrolls, and the rule on the side with rows out of sight carries their count (`─── ↑ 3 more ───`, `↓` below;
  bundled `editor.js`). A paste of more than ten lines or a thousand characters is shown as `[paste #1 +24 lines]` or
  `[paste #1 2400 chars]`.
- Pi draws its selectors and dialogs in the box's place, between rules of the same shape (theirs are coloured, not
  faint): `/model`, `/thinking`, `/scoped-models`, `/settings`, `/resume`, `/tree`, `/fork`, `/mcp`, `/login`, an
  extension's `confirm`, `select` and `input`, and the trust confirmation. Each has an entry under a list cursor (`→`
  or `›`) or ends with a row of key hints whose last says what Esc does (`… escape/ctrl+c cancel`, `Esc to cancel`,
  `escape/ctrl+c close`); the session selector starts with `Resume Session (`. Octoboard reads rows of that shape as
  no input box.
- Ctrl-U deletes from the cursor back to the start of its line, a wrapped line whole, and at the start of a line the
  line break before it; on an empty box it does nothing. Three lines took five presses. After a cancel gave two
  steering messages back, the cursor stood at the end of the last, and the presses took its text, then the two line
  breaks before it one a press, then the first message's text.
- Ctrl-D quits, during a `!` command too.

**Terminal title.** `π - <folder>` (the working directory's last component), and `π - <session name> - <folder>` once
the session has a name (`/name`). No activity glyph. Neither part is escaped: in a folder named `my - proj` the titles
were `π - my - proj` and, after `/name Fix - the build`, `π - Fix - the build - my - proj` (1.5.2, 2026-11-23), so
Octoboard takes the folder off by the working directory's own name.

**Processes.**

- Every command Pi runs gets `AI_AGENT=pi`, `PI_CODING_AGENT=true` and `PI_SKIP_VERSION_CHECK=1`; those of the model's
  shell tool also `PI_SESSION_ID`, `PI_PROVIDER`, `PI_MODEL` and `PI_REASONING_LEVEL`, and `PI_SESSION_FILE` for a
  saved session (bundled docs).
- Pi runs each command in a process group of its own and kills the groups of the commands still running as it exits:
  a `sleep` running under `!` was gone after Ctrl-D, after SIGHUP and after SIGTERM, on each of which Pi exits at
  once. A job a finished command left in the background
  (`sleep 300 &`) is not Pi's to end and survived.

### Model and reasoning effort

`pi --list-models` prints a table of the models of every model provider the account holds a usable credential for,
sorted by provider and id, in about a second and without a model call:

```text
provider  model          context  max-out  thinking  images
openai    gpt-4.1        1.0M     32.8K    no        yes
```

A model is passed to `--model` as `provider/model`. A model id that holds a `/` itself is listed as it is
(`standin  org/sub-model`) and taken as `--model standin/org/sub-model`: the request named `org/sub-model`. Octoboard
offers the levels of `--thinking` for a model whose
`thinking` column says `yes` and none for the others. With no usable credential it prints, with exit status 0,
`No models available. Use /login to log into a provider via OAuth or API key. See:` and two paths of its docs.

### Sign-in

Pi signs in to model providers, not to one account. A credential is any of: an entry of `auth.json`
(`{"<provider>": {"type": "api_key", "key": …}}` after `pi auth login <provider> --api-key`, which asks for the key in
the terminal), a provider's key in the environment (`OPENAI_API_KEY`), or a provider of `models.json` with its key.
`pi auth check` needs `--provider` or `--model` (exit status 2 without) and prints, with `--json`,
`{"status":"ready","provider":"openai","authType":"api_key"}` (status 0) or
`{"status":"not_ready","provider":"openai","reason":"credentials_not_configured"}` (status 1). Since it answers for one
provider only, the background reading checks the named providers with `--json --no-refresh` when OAuth is stored.
A ready provider means signed in. If no check confirms a ready provider, the reading is unknown, since Pi may
refresh OAuth at the next session start. For accounts without stored OAuth, `pi --list-models` also covers credentials
from the environment and custom providers. Its no-models notice means signed out.

On Pi 1.5.2 on macOS (2026-11-24), the account held `anthropic` of type `api_key` (fields `type` and `key`) and
`xai` of type `oauth` (fields `type`, `access`, `refresh`, `expires`). The checks exited 0 with
`status: "ready"` and `authType: "api_key"` and `"oauth"` respectively. The same account returned 18 Anthropic
and 4 xAI rows in `pi --list-models` with its cached catalogue and `PI_OFFLINE=1`. The listing acquires an
`auth.json.lock` beside the resolved credential file, including through a symlink. Both checks and the listing
were run with the user's credential linked into scratch and protected from writes. The file stayed byte-identical
and the temporary lock was gone afterwards.

**Device sign-in.** `pi auth login xai --no-browser` refuses pipes with exit 1, requiring an interactive terminal.
On a private PTY, Pi 1.5.2 printed `Open <https URL on accounts.x.ai> and enter code: <device code>` and
`Waiting for authentication...`, and left the browser closed. The probe cancelled before authorizing the device.
Octoboard runs this command on a private PTY, shows the page and device code in its login dialog, and leaves the
authorization and credential storage to Pi. The window's **Log in** uses xAI. The CLI's
`pi auth login <provider>` remains available for other providers and `--api-key` for providers accepting a key.
Octoboard's login has no provider picker or field for a key.

**Pi has no plan of its own, so there is no plan quota to read.** What a session spends is counted by each model
provider against that provider's key or subscription, and `pi --help` (1.5.2) names no command for a quota or for
usage; Octoboard reads none for Pi, and its descriptor declares no quota window.

### Reading and carrying records

Measured on Pi 1.5.2 on macOS (2026-11-24). Pi writes each turn as `type: "message"` entries. Its message roles are
`user`, `assistant` and `toolResult` for the prompt, the reply and a tool outcome. An assistant's `toolCall` content
block carries the call ID, name and arguments. `model_change` names `provider` and `modelId`, while
`thinking_level_change` names `thinkingLevel`. Octoboard reads archived history and the live feed from the latest
written branch. It keeps the prompt and reply text, classifies tool calls and attaches outcomes to their call IDs
in the feed. The stand-in's raw branch capture is `apps/daemon/testdata/pi_branched_records.jsonl`. Going back to
the first reply and then prompting appended a new path in that file. The abandoned path stayed on disk and was
absent from the next model request.

A `compaction` appends a summary with `firstKeptEntryId`. Pi presents the summary first, followed by the entries
beginning at that ID and by entries after the compaction. It retains the earlier lines for history. A
`context_edit` with `targetId` and `replacement: null` leaves its target in the file and omits it from what the
model sees. A stand-in reply omitted this way stayed in the raw record and disappeared from the next request.
Octoboard keeps the original messages in the archive and marks compactions in the live feed.

With a stand-in replying with `input: 100`, `cacheRead: 23`, `output: 17`, and `totalTokens: 140`, Pi's
`ctx.getContextUsage()` reported 140 of 60,000 at settle. The context edit reset the usage baseline and the live
projection estimated 3,485 tokens. After compaction Pi reported a null size until the next reply, which restored
140. Pi's bundled implementation adds estimated trailing messages after the latest valid assistant usage and
falls back to an estimate from the projected context after a context edit. Octoboard's launch extension
reports this figure and the model window on session and model changes, before model requests, at turn end
and settle, and after tree navigation or compaction. An in-memory tree rewind reaches the collector even before
a new entry is written. Without a live report, the collector reads assistant usage on the persisted branch
and clears that reading at compaction or context edit until a new reply.

A resume in an independent scratch configuration directory returned exit 1 before its record was copied. After
copying only `sessions/<working directory>/<timestamp>_<id>.jsonl` at the same relative path, Pi resumed the same
conversation and the next model request included its earlier prompt. The signed-in check repeated the move from a
scratch home using the Anthropic key to another scratch home using xAI OAuth. The xAI model repeated the marker
from the prior reply and Pi appended to the copied file. Two real model turns were used. History, removal and
carrying operate on this one JSONL file. Pi's standard record layout has no separate archive directory. When the
configured records directories can be searched completely, the missing file means the conversation is gone.
Pi also advertises `PI_CODING_AGENT_SESSION_DIR` and `--session-dir` for nonstandard record locations. Octoboard
also searches the directory named by the environment variable, where Pi puts its JSONL files directly.
Relative directories are resolved from the launch's working directory.

### Sharing configuration and MCP inventory

Measured on Pi 1.5.2 on macOS (2026-11-24), with two scratch configuration directories and the second linking
`settings.json`, `models.json`, `mcp.json` and `extensions/` to the first. A local package install updated
`settings.json` through its link, and `pi mcp add` updated `mcp.json` through its link. Both links remained in
place. Installing a file stored under the second account as `installed.js` recorded `"installed.js"` in the
shared settings' `packages` array. Pi resolves that relative name under the active account directory. Sharing
`settings.json` therefore needs to handle account-relative local packages as well as absolute paths. The
shared `extensions/` directory can hold local extension sources with the same relative location in both
accounts. In a second 1.5.2 scratch run, a local model stand-in received the original prompt on resume through
linked `sessions/`. Both accounts loaded the linked extension, while only the default directory loaded the
relative package that existed beside its settings. A separate scratch probe set `httpProxy` and confirmed
that Pi sent the inline authentication to the local proxy. Octoboard shares `extensions/`, `sessions/` and
eligible `settings.json`. It leaves settings private and reports the reason when either file holds an inline
credential or a local source cannot be resolved. It resolves relative package sources from the default directory
before linking settings. During a session, a new relative source refers to the active account. Octoboard uses
the stable absolute spelling if it resolves outside the sharing account. Model and MCP definitions can hold
credentials in keys, headers or command arguments, so `models.json` and `mcp.json` stay private with
`auth.json` and `mcp-auth.json`. A separately configured `PI_CODING_AGENT_SESSION_DIR` is outside sharing.

`pi mcp list --json` reads `mcp.json`, starts enabled servers and returns each server's scope, transport, state
and tools. Pi 1.5.2 reported failures for a missing local command and an unreachable HTTP server and created
`mcp-auth.json` during the read. Octoboard lists the account's configured servers from `mcp.json` without
running them. It shows only an HTTP origin for a remote server and leaves runtime, tool count and remote sign-in
unknown. The CLI's live diagnostic remains available explicitly in a terminal.

### Install and update

The npm package `@earendil-works/pi-coding-agent`, binary `pi`; `pi update` updates Pi itself (`--self`, the default
with no target, updates Pi only). The package names `https://github.com/earendil-works/pi` as its repository
(`package.json`), which is the address Octoboard gives for installing it.

- **npm.** The binary on `PATH` links to `<package>/dist/bundle/cli.js` (1.5.2), and the package's `package.json` is
  three levels above it. Octoboard reads the version there, the package's name checked, and updates the install with
  npm.
- **The vendor's installer** (`curl -fsSL https://pi.dev/install.sh | sh`, `irm https://pi.dev/install.ps1 | iex` on
  Windows) makes a *managed* install on macOS and Linux. Read from the script as served on 2026-11-23 and from Pi
  1.5.2's own update code (`dist/package-manager-cli.js`), and seen on disk the same day: the script run in a scratch
  home on macOS, with only Node.js and npm on `PATH`, installed Pi 1.6.0 as follows. The root is `install` in the
  configuration directory (`~/.pi/agent/install`, or `$PI_CODING_AGENT_DIR/install` as the installer ran), holding
  `managed-install.json` (`{"kind": "pi-managed-install", "schemaVersion": 1, "layout": "releases-v1", "entrypoint":
  {"type": "symlink", "path": "<home>/.local/bin/pi"}}`), a `current-version` file of one line (`1.6.0`), a
  `staging` directory, and each release under `releases/<version>` (a lockfile root package
  `@earendil-works/pi-coding-agent-install` of the same version, a `metadata.json`, and its dependencies installed with
  `npm ci`) so that Pi is `releases/<version>/node_modules/@earendil-works/pi-coding-agent`, version 1.6.0 in its
  `package.json`. The command is a shell script the installer
  writes at `<configuration directory>/bin/pi`, which reads `current-version`, checks it holds only letters, digits and
  `._+-`, exports `PI_MANAGED_INSTALL_ROOT` and starts that release's `node_modules/.bin/pi`; a directory on `PATH`
  (`~/.local/bin`, `~/bin`, a Homebrew `bin`) gets a link to it. `pi update --self` started through the launcher
  updates the managed install itself (`runManagedSelfUpdate`); on Windows the installer makes an npm install. Octoboard
  reads a launcher's version from `current-version` and that release's `package.json`, the marker checked, and
  recognises an install whose launcher is `~/.pi/agent/bin/pi` (a link to it followed) as the installer's, updated with
  `pi update --self`, its latest version read from the npm registry. A launcher under another configuration directory,
  which can have any name, is recognised the same way by the marker `install/managed-install.json` beside its `bin`
  (`CliPackaging::native_markers`).

### Through a real model and a development daemon

Measured on 2026-11-22 with Pi 1.5.2 signed in to one model provider with an API key (`auth.json` holding
`{"anthropic": {"type": "api_key", …}}`), through a development build of the daemon driven on its control socket,
with `anthropic/claude-haiku-4-5` and `--thinking off`. The daemon ran under a home of its own whose `.pi` was a link
to the user's, since the daemon's data directory follows `HOME` and Pi's credential is a file, not a keychain entry.

- **Sign-in and models**: the account read as signed in, and the catalogue listed all 18 rows of `pi --list-models`
  as `anthropic/<model>`, each with the seven levels (every row's `thinking` column said `yes`).
- **Start**: a console session and a project session both got their Pi session id from the extension's
  `session_start` about a second after launch, and their first task from the launch arguments. No trust confirmation
  appeared for a project folder with no `.pi` resources. Pi wrote `settings.json` (`lastChangelogVersion`) and `bin/`
  (`fd`, `rg`) into a configuration directory that had only held `auth.json` and `models-store.json`.
- **Status**: a session launched with a task read *working*, then *idle* for a few tenths of a second (Pi's `idle` once
  it is past start-up), then *working* again when Pi reported the turn, and *idle* at `done`. The program status path
  has since been changed to hold an `idle` reported before the launch's prompt has started its turn and to pass it
  over when `working` follows; a Pi session has not been seen through a development daemon since.
- **Octoboard's tools**: the model called `mcp__octoboard__list_projects`, `open_session`, `send_message`,
  `interrupt_session`, `get_session` and `report` by those names, and each came back with its result.
- **A report to a parent**: a project session's `report` with status `done` reached the console session (queued while
  that session was mid-turn, delivered when it could take a message) and archived the reporter.
- **An instruction written in**: the parent's instruction arrived in the idle project session as a user message and
  started a turn (*working*).
- **An interrupt by a parent**: with the project session in a `sleep 120` of the shell tool, the Esc ended the command
  (`Command aborted`, the turn's last assistant entry `aborted`, no `sleep` left running) and the session read *idle*.
  Pi reports `idle`, not `error`, for this, so the session was **not shown paused**, and the parent was told that it
  had stopped without reporting. The program status path has since been changed to read `working` to `idle` with no
  `done` as an interrupted turn (`docs/product/program-status-reports.md`); a Pi session has not been seen through a
  development daemon since.
- **Resume after archive**: an archived session resumed with `--session <id>` into the same session file, and the
  model answered from the earlier turns.
- A fresh session's first request carried about 32,000 tokens of system prompt and tool declarations for a project
  session and 39,000 for a console session.

In a later Pi 1.5.2 macOS run with a loopback model, an interrupt returned a busy message to the daemon's queue
and left the terminal draft in place. Two turns read 110 and 210 context tokens in a 60,000-token window.
Rewinding without a summary returned the live count to 110 without another prompt. With
`PI_CODING_AGENT_SESSION_DIR` set, the daemon read archive history and context from that directory and resumed
the same Octoboard session.

### Prompts, the cross-project guard, images and slash commands

Measured on Pi 1.5.2 on macOS on 2026-11-23, in a scratch `HOME` and `PI_CODING_AGENT_DIR` and a scratch working
directory, with a local stand-in answering the model's requests as a provider of the scratch `models.json` (its model
declared `"input": ["text", "image"]`); first with a probe extension that logged Pi's events, then with Octoboard's
extension itself, a stub hook script and a stub guard script. No real model turn was spent. The captures are in
`apps/daemon/testdata/pi-captures.md`.

- **The prompt.** A user message fires `message_start` with `role: "user"` as it enters the conversation: the prompt
  that starts a run, after `input` and `before_agent_start`, and a steering message once the run takes it, after the
  current response (`input` fired for it at once, with `streamingBehavior: "steer"`). A steering message Esc put back in
  the input box fired `input` and never `message_start`. Its content is what the model is given: Pi trims what it takes
  from the box, and an `input` handler's transform is applied first. The extension forwards each as `message_start` with
  its text as `prompt`, which Octoboard reads as the submitted prompt (`HookInterpreter::starts_conversation`):
  measured, one for each of two prompts, a message of one image alone (an empty text), one of two images and text, and a
  prompt and its steering message. Pi awaits every `before_agent_start` handler before the run's `agent_start`, at which
  it reports `working` (bundled `agent-session.js` and `program-status-reporter.js`); its event carries the run's prompt
  as `prompt`, as the box gave it after the `input` transform, and nothing for a run of an extension's custom message.
  So the extension forwards a run's prompt from there, waiting for the hook script to exit (up to the hook mode's
  deadline and half a second), and does not forward that run's first user `message_start` again (a mark each run sets
  afresh and its `agent_end` or `agent_settled` clears, so a run cancelled before its own message leaves nothing to
  skip); a steering message is still forwarded at its `message_start`. Measured on 1.5.2 on 2026-11-23 against a
  stand-in model, with a stand-in hook script that took 0.3 s and logged its exit, and the PTY's reports logged as they
  were read: the script had exited 9 to 12 ms before `working` was read for a prompt the stand-in answered, one it
  refused with HTTP 400 (`error` 27 ms after `working`), one Esc cancelled mid-request, and one Esc cancelled as it was
  being started (`working` and then `idle` 5 ms apart); forwarded at `message_start`, the prompt would have gone out
  only after `working`. The hook script exits once the daemon has queued the event, which the daemon applies on a task
  of its own beside the one applying the reports, so the report can still be applied first: Pi's provider declares that
  a run's opening prompt may follow its turn's start (`HookInterpreter::opening_prompt_may_follow_start`).
- **The cross-project guard.** Pi's file-writing tools are `write` (`{path, content}`) and `edit`
  (`{path, edits: [{oldText, newText}]}`); `bash` writes too, and like the other agents' shell tools is not guarded.
  Pi resolves `path` before it writes (`resolveToCwd`): unicode spaces read as spaces, a leading `@` dropped, `~` the
  home directory, a `file://` URL its path, and a relative path against the working directory. A `tool_call` handler
  runs before the tool, sibling calls of one message one after another, and is awaited: an `async` handler that took a
  second refused the call. Answering `{ block: true, reason }` refuses it, and the model is given `reason`, as it was
  written, as the call's result; the call's other siblings went on (`local.txt` was written beside a refused write).
  The extension, given the session's guard script in its data file, asks it about every `write` and `edit` with
  `{"tool_name", "tool_input": {"path": <resolved>}, "cwd"}` on stdin, and hands back a refusal it answers; a guard
  that does not answer within ten seconds, cannot run, or answers anything else lets the call go ahead. Measured: a
  `write` into another directory and an `edit` named `~/../other/y.txt` were refused with the stub guard's reason and
  left the files as they were, and a `write` into the working directory was asked about and went ahead.
- **Images.** Pi 1.5.2 attaches an image only from the clipboard (Ctrl+V writes it to a temporary file and inserts that
  file's path) and from a `@file` argument of the launch: a path pasted into the box is shown as the text it is,
  wrapped at the box's width like any text, and the box submits text alone (`session.prompt(text)`), so the model is
  given the path as text. An `input` handler's `{action: "transform", text, images}` attaches images: the model was
  given `[{type: "text"}, {type: "image_url", …}]`. The extension takes the paths of the images Octoboard pasted out
  of the text, a file `paste-<n>-<n>.<extension>` whose real directory is the session's `pasted` directory beside the
  extension, bare or in single quotes, and attaches each file; any other path is left as text. Octoboard pastes each
  path on its own with nothing between, and the extension took two paths and text pasted one after another apart. A
  path pasted alone and entered was sent, the Enter not lost, as one image with no text. Octoboard counts the paths in
  the box as the images attached (`ScreenReader::image_placeholders`).
- **Slash commands.** A command pasted into the box and entered runs: `/name probe-name` renamed the session. Pi trims
  the text first, so ` /name guarded` ran too, and a message's leading slash is guarded with a zero-width space
  (U+200B), which Pi keeps: `\u{200B}/name zw` reached the model as that text. A pasted command draws no
  completion: Pi opens its command list only as `/` is typed, and `/na` pasted showed none; an Enter runs the pasted
  text as it is, so a command Pi does not know (`/sess`) reaches the model as a message.
- **Memory.** Pi keeps no memory of its own across sessions: its docs and code have none (the only matches in the
  package are the bundled Anthropic SDK's memory-store client), a new session's first request held nothing of an
  earlier session's messages, and the control, a resume of that earlier session, held them all. Its descriptor
  declares `NativeMemory::Absent`.
- **Prompts a window could answer.** Pi asks for no approval and offers no tool for asking the user; a dialog a user's
  own extension draws (`ctx.ui.confirm()`, `select()`, `input()`) and a sign-in that waits are reported `blocked`
  (above) and are no tool call. Its descriptor declares neither kind of `interactions`.

### Not measured

The guard, the forwarded prompt and an image sent through a development
daemon; an image Pi resizes or refuses for its size, and a model that takes no images; Windows paths in the guard and
in an image's path. Completing a new device authorization through Octoboard's window and the trust confirmation pressed by the daemon in a
real project, and the interrupted turn, the cleared input box and the session id after `/fork` seen through a
development daemon, and neither the extension's own registration of Octoboard's tools nor the turn's message it
forwards; a session whose built-in MCP support is off or replaced (`"extensions": ["-builtin:mcp"]`, an extension
that registers `/mcp`), where the bundled `extensions.md` says a registration is reported as an extension error;
a selector or dialog an extension draws with `ctx.ui.custom()`, which Octoboard would read as the input box unless it
has a list cursor or a key-hint row, and whether any panel Pi itself draws has neither (the step of `/login` that
takes a pasted code, for one).

### A user message sent by an extension

Measured on Pi 1.5.2 on macOS on 2026-11-23, in a scratch `PI_CODING_AGENT_DIR` and a scratch working directory, with
a local stand-in answering the model's requests as a provider of the scratch `models.json`; no real turn was spent. A
probe extension loaded with `--extension` called `pi.sendUserMessage` when a file appeared and logged Pi's events.
The provider uses the session's channel helper, with its path in the extension's launch data.

- **At the prompt** `pi.sendUserMessage(text)` starts a turn at once: the `input` event fires with
  `source: "extension"`, then `before_agent_start` with the text as the prompt, and the model is asked with it as a
  user message. It returns `undefined` and throws nothing. Text the user has typed into the input box and not sent is
  left there untouched, and no part of it goes with the message.
- **During a run it has to say how the message is delivered.** With `{deliverAs: "steer"}` or
  `{deliverAs: "followUp"}` the message waits (`ctx.hasPendingMessages()` turns true) and joins the run: the model was
  asked for the steering message as soon as the running response was done, and for the follow-up one right after,
  each as a user message of its own, inside one `agent_start` to `agent_end` span. With neither, the call still
  returns `undefined` and throws nothing, the `input` event fires, and the message is dropped: it never reached the
  model (the bundled `extensions.md` says Pi reports an extension error). `ctx.isIdle()` tells the two cases apart.
- **Esc gives the waiting messages back.** With a steering and a follow-up message of the extension's waiting, Esc
  ended the run and put both into the input box, unsent, in order with an empty line between two, followed by what
  the user had typed there; the next Enter sent all of it as one message (`source: "interactive"`). So a message an
  extension sent during a run is not delivered once the run is cancelled, and it then sits in the user's input box.
  `ctx.ui.getEditorText()` and `ctx.ui.setEditorText()` read and replace the box's text.
- Not measured: a message sent while the project-trust confirmation or an extension dialog is up; images in the
  content list; `expandPromptTemplates`.

With the production extension and a stand-in channel helper on Pi 1.5.2 (macOS, 2026-11-24), the idle message
arrived in the model request and was reported `taken`. During a delayed reply, a follow-up was reported
`taken` when the extension observed Pi's queue become nonempty. Escape restored it along with the draft.
The extension recognized the exact restored editor contents, removed its own message while retaining the draft
and reported `returned`. The waiting message never reached the model. With other pending input or an ambiguous
editor, the extension requires a matching message entry for acceptance or leaves the restored message in the
editor. A missing channel helper leaves the ordinary terminal path available. Explicit slash commands and image
attachments use the existing terminal path. On Windows the extension starts the executable directly and
detached. Pi 1.5.2 delivered messages with that helper from a path containing spaces.

A direct loopback request to the daemon's production channel route, with a stand-in registered as Pi, released
its pending poll about 7 ms after the client disconnected on macOS. The probe shortened the response deadline
to two seconds, so the release was from the disconnect.

## Across the agents

**Injected MCP tool names** differ in shape, which matters wherever a role description has to name a tool: a server
registered under the key `k` exposes its tool `t` as `mcp__k__t` on Claude Code, Codex, Qwen Code and Pi. The same
tool is qualified as `k__t` on Grok Build, `k/t` in Antigravity CLI, `k-t` on GitHub Copilot CLI and `k_t` on OpenCode.
Antigravity CLI calls it through its generic MCP tool.

**Claude Code, Codex and Grok Build connect their MCP servers when the process starts**, before any turn — a `command`-type server's child is
already running by the time the TUI is drawn. So whether an injected server was accepted is observable without a prompt
ever being submitted.

### Each agent's own memory, and turning it off for one launch

Qwen Code reads and writes managed auto memory. The launch’s system settings turn off both managed auto memory and auto dream. GitHub Copilot CLI keeps Copilot Memory and offers no per-launch off switch. Antigravity CLI, OpenCode and Pi have no native memory. See the memory measurements in each agent’s section.

Claude Code, Codex and Grok Build keep a memory of their own across sessions: notes the agent writes by itself and reads back in a later
session, apart from the instruction files. Each can be turned off for one launch without touching any configuration,
and the switch outranks the user's own settings. Measured on macOS with Claude Code 2.1.328 (the off switch also read out
of 2.1.274), Codex 0.167.1 and Grok Build 1.0.89, each from a scratch home with a fake API key and inference on a local
stand-in endpoint that recorded the request and answered HTTP 400, so nothing reached a provider. "Off" was read from
the recorded request (the memory instructions gone) and from the home (nothing created where the memory is kept), each
against a run with memory on in the same setup.

| | Claude Code | Codex | Grok Build |
|---|---|---|---|
| Name | auto memory | memories (`features.memories`) | memory (`memory_v2`) |
| On by default | yes | no (`stable`, default `false`) | no ("experimental"); a managed remote setting can turn it on |
| Where it is kept | `<config dir>/projects/<key>/memory/` (`MEMORY.md` and topic files), or `autoMemoryDirectory` | `$CODEX_HOME/memories/` (`memory_summary.md`, `MEMORY.md`, `rollout_summaries/`, `extensions/`), with a `memories_1.sqlite` state database beside it | `$GROK_HOME/memory-v2/` (`global/` and `workspaces/<project>-<hash>/`, each with `topics/`, `observations/` and a generated `MEMORY.md`) |
| What the model is given | a "persistent file-based memory" section naming the directory, and the index | the `memory_summary.md` text, between `MEMORY_SUMMARY BEGINS`/`ENDS` markers | a `<memory>` section naming both directories, and the index of each scope |
| Off for one launch | `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` | `-c features.memories=false` | `GROK_MEMORY=0` |

- **Claude Code.** The variable is read before every setting: a truthy value is off whatever `autoMemoryEnabled` says,
  and `0` forces memory on. Off, the section is missing from the system prompt and no `memory/` directory is created
  (with memory on, a `-p` run with no turn of its own creates the directory). `autoMemoryEnabled: false` in a settings
  layer turns it off too, but loses to the variable. It leaves `CLAUDE_CODE_REMOTE_MEMORY_DIR` (see "Sharing a
  configuration directory between accounts") without effect on auto memory.
- **Codex.** `--disable memories` and `-c memories.use_memories=false` turn the injection off as well, but `--disable`
  refuses a feature name the installed Codex does not know (`Error: Unknown feature flag`), while an unknown
  configuration key is ignored. Off, two runs wrote nothing under `memories/`; with memory on, a run created
  `extensions/`, `rollout_summaries/` and `memories.lock`. The `memories_1.sqlite` database is created either way.
  `codex debug prompt-input` shows no memory text even with the feature on, so it cannot settle this; the request has to
  be read.
- **Grok Build.** The undocumented `--no-memory` flag (absent from `--help`) works the same as the documented variable.
  Its guide's priority order puts only a `requirements.toml` pin above either. Off, no `memory-v2/` directory is
  created. Grok also keeps memory off by itself in a temporary directory.

While Octoboard's own memory is on (the application setting `memory_enabled`, off by default), Octoboard turns off switchable native memory for every console and project launch. Copilot Memory follows its own account setting.
each session keeps what it learns in Octoboard's memory instead (a console's, or a project's place's), which every
agent and account reads. While the setting is off, each agent's memory is left as the agent and the user set it.
Memory files an agent wrote before are left where they are.

### Whether a command the agent runs sees the launch's variables unchanged

The newer agents’ launch sections record which variables reach their tools and children. OpenCode restores the shell’s original Claude-compatibility variable values in commands the agent starts. Qwen Code guards its injected hook environment from nested Qwen launches. Pi, Copilot and Antigravity launch with their provider-specific account or scratch paths.

A daemon started from a command an agent runs tells what the session's launch set from what the user set by comparing
each variable's value with a digest the launch recorded (`OCTOBOARD_SESSION_EXPORTS`; "The launch environment" in
`docs/product/launching-agents.md`). A CLI that rewrote one of those variables for its commands would make the
comparison miss, and the rewritten value would be kept as the user's.

- **Claude Code 2.1.330, macOS, measured** by comparing the environment of the `claude` process of a project session
  on the default account with the environment of a command its Bash tool ran (names compared, no model turn spent on
  it beyond the command): every variable of the process reaches the command with the same value, `PATH`, `HOME`,
  `TERM`, `GIT_CONFIG_PARAMETERS` and Octoboard's `OCTOBOARD_*` variables included; only `PWD`, `OLDPWD`, `SHLVL` and
  `_` differ, which the shell sets. It adds variables of its own and changes none: `AI_AGENT`, `CLAUDECODE`,
  `CLAUDE_CODE_CHILD_NOTES_DIR`, `CLAUDE_CODE_CHILD_SESSION`, `CLAUDE_CODE_ENTRYPOINT`, `CLAUDE_CODE_EXECPATH`,
  `CLAUDE_CODE_MESSAGING_SOCKET`, `CLAUDE_CODE_MESSAGING_TOKEN`, `CLAUDE_CODE_SESSION_ATTENDED`,
  `CLAUDE_CODE_SESSION_ID`, `CLAUDE_EFFORT`, `CLAUDE_PID`, `COREPACK_ENABLE_AUTO_PIN`, `GIT_EDITOR` and
  `NoDefaultCurrentDirectoryInExePath` (and, from the user's shell profile rather than from Claude Code, `CPATH`,
  `LIBRARY_PATH`, `MANPATH` and `SDKROOT`). `CLAUDE_CODE_CHILD_NOTES_DIR` was first seen on
  2.1.330 and is declared with the others as the session's. Not measured: a pinned account (`CLAUDE_CONFIG_DIR` set by the launch), and the launch's memory
  variables; nothing seen suggests they are treated differently from the variables that were present.
- **Codex 0.167.1, macOS, measured** with one `codex exec --sandbox workspace-write --ephemeral` turn on the default
  account, whose only command wrote the names of its environment and digests of a fixed few, compared with the same
  taken where Codex was started: no variable of the process is missing from the command, and `HOME`, `TERM`,
  `GIT_CONFIG_PARAMETERS` and the `OCTOBOARD_*` variables hold the same values. `PATH` is the one value changed. Added:
  `CODEX_APPLY_PATCH_PRESERVE_LINE_ENDINGS`, `CODEX_CI`, `CODEX_SANDBOX`, `CODEX_SESSION_ID`, `CODEX_THREAD_ID`,
  `CODEX_TOOL_CALL_ID`, `CODEX_VERSION`, `COLORTERM`, `GH_PAGER`, `GIT_PAGER`, `LC_ALL`, `LC_CTYPE` and `NO_COLOR`.
  `CODEX_HOME` was unset and stayed unset; a pinned account's, and the interactive TUI, were not measured. A user's own
  `shell_environment_policy.set` could give a command another value for a variable the launch set; that value is then
  the user's doing and is kept.
- **Grok Build 1.0.92, macOS, measured** the same way with one `grok -p --always-approve` turn on the default account:
  no variable of the process is missing from the command, the same variables hold the same values, and `PATH` is the
  one value changed. Added: `AWS_PAGER`, `CARGO_TERM_PROGRESS_WHEN`, `CARGO_TERM_PROGRESS_WIDTH`, `CLICOLOR`,
  `CLICOLOR_FORCE`, `COLORTERM`, `FORCE_COLOR`, `GH_PAGER`, `GIT_PAGER`, `GIT_SEQUENCE_EDITOR`, `GIT_TERMINAL_PROMPT`,
  `GPG_TTY`, `GRADLE_OPTS`, `GROK_AGENT`, `GROK_SESSION_ID`, `MANPAGER`, `MAVEN_OPTS`, `NO_COLOR`,
  `NPM_CONFIG_PROGRESS`, `PIP_PROGRESS_BAR` and `SYSTEMD_PAGER`. `GROK_HOME` was unset and stayed unset; a session
  launched with one, and the interactive TUI, were not measured.
- So none of the three changes or removes, for the commands it runs, a variable a launch sets. What each adds is
  handled in two ways. A variable of the CLI's own that marks its session is declared by name and taken out of a nested
  daemon's snapshot (`inherited_session_vars`): Codex's `CODEX_APPLY_PATCH_PRESERVE_LINE_ENDINGS` (`1`), `CODEX_CI`
  (`1`), `CODEX_SANDBOX` (`seatbelt` on macOS), `CODEX_SESSION_ID`, `CODEX_THREAD_ID`, `CODEX_TOOL_CALL_ID` and
  `CODEX_VERSION`; Grok Build's `GROK_AGENT` (`1`) and `GROK_SESSION_ID`. A switch under a name users also set is
  declared with its exact value (`command_environment`) and put back by value, so that a user's own setting of the same
  name is kept:

  | CLI | Sets for a command |
  |---|---|
  | Claude Code 2.1.330 | `COREPACK_ENABLE_AUTO_PIN=0`, `GIT_EDITOR=true`, `NoDefaultCurrentDirectoryInExePath=1` |
  | Codex 0.167.1 (`codex exec`) | `GH_PAGER=cat`, `GIT_PAGER=cat`, `LC_ALL=C.UTF-8`, `LC_CTYPE=C.UTF-8`, `NO_COLOR=1`, `PAGER=cat` (only where `PAGER` was unset: a `PAGER=less` of the user's stayed), and an empty `COLORTERM` |
  | Grok Build 1.0.92 (`grok -p`) | `AWS_PAGER=` (empty), `CARGO_TERM_PROGRESS_WHEN=always`, `CARGO_TERM_PROGRESS_WIDTH=80`, `CLICOLOR=1`, `CLICOLOR_FORCE=1`, `COLORTERM=truecolor`, `FORCE_COLOR=1`, `GH_PAGER=cat`, `GIT_EDITOR=true`, `GIT_PAGER=cat`, `GIT_SEQUENCE_EDITOR=true`, `GIT_TERMINAL_PROMPT=0`, `GPG_TTY=` (empty), `GRADLE_OPTS=-Dorg.gradle.console=rich`, `MANPAGER=cat`, `MAVEN_OPTS=-Dstyle.color=always`, `NO_COLOR=1`, `NPM_CONFIG_PROGRESS=true`, `PAGER=cat` (also over a `PAGER=less` of the user's), `PIP_PROGRESS_BAR=on`, `SYSTEMD_PAGER=cat` |

  Codex sets neither `GIT_EDITOR`, `COREPACK_ENABLE_AUTO_PIN` nor `NoDefaultCurrentDirectoryInExePath`, and Grok Build
  neither of the last two (measured with all three removed from where the CLI was started). `COLORTERM` is one of the
  snapshot shell's terminal variables, which a snapshot drops by name already. The Codex values were read under
  `codex exec --color never` and the Grok Build ones under `grok -p`, both headless turns; an interactive session was
  not measured. A value that differs there only makes the comparison miss, which leaves that variable in a nested
  daemon as it was before these values were declared.
- **`PATH`.** Claude Code 2.1.330 gives a command the `PATH` it was started with. Codex and Grok Build run a command
  through the user's shell with its profile read again, so whatever the profile does to `PATH` is done once more: the
  entries it adds are added again (21 entries became 32 under Codex), and an entry it makes per shell (fnm's
  `fnm_multishells/<pid>/bin`) is another one. Grok Build 1.0.92 adds nothing of its own. Codex 0.167.1 adds the
  `codex-path` directory of its installed release (first, and once more further on) and a directory of the session's
  own, `$CODEX_HOME/tmp/arg0-<os>/codex-arg0<random>`. The `PATH` a daemon started from such a command inherits is
  therefore the profile's doing as much as the CLI's, not the launch's with a fixed set of entries added, and it is left
  as inherited on purpose: a nested daemon's own reading of the shell environment runs the profile again and rebuilds
  it, and what remains of Codex's is a tool directory and a temporary directory that is gone.
- **OpenCode** is the one known to rewrite: Octoboard's own plugin hands the user's `OPENCODE_CONFIG_CONTENT` and
  `OPENCODE_DISABLE_TERMINAL_TITLE` back to the shells the agent starts, so a daemon started there sees the user's
  values, which is what it should keep.

### Exiting ends the process groups of running commands

GitHub Copilot CLI, OpenCode, Qwen Code and Pi also end the groups of commands still running at exit. Antigravity CLI’s measured shell formed its own group, and a descendant ignoring SIGHUP, SIGTERM and SIGINT outlived both hang-up and normal exit. See the exit measurements in each agent’s section.

Measured on macOS with Claude Code 2.1.327 and Codex 0.167.1, by sending `SIGTERM` to `claude -p` and `codex exec` and
watching marker `sleep` processes. Grok Build 1.0.89 was only read from its executable, which runs each command in a
process group of its own and ends child groups with `killpg`; its headless runs stop their background work on exit, and
an interactive exit was not tried.

Each command the agent runs is a shell leading its own process group, a direct child of the process that runs commands;
as the agent exits it kills each such group still running, orphans of the group included. A `setsid` job survives; so,
under Claude Code, does a `nohup … &` orphan whose command had already finished. `claude -p` waits for its background
tasks before it exits, so a probe of this has to end it. How Octoboard refuses to keep such a process is in "Processes
in a command the agent still runs" in `docs/product/background-processes.md`.

GitHub Copilot CLI 1.0.114 does the same through its npm loader (measured against a stand-in; "GitHub Copilot CLI",
"Command groups at exit").

### Terminal titles and the titles in an agent's own records

- **Antigravity CLI** emits no title unless given a title command. Its saved transcript annotation can carry a task title.
- **GitHub Copilot CLI** uses its recorded session name in the title. It reports activity separately through OSC 7501.
- **OpenCode** emits a terminal title. The conversation’s name comes from its native session records.
- **Qwen Code** uses `Qwen - <folder>` by default with activity glyphs. Its active conversation can have a saved title of its own.
- **Pi** uses the folder or its recorded session name, with no activity glyph.

Each agent sends its title with OSC 0/2 and decorates it (raw strings observed with Codex 0.163, Claude Code 2.1.301 and
Grok Build 1.0.52, `proj` being the working directory's name):

- Codex — items joined by ` | `, the project last: `proj` at startup, `⠧ ⠧ | proj` and `⠙ | proj` while working, `[ ! ]
  Action Required | proj` at an approval, `Explain pseudo terminal | proj` once the thread is named, and an empty title
  at exit. The project item is the working directory's name (whether it is the git repository root's name inside a
  worktree is unverified), and a name longer than 24 graphemes is cut to its first 21 and an ASCII `...`
  (`truncate_terminal_title_part` in `codex-rs/tui/src/chatwidget/status_surfaces.rs`), so a console's directory, named
  by its 36-character id, shows as `f1120ca1-f304-4489-aa...`. A thread is named in `<CODEX_HOME>/session_index.jsonl`,
  append-only lines `{"id","thread_name","updated_at"}` with the thread id as `id`, the last line of an id winning,
  written by `/rename` too. Codex's own naming fails while an MCP server is supplied through `-c` (openai/codex#53087),
  so an Octoboard session stays unnamed there. What the user typed is in the rollout as an `event_msg` of type
  `user_message` (`payload.message`) up to 0.142.5, and as one of type `item_completed` whose `item` is a `UserMessage`
  (`item.content[].text`) in 0.163.0; its user-role `response_item` messages include the injected environment context
  and `AGENTS.md` instructions.
- Claude Code — a status glyph, a space and the title: `✳ Claude Code` (a placeholder until the conversation has a
  title), `◐ Claude Code`, `◐ Pseudo terminal explanation`, `✳ Pseudo terminal explanation`, and an empty title at exit.
  The transcript carries `{"type":"custom-title","customTitle"}` (the user's rename) and `{"type":"ai-title","aiTitle"}`
  lines; the last of each kind wins and the rename outranks the generated one. A conversation that opens with a slash
  command gets no `ai-title` line and its terminal title stays `✳ Claude Code` (2.1.308), until a prompt of the
  user's own gets one generated. From 2.1.322, once the terminal answers its OSC 7501 support query, the glyph is
  dropped (`Claude Code`, `Essay about rivers`, and an empty title at exit), and the turn's state goes into OSC 7501
  reports instead (see "## Claude Code").
- Grok Build — items joined by ` - `, its own name last: `grok` at startup and exit, `⠋ - Waiting for response… - grok`
  and `⠋ - Waiting for response… - Explain in one sentence what a pseudo te… - grok` while working, `Explain in one
  sentence what a pseudo te… - grok` when idle. The session directory's `summary.json` holds `generated_title`.

### What the user said, in an agent's own record

Antigravity CLI keeps user steps in its transcript, GitHub Copilot CLI writes user messages to its session event log, OpenCode exposes user messages from its native store, Qwen Code writes them on its active chat branch and Pi records them on its JSONL session tree. Rewinds, attachments and native follow-up turns are described in the corresponding agent sections.

An agent's record holds more than the user lines one would expect (observed on Claude Code 2.1.295–2.1.302, Codex
0.162.1 and Grok Build 1.0.62):

- Claude Code — a message typed while the model is working is recorded only as an `attachment` line with
  `attachment.type` `queued_command` and `attachment.origin.kind` `human`, the text in `attachment.prompt` (a string or
  a list of text blocks); no `type: "user"` line carries it. Conversely, `type: "user"` lines whose `origin.kind` is
  present and not `human` are not the user's: `task-notification` (a background task's or subagent's completion,
  text opening with `<task-notification>`) and `peer` (a message from another session).
- Claude Code slash commands (2.1.308) — a command sent as a prompt is a `type: "user"` line with a `promptId` and the
  string content `<command-message>x</command-message>\n<command-name>/x</command-name>`, followed by
  `<command-args>…</command-args>` when given; a skill's body follows as a `user` line with `isMeta: true` sharing the
  `promptId`, its content a list of text blocks. A local command such as `/clear` is written in the same tags with no
  `promptId`, after a `<local-command-caveat>` line marked `isMeta: true`. A compaction summary is a `user` line with
  `isCompactSummary: true`, and tool results are `user` lines whose content is `tool_result` blocks.
- Compaction — Claude Code and Codex keep the earlier messages in the same record. Grok Build rewrites
  `chat_history.jsonl`: the first `<user_query>` stays, most of the earlier conversation moves to
  `compaction/segment_NNN.md` beside it (indexed by `compaction/INDEX.md`), and the summary is a `user` line with
  `synthetic_reason` `compaction_meta` opening "This session is being continued from a previous conversation"; another
  `compaction_meta` line carries the user info and rules.

### Subagent events

Antigravity CLI follows background tasks and identifies subagent hooks separately from the main turn. GitHub Copilot CLI attributes events to subagents by `agentId` and reads the record for background work. OpenCode separates child sessions from the attached conversation. Qwen Code reports subagent events and lists background tasks on stop. Pi’s extension receives the main session’s events.

Subagent events carry join traps that make them unusable as if they were the session's own. Octoboard drops them from
the session's status, and registers Claude Code's `SubagentStart` and `SubagentStop` only for the background-task list:

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

### Quitting at the prompt

For GitHub Copilot CLI, Qwen Code and Antigravity CLI, the individual sections describe the keys that leave the prompt and the difference between a cancellation and an exit. Pi exits with Ctrl-D. OpenCode has no folder-trust prompt to clear before its terminal is ready. Octoboard observes the process ending directly.

Quitting an agent cleanly at its prompt fires no `UserPromptSubmit` on Claude Code, Codex or Grok Build (measured on Claude Code
2.1.316 and 2.1.274, Codex 0.163.0 and Grok Build 1.0.70, each in an isolated probe), so the session's last reported
status at a clean quit is still its prompt. The quits measured were `/exit`, `/quit`, Ctrl+D, Ctrl+C and a bare
`exit`, plus Ctrl+Q on Grok Build. What fires instead:

- Claude Code — `SessionEnd` with `reason` `prompt_input_exit`.
- Codex — `SessionEnd` with `reason` `other`; with the TUI attached to an app-server (`--remote`), quitting only
  detaches it ("Disconnected from this task"), and `SessionEnd` fires when the server ends.
- Grok — `SessionEnd`, then `Stop` with `reason` `shutdown`.

### Stopped by a signal mid-turn

The command cleanup measurements for GitHub Copilot CLI, OpenCode, Qwen Code and Pi are in their sections and summarized above. Antigravity CLI can leave a signal-resistant descendant running after a terminal hang-up. These cleanup measurements do not establish whether a partial reply is saved.

Sent `SIGTERM` while a turn is in progress (the model request held open), no agent reports the turn as having ended
normally. Measured on Codex 0.163.0, Claude Code 2.1.316 and Grok Build 1.0.70 in an isolated probe:

- Claude Code — fires only `SessionEnd` with `reason` `other`, carrying the turn's `prompt_id`; no `Stop`.
- Codex — exits at once (status 15) and fires no hook at all: no `Stop`, `Interrupt` or `SessionEnd`.
- Grok — fires `SessionEnd` and `Stop`, both with `reason` `shutdown` and in either order, neither naming a
  `promptId`; no `StopCancelled` or `StopFailure`. It exits with status 0, and its record notes no end of the turn.

Grok Build's hooks doc says its session-end `Stop` carries `reason` `channel_closed` or `shutdown`, naming no trigger
for the former. On 1.0.70 no ending produced `channel_closed`, mid-turn or idle: `SIGTERM` or `SIGHUP` to the leader,
the PTY master closed, stdin closed or `session/close` on `grok agent stdio` all fire the `shutdown` pair above, and
`SIGKILL` fires nothing. Each of Octoboard's teardowns is one of these, so a `channel_closed` `Stop` is not expected
from it; were one to arrive, Octoboard's mapping, which only sets `shutdown` aside, would read it as a normal turn end.

## Linux release verification

Focused Linux Rust tests for the five providers (credential-store, account, settings layering and OpenCode paths)
are due at the first Linux release that includes them. The local Docker build of the test binary hit the VM's
8 GiB memory limit. Confirm that the release ref passes the `linux-daemon` gate in `.github/workflows/release.yml`,
which calls `.github/workflows/linux-daemon.yml` for clippy and the full glibc and musl daemon suites.

The installed-Qwen hook ancestry test is ignored by those suites because it requires Qwen Code on `PATH`.
The live Debian 0.28.0 probe covered the marked outer hook and the suppression of inherited hooks in nested shell
and MCP callers. See "Settings layers, and how Octoboard is injected" under "Qwen Code".

## The agents on Windows

What differs when the daemon runs on Windows, beyond what the provider sections above say. Measured in
November 2026 on Windows 11 23H2 (an ARM64 virtual machine) on
**Claude Code 2.1.318** and **2.1.319**, **Codex 0.166.0** (the stop and the hook commands also on **0.163.0**) and
**Grok Build 1.0.77**; each fact names the build it came from where it matters. The probes ran from an elevated session
unless a note says otherwise, and nothing was run on x64 hardware or on Windows 10. Both Claude Code and Codex need Git
for Windows for their own shell tooling.

### Stopping an agent on a pseudoconsole

Each agent ran in a pseudoconsole of the inbox console host, was given one turn, and was stopped when idle or 2 to 3 s
into a long reply. `ClosePseudoConsole` hands every process on the console `CTRL_CLOSE_EVENT`; the two agents answer it
differently.

| Stop | Claude Code 2.1.318 (native `claude.exe`) | Codex 0.166.0 (native `codex.exe`) |
|---|---|---|
| `ClosePseudoConsole` or `CTRL_BREAK_EVENT` to its process group | Exits in 0.8 to 0.9 s, code 129 on the close and 0 on the break; `SessionEnd` with `reason` `other`; the session trailer and `cost-state` are written, and a partial reply and its `turn_duration` mid-turn | Exits in 0.05 to 0.2 s with `0xC000013A` (`STATUS_CONTROL_C_EXIT`, the default handler's); no `SessionEnd` or `Interrupt`; mid-turn the rollout ends at a `reasoning` item, with no `turn_aborted` and the partial reply lost |
| Ctrl-C twice through the input | Exits in 1.3 s with code 0 and `SessionEnd` `prompt_input_exit`; 5.4 s with an unsent draft in the box | Idle: 1.1 s (5.5 s once before any turn, 3.9 s with a draft), code 0, `SessionEnd`, "Shutting down…" and the resume hint printed. Mid-turn: 2.0 s, the first Ctrl-C fires `Interrupt` and the second `SessionEnd`, and `token_count`, the final message and `turn_aborted` are written |
| `TerminateProcess` | `SessionEnd` never fires; `cost-state` is missing, and mid-turn the partial reply and thinking are lost | Code 1, no hook; the rollout is complete up to the last `task_complete` |

- **Claude Code stops politely on the close; Codex does not.** Codex installs no handler for `CTRL_CLOSE_EVENT` or
  `CTRL_BREAK_EVENT`, so for it both are as abrupt as a terminate. A Ctrl-C written into its input pipe is the only
  polite stop that works: it arrives as a key event in its raw-mode input, even though the process group was created
  with `CREATE_NEW_PROCESS_GROUP`, which disables `CTRL_C_EVENT`. Claude Code also quits on Ctrl-C twice, so one
  sequence serves both.
- **`ClosePseudoConsole` returns only once Claude Code has exited**, 0.77 to 0.93 s: it blocks while the client shuts
  down, so it has to be called off the thread reading the console's output, with that output still drained.
- **Codex through its npm shim** (`cmd /d /s /c codex.cmd`, the tree `cmd` to `node codex.js` to `codex.exe`): the close
  and the break ended the whole tree as abruptly (`0xC000013A`, 0.2 s), and a single Ctrl-C ended it gracefully (code
  0). Passing `-c hooks.*` overrides through the shim failed, since `cmd` mangled the arguments holding `&` and `\"`.
- **Children.** An MCP server an agent starts has a hidden console host of its own: both agents start their children
  windowless, so they are not attached to the pseudoconsole and receive no console event. After a graceful stop of
  Claude Code the server read end-of-file on its stdin and exited; after a terminate of either agent, and after every
  stop of Codex, it disappeared without logging an end-of-file or an exit, together with its console host. That the
  agents hold their children in a kill-on-close job of their own is a conjecture; only the disappearance was seen. No
  member of the launch's job outlived the agent in any run.
- **Ctrl-C and the inherited "ignore Ctrl-C" flag.** A process whose parent set Windows' per-process ignore flag (every
  program started over the virtual machine's ssh had it) raises no `CTRL_C_EVENT` for a raw `0x03` written to its
  terminal, on either console host. Claude Code 2.1.319 and Codex 0.166.0 read the byte as a key whatever the flag says:
  both clear the input, and both quit on an empty one (Codex with exit code 0).
- Not tested: a stop during a running tool call or a permission prompt, and a stop of Codex's headless app server.

### Hook and status-line commands

- **Claude Code runs command hooks and the status line through Git Bash** (2.1.318): the chain is `probe.exe`, Git's
  `usr\bin\bash.exe`, Git's `bin\bash.exe` launcher, `claude.exe`, with `MSYSTEM` set by Git and `SHELL` set to
  `C:\Program Files\Git\bin\bash.exe`; an inherited `SHELL` naming PowerShell was ignored, and a
  `CLAUDE_CODE_GIT_BASH_PATH` naming a missing file still ran Git Bash. Forms measured with an executable path that
  holds a space:
  - `exec 'C:\…\bin dir\tool.exe' args 2>/dev/null`, the macOS form with POSIX single quotes: works unchanged.
  - A double-quoted `"C:\…\tool.exe" args` with no `exec`: works, but bash eats a backslash before `\`, `"`, `$` or a
    backquote, so only values free of those survive; `\U`, `\a`, `\c` and the like are not escapes and pass.
  - A quoted `.cmd` (`"C:\…\hook.cmd" args`): works, bash starts `cmd.exe`, which starts the program. The same path
    unquoted fails, as bash eats its backslashes (`C:Users…: command not found`) and Claude Code shows a hook error.
  - The per-hook field `"shell": "powershell"` makes Claude Code run `pwsh.exe` directly: `& "C:\…\tool.exe" args` works
    and the bare quoted path is a ParserError. The binary also says it needs "either Git for Windows (for bash) or
    PowerShell"; which shell hooks use with Git for Windows absent was not tested, as Git could not be hidden from its
    detection.
- **HTTP hooks work without any shell** (`UserPromptSubmit`, `Stop`, `SessionEnd`; `User-Agent: axios/1.15.2`).
- **Codex runs hook commands through PowerShell**, never a direct exec and never `cmd`: `pwsh.exe -NoProfile -Command
  "<command>"` when `pwsh` is on the `PATH`, else `C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe -NoProfile
  -Command "<command>"` (0.163.0 and 0.166.0; read from `[Environment]::CommandLine` inside a hook). So the command
  string is PowerShell source: `& "C:\…\bin dir\tool.exe" args` and `& 'C:\…\bin dir\hook.cmd' args` work, the cmd or
  bash form `"C:\…\tool.exe" args` fails with "hook exited with code 1", and a bare `.cmd` path works only when it holds
  no space. Hooks Codex's remote TUI attached to ran in the app server's process.
- **Codex's first hook is slow.** A cold `SessionStart` through `pwsh` under a home directory with non-ASCII characters
  took 4.3 s on 0.166.0 (4.6 s earlier), so the Windows command-hook timeout is 15 s; `Interrupt` keeps 3 s.
- **Both agents choose their own PowerShell** for the tools and hooks they start (Codex the one that runs its hooks and
  tool calls, Claude Code its PowerShell tool's), and Octoboard starts neither. Where it is the Store's `pwsh`, what it
  starts is outside the launch's job (the "A Store PowerShell leaves the launch's job" entry of "Known pitfalls" in
  `docs/architecture.md`).

### What they draw and read on a pseudoconsole

Claude Code 2.1.319 and Codex 0.166.0, on the inbox console host and on the bundled one ("The two Windows console
hosts" in `docs/architecture.md`):

- **Claude Code** draws every stage as on macOS on both hosts. The trust screen selects "No, exit" first with the glyph
  `❯` (Enter on the default quits; Down then Enter trusts), and with `TERM` unset the same binary draws `>`, for a
  reason not established. The fresh session's placeholder `Try "…"` is faint (SGR 2) and gone after a turn. `ESC[?2004h`
  is on at the trust screen and in the session. Titles run `claude`, `✳ Claude Code`, `◐ Claude Code`, `◐ <task>`, `✳
  <task>`, with the intermediate spinner titles dropped by the inbox host. It queries `ESC]7501;?` and writes
  `ESC[?9001l ESC[?9001h` itself. Words are separated by cursor moves in the bytes (`ESC[1C` on the inbox host, `ESC[nG`
  on the bundled one), so only the parsed screen holds a phrase.
- **Codex** shows its folder-access screen only inside a git repository, with the selected row (`› 1. Trust and
  continue`) bold and inverse across the row and, in a subdirectory, a note "You're in a subdirectory of a Git project"
  with a lower-cased `c:\users\…` path. The composer's placeholder `Ask Codex to do anything` is faint. Titles are
  `<folder>`, `⠸ <folder>` while working and `<task> | <folder>`. It emits `ESC]7501;state=idle|working|done:app=codex
  ESC\`, which passes both hosts. It enters the alternate screen at start on the bundled host (invisible on the inbox
  one).
- **Codex reads input records, not VT bytes.** The console host consumes a bracketed paste's markers and hands Codex the
  pasted text as typed keys, and an Enter written right behind it as one more key in the same burst; Codex draws the
  paste only when its paste burst flushes, 100 to 150 ms later, and takes that Enter for a line break inside the paste.
  On the bundled host, with its cursor-position query unanswered (about 500 ms stalled), an Enter written 300 ms after
  the paste still arrived inside the burst and the message was not submitted.
- **Queries.** Both agents issue cursor-position queries on the bundled host (Codex after every paste and every submit,
  Claude Code once after the trust screen); unanswered, each holds the agent for about 500 ms, and Codex's lost the
  submit as above. Answered at once from a parse of the stream, every stage worked. Whether the host or the agent issues
  them is unverified.
- **Settling.** After a keystroke Claude Code's output is one burst of 15 to 46 ms; its start after the trust screen has
  gaps up to 237 ms, Codex's up to 308 ms, and a pasted draft is drawn 100 to 150 ms after the paste, so an idle window
  of 100 ms declares Codex's screen settled before the draft is drawn.
- **Past the trust screen.** Claude Code 2.1.322 on the inbox host (Windows 11 on ARM64, the daemon an x64 build under
  emulation) clears its screen once the Enter on "Yes" is taken, then draws its next screen 4.1 to 6.8 s later, with
  silent gaps of up to 2.4 s in between; the trust is already recorded in `.claude.json` while the screen is blank. The
  cursor reached "Yes" 146 to 288 ms after the Down.

### Where the agents live on Windows

- **Claude Code** is a native `claude.exe` (about 325 MB) at `%USERPROFILE%\.local\bin\claude.exe`, on the user's
  `PATH`, with no shim; `claude --version` answers. It updates itself while running (2.1.318 became 2.1.319 during a
  probe).
- **Codex** from npm installs `codex.cmd`, `codex.ps1` and a `codex` sh script into `%APPDATA%\npm` (PowerShell's
  `Get-Command` picks `codex.ps1`, a `PATHEXT` lookup `codex.cmd`), a chain of `cmd`, `node codex.js` and the native
  program, which sits in a platform package under the main one:
  `…\node_modules\@openai\codex\node_modules\@openai\codex-win32-<arch>\vendor\<triple>\bin\codex.exe`, with
  `codex-win32-arm64` and `aarch64-pc-windows-msvc` on ARM64 and `codex-win32-x64` and `x86_64-pc-windows-msvc` on x64.
  The launcher script sets the `CODEX_MANAGED_*` variables and puts the package's `codex-path` directory, which holds
  `rg.exe`, on the `PATH`; a native `codex.exe` started directly, an ARM64 standalone one and the x64 package's under
  emulation alike, carries no `CODEX_MANAGED_*` and still started, with `rg.exe` 15.2 found.
- **Elevation.** Codex run as administrator warns "Running as administrator: shared background server disabled", and
  files created from such a session are owned by the Administrators group, not the user.
- **Grok Build** is installed with `irm https://x.ai/cli/install.ps1 | iex`, which puts `grok.exe` in
  `%USERPROFILE%\.grok\bin`; the installer copies it from the unversioned `downloads\grok-windows-<arch>.exe`, and `grok
  update` leaves `downloads\grok-<version>-windows-<arch>` (no `.exe`) of the same size as `bin\grok.exe`, from which
  the installed version reads without starting a process. A `bin` copy with no versioned download beside it has to be
  asked with `--version`.
- **Grok Build has no system-wide configuration directory on Windows**: its Windows implementation's system
  configuration directory is `None`, and `grok inspect` (1.0.77) does not load a `C:\etc\grok` candidate, where Unix
  reads `/etc/grok`.

### Managed settings and credentials

- **Claude Code's managed settings** on Windows are `C:\Program Files\ClaudeCode` (the `managed-settings.json` and the
  `managed-settings.d` directory beside the `/Library/Application Support/ClaudeCode` and `/etc/claude-code` of the
  other systems), and the registry policy `HKLM\SOFTWARE\Policies\ClaudeCode` and `HKCU\SOFTWARE\Policies\ClaudeCode`,
  found in the binary's strings. The registry value carries settings such as `allowedHttpHookUrls` or a managed
  `statusLine`.
- **Claude Code keeps its OAuth tokens in `%USERPROFILE%\.claude\.credentials.json`** (`claudeAiOauth`) on this build,
  with nothing in Credential Manager. The binary has a second store, `windows-credman-with-plaintext-fallback`
  (`CredReadW` and `CredWriteW`), chosen when the server-side flag `tengu_windows_credman` is true, which is cached in
  `~/.claude.json` and was false here. The file is today's default, not a guarantee: a missing file may mean "stored
  elsewhere" rather than "signed out".
- **Claude Code's login honours `BROWSER`** (`claude auth login` against an empty `CLAUDE_CONFIG_DIR`, so the real login
  was untouched): it prints "Opening browser to sign in…" and runs `$BROWSER` with the authorize URL as one argument.
  `BROWSER` naming an executable (unquoted, a path with a space) is started directly, with no shell. Naming a `.cmd`, it
  runs through `cmd.exe` with the URL quoted and whole: `&` is safe through `%*`, but `echo %1 >> file` or `%~1` breaks
  at the `&` (a `%~1` argument ended at `=`, `;` or `,`). A real login URL of 447 characters with 7 `&` arrived whole
  through a `%*` recorder. The URL's `redirect_uri` is `http://localhost:<port>/callback`, a local listener, and the
  screen also offers a paste-code URL.

### Grok Build's session home without file links

Where the host makes no file links (Developer Mode off, not elevated), a per-session `GROK_HOME` on Windows holds
junctions for directories (`sessions`, `skills` and the rest), regular-file copies of the other files, no `*.lock` or
`*.tmp` file and no `auth.json`, and `GROK_AUTH_PATH` names `%USERPROFILE%\.grok\auth.json` (1.0.77). A conversation
started in it is listed and resumed by the user's own `grok`. A token refresh inside the session is written back to the
user's `auth.json`, and Grok creates `auth.json.lock` beside that file, not in the session home. A theme change in the
session is not written back for a plain `config.toml`, as on macOS. Grok's own lock and temporary names, which a copy
leaves out, are a bare `.lock`, a `<file>.lock` beside the file it guards, `.tmp` and six letters or digits, and
`.<number>.<number>.tmp`; `uv.lock` and a `.tmpl…` name are the user's and kept. With file links on (Developer Mode on),
the home links files and locks as on macOS and `worktrees.db` is a copy, as SQLite on Windows names its log from the
link's path ("Codex's state database through a link" under "Sharing a session directory between accounts").

### Codex's app server on Windows

A Codex session's private app server (see "Codex has a separate message queue" below) listens on a loopback WebSocket
on Windows. Measured on 0.163.0 and 0.166.0:

- `--listen ws://127.0.0.1:0` works, and the port the system chose is printed only on **stderr** (`listening on:
  ws://127.0.0.1:<port>`, among a few other lines); 0.166.0 prints it the same way on macOS.
- `--ws-auth capability-token --ws-token-file <file>` rejects a WebSocket with no token or a wrong one (the TUI shows
  `HTTP error: 401 Unauthorized`); the TUI passes it with `--remote-auth-token-env <VAR>` and a client sends
  `Authorization: Bearer <token>`; `/readyz` stays open. `--ws-token-sha256 <hex>` in place of the file takes the
  lowercase hexadecimal SHA-256 of the token's UTF-8 bytes, so no token is written to disk (checked on macOS with
  0.166.0: the right bearer token upgraded, a wrong or missing one got 401). Any local process can reach the port; the
  token is the only guard, and the TUI takes it only from its environment.
- `--listen unix://<path>` works on Windows, if the socket's directory is private to the user: with the inherited ACL,
  or an owner-only ACL whose owner was the Administrators group, Codex refused with "socket directory is not private to
  the current user"; with inheritance removed, only the user granted, and the user as the directory's owner, it
  listened. A bare `--listen unix://` uses `%USERPROFILE%\.codex\app-server-control\app-server-control.sock`.
  Neither the standard library nor tokio connects to a Unix socket on Windows, so a client there would need a crate of
  its own or `codex app-server proxy --sock <path>`.
- A remote TUI asks "Trust this folder?" even with a `projects.<dir>.trust_level="trusted"` override on both sides
  (0.163.0), and on 0.166.0 it lands in an "Agent command center" while `thread/loaded/list` stays empty during the
  trust screen. The hook trust review works as elsewhere; the key of a `-c` hook reads
  `C:\<session-flags>\config.toml:<event>:0:0`, and a resumed thread's hook payload carries a `transcript_path` with
  a `\\?\C:\...` prefix (a fresh one does not). Hooks run through `powershell -NoProfile -ExecutionPolicy Bypass
  -File ...` work.
- The two processes do not end each other (0.166.0). A TUI that exits leaves the server, and the thread, running. A
  server that dies leaves the TUI retrying `Reconnecting to server… (52s)` for good; a server started again on the same
  address is reconnected to, with the draft kept. Killing only the server's process left a command the model was
  running (`PING.EXE`) alive.
- Not measured on Windows: a non-elevated (medium integrity) run, a long collapsed paste through the app server, x64,
  and Octoboard's own generated hook scripts, MCP server, developer instructions and reviewer overrides.

### Antigravity CLI, GitHub Copilot CLI, OpenCode, Qwen Code and Pi on Windows

The Windows 11 ARM64 VM ran build **22631.6199** (23H2). Pi 1.5.2, Qwen Code 0.28.3,
OpenCode 1.19.6 and GitHub Copilot CLI 1.0.114 were installed with npm 11.20.0 into isolated
global prefixes on the VM's ARM64 Node 26.13.0. Antigravity CLI 1.5.16 came from the vendor's
Windows ARM64 manifest. Pi used `PI_CODING_AGENT_DIR/sessions`, Qwen used `QWEN_HOME/projects`,
Copilot used `COPILOT_HOME/session-state`, OpenCode used `XDG_DATA_HOME/opencode/opencode.db`,
and Antigravity used the `config` and `antigravity-cli` entries under `HOME/.gemini`. Each
provider section gives the launcher measurement.

The x64 development daemon resolved the four npm configurations on the VM and reported each
installed version. It recognised the Antigravity native layout and read the available version
from its update manifest. OpenCode 1.19.6 reached the daemon's idle state for both session
kinds. The first `POST /session` timed out in the cold scratch home while the CLI was downloading
ripgrep, leaving the launch trust gate pending. After ripgrep 15.1.0 was in the scratch cache and
on the task PATH, the daemon delivered the first console and project prompts and the first
prompt after `/new` to the stand-in, with no message delivery notices.

On the inbox pseudoconsole, a native child confirmed that stdin and stdout were terminals by
asking its question only there. The prompt arrived whole and unfinished, the answer echo was
an ended line, and the last words arrived unfinished and were captured. A separate redraw test
confirmed that the login flow puts one question to the user. The Copilot native stand-in also
covered the device page, code, and the explicit answer about plaintext storage.

The SSH logon had no desktop session. In that logon, `cmdkey /list:<target>` returned exit
status 0 for a missing target, and `cmdkey /generic:<target>` refused to create a synthetic
credential in that logon. `CredWriteW` also returned 1312 for a synthetic session credential
with an empty blob. In desktop session 1, `cmdkey /list` returned metadata for Antigravity
and Copilot. A targeted query returned the same exit status for their targets and for a
random absent target. The daemon uses the native generic credential lookup. WinCred returns a distinct
error when a target is absent and when there is no logon vault. A native task in the desktop logon found
both live sign-in items and reported the scratch Copilot target absent. No credential dialog appeared.

Pi, Qwen Code, Copilot and OpenCode ran console and project turns against local stand-ins, exchanged
Octoboard tool calls, reports and instructions, and interrupted active requests. Pi, Copilot and OpenCode
sent again after archive and resume. Antigravity's first-run Terms screen held the interactive session
waiting for the user's answer. The user accepted stopping the Windows lifecycle verification there.

## Moving a conversation to another config directory

GitHub Copilot CLI carries the session directory and registers the copied conversation in the destination index on resume. Pi carries its JSONL session file. Qwen Code carries its chat and title and runtime sidecars. Antigravity CLI uses one system login even when `--gemini_dir` changes. OpenCode keeps its database under `XDG_DATA_HOME` and has no agent-specific data-directory override. Neither offers an account-to-account move.

Claude Code, Codex and Grok Build resume a conversation under a second config directory when **the session's own record** is copied
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
  subagent's rollout but a later segment of the same thread**: every such file on the measured machine (26, written by
  0.159.2–0.162.1) has a `session_meta` carrying the thread's own id, and its lines carry only that thread id. The
  `session_meta` has `history_mode: "paginated"` and `history_base: {thread_id, end_ordinal_exclusive,
  end_byte_offset}`, where `thread_id` names the file the segment continues from — the thread's first rollout by the
  thread id, a later segment by its segment id — and the segment's ordinals pick up at that cut. So the files form a
  chain, its last segment (the chain's leaf) is the conversation's head, and the database's `rollout_path` names the
  head (read from a copy of the real database). Reverting a turn starts a segment: through `codex app-server`, a thread
  with two turns, `thread/revert` before the second, then a third turn, left the first rollout (ordinals 0–25) plus a
  segment (ordinals 15–28, based on the first at ordinal 15). A segment can also start a fresh history instead
  (`history_base` null, ordinals from 0; seen on disk, trigger not measured). Resumed with `codex exec resume <thread
  id>` in fresh homes, asking for the codewords given so far:
  - every file of the chain present, no database row → the head, and the right conversation (first and third turns);
    the same with the chain compressed, where only the head was decompressed and the first rollout read in place;
  - the first rollout alone → that file, **the reverted turn back and the third turn gone, with no error or warning**;
  - the head segment alone → refused: `thread/resume failed: invalid paginated history lineage for <id>: missing source
    rollout`.

  So a resume that matches the newest file containing the id picks the head, which is correct. What makes a file the
  newest was not settled. In the fork described under "Which rollout a home resumes, over a shared directory" below, a
  home with no row picked the segment with the later timestamp in its name. That segment was also the chain's leaf,
  while the other segment had the later mtime. So mtime does not decide it; name order or lineage might.
  **Octoboard's switch carries the whole chain**: the head as the record and every other file of the thread as a
  companion, through the same staged copy. Measured with 0.162.1 on a thread made through `codex app-server` (two turns,
  `thread/revert` of the second, a third turn, `thread/revert` of the third, a fourth: a first rollout and two
  segments), the chain copied by that code into a fresh home, plain and compressed, resumed with the surviving first and
  fourth turns and neither reverted one; the new database row named the head (a compressed head came back as a plain
  `.jsonl`, the other two files stayed compressed). Octoboard reads the head for a thread's context size: the last
  `token_count` record with a non-null `info` gives `last_token_usage.total_tokens` and `model_context_window`. The
  cumulative total in a segment continues from the segment before it (in a real thread the first rollout ended on
  964,390 and its segment began at 1,036,910), which is why the last request's figure is read and not the total. A head
  segment has no `token_count` until its next turn after a revert, so until then the last one in the file it continues,
  before that segment's `history_base.end_byte_offset`, stands in, or failing that the nearest earlier file's. A segment
  holds only the lines after the last cut, so the opening prompt comes from the first rollout, and the conversation as
  it stands is the head preceded by each file it continues up to its `history_base.end_byte_offset` (decompressed bytes;
  in a real thread the offset fell on the line boundary before the next segment's first line). Subagents are separate
  threads: a spawned agent's rollout is `rollout-<child start>-<child thread id>.jsonl`, with
  `source.subagent.thread_spawn.parent_thread_id` in its `session_meta`, so its name never contains the parent's id. A
  home with no row resuming the parent with two such rollouts beside it opened the parent with the conversation intact
  (and took rows for both children).
  **A target whose own row names an older head resumes that head, with the whole chain copied.** Measured with 0.162.1:
  a home holding the first rollout and the first segment was resumed once, so its row named that segment; the whole
  chain was then copied in over it (the third file added, the other two replaced) and the thread resumed again. It came
  up on the older branch (the first and third turns; the third had been reverted since, and the fourth was absent), with
  no error, and the row still named the older segment. So a target that held the thread before it was reverted elsewhere
  has to have that row dealt with, not only the files.
  **The row decides, and either removing it or repointing it fixes the resume** (0.162.1, same setup, with a
  `thread_attachments` row and a `thread_dynamic_tools` row seeded for the thread, the database edited with `sqlite3`
  while Codex was not running):
  - Deleting the thread's `threads` row cascades into those two tables when foreign keys are on (both are declared `ON
    DELETE CASCADE`; the `sqlite3` shell has them off by default, and Codex's own connections turn them on). With
    foreign keys off the children stayed, Codex's own insert of the fresh row succeeded with no foreign-key complaint,
    and the children were attached to it again. The resume answered "ALPHA, DELTA". The fresh row is built from the
    rollouts, so the old row's title, pin, section and project were gone; its title and preview now came from the head.
  - Updating only `rollout_path` to the head's path (the whole chain in place) also answered "ALPHA, DELTA", and kept
    the row's other columns and its children untouched. The row's `title`, `preview` and `first_user_message` stayed
    those of the older branch and were not refreshed by the resume's turn. With the chain compressed, a path naming the
    `.jsonl` that no longer exists and one naming the `.jsonl.zst` both resumed the head.
  - Clearing only the home's `thread_history_1.sqlite` rows (the `threads` row left standing) still resumed the older
    branch, and deleting them along with the `threads` row changed nothing: its tables (`thread_turns`, `thread_items`,
    `thread_history_projection_state`) are keyed by the id of a rollout *file*, one set for the first segment and one
    for the head here, not by the thread's, and Codex rebuilt them.
  - Other tables keyed by thread id: `thread_spawn_edges`, `thread_goals` (in `goals_1.sqlite`) and the queue tables
    were empty in the measured home and were not exercised; `thread_sections` is not per thread.
  - **Octoboard's switch updates `rollout_path` only**, in every `state_<n>.sqlite` of the target's database directory
    that has a `threads` table, to the head's path in the target without `.zst`, the form Codex keeps for a compressed
    rollout (no row of 1,212 in a real database ended in `.zst`). It changes nothing else, so nothing cascades and the
    user's title, pin and links stay. The cost is that the older branch's title and preview stay on the row. A home with
    no row is left alone, since it resumes the head by name.
  - **Codex can keep its databases outside the home** (0.162.1): `sqlite_home` in `config.toml` and the
    `CODEX_SQLITE_HOME` environment variable both moved the whole set of `*.sqlite` files, and with both set the
    configuration won. A relative `sqlite_home` (`"db"`, `"../up"`) was taken against the home, and a leading `~/` was
    expanded with the process's `HOME`. A relative `CODEX_SQLITE_HOME` was taken against the directory Codex was started
    in, not the home. The binary also names an enterprise requirement for `sqlite_home` that overrides the environment
    variable; that was not looked for. The switch reads the target's `config.toml` and the environment the relaunch will
    see (the home's `~` is that environment's `HOME`), and fails with `relocation_failed`, rather than update a database
    it has guessed, when the configuration cannot be read or parsed, `CODEX_SQLITE_HOME` is relative (the starting
    directory is not known there), or the directory cannot be listed; a directory that does not exist means no database.
  - **A managed daemon holding the thread was not measured.** In a scratch home every 0.162.1 TUI start launched
    `<home>/packages/app-server-daemon/…/codex app-server --managed-daemon` (parent pid 1), which outlived the TUI and
    copied about 323 MB into the home's `packages/` (measured 2026-10-20). Whether a daemon in the target holding the
    thread loaded would ignore a changed row is unverified.
- **Claude Code, beside the transcript** —
  `projects/<slug>/<session id>/` holds `subagents/`, `tool-results/` and `custom-title.json`, and
  `file-history/<session id>/` holds the rewind checkpoints (`<hash>@v<n>`). Both are layout as found on disk; a resume
  with the transcript alone was measured above, and what a resume without these loses (rewind, subagent transcripts) was
  not.
- **Grok Build** — the session directory copied under a different working directory's encoded name resumed with
  the conversation intact; `sessions/session_search.sqlite` is not consulted. **An unauthenticated home does not
  refuse**: measured with Grok Build 1.0.46, `grok --resume <id>` against a home with no login stays up on a browser
  device-code sign-in (`Approve in your browser to finish signing in`, then `Waiting for approval`) instead of exiting.
  So, as with Claude Code and Codex, a switch to an account that is not signed in comes up and counts as a success, and
  the relaunched session can start the agent's own sign-in flow. Whether the session is looked up before or after that
  sign-in was not measured, so a relocation cannot be verified without a login. For a session it does not hold locally,
  Grok prints `Session "<id>" not found locally, restoring conversation from remote...`. Its remote registry has never
  been seen to restore one: every attempt so far answered 404 (see "Grok Build over a linked `sessions/`" below).
  Whether the registry would serve a session to a different account is unmeasured.
- **Under a different account** — a
  conversation copied into a config directory signed in to another account:
  - **Codex resumes it, encrypted reasoning
  included** (0.162.1). The two homes were signed in by copies of the `auth.json` of two genuinely different ChatGPT
  accounts: a personal Plus account and a Business workspace, with different account and organisation ids. The thread
  was made under A on `gpt-6.1-sol` at medium effort, with a turn whose rollout holds a `reasoning` item carrying
  `encrypted_content`. Copied into B's home and resumed on the same model, it ran a turn and recalled the earlier
  conversation. The response's per-input-item usage in the trace log (`RUST_LOG=trace`) listed that reasoning item's id,
  so it was sent and accepted. Control in the same run: the same rollout with eight characters of the
  `encrypted_content` changed was refused with `invalid_encrypted_content` ("The encrypted content for item rs_… could
  not be verified. Reason: Encrypted content could not be decrypted or parsed."). So the server verifies an earlier
  turn's reasoning, and it accepted the other account's. The reverse direction (made under B, resumed under A) was the
  same. A resume on a smaller model (`gpt-6-luna`) worked too, with a warning that the session was recorded with another
  model.
  - **Claude Code does not resend an earlier turn's thinking, so its signature is no obstacle; the resume under a
  second account itself was not measured** (2.1.302). Two scratch config directories were each signed in by the user, to
  a personal Max account (A) and to a Team account (B). Under A, `claude -p --model haiku` (`claude-haiku-5-5`) wrote a
  transcript whose assistant turn holds a `thinking` block with a 1,580-character `signature`. Two copies were resumed
  under A with `--resume`, one as written and one with eight characters of the signature changed. Both resumed and
  recalled the conversation; the tampered signature was not refused. The transcript's own request log explains why:
  Claude Code 2.1.302 writes `api-request-blob` and `api-request` lines into the transcript, and in the resumed request
  the earlier assistant turn was sent as its text alone, without the thinking block. The request asked for `thinking:
  {type: "adaptive", display: "omitted"}`, with context management `clear_thinking_20251015`, `keep: "all"`. The resume
  under B could not be run because every request B made was answered "You've hit your weekly limit"; the signature
  finding makes a refusal on signatures unlikely.
  - **Grok Build:** not measured; the machine has one Grok Build
  account.

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

| Agent | Sign-in observed |
|---|---|
| Antigravity CLI | Google OAuth through a browser link and an authorization code entered in the dialog. The system credential store holds the login. |
| GitHub Copilot CLI | GitHub device authorization. Where no credential store is available, Copilot asks for explicit permission to store the token in plaintext. |
| OpenCode | `opencode auth login` asks for a provider key in the terminal. Octoboard offers no sign-in dialog. The OAuth route is awaiting measurement. |
| Qwen Code | Qwen’s Web Shell collects and saves the provider key. |
| Pi | `pi auth login xai --no-browser` requires a PTY and prints the device page and code. Other providers and API-key entry use Pi’s terminal. |

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

- **Claude Code** (`claude auth login`; `--console` and `--sso` were not tried) prints on stdout `Opening browser to
  sign in…`, then `If the browser didn't open, visit: <url>`, then `Paste code here if prompted > ` with no newline. It
  listens on `127.0.0.1` at an ephemeral port (53228 in one run) and hands the browser a URL of its own,
  `https://claude.com/cai/oauth/authorize?code=true&client_id=…&response_type=code&redirect_uri=http%3A%2F%2Flocalhost%3A<port>%2Fcallback&scope=…&code_challenge=…&code_challenge_method=S256&state=…`.
  The browser is run as `$BROWSER <url>` when `BROWSER` is set, and otherwise as `open <url>` looked up on `PATH`. Both
  were replaced by scripts that only recorded their argument, and no LaunchServices launch came from the process. So the
  URL Octoboard should open is the one handed to `BROWSER`, captured by a `BROWSER` that reports it back, and the
  loopback callback then finishes the login with nothing pasted. The URL on stdout is a different one: its
  `redirect_uri` is `https://platform.claude.com/oauth/code/callback`, the page that shows the user a code to paste. The
  paste does work without a terminal: `notarealcode#notarealstate` written to a piped stdin was taken and sent, and the
  command answered `Login failed: Request failed with status code 400` on stderr and exited 1 within a second. An empty
  stdin (`/dev/null`, or a pipe held open) does not end the wait. SIGTERM ends it with exit 143. It created a config
  directory that did not exist (`.claude.json` and `backups/`) before any sign-in. On success the credential goes to the
  Keychain item named under "A second config directory is a second login" above, and the account record to
  `.claude.json`. Both, and the `Login successful.` line, come from the executable and the earlier measurements, not
  from a completed login here. `claude auth status` reporting `loggedIn: true` afterwards is the check to make.
- **Codex** has three ways in, and the app server is the one made for a front end. `codex app-server` (the same command
  and flags as the quota probe) takes `account/login/start` after `initialize`:
  - `{"type":"chatgpt"}` replies at once with
    `{"type":"chatgpt","loginId":"<uuid>","authUrl":"https://auth.openai.com/oauth/authorize?…&redirect_uri=http%3A%2F%2F127.0.0.1%3A1455%2Fauth%2Fcallback&…"}`
    and listens on `127.0.0.1:1455`. It opens no browser: no LaunchServices launch came from the process, and nothing
    was run through `BROWSER` or `open`. With 1455 already held by another process, the reply took about four seconds
    and named port 1457, where it listened instead. Whether the authorization server accepts that port was not tried, so
    two logins at once are a risk.
  - `{"type":"chatgptDeviceCode"}` replies with
    `{"type":"chatgptDeviceCode","loginId":…,"verificationUrl":"https://auth.openai.com/codex/device","userCode":"<4
    characters>-<5 characters>"}`, with no listener and no browser.
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
  `PATH`, so it would put up a page Octoboard did not ask for. `codex login --device-auth` prints on **stdout** the link
  `https://auth.openai.com/codex/device`, `Enter this one-time code (expires in 15 minutes)` and the code, with ANSI
  colour escapes even into a pipe, and opens nothing and listens on nothing. Both died on SIGTERM. Neither creates a
  missing `CODEX_HOME`: both exit 1 with `Error loading configuration: CODEX_HOME points to "<dir>", but that path does
  not exist`. On success Codex writes `auth.json` into the home. That rests on the earlier measurement of a copied
  `auth.json` under "A second config directory is a second login" above. Whether the `cli_auth_credentials_store`
  setting could send it elsewhere was not examined.
- **Grok Build** (`grok login`, with no flag) printed on **stderr**
  `To sign in, open this URL in your browser:`, `https://accounts.x.ai/oauth2/device?user_code=<4 characters>-<4
  characters>`, `Confirm this code in your browser:`, the code, and `Waiting for authorization...`, about a second after
  it started. Its debug log says the device flow was chosen by a login configuration fetched from the server (`resolved
  interactive transport transport="device" source=remote`), so the default could change without an upgrade;
  `--device-auth` asks for it explicitly, and `--oauth` was not tried. It listens on nothing. **It opens the browser
  itself through LaunchServices** (the system log showed `LAUNCH: Asking CSUI to launch 1 items` from the process in
  both runs it was watched in), with `BROWSER` set and a recording `open` first on `PATH`; `GROK_TEST_OPEN_URL_FILE`, a
  name in the executable, did not stop it either. So Octoboard cannot be the only one to open the page. It created a
  `GROK_HOME` that did not exist (`agent_id`, `docs/`, `logs/`). In the first run, nothing on the machine knowingly
  approved the code. The login still completed 5.4 seconds after the URL appeared: the browser it opened was already
  signed in to x.ai, and whether a person clicked through is not known. It wrote `auth.json` into the scratch home,
  printed `✓ Signed in as <email>` on stderr and exited 0. So completion is exit 0, with `auth.json` present in
  `GROK_HOME`. Killed while waiting, it died on SIGTERM.

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

### Before a login, on the quota path

For Antigravity CLI and GitHub Copilot CLI, the quota probe and credential-presence rules are in the respective Plan quota and sign-in measurements. Pi, OpenCode and Qwen Code's key accounts have no plan quota to probe, so sign-in is read separately.

The same probes as in "Plan quota" below, against a directory that does not exist and one created empty:

- **Claude Code does not tell the two apart, and recreates a missing directory.** Both the probe and `claude auth
  status` create the directory (`.claude.json`, `backups/`, and for the probe `plugins/` and `sessions/`) and then
  answer as for an empty one. The probe's `get_usage` succeeds, with
  `{"subscription_type":null,"rate_limits_available":false,"rate_limits":null,"behaviors":null,"session":{…}}`. That is
  presumably what an account without a subscription answers as well (not measured), so it does not say "not signed in"
  alone. The `initialize` reply in the same exchange says more: its `account` is
  `{"tokenSource":"none","apiProvider":"firstParty"}`. But `tokenSource: "none"` alone is not a sign-out. Measured on
  Claude Code 2.1.311, in scratch homes with fake keys and fake credential files (nothing was sent to a real account),
  `account` was: for `ANTHROPIC_API_KEY` in the environment, in the settings' `env`, or a key stored by `/login`,
  `{"tokenSource":"none","apiKeySource":"ANTHROPIC_API_KEY"` (or `"/login managed key"`)`,"apiProvider":"firstParty"}`;
  for an `apiKeyHelper`, `ANTHROPIC_AUTH_TOKEN` or `CLAUDE_CODE_OAUTH_TOKEN`, `tokenSource` is `"apiKeyHelper"`,
  `"ANTHROPIC_AUTH_TOKEN"` or `"CLAUDE_CODE_OAUTH_TOKEN"`; for Bedrock, Vertex or Foundry, just `{"apiProvider":"bedrock"}`
  (`"vertex"`, `"foundry"`), with no `tokenSource`; for a credentials file holding a subscription,
  `{"subscriptionType":"Claude Max","apiProvider":"firstParty"}`; and for a subscription together with a key,
  `{"tokenSource":"claude.ai","apiKeySource":"ANTHROPIC_API_KEY","apiProvider":"firstParty"}`. A custom
  `ANTHROPIC_BASE_URL` alone left it at `none`. Every one of them answered `get_usage` with `rate_limits: null` like the
  empty home (the fake subscription's too, as nothing could reach the API), so the usage reply cannot tell them apart. So not signed in means: `apiProvider`
  is `"firstParty"` (or absent), `tokenSource` is `"none"`, `apiKeySource` is absent or empty, and `subscriptionType` is
  absent. **Not measurable here:** the key a Console login keeps in the macOS Keychain (the `/login managed key` case was
  a key file in the scratch home). `claude auth status` (JSON by default) prints on stdout
  `{"loggedIn":false,"authMethod":"none","apiProvider":"firstParty",…,"configDirectory":"<dir>"}` and exits **1**;
  `--text` prints `Not logged in. Run claude auth login to authenticate.`, exit 1. The probe and `claude auth status`
  each answered in under half a second. The probe also writes `Ignoring <n> permissions.allow entries from
  .claude/settings.json: this workspace has not been trusted…` to stderr, because the home directory it runs in holds
  `~/.claude/settings.json` as a project file and the fresh directory has no trust record for it. That is noise, not an
  error.
- **Codex** keeps the two distinct. With the directory missing, `codex app-server` writes nothing on stdout and exits 1
  within a tenth of a second, its stderr ending in `Error:` and `CODEX_HOME points to "<dir>", but that path does not
  exist`, after a `WARNING: proceeding, even though we could not create PATH aliases: …` line naming the same path. It
  does not create the directory. Empty, it starts, and `account/rateLimits/read` answers
  `{"error":{"code":-32600,"message":"codex account authentication required to read rate limits"},"id":2}`. The code
  alone is JSON-RPC's generic invalid request, and the message is not specific to a home signed in to nothing: measured
  on Codex 0.163.0 in scratch homes with fake keys, the same refusal (also as `chatgpt authentication required to read
  rate limits`) came for a home holding an API key in `auth.json`, for `OPENAI_API_KEY` or `CODEX_API_KEY` in the
  environment, for a custom model provider, for Azure and for a local Ollama provider, and for Amazon Bedrock. None of
  those has a quota to read. `account/read` (params `{}`, sent after `account/rateLimits/read` as id 3) is the
  structured answer, and it tells them apart: `{"result":{"account":null,"requiresOpenaiAuth":true,"workspaceRouting":null}}`
  for the empty home, `{"account":{"type":"apiKey"},"requiresOpenaiAuth":true}` for an `auth.json` key,
  `{"account":{"type":"amazonBedrock","usesCodexManagedCredentials":false},"requiresOpenaiAuth":false}` for Amazon
  Bedrock, and `{"account":null,"requiresOpenaiAuth":false}` for the custom, Azure and Ollama providers. The
  environment variables `OPENAI_API_KEY` and `CODEX_API_KEY` alone left `account` `null` with `requiresOpenaiAuth`
  `true`, as the empty home does, so a home signed in that way reads as signed out. A live ChatGPT login is documented
  by the schema as `{"type":"chatgpt","email","planType"}`; **not measurable here** (no real login): what
  `account/read` answers for one, and for a fake ChatGPT token it answered with an error (`workspace routing discovery
  unauthorized (401)`), so an error from `account/read` is not a sign-out.
  `codex login status` prints `Not logged in` on stderr and exits 1 for the empty home, and `Error loading
  configuration: CODEX_HOME points to …` with exit 1 for the missing one. The empty home is not left empty: the app
  server writes its databases, `skills/` and `installation_id` into it.
- **Grok Build does not tell the two apart, and creates a missing directory.** `grok agent --no-leader stdio` creates it
  (`config.toml`, `README.md`, `docs/`, `logs/`, `sessions/` and lock files; on 1.0.62 also `installed-plugins/`, and on
  a home that already has a `config.toml` it appends `[marketplace] default_skills_installs_purged = true`, writing
  through a symlink) and then answers `_x.ai/billing` for either with
  `{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"Authentication required","data":"Authentication required to
  fetch billing data"}}`. The `initialize` reply in a home with no login has `agentCapabilities.auth` `{}`,
  `authMethods` `[{"id":"grok.com","name":"Grok","description":"Sign in with Grok"}]` and `_meta.defaultAuthMethodId`
  `null`. Billing is refused the same way, `-32000` `Authentication required`, in every home measured, so it cannot
  say a key is in use; the `initialize` reply can, but only for a key Grok Build accepts. On Grok Build 1.0.70 a fake
  key in `config.toml` (`api_key`) and fake `auth.json` files holding an `xai::api_key` entry in most shapes left
  `_meta.defaultAuthMethodId` `null` with `grok.com` the only method. It became `"xai.api_key"` (with `authMethods`
  offering `{"id":"xai.api_key","name":"xai.api_key","description":"XAI_API_KEY or api_key/env_key in config.toml"}`
  before `grok.com`) for an `auth.json` with `{"xai::api_key":{"key":…,"auth_mode":"api_key","create_time":…,"user_id":…}}`
  and for runs against a local mock standing in for the x.ai key check (`GET /v1/api-key`); billing was still refused with
  `-32000` in all of them. **Not measurable here:** a real API key's billing answer, and what a real `grok.com` login puts
  in these fields. The two replies come in either order (billing was refused before `initialize` was answered).
  A home whose stored sign-in has expired (grok 1.0.62) answers `_x.ai/billing` with a generic error and the cause only in `data`:
  `{"jsonrpc":"2.0","id":2,"error":{"code":-32603,"message":"Internal error","data":"Billing service error: Invalid or
  expired credentials (auth_kind=bearer, x_xai_token_auth=xai-grok-cli, upstream=PermissionDenied, reason=no auth
  context)"}}`.

So "the directory is missing" can only be told reliably by looking for the directory before the probe runs. Codex is the
only one that refuses a missing directory; Claude Code and Grok Build quietly recreate it, empty, and then report it as
not signed in. "Not signed in" has a marker for each, but the quota request alone is not it, because an API key, a cloud
provider or a custom provider is refused or empty the same way and works: Claude Code's `initialize` `account` with
`apiProvider` `firstParty` (or absent), `tokenSource` `"none"`, no `apiKeySource` and no `subscriptionType`, or
`claude auth status` exiting 1 with `loggedIn: false`; Codex's `account/rateLimits/read` refused as above **and**
`account/read` answering `account: null` with `requiresOpenaiAuth: true`; Grok Build's `-32000` `Authentication
required` billing refusal **and** an `initialize` `_meta.defaultAuthMethodId` of `null`, or for expired credentials the
`-32603` error whose `data` says `Invalid or expired credentials`. Where Codex or Grok Build refuses and the other reply
shows another sign-in, the account has no quota; where `account/read` fails or `initialize` carries no
`defaultAuthMethodId`, it is unknown and shown as a failed reading. These markers were measured on Claude Code
**2.1.311**, Codex **0.163.0** and Grok Build **1.0.70**, against scratch homes with fake keys, so a reply that matches
none of them should still be shown as a plain failure.

### What the background runs write into an account's configuration

Antigravity CLI model and quota queries use Octoboard-owned directories. GitHub Copilot CLI runs background quota and catalogue queries in private configuration. OpenCode may initialize its own local data on a CLI query. Qwen Code’s catalogue reads account settings without starting the CLI. Pi’s model listing can create a temporary lock next to the resolved credential file.

Measured on macOS on 2026-11-22 with Claude Code **2.1.330**, Codex **0.167.1** and Grok Build **1.0.92**: every run
of an agent's CLI the daemon makes on its own, each against two scratch homes, one the CLI had already been run in
interactively and one holding only a configuration file the CLI had never run against, comparing the home's
configuration files before and after (Claude Code's `.claude.json` and `settings.json`, Codex's `config.toml`, Grok
Build's `config.toml` and `trusted_folders.toml`). No run was signed in, and Grok Build's were given a fake
`GROK_CODE_XAI_API_KEY` so that none could start a sign-in.

| Run | A home the CLI has run in | A home it has never run in |
|---|---|---|
| `claude --version` | unchanged | unchanged |
| Claude Code's quota probe (`claude -p --input-format stream-json …`, `initialize` and `get_usage`) | unchanged | `.claude.json` gains its first-run keys (`firstStartTime`, `firstStartVersion`, `machineID`, `userID`, `migrationVersion`, `seenNotifications`, `pluginUsage` and two migration flags), and a `theme` kept in `.claude.json` is moved into `settings.json` |
| `claude mcp list`, `claude agents --json` | unchanged | the same first-run keys and the same move of `theme` |
| `claude auth status` (measured 2026-11-24) | unchanged | `.claude.json` gains `firstStartTime`, `firstStartVersion`, `machineID`, `migrationVersion`, `seenNotifications` and two migration flags, and a backup of it is written under `backups/` |
| Claude Code's kick (the command of "Starting a quota window", measured 2026-11-24 with one real request) | no setting changed: `.claude.json` has only its own caches refreshed (`cachedFeatureFlags`, `cachedGrowthBookFeatures`, `cachedExperimentFeatures` and their kin, `clientDataCacheSlots`), and the CLI writes a backup of `.claude.json`, its model catalog cache and its plugin sync state, all inside the config directory; no conversation record | not measured: a kick needs the account's sign-in, which a home the CLI has never run in does not hold |
| Codex's kick (`codex exec --ephemeral --ignore-user-config …`, measured 2026-11-24 with one real request) | `config.toml` unchanged; Codex writes its model cache and its log and state databases, and no session file | not measured, for the same reason |
| `codex --version`, `codex app-server` with the quota requests, `codex debug models`, `codex mcp list --json`, `codex app-server` for the MCP inventory | unchanged | unchanged |
| `grok update --check --json`, `grok mcp list --json`, `grok mcp doctor --json` | unchanged | unchanged |
| `grok --version` | not measured | unchanged: nothing in the home is written at all |
| `grok models` | unchanged | `config.toml` gains `[marketplace] default_skills_installs_purged = true` |
| `grok agent --no-leader stdio` with `initialize` and `_x.ai/billing` | unchanged | `config.toml` gains the same lines |
| Grok Build's kick (`grok -p … -m … --max-turns 1 …`, the command of "Starting a quota window") | the signed-in run below left the account's configuration unchanged | `config.toml` gains the same lines, before any request is sent: the run ended at its sign-in check, with exit code 1 |

The rows measured on 2026-11-24 used the same versions. `claude auth status` was run as the MCP inventory runs it, in a
scratch `HOME` holding nothing, then a second time there (byte for byte unchanged). The two kicks were run once each,
signed in, against the user's own default configuration directory, exactly as the daemon runs a kick for the default
account (the login shell's environment, the home directory as working directory, the provider's arguments and
environment); every file under the home directory, the agents' directories and `TMPDIR` written during the kick was
compared with what was written in a window of the same length without one, so that the agent sessions running meanwhile
could be told apart. Neither kick wrote anything outside the agent's own configuration directory.

So Grok Build's quota reading, its model listing and a headless turn each initialise a home Grok has not run in, and
append its marker to that home's `config.toml` as they do; `grok update --check --json`, `grok mcp list --json` and
`grok mcp doctor --json` do not initialise a home and leave the file alone (measured twice). The first run of Claude
Code in a home carries out its first-run set-up and whatever migrations are pending there. Once the CLI has run in a
home, none of the runs measured there changes its configuration files.

**Grok Build reads the same account from a home of its own.** `grok agent --no-leader stdio` run with `GROK_HOME`
naming a new directory that held only a copy of the account's `config.toml`, and `GROK_AUTH_PATH` naming the account's
own `auth.json`, answered `initialize` with `_meta.defaultAuthMethodId` `"cached_token"` and `_x.ai/billing` with the
account's billing period, usage and `subscription_tier`, twice running. Nothing in the account's own directory
changed, two levels deep, files and links compared before and after, lock files included: what Grok wrote (its
`README.md`, `docs/`, `bundled/`, `logs/`, `sessions/session_search.sqlite`, `models_cache.json`, `agent_id` and lock
files) landed in the new directory. A real sign-in refresh is still unobserved in this background setup. Grok Build
1.0.77 saved a refresh to the file named by `GROK_AUTH_PATH` in a session-home measurement ("Injecting Octoboard
into each agent" below).

**A real Grok Build kick writes its conversation inside the run home.** On macOS on 2026-11-24, Grok Build
1.0.92 completed one signed-in kick. The CLI used the daemon's login-shell environment, arguments and overrides,
with the user's home as its working directory and `GROK_HOME` pointing to an isolated copy of the daemon's
`agent-query-homes/grok/<account>/kick` layout. The run home held the account's copied `config.toml`. The
account's sign-in was read in place through `GROK_AUTH_PATH`. Grok wrote its new conversation, prompt history
and search database under `sessions/`, alongside its own guides, bundled assets, logs, caches, locks, metadata
and worktree database in the run home. The copied and source `config.toml` both stayed byte for byte unchanged.
Before-and-after file listings showed no changes in the source Grok home, up to three levels deep, during the kick
or during a control window of the same duration. The login was still valid, the file named by `GROK_AUTH_PATH`
stayed byte for byte unchanged, and the run recorded no attempt to take its refresh lock.

**Grok Build's first-run step, repeated, stays inside `GROK_HOME`.** A home whose `config.toml` is put back without
the marker before every run has the marker appended again each time, and the step that goes with it ("default skills
installs purged") run again. Measured with `grok agent --no-leader stdio` and the two requests above, four times in a
row, `GROK_HOME` naming a directory inside a scratch user home that also held a `~/.grok` of its own (a `config.toml`,
a skill, an `installed-plugins/registry.json`, a `bundled/manifest.json`), `~/.agents/skills`, `~/.claude/skills` and
`~/.claude.json`: every file of the scratch user home outside `GROK_HOME` was byte for byte as before, and nothing
was added there or in `TMPDIR`. Inside `GROK_HOME` the file count stayed at 37 after the first run; what grew was
`logs/unified.jsonl`, by about 10.6 KB a run, with nothing seen to cut it back.

**The login store's lock is taken beside the file `GROK_AUTH_PATH` names, not in `GROK_HOME`.** With a fake, expired
login in the shape of a real one at `<dir>/auth.json`, named by `GROK_AUTH_PATH`, and `GROK_HOME` naming another
directory, `grok agent --no-leader stdio` tried to refresh it: its log records `auth lock: attempting acquire` and
`auth lock: acquired` with the path `<dir>/auth.json.lock`, and `auth.json.lock` and `auth.json.commit.lock` appeared
in `<dir>`, none in `GROK_HOME` (the refresh itself was refused, the issuer being fake). So a run against another
home and the user's own `grok` take the same lock before they refresh the sign-in.

## Sharing a session directory between accounts

GitHub Copilot CLI links `session-state/` but keeps its index account-local. Qwen Code links `projects/`. Pi links `sessions/`. Antigravity CLI and OpenCode have one account each and offer no shared session directory. See the provider sections for the respective copy and resume behavior.

Octoboard links an account's session directory into another account's only as part of an account sharing the whole of
the default account's configuration (see "What is shared" in `docs/product/sharing-the-default-configuration.md`). This
section records the measurements of a session directory shared without the rest; linking the rest of a configuration
directory, and what changes once the databases are linked too, is under "Sharing a configuration directory between
accounts" below. Codex and Claude Code lose or split conversations in a shared directory, in ways they give no warning
of, and Grok Build has no guard against two writers:

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

GitHub Copilot CLI shares eligible settings, MCP configuration and instructions, with credentials and index kept account-local. Qwen Code shares `QWEN.md` and skills, while extension settings can write an `.env` through a linked extensions directory, so extensions stay account-local. Pi shares eligible settings and extensions separately from model, MCP and sign-in files. Antigravity CLI and OpenCode offer no configuration sharing because each has a single supported account. See the individual provider measurements for eligible paths.

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
- **Grok Build follows the whole chain** (1.0.62, measured 2026-10-21, scratch homes, a fake API key, the model endpoint
  on `127.0.0.1`). Default directory D held the files; account A linked `AGENTS.md`, `pager.toml` and `disabled-hooks`
  at D's resolved paths; a per-session farm linked those names either at D's files (one link) or at A's entries (two
  links). In both layouts the model's `write` of `AGENTS.md` and of `pager.toml`, and its `search_replace` of
  `AGENTS.md`, replaced D's file (new inode) and left A's link and the farm's link in place — the farm's still pointing
  at A when the layout was two links. `/hooks` disable wrote D's `disabled-hooks` in place (same inode); `/hooks` enable
  replaced D's file (new inode, emptied). A's link and the farm's link stayed, including when the farm pointed at A. The
  one-link results in the table above still hold. A two-link chain was not measured for `config.toml`,
  `trusted_folders.toml` or `/settings` as a `pager.toml` writer. `/theme` writes `[ui] theme` into `config.toml`, not
  into `pager.toml`.
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
- **A plugin installed under a per-session home is recorded by that home's path** (1.0.92, measured 2026-11-22, scratch
  homes, a fake API key, no model turn). With `GROK_HOME` a directory whose `installed-plugins/` is a link to a source
  home's, as a per-session home's is, `grok plugin install <git URL or path> --trust` put the plugin's files and
  `registry.json` in the source home's `installed-plugins/`, through the link, and wrote the entry's `path` as
  `<GROK_HOME>/installed-plugins/<name>-<hash>`, for a git source and for a local one alike; `[plugins] enabled` went
  into that home's own `config.toml` copy. With that directory removed, the source home's own Grok still listed both
  plugins, and `grok plugin update` failed for the git one with `repo directory not found: <the removed path>`. After
  the session's process exits, Octoboard points such an entry at `<source home>/installed-plugins/<name>-<hash>`, the
  path Grok records when run against the source home, and before each launch points what an earlier process left at the
  resolved path of the directory the plugins are in. Not measured: whether a Grok process that stays up (the TUI) keeps
  the registry in memory and writes it back whole at a later install, which would put a rewritten path back until the
  next of those two moments. With a source home that has no `installed-plugins/`, the per-session home gets a directory
  of its own: the plugin's files and the registry are that home's, nothing reaches the source home, and both are gone
  with the session, while `[plugins] enabled` named the plugins in the copy all the same (Octoboard does not carry the
  plugin switches out of such a home). A home Grok 1.0.62 or later has run against has the directory.
- **Grok's `disabled-hooks` is keyed by hook source**, e.g. `global/octoboard:session_start[0].hooks[0]`. Switching
  Octoboard's hook off in one session writes the entry into the source home's file, silencing it in every later session
  from that home; with the file linked, that is every sharing account.
- **Grok's sandbox mode refuses a linked tree** (1.0.62, measured in a per-session home built over a home with
  `bundled/`). `grok --sandbox workspace` refuses to start when `hooks/`, `hooks-paths` or `bundled/` is a symlink (the
  guide adds `bundles/` and a symlinked `GROK_HOME`). Sandbox mode is off by default. The rest is under "Injecting
  Octoboard into each agent" below.

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
- **When a registry takes the installing directory's spelling** (measured 2026-11-22 on Claude Code 2.1.330, Codex
  0.167.1 and Grok Build 1.0.92, scratch homes, an account directory A whose entries link into a default directory D, a
  local marketplace and plugin, no login, no model turn).
  - *Claude Code:* `claude plugin install` and `claude plugin update` run under A each write `installPath` in D's
    `plugins/installed_plugins.json` as `<A>/plugins/cache/<marketplace>/<plugin>/<version>`; an update replaces a path
    already spelled through D with A's spelling again. `plugin list` and `plugin marketplace update` under A leave a
    path spelled through D as it is. A marketplace added from a local directory records that directory, not the
    config directory, in `known_marketplaces.json` (`installLocation`) and in `settings.json`
    (`extraKnownMarketplaces`); a marketplace cloned from a repository was not re-measured on this version (the clone
    needs a `https://` or GitHub source). Nothing else in D named A: `settings.json` `enabledPlugins` holds names, and
    `plugins/plugin-directory-cache-v2.json`, which `plugin marketplace update` writes, holds none. With `installPath`
    leading nowhere, the default account still listed the plugin as enabled and read it from the local marketplace's
    source.
  - *Grok Build:* `grok plugin install <path or git URL> --trust` under A writes `path` in D's
    `installed-plugins/registry.json` as `<A>/installed-plugins/<name>-<hash>`, for a local source (installed as a
    link to the source) and for a git one. `grok plugin list` and `grok plugin update` under A keep a `path` already
    spelled through D, an update that fetched a new commit included. The same holds for any `GROK_HOME` whose
    `installed-plugins/` is a link, which is what a per-session home is: the registry takes that home's spelling.
    Nothing else in D named A (`config.toml` holds `[plugins] enabled` names and a marketplace source's own path).
    With `path` leading nowhere, the default account still listed both plugins, a plugin from a git source was no
    longer loaded (`grok inspect`) and `grok plugin update` failed for it with `repo directory not found`; a plugin
    installed from a local path, being a link, still loaded.
  - *Codex:* `codex plugin marketplace add <path>` and `codex plugin add <plugin>@<marketplace>` under A write
    `[marketplaces.<name>]` (`source_type`, `source`) and `[plugins."<plugin>@<marketplace>"] enabled` into D's
    `config.toml` and copy the plugin into D's `plugins/cache/`; no file in D names A. A marketplace from a
    repository keeps its snapshot under `<CODEX_HOME>/.tmp/marketplaces`, which is not shared; what `config.toml`
    records for one was not measured (the source has to be a GitHub repository or a git URL that these builds did not
    take from a loopback server).

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

- **The write-ahead log sits beside the target** (macOS). With `A/state_5.sqlite` linked to `D/state_5.sqlite`, no
  `-wal` or `-shm` appeared in A, and `lsof` showed D's database and both companions open.
- **Not on Windows** (Windows 11; SQLite 3.46.0 as bundled by `rusqlite`, and the system's `winsqlite3` 3.43.2; Codex
  itself not run). SQLite there names the `-wal` and `-shm` from the path a database is opened by, without resolving a
  file symbolic link: two processes, one opening the link and one the target, each kept a log of their own over the one
  file, never saw each other's rows, and after both closed only one side's rows remained (`integrity_check` stayed ok).
  So a thread database is not shared through a link on Windows.
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

### Residue the probes leave outside the scratch directory

- **Grok** writes lock-slot files to `/tmp/grok-file-lock-<uid>/` whatever `TMPDIR` says.
- **Claude Code** creates per-working-directory temp directories in `/tmp/claude-<uid>/` despite `TMPDIR`. Its first
  TUI run from a throwaway `HOME` also registered `<HOME>/Applications/Claude Code URL Handler.app` as a system-wide
  `claude-cli://` handler with LaunchServices (`lsregister -u` removes it); setting `disableDeepLinkRegistration` to
  `"disable"` in the settings stops later runs from doing it.
- **Codex** makes every TUI start in a scratch home launch `<home>/packages/app-server-daemon/…/codex app-server
  --managed-daemon` (parent pid 1), which outlives the TUI and copies about 323 MB into that home's `packages/`. Its
  sockets in `/tmp/codex-daemon-<uid>/` are removed on exit.

### Not verified

Claude Code: the model's retry of a refused Edit or Write on the link's target; synced-skill buckets across accounts;
whether the history lock is shared across accounts; the retention prune of `history.jsonl` (it never ran, even in a
control); loading of `workflows/`, `themes/`, `hooks/`, `agent-memory/` and `loop.md` through a link; the install
refusal for a symlinked `plugins/`; Claude Code 2.1.274. Codex: a link on a future `state_6.sqlite`; `plugins/`,
`agents/`, `themes/`, `hooks.json` through a link; whether rollouts reference `generated_images/`; a rule's effect on a
real tool call; whether a daemon holding a thread notices another home changing the shared row; `prompts/` on other
versions. Grok: two processes on one session; loading of `personas/`, `workflows/`, `lsp.json`, `settings.json`,
`sandbox.toml`, `statusline.sh`.

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

- **A compressed rollout resumes in place, and Codex decompresses it** (see "Codex, a rollout it has compressed" under
  "Moving a conversation to another config directory" above). So a `.zst` under `sessions/` counts as a present
  record.
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

Claude Code, Codex and Grok Build expose plan quotas through interfaces for their own front ends. Their quota
interfaces were marked experimental at the measured versions. What Octoboard does with a reading is under "How a quota is read" and "What
reading a quota leaves behind" in `docs/product/account-quota-readings.md`. Measured against the default account of each on
Claude Code 2.1.302, Codex 0.162.1 and Grok Build 1.0.60, with the working directory set to the home directory and the
agent's configuration-directory variable (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GROK_HOME`) set to a pinned account's
directory, or left alone for the default account. The CLI handles its own login, Keychain access and token refresh.

The other agents’ quota measurements are in each agent’s section: GitHub Copilot CLI's free plan
answers through its SDK server and Antigravity CLI's through `agy -p "/usage"`, both measured to run in a directory
that is not the user's; Qwen Code has no quota for an account that signs in with a key; Pi and OpenCode have no plan
of their own.

Codex and Grok Build want **stdin kept open until the reply is on stdout**: they exit silently when it closes first.
Claude Code answers with stdin open as well, so all three are asked the same way. Each answers in about one to three
seconds, though a Claude Code start (2.1.308 and 2.1.309) took up to 17 seconds on a 10-core machine at a load average
of about 54.

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
{"jsonrpc":"2.0","id":3,"method":"account/read","params":{}}
```

The `account/read` request (id 3) is read only where `account/rateLimits/read` is refused with `authentication required
to read rate limits` (see "Before a login, on the quota path"). Notifications (`remoteControl/status/changed`, `account/updated`) arrive between the replies and are passed over. The
reply to id 2 is `{"id":2,"result":{...}}`, or `{"id":2,"error":{"code","message"}}`. `result.rateLimits` is
`{limitId:"codex", primary, secondary, credits, planType, ...}` where `primary` and `secondary` are
`{usedPercent, windowDurationMins, resetsAt}` — `300` and `10080` minutes on the measured account, `resetsAt` in
Unix **seconds** — and `planType` is `"plus"`. `result.rateLimitsByLimitId` repeats the same under each limit id. The
schema is available from `codex app-server generate-json-schema` (`v2/GetAccountRateLimitsResponse.json`).

The same reply carries the account's **rate limit reset credits** beside `rateLimits`, as `result.rateLimitResetCredits`
(measured on codex-cli 0.163.0, on an account holding one):

```
{"availableCount":1,"credits":[{"id":"RateLimitResetCredit_462e…","resetType":"codexRateLimits","status":"available",
"grantedAt":1791413156,"expiresAt":1794005156,"title":"Full reset (Weekly + 5 hr)",
"description":"Thanks for using Codex! You've been granted one free rate limit reset."}]}
```

From the schema: `availableCount` is an integer and the only required member; `credits` is a list or `null`, where
`null` means only the count is known, and the list may be capped below `availableCount`; the whole field may be `null`
or absent. A credit has `id`, `resetType` (`codexRateLimits` or `unknown`), `status` (`available`, `redeeming`,
`redeemed` or `unknown`), `grantedAt` and `expiresAt` in Unix **seconds** (`expiresAt` is `null` for a credit that does
not expire) and `title` and `description`, both nullable. The read request also takes `excludeResetCreditDetails` to get
the count alone; Octoboard sends no params, so the details come back. Claude Code's and Grok Build's replies carry
nothing like it.

**Using a reset credit** is a further request on the same app server, started and bounded like the probe:

```
{"jsonrpc":"2.0","id":2,"method":"account/rateLimitResetCredit/consume","params":{"creditId":"RateLimitResetCredit_…","idempotencyKey":"<uuid>"}}
```

`creditId` is optional: left out, the backend picks the next available credit. `idempotencyKey` is required, a UUID;
the same value is reused when the same attempt is retried, and a new one for a new attempt. The reply is
`{"id":2,"result":{"outcome":…}}` with `outcome` one of `reset` (the credit was consumed and the eligible windows were
reset), `nothingToReset` (no current window is eligible), `noCredit` (none available) or `alreadyRedeemed` (that
idempotency key already succeeded). Whether `nothingToReset` keeps the credit was not measured: the schema does not say,
and using a credit is irreversible, so it was not run against an account holding one. An account signed in to nothing
(an empty `CODEX_HOME`, measured) is refused with `{"code":-32600,"message":"codex account authentication required for
rate limit reset credits"}`.

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
(a float, `100.0` on the measured account; left out altogether while nothing of the current period is used, as
measured right after a weekly reset), `config.currentPeriod` `{type:"USAGE_PERIOD_TYPE_WEEKLY", start, end}`
(ISO 8601), and spend fields (`onDemandCap`, `onDemandUsed`, `prepaidBalance`) that are not a quota window. There is
one credit pool, not a session and a weekly window.

**What a probe touches.** A probe is run repeatedly per account (see "When quotas are read" in
`docs/product/account-quota-readings.md`), so what a start does matters. Measured on the same builds by listing the files each
probe changed in the agent's home and the processes alive in its process group when the reply arrived.

- **Claude Code** syncs plugins on start (it rewrites `plugins/synced/…` and a `.in_use` marker per installed plugin
  version) and registers the process in `sessions/<pid>.json` with a `.key` file. `--setting-sources local` stopped the
  plugin writes, but it also drops the user's `settings.json`, which can carry `apiKeyHelper`, `env` (API key or auth
  token, base URL, Bedrock or Vertex, proxy, extra CA certificates) and `forceLoginMethod`, so a probe could fail or
  read a different identity than the account's sessions use; the probe therefore leaves settings loading as it is. An
  `enabledPlugins` override through `--settings` did not stop the sync. `--strict-mcp-config` with an empty
  `--mcp-config` does keep the user's MCP servers from starting. So the plugin sync and the `sessions/` registration
  stay. A probe that exits by itself once its stdin closes removes its own registration (measured with 2.1.316: an empty
  `sessions/` after the exit); one that is killed leaves it until a later Claude Code start clears a registration whose
  process is gone. No session record is written, with `--no-session-persistence`.
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

### The sign-in email

None of the three probes above names the address an account is signed in with, so it is read from the file where each
CLI keeps its sign-in. These are the CLIs' own files, not interfaces they publish; the fields were seen on a real
signed-in account of each (Claude Code 2.1.312, Codex 0.163.0, Grok Build 1.0.70), by name only:

- **Claude Code**: `oauthAccount.emailAddress` in `.claude.json`, which beside `emailAddress` holds `displayName`,
  `organizationName` and the like. With `CLAUDE_CONFIG_DIR` set the file is `$CLAUDE_CONFIG_DIR/.claude.json`; without
  it, `~/.claude.json` — not inside `~/.claude/`.
- **Codex**: the `email` claim of the JWT in `tokens.id_token` of `$CODEX_HOME/auth.json` (`~/.codex` by default), with
  the claim under `https://api.openai.com/profile` as a fallback. The app server's `account/read` documents
  `{"type":"chatgpt","email","planType"}` for a ChatGPT login, but what it answers for a real login was not measured.
  A login kept in the system keyring rather than the file (`cli_auth_credentials_store`) leaves no file to read.
- **Grok Build**: `$GROK_HOME/auth.json` (`~/.grok` by default) is an object keyed `https://auth.x.ai::<uuid>`, each
  entry holding `email`, `first_name`, `user_id` and the token. Which entry the CLI uses when there are several was not
  established. A finished `grok login` also prints `✓ Signed in as <email>` on stderr.

## Starting a quota window

The measured kick commands below apply to Claude Code, Codex and Grok Build. GitHub Copilot CLI reports a fixed monthly reset rather than a first-use start. Antigravity CLI reports its two model groups’ weekly reset times without a start action. Pi and OpenCode have no plan quota, and Qwen Code’s key accounts report local use only.

A first-use window starts counting at the account’s first request after it resets; until then the CLI reports it with no
reset time, except Codex, which reports a placeholder reset (see the Codex measurement at the end of this section). A
**kick** is one one-word request on that account that starts it. How Octoboard runs one (the environment, the time and
output limits, what counts as success) is under "How a kick runs" in `docs/product/starting-usage-windows.md`. Each
agent is run with stdin closed and its reply not read. Each takes the cheapest model its catalogue offers and its lowest
reasoning effort, and is made as small as the CLI allows: a one-line system prompt in place of its own, no tools, one
turn, a capped reply, and as little of the user's setup as can be left out without changing which sign-in is used. No
agent is run with an option that changes the credentials it reads (Claude Code's `--bare` skips the Keychain).

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
stand-in endpoint on a scratch configuration directory with a fake key (so nothing reached the provider); sizes are the
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
Grok Build window reports no reset time were not measured then (Codex was measured later, below).

With a model it does not know, Claude Code's print mode wrote its failure to standard output, not error output (measured
on **Claude Code 2.1.322** on 2026-11-12, from the home directory): it exited with status 1, while its error output held
only warnings, ending in the untrusted-workspace notice about the home directory's `.claude/settings.json`. Where a
quota refusal goes was not measured.

An unstarted Codex window does not report an empty reset time (measured on **Codex 0.167.1** on 2026-11-12, reading
`account/rateLimits/read` every 20 seconds for 15 minutes after the 5-hour window reset, with no request made on the
account): the window was listed with `usedPercent` 0 and a `resetsAt` exactly `windowDurationMins` after the moment of
each answer, so it moved forward with every reading. So whether a window is counting cannot be read from one reading for
every agent; Octoboard compares readings over time (a reset that keeps its distance from now is not counting), as
"Starting a usage window that has not begun counting" in `docs/product/starting-usage-windows.md` describes. Whether an
unstarted Grok Build window reports a reset time is still not measured.

## Injecting Octoboard into each agent

| Agent | Per-launch injection |
|---|---|
| Antigravity CLI | A private `--gemini_dir` holds the merged hooks, MCP configuration and role text. The CLI identifies its transcript after launch. |
| GitHub Copilot CLI | A launch plugin supplies hooks, `--additional-mcp-config` supplies the server and `COPILOT_CUSTOM_INSTRUCTIONS_DIRS` includes the role. The launch supplies the session ID. |
| OpenCode | The launch gives its plugin, MCP declaration and role instructions through the inline configuration. The attached session supplies the conversation ID. |
| Qwen Code | A launch system settings file carries hooks. `--mcp-config` supplies the server and `--append-system-prompt` supplies the role. The launch supplies the session ID. |
| Pi | A launch extension reports events and provides the Octoboard tools, with the role appended to the system prompt. The launch supplies the session ID. |

The individual sections give the initialization flags, merge rules and platform differences.

How each agent is given Octoboard's status hooks, its MCP server and a role description, with the condition each one
brings. Everything is per launch (arguments, an environment variable, or files Octoboard writes itself) and never an
edit of the project's or the user's files, and was exercised against the versions above. What a user sees of it is in
"What is injected on every launch" in `docs/product/launching-agents.md` and in "How each agent is given the injection"
in `docs/product/per-agent-launch-specifics.md`.

| Capability | Claude Code | Codex | Grok Build |
|---|---|---|---|
| Launch with an initial task | `claude "<task>"` | `codex "<task>"` | `grok "<task>"` |
| Pre-allocate a session id | `--session-id <uuid>` | **Not possible.** Take it from the `SessionStart` payload; the interactive thread is created lazily on the first prompt, so a session opened without a task has no id until the user types | `--session-id <uuid>` (new sessions only) |
| Inject hooks | `--settings <json-or-path>`, merged with the project settings; for a project session under **Keep sessions to their own project**, a second, synchronous `PreToolUse` group (matcher `Edit\|Write\|MultiEdit\|NotebookEdit`, a POSIX command line ending in `exit 0`) beside the status hooks | `-c 'hooks.<Event>=[{hooks=[{type="command",command=…,timeout=3 (15 on Windows, except 3 for `Interrupt`),async=true}]}]'`, one per event, plus the hook-trust step; the guard is a second `PreToolUse` group (matcher `apply_patch`, `timeout=10`, no `async`) in the same `hooks.PreToolUse` override, run through the per-session script and trusted on a private-server resume | A per-session `GROK_HOME` whose `hooks/` directory is Octoboard's and whose other entries mostly symlink to the source home; the guard is a second `PreToolUse` group in its `hooks/octoboard.json` |
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
  sessions stay resumable from the user's own `grok`. Where the host makes no file links (Windows without Developer
  Mode or elevation), directories are still linked, the other files are copies with lock and temporary files left out,
  and `auth.json` is left out too: `GROK_AUTH_PATH` names the login store in its place. Grok Build 1.0.77 (macOS) read
  its login from the file that variable named, with an empty `GROK_HOME` (`grok models` listed the signed-in models,
  and the signed-out ones without it), and created neither `auth.json` nor `auth.json.lock` in the home; whether a
  refresh or a sign-in writes back to that file, and where it takes `auth.json.lock` then, was not measured there (on
  Windows it was: "Grok Build's session home without file links"). The
  mechanism cannot run under Grok's sandbox (Grok Build 1.0.62, macOS):
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

  Octoboard's refusal of such a launch is under "Grok's own sandbox" in `docs/product/per-agent-launch-specifics.md`.
- **Codex** gates hooks behind a persisted trust hash: without it an interactive session raises a blocking review modal
  and a headless one **hangs indefinitely**. `--dangerously-bypass-hook-trust` clears that at the cost of two warning
  lines per launch. The hash is taken over the handler definition, which includes the hook command, and that command is
  the session's own script path, so hashes captured once could never match a later session. Removing the warning would
  mean a session-independent hook command. Project trust is not passed: a per-launch `projects` override is saved
  nowhere, so Codex asks through its own confirmation (see "Folder-trust confirmations" below).
- **Workspace trust gates the project's own configuration on Claude Code and Grok**, and fails silently. On Grok an
  untrusted folder makes the project's `AGENTS.md`, hooks and MCP servers not load; trust lives inside `GROK_HOME` (an
  undocumented `--trust` flag also exists), and Grok's project hooks, unlike its instruction files, also need a
  recognized **git** root, since they did not load in a trusted non-git directory. What Octoboard does about each
  confirmation is under "How a confirmation is pressed" in `docs/product/folder-trust.md`. On Claude Code an untrusted
  workspace makes the project's `allow` rules ignored (with a line on stderr) while `deny` still applies, so the session
  is only ever more restrictive. Its trust lives in the global config file (`~/.claude.json`, or `.claude.json` inside
  the config directory in effect). Octoboard reads it, rather than matching the stderr line, because on a PTY stderr is
  the rendered UI, and matching rendered text would need a VT emulator in the daemon (see "Claude Code" in
  `docs/product/per-agent-launch-specifics.md` for what it reads and when it warns).
- **Each agent reads a different instruction filename, matched by exact spelling.** Claude Code reads
  `CLAUDE.md` and `CLAUDE.local.md`, and falls back to a project's `AGENTS.md` only when the project has no
  `CLAUDE.md` (2.1.319; see "Claude Code" above), so beside a `CLAUDE.md` it does not read `AGENTS.md` on its
  own; a line `@AGENTS.md` in `CLAUDE.md` imports it, with no confirmation for a file inside the working
  directory (its "Allow external CLAUDE.md file imports?" dialog is for files outside it), and the request
  then carries a block for each file with the text once (Claude Code 2.1.319, `-p` with Octoboard's
  `--settings`, `--append-system-prompt` and `--mcp-config` against a local stub); where it must be shown a
  project's `AGENTS.md`, `--append-system-prompt-file` lands it in the system prompt. Codex reads `AGENTS.md`.
  Grok reads `AGENTS.md`, `Agents.md`, `CLAUDE.md`, `Claude.md` and `CLAUDE.local.md`, and on a case-sensitive
  volume loads every matching spelling present. No agent accepts an all-lowercase name. Folder trust, not a
  git repository, is what gates Grok's instruction files: in a trusted folder that is not a repository Grok
  reads `AGENTS.md` and `CLAUDE.md` from the working directory alone (Grok's own guide says outside a
  repository only the working directory is read), and an untrusted one loads neither (measured on Grok Build
  1.0.52). Its project *hooks* are the part that still needs a git root. Grok loads one file reached by two
  names once when the second is a symlink to the first, and a pair of separate identical files twice, a
  hard-linked pair included; a `CLAUDE.md` holding the line `@AGENTS.md` reaches the model as that line, not
  expanded (Grok Build 1.0.77, `grok inspect --json` and a local stub). Codex 0.166.0 reads only `AGENTS.md`
  beside any `CLAUDE.md` (`codex debug prompt-input`). So a console's working directory holds the canonical
  content in `octoboard-instructions.md`, with each provider's declared instruction filename a relative
  symlink to it. Where the host makes no file links, `CLAUDE.md` imports `@octoboard-instructions.md` and
  `AGENTS.md` holds a copy. Every agent's console session reads the same text once. Claude Code follows the
  symlink, and replacing the target by a temporary file and an atomic rename leaves the link working, with the
  next launch seeing the new content (Claude Code 2.1.300). The file's text therefore names the tools without
  any agent's prefix, since each agent shows them under its own.
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
  changed role text is ignored on resume. A session's role text is immutable for its lifetime. Grok also freezes its
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
  Octoboard knows of. The binary's own default-config snapshot reads `project_doc_max_bytes = 32768` (0.162.1 and
  0.167.1), and the key is a documented `config.toml` setting. Measured on Codex 0.167.1 with `codex exec` from a
  scratch `CODEX_HOME`, a fake key and a local stand-in endpoint that recorded the request, the file generated by
  `console_session_instructions` as the working directory's `AGENTS.md`, outside any git repository:
  - With the asking guidance, the language guidance (`zh-Hans`) and a memory index filled to its limit (4,050 bytes
    of index, the line saying how many more entries there are included), the file holds 27,002 bytes before any user
    section: 20,723 bytes of built-in text and 6,279 bytes of memory section. A user section adds 397 bytes of heading
    and tags to the user's text.
  - Recounted from the current `console_session_instructions` (not run through Codex again): with the same guidance
    (`zh-CN`) and the index filled to its limit (4,095 bytes of index lines), the file holds 27,760 bytes before any
    user section, 21,247 of built-in text and 6,513 of memory section; a user section adds 398 bytes to the user's
    text. A test keeps the built-in text with every addition at its fullest, plus an empty user section's heading and
    tags, within 28 KiB, leaving at least 4 KiB of user text under the cap.
  - With 4,167 bytes of user text, the 31,566-byte file reached the model whole, its last line included.
  - A 34,086-byte variant was cut silently: the request carried the first 32,768 bytes of it, the text ended
    mid-line, and nothing in the output said so.
  - A global `$CODEX_HOME/AGENTS.md` does not share the budget. With a 3,000-byte one beside the 31,566-byte file,
    both reached the model whole.

  So in that arrangement a console's own text fits up to about 4,600 bytes (32,768 less 27,760 less 398), or about
  1,530 characters of Chinese at three bytes each. Past that the end of it is lost for Codex alone. Octoboard sets no limit on the user's text.
- **Hook scripts must fail silently and fast on all three.** Every agent surfaces a failing hook, and a hook with no
  timeout blocks the turn for its full duration. Octoboard's scripts exit 0 unconditionally, write nothing to stderr,
  set a short explicit timeout (3 s; command hooks 15 s on Windows, HTTP hooks and Codex's `Interrupt` 3 s everywhere, where the first hook of a launch took 4.6 s to start under Codex's `pwsh`; Codex clamps some events to 3 s), are asynchronous where the agent allows it, and give
  the daemon call a hard deadline. Octoboard's Grok hooks are `command` type because Grok's SSRF protection was found to
  reject plain HTTP and private addresses, though a later probe on 1.0.77 reached `http://127.0.0.1:<port>` (see Grok's
  "A `PreToolUse` deny blocks a write" above). The guard's behavior is in "How each agent is given the injection" in
  `docs/product/per-agent-launch-specifics.md`.
- Injection never modifies project files or the user's global configuration; where an agent can only be configured
  through a file, a "use this config directory" variable or flag is preferred.

### Folder-trust confirmations

Antigravity CLI’s Enter on the trust option writes to its private launch configuration. GitHub Copilot CLI’s first option approves this session without recording trust. Qwen Code asks only with folder trust enabled and can use a temporary trust file for a launch. Pi asks when there are project resources to load and has a session-only trust choice. See their sections for what answering each screen writes.

Some agents gate the prompt on a folder-trust confirmation. Qwen Code can report startup before asking. OpenCode shows no such confirmation.
What Octoboard sends to answer each is under "How a confirmation is pressed" in `docs/product/folder-trust.md`.

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
  cover more than the folder asked about. A trusted directory above the repository covers nothing.
  Trust is decided separately for each directory from the repository root down to the working directory (0.166.0,
  `codex doctor --json` in a scratch `HOME`/`CODEX_HOME` with a repository, a subdirectory layer and a linked
  worktree). For each directory the first entry that applies decides: its own `projects."<dir>"` entry, else that of the
  nearest `.git` directory (a linked worktree's own top), else that of the main repository root (the parent of `git rev-parse
  --git-common-dir`). Entries without `trust_level` are skipped, and only `"trusted"` trusts. Measured, with `M` the
  main root, `M/sub` a layer below it and `L` a linked worktree: `M` trusted and `M/sub` untrusted trusts `M` only; `M`
  trusted and an `M/sub` entry without `trust_level` trusts both; `M` untrusted and `M/sub` trusted trusts `M/sub` only;
  only `M/sub` trusted trusts `M/sub` only; `L` trusted with the working directory `L/sub` trusts both of `L`'s layers;
  `M` trusted and `L` untrusted trusts nothing.
  No hook fires before or right after the confirmation, and none at startup at all until the first prompt is submitted.
  Codex sets no terminal title before the confirmation or the sign-in screen that precedes it on an account's first
  launch; its first title (OSC 0/2) comes once its session starts (0.160.0–0.163.0), so that title marks it past both.
  With `[tui] terminal_title = []` it never sets one (see "Codex" in `docs/product/per-agent-launch-specifics.md` for
  the override Octoboard passes). The ">_ OpenAI Codex (v…)" banner is drawn before the sign-in screen and the
  confirmation alike, so it marks neither. Codex redraws only the cells that changed, so text read off the screen can
  lose characters wherever the frame before held the same one; the confirmation's own phrases came through intact in its
  first drawing, a path above it did not. The review Codex raises for new hooks without
  `--dangerously-bypass-hook-trust` reads "Hooks need review … 2. Trust all and continue" and is a different modal. On
  0.161.0 a key the confirmation ignores (a letter, Right, Tab, `?`) draws nothing but a bare synchronized-update frame
  (`ESC[?2026h ESC[39m ESC[49m ESC[0m ESC[?2026l`); accepting it clears the screen and redraws it whole, starting with
  the ">_ OpenAI Codex (v…)" banner, which is how an answer given in the terminal is told apart.
- **Grok Build** shows its confirmation only when the folder holds content its trust gates, such as an `AGENTS.md`,
  whether or not it is a git repository, so a console directory holding an instruction file is asked about too. The
  screen reads "Do you trust the contents of this directory?", "Grok Build may run or modify contents in this directory,
  posing security risks.", "Yes, proceed y" and "No, quit n", with no cursor; `y` or Enter accepts, and a decline exits.
  Grok writes `[folders."<canonical cwd>"]` with `trusted = true` and `decided_at` (an integer of Unix seconds) into
  `$GROK_HOME/trusted_folders.toml`; on 1.0.62, if the existing store does not parse (a string `decided_at`, say), an
  accepted confirmation fails with "Couldn't save folder trust: the trust store could not be read…" and Grok exits. It
  saves by replacing the file, so against Octoboard's per-session `GROK_HOME` the entry lands only in the session's copy
  and a symlink there is replaced rather than followed; no setting moves the store; how Octoboard carries the entry over
  is under "Carrying Grok Build's trust record" in `docs/product/folder-trust.md`. Grok locks the
  `trusted_folders.toml.lock` file kept beside the store before it replaces the store (measured on 1.0.50: with an
  exclusive `flock` held on it elsewhere, an accepted confirmation wrote nothing until it was released). `SessionStart`
  fires right after the press. While it waits, Grok animates its logo at about 2 KB/s, so the first drawing soon leaves
  any window of recent output and the terminal is never quiet.

**Trusting a console's directory for one launch** (measured on macOS on 2026-11-22, each in a scratch home with a fake
key and no prompt sent, in a directory shaped like a console's, `<home>/.octoboard/consoles/<id>` holding an
instruction file). A console's directory is Octoboard's own, and the user is never asked about it, so the question is
whether one launch can be trusted there without the agent recording the directory in the user's trust list.

- **Claude Code 2.1.330 has no documented way.** Its `--help` names none for an interactive launch: the trust screen
  is skipped only without a terminal (`-p`, or output that is not a TTY), and `--trust-workspace` belongs to
  `claude ultrareview` alone. Read out of the executable, the workspace counts as trusted for the process when the
  environment variable `CLAUDE_CODE_SANDBOXED` is set, before any record is looked at; with `CLAUDE_CODE_SANDBOXED=1`
  the launch drew no trust screen, came up at its prompt with the directory's `CLAUDE.md` listed under `/memory` as
  the project's instructions, and left `projects["<dir>"].hasTrustDialogAccepted` at `false` in the global config
  file. The variable appears in no documentation and says the whole process runs sandboxed: the same check that
  trusts the working directory also lets the permission rules and additional directories of any project's settings
  apply, in a session that has moved to another folder included, so it trusts more than the console's directory. A
  directory outside a git repository is also trusted through any trusted directory above it, but that takes an entry
  in the user's own list. Without the variable the screen comes up as for any other folder.
- **Codex 0.167.1 does not ask outside a git repository**, as on the earlier versions above: the launch in the
  console-shaped directory went straight to its composer and recorded nothing about trust (it added `[tui]
  screen_reader_detection_done = true` to a `config.toml` it had not run against before, which is unrelated to trust).
  Where a repository holds the directory (the directory made inside a fresh `git init`), it asks, in the folder-access
  form ("Note: You're in a subdirectory of a Git project. Trusting will apply to the repository root: …", "1. Trust and
  continue", "2. Back to Agent Command Center"), and a per-launch override does not answer it: with `-c
  'projects."<directory>".trust_level="trusted"'` and with the same override naming the repository's root the
  confirmation still came up, and `config.toml` was left as it was.
- **Grok Build 1.0.92 takes an entry in the store it reads.** With `[folders."<canonical cwd>"]`, `trusted = true` and
  `decided_at` (Unix seconds) already in `$GROK_HOME/trusted_folders.toml`, the launch drew no confirmation and left
  the store byte for byte as it was; without the entry the confirmation came up as described above. Against a
  session's own `GROK_HOME` that store is the session's copy, so an entry written there before the launch trusts the
  directory for that launch and is gone with the session's home.

### Writing into a running session

Pi can accept plain text on the launch extension’s channel without disturbing the editor draft. OpenCode’s private server can steer a running conversation and reports when a prompt reaches a turn. GitHub Copilot CLI and Qwen Code have native queues whose held messages may be dropped on cancellation. Antigravity CLI uses its terminal and reports turn activity separately. See each provider’s measurements for terminal paste, image attachments, command completion and interruption.

**A terminal paste submits any existing draft too.** With Claude Code 2.1.274 and 2.1.314, and Grok Build 1.0.70,
typing a draft without Enter and then writing a bracketed paste followed by CR submits the draft and the pasted
message together. Isolated runs with fake keys and local HTTP endpoints confirmed both markers in the same user
message in the actual inference request. A terminal input lock prevents byte interleaving, but does not separate a
message from text already in the composer. Reading an empty composer is not sufficient either: user input can have
reached the PTY before the agent renders it.

Grok Build 1.0.70 reports the submitted text in the top-level `prompt` field of its `UserPromptSubmit` hook. A
181-line, 6,694-character prompt reached that field unchanged, including 180 LF separators, without a paste wrapper.
Not every Enter submits a prompt, and background wakes also produce this hook, so an Enter count alone cannot
associate a hook with a person's input.

Codex 0.163.0 also reports a top-level `prompt` in `UserPromptSubmit`. With the same `-c` injection style as an
Octoboard launch, a long multi-line paste and one with a blank first line reached that field without a paste
wrapper; leading and trailing whitespace was trimmed, while inner LF separators were preserved.

**Claude Code and Grok Build can stash their own editor state.** Claude Code 2.1.274 and 2.1.314, and Grok
Build 1.0.70, accept Ctrl-S with a nonempty composer, show a stash indicator, and leave an empty composer. After a
separate bracketed paste and CR, they automatically restore the saved draft, even while the new model request is
still running. Do not press Ctrl-S again after submission: it toggles the stash and would hide the restored draft.
A second stash can replace the person's existing stash, so an occupied stash must be left alone. In Grok, sending
Ctrl-S and the paste in one PTY write merged the draft with the message; observe the stashed, empty composer before
writing the payload instead of relying on a delay. Claude's indicator, drawn in grey (`ESC[38;5;246m`), sits at the
right end of the row just above the composer's top rule: `› stashed` in 2.1.274 and 2.1.314, `draft stashed` in 2.1.329
(after `◐ medium · /effort` for the moment that indicator still shows, then alone), measured in a 120×40 PTY and kept
as `apps/daemon/testdata/claude_native_stashed.bin` and `claude_native_stashed_2.1.329.bin`. In 2.1.329, a message
pasted into the stashed composer and submitted with CR brought the draft back and removed the indicator within about
0.2 seconds. The rows Claude draws under the composer vary (a warning line, the mode line,
the agents hint), so the indicator's distance from the screen's bottom is not fixed. Grok's top composer border
includes `Stashed`.

Isolated local-endpoint runs confirmed exact restoration of Chinese text, multiline drafts, and collapsed long
pastes, including a 4,699-character draft. Claude retained the cursor; Grok restored it at the end. Attachments were
not tested. Claude's source retains its pasted-content records, and Grok documents image retention, but these do not
replace a measured attachment round trip. Claude's `chat:stash` action is customizable through `keybindings.json`;
Ctrl-S must not be assumed to stash when its binding is unknown or overridden.

**Claude Code's queued-message hint reads as text unless the cursor cell is discounted.** While a message waits in
Claude Code 2.1.316's queue, its empty composer shows `❯ Press up to edit queued messages`. The hint is faint, but
its first character is the cursor, drawn in inverse video rather than faint, so an undimmed read of the box yields
`P` and looks like a draft. Typed text replaces the hint and is never faint. Separately, for roughly 15–20 ms after a
bracketed paste and CR, the box still shows the pasted message before Claude takes it; a Ctrl-S landing in that
window stashed the message itself, which was later restored and submitted joined onto the next message. Both were
measured in a PTY against a local stub endpoint.

**Codex has a separate message queue.** Codex 0.163.0's ordinary `codex queue` works with the shared daemon, but
passing even one `-c` override disables implicit shared-daemon selection and uses embedded mode unless an
explicit remote is selected. The command can enqueue without an idle embedded
TUI waking up; an interrupted thread can also retain queued messages without starting them. Neither successful
queueing nor a missing immediate turn justifies sending the message again.

A private app server supports the launch overrides and independent delivery together; Octoboard runs one for each Codex
0.166.0 or later session. Start the same binary as `codex -c <overrides> app-server --listen unix://<private-socket>`,
then its TUI with `codex --remote unix://<private-socket>` and the original launch flags and `-c` overrides. The Unix
socket carries WebSocket frames with handshake URI `ws://localhost/rpc`. An explicit `--remote` takes precedence over
the `-c` rule, so the same launch overrides can be supplied to both processes. The TUI needs its own copy for local
settings: with an account setting `tui.terminal_title=[]`, a server-only title override produced no OSC 0 titles, while
copying the overrides to the explicitly remote TUI restored them and retained working, idle, cancelled-turn, hook, and
draft behavior. Initialize with `capabilities.experimentalApi: true`, then send the `initialized` notification. Before
any user submission, `thread/loaded/list` returns the new TUI's thread ID even though `thread/list` is still empty.
Creating a thread through `thread/start` and then resuming it instead raised a hook review modal despite the TUI's
bypass flag; that bootstrap path does not preserve the ordinary fresh-launch flow.

On 0.166.0 a private server's loaded threads are not the TUI's alone. After the first turn the server loads an
ephemeral title thread (`threadSource: "thread_title"`, `ephemeral: true`, `path: null`) for about 60 seconds, until
`thread/closed`; the agent command center (Left arrow on an empty composer, or Esc or option 2 on the trust screen) can
start further tasks (`n`); and sub-agent threads carry a `parentThreadId`. The TUI's own thread is told by the
`thread/started` notification the server pushes to every client connected before the TUI starts, whose `thread` has
`threadSource: "user"` and `ephemeral: false`; `thread/read` reports the same fields for a loaded thread. The
`SessionStart` hook fires only with the first turn, so it cannot identify the thread before the first prompt.

Keep `--cd <dir>` on the remote TUI (0.166.0): without it the TUI takes the server process's working directory and
shows no folder-trust screen at all, while with it an untrusted folder shows "Folder access / Trust this folder? … 1.
Trust and continue / 2. Back to Agent Command Center", even outside a git repository, and `thread/loaded/list` stays
`[]` until it is answered. A server-side `-c 'projects={"<dir>"={trust_level="trusted"}}'` skips the screen but
replaces the whole `projects` table, hiding the user's other trust entries. On macOS the socket path given to
`--listen unix://` becomes a symbolic link to `/private/tmp/codex-daemon-<uid>/<sha256>`, with a `.lock` file; both go
when the server exits, and the path exists once the server listens.

Persistent remote resumes ignore hook-trust bypass too. `hooks/list` exposes each hook's native key, source, command,
and current hash. A resume can trust a single generated hook command with invocation-local `hooks.state`
overrides, leaving user hooks untouched. Pass the complete state table as one TOML assignment:
`-c 'hooks.state={"/<session-flags>/config.toml:stop:0:0"={trusted_hash="sha256:..."}}'`. A quoted dotted key passed
through `-c` was split incorrectly at the native key's `.toml`; the complete table retained it exactly. An isolated
existing-thread resume using this method confirmed no extra review for the injected hooks, independent message
delivery, and draft and cursor preservation. No user's trust file was changed.

Codex 0.163.0 rejects explicit permission overrides, including `approvals_reviewer`, in a remote TUI resume, and
0.166.0 rejects every one of them: `default_permissions`, `approval_policy`, `approvals_reviewer` and `sandbox_mode`
each end it with "Error: Permission overrides are not supported when resuming a remote task." The private server can
receive that configuration, but a saved thread reviewer takes precedence over its CLI default.
Source inspection of that release confirms that an explicit `thread/resume` with `approvalsReviewer` checkpoints and
flushes the effective settings before replying, even without a turn. A reviewer applied through that request
before the TUI connects without its reviewer override survives; a subsequent TUI reload restores the checkpointed
choice. On 0.166.0 `thread/resume` takes the whole preset, `permissions` (a built-in profile such as `:workspace`),
`approvalPolicy` and `approvalsReviewer`; an isolated resume through Octoboard's launch path with the `auto_review`
preset left the TUI running and recorded `auto_review` in the thread.

`thread/queue/add` accepts `threadId`, text `input`, and a stable `clientUserMessageId`. Its response includes
`queuedSubmission.id`, passed with `threadId` to `thread/queue/start`. In idle sessions the add can start the item
before start arrives; a subsequent “queued submission not found” is not grounds to enqueue again. During a turn, start
reports an active or pending turn and the queue consumes the item afterwards. After cancellation, explicit start wakes
the interrupted thread too. Isolated tests using the actual hook, MCP, and developer-instruction injection confirmed
their execution, separate inference messages, and preservation of multiline and 6,018-character collapsed drafts and
cursor positions, including a fresh TUI with no prior user submission. No composer keystrokes are needed for this
delivery path. The same held on 0.166.0, on macOS and Windows: idle, mid-turn and post-cancel delivery, a fresh TUI, a
resumed one, multiline drafts and (macOS) a `[Pasted Content 6599 chars]` collapsed paste submitted intact later. Each
queued message fires the `UserPromptSubmit` and `Stop` hooks. Esc still interrupts a remote TUI; Ctrl-C on an empty
composer quits it at once ("Disconnected from this task. Any running work continues."). An item queued during a turn the
user cancels with Esc stays queued (it had not run six seconds later), and `thread/queue/start` with the `threadId`
alone, no `queuedSubmissionId`, ran it as a turn of its own, the draft left in the composer (macOS, through Octoboard's
launch path). Not measured: approval prompts and MCP tool calls of a real model during queue delivery, and 0.167.0.

More of Codex 0.166.0's queue and threads, measured on macOS against a private server and its remote TUI with a stub
model endpoint, in a scratch home:

- `thread/queue/start` on an empty queue is refused with -32600 "queue is empty", while the thread is idle and while a
  turn runs alike; with an item waiting behind a running turn it is refused with "thread already has an active or
  pending turn". `thread/queue/list {threadId}` lists the waiting items, each with its `id`, `input` and
  `clientUserMessageId`; a started item leaves the list.
- A thread the TUI switches away from in the agent command center (a new task with `n`) stays loaded for two to three
  minutes and is then unloaded (gone from `thread/loaded/list`). `thread/queue/add` to an unloaded thread is accepted and
  the item kept, but it does not run. `thread/resume {threadId, excludeTurns: true}` from another connection loads the
  thread again and runs what its queue holds, in order, while the TUI keeps showing the other task with its draft; a
  message queued to the original while it was still loaded ran there too, in the background.
- An approval request (`item/commandExecution/requestApproval`, from a command run with `sandbox_permissions:
  "require_escalated"`) of a turn started through `thread/queue/add` reached neither the connection that queued the
  message, kept open, nor another client connected before the TUI: only the TUI showed it. Those connections saw only
  `thread/status/changed` with `activeFlags: ["waitingOnApproval"]`.
- `-c 'hooks.state={…}'` naming only the generated session-flag hook does not replace the user's own trust: a user
  hook trusted in `config.toml`'s `[hooks.state]` stayed `trustStatus: "trusted"` beside it.
- A connection that loads a thread with `thread/resume` is subscribed to it. With the TUI showing another task, the
  approval request of a turn on that thread (queued through the same connection) went to that connection alone, and the
  thread stayed `waitingOnApproval` after it closed; the agent command center listed the task as "Needs input", and
  opening it showed the approval in the TUI, where it could be answered. A message queued and its approval raised before
  a resumed TUI attached was likewise shown, and answered, in the TUI once it attached.
- The TUI does not put the user's own input typed during a turn into `thread/queue`: Enter steered it into the running
  turn (the model request restarted with it), and Tab queued it in the TUI itself, which ran it after the turn;
  `thread/queue/list` stayed empty meanwhile, and an item queued through `thread/queue/add` ran after both.
- `thread/queue/add` on an idle thread starts the item itself only when the queue was empty. After an Esc left items
  queued, an item added to the idle thread was only appended, and nothing ran; `thread/queue/start` with the first item's
  `queuedSubmissionId` then ran all three in order, one turn each.
- The app server's program carries the reply "Server overloaded; retry later." for a request it has no room for (found
  in the 0.166.0 binary's strings, beside its request intake; the JSON-RPC code was not observed, and is taken to be
  -32001). Octoboard treats it as a wait, not as a refusal of the message.
- Read from the 0.167.1 source (`rust-v0.167.1`), not measured: `thread/queue/start` answers once the core has begun
  the turn, with `{"turn":{"id",…,"status":"inProgress"}}`, and refuses with "thread already has an active or pending
  turn" when the core is not idle. The status `thread/read` reports (`notLoaded`, `idle`, `systemError`, `active`) names
  no turn, and the server updates it from the turn's start and end events as it handles them, so it lags the start's
  answer and, after a cancel, the cancelled turn's end. `thread/turns/list {threadId, limit, sortDirection: "desc",
  itemsView: "notLoaded"}` lists the latest turn first, rebuilt from the rollout with the turn running merged in from
  those same events; the listing does not say which source a turn came from, and the rollout may record a new turn
  before the server has handled its events. Octoboard waits for up to 2 seconds after a start until the status is
  active and the latest turn is the one started.

The terminal paste sequence measured for Claude Code, Codex and Grok Build is `ESC[200~`, the text with LF separators, `ESC[201~`, then `CR`;
multi-line text arrives as one message with no delay between paste and Enter. Every agent queues a message written
mid-turn and consumes it when the turn ends. (Codex additionally has `codex queue --thread <session id>`.) Seven rules,
each failing silently or destructively if ignored:

- A message starting with `/` runs as a slash command even inside a bracketed paste; prefix a single space. The same
  holds for a prompt given on the command line (Claude Code 2.1.308, a fresh launch against a stub endpoint under a
  scratch `HOME`): `claude "/help me please"` runs the command and makes no model request, while `claude " /help me
  please"` reaches the model as text, so the launch prompt is guarded by the same leading space. Not run with
  `--resume`. Grok Build 1.0.89 is the exception for a paste: it runs ` /session-info` (and a custom command such as
  ` /probecmd guarded words`) after a leading space, a leading line break or a leading no-break space as it does with
  nothing in front, while after a zero-width space (U+200B) the text reaches the model and `UserPromptSubmit` reports
  the prompt with the zero-width space in front (`testdata/grok-slash-guard.md`; scratch `GROK_HOME`, fake key, local
  stub endpoint). Its command line was not measured.
- A macOS PTY master accepts only about 1022 bytes before `EAGAIN` when the child is not draining, so the write is a
  non-blocking partial-write-and-retry loop in slices, never a blocking `write_all` on the daemon's event loop.
- Nothing may be written while a modal dialog is up. The paste is discarded but the trailing `CR` confirms whatever is
  highlighted: at Claude Code's trust dialog that was seen to exit the session for a short message (an 8.8 KB paste left
  it untouched, unexplained: the write was answered `delivered` while no text appeared), at Grok's approval modal it
  would select "always-approve", and at Codex's or Grok's folder-trust confirmation it accepts, trusting the folder.
  Only Codex's approval modal was seen to take a paste without effect. Never send bare keys either, since Grok and Codex
  treat digits as confirm hotkeys. The one deliberate exception is the daemon's press of an agent's own folder-trust
  confirmation: Down and Enter for Claude Code, Enter for Codex, `y` for Grok, and nothing else.
- The gate comes from hook-reported state, never from the terminal, because none of the agents signal modal state
  through terminal modes. If the hook state is missing or stale, do not write. Two kinds of screen are the exception,
  since no hook announces them, and are read off the terminal instead. The folder-trust confirmations are read from the
  output: no hook runs before one is answered, and nothing is written into a session from its launch until its first
  hook has arrived (for Claude Code, also until bracketed paste is on, below); for Codex, which runs none until its
  first prompt, also until the confirmation has been pressed, the person's answer in the terminal is seen to replace it,
  or the watch for it has ended with none sighted; for Grok Build, which may run no hook at all, at most until 30 s have
  passed with no confirmation sighted, or, for one sighted, until the rendered screen no longer shows it. The dialogs an
  agent raises that only a person can answer (Claude Code's launch and resume dialogs and its views of background
  agents, Codex's and Grok Build's own screens) are read from the rendered screen, which is looked at before each write:
  nothing is written while one is up.
- Claude Code reads nothing written before it turns bracketed paste on. From its start until Ink first puts the
  terminal in raw mode, it reads stdin through an early-input capture that drops every `ESC[` sequence (so the paste
  markers vanish) and turns `CR` into a newline, then seeds the prompt with what it caught; Ink's first raw-mode enable
  stops the capture and writes `ESC[?2004h` (with `?1004h` and `?2031h`). With the default inline renderer that comes
  before `SessionStart`; with the fullscreen renderer (`"tui": "fullscreen"`) `SessionStart` fires first, 28–325 ms
  ahead of it, and a paste and `CR` written in between is left unsent in the input box, or wiped when a launch prompt
  is submitted. Ink's enable is not the first `ESC[?2004h` on the terminal, though: as it starts, before it reads
  anything, Claude Code probes the terminal with `ESC7 ESC[r ESC8 ESC[?25h ESC[?25l ESC[?2004h ESC[?2031h ESC[?1004h
  ESC[>0q ESC[?u ESC]7501;? BEL ESC[c ESC[>4m ESC[<u ESC[?1004l ESC[?2031l ESC[?2004l`, turning bracketed paste on
  and straight off again. A read of its output can end between the two, and a message written after an early
  `SessionStart` on the strength of that toggle came while the terminal was not yet in raw mode: the tty echoed it,
  Claude Code later showed it as `[Pasted text #1 +4 lines]` in the input box, and the `CR` was lost (3 of about 45
  resumes with no launch prompt, inline renderer). Its folder-trust confirmation also turns bracketed paste on; a
  message written right after it is answered and `SessionStart` arrived, before the main screen turned it on again,
  was still sent (15 of 15, fullscreen). So Octoboard holds a Claude Code session's writes until its first hook has
  arrived and its terminal has bracketed paste on at that hook or after it; bracketed paste turned off again before
  the hook, the probe's toggle and the trust confirmation's alike, does not count.
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
  isolates (U+2068, U+2069), which an agent copies into what it writes, are the usual trigger. Octoboard's own handling
  of these characters is in `apps/daemon/src/session/term/invisible.rs` and in "Messages held until a session can take
  them" in `docs/product/message-delivery.md`. Because a held prompt fires no `UserPromptSubmit`, that hook is also how
  Octoboard notices a write to an idle Claude Code session that was not sent, whatever held it; a write mid-turn cannot
  be checked that way, since the agent sends nothing until its turn ends (the behaviour is in "A message written but not
  sent" in `docs/product/message-delivery.md`). Codex and Grok Build were not measured: neither executable carries any
  wording about such a hold, but neither was run against one.
- Claude Code reads a pasted piece that ends in an image extension as an image and loses the `CR` that follows
  (measured on 2.1.310, 2026-10-25, a real `claude` in a PTY against a stub endpoint). Its paste handler splits the text
  on newlines and on a space right before `/` or a Windows drive path; a piece whose trimmed, quote-stripped and
  backslash-unescaped text ends in `.png`, `.jpg`, `.jpeg`, `.gif` or `.webp` (any case) takes an asynchronous
  image-read path, whether or not the file exists and whether it is absolute or relative. An `Enter` that arrives while
  the paste is processed is held and replayed on the text path, but the image path clears it when it completes: the
  message sits in the input box as `[Image #1]` plus `[Pasted text #N +M lines]`, the path text is gone and an image the
  receiver never asked for is inlined. Idle, no `UserPromptSubmit` fires; mid-turn, the message is not queued but left
  in the input box. A plain multi-line paste was sent in about 0.4 s, any paste with such a piece was not, and a `CR`
  written separately 0.3 s or more later sent it. A report listing produced screenshots one per line is the usual
  trigger. A trailing space or invisible character does not avoid it (the piece is trimmed first; invisible characters
  hold the prompt anyway). Octoboard's shaping against it is in `apps/daemon/src/session/term/image_path.rs` and in
  "Messages held until a session can take them" in `docs/product/message-delivery.md`; Codex and Grok Build were not
  measured. Measured with the shaped message: sent both idle and mid-turn.
- All three take a bracketed paste of an image file's absolute path as an attached image, shown as `[Image #1]` in
  the input box in place of the path (measured in November 2026 on **Codex 0.167.1** in a PTY, and through the desktop
  app on Codex, Claude Code and **Grok Build 1.0.77**, whose preview names the file's format, size and path). Codex
  reads a path containing spaces as plain text unless it is single-quoted, and then reads it as an image. An empty
  bracketed paste is a different matter: Claude Code and Grok Build take it as the cue to read the system clipboard
  themselves (Grok's own documentation lists `Cmd+V` for a screenshot on macOS), Codex ignores it and reads the clipboard
  only on its own key, `Ctrl+V` (its hint: "Paste an image with `Ctrl+V` to attach it to your next message."). Either
  clipboard read reaches only the clipboard of the machine the agent runs on. Octoboard's paste of a clipboard image
  therefore pastes a saved file's path (`docs/product/pasting-images.md`).
