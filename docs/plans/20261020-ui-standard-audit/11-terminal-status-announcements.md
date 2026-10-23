# 11 Defer initial terminal status writes

> Goal: defer initial terminal status writes.
> Completion criteria: Initial and later terminal notices use the established mounted/deferred status mechanism without duplicate visible/spoken copy or disrupted terminal focus. All listed validation passes, and the independently reviewed result is accepted.

Status: **ready; not started**.

## Scope

The accepted finding, re-inventoried against main at `19f58fc79`, is reproduced below so this brief remains
self-contained. The original finding records are in Git: `git show 7d7fdb2da^:docs/plans/20261020-ui-standard-audit/{audit-states,audit-interactions,audit-gallery}.md`.

- **Initial writes (I-05):** raw text sits directly in `role="status"` at `terminal/TerminalPane.tsx:528-530` (cover
  status), at the dormant bar `TerminalPane.tsx:565-575`, and in `ShellTerminalView.tsx:267-269` (trouble) and `270-279`
  (exited). The dormant bar now also has an archived variant (`fa2aa8960`) and is hidden while an account switch is in
  progress (`a6eb3d8de`). Later changes in a persistent region are not part of this finding. Use the established status
  mechanism so an empty region mounts before the deferred first write, while durable visible copy, reconnect/resume
  actions, one active shell-tab announcement, modal exposure and focus timing remain intact.
- **New callers:** "Switching account…" (`TerminalPane.tsx:457-458`, scenario `terminal-switching-account`) and
  "Resuming session…" (`TerminalPane.tsx:461`). An archived dormant bar and `SessionHistoryView`'s `StatusAnnouncer`
  may compete; include them in the no-duplicate-announcement check.
- Check `terminal-starting`, `terminal-reconnecting`, `terminal-disconnected`, `terminal-switching-account`, a dormant
  and an archived scenario, `shell-panel-reconnecting`, `shell-panel-disconnected` and `shell-panel-exited`, separating
  initial mount from later changes. No real daemon/account mutation is needed.
- **Acceptance (user decision):** close on candidate VoiceOver matches (PLAIN-2 for the persistent visible status with
  an action, SA-H for a first mount with no focus move, both in `docs/voiceover-cases.md`) plus automated and browser
  checks. The ShellTerminalView exited fill is expected to match PLAIN-2 (it later fills in place). Writes that match no
  case are recorded as open items in `docs/voiceover-cases.md` during this milestone's implementation; fixture-backed
  packaged listening is not required. Chromium and Playwright WebKit snapshots do not establish spoken output.

## Implementation contract

- [ ] Replace raw initial status text paths for agent cover/dormant (including the archived variant) and shell trouble/exited with the established announcement mechanism.
- [ ] Preserve durable visible copy, reconnect/resume actions, one active shell-tab announcement, modal exposure and attended/focus timing where applicable.
- [ ] Match each write to a recorded VoiceOver case (PLAIN-2 / SA-H) as a candidate, and record the unmatched writes as open items in `docs/voiceover-cases.md`.

## Validation requirements

- [ ] Use the scenarios listed in Scope, separating mount-time from later changes. Check initial empty region/deferred write, focus timing, modal and tab visibility, no duplicate announcement (including archived dormant bar against the history `StatusAnnouncer`), F6 escape and reconnect/resume. Run status/terminal tests, gallery exposure checks and static checks.
- [ ] Complete the project gallery/static/change-check/documentation gates; preserve failed or unresolved checks explicitly. Do not skip scenarios or weaken assertions.

## Dependencies and handoff

None beyond the established StatusAnnouncer contract. No real daemon mutation or real account/login is required; use fixtures. 09 also edits `TerminalPane.tsx`; merge the two one at a time.

## Test invocation

Use `pnpm --filter @octoboard/ui exec vitest run <explicit paths>` for focused UI tests. Passing a path after
`pnpm ... test --` did not constrain the audit's run and launched the wider suite. Record the actual command,
selected files and outcome; do not infer a whole-suite pass from a focused check.

## Notes for the developer

Reuse the existing shared components and mechanisms named in the finding records. Follow `docs/ui-accessibility.md`, `packages/ui/README.md` “The UI state gallery”, `docs/voiceover-cases.md`, and project gallery/change-check workflows. Only gallery fixtures may stage data. Read the exact baseline evidence and recheck caller inventory after rebasing; other owners’ pending changes are not certified by this audit.
