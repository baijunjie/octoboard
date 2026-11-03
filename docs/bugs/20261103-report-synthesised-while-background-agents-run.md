> Severity: Moderate

## Symptom

A Claude Code child session whose turn ends while its background subagents are still running gets a synthesised
"stopped without reporting" report sent to its owner, although no report should be synthesised for such a turn.

## Reproduction steps

1. In a project, have a coordinator start a Claude Code child session (a worker reporting to it).
2. In the child, within one turn, start one or more named subagents in the background (the Agent tool with
   `run_in_background`, for example two review subagents), then end the turn with a short message such as "Waiting for
   the review" while they are still running, without calling `report`.
3. Look at the coordinator's conversation (`read_session_history`) and the child's record (`get_session`).
4. The coordinator has received "Report from session <child> … Status: needs_decision — This session stopped without
   reporting …" carrying the child's last message. When the subagents finish, the child takes another turn and sends
   its real report.

Seen twice on 2026-11-03 in one coordinator's run (both cases below); how often it happens for every such turn is
unknown.

## Expected vs. actual

- Expected: no report is synthesised for a Claude Code turn that ends with work still running in the background; the
  session reads as *working in background* until that work is over (`docs/product/reporting-to-owners.md`, "When a
  session does not report"; `docs/product/session-statuses.md`, "Working in background").
- Actual: a synthesised report with a guessed `needs_decision` status reaches the owner while the background
  subagents are still running, followed later by the session's own report.

## Environment

Octoboard desktop app built from local main on 2026-11-03 (before 683342aae); Claude Code 2.1.317 on the machine at
filing time (the version the sessions ran is unknown); macOS. Child sessions were workers on the Claude Code agent,
reporting to a project coordinator.

Observed cases (timestamps UTC):

- Session `d111f1d9-eeb6-4c16-97f5-c94d312f4a62`, synthesised report at 2026-11-03T00:40:22Z:
  "Status: needs_decision / This session stopped without reporting, so the text below is its last message and the
  status is Octoboard's guess, not its own. / Scope 2 passed. Waiting for scope 1." Its own `done` report followed at
  01:21:14Z.
- Session `4ed2fe65-b3bf-49a7-9145-33c860022a5c`, synthesised report at 2026-11-03T05:15:26Z, last message
  "ui-checker 已通过，等待 change-checker 第二轮结果。" ("ui-checker passed, waiting for change-checker round 2"). Its own
  `done` report followed at 05:26:31Z.

## Scope of impact

Every owner of a Claude Code session that waits on background subagents at the end of a turn: it gets a false
`needs_decision` report it may act on (instruct or archive a session that is still working), then a second, real
report for the same round. Workaround: the owner checks `background_tasks` with `get_session` before acting on a
synthesised report.

## Leads

- Verified: the synthesis is skipped only when the Claude Code `Stop` hook payload carries a non-empty
  `background_tasks` (`claude_has_background_work` in `apps/daemon/src/hooks/mod.rs`, used where the stop is judged
  reportable).
- Verified: both sessions' last messages say they were waiting for subagents, and their real reports arrived 11 and
  41 minutes later.
- Inferred: the `Stop` payload of these turns listed no background tasks even though background subagents were
  running, for example because Claude Code does not list subagents started with `run_in_background` there, or lists
  them only through `SubagentStart`, which the daemon ignores for the list inside a turn ("Inside a turn these starts
  and stops change nothing", `docs/product/session-statuses.md`). Not checked against a captured hook payload.
- Unknown: whether the sessions read as *working in background* in the UI at the time.

## Acceptance criteria

- [ ] A Claude Code child session that ends a turn while named background subagents it started are still running gets
      no synthesised report, and reads as *working in background*.
- [ ] When those subagents finish and the session's next turn ends without `report` and with nothing left running, a
      report is synthesised as for any other turn.
- [ ] A turn that ends with nothing left running is still reported on as before.
