# 11 Defer initial terminal status writes

> Goal: defer initial terminal status writes.
> Completion criteria: Initial and later terminal notices use the established mounted/deferred status mechanism without duplicate visible/spoken copy or disrupted terminal focus. All listed validation passes, and the independently reviewed result is accepted.

Status: **accepted; implementation not started**. Implementation begins only after the audit passes and the preceding required work is closed.

## Scope

The accepted finding and native verification boundary are reproduced below so this brief remains self-contained.

- **Initial writes (I-05):** `TerminalPane.tsx` currently places initial starting/loading and dormant text directly
  in `role="status"`; `ShellTerminalView.tsx` does the same for initial trouble/exited text. Later changes in a
  persistent region are not part of this finding. Use the established status mechanism so an empty region mounts
  before the deferred first write, while durable visible copy, reconnect/resume actions, one active shell-tab
  announcement, modal exposure and focus timing remain intact.
- Check `terminal-starting`, `terminal-reconnecting`, `terminal-disconnected`, `shell-panel-reconnecting`,
  `shell-panel-disconnected` and `shell-panel-exited`, separating initial mount from later changes. Match an
  existing passing VoiceOver case item by item; where none matches, complete fixture-backed packaged listening.
  Chromium or Playwright WebKit snapshots do not establish spoken output. No real daemon/account mutation is needed.

## Implementation contract

- [ ] Replace raw initial status text paths for agent cover/dormant and shell trouble/exited with the established announcement mechanism.
- [ ] Preserve durable visible copy, reconnect/resume actions, one active shell-tab announcement, modal exposure and attended/focus timing where applicable.
- [ ] Match recorded VoiceOver cases item by item; if no passing case matches, obtain the required fixture-backed packaged listening result rather than claiming Chromium proves speech.

## Validation requirements

- [ ] Use terminal-starting, terminal-reconnecting, terminal-disconnected, shell-panel-reconnecting, shell-panel-disconnected and shell-panel-exited, separating mount-time from later changes. Check initial empty region/deferred write, focus timing, modal and tab visibility, no duplicate announcement, F6 escape and reconnect/resume. Run status/terminal tests, gallery exposure checks, applicable native listening and static checks.
- [ ] Complete the project gallery/static/change-check/documentation gates; preserve failed or unresolved checks explicitly. Do not skip scenarios or weaken assertions.

## Dependencies and handoff

None beyond the established StatusAnnouncer contract. No real daemon mutation or real account/login is required; use fixtures.

## Test invocation

Use `pnpm --filter @octoboard/ui exec vitest run <explicit paths>` for focused UI tests. Passing a path after
`pnpm ... test --` did not constrain the audit's run and launched the wider suite. Record the actual command,
selected files and outcome; do not infer a whole-suite pass from a focused check.

## Notes for the developer

Reuse the existing shared components and mechanisms named in the linked finding record. Follow `docs/ui-accessibility.md`, `packages/ui/README.md` “The UI state gallery”, `docs/voiceover-cases.md`, and project gallery/change-check workflows. Only gallery fixtures may stage data. Read the exact baseline evidence and recheck caller inventory after rebasing; other owners’ pending changes are not certified by this audit.
