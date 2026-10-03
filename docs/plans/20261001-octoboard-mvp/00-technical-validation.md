# 00 Technical Validation

> Goal: before any product code is written, settle one by one the untested premises the plan depends on, giving each a
> conclusion (feasible / not feasible / feasible with conditions) and the way it was verified.
> Done when: every investigation item below has a conclusion; infeasible items have been adjusted along their fallback and
> written back into the corresponding section of `docs/mvp.md`; and the validation prototype can run the whole chain
> "daemon starts all three agents over a PTY → hooks report status → a message reaches a running session → an MCP tool gets
> called → the frontend renders the terminal over WebSocket".

## Landing status

Wrapped up. All 21 investigation items have a conclusion; the conclusions are folded back into `docs/mvp.md` (4.4 launch
mechanism and pitfalls, 5.3 reporting, 6 agent adapters, 13 validation status). This document is kept only because of the
debt below.

### How the final form differs from the plan

- **The launch mechanism changed.** `$SHELL -l -c` is not enough; it is a login *and interactive* shell, snapshotted per
  launch, and the snapshot has to be filtered before use.
- **Forced reporting was dropped.** Blocking the stop works, but it is user-visible as an error and the model sometimes
  refuses; the daemon synthesises the report instead.
- **Two fallbacks turned out to be unnecessary** — Codex and Grok both have a real append mechanism for the role
  description, so neither needs it smuggled into the initial task.
- **T2's planned remedy does not address the actual failure.** Neither batching nor binary framing prevents clients being
  dropped under heavy output; backpressure does. Binary frames were used from the start rather than compared, and whether
  output framing affects throughput was never measured.
- **Grok's injection has no flag at all** and needs a per-session `GROK_HOME` built as a symlink farm — the most
  intricate mechanism of the three, and the one most likely to break on a Grok upgrade.

### Deliberately left transitional layers

- **The whole `prototype/` directory.** `TODO(milestone 01)` in `prototype/README.md` records that milestone 01 deletes
  it. It is referenced from `docs/project-map.md`, which must be rewritten for the real code tree at the same time.
  Nothing outside `prototype/` depends on it.

### Debt handed to later milestones

Each item below was written into the document that acts on it; this list exists so a later session can tell whether 00's
debt is settled, not to hold the content. **Nothing here should be read instead of the receiving document.**

| Debt | Written into |
|---|---|
| The PTY write rules, the environment-snapshot filter, the `Ctrl+C` interception, forwarding `onBinary` for mouse reports, the per-volume file-access prompt a packaged application raises, the full-width punctuation defect and the Rust toolchain floor — all durable product constraints | `docs/mvp.md`, sections 4.4 and 6 |
| Terminal-pane state ownership, `Ctrl+C`, forwarding `onBinary`, degrading gracefully on a volume with no file access, tolerating an unknown `agent_session_id`, PTY reader backpressure, clean session teardown, and **never signalling a process group by pid** — the prototype did, and with a recycled pid it killed the spawn helper of the editor the daemon ran under, leaving that process unable to start any child until restarted | `01-shell.md` |
| The report synthesis replacing the blocking Stop gate and the conditions qualifying it, and a Grok hub taking its role from `--rules` because a console working directory is not a git root | `02-orchestration.md` |
| Full-width punctuation under a CJK input method | its own plan directory, dated 2026-10-03 — not milestone debt, since the decision is to keep the framework's default composition handling |

### How it was verified, and what was not

Each item carries its own verification below. Everything was exercised against the real CLIs — Claude Code 2.1.274,
Codex 0.160.0, Grok Build 1.0.46. The prototype ran the full chain **on Claude Code** (PTY launch, hooks, a message
written into a running session, an injected MCP tool call, the terminal over WebSocket) and launched all three agents over
a PTY; Grok's and Codex's hook and MCP injection was settled by direct probes against those CLIs rather than through the
prototype, which runs them with no hooks and no MCP. T1 was driven by hand in the real Tauri window, and the packaging
checks against the built `.app`.

Every hook event that an earlier revision of this document listed as "registered but never fired" was subsequently
captured from a live session, with one exception. **Nothing is left outstanding here**: what could not be settled is in
`05-final-confirmation.md`, which exists so that none of it is carried silently.

- **Developer ID signing and notarization** — needs a credential this machine does not have. Leaves open only whether
  Gatekeeper admits the bundle elsewhere; the Keychain question it used to carry is settled and passes.
- **Grok's `StopFailure`** — the only uncaptured hook event. Provoking it means redirecting an authenticated client's API
  traffic to a server under our control, which needs the user's explicit authorisation and was correctly refused.
- **The end-to-end latency the user perceives** — the rendering step can only be instrumented once the real application
  exists.
Three items an earlier revision of this list carried — whether the turn after a Claude Code interrupt reports normally,
Claude Code's `AGENTS.md` fallback option, and the nested instruction-file casing — were settled rather than deferred, and
their conclusions are recorded in G3 and A7 below. Gatekeeper admission is milestone 04's own completion criterion, not a loose
end.

## Technical design

Prototype scope as planned: the daemon launches all three agents over a PTY; hooks report status; messages are written into
a running session; a minimal MCP server is injected; the frontend renders the terminal with `xterm.js` over WebSocket.
What was actually built is narrower, deliberately — see "Landing status" above.
The per-agent injection mechanisms, which were the to-be-verified part, are now settled and written into the "Agent
adapters" table in section 6 of `docs/mvp.md`.

## Investigation items

Conclusions and the way each was verified are recorded after the item; the parenthesized text is the fallback if it turns out
not to be feasible.

### Common to all agents

- [x] G1 With the project directory as cwd, the project's own configuration (instruction files, skills, hooks, permissions)
  and the injected content both take effect without overriding each other (change the injection mechanism; project
  configuration must not be overridden)
  — **Claude Code: feasible.** Injected `permissions` merge with the project's, and precedence is by rule *kind* rather than
  by source (`deny` > `ask` > `allow`), so an injected `allow` can never re-open something the project denies. Verified with
  a six-case matrix over `Bash(curl *)`. Two conditions: **never emit `--setting-sources`** (with `--setting-sources user`
  the project's `deny` rule *and* the project's own hooks were both silently dropped), and note that **workspace trust gates
  project `allow` rules** — in an untrusted workspace they are ignored with an explanatory line on stderr while project
  `deny` rules still apply, so an untrusted project is only ever more restrictive. Trust lives in `~/.claude.json` under
  `projects[<path>].hasTrustDialogAccepted`, which Octoboard must not write; the adapter should detect that stderr line and
  surface it to the user.
  — **Grok Build: feasible with conditions** under the `GROK_HOME` mechanism of A6. Project `AGENTS.md`, project hooks,
  Octoboard's hooks and Octoboard's MCP tool were all in effect in one run; hooks are additive across layers and
  byte-identical handlers are deduplicated. Two conditions, both of which silently disable the *project's* own
  configuration if missed: the project folder must be trusted (an undocumented `--trust` flag works, and
  `trusted_folders.toml` must be symlinked into Octoboard's `GROK_HOME` or every project looks untrusted), and the project
  must have a recognised git workspace root — project hooks did not load in a trusted non-git directory.
  — **Codex: feasible.** Verified in one interactive answer carrying all three markers at once: the injected role
  description, the project `AGENTS.md`, and a project-local skill. Codex concatenates `<cwd>/AGENTS.md` *after*
  `~/.codex/AGENTS.md` inside one instructions block separated by a `--- project-doc ---` line, so the user's global file
  survives; it discovers both `<cwd>/.agents/skills/` and `<cwd>/.codex/skills/`; and project hooks in
  `<cwd>/.codex/hooks.json` **merge** with the injected ones. Notably `<cwd>/.codex/config.toml` is **not read at all**, so
  Codex has no project-level permission or approval configuration for the injection to collide with.
- [x] G2 When launching through the login shell, PATH, API keys, and each agent's login state are fully available, including
  when the application is launched from Finder (read the user's shell environment and pass it in explicitly)
  — **feasible with conditions, and the plan's mechanism was wrong**. `$SHELL -l -c` is *not* enough: `grok`, `codex` and
  fnm's `node` are put on `PATH` by `~/.zshrc`, which a login-only non-interactive zsh never reads. `$SHELL -l -i -c`
  resolves all of them and produces no stdout/stderr noise, so the adapter snapshots the environment once with
  `$SHELL -l -i -c 'env -0'` and spawns each agent binary directly with it, rather than running the agent inside a shell.
  Note fnm's `PATH` entry points at a per-shell-instance directory under `~/.local/state/fnm_multishells/`, so re-snapshot
  per launch rather than caching for the daemon's lifetime.
  Verified: `env -i HOME=… /bin/zsh -l -c 'command -v grok'` finds nothing while `-l -i -c` finds it, and a login+interactive
  shell writes 0 bytes to stdout and stderr.
  **The login-state half is also verified, and an earlier revision of this item got it wrong.** It recorded that a run from
  a stripped environment reports "Not logged in" and concluded the question could not be settled by probing. That was an
  artefact of stripping too much: with the session markers removed by enumeration and the rest of the environment left
  intact, `claude auth status` returns `loggedIn: true`, identical to an unstripped run. Credentials live in the macOS
  Keychain and survive the filtering, so a "Not logged in" from a filtered launch would be a real finding rather than
  noise. T4 separately confirms the same holds from the packaged bundle.
- [x] G3 Status events cover: work started, stopped, permission requested, question asked of the user, resumed after the user
  answered (missing states degrade to not being displayed, or are inferred from terminal output patterns)
  — **Claude Code: feasible with conditions.** 33 hook events exist (none documented in `--help`; the authoritative list is
  embedded in the binary). Every payload carries `session_id`, `transcript_path`, `cwd`, `hook_event_name`, and within a
  turn also `prompt_id`, which is **stable for the whole turn including Stop-hook re-runs** — that is the per-turn key the
  daemon should use. Mapping: (a) `UserPromptSubmit`; (b) `Stop`, whose payload carries `last_assistant_message` so no
  transcript parsing is needed, plus `background_tasks` (non-empty means paused on background work, not finished), backed up
  by `Notification` with `notification_type: "idle_prompt"`; (c) `PermissionRequest` — note `tool_use_id` is **absent**
  despite the embedded docs, so correlate with the immediately preceding `PreToolUse`, which does carry it; (d)
  `PreToolUse` with `tool_name: "AskUserQuestion"`; (e) **no dedicated event** — infer from the `PostToolUse` /
  `PostToolUseFailure` that resolves the pending call, with the daemon holding a state machine keyed on
  `(session_id, prompt_id, tool_use_id)`.
  Two gaps to accept: a turn ending in a **plain-text** question to the user produces an ordinary `Stop` with nothing in the
  payload to distinguish it from a finished turn, and `Notification` with `notification_type: "permission_prompt"` never
  fired in 50 s and 65 s open-dialog tests, so do not build on it. One trap: `SubagentStop` also fires for internal helper
  agents with `agent_type: ""` — filter on a non-empty `agent_type`.
  **Three further findings, each captured from real payloads, that change what an adapter must register:**
  - **`Stop` and `StopFailure` are mutually exclusive.** When a turn ends in an API error the order is
    `SessionStart` → `UserPromptSubmit` → `StopFailure`, and `Stop` never fires — confirmed both empirically and in the
    dispatcher, which returns before reaching the `Stop` hook. An adapter that keys "turn finished" on `Stop` alone leaves
    the session looking busy forever after any API error. In the TUI `StopFailure` leaves the session alive at the prompt;
    only headless `-p` tears it down, so it must not be read as session death. Its payload carries `error`,
    `error_details` (optional, and the raw HTTP status plus verbatim body rather than prose) and `last_assistant_message`
    (the user-facing error text, not model output). `error` is **not** a function of HTTP status: a plain 400 gives
    `unknown`, a 400 whose body says the prompt is too long gives `invalid_request`, and a 529 gives `server_error`
    rather than `overloaded`.
  - **A user interrupt produces no turn-terminating event at all.** Verified mid-tool as well as mid-thinking, with both
    asynchronous and synchronous hooks, against a tool confirmed to be executing: after Esc, none of `PostToolUse`,
    `PostToolUseFailure`, `PostToolBatch`, `Stop`, `StopFailure` or `Notification` fire. The failure mode is therefore a
    **silent hang**, not a false report — the daemon is left holding an open `PreToolUse` that nothing ever closes. Any
    per-tool state the daemon keeps needs an independent signal to close it — see the recovery note below. The only
    machine-readable trace of the interrupt itself is
    in the transcript: a `tool_result` with `is_error: true` beginning "The user doesn't want to proceed with this tool
    use", followed by a user block `[Request interrupted by user for tool use]`; `transcript_path` from any earlier
    payload locates it.
    **The session recovers fully, though.** Verified by interrupting mid-tool and then running two more turns: the next
    turn fires `Stop` normally and the turn after that pairs `PreToolUse`/`PostToolUse`/`PostToolBatch`/`Stop` as usual.
    Only the interrupted turn is silent, so a session does not have to be treated as unreliable afterwards. Better still,
    the daemon needs no wall-clock timeout: **the next `UserPromptSubmit` is a reliable signal that the previous turn is
    over**, and is what should close a dangling `PreToolUse`. And the dangling state is bookkeeping only — Claude Code
    does kill the tool's child process on interrupt, it simply fires no hook about it.
  - **A tool call rejected by Claude Code's own pre-execution guard fires only `PostToolBatch`** — `PreToolUse`,
    `PostToolUse` and `PostToolUseFailure` are all skipped, with the error inside `tool_calls[0].tool_response`. A daemon
    pairing `PreToolUse` with `PostToolUse` misses those calls entirely.
  And one confirmation: `Stop`'s `background_tasks` really does distinguish "paused on background work" from "finished" —
  a backgrounded shell call produced an entry carrying the same id as the matching `PostToolUse.tool_response`'s
  `backgroundTaskId`, so the two can be correlated without parsing the transcript, and an idle turn produced an empty
  array.
  — **Grok Build: feasible, all five states covered.** 16 events, payload as JSON on stdin (every event carries both a
  PascalCase `hook_event_name` and a snake_case `hookEventName`), plus reserved env vars `GROK_HOOK_EVENT`,
  `GROK_HOOK_NAME` (whose `global/` / `project/` / `managed:` prefix is usable provenance), `GROK_SESSION_ID`,
  `GROK_WORKSPACE_ROOT`. Mapping: (a) `UserPromptSubmit`; (b) `Stop` + `StopFailure` + `StopCancelled` +
  `Notification`/`idle_prompt`; (c) `Notification` with `matcher: "permission_prompt"`; (d) `Notification` with
  `matcher: "elicitation_dialog"`, preceded by `PreToolUse` on `ask_user_question`; (e) the `PostToolUse` for the same tool.
  Match on `notificationType`, never on `message`, which is display text that changes between releases.
  Three traps: **`Stop` fires twice per session** — once per turn with `reason: "end_turn"` and again at teardown with
  `reason: "shutdown"`, so filter on the reason or every session ends with a phantom turn; `SessionStart.source` on resume
  is `"load"`, not `"resume"`; and some turns (bash-mode, builtin slash commands, cancel-and-send, rewinds) report none of
  the three stop events, which is what `idle_prompt` is the backstop for.
  **`idle_prompt` was captured, and the case the guarantee rests on was reproduced.** It fires about 60 s after a turn
  ends, anchored on the turn end rather than on last activity — a `/compact` run inside that window did not reset the
  timer. More importantly, a turn cancelled with Ctrl-C before the first token produced **no `Stop`, no `StopCancelled`
  and no `StopFailure` at all**, and `idle_prompt` was the only turn-end signal the session ever emitted. So 5.3's
  "a report always reaches the hub" guarantee does hold for Grok, and it holds *because* of this event.
  Two conditions come with it: the payload has **no `promptId`**, so the backstop cannot be correlated to a turn by id —
  only by session and clock; and it needs at least one turn to have ended, so a session that only ran a slash command or a
  bash-mode command never emits it.
  The teardown `Stop` is also distinguishable by shape, not just by `reason`: it carries no `promptId`,
  `lastAssistantMessage`, `backgroundTasks` or `sessionCrons`, all of which the per-turn fire does.
  **`SIGTERM` produces a graceful teardown** that runs `SessionEnd` followed by `Stop(shutdown)` — which is how Octoboard
  should stop a Grok session. A `Ctrl-C`-driven quit was never achieved and remains untested.
  **Bash mode (`!`) bypasses the hook and permission systems entirely.** With a `--deny` rule in force, a bash-mode
  command matching that rule still executed, and fired no `PreToolUse`, no `PostToolUse` and no `PermissionDenied`.
  Anything Octoboard infers from tool hooks is blind to bash-mode activity, and a deny rule is not a containment boundary
  against it.
  `PermissionDenied` was captured and carries only `toolName`, `toolUseId`, `toolInput`, `toolInputTruncated` — **no
  reason, rule or source**, so it cannot distinguish a policy deny from a user declining a prompt. That distinction has to
  come from `StopCancelled`'s `reason`/`reasonDetails`. `PreToolUse` fires *before* the permission check, so a denied call
  leaves an open `PreToolUse` with neither `PostToolUse` nor `PostToolUseFailure`.
  `PostToolUseFailure` was captured, and is mutually exclusive with `PostToolUse`. One shape trap: for an MCP call routed
  through the dispatcher, `toolInput` is the *wrapper* (`{"tool_name": …, "tool_input": {…}}`) and the real arguments are
  nested one level down; the top-level `toolName` is already the qualified `server__tool` name, so match on that.
  `SubagentStart`/`SubagentStop` and `PreCompact`/`PostCompact` were captured too. Their traps, if ever used:
  `SubagentStart` fires in the parent and `SubagentStop` in the child (where `sessionId == subagentId`); the child also
  fires its own `SessionEnd`, so a host must filter on `subagentType` to tell a child teardown from its own; the resolved
  `subagentType` is not the one the caller asked for, so a matcher written against a requested name misses; and a parent's
  `Stop` is no promise that its subagents are finished. `PostCompact` carries no statistics at all — no summary, no token
  counts, no before-and-after sizes.
  — **Codex: feasible with conditions — state (d) has no coverage.** 12 events, payload as JSON on stdin. Every payload
  carries `session_id`, `transcript_path`, `cwd`, `hook_event_name`, `model`, `permission_mode`, and turn-scoped events add
  `turn_id`. Mapping: (a) `UserPromptSubmit` (plus `SessionStart` for process start, whose `source` is one of
  `startup`/`resume`/`clear`/`compact`/`fork`); (b) `Stop`, carrying `turn_id`, `last_assistant_message` and
  `stop_hook_active`, with `SessionEnd` for process end; (c) `PermissionRequest`, which fires **before** the modal is shown
  to the user and is non-interfering if the hook exits 0 with no stdout; (e) no dedicated event — `PostToolUse` fires once
  the tool actually runs, i.e. after the user allows, so raise the hand on `PermissionRequest` and clear it on the next
  `PostToolUse` / `PreToolUse` / `Stop` / `Interrupt` with the same `turn_id`.
  **(d) has no hook coverage on Codex**: there is no question or notification event, and the features that would produce a
  structured question (`default_mode_request_user_input`, `request_permissions_tool`) are off in this build. A question
  asked in prose is indistinguishable from a normal turn end — the same gap Claude Code has.
  **`Interrupt` closes Codex's gap for a cancelled turn, and was captured.** It carries only the base payload plus
  `turn_id`, and that `turn_id` is **identical** to the cancelled turn's `UserPromptSubmit`, so correlation needs no
  heuristics. `Stop` does *not* fire for an interrupted turn and `Interrupt` does not fire for a completed one, so the two
  are a clean turn-outcome discriminator — the one agent of the three where a user cancellation is observable at all.
  Pressing Esc while idle fires nothing; Ctrl-C during an in-flight turn also fires `Interrupt`.
  **Declining an approval also fires `Interrupt`**, which settles an ambiguity the first probe could not: in 0.160.0 the
  modal's "No" option aborts the turn by design, so this is not an artefact of having pressed Esc. `Interrupt` alone
  therefore cannot separate "the user cancelled" from "the user declined"; the discriminator is a `PermissionRequest`
  with no matching `PostToolUse`, immediately followed by `Interrupt`. A declined call emits no `PostToolUse` at all.
  `SubagentStart`/`SubagentStop` fired too and carry three traps if ever used: `transcript_path` is the *subagent's* file
  on `SubagentStart` but the *parent's* on `SubagentStop` (where the subagent's is in `agent_transcript_path`);
  `session_id` is the parent's while `turn_id` is the subagent's own, so the pair cannot be joined to the parent turn by
  `turn_id` — only `agent_id` works; and the subagent thread emits no `SessionStart` or `Stop` of its own. They remain
  noise for Octoboard and should be left uninjected.
  `PreCompact`/`PostCompact` fired on both triggers (`/compact` is a manual trigger, so no context window needs filling).
  Neither carries `permission_mode`, and `PostCompact` carries no summary or token counts. Ordering differs: a manual
  compaction gets its own fresh `turn_id`, while an automatic one fires *after* `Stop` and **reuses the finished turn's
  `turn_id`** — so the same `turn_id` can legitimately appear on `Stop` and again on compaction.
  Two further facts worth the adapter's attention: `SessionEnd.reason` is schema-pinned to the constant `"other"` and so
  carries no information, and `async: true` is **ignored for `SessionEnd`**, which Codex forces synchronous and therefore
  really does bind to its 3 s clamp. `Interrupt` does not fire in `codex exec` at all, so it cannot be regression-tested
  headlessly. Finally, the binary embeds the full JSON Schema for every hook payload with `additionalProperties: false`
  and an explicit required list — that is the authoritative field reference, and every captured payload matched it.
- [x] G4 When a hook fails or the daemon is unreachable, the agent neither hangs nor raises errors that disturb the user
  (hook scripts fail fast and exit silently)
  — **Claude Code: feasible with conditions. Hook errors *are* user-visible**, so the fallback's "fail fast and exit
  silently" is not optional, it is mandatory. A non-zero exit renders an error block in the TUI carrying the script's
  stderr (hook stdout is never shown); exit 2 *blocks*, event-specifically, and feeds stderr to the model; a hook with no
  `timeout` field blocks the turn for its full duration (a 30 s hook took the turn from ~7 s to 36.7 s, so the default
  timeout is over 30 s); and a `curl` to a dead port on a `Stop` hook additionally raised an immediate
  `Stop hook error occurred · ctrl+o to see` notice. The required shape is therefore: `exit 0` unconditionally (never
  `exit $?`), nothing on stderr, `"async": true` on the hook entry (verified to take the same 30 s hook off the turn's
  critical path: 6.8 s versus 36.7 s), an explicit short `"timeout"`, and a hard `curl --max-time`. Octoboard must own the
  hook scripts in its own support directory rather than inlining them, because a visible hook error on every turn while the
  daemon restarts would be the product's single most visible failure mode.
  — **Grok Build: feasible — fail-open**, but with the same user-visible cost: one scrollback line per failed hook carrying
  the first stderr line, after which the turn proceeds. Those lines appear **only in the TUI**, not in headless `-p`, so
  adapter hooks must be tested in the TUI. Timeouts default to 5 s but are **600 s** for `Stop`, `SubagentStop` and
  `PostToolUse`, which Grok treats as gates — Octoboard must set an explicit 2-5 s timeout on those three or a wedged
  observer hook stalls the UI for ten minutes. Exit 2 is meaningful on all three of those events too, so a literal `exit 0`
  is mandatory, and for `PreToolUse` a `deny` in stdout JSON is honoured regardless of exit code, so an observer hook must
  emit no JSON at all.
  **Blocking finding: Grok's HTTP hooks cannot reach the daemon.** `{"type":"http","url":"http://127.0.0.1:…"}` is rejected
  by SSRF protection ("only https:// URLs are allowed for HTTP hooks"), and the binary also blocks URLs resolving to
  private addresses, with no `allow_local` escape for hooks. Octoboard must use `type: "command"` hooks exclusively and let
  the script talk to the daemon itself.
  — **Codex: feasible — never hangs, and silent only with `async = true`.** With four deliberately broken hooks on one
  event (exit 1 with stderr, a `sleep 10` against `timeout=3`, a `curl` to an unreachable port, and a missing binary),
  Codex never hung and the turn always completed, but each failure rendered a visible history cell (`Hook failed`, with the
  exit code, or `hook timed out after 3s`; the hook's stderr text is not shown). A **synchronous** hook that hangs costs
  the turn its full timeout. With `async = true` a hook that exits 1 and writes stderr produced **no cell and no warning at
  all**. Required shape: `type="command"`, `async=true`, `timeout=3` — never more, because `SessionEnd` and `Interrupt` are
  hard-clamped to 3 s and exceeding it prints a visible warning — and a script that swallows everything and exits 0
  unconditionally.
- [x] G5 The injected MCP server can distinguish identities per session, and the hub and workers see only their own tools
  (use different ports or paths per role)
  — **Claude Code and Grok: feasible, and the fallback of per-role ports or paths is unnecessary.** A stdio server spawned per
  session is sufficient, because both CLIs hand the child a session identity for free: Claude Code sets
  `CLAUDE_CODE_SESSION_ID` in every MCP child's environment, Grok sets `GROK_SESSION_ID`. Per-role tool sets work by
  passing the role in the server entry's `args`/`env` and returning a role-filtered `tools/list`; verified on both agents
  that a hub session saw only the hub tool and a worker session only the worker tool. Use the same server key in both roles
  so tool names stay stable (`mcp__<key>__<tool>` on Claude Code, `<key>__<tool>` on Grok).
  Claude Code conditions: **do not pass `--strict-mcp-config`** — with it, the project's `.mcp.json` server, the user-scope
  servers and all claude.ai connectors disappeared from the session, which directly violates "project configuration must
  keep working"; the accepted cost of omitting it is that the session also inherits the user's global MCP servers. And a
  **server-key collision silently favours the injected server**: with the project's `.mcp.json` using the same key, the
  project's definition was never spawned, so the daemon must verify the key is absent from the project and user config, or
  use a deliberately improbable one.
  Grok condition: **`{{session_id}}` templating does not work in `args` or `env`** — it is passed through literally, so use
  `GROK_SESSION_ID`.
  — **Codex: feasible**, verified with two concurrent TUI sessions in the same project: each spawned its own stdio server
  process parented to its own `codex` process, each received its identity in argv, and the hub session's `tools/list`
  returned only the hub tool while the worker's returned only the worker tool. Unlike the other two agents, **Codex passes
  no `CODEX_*` environment variables to the MCP child**, so identity must come from argv or an explicit
  `-c mcp_servers.octoboard.env={…}` table — both fully under Octoboard's control. One extra condition: an MCP tool call
  can require approval, failing with "MCP tool call requires approval, but approval policy is never"; fix it with
  `-c mcp_servers.octoboard.default_tools_approval_mode="auto"`.
- [x] G6 Whether injected arguments must be passed again when resuming an interrupted session with `--resume`, and whether
  they take effect (the adapter reassembles all injected arguments on resume)
  — **Claude Code and Grok: feasible with conditions. The rule is asymmetric, and the role description is the exception.**
  Hooks and MCP servers are resolved from the launch arguments every time and are **lost on a resume that omits them** —
  verified on Claude Code (resume without `--settings` fired no hooks; without `--mcp-config` the injected server was gone)
  and on Grok (a bare `grok -r <id>` produced 0 hook events, while the same resume with `GROK_HOME` produced 5). A resume
  that forgets them does not fail loudly; it silently degrades to an unobserved session.
  Grok's `--session-id` was also exercised rather than taken from `--help`: launching with a generated UUID produced a
  session record under exactly that id, so pre-allocation works there, unlike on Codex.
  The role description behaves differently. Grok persists `--rules` into the session record, so it survives a bare resume.
  Claude Code records the appended system prompt on the conversation's first request and **replays it verbatim on every
  resume**, because `--system-prompt-snapshot` defaults to `on`: passing *different* `--append-system-prompt` text on
  resume is silently ignored (verified — the original marker still won). So the role must be treated as **immutable for the
  life of a session**; re-roling means a new session or `--resume --fork-session`. Passing
  `--system-prompt-snapshot off` inverts this but then the role must be passed on every single launch or it vanishes, and
  it also costs prompt-cache replay, so it should not be used casually.
  Adapter rule: reassemble and pass the complete injection set on every launch, resume included — the appended prompt is
  cheap to re-pass and *becomes* mandatory after a compaction, when the snapshot is re-rendered from whatever that launch
  passed.
  — **Codex: feasible, with one absolute rule.** Every `-c` override is per-invocation and is persisted nowhere, so the
  adapter must re-assemble and re-pass the *complete* injection set on every `codex resume <id>` —
  `developer_instructions`, `mcp_servers.*`, every `hooks.*`, `hooks.state`, and the project trust entry. Verified: a
  resume with the full set re-passed produced `SessionStart` with `source: "resume"` and the **same `session_id`**, the MCP
  server booted, and the model recalled the earlier turns, so Octoboard's `agent_session_id` mapping survives resume
  unchanged. `codex fork <id>` exists if a copy rather than a continuation is ever wanted.
- [x] G7 Using the Stop hook to block a stop and demand a `report` call is feasible; "already reported this turn" and
  "currently waiting for the user" can be determined (do not block; go straight to the fallback report)
  — **technically feasible, but the fallback is what Octoboard will use: the MVP does not block.**
  The mechanism works. On Claude Code the hook blocks by printing `{"decision":"block","reason":"<text>"}` on stdout and
  exiting 0 (the exit-2-with-stderr form works too but leaks the script's absolute path into the model's context). Verified
  end to end: the gate blocked, the model called the injected `report` tool, `Stop` fired again with
  `stop_hook_active: true`, and the turn ended cleanly. Consecutive blocks are capped at about 13.
  Both determinations are available. **"Already reported this turn"** is reliable because `prompt_id` is present on
  `UserPromptSubmit`, every tool event and `Stop`, and is identical across a block and its continuation — the daemon keys
  on `(session_id, prompt_id)`. **"Currently waiting for the user"** is safe for every structural wait, because `Stop`
  simply does not fire while a permission dialog or an `AskUserQuestion` dialog is open, nor on an Esc interrupt during
  thinking (each verified). The one case it cannot see is a turn that ends with a plain-text question: `Stop` fires and no
  payload field distinguishes it, so the residual risk is a few forced extra model turns, not a deadlock.
  **Why the MVP does not block anyway** (decided with the user): blocking is unavoidably user-visible and is labelled an
  error — the TUI shows `Stop hook error: …` plus a status-line notice on every gated turn, and `"suppressOutput": true`
  does **not** suppress it (verified). Worse, the model sometimes reads the injected feedback as a prompt-injection attempt
  and refuses: in one probe it said so outright and refused, and an unconditionally blocking hook made another model argue
  for 13 turns rather than comply. Both failure modes are visible in exactly the place Octoboard is supposed to feel calm.
  **The mechanism instead**: the `report` tool stays, and the role description encourages calling it; when a turn ends
  without a report, the daemon synthesises one from `Stop`'s `last_assistant_message` and pushes it to the hub. The
  "a report always arrives" guarantee is therefore met at the daemon level without blocking the model at all. The cost
  accepted is that the structured fields (`status`, `open_items`) degrade to prose on turns where the session did not call the
  tool.
  If the gate is ever revisited, the mitigations that made it behave are: declare the obligation in the role description up
  front, name the exact tool in the `reason`, and block at most once per turn (`stop_hook_active: true` always exits 0).

### Sending messages to a running session

- [x] M1 Claude Code / Grok: multi-line text written over the PTY as bracketed paste + Enter is submitted as one complete
  message (switch to single-line escaped text, or look for an official message-injection interface)
  — **feasible**. The sequence is `ESC [ 2 0 0 ~`, the text with LF separators and no trailing newline, `ESC [ 2 0 1 ~`,
  then `CR`; a single atomic write submits correctly, so no delay between the paste and the Enter is needed. It must be
  `\r`, not `\n` — a raw `\n` inserts a newline and does not submit. Verified on both agents for three lines, an embedded
  blank line, CJK text, and payloads up to 64 KiB / 925 lines, with the delivered line count matching the payload exactly.
  Two conditions the plan did not anticipate:
  - **A message starting with `/` is executed as a slash command**, and bracketed paste does not protect against it —
    pasting `/help…` opened Claude Code's help dialog and the message never reached the model. Prefix a single space
    whenever the message starts with `/`; verified to make it arrive as ordinary text.
  - On macOS a PTY master accepts only **1022 bytes** before `EAGAIN` when the child is not draining, so the write must be
    a non-blocking partial-write-and-retry loop (4 KiB slices, no artificial delay) and never a blocking `write_all` on the
    daemon's event loop. 64 KiB arrived complete in about 1 s that way.
- [x] M2 Behaviour when writing while the session sits at a permission prompt, a multiple-choice question, or mid-execution —
  above all, ruling out being misread as a keypress selection (write only in the "awaiting instructions" state, queue in
  every other state)
  — **feasible with conditions, and the plan's rule needs correcting.** The real boundary is modal versus non-modal, not
  busy versus idle. The message text is never misread as a keypress selection in any state tested; the danger is the
  trailing Enter.
  - **Mid-execution is safe and is the better path.** Both CLIs have their own message queue: paste + CR while a tool is
    running is queued, does not interrupt the turn, and is delivered when the turn ends. So the daemon should let the CLI
    queue rather than holding the message itself, which the plan's stricter rule would have cost in latency.
  - **While a modal dialog is up, nothing may be written.** The paste is silently discarded, but the trailing Enter is
    consumed as that dialog's confirm, with whatever option is currently highlighted. Verified: at Claude Code's
    folder-trust dialog (highlighted `No, exit`) a paste + CR made the process exit; at Grok's approval modal it answered
    the dialog, and Grok's pre-highlighted option there is `Yes, and don't ask again for anything (always-approve mode)`,
    so one mistimed write could silently put a session into always-approve for good. Grok's digits 1-5 are confirm
    hotkeys, which also rules out the "type the text instead" fallback.
  - Grok's slash-command picker is an autocomplete over the input box, so a paste there is appended as text and corrupts
    the buffer rather than being discarded.
  **The state must come from the hooks, not from the terminal.** Both CLIs set the bracketed-paste and focus-reporting
  modes once at startup and never toggle them per state, so there is no non-text signal to latch onto; the only terminal-side
  alternative is matching rendered footer strings, which needs a full VT emulator in the daemon, differs between Claude
  Code's two renderers, and has a safety-critical failure mode. Rule: gate writes on hook-reported state, and if that state
  is missing or stale, do not write. Screen-text matching is acceptable only as a secondary veto.
- [x] M3 Codex `codex queue`: when the message is consumed, whether it works while the TUI is running, and whether it
  conflicts with PTY input (fall back to PTY input)
  — **feasible, and the PTY fallback also works, so Codex has two viable paths.**
  `codex queue --thread <session_id> --message <text>` exists in 0.160.0 and works against a live interactive session,
  including one started with `-c` overrides. Consumption: delivered immediately as a normal user turn when the session is
  idle; queued and consumed right after the running turn's `Stop` when busy; queued and held while an approval modal is up.
  It does **not** conflict with concurrent PTY input — with user-typed text sitting in the composer, the queued message
  arrived as a separate turn and the composer's contents survived intact.
  The PTY bracketed-paste path is safe on Codex too, and notably safer than on the other two agents: at a live approval
  modal a paste of text full of `y`, `n` and digits changed the screen not at all and approved nothing, and at the `/model`
  selection list it selected nothing. Raw single keystrokes *do* act as list keys, so the rule is unchanged — always wrap
  injected text in a bracketed paste, never send bare keys.

### Per-agent specifics

- [x] A1 Claude Code: hooks injected via `--settings` merge with, rather than override, the hooks in the project settings
  (they must merge; otherwise change the injection mechanism)
  — **feasible**. `--settings` accepts a JSON *string* as well as a path, and its `hooks` merge with the project's
  `.claude/settings.json`. Verified by defining `SessionStart`, `PreToolUse(Read)` and `Stop` hooks in both places and
  observing all six fire, each pair in the same turn.
- [x] A2 Codex: whether a session id can be pre-allocated, and where to obtain it reliably otherwise (match the session
  directory by cwd and start time)
  — **pre-allocation is not feasible, but the planned racy fallback is not needed either.** No `--session-id` flag exists,
  and `CODEX_SESSION_ID` / `CODEX_THREAD_ID` are outputs rather than inputs: setting either before launch produced a
  different, internally generated id (compared across three launches). No `-c` key works either. Ids are UUIDv7.
  The reliable source is the **`SessionStart` hook payload**, which carries `session_id` together with `transcript_path`
  and `source`. **Thread id and session id are the same value** — the id from the hook was accepted verbatim by both
  `codex queue --thread` and `codex resume`.
  **Timing condition the data model must absorb:** in the interactive TUI `SessionStart` does *not* fire at process start —
  the thread is created lazily on the **first prompt submission**. Since Octoboard always launches with an initial task the
  id arrives within about a second, but a session the user opens with no task has no id until they type something, so the
  adapter must tolerate `agent_session_id = null`. This is exactly why the data model keeps `agent_session_id` separate
  from Octoboard's own `id`.
  `~/.codex/session_index.jsonl` is **not** usable: it only records *named* threads and is written late.
- [x] A3 Codex: which hook events are supported and in what payload format, and whether they can be injected via `-c` without
  modifying `~/.codex/config.toml` (use an environment variable to point at a separate config directory)
  — **feasible via `-c`, with a hook-trust condition. The `CODEX_HOME` fallback must *not* be used.**
  `features.hooks` is already stable and on; nothing needs enabling. Injection shape is
  `-c 'hooks.<Event>=[{hooks=[{type="command",command="<abs path>",timeout=3,async=true}]}]'`, one `-c` per event, and it
  needs no change to `~/.codex/config.toml` — the TUI's hook browser labels them as coming from session flags. The 12
  events and their payloads are recorded under G3.
  Two quoting rules the adapter must obey: **`-c` splits the key on `.`, so a dotted path containing a quoted segment is
  silently ignored** — `-c 'projects."/path".trust_level="trusted"'` does nothing, and the whole table must be passed as
  `-c 'projects={"/path"={trust_level="trusted"}}'`; and the project path must be the **canonical** one
  (`/private/tmp/…`, not `/tmp/…`) or the folder-trust modal still appears.
  **The condition — hook trust.** Codex gates any new or changed hook behind a persisted trust hash. Untrusted hooks raise
  a blocking startup modal ("Hooks need review …") and *no hook runs*. The verified way through is to seed the trust via
  `-c 'hooks.state={"<flags-path>:<snake_case_event>:<matcherIdx>:<handlerIdx>"={enabled=true,trusted_hash="sha256:…"}}'`,
  setting both keys. The hash preimage could not be derived (about twenty candidate serialisations failed), and the hash
  covers the handler definition, so it changes whenever the command string, timeout or async flag changes. The practical
  consequence for the adapter: make the hook command **one fixed argument-free binary** that derives the event from
  `hook_event_name` on stdin, so the 12 definitions are constant across every session and project, then capture the 12
  hashes once per Codex version and ship them with the adapter.
  **A documented flag removes that burden entirely, and it is verified:** `--dangerously-bypass-hook-trust` makes enabled
  hooks run without the persisted trust, so no hash has to be captured or shipped. Verified with `codex exec`: the run
  logged `hook: SessionStart` / `hook: SessionStart Completed`, the hook received its payload
  (`session_id`, `transcript_path`, `cwd`, `hook_event_name`, `model`, `permission_mode`, `source: "startup"`) and the
  command exited 0. Its cost is two visible warning lines per invocation
  (*"`--dangerously-bypass-hook-trust` is enabled. Enabled hooks may run without review for this invocation."*), on top of
  the permanent `-c` embedded-mode badge below. Despite the name it does not weaken the sandbox or the approval policy —
  it only skips the review of hooks Octoboard itself generated. Seeding `hooks.state` stays available as the
  warning-free alternative if those lines prove unacceptable.
  **Additional finding, worse than the modal:** with injected hooks and *no* seeded `hooks.state`, `codex exec` (headless)
  did not report anything and did not fall through — it **hung indefinitely** with no output and no hook fired, and had to
  be killed after five minutes. So untrusted hooks are not merely skipped in headless mode; they stall the process. For
  Octoboard this means the trust step is not optional polish, it is a launch prerequisite: a Codex session launched with
  hooks but without trust seeded would look exactly like a hung session.
  **Why not `CODEX_HOME`:** it is the right variable name and auth works if `auth.json` is symlinked in, but it makes Codex
  ignore the user's *entire* global setup — `~/.codex/AGENTS.md` disappears from the prompt (verified by marker), along
  with their model choice, `approval_policy`, skills, plugins and rules. That breaks the user's setup, which is the same
  objection as modifying it.
  **Two visible side effects of using `-c` at all:** any `-c` forces embedded mode, so the TUI carries a permanent
  `⚠ 1 warning` badge explaining that command-line overrides require it, and the session becomes invisible to
  `codex agents`. And hook timeouts must be `<= 3`, because `SessionEnd` and `Interrupt` are hard-clamped to 3 s and each
  clamp prints another visible warning.
- [x] A4 Codex: how to append a role description without replacing the default system prompt (write it into the initial task
  prompt)
  — **feasible, and the planned fallback is not needed.** `-c 'developer_instructions="<text>"'` prepends an extra
  *developer* message and changes nothing else: with the override, the skills block, permissions block, multi-agent role,
  `AGENTS.md` item and environment context were all byte-identical to the baseline, and a live run echoed the injected
  marker alongside the project's own markers.
  Do **not** use `-c instructions=…`, which replaces the base system prompt instead of adding to it.
  Useful for the adapter's own tests: `codex debug prompt-input [-c …] '<prompt>'` dumps the exact model-visible input
  list as JSON with no API call.
- [x] A5 Codex: an MCP server injected via `-c mcp_servers.…` takes effect in the interactive TUI (same as A3)
  — **feasible.** The syntax that works keeps argv as its own TOML array key rather than part of the command:
  `-c 'mcp_servers.octoboard.command="<abs path>"'` plus `-c 'mcp_servers.octoboard.args=["<abs path>", …]'`. Verified in
  the interactive TUI: the server booted with exactly that argv and Codex sent `initialize`,
  `notifications/initialized` and `tools/list`; verified end to end with a real tool call, exposed to the model as
  `mcp__octoboard__<tool>`. It **merges** with the user's own servers — all five of theirs were still listed with
  `octoboard` added.
- [x] A6 Grok Build: how to inject hooks and MCP, and whether it is possible without modifying project or user-global
  configuration (a separate config directory; failing that, degrade to an ordinary terminal session)
  — **feasible with conditions, via a per-session `GROK_HOME` symlink farm.** The obvious routes do not work:
  `GROK_CONFIG` / `GROK_CONFIG_PATH` do layer on top of `config.toml`, but only for allowlisted keys, so `hooks` and
  `mcp_servers` silently drop; `$GROK_HOME/managed_config.toml` and `requirements.toml` load but are *deleted* by the
  deployment sync at runtime; `grok mcp add` only writes persistently to user or project scope; `CLAUDE_CONFIG_DIR` is
  ignored. What works is pointing `GROK_HOME` at an Octoboard-owned directory in which every entry is a symlink to the real
  `~/.grok` except two: a `config.toml` copied from the user's with an `[mcp_servers.octoboard]` block appended, and a
  `hooks/` directory holding Octoboard's hooks. `auth.json` and `sessions/` stay symlinks, so login state is shared and
  Octoboard-launched sessions remain resumable from the user's own plain `grok`; `managed_config.toml` and
  `requirements.toml` must never be created there.
  Verified: `GROK_HOME=<ours> grok inspect` reports `octoboard (stdio) config` and `Hooks (13)` while Skills, Agents,
  Plugins and the permissions source stay byte-identical to the baseline, and a live run authenticated, fired the hooks and
  called the injected tool.
  **Residual cost, accepted deliberately:** the copied `config.toml` is a snapshot, so a user edit made while a session runs
  is not seen, and anything the session persists (`/settings`, `/model`) lands in Octoboard's copy and is lost to the user.
  Mitigate by re-copying at every launch.
  **`config.toml` must always be a copy, never a symlink** — an earlier revision of this item suggested leaving it a
  symlink for sessions needing hooks but no MCP server. That is unsafe: Grok persists session settings back into it, and
  `--reasoning-effort low` was observed rewriting `default_reasoning_effort` in the file. Against a symlink the write
  would either be replaced (losing the link) or followed (destroying the user's configuration); the first was not tested
  and the second must never be risked.
  **Hooks need no folder trust**, which simplifies the mechanism: hooks placed in `$GROK_HOME/hooks/*.json` load at
  `global` scope, which is always trusted, and all of them fired in a workspace that was never added to
  `trusted_folders.toml`. The trust requirement recorded under G1 applies to the *project's* own `<project>/.grok/hooks`
  and instruction files, not to Octoboard's injected ones.
  Grok also ships its full user guide on disk at `~/.grok/docs/user-guide/` (`10-hooks.md` is the authoritative hooks spec,
  `26-config-reference.md` the config key table); it is rewritten by `grok update`, so pin behaviour to the version.
- [x] A7 Grok Build: how to append a role description (write it into the initial task prompt)
  — **feasible, and better than the planned fallback**: `--rules <text>` appends to the system prompt without replacing it,
  so the role description does not have to be smuggled into the initial task. Verified with two markers — a project
  `AGENTS.md` marker and a `--rules` marker both appeared in the same reply, with the default toolchain intact.
  **Casing was settled afterwards on a case-sensitive volume, and it matters.** All three agents match instruction
  filenames by exact spelling, and **none of them accepts an all-lowercase name** — `agents.md` and `claude.md` are dead
  for all three:

  | spelling | Claude Code | Codex | Grok |
  |---|---|---|---|
  | `AGENTS.md` | ignored | read | read |
  | `Agents.md` | ignored | ignored | read |
  | `CLAUDE.md` | read | ignored | read |
  | `Claude.md` | ignored | ignored | read |
  | `CLAUDE.local.md` | read | ignored | read |
  | all-lowercase forms | ignored | ignored | ignored |

  Established without spending model turns where possible: `grok inspect --json` lists every discovered file, and
  `codex debug prompt-input` dumps the model-visible input list. Trust was ruled out on each agent by measuring both
  sides of the gate.
  **A correction that has nothing to do with casing: Claude Code 2.1.274 does not read `AGENTS.md` at all by default.**
  Its discovery list is `CLAUDE.md`, `.claude/CLAUDE.md`, `CLAUDE.local.md`; `AGENTS.md` support lives in a separate
  built-in plugin that is off for this purpose, and the binary carries its own hint saying the plugin's
  `projectInstructions` option must be set to `agents-fallback` to load it. **That option cannot be used**: the plugin is
  not registered in this build at all (`claude plugin details agents-md` reports it not found, and the plugin's own
  "this project has AGENTS.md but no CLAUDE.md" hint never fires), so neither a per-launch `--settings` injection nor a
  persistent configuration write turns it on. The key shape is accepted and simply inert; the binary gates the plugin on
  a server-side feature flag, which is inference rather than something measurable locally.
  So the hub's working-directory instruction file must be named per agent — `CLAUDE.md` for Claude Code, `AGENTS.md` for
  Codex, either for Grok. If Octoboard ever needs Claude Code to honour a project's existing `AGENTS.md`, the verified
  per-launch route is `--append-system-prompt-file <path>`, which does reach the model — but it lands in the system
  prompt rather than the instruction-file block, so Octoboard would own discovery and precedence rather than the agent.
  Two further facts for the adapter: **on a case-sensitive volume Grok loads *every* matching spelling in one directory**,
  not just the first, so the same repository contributes more instruction text there than on a case-insensitive one (its
  dedup is by inode and only fires when two spellings resolve to one file); and **`grok inspect` can report a path
  spelling that does not exist on disk** — it labels a hit with the candidate name it probed, so on a case-insensitive
  volume that path is not safe to use to locate a file.
  **The nested form was measured too, and is exact-match in both the directory name and the filename** for all three
  agents: Claude Code reads `.claude/CLAUDE.md` only (not `.Claude/`, not `.claude/Claude.md`, and there is no nested
  `CLAUDE.local.md` candidate); Grok reads `.claude/CLAUDE.md`, `.claude/CLAUDE.local.md`, `.grok/rules/*.md` and
  `.claude/rules/*.md`, all case-exact; Codex has **no** nested dot-directory form at all — `.codex/AGENTS.md` is not
  read — though it does pick up `AGENTS.md` in a subtree under the same literal name.
  Two traps: **Grok's casing is asymmetric between levels** — top-level `Claude.md` is read while nested
  `.claude/Claude.md` is not; and **Grok reads no project instructions at all in a non-git directory**, because it
  locates the project root by walking up for `.git`. The second one produces an all-negative result that looks exactly
  like a casing conclusion, so any future probe of this needs a positive control.

### Terminal and architecture

- [x] T1 All three agents' TUIs render and behave correctly in `xterm.js` inside WKWebView: full screen, mouse, keyboard
  shortcuts, CJK input methods, window resizing (targeted configuration, e.g. Grok's non-full-screen mode)
  — **feasible with conditions.** Driven by hand in the real Tauri window, with screenshots as evidence. No targeted per-agent terminal
  configuration turned out to be necessary.
  - **Full-screen rendering: Grok and Codex pass outright** — both take the alternate screen and fill the pane
    edge to edge, with their status bars pinned to the top and bottom rows. **Claude Code renders inline instead**, as a
    block at the top of the pane with empty space below, because on this machine it falls back to its *classic* renderer
    (the same fallback the PTY probe ran into).
    **The cause is established, and it is self-inflicted.** Two environment confounds were found and ruled out first —
    the prototype was handing agents the snapshot shell's `TERM=dumb`, and it was leaking its own `CLAUDE_CODE_*` session
    markers, both of which Claude Code acts on; fixing each changed the session's behaviour (colour returned, the
    "transcript saving is off" warning disappeared) but not the renderer. The actual cause is a sticky per-machine
    auto-disable: `~/.claude.json` carries
    `fullscreenAutoDisabled: {"version": "2.1.274", "at": …, "strikes": 2}`, timestamped inside the window when this
    milestone's own PTY probes were repeatedly killing sessions mid-startup. Claude Code counted two failed renderer
    starts and turned its fullscreen renderer off for this machine and version.
    So this is **not** a property of `xterm.js`, WKWebView or the prototype, and milestone 01 has nothing to decide about
    forcing a renderer. What it does inherit is the cause: **abruptly killing an agent process can trip the agent's own
    degradation counters**, so session teardown should let the agent exit cleanly where it can. Clearing that key resets
    the behaviour; it lives in the user's global configuration, which Octoboard must not write.
  - **Keyboard shortcuts: pass, with one exception.** `Shift+Tab` cycled Claude Code's permission mode (`auto mode on` →
    `manual mode on`), `Ctrl+U` cleared the input line (the footer then offered `Ctrl+Y to paste deleted text`) and
    `Ctrl+W` deleted a word — so modifier combinations reach the PTY through WKWebView. **`Ctrl+C` alone does not.**
    Sending `0x03` straight to the PTY cleared the input line, while `Ctrl+C` typed into the window did nothing, with
    `Ctrl+U` and `Ctrl+W` working in the same session — so it is being swallowed above the terminal, not ignored by the
    agent. Since `Ctrl+C` is the most-used terminal key there is, milestone 01's UI must intercept it explicitly (an
    `xterm.js` custom key-event handler) and forward `0x03` itself.
  - **CJK input method: feasible with conditions — composed *text* works, full-width punctuation does not.** A Chinese
    sentence requiring candidate conversion landed correctly in Claude Code's input line, not duplicated and not delayed.
    But a full-width punctuation mark such as `？`, which a Chinese input method emits directly without a candidate
    window, **needs the key pressed twice**; the first press is swallowed. Native macOS applications take it on the first
    press, so this is the terminal's doing.
    Three things were ruled out by bisection before concluding that: plain shifted ASCII symbols reach the PTY correctly
    when typed into the window (`?`, `!`, `:` all arrive); the same symbols sent straight to the daemon arrive intact, so
    the daemon, the PTY and the agent are not involved; and the UI installs no custom key handler, so this is
    `xterm.js` 5.5.0's composition handling inside WKWebView. Both the user's verification and this bisection were needed
    — scripted key injection bypasses the input method entirely and can prove nothing about it on its own.
    Settled with the user: **keep the framework's default composition handling rather than writing one**, and treat this
    as a separate, low-priority improvement rather than milestone 01 debt. It does not block anything — the key can be
    pressed twice, or the input method switched to English.
  - **Window resizing: pass.** Resized the window across three sizes; the agent's *own* rendering reflowed each time — its
    footer rewrapped from one line to two and back, and the prompt box's rules redrew to the new width — so the resize
    reaches the PTY rather than only the `xterm.js` grid. Typing immediately after a resize landed at the correct cursor
    position.
  - **Reconnect: pass** visually — disconnecting and reconnecting redrew the pane with no garbled characters at the seam
    (T3 has the byte-level version of this).
  - **Switching sessions: passes for display** — each chip showed its own session's content, never a mix or a stale
    screen.
  **Two prototype UI defects found while doing this** (they belong to the throwaway UI, not to the architecture, but
  milestone 01 must not reproduce them):
  1. **Input was silently dropped after switching sessions with a chip** (since fixed in the prototype). Output kept
    flowing and the pane updated live, but keystrokes never reached the PTY — proven by sending the same text straight to the daemon, where it appeared in the
    agent's input box immediately. **The cause was focus, not a stale binding**: the UI never called `term.focus()`, and
    the session chips and terminal controls are ordinary DOM elements, so clicking one moved focus off `xterm.js`'s
    hidden textarea. Output is focus-independent, which is why the pane kept updating. This also explains why clicking
    *Reconnect* did not restore input.
  2. **The socket-state label went stale** (since fixed). It read `closed` while the pane was visibly receiving live
    output, and clicking *Reconnect* flipped the label to `open` without restoring input, because nothing distinguished a
    superseded socket's events from the current one's.
  - **Mouse: pass.** Verified by the user against a long agent transcript: wheel scrolling moves the TUI's own history
    and behaves as it does in a normal terminal, with no desynchronisation between what `xterm.js` shows and what the TUI
    believes is on screen. Note this was only meaningful after the UI gained an `onBinary` handler — without it, mouse
    reports past column 95 are silently dropped, because their coordinate bytes exceed 127 and never reach `onData`.

- [x] T2 The latency of daemon → WebSocket → `xterm.js` and the throughput under heavy output (batch frames together, use
  binary frames)
  — **feasible; neither planned fallback addresses the failure that was actually found.**
  Latency, 300 samples over loopback: **median 0.07-0.08 ms, p95 0.12-0.14 ms** across repeated runs. Machine-local
  figures that vary run to run, hence ranges.
  Throughput, 1 MiB echoed through a `cat` session with an event-driven backpressure gate: roughly **490-525 MiB/s when
  the client writes in 64 KiB chunks** and **29-35 MiB/s at 4 KiB chunks**, both with zero loss and zero drops. These are
  *echo round-trip* figures — each byte crosses client → daemon → PTY → `cat` → PTY → broadcast → client.
  **Read those two numbers narrowly.** What that bench varies is the **client-to-daemon input chunk size**, and the two
  runs are not in the same regime: the backpressure gate engages in the 4 KiB run and never in the 64 KiB one, and neither
  payload exceeds the daemon's broadcast buffer, so both are bursts it absorbs whole rather than steady state. The gap is
  real as measured, but it says nothing about output framing.
  **Output framing was measured separately, and the answer is a null result.** Sweeping the daemon's PTY read buffer —
  which is what sets the size of every frame it sends — over 4, 8, 32, 64 and 256 KiB changed throughput by **nothing
  measurable**, at two very different source rates: about 11.8 MiB/s from a paced producer (spread 0.2 MiB/s across all
  five sizes) and about 102 MiB/s from an unpaced one (spread 1.3 MiB/s). So there is no reason for milestone 01 to tune
  output frame size, and the plan's "batch frames together" fallback has no measured benefit to offer.
  The honest limits of that null result, which the bench prints itself: the paced rate is set by the fixture rather than
  chosen, so on its own it could equally mean the producer never pushed hard enough; the unpaced rate is capped at about
  102 MiB/s by the kernel's own PTY buffer blocking the writer, not by anything in the daemon, which is also why it never
  reached the regime where the broadcast channel would be tested. Rates between or above those two were not probed, and
  neither variant resembles a real agent's bursty, escape-sequence-laden output.
  **This item has now been wrong in both directions, which is worth recording.** A first revision reported 13.1 and
  8.7 MiB/s and concluded frame size did not matter; those figures were dominated by the bench's own polling timer, which
  clamped a 1 ms sleep upward against a window smaller than a single 64 KiB chunk. The corrected figures then prompted the
  opposite conclusion — that frame size matters by an order of magnitude — which the method does not support either. What
  is settled is the failure mode below. Output framing is settled too, as a null result within the method limits recorded
  above — no further work is planned on it, and it is not carried by any later milestone.
  **The real finding is the opposite failure mode.** An *unthrottled* flood disconnects the client almost immediately
  (about 0.2 s in, after roughly 0.1-0.9 MiB of 20 MiB), at every frame size. The cause is that the PTY reader thread
  always runs ahead of the broadcast channel, so a bounded channel fills in a fraction of a second whenever a child
  out-produces what a client can physically drain — and on one machine an OS thread doing `memcpy` vastly outpaces any
  client. This is not a slow-client problem and it is not fixed by framing. **Decision for milestone 01: the PTY reader
  must take backpressure from the channel rather than running ahead of it**, or heavy output will keep dropping clients.
  Caveat on the numbers: they measure the daemon-to-WebSocket path only and **exclude the `xterm.js` render step**, which
  cannot be measured without the real app, so the end-to-end figure the user perceives is still unmeasured.
- [x] T3 After the frontend disconnects and reconnects, the terminal contents are restored from the daemon's cached output
  (replay raw PTY output from a ring buffer)
  — **feasible.** Verified with a free-running, independently checkable counter stream, attaching, detaching mid-stream and
  reattaching, then checking the bytes rather than eyeballing the screen.
  - Short detach, well inside the ring buffer: the replay is exact — no gap and no duplicate within the reconnect's own
    snapshot-to-live transition, and no counter value missing from the union of what the clients saw.
  - Long detach that deliberately overruns the buffer: output is permanently lost (about 119,000 lines in the run recorded
    here), which is correct by design, and there was still zero duplication on either side of the gap.
  Note the counter check is exact at *line* granularity, not byte-for-byte: corruption that left the digits intact and the
  line structure valid would pass.
  Note what the guarantee actually is, because it is narrower than "the client resumes where it left off": the ring buffer
  has **no per-client cursor**, so a short detach legitimately re-displays content the client had already seen — the same
  semantics as `tmux attach`. The real UI has to cope with both that re-display and the overrun gap.
- [x] T4 Packaging and the lifecycle of the daemon as a Tauri sidecar: **feasible**. Signing is **not verified** — see below.
  `cargo tauri build` produced a `.app` with the sidecar correctly embedded at `Contents/MacOS/obd-proto`. Launching the
  bundle started the sidecar with `--parent-pid <app pid>`, and killing the app's process made the sidecar exit via its own
  watchdog with no orphan left; the watchdog was separately confirmed to notice a vanished parent within about a second.
  The architectural constraint held in the strongest form: the prototype shell makes **zero** Tauri `invoke` calls — the
  daemon's port is parsed from its stdout and put into the window URL before the window is created, so the UI really does
  talk to the daemon over WebSocket only.
  **Tauri 2 does build on this machine's rustc 1.86**, but only with exact version pins down through `tauri-utils`,
  `tauri-runtime`, `darling`, `encoding_rs`, the `icu_*` crates, `time`, `plist` and `serde_with`; left unpinned, cargo
  resolves each to a release whose own declared `rust-version` is higher. Milestone 01 should either carry the same pins or
  raise the toolchain — and note the daemon independently had to drop `reqwest` for the same reason.
  **The Keychain question folded in from G2 is settled and passes.** An agent launched from the built bundle reached the
  user's Claude Code login normally — the session came up as `Opus 5 (1M context) · Claude Max` with no login prompt — so
  packaging does not break credential access.
  **One packaging difference the development build never shows:** the bundle raised a macOS prompt for file access the
  moment a session's project directory was on an external volume. Projects will be scattered across volumes, so
  associating one has to cope with that prompt being declined or unanswered.
  **Not verified:** Developer ID signing and notarization, for want of a Developer ID — which leaves open only whether
  Gatekeeper admits the bundle on another machine. DMG bundling no longer applies: the bundle targets were narrowed to the
  `.app`, and it now builds cleanly.

## Notes for developers

- **Key points**: the UI and the daemon interact only over the network protocol, and the prototype obeys that too; agents are
  launched from an environment snapshotted per launch via `$SHELL -l -i -c 'env -0'` and filtered of the daemon's own
  agent variables, with the agent binary spawned directly rather than inside a shell (a login-only shell was proven
  insufficient — see G2); injection must not modify project files or the user's global configuration.
- **Reference**: `docs/mvp.md` sections 4.4 and 6.
