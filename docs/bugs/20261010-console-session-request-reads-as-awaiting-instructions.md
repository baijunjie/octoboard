> Severity: Moderate

# A session waiting for its console session request reads as awaiting instructions

## Symptom

While a Claude Code session's `request_console_session` call waits for the user's answer, the session reads first as
*working* and then as *awaiting instructions*, never as *waiting for the user*.

## Reproduction steps

1. Open an unbound Claude Code project session by hand in a console that holds at least two projects.
2. Ask it to call `request_console_session` (for example: "This needs work in another project; call
   request_console_session").
3. When the confirmation dialog appears, do not answer it.
4. Watch the session's status: it reads *working*. About 120 s after the call, Claude Code moves the call to the
   background and ends its turn; about 60 s after that, the session switches to *awaiting instructions*, while the
   dialog is still up and the call is still waiting.

Occurs on every such request left unanswered for about three minutes.

## Expected vs. actual

- Expected: for the whole wait the session reads *waiting for the user* (the raised hand) — the position the user
  confirmed on the spot (2026-10-10), in line with `docs/product/sessions.md`'s status table, where *waiting for the
  user* means the agent is waiting on the user's decision or has asked the user a question.
- Actual: *working* until Claude Code's `idle_prompt` notification, then *awaiting instructions*; no raised hand at any
  point.

## Environment

- Branch `feat/lead-sessions` at `f0917b9` (the `request_console_session` tool is not on `main` before that branch is
  merged), daemon run standalone, macOS.
- Claude Code 2.1.295. Codex and Grok Build: Unknown (not tried; both accounts were at their usage limits).

## Scope of impact

Anyone approving a console session request. The dialog itself is shown, so the request is not lost, but the rail's
waiting count and the session's row do not show that the session is waiting on the user, and after about three minutes
the row says the session is ready for instructions. No workaround beyond noticing the dialog.

## Leads

- Verified (hook log of the run): the call's `PreToolUse` at 11:23:18; `PostToolUse` and `PostToolBatch` at 11:25:18
  when Claude Code moved it to the background; `Stop` at 11:25:22 with `background_tasks` holding the running MCP task
  (the daemon keeps *working* for that); `Notification` with `notification_type: "idle_prompt"` at 11:26:22, after
  which the status read *awaiting instructions* (11:26:22.099) while the call was still pending.
- Verified: no hook event or daemon status change marks the session as *waiting for the user* at any point of the wait.
- Inferred: the daemon knows a request is pending for that session (it holds it in memory and broadcasts
  `console_session_request`), so the information needed is on the daemon's side rather than the agent's.

## Acceptance criteria

- [ ] From the moment a session's `request_console_session` call starts waiting until the request is answered, times
  out or is withdrawn, the session reads *waiting for the user*, with its raised hand counted like any other.
- [ ] Claude Code backgrounding the call, its `Stop` while the call is pending, and its `idle_prompt` notification do
  not move the session out of *waiting for the user* during that time.
- [ ] Once the request ends, the session's status follows its agent's events again as before.
